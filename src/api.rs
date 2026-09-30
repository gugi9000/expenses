//! Server functions called from the UI. Every call is scoped to the logged-in user.

use leptos::prelude::*;

use crate::model::{Category, ExpenseDetail, ExpenseListItem};
#[cfg(feature = "ssr")]
use crate::server::{auth::require_user, expenses::ExpenseError};

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
    crate::server::expenses::categories(&ctx.state.pool).await.map_err(internal)
}

#[server]
pub async fn list_expenses(status: Option<String>, category_id: Option<i64>) -> Result<Vec<ExpenseListItem>, ServerFnError> {
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
    crate::server::expenses::detail(&ctx.state.pool, ctx.user.id, id).await.map_err(internal)
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
    let form = ExpenseForm { kind, category_id, vendor, description, expense_date, amount, currency };
    update(&ctx.state.pool, &ctx.user, id, form, ctx.ip.as_deref()).await.map_err(to_server_error)
}

#[server]
pub async fn change_expense_status(id: i64, status: String) -> Result<(), ServerFnError> {
    use crate::{i18n::t, model::ExpenseStatus};
    let ctx = require_user().await?;
    let status = ExpenseStatus::parse(&status).ok_or_else(|| ServerFnError::new(t::ERR_STATUS_CHANGE))?;
    crate::server::expenses::change_status(&ctx.state.pool, &ctx.user, id, status, ctx.ip.as_deref())
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
