use chrono::{DateTime, Utc};
use sqlx::{FromRow, SqlitePool};

use crate::model::{AdminUser, AuditEntry, Role};

pub const AUDIT_PAGE_SIZE: i64 = 100;

#[derive(FromRow)]
struct UserRow {
    id: i64,
    display_name: String,
    username: String,
    email: Option<String>,
    auth_provider: String,
    role: String,
    disabled: bool,
    created_at: DateTime<Utc>,
    last_login_at: Option<DateTime<Utc>>,
    expense_count: i64,
    sheet_count: i64,
}

pub async fn users(pool: &SqlitePool) -> sqlx::Result<Vec<AdminUser>> {
    let rows: Vec<UserRow> = sqlx::query_as(
        "SELECT u.id, u.display_name, u.username, u.email, u.auth_provider, u.role, u.disabled,
                u.created_at, u.last_login_at,
                (SELECT COUNT(*) FROM expenses e WHERE e.owner_id = u.id AND e.deleted_at IS NULL) AS expense_count,
                (SELECT COUNT(*) FROM expense_sheets s WHERE s.owner_id = u.id) AS sheet_count
         FROM users u ORDER BY u.display_name COLLATE NOCASE, u.id",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| AdminUser {
            id: r.id,
            display_name: r.display_name,
            username: r.username,
            email: r.email,
            provider: r.auth_provider,
            role: Role::parse(&r.role).unwrap_or(Role::User),
            disabled: r.disabled,
            created_at: r.created_at,
            last_login_at: r.last_login_at,
            expense_count: r.expense_count,
            sheet_count: r.sheet_count,
        })
        .collect())
}

#[derive(FromRow)]
struct AuditRow {
    id: i64,
    at: DateTime<Utc>,
    actor_user_id: Option<i64>,
    actor_name: Option<String>,
    action: String,
    entity_type: Option<String>,
    entity_id: Option<String>,
    details: Option<String>,
    ip: Option<String>,
}

/// Newest first; pass the smallest id already shown as `before_id` to page backwards.
pub async fn audit_log(
    pool: &SqlitePool,
    user_id: Option<i64>,
    before_id: Option<i64>,
) -> sqlx::Result<Vec<AuditEntry>> {
    // A user filter also matches events about that user, e.g. failed logins or role changes.
    let rows: Vec<AuditRow> = sqlx::query_as(
        "SELECT a.id, a.at, a.actor_user_id, u.display_name AS actor_name, a.action, a.entity_type,
                a.entity_id, a.details, a.ip
         FROM audit_log a LEFT JOIN users u ON u.id = a.actor_user_id
         WHERE (?1 IS NULL OR a.actor_user_id = ?1 OR (a.entity_type = 'user' AND a.entity_id = CAST(?1 AS TEXT)))
           AND (?2 IS NULL OR a.id < ?2)
         ORDER BY a.id DESC LIMIT ?3",
    )
    .bind(user_id)
    .bind(before_id)
    .bind(AUDIT_PAGE_SIZE)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| AuditEntry {
            id: r.id,
            at: r.at,
            actor_id: r.actor_user_id,
            actor_name: r.actor_name,
            action: r.action,
            entity_type: r.entity_type,
            entity_id: r.entity_id,
            details: r.details,
            ip: r.ip,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::audit;

    #[tokio::test]
    async fn lists_users_and_pages_audit_log() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        let mut ids = Vec::new();
        for name in ["bob", "Alice"] {
            let id: i64 = sqlx::query_scalar(
                "INSERT INTO users (auth_provider, username, display_name, password_hash, role)
                 VALUES ('local', ?, ?, 'x', 'user') RETURNING id",
            )
            .bind(name.to_lowercase())
            .bind(name)
            .fetch_one(&pool)
            .await
            .unwrap();
            ids.push(id);
        }
        sqlx::query("INSERT INTO expenses (owner_id) VALUES (?)").bind(ids[0]).execute(&pool).await.unwrap();

        let list = users(&pool).await.unwrap();
        assert_eq!(list.iter().map(|u| u.display_name.as_str()).collect::<Vec<_>>(), ["Alice", "bob"]);
        assert_eq!(list[1].expense_count, 1);

        for i in 0..150 {
            audit::record(&pool, Some(ids[i % 2]), "login", Some(("user", &ids[i % 2].to_string())), None, None)
                .await
                .unwrap();
        }
        audit::record(&pool, None, "login_failed", Some(("user", &ids[1].to_string())), None, None).await.unwrap();

        let first = audit_log(&pool, None, None).await.unwrap();
        assert_eq!(first.len(), 100);
        assert_eq!(first[0].action, "login_failed");
        let rest = audit_log(&pool, None, Some(first.last().unwrap().id)).await.unwrap();
        assert_eq!(rest.len(), 51);

        let alice = audit_log(&pool, Some(ids[1]), None).await.unwrap();
        assert_eq!(alice.len(), 76, "75 own logins plus the failed login about her");
        assert!(alice.iter().all(|e| e.actor_id != Some(ids[0])));
    }
}
