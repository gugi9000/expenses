use std::net::SocketAddr;

use axum::{
    extract::{Request, State},
    http::{HeaderMap, Method, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};

use super::state::AppState;

/// Rejects cross-site state-changing requests (CSRF defence on top of `SameSite=Lax`).
pub async fn require_same_origin(State(state): State<AppState>, req: Request, next: Next) -> Response {
    if matches!(*req.method(), Method::GET | Method::HEAD | Method::OPTIONS) {
        return next.run(req).await;
    }
    if is_same_origin(req.headers(), &state.config.base_url) {
        next.run(req).await
    } else {
        tracing::warn!(method = %req.method(), uri = %req.uri(), "rejected cross-origin request");
        (StatusCode::FORBIDDEN, "Forbidden").into_response()
    }
}

fn is_same_origin(headers: &HeaderMap, base_url: &str) -> bool {
    match headers.get(header::ORIGIN) {
        Some(origin) => origin.to_str().is_ok_and(|o| o == base_url),
        // Older clients omit Origin; fall back to Fetch Metadata when present.
        None => headers
            .get("sec-fetch-site")
            .and_then(|v| v.to_str().ok())
            .is_none_or(|v| v == "same-origin" || v == "none"),
    }
}

pub fn client_ip(headers: &HeaderMap, peer: Option<SocketAddr>, trust_proxy: bool) -> Option<String> {
    if trust_proxy {
        // The proxy appends the address it saw, so the last entry is the trustworthy one.
        if let Some(ip) = headers
            .get("x-forwarded-for")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.rsplit(',').next())
            .map(str::trim)
            .filter(|v| !v.is_empty())
        {
            return Some(ip.to_string());
        }
    }
    peer.map(|p| p.ip().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn origin_check() {
        let base = "https://udgifter.example.dk";
        let mut h = HeaderMap::new();
        h.insert(header::ORIGIN, HeaderValue::from_static("https://udgifter.example.dk"));
        assert!(is_same_origin(&h, base));

        h.insert(header::ORIGIN, HeaderValue::from_static("https://evil.example"));
        assert!(!is_same_origin(&h, base));

        let mut h = HeaderMap::new();
        h.insert("sec-fetch-site", HeaderValue::from_static("cross-site"));
        assert!(!is_same_origin(&h, base));
        assert!(is_same_origin(&HeaderMap::new(), base));
    }

    #[test]
    fn forwarded_ip_only_when_trusted() {
        let mut h = HeaderMap::new();
        h.insert("x-forwarded-for", HeaderValue::from_static("1.2.3.4, 10.0.0.1"));
        let peer: SocketAddr = "127.0.0.1:5000".parse().unwrap();
        assert_eq!(client_ip(&h, Some(peer), true).as_deref(), Some("10.0.0.1"));
        assert_eq!(client_ip(&h, Some(peer), false).as_deref(), Some("127.0.0.1"));
    }
}
