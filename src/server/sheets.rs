use std::{collections::HashSet, net::SocketAddr};

use axum::{
    Router,
    body::Body,
    extract::{ConnectInfo, Path, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use chrono::{DateTime, NaiveDate, Utc};
use serde_json::json;
use sqlx::{FromRow, SqlitePool};

use super::{
    audit,
    auth::{AppError, CurrentUser},
    expenses::{ExpenseError, UserError},
    pdf::{self, PdfAttachment, PdfItem, PdfSheet},
    security::client_ip,
    state::AppState,
};
use crate::{
    i18n::{format_date, t},
    model::{ExpenseKind, SessionUser, SheetDetail, SheetItem, SheetStatus, SheetSummary},
};

const MAX_TITLE_CHARS: usize = 120;
const MAX_BANK_ACCOUNT_CHARS: usize = 40;
const MAX_ITEMS: usize = 500;

pub fn routes() -> Router<AppState> {
    Router::new().route("/afregninger/{id}/pdf", get(download_pdf))
}

fn placeholders(n: usize) -> String {
    vec!["?"; n].join(", ")
}

#[derive(FromRow)]
struct SummaryRow {
    id: i64,
    owner_name: String,
    title: String,
    status: String,
    created_at: DateTime<Utc>,
    item_count: i64,
    total_base_minor: i64,
}

impl From<SummaryRow> for SheetSummary {
    fn from(r: SummaryRow) -> Self {
        SheetSummary {
            id: r.id,
            owner_name: r.owner_name,
            title: r.title,
            status: SheetStatus::parse(&r.status).unwrap_or(SheetStatus::Active),
            created_at: r.created_at,
            item_count: r.item_count,
            total_base_minor: r.total_base_minor,
        }
    }
}

const SUMMARY_SELECT: &str =
    "SELECT s.id, u.display_name AS owner_name, s.title, s.status, s.created_at,
        s.total_base_minor,
        (SELECT COUNT(*) FROM expense_sheet_items i WHERE i.sheet_id = s.id) AS item_count
     FROM expense_sheets s JOIN users u ON u.id = s.owner_id";

pub async fn list(pool: &SqlitePool, owner_id: i64) -> sqlx::Result<Vec<SheetSummary>> {
    list_all(pool, Some(owner_id)).await
}

/// `owner_filter = None` lists every user's sheets (admin view).
pub async fn list_all(
    pool: &SqlitePool,
    owner_filter: Option<i64>,
) -> sqlx::Result<Vec<SheetSummary>> {
    let rows: Vec<SummaryRow> = sqlx::query_as(&format!(
        "{SUMMARY_SELECT} WHERE (?1 IS NULL OR s.owner_id = ?1) ORDER BY s.created_at DESC, s.id DESC LIMIT 500"
    ))
    .bind(owner_filter)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(Into::into).collect())
}

#[derive(FromRow)]
struct ItemRow {
    expense_id: i64,
    kind: String,
    category_name: Option<String>,
    vendor: Option<String>,
    description: Option<String>,
    expense_date: NaiveDate,
    amount_minor: i64,
    currency: String,
    fx_rate: String,
    amount_base_minor: i64,
}

async fn items(pool: &SqlitePool, sheet_id: i64) -> sqlx::Result<Vec<SheetItem>> {
    let rows: Vec<ItemRow> = sqlx::query_as(
        "SELECT expense_id, kind, category_name, vendor, description, expense_date, amount_minor,
                currency, fx_rate, amount_base_minor
         FROM expense_sheet_items WHERE sheet_id = ? ORDER BY expense_date, expense_id",
    )
    .bind(sheet_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .enumerate()
        .map(|(i, r)| SheetItem {
            position: i + 1,
            expense_id: r.expense_id,
            kind: ExpenseKind::parse(&r.kind).unwrap_or(ExpenseKind::Receipt),
            category: r.category_name,
            vendor: r.vendor,
            description: r.description,
            expense_date: r.expense_date,
            amount_minor: r.amount_minor,
            currency: r.currency,
            fx_rate: r.fx_rate,
            amount_base_minor: r.amount_base_minor,
        })
        .collect())
}

/// `owner_id = None` is the admin view and returns any user's sheet.
pub async fn detail(
    pool: &SqlitePool,
    owner_id: Option<i64>,
    id: i64,
) -> sqlx::Result<Option<SheetDetail>> {
    let summary: Option<SummaryRow> = sqlx::query_as(&format!(
        "{SUMMARY_SELECT} WHERE s.id = ? AND (? IS NULL OR s.owner_id = ?)"
    ))
    .bind(id)
    .bind(owner_id)
    .bind(owner_id)
    .fetch_optional(pool)
    .await?;
    let Some(summary) = summary else {
        return Ok(None);
    };

    let (owner_name, bank_account): (String, Option<String>) = sqlx::query_as(
        "SELECT u.display_name, s.bank_account FROM expense_sheets s JOIN users u ON u.id = s.owner_id WHERE s.id = ?",
    )
    .bind(id)
    .fetch_one(pool)
    .await?;
    Ok(Some(SheetDetail {
        summary: summary.into(),
        owner_name,
        bank_account,
        items: items(pool, id).await?,
    }))
}

#[derive(FromRow)]
struct CandidateRow {
    id: i64,
    kind: String,
    category_name: Option<String>,
    vendor: Option<String>,
    description: Option<String>,
    expense_date: Option<NaiveDate>,
    amount_minor: Option<i64>,
    currency: Option<String>,
    fx_rate: Option<String>,
    amount_base_minor: Option<i64>,
}

pub fn default_title(today: NaiveDate) -> String {
    format!("{} {}", t::EXPENSE_SHEET, format_date(today))
}

/// Accepts a Danish reg.nr. + kontonr. or an IBAN; returns `None` for an empty field.
fn normalize_bank_account(input: &str) -> Result<Option<String>, UserError> {
    let s = input.split_whitespace().collect::<Vec<_>>().join(" ");
    if s.is_empty() {
        return Ok(None);
    }
    let valid = s.len() <= MAX_BANK_ACCOUNT_CHARS
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, ' ' | '-' | '.'))
        && s.chars().filter(char::is_ascii_digit).count() >= 6;
    if valid {
        Ok(Some(s.to_ascii_uppercase()))
    } else {
        Err(UserError(t::ERR_BANK_ACCOUNT))
    }
}

/// Keeps account numbers out of the audit log except for the last four characters.
fn mask_account(account: &str) -> String {
    let chars: Vec<char> = account
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect();
    let tail: String = chars[chars.len().saturating_sub(4)..].iter().collect();
    format!("…{tail}")
}

pub async fn saved_bank_account(pool: &SqlitePool, user_id: i64) -> sqlx::Result<Option<String>> {
    sqlx::query_scalar("SELECT bank_account FROM users WHERE id = ?")
        .bind(user_id)
        .fetch_one(pool)
        .await
}

pub async fn create(
    state: &AppState,
    user: &SessionUser,
    title: &str,
    bank_account: &str,
    expense_ids: &[i64],
    ip: Option<&str>,
) -> Result<i64, ExpenseError> {
    let bank_account = normalize_bank_account(bank_account)?;
    let title = title.split_whitespace().collect::<Vec<_>>().join(" ");
    if title.chars().count() > MAX_TITLE_CHARS {
        return Err(UserError(t::ERR_TITLE_TOO_LONG).into());
    }
    let title = if title.is_empty() {
        default_title(Utc::now().date_naive())
    } else {
        title
    };
    let ids: Vec<i64> = expense_ids
        .iter()
        .copied()
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    if ids.is_empty() {
        return Err(UserError(t::ERR_SHEET_EMPTY).into());
    }
    if ids.len() > MAX_ITEMS {
        return Err(UserError(t::ERR_SHEET_STALE).into());
    }

    let mut tx = state.pool.begin().await?;
    let sql = format!(
        "SELECT e.id, e.kind, c.name_da AS category_name, e.vendor, e.description, e.expense_date,
                e.amount_minor, e.currency, e.fx_rate, e.amount_base_minor
         FROM expenses e LEFT JOIN expense_categories c ON c.id = e.category_id
         WHERE e.owner_id = ? AND e.status = 'new' AND e.deleted_at IS NULL AND e.id IN ({})",
        placeholders(ids.len())
    );
    let mut query = sqlx::query_as::<_, CandidateRow>(&sql).bind(user.id);
    for id in &ids {
        query = query.bind(id);
    }
    let rows = query.fetch_all(&mut *tx).await?;
    // Every selected expense must still be the user's own, unused and complete.
    if rows.len() != ids.len() {
        return Err(UserError(t::ERR_SHEET_STALE).into());
    }

    let mut complete = Vec::with_capacity(rows.len());
    for r in rows {
        let (Some(date), Some(amount), Some(currency), Some(rate), Some(base)) = (
            r.expense_date,
            r.amount_minor,
            r.currency,
            r.fx_rate,
            r.amount_base_minor,
        ) else {
            return Err(UserError(t::ERR_SHEET_STALE).into());
        };
        complete.push((
            r.id,
            r.kind,
            r.category_name,
            r.vendor,
            r.description,
            date,
            amount,
            currency,
            rate,
            base,
        ));
    }
    let total: i64 = complete.iter().map(|c| c.9).sum();

    let sheet_id: i64 = sqlx::query_scalar(
        "INSERT INTO expense_sheets (owner_id, title, total_base_minor, bank_account) VALUES (?, ?, ?, ?) RETURNING id",
    )
    .bind(user.id)
    .bind(&title)
    .bind(total)
    .bind(&bank_account)
    .fetch_one(&mut *tx)
    .await?;

    if let Some(account) = &bank_account {
        let changed =
            sqlx::query("UPDATE users SET bank_account = ? WHERE id = ? AND bank_account IS NOT ?")
                .bind(account)
                .bind(user.id)
                .bind(account)
                .execute(&mut *tx)
                .await?
                .rows_affected();
        if changed == 1 {
            audit::record(
                &mut *tx,
                Some(user.id),
                "user_bank_account_changed",
                Some(("user", &user.id.to_string())),
                Some(json!({"bank_account": mask_account(account)})),
                ip,
            )
            .await?;
        }
    }

    for (id, kind, category, vendor, description, date, amount, currency, rate, base) in &complete {
        sqlx::query(
            "INSERT INTO expense_sheet_items (sheet_id, expense_id, kind, category_name, vendor, description,
                 expense_date, amount_minor, currency, fx_rate, amount_base_minor)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(sheet_id)
        .bind(id)
        .bind(kind)
        .bind(category)
        .bind(vendor)
        .bind(description)
        .bind(date)
        .bind(amount)
        .bind(currency)
        .bind(rate)
        .bind(base)
        .execute(&mut *tx)
        .await?;
        let updated = sqlx::query(
            "UPDATE expenses SET status = 'used', updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
             WHERE id = ? AND status = 'new'",
        )
        .bind(id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if updated != 1 {
            return Err(UserError(t::ERR_SHEET_STALE).into());
        }
        audit::record(
            &mut *tx,
            Some(user.id),
            "expense_status_changed",
            Some(("expense", &id.to_string())),
            Some(json!({"from": "new", "to": "used", "sheet_id": sheet_id})),
            ip,
        )
        .await?;
    }
    audit::record(
        &mut *tx,
        Some(user.id),
        "sheet_created",
        Some(("sheet", &sheet_id.to_string())),
        Some(json!({"title": title, "expense_ids": ids, "total_base_minor": total})),
        ip,
    )
    .await?;
    tx.commit().await?;

    if let Err(e) = ensure_pdf(state, sheet_id).await {
        // The download route retries, so a failure here doesn't lose the sheet.
        tracing::error!("generating PDF for sheet {sheet_id} failed: {e:#}");
    }
    Ok(sheet_id)
}

pub async fn void(
    pool: &SqlitePool,
    user: &SessionUser,
    id: i64,
    ip: Option<&str>,
) -> Result<(), ExpenseError> {
    let mut tx = pool.begin().await?;
    let status: Option<String> =
        sqlx::query_scalar("SELECT status FROM expense_sheets WHERE id = ? AND owner_id = ?")
            .bind(id)
            .bind(user.id)
            .fetch_optional(&mut *tx)
            .await?;
    match status.as_deref() {
        None => return Err(UserError(t::SHEET_NOT_FOUND).into()),
        Some("active") => {}
        Some(_) => return Err(UserError(t::ERR_SHEET_NOT_ACTIVE).into()),
    }

    sqlx::query(
        "UPDATE expense_sheets SET status = 'voided', voided_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
         WHERE id = ? AND status = 'active'",
    )
    .bind(id)
    .execute(&mut *tx)
    .await?;
    let expense_ids: Vec<i64> = sqlx::query_scalar(
        "SELECT expense_id FROM expense_sheet_items WHERE sheet_id = ? AND active = 1",
    )
    .bind(id)
    .fetch_all(&mut *tx)
    .await?;
    sqlx::query("UPDATE expense_sheet_items SET active = NULL WHERE sheet_id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    for expense_id in &expense_ids {
        sqlx::query(
            "UPDATE expenses SET status = 'new', updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
             WHERE id = ? AND status = 'used'",
        )
        .bind(expense_id)
        .execute(&mut *tx)
        .await?;
        audit::record(
            &mut *tx,
            Some(user.id),
            "expense_status_changed",
            Some(("expense", &expense_id.to_string())),
            Some(json!({"from": "used", "to": "new", "sheet_id": id})),
            ip,
        )
        .await?;
    }
    audit::record(
        &mut *tx,
        Some(user.id),
        "sheet_voided",
        Some(("sheet", &id.to_string())),
        Some(json!({"expense_ids": expense_ids})),
        ip,
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Returns the stored PDF's hash, rendering and storing it on first use. A stored PDF is never regenerated.
pub async fn ensure_pdf(state: &AppState, sheet_id: i64) -> anyhow::Result<String> {
    let existing: Option<String> =
        sqlx::query_scalar("SELECT pdf_sha256 FROM expense_sheets WHERE id = ?")
            .bind(sheet_id)
            .fetch_one(&state.pool)
            .await?;
    if let Some(sha) = existing {
        return Ok(sha);
    }

    let detail = detail(&state.pool, None, sheet_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("sheet {sheet_id} not found"))?;
    let mut items = Vec::with_capacity(detail.items.len());
    for item in detail.items {
        let files: Vec<(String, String)> = sqlx::query_as(
            "SELECT sha256, mime FROM attachments WHERE expense_id = ? ORDER BY page_order, id",
        )
        .bind(item.expense_id)
        .fetch_all(&state.pool)
        .await?;
        let mut attachments = Vec::with_capacity(files.len());
        for (sha, mime) in files {
            attachments.push(PdfAttachment {
                bytes: state.files.get(&sha).await?,
                mime,
            });
        }
        items.push(PdfItem { item, attachments });
    }
    let sheet = PdfSheet {
        summary: detail.summary,
        owner_name: detail.owner_name,
        bank_account: detail.bank_account,
        items,
    };
    let bytes = tokio::task::spawn_blocking(move || pdf::render(&sheet)).await??;
    let sha = state.files.put(&bytes).await?;

    let stored =
        sqlx::query("UPDATE expense_sheets SET pdf_sha256 = ? WHERE id = ? AND pdf_sha256 IS NULL")
            .bind(&sha)
            .bind(sheet_id)
            .execute(&state.pool)
            .await?
            .rows_affected();
    if stored == 1 {
        audit::record(
            &state.pool,
            None,
            "sheet_pdf_stored",
            Some(("sheet", &sheet_id.to_string())),
            Some(json!({"sha256": sha, "size": bytes.len()})),
            None,
        )
        .await?;
        Ok(sha)
    } else {
        // Another request stored it first; serve that one.
        Ok(
            sqlx::query_scalar("SELECT pdf_sha256 FROM expense_sheets WHERE id = ?")
                .bind(sheet_id)
                .fetch_one(&state.pool)
                .await?,
        )
    }
}

async fn download_pdf(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> Result<Response, AppError> {
    let owner: Option<i64> = sqlx::query_scalar("SELECT owner_id FROM expense_sheets WHERE id = ?")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?;
    match owner {
        Some(o) if o == user.id || user.is_admin() => {}
        _ => return Ok(StatusCode::NOT_FOUND.into_response()),
    }

    let sha = ensure_pdf(&state, id).await?;
    let bytes = state.files.get(&sha).await?;
    let ip = client_ip(&headers, Some(peer), state.config.trust_proxy);
    audit::record(
        &state.pool,
        Some(user.id),
        "sheet_pdf_downloaded",
        Some(("sheet", &id.to_string())),
        None,
        ip.as_deref(),
    )
    .await?;

    let mut res = Response::new(Body::from(bytes));
    let h = res.headers_mut();
    h.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/pdf"),
    );
    h.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!(
            "attachment; filename=\"udgiftsafregning-{id}.pdf\""
        ))?,
    );
    h.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    h.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    Ok(res)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::{
        model::Role,
        server::{config::Config, files::FileStore, ocr::NoopOcr},
    };

    async fn test_state() -> AppState {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        let data_dir = std::env::temp_dir().join(format!(
            "udgifter-test-{}",
            crate::server::session::random_token()
        ));
        AppState {
            leptos_options: leptos::config::LeptosOptions::builder()
                .output_name("expenses")
                .build(),
            pool,
            files: Arc::new(FileStore::new(&data_dir)),
            ocr: Arc::new(NoopOcr),
            config: Arc::new(Config {
                database_url: String::new(),
                data_dir,
                base_url: "http://localhost".into(),
                cookie_secure: false,
                trust_proxy: false,
                session_hours: 1,
                entra: None,
            }),
            entra: None,
        }
    }

    async fn user(pool: &SqlitePool, name: &str) -> SessionUser {
        let id = sqlx::query_scalar(
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
            role: Role::User,
            theme: Default::default(),
        }
    }

    async fn expense(pool: &SqlitePool, owner: &SessionUser, status: &str, base: i64) -> i64 {
        sqlx::query_scalar(
            "INSERT INTO expenses (owner_id, category_id, vendor, expense_date, amount_minor, currency, fx_rate,
                 fx_rate_date, amount_base_minor, status)
             VALUES (?, 1, 'Café', '2026-09-29', ?, 'DKK', '1.000000', '2026-09-29', ?, ?) RETURNING id",
        )
        .bind(owner.id)
        .bind(base)
        .bind(base)
        .bind(status)
        .fetch_one(pool)
        .await
        .unwrap()
    }

    async fn status(pool: &SqlitePool, id: i64) -> String {
        sqlx::query_scalar("SELECT status FROM expenses WHERE id = ?")
            .bind(id)
            .fetch_one(pool)
            .await
            .unwrap()
    }

    fn user_err<T>(r: Result<T, ExpenseError>) -> &'static str {
        match r {
            Err(ExpenseError::User(m)) => m,
            Err(ExpenseError::Internal(e)) => panic!("internal error: {e:#}"),
            Ok(_) => panic!("expected an error"),
        }
    }

    #[tokio::test]
    async fn sheet_lifecycle() {
        let state = test_state().await;
        let pool = &state.pool;
        let (alice, bob) = (user(pool, "alice").await, user(pool, "bob").await);
        let a1 = expense(pool, &alice, "new", 10000).await;
        let a2 = expense(pool, &alice, "new", 2550).await;
        let draft = expense(pool, &alice, "draft", 100).await;
        let b1 = expense(pool, &bob, "new", 500).await;

        assert_eq!(
            user_err(create(&state, &alice, "", "", &[], None).await),
            t::ERR_SHEET_EMPTY
        );
        assert_eq!(
            user_err(create(&state, &alice, "", "", &[a1, b1], None).await),
            t::ERR_SHEET_STALE
        );
        assert_eq!(
            user_err(create(&state, &alice, "", "", &[a1, draft], None).await),
            t::ERR_SHEET_STALE
        );
        assert_eq!(
            user_err(create(&state, &alice, "", "12 34", &[a1], None).await),
            t::ERR_BANK_ACCOUNT
        );
        assert_eq!(
            status(pool, a1).await,
            "new",
            "a failed create must not change anything"
        );

        let sheet = create(
            &state,
            &alice,
            "  Rejse   til Aarhus ",
            " 1234  0001234567",
            &[a1, a2, a1],
            None,
        )
        .await
        .ok()
        .unwrap();
        assert_eq!(
            (status(pool, a1).await, status(pool, a2).await),
            ("used".into(), "used".into())
        );
        let d = detail(pool, Some(alice.id), sheet).await.unwrap().unwrap();
        assert_eq!(d.summary.title, "Rejse til Aarhus");
        assert_eq!(d.bank_account.as_deref(), Some("1234 0001234567"));
        assert_eq!(
            saved_bank_account(pool, alice.id).await.unwrap().as_deref(),
            Some("1234 0001234567")
        );
        assert_eq!(saved_bank_account(pool, bob.id).await.unwrap(), None);
        assert_eq!(d.summary.total_base_minor, 12550);
        assert_eq!(
            d.items.iter().map(|i| i.position).collect::<Vec<_>>(),
            vec![1, 2]
        );
        assert!(detail(pool, Some(bob.id), sheet).await.unwrap().is_none());
        assert!(
            detail(pool, None, sheet).await.unwrap().is_some(),
            "admin view sees every sheet"
        );

        let pdf: Option<String> =
            sqlx::query_scalar("SELECT pdf_sha256 FROM expense_sheets WHERE id = ?")
                .bind(sheet)
                .fetch_one(pool)
                .await
                .unwrap();
        let pdf = pdf.expect("PDF stored at creation");
        assert_eq!(
            ensure_pdf(&state, sheet).await.unwrap(),
            pdf,
            "stored PDF is never regenerated"
        );
        assert!(state.files.get(&pdf).await.unwrap().starts_with(b"%PDF"));

        assert_eq!(
            user_err(create(&state, &alice, "", "", &[a1], None).await),
            t::ERR_SHEET_STALE
        );
        assert_eq!(
            user_err(void(pool, &bob, sheet, None).await),
            t::SHEET_NOT_FOUND
        );

        void(pool, &alice, sheet, None).await.ok().unwrap();
        assert_eq!(
            (status(pool, a1).await, status(pool, a2).await),
            ("new".into(), "new".into())
        );
        assert_eq!(
            user_err(void(pool, &alice, sheet, None).await),
            t::ERR_SHEET_NOT_ACTIVE
        );

        let again = create(&state, &alice, "", "", &[a1], None)
            .await
            .ok()
            .unwrap();
        assert_ne!(again, sheet);
        assert_eq!(
            saved_bank_account(pool, alice.id).await.unwrap().as_deref(),
            Some("1234 0001234567"),
            "an empty field doesn't erase the saved account"
        );
        assert_eq!(list(pool, alice.id).await.unwrap().len(), 2);
        assert!(list(pool, bob.id).await.unwrap().is_empty());

        let actions: Vec<String> = sqlx::query_scalar(
            "SELECT action FROM audit_log WHERE entity_type = 'sheet' ORDER BY id",
        )
        .fetch_all(pool)
        .await
        .unwrap();
        assert_eq!(
            actions,
            [
                "sheet_created",
                "sheet_pdf_stored",
                "sheet_voided",
                "sheet_created",
                "sheet_pdf_stored"
            ]
        );
        let _ = std::fs::remove_dir_all(&state.config.data_dir);
    }
}
