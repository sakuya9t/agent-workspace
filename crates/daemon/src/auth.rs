use std::net::SocketAddr;

use axum::extract::{ConnectInfo, Request, State};
use axum::http::{Method, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;
use uuid::Uuid;

use crate::api::AppState;
use crate::util::now_millis;

/// A 256-bit opaque bearer token (two v4 UUIDs of CSPRNG entropy).
pub fn gen_device_token() -> String {
    format!(
        "{}{}",
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple()
    )
}

/// A shorter, human-typeable enrollment secret (128-bit).
pub fn gen_enrollment_token() -> String {
    Uuid::new_v4().simple().to_string()
}

pub fn gen_server_id() -> String {
    Uuid::new_v4().to_string()
}

/// Auth policy:
/// - `/health`, static assets, and the auth bootstrap endpoints are public.
/// - Loopback connections (localhost, and SSH local port-forwards, which
///   terminate on loopback of the remote host) are trusted without a token.
/// - Every other connection must present a valid device bearer token
///   (Authorization: Bearer <token>, or `?access_token=` for WebSocket).
pub async fn require_auth(State(state): State<AppState>, req: Request, next: Next) -> Response {
    // CORS preflight must pass through untouched.
    if req.method() == Method::OPTIONS {
        return next.run(req).await;
    }

    let path = req.uri().path();
    if is_public(path) {
        return next.run(req).await;
    }

    if peer_is_loopback(&req) {
        return next.run(req).await;
    }

    if let Some(token) = extract_token(&req) {
        if let Ok(Some(dev)) = state.manager.db.device_by_token(&token) {
            let _ = state.manager.db.touch_device(&dev.id, now_millis());
            return next.run(req).await;
        }
    }

    (
        StatusCode::UNAUTHORIZED,
        Json(json!({ "error": "unauthorized", "hint": "enroll this device to obtain a token" })),
    )
        .into_response()
}

fn is_public(path: &str) -> bool {
    !path.starts_with("/api")
        || path == "/api/auth/status"
        || path == "/api/auth/enroll"
}

fn peer_is_loopback(req: &Request) -> bool {
    req.extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ci| ci.0.ip().is_loopback())
        .unwrap_or(false)
}

fn extract_token(req: &Request) -> Option<String> {
    if let Some(h) = req.headers().get("authorization") {
        if let Ok(s) = h.to_str() {
            if let Some(rest) = s.strip_prefix("Bearer ") {
                return Some(rest.trim().to_string());
            }
        }
    }
    // WebSocket clients (browsers) cannot set headers; accept a query param.
    if let Some(q) = req.uri().query() {
        for pair in q.split('&') {
            if let Some(v) = pair.strip_prefix("access_token=") {
                return Some(v.to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_paths() {
        assert!(is_public("/health"));
        assert!(is_public("/"));
        assert!(is_public("/assets/index.js"));
        assert!(is_public("/api/auth/status"));
        assert!(is_public("/api/auth/enroll"));
        assert!(!is_public("/api/sessions"));
        assert!(!is_public("/api/auth/devices"));
        assert!(!is_public("/api/sessions/abc/stream"));
    }

    #[test]
    fn tokens_are_unique_and_sized() {
        let a = gen_device_token();
        let b = gen_device_token();
        assert_ne!(a, b);
        assert_eq!(a.len(), 64); // two 32-char simple UUIDs
        assert_eq!(gen_enrollment_token().len(), 32);
    }
}
