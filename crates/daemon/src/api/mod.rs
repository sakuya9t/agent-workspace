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
use crate::source_control::SourceControl;
use crate::util::now_millis;

mod auth;
mod fs;
mod scm;
mod ws;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Shared application state handed to every handler.
#[derive(Clone)]
pub struct AppState {
    pub manager: Arc<SessionManager>,
    pub config: Arc<Config>,
    pub scm: Arc<dyn SourceControl>,
    pub started_at: i64,
}

pub fn router(state: AppState) -> Router {
    let static_dir = state.config.static_dir.clone();

    let mut app = Router::new()
        .route("/health", get(health))
        .route("/api/auth/status", get(auth::status))
        .route("/api/auth/enroll", post(auth::enroll))
        .route("/api/auth/enrollment-token", get(auth::enrollment_token))
        .route("/api/auth/devices", get(auth::list_devices))
        .route("/api/auth/devices/:id/revoke", post(auth::revoke_device))
        .route("/api/fs/list", get(fs::list))
        .route("/api/plugins", get(list_plugins))
        .route("/api/workspaces", get(list_workspaces).post(add_workspace))
        .route("/api/workspaces/:id/init-git", post(init_workspace_git))
        .route("/api/workspaces/:id/branches", get(list_workspace_branches))
        .route("/api/sessions", get(list_sessions).post(create_session))
        .route("/api/sessions/:id", get(get_session))
        .route("/api/sessions/:id/summary", get(get_summary))
        .route("/api/sessions/:id/workspace", get(get_session_workspace))
        .route("/api/sessions/:id/stop", post(stop_session))
        .route("/api/sessions/:id/archive", post(archive_session))
        .route("/api/sessions/:id/cleanup", post(cleanup_instance))
        .route("/api/sessions/:id/resize", post(resize_session))
        .route("/api/sessions/:id/ack", post(ack_attention))
        .route("/api/sessions/:id/open-vscode", post(open_vscode))
        .route("/api/sessions/:id/stream", get(ws::stream))
        .route("/api/sessions/:id/scm/status", get(scm::status))
        .route("/api/sessions/:id/scm/diff", get(scm::diff))
        .route("/api/sessions/:id/scm/log", get(scm::log));

    // Optionally serve a packaged web client.
    if let Some(dir) = static_dir {
        if dir.is_dir() {
            app = app.fallback_service(ServeDir::new(dir));
        }
    }

    // Auth runs inside CORS so preflight is handled before token checks.
    app.layer(axum::middleware::from_fn_with_state(
        state.clone(),
        crate::auth::require_auth,
    ))
    .layer(CorsLayer::permissive())
    .with_state(state)
}

// ---------- handlers ----------

async fn health(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(json!({
        "status": "ok",
        "version": VERSION,
        "hostname": hostname(),
        "platform": current_platform(),
        "uptime_ms": now_millis() - state.started_at,
        "database": "ok",
        "backend": state.manager.backend_id(),
        "active_sessions": state.manager.live_count(),
    }))
}

/// Best-effort host name for the pool/host node in the client.
fn hostname() -> String {
    std::env::var("HOSTNAME")
        .ok()
        .or_else(|| std::env::var("COMPUTERNAME").ok())
        .or_else(|| {
            std::fs::read_to_string("/etc/hostname")
                .ok()
                .map(|s| s.trim().to_string())
        })
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "local".to_string())
}

async fn list_plugins(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(json!({ "plugins": state.manager.registry.describe() }))
}

async fn list_workspaces(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, AppError> {
    Ok(Json(json!({ "workspaces": state.manager.list_workspaces()? })))
}

#[derive(Debug, Deserialize)]
struct AddWorkspaceBody {
    name: String,
    root_path: String,
}

async fn add_workspace(
    State(state): State<AppState>,
    Json(body): Json<AddWorkspaceBody>,
) -> Result<Json<serde_json::Value>, AppError> {
    let w = state.manager.register_workspace(body.name, body.root_path)?;
    Ok(Json(json!({ "workspace": w })))
}

async fn init_workspace_git(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let w = state.manager.init_workspace_git(&id)?;
    Ok(Json(json!({ "workspace": w })))
}

async fn list_workspace_branches(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let (branches, head) = state.manager.list_workspace_branches(&id)?;
    Ok(Json(json!({ "branches": branches, "head": head })))
}

async fn get_session_workspace(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let instance = state.manager.get_instance_for_session(&id)?;
    Ok(Json(json!({ "instance": instance })))
}

#[derive(Debug, Deserialize)]
struct CleanupParams {
    #[serde(default)]
    force: bool,
}

async fn cleanup_instance(
    State(state): State<AppState>,
    Path(id): Path<String>,
    axum::extract::Query(params): axum::extract::Query<CleanupParams>,
) -> Result<Json<serde_json::Value>, AppError> {
    state.manager.cleanup_instance(&id, params.force)?;
    Ok(Json(json!({ "ok": true })))
}

async fn list_sessions(State(state): State<AppState>) -> Result<Json<serde_json::Value>, AppError> {
    let sessions = state.manager.list_sessions()?;
    Ok(Json(json!({ "sessions": sessions })))
}

#[derive(Debug, Deserialize)]
struct CreateSessionBody {
    agent_plugin_id: String,
    /// Required unless `workspace_id` is provided (then the instance path is used).
    #[serde(default)]
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
    #[serde(default)]
    direct_checkout: bool,
    #[serde(default)]
    branch: Option<String>,
    #[serde(default)]
    create_branch: bool,
    #[serde(default)]
    base_ref: Option<String>,
    #[serde(default)]
    options: HashMap<String, bool>,
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
        direct_checkout: body.direct_checkout,
        branch: body.branch,
        create_branch: body.create_branch,
        base_ref: body.base_ref,
        options: body.options.into_iter().collect(),
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

/// Open the session's isolated workspace instance in VS Code. Opening the
/// editor does not touch the running agent session; the working directory is
/// already the isolated instance (worktree) for isolated sessions.
async fn open_vscode(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let session = state
        .manager
        .get_session(&id)?
        .ok_or_else(|| AppError(StatusCode::NOT_FOUND, "no such session".into()))?;
    let target = session.working_directory;

    let code = crate::plugins::find_in_path("code").ok_or_else(|| {
        AppError(
            StatusCode::BAD_REQUEST,
            "VS Code CLI `code` not found in PATH on the daemon host. For a remote daemon, \
             use VS Code Remote-SSH to open this path."
                .into(),
        )
    })?;

    std::process::Command::new(code)
        .arg(&target)
        .spawn()
        .map_err(|e| {
            AppError(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed to launch VS Code: {e}"),
            )
        })?;

    Ok(Json(json!({ "opened": true, "path": target })))
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
