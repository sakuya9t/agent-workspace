use std::path::PathBuf;

use axum::extract::{Path, Query, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;
use serde_json::json;

use crate::source_control::MergeConflict;

use super::paste::sniff_image_mime;
use super::{AppError, AppState};

async fn session_cwd(state: &AppState, id: &str) -> Result<PathBuf, AppError> {
    let session = state
        .manager
        .get_session(id)?
        .ok_or_else(|| AppError(StatusCode::NOT_FOUND, "no such session".into()))?;
    Ok(PathBuf::from(session.working_directory))
}

/// The session's working directory *and* its agent id — the latter so a conflict
/// resolver can prefer the session's own agent to resolve its rebase/merge.
async fn session_cwd_and_agent(state: &AppState, id: &str) -> Result<(PathBuf, String), AppError> {
    let session = state
        .manager
        .get_session(id)?
        .ok_or_else(|| AppError(StatusCode::NOT_FOUND, "no such session".into()))?;
    Ok((
        PathBuf::from(session.working_directory),
        session.agent_plugin_id,
    ))
}

async fn run_blocking<T, F>(f: F) -> Result<T, AppError>
where
    T: Send + 'static,
    F: FnOnce() -> anyhow::Result<T> + Send + 'static,
{
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| AppError(StatusCode::INTERNAL_SERVER_ERROR, format!("task join: {e}")))?
        .map_err(AppError::from)
}

pub async fn status(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let cwd = session_cwd(&state, &id).await?;
    let scm = state.scm.clone();
    let status = run_blocking(move || scm.status(&cwd)).await?;
    Ok(Json(json!({ "status": status })))
}

#[derive(Debug, Deserialize)]
pub struct DiffParams {
    path: String,
    #[serde(default)]
    untracked: bool,
    /// When set, show the path's diff as introduced by this commit rather than
    /// the working-tree diff.
    #[serde(default)]
    commit: Option<String>,
}

pub async fn diff(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(params): Query<DiffParams>,
) -> Result<Json<serde_json::Value>, AppError> {
    let cwd = session_cwd(&state, &id).await?;
    let scm = state.scm.clone();
    let path = params.path.clone();
    let untracked = params.untracked;
    let commit = params.commit.clone();
    let diff = run_blocking(move || scm.diff(&cwd, &path, untracked, commit.as_deref())).await?;
    Ok(Json(json!({ "path": params.path, "diff": diff })))
}

/// Which side of the diff to preview: the new content or the prior version.
#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DiffSide {
    /// The prior version — HEAD for a working-tree change, the commit's parent
    /// for a commit diff.
    Before,
    /// The new content — the working tree, or the file at the commit.
    #[default]
    After,
}

#[derive(Debug, Deserialize)]
pub struct FileParams {
    path: String,
    /// When set, preview the file as it existed at this commit rather than the
    /// working-tree version.
    #[serde(default)]
    commit: Option<String>,
    /// Which side of the diff to fetch (default `after`).
    #[serde(default)]
    side: DiffSide,
}

/// Serve one side of a changed file's inline preview (images in the diff
/// panel). Only recognised image types are returned: the `Content-Type` is
/// sniffed from the leading bytes — never guessed from the path — and anything
/// that isn't a known image is refused, so this can't be turned into a generic
/// file reader or an XSS vector via a mislabelled extension. A side that has no
/// content (a new file's "before", or a deleted file's "after") is a 404 the
/// client renders as a one-sided diff.
pub async fn file(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(params): Query<FileParams>,
) -> Result<Response, AppError> {
    let cwd = session_cwd(&state, &id).await?;
    let scm = state.scm.clone();
    let path = params.path.clone();
    let side = params.side;
    let commit = params.commit.clone();

    // Validate a client-supplied commit up front: the "before" side builds a
    // `<hash>^` parent expression from it, so it must be a bare hash and never
    // anything git could read as an option.
    if let Some(h) = commit.as_deref() {
        if !crate::source_control::is_commit_hash(h) {
            return Err(AppError(StatusCode::BAD_REQUEST, "invalid commit hash".into()));
        }
    }

    let bytes = run_blocking(move || -> anyhow::Result<Option<Vec<u8>>> {
        // Resolve the revision this side reads from; `None` means the working
        // tree. `resolve_commit` only ever sees controlled expressions.
        let rev: Option<String> = match (side, commit.as_deref()) {
            (DiffSide::After, None) => None,
            (DiffSide::After, Some(h)) => Some(h.to_string()),
            (DiffSide::Before, None) => match scm.resolve_commit(&cwd, "HEAD")? {
                Some(h) => Some(h),
                None => return Ok(None), // empty repo — no prior version
            },
            (DiffSide::Before, Some(h)) => match scm.resolve_commit(&cwd, &format!("{h}^"))? {
                Some(h) => Some(h),
                None => return Ok(None), // root commit — no parent version
            },
        };
        scm.file_bytes(&cwd, &path, rev.as_deref())
    })
    .await?
    .ok_or_else(|| AppError(StatusCode::NOT_FOUND, "no such file version".into()))?;

    let mime = sniff_image_mime(&bytes).ok_or_else(|| {
        AppError(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "not a previewable image".into(),
        )
    })?;

    Ok((
        [
            (header::CONTENT_TYPE, mime),
            // The sniffed type is authoritative; stop the browser guessing.
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        bytes,
    )
        .into_response())
}

#[derive(Debug, Deserialize)]
pub struct CommitParams {
    hash: String,
}

pub async fn commit(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(params): Query<CommitParams>,
) -> Result<Json<serde_json::Value>, AppError> {
    let cwd = session_cwd(&state, &id).await?;
    let scm = state.scm.clone();
    let hash = params.hash.clone();
    let commit = run_blocking(move || scm.show(&cwd, &hash)).await?;
    Ok(Json(json!({ "commit": commit })))
}

#[derive(Debug, Deserialize)]
pub struct LogParams {
    #[serde(default)]
    limit: Option<usize>,
}

pub async fn log(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(params): Query<LogParams>,
) -> Result<Json<serde_json::Value>, AppError> {
    let cwd = session_cwd(&state, &id).await?;
    let scm = state.scm.clone();
    let limit = params.limit.unwrap_or(30).min(200);
    let commits = run_blocking(move || scm.log(&cwd, limit)).await?;
    Ok(Json(json!({ "commits": commits })))
}

/// Local branches (rebase-target choices for the history panel).
pub async fn branches(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let cwd = session_cwd(&state, &id).await?;
    let scm = state.scm.clone();
    let (branches, head) = run_blocking(move || scm.branches(&cwd)).await?;
    Ok(Json(json!({ "branches": branches, "head": head })))
}

/// Refresh every remote's tracking refs, so the remote commits the panel shows
/// are current rather than as-of-the-last-fetch. Changes no branch.
pub async fn fetch(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let cwd = session_cwd(&state, &id).await?;
    let scm = state.scm.clone();
    let output = run_blocking(move || scm.fetch(&cwd)).await?;
    Ok(Json(json!({ "output": output })))
}

/// Fast-forward-only pull of the session's current branch.
pub async fn pull(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let cwd = session_cwd(&state, &id).await?;
    let scm = state.scm.clone();
    let output = run_blocking(move || scm.pull(&cwd)).await?;
    Ok(Json(json!({ "output": output })))
}

/// Push the session's current branch to origin, creating the remote branch when
/// it doesn't exist yet.
pub async fn push(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let cwd = session_cwd(&state, &id).await?;
    let scm = state.scm.clone();
    let output = run_blocking(move || scm.push(&cwd)).await?;
    Ok(Json(json!({ "output": output })))
}

async fn set_branch_attached(
    state: AppState,
    id: String,
    attached: bool,
) -> Result<Json<serde_json::Value>, AppError> {
    let manager = state.manager.clone();
    let branch = run_blocking(move || manager.set_instance_branch_attached(&id, attached)).await?;
    Ok(Json(json!({ "branch": branch, "attached": attached })))
}

/// Release the session worktree's branch so another worktree can check it out.
/// The session stays at the exact same commit with all local changes intact.
pub async fn detach_branch(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    set_branch_attached(state, id, false).await
}

/// Reclaim the session's recorded branch after the other checkout releases it.
pub async fn attach_branch(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    set_branch_attached(state, id, true).await
}

#[derive(Debug, Deserialize)]
pub struct RebaseBody {
    onto: String,
}

/// Rebase the session's current branch onto another local branch. Conflicts are
/// handed to the session's agent to auto-resolve before any abort.
pub async fn rebase(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<RebaseBody>,
) -> Result<Json<serde_json::Value>, AppError> {
    let (cwd, agent) = session_cwd_and_agent(&state, &id).await?;
    let scm = state.scm.clone();
    let registry = state.manager.registry_arc();
    let onto = body.onto;
    let output = run_blocking(move || {
        let resolver = crate::conflict_resolve::AgentConflictResolver::new(registry, Some(agent));
        scm.rebase(&cwd, &onto, Some(&resolver))
    })
    .await?;
    Ok(Json(json!({ "output": output })))
}

#[derive(Debug, Deserialize)]
pub struct MergeBody {
    target: String,
}

/// Merge the session's current branch into another local branch. Conflicts are
/// handed to the session's agent to auto-resolve; a `MergeConflict` now means the
/// agent could not finish, not that no attempt was made.
pub async fn merge(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<MergeBody>,
) -> Result<Json<serde_json::Value>, AppError> {
    let (cwd, agent) = session_cwd_and_agent(&state, &id).await?;
    let scm = state.scm.clone();
    let registry = state.manager.registry_arc();
    let target = body.target;
    let result = tokio::task::spawn_blocking(move || {
        let resolver = crate::conflict_resolve::AgentConflictResolver::new(registry, Some(agent));
        scm.merge_to_branch(&cwd, &target, Some(&resolver))
    })
    .await
    .map_err(|e| AppError(StatusCode::INTERNAL_SERVER_ERROR, format!("task join: {e}")))?;
    match result {
        Ok(output) => Ok(Json(json!({ "output": output }))),
        Err(e) if e.downcast_ref::<MergeConflict>().is_some() => {
            Err(AppError(StatusCode::CONFLICT, format!("{e:#}")))
        }
        Err(e) => Err(AppError::from(e)),
    }
}
