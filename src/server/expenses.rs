use std::net::SocketAddr;

use axum::{
    Json, Router,
    body::Body,
    extract::{ConnectInfo, DefaultBodyLimit, Multipart, Path, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use chrono::{Days, NaiveDate, Utc};
use serde_json::json;
use sqlx::{FromRow, SqlitePool};

use super::{
    audit, autocrop,
    auth::{AppError, CurrentUser},
    fx,
    security::client_ip,
    state::AppState,
};
use crate::{
    i18n::{parse_amount, t},
    model::{
        AttachmentRef, CURRENCIES, Category, ExpenseDetail, ExpenseKind, ExpenseListItem,
        ExpenseStatus, SessionUser, UploadResponse,
    },
};

const MAX_FILE_BYTES: usize = 20 * 1024 * 1024;
const MAX_FILES: usize = 10;
const MAX_REQUEST_BYTES: usize = 64 * 1024 * 1024;
const ALLOWED_MIME: &[&str] = &[
    "image/jpeg",
    "image/png",
    "image/webp",
    "image/heic",
    "image/heif",
    "application/pdf",
];

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/bilag/upload",
            post(upload).layer(DefaultBodyLimit::max(MAX_REQUEST_BYTES)),
        )
        .route("/filer/{id}", get(serve_file))
        .route("/filer/{id}/original", get(serve_original))
}

/// A user-facing validation error; the message is shown as-is.
#[derive(Debug)]
pub struct UserError(pub &'static str);

pub enum ExpenseError {
    User(&'static str),
    Internal(anyhow::Error),
}

impl<E: Into<anyhow::Error>> From<E> for ExpenseError {
    fn from(e: E) -> Self {
        ExpenseError::Internal(e.into())
    }
}

impl From<UserError> for ExpenseError {
    fn from(e: UserError) -> Self {
        ExpenseError::User(e.0)
    }
}

fn bad_request(msg: &'static str) -> Response {
    (StatusCode::BAD_REQUEST, msg).into_response()
}

struct IncomingFile {
    bytes: Vec<u8>,
    mime: &'static str,
    name: Option<String>,
    /// The upload as received, when `bytes` holds an auto-cropped version.
    original: Option<Vec<u8>>,
}

async fn crop_images(files: Vec<IncomingFile>) -> anyhow::Result<Vec<IncomingFile>> {
    Ok(tokio::task::spawn_blocking(move || {
        files
            .into_iter()
            .map(|mut f| {
                if matches!(f.mime, "image/jpeg" | "image/png" | "image/webp")
                    && let Some(cropped) = autocrop::auto_crop(&f.bytes, f.mime)
                {
                    f.original = Some(std::mem::replace(&mut f.bytes, cropped));
                    f.mime = "image/jpeg";
                }
                f
            })
            .collect()
    })
    .await?)
}

async fn upload(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Result<Response, AppError> {
    let ip = client_ip(&headers, Some(peer), state.config.trust_proxy);
    let mut files = Vec::new();
    let mut expense_id: Option<i64> = None;
    let mut autocrop = false;

    loop {
        let field = match multipart.next_field().await {
            Ok(Some(f)) => f,
            Ok(None) => break,
            Err(e) if e.status() == StatusCode::PAYLOAD_TOO_LARGE => {
                return Ok(bad_request(t::ERR_FILE_TOO_LARGE));
            }
            Err(_) => return Ok(bad_request(t::UPLOAD_FAILED)),
        };
        match field.name() {
            Some("expense_id") => {
                let text = field.text().await.unwrap_or_default();
                expense_id = Some(match text.trim().parse() {
                    Ok(id) => id,
                    Err(_) => return Ok(bad_request(t::UPLOAD_FAILED)),
                });
            }
            Some("autocrop") => {
                autocrop = field.text().await.is_ok_and(|v| v.trim() == "1");
            }
            Some("files") => {
                if files.len() >= MAX_FILES {
                    return Ok(bad_request(t::ERR_TOO_MANY_FILES));
                }
                let name = field
                    .file_name()
                    .map(|n| n.chars().take(200).collect::<String>());
                let Ok(bytes) = field.bytes().await else {
                    return Ok(bad_request(t::ERR_FILE_TOO_LARGE));
                };
                if bytes.len() > MAX_FILE_BYTES {
                    return Ok(bad_request(t::ERR_FILE_TOO_LARGE));
                }
                // The declared content type is ignored; only the file's magic bytes count.
                let Some(mime) = infer::get(&bytes)
                    .map(|k| k.mime_type())
                    .and_then(|m| ALLOWED_MIME.iter().copied().find(|a| *a == m))
                else {
                    return Ok(bad_request(t::ERR_FILE_TYPE));
                };
                files.push(IncomingFile {
                    bytes: bytes.to_vec(),
                    mime,
                    name,
                    original: None,
                });
            }
            _ => {}
        }
    }
    if files.is_empty() {
        return Ok(bad_request(t::ERR_NO_FILES));
    }
    if autocrop {
        files = crop_images(files).await?;
    }

    match store_upload(&state, &user, files, expense_id, ip.as_deref()).await {
        Ok(id) => Ok(Json(UploadResponse { expense_id: id }).into_response()),
        Err(ExpenseError::User(msg)) => Ok(bad_request(msg)),
        Err(ExpenseError::Internal(e)) => Err(AppError::from(e)),
    }
}

async fn store_upload(
    state: &AppState,
    user: &SessionUser,
    files: Vec<IncomingFile>,
    expense_id: Option<i64>,
    ip: Option<&str>,
) -> Result<i64, ExpenseError> {
    if let Some(id) = expense_id {
        let status = owned_status(&state.pool, user.id, id)
            .await?
            .ok_or(UserError(t::VOUCHER_NOT_FOUND))?;
        if !status.is_editable() {
            return Err(UserError(t::LOCKED_USED).into());
        }
    }

    let mut stored = Vec::with_capacity(files.len());
    for f in &files {
        let original = match &f.original {
            Some(o) => Some(state.files.put(o).await?),
            None => None,
        };
        stored.push((state.files.put(&f.bytes).await?, original));
    }

    let mut suggestion = None;
    if expense_id.is_none() {
        let first = &files[0];
        match state.ocr.extract(first.mime, &first.bytes).await {
            Ok(s) => suggestion = s,
            Err(e) => tracing::warn!("OCR failed: {e:#}"),
        }
    }

    let mut tx = state.pool.begin().await?;
    let id = match expense_id {
        Some(id) => id,
        None => {
            let s = suggestion.unwrap_or_default();
            let id: i64 = sqlx::query_scalar(
                "INSERT INTO expenses (owner_id, vendor, expense_date, amount_minor, currency)
                 VALUES (?, ?, ?, ?, ?) RETURNING id",
            )
            .bind(user.id)
            .bind(s.vendor)
            .bind(s.expense_date)
            .bind(s.amount_minor)
            .bind(s.currency)
            .fetch_one(&mut *tx)
            .await?;
            audit::record(
                &mut *tx,
                Some(user.id),
                "expense_created",
                Some(("expense", &id.to_string())),
                None,
                ip,
            )
            .await?;
            id
        }
    };

    let mut order: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(page_order), -1) + 1 FROM attachments WHERE expense_id = ?",
    )
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;
    for (f, (sha, original_sha)) in files.iter().zip(&stored) {
        let att_id: i64 = sqlx::query_scalar(
            "INSERT INTO attachments (expense_id, sha256, mime, size_bytes, original_name, page_order, original_sha256)
             VALUES (?, ?, ?, ?, ?, ?, ?) RETURNING id",
        )
        .bind(id)
        .bind(sha)
        .bind(f.mime)
        .bind(f.bytes.len() as i64)
        .bind(&f.name)
        .bind(order)
        .bind(original_sha)
        .fetch_one(&mut *tx)
        .await?;
        order += 1;
        audit::record(
            &mut *tx,
            Some(user.id),
            "attachment_added",
            Some(("expense", &id.to_string())),
            Some(json!({
                "attachment_id": att_id,
                "sha256": sha,
                "mime": f.mime,
                "size": f.bytes.len(),
                "auto_cropped": original_sha.is_some(),
                "original_sha256": original_sha,
            })),
            ip,
        )
        .await?;
    }
    sqlx::query(
        "UPDATE expenses SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = ?",
    )
    .bind(id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(id)
}

async fn serve_file(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<i64>,
) -> Result<Response, AppError> {
    serve_attachment(&state, &user, id, false).await
}

async fn serve_original(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<i64>,
) -> Result<Response, AppError> {
    serve_attachment(&state, &user, id, true).await
}

async fn serve_attachment(
    state: &AppState,
    user: &SessionUser,
    id: i64,
    original: bool,
) -> Result<Response, AppError> {
    // Admins may view everyone's files (read-only god view).
    let row: Option<(String, String, Option<String>)> = sqlx::query_as(
        "SELECT a.sha256, a.mime, a.original_sha256 FROM attachments a JOIN expenses e ON e.id = a.expense_id
         WHERE a.id = ? AND (e.owner_id = ? OR ?)",
    )
    .bind(id)
    .bind(user.id)
    .bind(user.is_admin())
    .fetch_optional(&state.pool)
    .await?;
    let (sha, mime) = match (row, original) {
        (Some((sha, mime, _)), false) => (sha, mime),
        (Some((_, _, Some(orig))), true) => (orig, String::new()),
        _ => return Ok(StatusCode::NOT_FOUND.into_response()),
    };
    let bytes = state.files.get(&sha).await?;
    let mime = if mime.is_empty() {
        infer::get(&bytes).map_or("application/octet-stream", |k| k.mime_type()).to_string()
    } else {
        mime
    };
    let mut res = Response::new(Body::from(bytes));
    let h = res.headers_mut();
    h.insert(header::CONTENT_TYPE, HeaderValue::from_str(&mime)?);
    h.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    h.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_static("inline"),
    );
    h.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, max-age=86400, immutable"),
    );
    Ok(res)
}

pub async fn owned_status(
    pool: &SqlitePool,
    owner_id: i64,
    id: i64,
) -> sqlx::Result<Option<ExpenseStatus>> {
    let status: Option<String> = sqlx::query_scalar(
        "SELECT status FROM expenses WHERE id = ? AND owner_id = ? AND deleted_at IS NULL",
    )
    .bind(id)
    .bind(owner_id)
    .fetch_optional(pool)
    .await?;
    Ok(status.and_then(|s| ExpenseStatus::parse(&s)))
}

pub async fn categories(pool: &SqlitePool) -> sqlx::Result<Vec<Category>> {
    let rows: Vec<(i64, String)> = sqlx::query_as(
        "SELECT id, name_da FROM expense_categories WHERE active = 1 ORDER BY sort_order, name_da",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(id, name)| Category { id, name })
        .collect())
}

#[derive(FromRow)]
struct ListRow {
    id: i64,
    owner_name: String,
    deleted: bool,
    kind: String,
    status: String,
    vendor: Option<String>,
    description: Option<String>,
    category: Option<String>,
    expense_date: Option<NaiveDate>,
    amount_minor: Option<i64>,
    currency: Option<String>,
    amount_base_minor: Option<i64>,
    thumb_id: Option<i64>,
    thumb_mime: Option<String>,
    attachment_count: i64,
}

pub async fn list(
    pool: &SqlitePool,
    owner_id: i64,
    status: Option<ExpenseStatus>,
    category_id: Option<i64>,
) -> sqlx::Result<Vec<ExpenseListItem>> {
    list_scoped(pool, Some(owner_id), false, status, category_id).await
}

/// Admin view across all users, including deleted expenses.
pub async fn list_all(
    pool: &SqlitePool,
    owner_filter: Option<i64>,
    status: Option<ExpenseStatus>,
) -> sqlx::Result<Vec<ExpenseListItem>> {
    list_scoped(pool, owner_filter, true, status, None).await
}

async fn list_scoped(
    pool: &SqlitePool,
    owner_id: Option<i64>,
    include_deleted: bool,
    status: Option<ExpenseStatus>,
    category_id: Option<i64>,
) -> sqlx::Result<Vec<ExpenseListItem>> {
    let rows: Vec<ListRow> = sqlx::query_as(
        "SELECT e.id, u.display_name AS owner_name, e.deleted_at IS NOT NULL AS deleted,
                e.kind, e.status, e.vendor, e.description, c.name_da AS category,
                e.expense_date, e.amount_minor, e.currency, e.amount_base_minor,
                (SELECT a.id FROM attachments a WHERE a.expense_id = e.id ORDER BY a.page_order, a.id LIMIT 1) AS thumb_id,
                (SELECT a.mime FROM attachments a WHERE a.expense_id = e.id ORDER BY a.page_order, a.id LIMIT 1) AS thumb_mime,
                (SELECT COUNT(*) FROM attachments a WHERE a.expense_id = e.id) AS attachment_count
         FROM expenses e
         JOIN users u ON u.id = e.owner_id
         LEFT JOIN expense_categories c ON c.id = e.category_id
         WHERE (?1 IS NULL OR e.owner_id = ?1)
           AND (?4 OR e.deleted_at IS NULL)
           AND (?2 IS NULL OR e.status = ?2)
           AND (?3 IS NULL OR e.category_id = ?3)
         ORDER BY COALESCE(e.expense_date, substr(e.created_at, 1, 10)) DESC, e.id DESC
         LIMIT 500",
    )
    .bind(owner_id)
    .bind(status.map(|s| s.as_str()))
    .bind(category_id)
    .bind(include_deleted)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| ExpenseListItem {
            id: r.id,
            owner_name: r.owner_name,
            deleted: r.deleted,
            kind: ExpenseKind::parse(&r.kind).unwrap_or(ExpenseKind::Receipt),
            status: ExpenseStatus::parse(&r.status).unwrap_or(ExpenseStatus::Draft),
            vendor: r.vendor,
            description: r.description,
            category: r.category,
            expense_date: r.expense_date,
            amount_minor: r.amount_minor,
            currency: r.currency,
            amount_base_minor: r.amount_base_minor,
            thumbnail: r
                .thumb_id
                .zip(r.thumb_mime)
                .map(|(id, mime)| AttachmentRef {
                    id,
                    mime,
                    original_name: None,
                    cropped: false,
                }),
            attachment_count: r.attachment_count,
        })
        .collect())
}

#[derive(FromRow)]
struct DetailRow {
    id: i64,
    owner_id: i64,
    owner_name: String,
    deleted: bool,
    kind: String,
    status: String,
    category_id: Option<i64>,
    vendor: Option<String>,
    description: Option<String>,
    expense_date: Option<NaiveDate>,
    amount_minor: Option<i64>,
    currency: Option<String>,
    fx_rate: Option<String>,
    fx_rate_date: Option<NaiveDate>,
    amount_base_minor: Option<i64>,
}

pub async fn detail(
    pool: &SqlitePool,
    owner_id: i64,
    id: i64,
) -> sqlx::Result<Option<ExpenseDetail>> {
    detail_scoped(pool, Some(owner_id), id).await
}

/// Admin view of any expense, including deleted ones.
pub async fn detail_any(pool: &SqlitePool, id: i64) -> sqlx::Result<Option<ExpenseDetail>> {
    detail_scoped(pool, None, id).await
}

async fn detail_scoped(
    pool: &SqlitePool,
    owner_id: Option<i64>,
    id: i64,
) -> sqlx::Result<Option<ExpenseDetail>> {
    let Some(r): Option<DetailRow> = sqlx::query_as(
        "SELECT e.id, e.owner_id, u.display_name AS owner_name, e.deleted_at IS NOT NULL AS deleted,
                e.kind, e.status, e.category_id, e.vendor, e.description, e.expense_date, e.amount_minor,
                e.currency, e.fx_rate, e.fx_rate_date, e.amount_base_minor
         FROM expenses e JOIN users u ON u.id = e.owner_id
         WHERE e.id = ?1 AND (?2 IS NULL OR (e.owner_id = ?2 AND e.deleted_at IS NULL))",
    )
    .bind(id)
    .bind(owner_id)
    .fetch_optional(pool)
    .await?
    else {
        return Ok(None);
    };

    let attachments: Vec<(i64, String, Option<String>, bool)> = sqlx::query_as(
        "SELECT id, mime, original_name, original_sha256 IS NOT NULL FROM attachments
         WHERE expense_id = ? ORDER BY page_order, id",
    )
    .bind(id)
    .fetch_all(pool)
    .await?;

    // Compare uploads as received, so a cropped and an uncropped copy of one photo still match.
    let duplicates_of: Vec<i64> = sqlx::query_scalar(
        "SELECT DISTINCT e2.id FROM attachments a1
         JOIN attachments a2
           ON COALESCE(a2.original_sha256, a2.sha256) = COALESCE(a1.original_sha256, a1.sha256)
          AND a2.expense_id <> a1.expense_id
         JOIN expenses e2 ON e2.id = a2.expense_id
         WHERE a1.expense_id = ? AND e2.owner_id = ? AND e2.deleted_at IS NULL
         ORDER BY e2.id",
    )
    .bind(id)
    .bind(r.owner_id)
    .fetch_all(pool)
    .await?;

    Ok(Some(ExpenseDetail {
        id: r.id,
        owner_name: r.owner_name,
        deleted: r.deleted,
        kind: ExpenseKind::parse(&r.kind).unwrap_or(ExpenseKind::Receipt),
        status: ExpenseStatus::parse(&r.status).unwrap_or(ExpenseStatus::Draft),
        category_id: r.category_id,
        vendor: r.vendor,
        description: r.description,
        expense_date: r.expense_date,
        amount_minor: r.amount_minor,
        currency: r.currency,
        fx_rate: r.fx_rate,
        fx_rate_date: r.fx_rate_date,
        amount_base_minor: r.amount_base_minor,
        attachments: attachments
            .into_iter()
            .map(|(id, mime, original_name, cropped)| AttachmentRef {
                id,
                mime,
                original_name,
                cropped,
            })
            .collect(),
        duplicates_of,
    }))
}

/// Raw form values from the edit page.
pub struct ExpenseForm {
    pub kind: String,
    pub category_id: String,
    pub vendor: String,
    pub description: String,
    pub expense_date: String,
    pub amount: String,
    pub currency: String,
}

struct ValidExpense {
    kind: ExpenseKind,
    category_id: Option<i64>,
    vendor: Option<String>,
    description: Option<String>,
    expense_date: Option<NaiveDate>,
    amount_minor: Option<i64>,
    currency: String,
}

fn optional_text(s: &str, max_chars: usize) -> Result<Option<String>, UserError> {
    let s = s.trim();
    if s.chars().count() > max_chars {
        return Err(UserError(t::ERR_TOO_LONG));
    }
    Ok((!s.is_empty()).then(|| s.to_string()))
}

fn validate(form: &ExpenseForm, today: NaiveDate) -> Result<ValidExpense, UserError> {
    let kind = ExpenseKind::parse(&form.kind).ok_or(UserError(t::ERR_INVALID_KIND))?;
    let category_id = match form.category_id.trim() {
        "" => None,
        s => Some(s.parse().map_err(|_| UserError(t::ERR_INVALID_CATEGORY))?),
    };
    let currency = form.currency.trim().to_uppercase();
    if !CURRENCIES.contains(&currency.as_str()) {
        return Err(UserError(t::ERR_INVALID_CURRENCY));
    }
    let expense_date = match form.expense_date.trim() {
        "" => None,
        s => {
            let d = NaiveDate::parse_from_str(s, "%Y-%m-%d")
                .map_err(|_| UserError(t::ERR_INVALID_DATE))?;
            // One day of slack for users ahead of the server's UTC date.
            if d > today + Days::new(1) {
                return Err(UserError(t::ERR_FUTURE_DATE));
            }
            if d < NaiveDate::from_ymd_opt(2000, 1, 1).expect("valid date") {
                return Err(UserError(t::ERR_INVALID_DATE));
            }
            Some(d)
        }
    };
    let amount_minor = match form.amount.trim() {
        "" => None,
        s => Some(
            parse_amount(s, &currency)
                .filter(|a| *a > 0)
                .ok_or(UserError(t::ERR_INVALID_AMOUNT))?,
        ),
    };
    Ok(ValidExpense {
        kind,
        category_id,
        vendor: optional_text(&form.vendor, 200)?,
        description: optional_text(&form.description, 1000)?,
        expense_date,
        amount_minor,
        currency,
    })
}

/// `draft` and `new` follow completeness; `invalid`/`duplicate` are only left explicitly.
fn status_after_edit(current: ExpenseStatus, complete: bool) -> ExpenseStatus {
    match current {
        ExpenseStatus::Draft | ExpenseStatus::New if complete => ExpenseStatus::New,
        ExpenseStatus::Draft | ExpenseStatus::New => ExpenseStatus::Draft,
        other => other,
    }
}

pub async fn update(
    pool: &SqlitePool,
    user: &SessionUser,
    id: i64,
    form: ExpenseForm,
    ip: Option<&str>,
) -> Result<(), ExpenseError> {
    let current = owned_status(pool, user.id, id)
        .await?
        .ok_or(UserError(t::VOUCHER_NOT_FOUND))?;
    if !current.is_editable() {
        return Err(UserError(t::LOCKED_USED).into());
    }
    let v = validate(&form, Utc::now().date_naive())?;

    if let Some(cat) = v.category_id {
        let ok: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM expense_categories WHERE id = ? AND active = 1)",
        )
        .bind(cat)
        .fetch_one(pool)
        .await?;
        if !ok {
            return Err(UserError(t::ERR_INVALID_CATEGORY).into());
        }
    }

    let fx = match (v.amount_minor, v.expense_date) {
        (Some(amount), Some(date)) => Some(
            fx::to_base(pool, amount, &v.currency, date)
                .await?
                .ok_or(UserError(t::ERR_FX_MISSING))?,
        ),
        _ => None,
    };
    let complete = v.category_id.is_some() && fx.is_some();
    let status = status_after_edit(current, complete);

    let mut tx = pool.begin().await?;
    sqlx::query(
        "UPDATE expenses SET kind = ?, category_id = ?, vendor = ?, description = ?, expense_date = ?,
                amount_minor = ?, currency = ?, fx_rate = ?, fx_rate_date = ?, amount_base_minor = ?,
                status = ?, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
         WHERE id = ? AND owner_id = ?",
    )
    .bind(v.kind.as_str())
    .bind(v.category_id)
    .bind(&v.vendor)
    .bind(&v.description)
    .bind(v.expense_date)
    .bind(v.amount_minor)
    .bind(&v.currency)
    .bind(fx.as_ref().map(|f| &f.rate))
    .bind(fx.as_ref().map(|f| f.rate_date))
    .bind(fx.as_ref().map(|f| f.amount_base_minor))
    .bind(status.as_str())
    .bind(id)
    .bind(user.id)
    .execute(&mut *tx)
    .await?;
    audit::record(
        &mut *tx,
        Some(user.id),
        "expense_updated",
        Some(("expense", &id.to_string())),
        Some(json!({
            "kind": v.kind.as_str(),
            "category_id": v.category_id,
            "vendor": v.vendor,
            "description": v.description,
            "expense_date": v.expense_date,
            "amount_minor": v.amount_minor,
            "currency": v.currency,
            "fx_rate": fx.as_ref().map(|f| &f.rate),
            "amount_base_minor": fx.as_ref().map(|f| f.amount_base_minor),
            "status": status.as_str(),
        })),
        ip,
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

pub async fn change_status(
    pool: &SqlitePool,
    user: &SessionUser,
    id: i64,
    requested: ExpenseStatus,
    ip: Option<&str>,
) -> Result<(), ExpenseError> {
    let current = owned_status(pool, user.id, id)
        .await?
        .ok_or(UserError(t::VOUCHER_NOT_FOUND))?;
    if !current.user_can_change_to(requested) {
        return Err(UserError(t::ERR_STATUS_CHANGE).into());
    }
    let new_status = match requested {
        // Restoring lands in draft or new depending on whether the details are complete.
        ExpenseStatus::Draft | ExpenseStatus::New => {
            let complete: bool = sqlx::query_scalar(
                "SELECT category_id IS NOT NULL AND amount_base_minor IS NOT NULL FROM expenses WHERE id = ?",
            )
            .bind(id)
            .fetch_one(pool)
            .await?;
            if complete {
                ExpenseStatus::New
            } else {
                ExpenseStatus::Draft
            }
        }
        other => other,
    };

    let mut tx = pool.begin().await?;
    sqlx::query(
        "UPDATE expenses SET status = ?, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
         WHERE id = ? AND owner_id = ? AND status = ?",
    )
    .bind(new_status.as_str())
    .bind(id)
    .bind(user.id)
    .bind(current.as_str())
    .execute(&mut *tx)
    .await?;
    audit::record(
        &mut *tx,
        Some(user.id),
        "expense_status_changed",
        Some(("expense", &id.to_string())),
        Some(json!({"from": current.as_str(), "to": new_status.as_str()})),
        ip,
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

pub async fn delete(
    pool: &SqlitePool,
    user: &SessionUser,
    id: i64,
    ip: Option<&str>,
) -> Result<(), ExpenseError> {
    let current = owned_status(pool, user.id, id)
        .await?
        .ok_or(UserError(t::VOUCHER_NOT_FOUND))?;
    if !current.is_editable() {
        return Err(UserError(t::LOCKED_USED).into());
    }
    let mut tx = pool.begin().await?;
    sqlx::query(
        "UPDATE expenses SET deleted_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
         WHERE id = ? AND owner_id = ? AND status <> 'used'",
    )
    .bind(id)
    .bind(user.id)
    .execute(&mut *tx)
    .await?;
    audit::record(
        &mut *tx,
        Some(user.id),
        "expense_deleted",
        Some(("expense", &id.to_string())),
        None,
        ip,
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn form(date: &str, amount: &str, currency: &str) -> ExpenseForm {
        ExpenseForm {
            kind: "receipt".into(),
            category_id: "".into(),
            vendor: "  Café  ".into(),
            description: "".into(),
            expense_date: date.into(),
            amount: amount.into(),
            currency: currency.into(),
        }
    }

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 9, 30).unwrap()
    }

    #[test]
    fn validates_form() {
        let v = validate(&form("2026-09-29", "1.234,50", "dkk"), today()).unwrap();
        assert_eq!(v.amount_minor, Some(123450));
        assert_eq!(v.currency, "DKK");
        assert_eq!(v.vendor.as_deref(), Some("Café"));
        assert_eq!(v.description, None);

        let v = validate(&form("", "", "EUR"), today()).unwrap();
        assert_eq!((v.amount_minor, v.expense_date), (None, None));
    }

    #[test]
    fn rejects_bad_form() {
        assert_eq!(
            validate(&form("2026-10-05", "", "DKK"), today())
                .err()
                .unwrap()
                .0,
            t::ERR_FUTURE_DATE
        );
        assert_eq!(
            validate(&form("30.09.2026", "", "DKK"), today())
                .err()
                .unwrap()
                .0,
            t::ERR_INVALID_DATE
        );
        assert_eq!(
            validate(&form("", "abc", "DKK"), today()).err().unwrap().0,
            t::ERR_INVALID_AMOUNT
        );
        assert_eq!(
            validate(&form("", "0", "DKK"), today()).err().unwrap().0,
            t::ERR_INVALID_AMOUNT
        );
        assert_eq!(
            validate(&form("", "", "XXX"), today()).err().unwrap().0,
            t::ERR_INVALID_CURRENCY
        );
    }

    #[test]
    fn status_follows_completeness() {
        use ExpenseStatus::*;
        assert_eq!(status_after_edit(Draft, true), New);
        assert_eq!(status_after_edit(New, false), Draft);
        assert_eq!(status_after_edit(Invalid, true), Invalid);
        assert_eq!(status_after_edit(Duplicate, false), Duplicate);
    }

    async fn test_pool() -> SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        pool
    }

    async fn add_user(pool: &SqlitePool, name: &str) -> SessionUser {
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO users (auth_provider, username, display_name, password_hash, role)
             VALUES ('local', ?, ?, 'x', 'user') RETURNING id",
        )
        .bind(name)
        .bind(name)
        .fetch_one(pool)
        .await
        .unwrap();
        SessionUser {
            id,
            display_name: name.into(),
            role: crate::model::Role::User,
        }
    }

    async fn add_expense(pool: &SqlitePool, owner: &SessionUser) -> i64 {
        sqlx::query_scalar("INSERT INTO expenses (owner_id) VALUES (?) RETURNING id")
            .bind(owner.id)
            .fetch_one(pool)
            .await
            .unwrap()
    }

    fn filled(category: &str, date: &str, amount: &str, currency: &str) -> ExpenseForm {
        ExpenseForm {
            category_id: category.into(),
            ..form(date, amount, currency)
        }
    }

    fn user_err(r: Result<(), ExpenseError>) -> &'static str {
        match r {
            Err(ExpenseError::User(m)) => m,
            Err(ExpenseError::Internal(e)) => panic!("internal error: {e:#}"),
            Ok(()) => panic!("expected an error"),
        }
    }

    #[tokio::test]
    async fn users_cannot_touch_each_others_expenses() {
        let pool = test_pool().await;
        let (alice, bob) = (add_user(&pool, "alice").await, add_user(&pool, "bob").await);
        let id = add_expense(&pool, &alice).await;

        assert!(detail(&pool, alice.id, id).await.unwrap().is_some());
        assert!(detail(&pool, bob.id, id).await.unwrap().is_none());
        assert!(list(&pool, bob.id, None, None).await.unwrap().is_empty());
        assert_eq!(list(&pool, alice.id, None, None).await.unwrap().len(), 1);

        let r = update(&pool, &bob, id, filled("1", "", "", "DKK"), None).await;
        assert_eq!(user_err(r), t::VOUCHER_NOT_FOUND);
        let r = change_status(&pool, &bob, id, ExpenseStatus::Invalid, None).await;
        assert_eq!(user_err(r), t::VOUCHER_NOT_FOUND);
        assert_eq!(
            user_err(delete(&pool, &bob, id, None).await),
            t::VOUCHER_NOT_FOUND
        );
        assert!(detail(&pool, alice.id, id).await.unwrap().is_some());
    }

    #[tokio::test]
    async fn edit_converts_currency_and_drives_status() {
        let pool = test_pool().await;
        let alice = add_user(&pool, "alice").await;
        let id = add_expense(&pool, &alice).await;
        sqlx::query(
            "INSERT INTO fx_rates (rate_date, currency, rate_per_eur) VALUES
             ('2026-09-25', 'DKK', '7.4604'), ('2026-09-25', 'SEK', '11.2')",
        )
        .execute(&pool)
        .await
        .unwrap();

        // Saturday expense uses Friday's ECB rate.
        update(
            &pool,
            &alice,
            id,
            filled("1", "2026-09-26", "123,45", "SEK"),
            None,
        )
        .await
        .ok()
        .unwrap();
        let d = detail(&pool, alice.id, id).await.unwrap().unwrap();
        assert_eq!(d.status, ExpenseStatus::New);
        assert_eq!(d.amount_base_minor, Some(8223));
        assert_eq!(d.fx_rate_date, NaiveDate::from_ymd_opt(2026, 9, 25));

        let r = update(
            &pool,
            &alice,
            id,
            filled("1", "2026-06-01", "10", "SEK"),
            None,
        )
        .await;
        assert_eq!(user_err(r), t::ERR_FX_MISSING);

        update(
            &pool,
            &alice,
            id,
            filled("", "2026-09-26", "10", "DKK"),
            None,
        )
        .await
        .ok()
        .unwrap();
        assert_eq!(
            owned_status(&pool, alice.id, id).await.unwrap(),
            Some(ExpenseStatus::Draft)
        );

        change_status(&pool, &alice, id, ExpenseStatus::Invalid, None)
            .await
            .ok()
            .unwrap();
        update(
            &pool,
            &alice,
            id,
            filled("2", "2026-09-26", "10", "DKK"),
            None,
        )
        .await
        .ok()
        .unwrap();
        assert_eq!(
            owned_status(&pool, alice.id, id).await.unwrap(),
            Some(ExpenseStatus::Invalid)
        );
        change_status(&pool, &alice, id, ExpenseStatus::New, None)
            .await
            .ok()
            .unwrap();
        assert_eq!(
            owned_status(&pool, alice.id, id).await.unwrap(),
            Some(ExpenseStatus::New)
        );

        let logged: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM audit_log WHERE entity_id = ?")
            .bind(id.to_string())
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(logged, 5);
    }

    #[tokio::test]
    async fn used_expenses_are_locked() {
        let pool = test_pool().await;
        let alice = add_user(&pool, "alice").await;
        let id = add_expense(&pool, &alice).await;
        sqlx::query("UPDATE expenses SET status = 'used' WHERE id = ?")
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();

        let r = update(&pool, &alice, id, filled("1", "", "", "DKK"), None).await;
        assert_eq!(user_err(r), t::LOCKED_USED);
        assert_eq!(
            user_err(delete(&pool, &alice, id, None).await),
            t::LOCKED_USED
        );
        let r = change_status(&pool, &alice, id, ExpenseStatus::New, None).await;
        assert_eq!(user_err(r), t::ERR_STATUS_CHANGE);
    }

    #[tokio::test]
    async fn admin_queries_see_everything_user_queries_do_not() {
        let pool = test_pool().await;
        let (alice, bob) = (add_user(&pool, "alice").await, add_user(&pool, "bob").await);
        let a = add_expense(&pool, &alice).await;
        let b = add_expense(&pool, &bob).await;
        delete(&pool, &bob, b, None).await.ok().unwrap();

        let all = list_all(&pool, None, None).await.unwrap();
        assert_eq!(all.len(), 2);
        assert!(all.iter().any(|e| e.id == b && e.deleted && e.owner_name == "bob"));
        assert_eq!(list_all(&pool, Some(alice.id), None).await.unwrap().len(), 1);

        let d = detail_any(&pool, b).await.unwrap().unwrap();
        assert!(d.deleted);
        assert_eq!(d.owner_name, "bob");
        assert!(detail(&pool, bob.id, b).await.unwrap().is_none());
        assert!(detail(&pool, bob.id, a).await.unwrap().is_none());
        assert!(list(&pool, alice.id, None, None).await.unwrap().iter().all(|e| !e.deleted));
    }

    #[tokio::test]
    async fn crops_photos_but_never_pdfs() {
        use image::{DynamicImage, ImageFormat, Rgb, RgbImage};
        use imageproc::{drawing::draw_polygon_mut, point::Point};

        let mut img = RgbImage::from_pixel(900, 700, Rgb([50, 50, 50]));
        let corners = [Point::new(200, 100), Point::new(650, 130), Point::new(620, 620), Point::new(180, 600)];
        draw_polygon_mut(&mut img, &corners, Rgb([240, 240, 240]));
        let mut png = Vec::new();
        DynamicImage::ImageRgb8(img)
            .write_to(&mut std::io::Cursor::new(&mut png), ImageFormat::Png)
            .unwrap();
        let pdf = b"%PDF-1.7 not really".to_vec();
        let file = |bytes: Vec<u8>, mime| IncomingFile { bytes, mime, name: None, original: None };

        let out = crop_images(vec![
            file(png.clone(), "image/png"),
            file(pdf.clone(), "application/pdf"),
            file(vec![0; 64], "image/heic"),
        ])
        .await
        .unwrap();

        assert_eq!(out[0].mime, "image/jpeg");
        assert_eq!(out[0].original.as_deref(), Some(png.as_slice()));
        assert!(out[0].bytes.starts_with(&[0xFF, 0xD8]), "cropped result is a JPEG");
        assert_eq!((out[1].mime, out[1].bytes.as_slice(), out[1].original.is_none()), ("application/pdf", pdf.as_slice(), true));
        assert_eq!((out[2].mime, out[2].original.is_none()), ("image/heic", true));
    }

    #[tokio::test]
    async fn deleted_expenses_disappear() {
        let pool = test_pool().await;
        let alice = add_user(&pool, "alice").await;
        let id = add_expense(&pool, &alice).await;
        delete(&pool, &alice, id, None).await.ok().unwrap();
        assert!(detail(&pool, alice.id, id).await.unwrap().is_none());
        assert!(list(&pool, alice.id, None, None).await.unwrap().is_empty());
    }
}
