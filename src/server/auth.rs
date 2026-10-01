use std::{net::SocketAddr, sync::LazyLock};

use argon2::{
    Argon2, PasswordHash, PasswordHasher, PasswordVerifier,
    password_hash::{SaltString, rand_core::OsRng},
};
use axum::{
    Form, Router,
    extract::{ConnectInfo, FromRequestParts, Query, State},
    http::{HeaderMap, StatusCode, request::Parts},
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
};
use axum_extra::extract::CookieJar;
use serde::Deserialize;
use serde_json::json;
use sqlx::SqlitePool;

use super::{audit, security::client_ip, session, state::AppState};
use crate::model::{Role, SessionUser};

const MAX_FAILED_PER_USER: i64 = 5;
const MAX_FAILED_PER_IP: i64 = 20;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/auth/login", post(local_login))
        .route("/auth/logout", post(logout))
        .route("/auth/entra/login", get(entra_login))
        .route("/auth/entra/callback", get(entra_callback))
}

pub fn hash_password(password: &str) -> Result<String, argon2::password_hash::Error> {
    Ok(Argon2::default()
        .hash_password(password.as_bytes(), &SaltString::generate(&mut OsRng))?
        .to_string())
}

fn verify_password(password: &str, hash: &str) -> bool {
    PasswordHash::new(hash).is_ok_and(|h| {
        Argon2::default()
            .verify_password(password.as_bytes(), &h)
            .is_ok()
    })
}

/// Verified against when the username does not exist, so response time does not reveal valid usernames.
static DUMMY_HASH: LazyLock<String> =
    LazyLock::new(|| hash_password("dummy-password-for-timing").expect("argon2 hashing works"));

#[derive(Deserialize)]
struct LoginForm {
    username: String,
    password: String,
}

fn login_error(code: &str) -> Response {
    Redirect::to(&format!("/login?fejl={code}")).into_response()
}

async fn recent_failures(pool: &SqlitePool, column: &str, value: &str) -> sqlx::Result<i64> {
    let sql = format!(
        "SELECT COUNT(*) FROM audit_log
         WHERE action = 'login_failed' AND {column} = ?
           AND at > strftime('%Y-%m-%dT%H:%M:%fZ', 'now', '-15 minutes')"
    );
    sqlx::query_scalar(&sql).bind(value).fetch_one(pool).await
}

async fn local_login(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    jar: CookieJar,
    Form(form): Form<LoginForm>,
) -> Result<Response, AppError> {
    let pool = &state.pool;
    let ip = client_ip(&headers, Some(peer), state.config.trust_proxy);
    let username = form.username.trim().to_lowercase();

    if recent_failures(pool, "entity_id", &username).await? >= MAX_FAILED_PER_USER
        || recent_failures(pool, "ip", ip.as_deref().unwrap_or("")).await? >= MAX_FAILED_PER_IP
    {
        return Ok(login_error("spaerret"));
    }

    let user: Option<(i64, String, bool)> = sqlx::query_as(
        "SELECT id, password_hash, disabled FROM users WHERE auth_provider = 'local' AND username = ?",
    )
    .bind(&username)
    .fetch_optional(pool)
    .await?;

    let password = form.password;
    let hash = user
        .as_ref()
        .map_or_else(|| DUMMY_HASH.clone(), |(_, h, _)| h.clone());
    let valid = tokio::task::spawn_blocking(move || verify_password(&password, &hash)).await?
        && user.is_some();

    let Some((user_id, _, disabled)) = user.filter(|_| valid) else {
        audit::record(
            pool,
            None,
            "login_failed",
            Some(("user", &username)),
            Some(json!({"provider": "local"})),
            ip.as_deref(),
        )
        .await?;
        return Ok(login_error("login"));
    };
    if disabled {
        audit::record(
            pool,
            Some(user_id),
            "login_denied_disabled",
            Some(("user", &user_id.to_string())),
            None,
            ip.as_deref(),
        )
        .await?;
        return Ok(login_error("deaktiveret"));
    }

    Ok(start_session(
        &state,
        jar,
        user_id,
        json!({"provider": "local"}),
        ip.as_deref(),
    )
    .await?
    .into_response())
}

async fn start_session(
    state: &AppState,
    jar: CookieJar,
    user_id: i64,
    details: serde_json::Value,
    ip: Option<&str>,
) -> Result<(CookieJar, Redirect), AppError> {
    session::purge_expired(&state.pool).await?;
    let token = session::create(&state.pool, user_id, state.config.session_hours).await?;
    sqlx::query(
        "UPDATE users SET last_login_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = ?",
    )
    .bind(user_id)
    .execute(&state.pool)
    .await?;
    audit::record(
        &state.pool,
        Some(user_id),
        "login",
        Some(("user", &user_id.to_string())),
        Some(details),
        ip,
    )
    .await?;
    Ok((
        jar.add(session::cookie(&state.config, token)),
        Redirect::to("/"),
    ))
}

async fn logout(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    jar: CookieJar,
) -> Result<Response, AppError> {
    if let Some(cookie) = jar.get(session::COOKIE_NAME) {
        let token = cookie.value().to_string();
        if let Some(user) = session::lookup(&state.pool, &token).await? {
            let ip = client_ip(&headers, Some(peer), state.config.trust_proxy);
            audit::record(
                &state.pool,
                Some(user.id),
                "logout",
                Some(("user", &user.id.to_string())),
                None,
                ip.as_deref(),
            )
            .await?;
        }
        session::delete(&state.pool, &token).await?;
    }
    Ok((
        jar.remove(session::removal_cookie(&state.config)),
        Redirect::to("/login"),
    )
        .into_response())
}

async fn entra_login(State(state): State<AppState>) -> Result<Response, AppError> {
    let Some(entra) = &state.entra else {
        return Ok(StatusCode::NOT_FOUND.into_response());
    };
    let start = match entra.start().await {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("entra login start failed: {e:#}");
            return Ok(login_error("microsoft"));
        }
    };
    sqlx::query("INSERT INTO oidc_pending (state, nonce, pkce_verifier) VALUES (?, ?, ?)")
        .bind(&start.state)
        .bind(&start.nonce)
        .bind(&start.pkce_verifier)
        .execute(&state.pool)
        .await?;
    Ok(Redirect::to(&start.url).into_response())
}

#[derive(Deserialize)]
struct CallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}

async fn entra_callback(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    jar: CookieJar,
    Query(q): Query<CallbackQuery>,
) -> Result<Response, AppError> {
    let Some(entra) = &state.entra else {
        return Ok(StatusCode::NOT_FOUND.into_response());
    };
    let ip = client_ip(&headers, Some(peer), state.config.trust_proxy);
    let (pool, ip_ref) = (&state.pool, ip.as_deref());
    let fail = move |reason: String| async move {
        tracing::warn!("entra login failed: {reason}");
        audit::record(
            pool,
            None,
            "login_failed",
            None,
            Some(json!({"provider": "entra", "reason": reason})),
            ip_ref,
        )
        .await?;
        Ok::<_, AppError>(login_error("microsoft"))
    };

    let (Some(code), Some(oidc_state)) = (q.code, q.state) else {
        return fail(format!("callback without code: {:?}", q.error)).await;
    };

    // Single use: the row is consumed whether or not the rest succeeds.
    let pending: Option<(String, String)> = sqlx::query_as(
        "DELETE FROM oidc_pending
         WHERE state = ? AND created_at > strftime('%Y-%m-%dT%H:%M:%fZ', 'now', '-10 minutes')
         RETURNING nonce, pkce_verifier",
    )
    .bind(&oidc_state)
    .fetch_optional(&state.pool)
    .await?;
    let Some((nonce, pkce_verifier)) = pending else {
        return fail("unknown or expired state".into()).await;
    };

    let identity = match entra.finish(code, nonce, pkce_verifier).await {
        Ok(i) => i,
        Err(e) => return fail(format!("{e:#}")).await,
    };
    let role = if identity.is_admin {
        Role::Admin
    } else {
        Role::User
    };

    let (user_id, disabled): (i64, bool) = sqlx::query_as(
        "INSERT INTO users (auth_provider, entra_oid, username, display_name, email, role)
         VALUES ('entra', ?, ?, ?, ?, ?)
         ON CONFLICT (entra_oid) DO UPDATE SET
             username = excluded.username,
             display_name = excluded.display_name,
             email = excluded.email,
             role = excluded.role
         RETURNING id, disabled",
    )
    .bind(&identity.oid)
    .bind(&identity.username)
    .bind(&identity.display_name)
    .bind(&identity.email)
    .bind(role.as_str())
    .fetch_one(&state.pool)
    .await?;

    if disabled {
        audit::record(
            &state.pool,
            Some(user_id),
            "login_denied_disabled",
            Some(("user", &user_id.to_string())),
            None,
            ip.as_deref(),
        )
        .await?;
        return Ok(login_error("deaktiveret"));
    }
    let details = json!({"provider": "entra", "role": role.as_str()});
    Ok(start_session(&state, jar, user_id, details, ip.as_deref())
        .await?
        .into_response())
}

/// Axum extractor for routes that require a logged-in user.
pub struct CurrentUser(pub SessionUser);

impl FromRequestParts<AppState> for CurrentUser {
    type Rejection = StatusCode;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let jar = CookieJar::from_headers(&parts.headers);
        let token = jar
            .get(session::COOKIE_NAME)
            .ok_or(StatusCode::UNAUTHORIZED)?;
        session::lookup(&state.pool, token.value())
            .await
            .map_err(|e| {
                tracing::error!("session lookup failed: {e}");
                StatusCode::INTERNAL_SERVER_ERROR
            })?
            .map(CurrentUser)
            .ok_or(StatusCode::UNAUTHORIZED)
    }
}

/// Resolves the logged-in user inside a Leptos server function.
pub async fn session_user() -> Result<Option<SessionUser>, leptos::prelude::ServerFnError> {
    use leptos::prelude::*;
    let state = app_state()?;
    let jar: CookieJar = leptos_axum::extract().await?;
    let Some(token) = jar.get(session::COOKIE_NAME) else {
        return Ok(None);
    };
    session::lookup(&state.pool, token.value())
        .await
        .map_err(|e| {
            tracing::error!("session lookup failed: {e}");
            ServerFnError::new("internal error")
        })
}

/// Everything a server function needs to act on behalf of the logged-in user.
pub struct RequestCtx {
    pub state: AppState,
    pub user: SessionUser,
    pub ip: Option<String>,
}

/// Errors instead of panicking when a render path forgot to provide the state.
fn app_state() -> Result<AppState, leptos::prelude::ServerFnError> {
    leptos::prelude::use_context::<AppState>().ok_or_else(|| {
        tracing::error!("AppState missing from Leptos context");
        leptos::prelude::ServerFnError::new(crate::i18n::t::GENERIC_ERROR)
    })
}

pub async fn require_user() -> Result<RequestCtx, leptos::prelude::ServerFnError> {
    use leptos::prelude::*;
    let user = session_user()
        .await?
        .ok_or_else(|| ServerFnError::new(crate::i18n::t::ERR_NOT_LOGGED_IN))?;
    let state = app_state()?;
    let headers: HeaderMap = leptos_axum::extract().await?;
    let peer = leptos_axum::extract::<ConnectInfo<SocketAddr>>()
        .await
        .ok()
        .map(|c| c.0);
    let ip = client_ip(&headers, peer, state.config.trust_proxy);
    Ok(RequestCtx { state, user, ip })
}

pub async fn require_admin() -> Result<RequestCtx, leptos::prelude::ServerFnError> {
    let ctx = require_user().await?;
    if !ctx.user.is_admin() {
        tracing::warn!(
            user_id = ctx.user.id,
            "non-admin called an admin server function"
        );
        return Err(leptos::prelude::ServerFnError::new(
            crate::i18n::t::ERR_FORBIDDEN,
        ));
    }
    Ok(ctx)
}

pub struct AppError(anyhow::Error);

impl<E: Into<anyhow::Error>> From<E> for AppError {
    fn from(e: E) -> Self {
        Self(e.into())
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        tracing::error!("request failed: {:#}", self.0);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            crate::i18n::t::GENERIC_ERROR,
        )
            .into_response()
    }
}
