use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;
use tower_http::cors::CorsLayer;
use tower_http::services::{ServeDir, ServeFile};

use crate::config::Config;
use crate::plugins::{current_platform, PluginModels};
use crate::session_manager::{CreateSessionRequest, ForkRequest, SessionManager};
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
        .route("/api/fs/mkdir", post(fs::mkdir))
        .route("/api/plugins", get(list_plugins))
        .route("/api/plugins/:id/models", get(list_plugin_models))
        .route("/api/workspaces", get(list_workspaces).post(add_workspace))
        .route("/api/workspaces/:id", delete(remove_workspace))
        .route("/api/workspaces/:id/init-git", post(init_workspace_git))
        .route(
            "/api/workspaces/:id/cleanup-worktrees",
            post(cleanup_workspace_worktrees),
        )
        .route("/api/workspaces/:id/branches", get(list_workspace_branches))
        .route(
            "/api/workspaces/:id/branches/overview",
            get(workspace_branch_overview),
        )
        // Branch names contain `/` (`asm-session/…`), so the branch travels in the
        // JSON body rather than as a `:name` path segment.
        .route(
            "/api/workspaces/:id/branches/delete",
            post(delete_workspace_branch),
        )
        .route(
            "/api/workspaces/:id/branches/merge",
            post(merge_workspace_branches),
        )
        .route(
            "/api/workspaces/:id/branches/rebase",
            post(rebase_workspace_branches),
        )
        .route("/api/sessions", get(list_sessions).post(create_session))
        .route("/api/sessions/:id", get(get_session))
        .route("/api/sessions/:id/summary", get(get_summary))
        .route("/api/sessions/:id/transcript", get(get_transcript))
        .route("/api/sessions/:id/usage", get(get_session_usage))
        .route("/api/sessions/:id/workspace", get(get_session_workspace))
        .route("/api/sessions/:id/fork", post(fork_session))
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
        .route(
            "/api/sessions/:id/upload",
            // Same two-tier limit as `paste`, for the same reason: the transport
            // cap sits above the enforced one so the handler answers an oversize
            // upload with a clean 413.
            post(paste::upload_workspace)
                .layer(axum::extract::DefaultBodyLimit::max(paste::MAX_PASTE_BYTES + 512 * 1024)),
        )
        .route("/api/sessions/:id/ack", post(ack_attention))
        .route("/api/sessions/:id/deck", get(get_deck_prompt))
        .route(
            "/api/sessions/:id/deck/respond",
            post(respond_to_deck_prompt),
        )
        .route("/api/sessions/:id/vscode-target", get(vscode_target))
        .route("/api/sessions/:id/stream", get(ws::stream))
        .route("/api/sessions/:id/scm/status", get(scm::status))
        .route("/api/sessions/:id/scm/diff", get(scm::diff))
        .route("/api/sessions/:id/scm/file", get(scm::file))
        .route("/api/sessions/:id/scm/log", get(scm::log))
        .route("/api/sessions/:id/scm/commit", get(scm::commit))
        .route("/api/sessions/:id/scm/branches", get(scm::branches))
        .route("/api/sessions/:id/scm/fetch", post(scm::fetch))
        .route("/api/sessions/:id/scm/pull", post(scm::pull))
        .route("/api/sessions/:id/scm/push", post(scm::push))
        .route("/api/sessions/:id/scm/detach-branch", post(scm::detach_branch))
        .route("/api/sessions/:id/scm/attach-branch", post(scm::attach_branch))
        .route("/api/sessions/:id/scm/rebase", post(scm::rebase))
        .route("/api/sessions/:id/scm/merge", post(scm::merge));

    // Optionally serve a packaged web client.
    if let Some(dir) = static_dir {
        if dir.is_dir() {
            // `/deck` is a real, bookmarkable client-side route. Serve the SPA
            // entry at that exact path while preserving honest 404s for unknown
            // API paths (a catch-all SPA fallback would answer those with HTML).
            let index = dir.join("index.html");
            app = app
                .route_service("/deck", ServeFile::new(index.clone()))
                .route_service("/deck/", ServeFile::new(index))
                .fallback_service(ServeDir::new(dir));
        }
    }

    // Innermost first: the reconcile gate runs *after* auth (an unauthenticated
    // request must be rejected, not parked), and auth runs inside CORS so
    // preflight is handled before token checks.
    app.layer(axum::middleware::from_fn_with_state(
        state.clone(),
        await_startup_reconcile,
    ))
    .layer(axum::middleware::from_fn_with_state(
        state.clone(),
        crate::auth::require_auth,
    ))
    .layer(CorsLayer::permissive())
    .with_state(state)
}

/// How long a live-session request will wait for the startup reconcile pass
/// before giving up and running anyway. Adoption of a large session set takes
/// seconds; this is the backstop for a holder that never answers, so the request
/// fails its own honest way instead of hanging forever.
const READY_WAIT: std::time::Duration = std::time::Duration::from_secs(30);

/// Session sub-resources that resolve a *live* backend handle, and so mean
/// nothing until the startup reconcile pass has adopted the survivors.
const LIVE_ROUTES: [&str; 5] = ["stream", "stop", "resize", "paste", "deck"];

fn needs_live_session(path: &str) -> bool {
    path.strip_prefix("/api/sessions/")
        .and_then(|rest| rest.split_once('/'))
        .is_some_and(|(_, sub)| {
            sub.split('/')
                .next()
                .is_some_and(|resource| LIVE_ROUTES.contains(&resource))
        })
}

/// Park requests that need a live session until adoption has run.
///
/// The listener binds before the reconcile pass, so for the first seconds after
/// a restart a surviving session is `running` in the DB but not yet in `live`.
/// Attaching in that window would otherwise be served the read-only history
/// path — a live agent that looks dead. Everything else (`/health`, the session
/// list, auth) answers immediately, which is the whole point of binding early.
async fn await_startup_reconcile(
    State(state): State<AppState>,
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    if needs_live_session(req.uri().path()) && !state.manager.is_ready() {
        tracing::debug!(path = %req.uri().path(), "waiting for startup reconcile");
        if tokio::time::timeout(READY_WAIT, state.manager.wait_until_ready())
            .await
            .is_err()
        {
            tracing::warn!(
                path = %req.uri().path(),
                "startup reconcile still running after {READY_WAIT:?}; serving anyway"
            );
        }
    }
    next.run(req).await
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
        // Up and serving, but still adopting the sessions the previous daemon
        // left running — `active_sessions` is still climbing toward the truth.
        "reconciling": !state.manager.is_ready(),
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

/// The models a client can pick for one agent, for the new-session / fork
/// dropdown. Kept off `/api/plugins` because it can be slow — Codex queries its
/// app server and opencode shells out to `opencode models` — so it is only paid
/// when the dialog needs it. Runs on a blocking thread for the same reason.
async fn list_plugin_models(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let plugin = state
        .manager
        .registry()
        .get(&id)
        .ok_or_else(|| AppError(StatusCode::NOT_FOUND, format!("unknown agent plugin `{id}`")))?;
    let models = tokio::task::spawn_blocking(move || PluginModels::describe(plugin.as_ref()))
        .await
        .map_err(|e| AppError(StatusCode::INTERNAL_SERVER_ERROR, format!("models task: {e}")))?;
    Ok(Json(json!({ "models": models })))
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

/// Rich per-branch overview for the workspace branch-management dialog. Git-heavy
/// (per-branch reflog + rev-list), so it runs on a blocking thread.
async fn workspace_branch_overview(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let overview = tokio::task::spawn_blocking(move || state.manager.workspace_branch_overview(&id))
        .await
        .map_err(|e| AppError(StatusCode::INTERNAL_SERVER_ERROR, format!("task join: {e}")))??;
    Ok(Json(json!({ "overview": overview })))
}

#[derive(Debug, Deserialize)]
struct DeleteBranchBody {
    branch: String,
    #[serde(default)]
    force: bool,
}

async fn delete_workspace_branch(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<DeleteBranchBody>,
) -> Result<Json<serde_json::Value>, AppError> {
    let DeleteBranchBody { branch, force } = body;
    let res = tokio::task::spawn_blocking(move || {
        state.manager.delete_workspace_branch(&id, &branch, force)
    })
    .await
    .map_err(|e| AppError(StatusCode::INTERNAL_SERVER_ERROR, format!("task join: {e}")))?;
    match res {
        Ok(()) => Ok(Json(json!({ "ok": true }))),
        // Unmerged branch: 409 so the client can confirm and retry with force.
        Err(e) if e.downcast_ref::<crate::session_manager::NeedsForce>().is_some() => {
            Err(AppError(StatusCode::CONFLICT, format!("{e:#}")))
        }
        Err(e) => Err(e.into()),
    }
}

#[derive(Debug, Deserialize)]
struct MergeBranchesBody {
    source: String,
    target: String,
}

async fn merge_workspace_branches(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<MergeBranchesBody>,
) -> Result<Json<serde_json::Value>, AppError> {
    let MergeBranchesBody { source, target } = body;
    let res = tokio::task::spawn_blocking(move || {
        state.manager.merge_workspace_branches(&id, &source, &target)
    })
    .await
    .map_err(|e| AppError(StatusCode::INTERNAL_SERVER_ERROR, format!("task join: {e}")))?;
    match res {
        Ok(output) => Ok(Json(json!({ "output": output }))),
        // A conflicting merge is aborted; 409 so the client shows "resolve manually".
        Err(e) if e.downcast_ref::<crate::source_control::MergeConflict>().is_some() => {
            Err(AppError(StatusCode::CONFLICT, format!("{e:#}")))
        }
        Err(e) => Err(e.into()),
    }
}

#[derive(Debug, Deserialize)]
struct RebaseBranchBody {
    branch: String,
    onto: String,
}

async fn rebase_workspace_branches(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<RebaseBranchBody>,
) -> Result<Json<serde_json::Value>, AppError> {
    let RebaseBranchBody { branch, onto } = body;
    let output = tokio::task::spawn_blocking(move || {
        state.manager.rebase_workspace_branch(&id, &branch, &onto)
    })
    .await
    .map_err(|e| AppError(StatusCode::INTERNAL_SERVER_ERROR, format!("task join: {e}")))??;
    Ok(Json(json!({ "output": output })))
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
    // `session_json` reads agent files on a title-cache miss — keep the whole
    // enrichment off the async runtime.
    let sessions: Vec<serde_json::Value> = tokio::task::spawn_blocking(move || {
        Ok::<_, anyhow::Error>(
            state
                .manager
                .list_sessions()?
                .iter()
                .map(|s| session_json(s, &state))
                .collect(),
        )
    })
    .await
    .map_err(|e| AppError(StatusCode::INTERNAL_SERVER_ERROR, format!("list task: {e}")))??;
    Ok(Json(json!({ "sessions": sessions })))
}

/// Serialize a session plus what the list view derives at read time: `attached`
/// (is a live client currently on it? — so the UI can prompt for takeover
/// instead of silently stealing it), the agent's own `title` for the session,
/// and the `branch` its workspace instance holds. Blocking: a title-cache miss
/// reads the agent's transcript.
fn session_json(s: &crate::domain::Session, state: &AppState) -> serde_json::Value {
    let mut v = serde_json::to_value(s).unwrap_or_else(|_| json!({}));
    if let Some(obj) = v.as_object_mut() {
        obj.insert("attached".into(), json!(state.attachments.is_attached(&s.id)));
        let title = state.manager.registry().get(&s.agent_plugin_id).and_then(|p| {
            p.title(&crate::plugins::usage::TranscriptContext {
                cwd: std::path::PathBuf::from(&s.working_directory),
                started_at_ms: s.created_at,
            })
        });
        obj.insert("title".into(), json!(title));
        let branch = state
            .manager
            .get_instance_for_session(&s.id)
            .ok()
            .flatten()
            .and_then(|i| i.branch);
        obj.insert("branch".into(), json!(branch));
        // Whether this session's agent kept a conversation we could resume. A fork
        // that keeps the same agent then carries the whole conversation; anything
        // else carries a written brief. The id itself stays on the host.
        obj.insert(
            "has_agent_conversation".into(),
            json!(s.agent_session_id.is_some()),
        );
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
    /// Model override; omit to start with the agent's own configured default.
    #[serde(default)]
    model: Option<String>,
}

/// Treat an empty or whitespace-only model string as "no override" — the client's
/// "Default" dropdown entry sends `""`, which must launch with no model flag
/// rather than pass an empty `--model` to the agent.
fn normalize_model(model: Option<String>) -> Option<String> {
    model.map(|m| m.trim().to_string()).filter(|m| !m.is_empty())
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
        model: normalize_model(body.model),
        fork: None,
    };
    let session = state.manager.create_session(req)?;
    Ok(Json(json!({ "session": session })))
}

#[derive(Debug, Deserialize)]
struct ForkSessionBody {
    /// The agent to fork into — the origin's own, or a different one.
    agent_plugin_id: String,
    /// Continue on the origin's branch, in its worktree, instead of branching off
    /// it. Safe once the origin has stopped; while it is still running, this puts
    /// two agents in one directory and the client is expected to have warned.
    #[serde(default)]
    same_branch: bool,
    #[serde(default)]
    options: HashMap<String, bool>,
    /// Model override for the fork; omit to use the target agent's own default.
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    rows: Option<u16>,
    #[serde(default)]
    cols: Option<u16>,
}

/// Fork a session: a new session on the origin's branch (or one off it), carrying
/// the origin's context.
///
/// `spawn_blocking` is not an optimization here. Forking runs `git worktree add`
/// and may run an agent CLI headlessly for tens of seconds to write the handoff
/// brief; on the async runtime that would stall every other request in the daemon
/// for the duration.
async fn fork_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<ForkSessionBody>,
) -> Result<Json<serde_json::Value>, AppError> {
    let req = ForkRequest {
        agent_plugin_id: body.agent_plugin_id,
        options: body.options.into_iter().collect(),
        model: normalize_model(body.model),
        same_branch: body.same_branch,
        rows: body.rows.unwrap_or(24),
        cols: body.cols.unwrap_or(80),
    };
    let manager = state.manager.clone();
    let session = tokio::task::spawn_blocking(move || manager.fork_session(&id, req))
        .await
        .map_err(|e| anyhow::anyhow!("fork task panicked: {e}"))??;
    Ok(Json(json!({ "session": session })))
}

async fn get_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    // Same blocking enrichment as `list_sessions`.
    let session = tokio::task::spawn_blocking(move || {
        Ok::<_, anyhow::Error>(state.manager.get_session(&id)?.map(|s| session_json(&s, &state)))
    })
    .await
    .map_err(|e| AppError(StatusCode::INTERNAL_SERVER_ERROR, format!("get task: {e}")))??;
    match session {
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

/// Structured approval data for button-based controllers. `prompt: null` means
/// the session needs attention but its current screen cannot be answered safely
/// as numbered buttons; controllers should offer the full terminal instead.
async fn get_deck_prompt(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let prompt = state.manager.deck_prompt(&id)?;
    Ok(Json(json!({ "prompt": prompt })))
}

#[derive(Debug, Deserialize)]
struct DeckResponseBody {
    revision: String,
    option_id: usize,
}

/// Drive the same arrow+Enter input the terminal menu expects, without opening
/// a WebSocket attachment (and therefore without taking over another viewer).
async fn respond_to_deck_prompt(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<DeckResponseBody>,
) -> Result<Json<serde_json::Value>, AppError> {
    match state
        .manager
        .respond_to_deck_prompt(&id, &body.revision, body.option_id)
    {
        Ok(session) => Ok(Json(json!({ "session": session }))),
        Err(e) if e.downcast_ref::<crate::session_manager::StaleDeckPrompt>().is_some() => {
            Err(AppError(StatusCode::CONFLICT, format!("{e:#}")))
        }
        Err(e) => Err(e.into()),
    }
}

/// `?format=raw` opts out of the rendered conversation.
#[derive(Debug, Default, Deserialize)]
struct TranscriptQuery {
    format: Option<String>,
}

/// Download a session's conversation as Markdown, rendered from the agent's own
/// on-disk transcript (see [`crate::plugins::conversation`]).
///
/// This deliberately does *not* serve the recorded PTY stream by default: those
/// bytes are what a TUI sent a terminal — escape sequences and repainted frames,
/// tens of MB of them, which no editor renders and no human can read.
/// `?format=raw` still returns them, because they're the exact bytes replayed on
/// history attach and that's what you want when debugging replay. Raw is also
/// the fallback for agents that keep no transcript of their own (a plain shell),
/// where the PTY bytes *are* the record.
///
/// There is no delta — every call returns everything recorded so far (for a live
/// session, the conversation up to now). Archived sessions have had their bytes
/// discarded, so nothing is offered for them.
async fn get_transcript(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<TranscriptQuery>,
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

    if q.format.as_deref() != Some("raw") {
        // Reads and parses an agent file that can run to hundreds of MB — keep
        // it off the async runtime.
        let manager = state.manager.clone();
        let sess = s.clone();
        let rendered = tokio::task::spawn_blocking(move || {
            manager.registry().get(&sess.agent_plugin_id).and_then(|p| {
                p.conversation(&crate::plugins::usage::TranscriptContext {
                    cwd: std::path::PathBuf::from(&sess.working_directory),
                    started_at_ms: sess.created_at,
                })
            })
        })
        .await
        .map_err(|e| {
            AppError(StatusCode::INTERNAL_SERVER_ERROR, format!("transcript task: {e}"))
        })?;

        if let Some(markdown) = rendered {
            return Ok(attachment(
                markdown.into_bytes(),
                "text/markdown; charset=utf-8",
                &transcript_filename(&s, "md"),
            ));
        }
    }

    let bytes = state.manager.db().read_events_after(&id, 0)?;
    Ok(attachment(bytes, "text/plain; charset=utf-8", &transcript_filename(&s, "log")))
}

/// A file-download response. `no-store` because a live session's transcript
/// grows under the same URL.
fn attachment(body: Vec<u8>, content_type: &str, filename: &str) -> Response {
    (
        [
            (header::CONTENT_TYPE, content_type.to_string()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{filename}\""),
            ),
            (header::CACHE_CONTROL, "no-store".to_string()),
        ],
        body,
    )
        .into_response()
}

/// A safe download filename for a session's transcript. The session id is a
/// UUID, but the agent plugin id is free-form, so fold anything outside
/// `[A-Za-z0-9._-]` to `_` — this both tidies the name and keeps stray bytes
/// out of the `Content-Disposition` header.
fn transcript_filename(s: &crate::domain::Session, ext: &str) -> String {
    let agent: String = s
        .agent_plugin_id
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') { c } else { '_' })
        .collect();
    format!("session-{agent}-{}.{ext}", s.id)
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
            p.usage(&crate::plugins::usage::TranscriptContext {
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

#[cfg(test)]
mod tests {
    use super::{needs_live_session, normalize_model};

    #[test]
    fn normalize_model_treats_blank_as_no_override() {
        // The client's "Default" dropdown entry sends `""`; it must launch with
        // no model flag, not an empty `--model`.
        assert_eq!(normalize_model(None), None);
        assert_eq!(normalize_model(Some("".into())), None);
        assert_eq!(normalize_model(Some("   ".into())), None);
        assert_eq!(normalize_model(Some("  sonnet ".into())).as_deref(), Some("sonnet"));
    }

    #[test]
    fn gates_only_the_routes_that_resolve_a_live_handle() {
        assert!(needs_live_session("/api/sessions/abc/stream"));
        assert!(needs_live_session("/api/sessions/abc/stop"));
        assert!(needs_live_session("/api/sessions/abc/resize"));
        assert!(needs_live_session("/api/sessions/abc/paste"));
        assert!(needs_live_session("/api/sessions/abc/deck"));
        assert!(needs_live_session("/api/sessions/abc/deck/respond"));

        // Pure DB/git reads, and session creation — none of them touch `live`,
        // so they must not wait on adoption.
        assert!(!needs_live_session("/api/sessions"));
        assert!(!needs_live_session("/api/sessions/abc"));
        assert!(!needs_live_session("/api/sessions/abc/transcript"));
        // A workspace upload only reads the session record and writes a file —
        // no `live` handle — so it must not be parked behind adoption either.
        assert!(!needs_live_session("/api/sessions/abc/upload"));
        assert!(!needs_live_session("/api/sessions/abc/scm/status"));
        assert!(!needs_live_session("/health"));
    }
}
