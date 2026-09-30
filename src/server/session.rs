use axum_extra::extract::cookie::{Cookie, SameSite};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{Duration, Utc};
use rand::RngCore;
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;

use super::config::Config;
use crate::model::{Role, SessionUser};

pub const COOKIE_NAME: &str = "udgifter_session";

pub fn random_token() -> String {
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

/// Only the hash is stored, so a leaked database cannot be used to hijack sessions.
fn hash_token(token: &str) -> String {
    Sha256::digest(token.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

pub fn sqlite_timestamp(t: chrono::DateTime<Utc>) -> String {
    t.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
}

pub async fn create(pool: &SqlitePool, user_id: i64, hours: i64) -> sqlx::Result<String> {
    let token = random_token();
    sqlx::query("INSERT INTO sessions (token_hash, user_id, expires_at) VALUES (?, ?, ?)")
        .bind(hash_token(&token))
        .bind(user_id)
        .bind(sqlite_timestamp(Utc::now() + Duration::hours(hours)))
        .execute(pool)
        .await?;
    Ok(token)
}

pub async fn lookup(pool: &SqlitePool, token: &str) -> sqlx::Result<Option<SessionUser>> {
    let row: Option<(i64, String, String)> = sqlx::query_as(
        "SELECT u.id, u.display_name, u.role
         FROM sessions s JOIN users u ON u.id = s.user_id
         WHERE s.token_hash = ?
           AND s.expires_at > strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
           AND u.disabled = 0",
    )
    .bind(hash_token(token))
    .fetch_optional(pool)
    .await?;

    Ok(row.and_then(|(id, display_name, role)| {
        Some(SessionUser {
            id,
            display_name,
            role: Role::parse(&role)?,
        })
    }))
}

pub async fn delete(pool: &SqlitePool, token: &str) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM sessions WHERE token_hash = ?")
        .bind(hash_token(token))
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn purge_expired(pool: &SqlitePool) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM sessions WHERE expires_at <= strftime('%Y-%m-%dT%H:%M:%fZ', 'now')")
        .execute(pool)
        .await?;
    sqlx::query("DELETE FROM oidc_pending WHERE created_at <= strftime('%Y-%m-%dT%H:%M:%fZ', 'now', '-10 minutes')")
        .execute(pool)
        .await?;
    Ok(())
}

pub fn cookie(config: &Config, token: String) -> Cookie<'static> {
    Cookie::build((COOKIE_NAME, token))
        .path("/")
        .http_only(true)
        .secure(config.cookie_secure)
        .same_site(SameSite::Lax)
        .max_age(time::Duration::hours(config.session_hours))
        .build()
}

pub fn removal_cookie(config: &Config) -> Cookie<'static> {
    Cookie::build((COOKIE_NAME, ""))
        .path("/")
        .http_only(true)
        .secure(config.cookie_secure)
        .same_site(SameSite::Lax)
        .build()
}
