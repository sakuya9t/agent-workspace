use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;

use crate::config::Config;
use crate::plugins::current_platform;
use crate::session_manager::{CreateSessionRequest, SessionManager};
use crate::util::now_millis;

mod ws;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Shared application state handed to every handler.
#[derive(Clone)]
pub struct AppState {
    pub manager: Arc<SessionManager>,
    pub config: Arc<Config>,
    pub started_at: i64,
}

pub fn router(state: AppState) -> Router {
    let static_dir = state.config.static_dir.clone();

    let mut app = Router::new()
        .route("/health", get(health))
        .route("/api/plugins", get(list_plugins))
        .route("/api/sessions", get(list_sessions).post(create_session))
        .route("/api/sessions/:id", get(get_session))
        .route("/api/sessions/:id/summary", get(get_summary))
        .route("/api/sessions/:id/stop", post(stop_session))
        .route("/api/sessions/:id/archive", post(archive_session))
        .route("/api/sessions/:id/resize", post(resize_session))
        .route("/api/sessions/:id/ack", post(ack_attention))
        .route("/api/sessions/:id/stream", get(ws::stream));

    // Optionally serve a packaged web client.
    if let Some(dir) = static_dir {
        if dir.is_dir() {
            app = app.fallback_service(ServeDir::new(dir));
        }
    }

    app.layer(CorsLayer::permissive()).with_state(state)
}

// ---------- handlers ----------

async fn health(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(json!({
        "status": "ok",
        "version": VERSION,
        "platform": current_platform(),
        "uptime_ms": now_millis() - state.started_at,
        "database": "ok",
        "backend": state.manager.backend_id(),
        "active_sessions": state.manager.live_count(),
    }))
}

async fn list_plugins(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(json!({ "plugins": state.manager.registry.describe() }))
}

async fn list_sessions(State(state): State<AppState>) -> Result<Json<serde_json::Value>, AppError> {
    let sessions = state.manager.list_sessions()?;
    Ok(Json(json!({ "sessions": sessions })))
}

#[derive(Debug, Deserialize)]
struct CreateSessionBody {
    agent_plugin_id: String,
    cwd: String,
    #[serde(default)]
    command: Option<String>,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    env: HashMap<String, String>,
    #[serde(default)]
    rows: Option<u16>,
    #[serde(default)]
    cols: Option<u16>,
    #[serde(default)]
    workspace_id: Option<String>,
    #[serde(default)]
    approve_custom: bool,
}

async fn create_session(
    State(state): State<AppState>,
    Json(body): Json<CreateSessionBody>,
) -> Result<Json<serde_json::Value>, AppError> {
    let req = CreateSessionRequest {
        agent_plugin_id: body.agent_plugin_id,
        cwd: body.cwd,
        command: body.command,
        args: body.args,
        env: body.env.into_iter().collect(),
        rows: body.rows.unwrap_or(24),
        cols: body.cols.unwrap_or(80),
        workspace_id: body.workspace_id,
        approve_custom: body.approve_custom,
    };
    let session = state.manager.create_session(req)?;
    Ok(Json(json!({ "session": session })))
}

async fn get_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    match state.manager.get_session(&id)? {
        Some(s) => Ok(Json(json!({ "session": s }))),
        None => Err(AppError(StatusCode::NOT_FOUND, "no such session".into())),
    }
}

async fn get_summary(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    match state.manager.get_summary(&id)? {
        Some(s) => Ok(Json(json!({ "summary": s }))),
        None => Err(AppError(StatusCode::NOT_FOUND, "no summary yet".into())),
    }
}

async fn stop_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let s = state.manager.stop_session(&id)?;
    Ok(Json(json!({ "session": s })))
}

async fn archive_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let s = state.manager.archive_session(&id)?;
    Ok(Json(json!({ "session": s })))
}

#[derive(Debug, Deserialize)]
struct ResizeBody {
    rows: u16,
    cols: u16,
}

async fn resize_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<ResizeBody>,
) -> Result<Json<serde_json::Value>, AppError> {
    state.manager.resize_session(&id, body.rows, body.cols)?;
    Ok(Json(json!({ "ok": true })))
}

async fn ack_attention(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let s = state.manager.acknowledge_attention(&id)?;
    Ok(Json(json!({ "session": s })))
}

// ---------- error type ----------

pub struct AppError(StatusCode, String);

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({ "error": self.1 }))).into_response()
    }
}

impl From<anyhow::Error> for AppError {
    fn from(e: anyhow::Error) -> Self {
        AppError(StatusCode::BAD_REQUEST, format!("{e:#}"))
    }
}
