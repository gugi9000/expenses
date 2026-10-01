//! Server functions called from the UI. Every call is scoped to the logged-in user;
//! `admin_*` functions additionally require the admin role and are read-only.

use leptos::prelude::*;

use crate::model::{
    AdminUser, AuditEntry, Category, ExpenseDetail, ExpenseListItem, SheetDefaults, SheetDetail,
    SheetSummary,
};
#[cfg(feature = "ssr")]
use crate::server::{
    auth::{require_admin, require_user},
    expenses::ExpenseError,
};

#[cfg(feature = "ssr")]
fn internal(e: impl std::fmt::Display) -> ServerFnError {
    tracing::error!("server function failed: {e}");
    ServerFnError::new(crate::i18n::t::GENERIC_ERROR)
}

#[cfg(feature = "ssr")]
fn to_server_error(e: ExpenseError) -> ServerFnError {
    match e {
        ExpenseError::User(msg) => ServerFnError::new(msg),
        ExpenseError::Internal(e) => internal(format!("{e:#}")),
    }
}

/// The message to show the user for a failed server function.
pub fn error_text(e: &ServerFnError) -> String {
    match e {
        ServerFnError::ServerError(msg) => msg.clone(),
        _ => crate::i18n::t::GENERIC_ERROR.to_string(),
    }
}

#[server]
pub async fn list_categories() -> Result<Vec<Category>, ServerFnError> {
    let ctx = require_user().await?;
    crate::server::expenses::categories(&ctx.state.pool)
        .await
        .map_err(internal)
}

#[server]
pub async fn list_expenses(
    status: Option<String>,
    category_id: Option<i64>,
) -> Result<Vec<ExpenseListItem>, ServerFnError> {
    use crate::model::ExpenseStatus;
    let ctx = require_user().await?;
    let status = status.as_deref().and_then(ExpenseStatus::parse);
    crate::server::expenses::list(&ctx.state.pool, ctx.user.id, status, category_id)
        .await
        .map_err(internal)
}

#[server]
pub async fn get_expense(id: i64) -> Result<Option<ExpenseDetail>, ServerFnError> {
    let ctx = require_user().await?;
    crate::server::expenses::detail(&ctx.state.pool, ctx.user.id, id)
        .await
        .map_err(internal)
}

#[server]
#[allow(clippy::too_many_arguments)]
pub async fn update_expense(
    id: i64,
    kind: String,
    category_id: String,
    vendor: String,
    description: String,
    expense_date: String,
    amount: String,
    currency: String,
) -> Result<(), ServerFnError> {
    use crate::server::expenses::{ExpenseForm, update};
    let ctx = require_user().await?;
    let form = ExpenseForm {
        kind,
        category_id,
        vendor,
        description,
        expense_date,
        amount,
        currency,
    };
    update(&ctx.state.pool, &ctx.user, id, form, ctx.ip.as_deref())
        .await
        .map_err(to_server_error)
}

#[server]
pub async fn change_expense_status(id: i64, status: String) -> Result<(), ServerFnError> {
    use crate::{i18n::t, model::ExpenseStatus};
    let ctx = require_user().await?;
    let status =
        ExpenseStatus::parse(&status).ok_or_else(|| ServerFnError::new(t::ERR_STATUS_CHANGE))?;
    crate::server::expenses::change_status(
        &ctx.state.pool,
        &ctx.user,
        id,
        status,
        ctx.ip.as_deref(),
    )
    .await
    .map_err(to_server_error)
}

#[server]
pub async fn delete_expense(id: i64) -> Result<(), ServerFnError> {
    let ctx = require_user().await?;
    crate::server::expenses::delete(&ctx.state.pool, &ctx.user, id, ctx.ip.as_deref())
        .await
        .map_err(to_server_error)
}

#[server]
pub async fn list_sheets() -> Result<Vec<SheetSummary>, ServerFnError> {
    let ctx = require_user().await?;
    crate::server::sheets::list(&ctx.state.pool, ctx.user.id)
        .await
        .map_err(internal)
}

#[server]
pub async fn get_sheet(id: i64) -> Result<Option<SheetDetail>, ServerFnError> {
    let ctx = require_user().await?;
    crate::server::sheets::detail(&ctx.state.pool, Some(ctx.user.id), id)
        .await
        .map_err(internal)
}

#[server]
pub async fn new_sheet_defaults() -> Result<SheetDefaults, ServerFnError> {
    let ctx = require_user().await?;
    let bank_account = crate::server::sheets::saved_bank_account(&ctx.state.pool, ctx.user.id)
        .await
        .map_err(internal)?;
    Ok(SheetDefaults {
        title: crate::server::sheets::default_title(chrono::Utc::now().date_naive()),
        bank_account,
    })
}

#[server]
pub async fn create_sheet(
    title: String,
    bank_account: String,
    expense_ids: Vec<i64>,
) -> Result<i64, ServerFnError> {
    let ctx = require_user().await?;
    crate::server::sheets::create(
        &ctx.state,
        &ctx.user,
        &title,
        &bank_account,
        &expense_ids,
        ctx.ip.as_deref(),
    )
    .await
    .map_err(to_server_error)
}

#[server]
pub async fn void_sheet(id: i64) -> Result<(), ServerFnError> {
    let ctx = require_user().await?;
    crate::server::sheets::void(&ctx.state.pool, &ctx.user, id, ctx.ip.as_deref())
        .await
        .map_err(to_server_error)
}

#[server]
pub async fn admin_list_users() -> Result<Vec<AdminUser>, ServerFnError> {
    let ctx = require_admin().await?;
    crate::server::admin::users(&ctx.state.pool).await.map_err(internal)
}

#[server]
pub async fn admin_list_expenses(
    owner_id: Option<i64>,
    status: Option<String>,
) -> Result<Vec<ExpenseListItem>, ServerFnError> {
    use crate::model::ExpenseStatus;
    let ctx = require_admin().await?;
    let status = status.as_deref().and_then(ExpenseStatus::parse);
    crate::server::expenses::list_all(&ctx.state.pool, owner_id, status)
        .await
        .map_err(internal)
}

/// Opening another user's expense or sheet is logged, since it is access to personal data.
#[cfg(feature = "ssr")]
async fn log_admin_view(ctx: &crate::server::auth::RequestCtx, entity: &str, id: i64) -> Result<(), ServerFnError> {
    crate::server::audit::record(
        &ctx.state.pool,
        Some(ctx.user.id),
        &format!("admin_viewed_{entity}"),
        Some((entity, &id.to_string())),
        None,
        ctx.ip.as_deref(),
    )
    .await
    .map_err(internal)
}

#[server]
pub async fn admin_get_expense(id: i64) -> Result<Option<ExpenseDetail>, ServerFnError> {
    let ctx = require_admin().await?;
    let detail = crate::server::expenses::detail_any(&ctx.state.pool, id)
        .await
        .map_err(internal)?;
    if detail.is_some() {
        log_admin_view(&ctx, "expense", id).await?;
    }
    Ok(detail)
}

#[server]
pub async fn admin_list_sheets(owner_id: Option<i64>) -> Result<Vec<SheetSummary>, ServerFnError> {
    let ctx = require_admin().await?;
    crate::server::sheets::list_all(&ctx.state.pool, owner_id)
        .await
        .map_err(internal)
}

#[server]
pub async fn admin_get_sheet(id: i64) -> Result<Option<SheetDetail>, ServerFnError> {
    let ctx = require_admin().await?;
    let detail = crate::server::sheets::detail(&ctx.state.pool, None, id)
        .await
        .map_err(internal)?;
    if detail.is_some() {
        log_admin_view(&ctx, "sheet", id).await?;
    }
    Ok(detail)
}

#[server]
pub async fn admin_audit_log(
    user_id: Option<i64>,
    before_id: Option<i64>,
) -> Result<Vec<AuditEntry>, ServerFnError> {
    let ctx = require_admin().await?;
    crate::server::admin::audit_log(&ctx.state.pool, user_id, before_id)
        .await
        .map_err(internal)
}
