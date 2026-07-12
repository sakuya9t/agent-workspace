use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
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
mod paste;
mod scm;
pub mod ws;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Shared application state handed to every handler.
#[derive(Clone)]
pub struct AppState {
    pub manager: Arc<SessionManager>,
    pub config: Arc<Config>,
    pub scm: Arc<dyn SourceControl>,
    pub started_at: i64,
    /// This daemon's stable node id (== persisted `server_id`); advertised to
    /// the relay and surfaced on `/health` so a gateway can probe it (R4).
    pub node_id: String,
    /// Human label for this node (`ASM_NODE_LABEL`, default hostname).
    pub node_label: String,
    /// Tracks the single live WS attacher per session (takeover).
    pub attachments: Arc<ws::Attachments>,
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
        .route("/api/workspaces/:id", delete(remove_workspace))
        .route("/api/workspaces/:id/init-git", post(init_workspace_git))
        .route(
            "/api/workspaces/:id/cleanup-worktrees",
            post(cleanup_workspace_worktrees),
        )
        .route("/api/workspaces/:id/branches", get(list_workspace_branches))
        .route("/api/sessions", get(list_sessions).post(create_session))
        .route("/api/sessions/:id", get(get_session))
        .route("/api/sessions/:id/summary", get(get_summary))
        .route("/api/sessions/:id/transcript", get(get_transcript))
        .route("/api/sessions/:id/usage", get(get_session_usage))
        .route("/api/sessions/:id/workspace", get(get_session_workspace))
        .route("/api/sessions/:id/stop", post(stop_session))
        .route("/api/sessions/:id/archive", post(archive_session))
        .route("/api/sessions/:id/cleanup", post(cleanup_instance))
        .route("/api/sessions/:id/resize", post(resize_session))
        .route(
            "/api/sessions/:id/paste",
            // Raw image body; raise the transport limit above the enforced
            // `MAX_PASTE_BYTES` so an oversize upload gets a clean 413 from the
            // handler rather than a truncated-body error from the extractor.
            post(paste::upload)
                .layer(axum::extract::DefaultBodyLimit::max(paste::MAX_PASTE_BYTES + 512 * 1024)),
        )
        .route("/api/sessions/:id/ack", post(ack_attention))
        .route("/api/sessions/:id/vscode-target", get(vscode_target))
        .route("/api/sessions/:id/stream", get(ws::stream))
        .route("/api/sessions/:id/scm/status", get(scm::status))
        .route("/api/sessions/:id/scm/diff", get(scm::diff))
        .route("/api/sessions/:id/scm/file", get(scm::file))
        .route("/api/sessions/:id/scm/log", get(scm::log))
        .route("/api/sessions/:id/scm/commit", get(scm::commit))
        .route("/api/sessions/:id/scm/branches", get(scm::branches))
        .route("/api/sessions/:id/scm/pull", post(scm::pull))
        .route("/api/sessions/:id/scm/push", post(scm::push))
        .route("/api/sessions/:id/scm/rebase", post(scm::rebase))
        .route("/api/sessions/:id/scm/merge", post(scm::merge));

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
        "node_id": state.node_id,
        "label": state.node_label,
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
    Json(json!({ "plugins": state.manager.registry().describe() }))
}

async fn list_workspaces(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, AppError> {
    // Augment each workspace with a live `root_exists` check so the client can
    // flag a workspace whose directory was deleted on the host.
    let workspaces: Vec<serde_json::Value> = state
        .manager
        .list_workspaces()?
        .iter()
        .map(|w| {
            let mut v = serde_json::to_value(w).unwrap_or_else(|_| json!({}));
            if let Some(obj) = v.as_object_mut() {
                obj.insert(
                    "root_exists".into(),
                    json!(std::path::Path::new(&w.root_path).is_dir()),
                );
            }
            v
        })
        .collect();
    Ok(Json(json!({ "workspaces": workspaces })))
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

async fn remove_workspace(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    state.manager.remove_workspace(&id)?;
    Ok(Json(json!({ "ok": true })))
}

async fn init_workspace_git(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let w = state.manager.init_workspace_git(&id)?;
    Ok(Json(json!({ "workspace": w })))
}

async fn cleanup_workspace_worktrees(
    State(state): State<AppState>,
    Path(id): Path<String>,
    axum::extract::Query(params): axum::extract::Query<CleanupParams>,
) -> Result<Json<serde_json::Value>, AppError> {
    let report = state.manager.cleanup_orphan_worktrees(&id, params.force)?;
    Ok(Json(json!({ "report": report })))
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
    // Augment each session with `attached` (is a live client currently on it?)
    // so the UI can prompt for takeover instead of silently stealing it.
    let sessions: Vec<serde_json::Value> = state
        .manager
        .list_sessions()?
        .iter()
        .map(|s| with_attached(s, &state))
        .collect();
    Ok(Json(json!({ "sessions": sessions })))
}

/// Serialize a session and add the runtime `attached` flag.
fn with_attached(s: &crate::domain::Session, state: &AppState) -> serde_json::Value {
    let mut v = serde_json::to_value(s).unwrap_or_else(|_| json!({}));
    if let Some(obj) = v.as_object_mut() {
        obj.insert("attached".into(), json!(state.attachments.is_attached(&s.id)));
    }
    v
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
        Some(s) => Ok(Json(json!({ "session": with_attached(&s, &state) }))),
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

/// Download a session's full conversation as a raw terminal transcript: the
/// complete recorded PTY byte stream (ANSI included), the same bytes replayed on
/// history attach. There is no delta — every call returns everything persisted
/// so far (for a live session, the transcript up to now). Archived sessions have
/// been discarded, so their transcript is no longer offered.
async fn get_transcript(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Response, AppError> {
    let s = state
        .manager
        .get_session(&id)?
        .ok_or_else(|| AppError(StatusCode::NOT_FOUND, "no such session".into()))?;
    if s.status == crate::domain::SessionStatus::Archived {
        return Err(AppError(
            StatusCode::CONFLICT,
            "transcript unavailable for an archived session".into(),
        ));
    }
    let bytes = state.manager.db().read_events_after(&id, 0)?;
    let filename = transcript_filename(&s);
    Ok((
        [
            (header::CONTENT_TYPE, "text/plain; charset=utf-8".to_string()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{filename}\""),
            ),
            (header::CACHE_CONTROL, "no-store".to_string()),
        ],
        bytes,
    )
        .into_response())
}

/// A safe download filename for a session's transcript. The session id is a
/// UUID, but the agent plugin id is free-form, so fold anything outside
/// `[A-Za-z0-9._-]` to `_` — this both tidies the name and keeps stray bytes
/// out of the `Content-Disposition` header.
fn transcript_filename(s: &crate::domain::Session) -> String {
    let agent: String = s
        .agent_plugin_id
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') { c } else { '_' })
        .collect();
    format!("session-{agent}-{}.log", s.id)
}

/// Best-effort token/context usage for a session, read from the agent's own
/// on-disk transcript (Claude Code / Codex). Returns `available: false` for
/// agents that don't record usage.
async fn get_session_usage(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let s = state
        .manager
        .get_session(&id)?
        .ok_or_else(|| AppError(StatusCode::NOT_FOUND, "no such session".into()))?;
    // File reads plus a possible (cached) rate-limit HTTP fetch — keep them off
    // the async runtime.
    let manager = state.manager.clone();
    let usage = tokio::task::spawn_blocking(move || {
        manager.registry().get(&s.agent_plugin_id).and_then(|p| {
            p.usage(&crate::plugins::usage::UsageContext {
                cwd: std::path::PathBuf::from(&s.working_directory),
                started_at_ms: s.created_at,
            })
        })
    })
    .await
    .map_err(|e| AppError(StatusCode::INTERNAL_SERVER_ERROR, format!("usage task: {e}")))?
    .unwrap_or_else(|| crate::plugins::usage::AgentUsage {
        note: Some("No usage data available for this agent/session.".into()),
        ..Default::default()
    });
    Ok(Json(json!({ "usage": usage })))
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
    axum::extract::Query(params): axum::extract::Query<CleanupParams>,
) -> Result<Json<serde_json::Value>, AppError> {
    match state.manager.archive_session(&id, params.force) {
        Ok(s) => Ok(Json(json!({ "session": s }))),
        // Archiving would discard uncommitted/unmerged work: 409 so the client
        // can confirm and retry with `?force=true`.
        Err(e) if e.downcast_ref::<crate::session_manager::NeedsForce>().is_some() => {
            Err(AppError(StatusCode::CONFLICT, format!("{e:#}")))
        }
        Err(e) => Err(e.into()),
    }
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

/// Describe where the *client's* VS Code should connect to reach this
/// session's workspace. The daemon never launches an editor itself — the web
/// client turns this into a `vscode://` deep link (local folder when the
/// daemon is on the browser's machine, Remote-SSH otherwise). The path is
/// already the isolated instance (worktree) for isolated sessions.
async fn vscode_target(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let session = state
        .manager
        .get_session(&id)?
        .ok_or_else(|| AppError(StatusCode::NOT_FOUND, "no such session".into()))?;

    Ok(Json(json!({
        "path": session.working_directory,
        "ssh_user": daemon_user(),
        "hostname": hostname(),
    })))
}

/// User the daemon runs as — the account VS Code Remote-SSH should log in
/// with, since it owns the session worktrees.
fn daemon_user() -> Option<String> {
    std::env::var("USER")
        .or_else(|_| std::env::var("LOGNAME"))
        .or_else(|_| std::env::var("USERNAME"))
        .ok()
        .filter(|s| !s.is_empty())
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
