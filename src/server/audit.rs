use serde_json::Value;
use sqlx::SqliteExecutor;

/// Appends a row to the append-only audit log. Pass a transaction to make it atomic with the change.
pub async fn record<'e>(
    db: impl SqliteExecutor<'e>,
    actor_user_id: Option<i64>,
    action: &str,
    entity: Option<(&str, &str)>,
    details: Option<Value>,
    ip: Option<&str>,
) -> sqlx::Result<()> {
    let (entity_type, entity_id) = entity.unzip();
    sqlx::query(
        "INSERT INTO audit_log (actor_user_id, action, entity_type, entity_id, details, ip)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(actor_user_id)
    .bind(action)
    .bind(entity_type)
    .bind(entity_id)
    .bind(details.map(|d| d.to_string()))
    .bind(ip)
    .execute(db)
    .await?;
    Ok(())
}
