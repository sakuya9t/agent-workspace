use std::net::SocketAddr;

use axum::extract::{ConnectInfo, Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use super::{AppError, AppState};
use crate::auth::gen_device_token;
use crate::domain::{Device, DeviceInfo};
use crate::util::now_millis;

/// Public: lets a client discover the server identity before it has a token.
pub async fn status(State(state): State<AppState>) -> Result<Json<serde_json::Value>, AppError> {
    let (server_id, _) = state.manager.db.identity()?;
    Ok(Json(json!({
        "server_id": server_id,
        // Loopback (incl. SSH tunnels) is trusted; remote needs a device token.
        "auth": "loopback-open",
        "requires_token_for_remote": true,
    })))
}

#[derive(Debug, Deserialize)]
pub struct EnrollBody {
    enrollment_token: String,
    #[serde(default)]
    device_name: Option<String>,
}

/// Public bootstrap: exchange the shared enrollment token for a device token.
pub async fn enroll(
    State(state): State<AppState>,
    Json(body): Json<EnrollBody>,
) -> Result<Json<serde_json::Value>, AppError> {
    let (server_id, enrollment_token) = state.manager.db.identity()?;
    if body.enrollment_token.trim() != enrollment_token {
        return Err(AppError(
            StatusCode::UNAUTHORIZED,
            "invalid enrollment token".into(),
        ));
    }

    let now = now_millis();
    let device = Device {
        id: Uuid::new_v4().to_string(),
        name: body
            .device_name
            .filter(|n| !n.trim().is_empty())
            .unwrap_or_else(|| "device".to_string()),
        token: gen_device_token(),
        created_at: now,
        last_seen_at: now,
        revoked: false,
    };
    state.manager.db.insert_device(&device)?;

    Ok(Json(json!({
        "server_id": server_id,
        "device_id": device.id,
        "device_token": device.token,
        "device_name": device.name,
    })))
}

/// Loopback-only: reveal the enrollment token so the owner (at the host, or
/// over an SSH tunnel) can enroll another device. Never exposed to remote peers.
pub async fn enrollment_token(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
) -> Result<Json<serde_json::Value>, AppError> {
    if !peer.ip().is_loopback() {
        return Err(AppError(
            StatusCode::FORBIDDEN,
            "enrollment token is only visible from the daemon host (loopback)".into(),
        ));
    }
    let (_, token) = state.manager.db.identity()?;
    Ok(Json(json!({ "enrollment_token": token })))
}

/// Authenticated: list enrolled devices (no tokens).
pub async fn list_devices(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, AppError> {
    let devices: Vec<DeviceInfo> = state
        .manager
        .db
        .list_devices()?
        .iter()
        .map(DeviceInfo::from)
        .collect();
    Ok(Json(json!({ "devices": devices })))
}

pub async fn revoke_device(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let ok = state.manager.db.revoke_device(&id)?;
    if !ok {
        return Err(AppError(StatusCode::NOT_FOUND, "no such device".into()));
    }
    Ok(Json(json!({ "revoked": true })))
}
