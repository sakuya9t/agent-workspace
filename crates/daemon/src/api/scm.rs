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

/// A client-supplied commit reaches git as a positional argument and, for the
/// "before" side, as the base of a `<hash>^` parent expression. Restricting it
/// to a bare hash here blocks both option injection and revision expressions.
fn check_commit(commit: Option<&str>) -> Result<(), AppError> {
    match commit {
        Some(h) if !crate::source_control::is_commit_hash(h) => Err(AppError(
            StatusCode::BAD_REQUEST,
            "invalid commit hash".into(),
        )),
        _ => Ok(()),
    }
}

/// Read one side of a file, as `file_bytes` wants it: `Ok(None)` both for a side
/// that has no version at all (a new file's "before", a root commit's parent)
/// and for a path absent at the resolved revision. Callers turn that into a 404.
/// `resolve_commit` only ever sees controlled expressions.
fn read_side(
    scm: &dyn crate::source_control::SourceControl,
    cwd: &std::path::Path,
    path: &str,
    side: DiffSide,
    commit: Option<&str>,
) -> anyhow::Result<Option<Vec<u8>>> {
    let rev: Option<String> = match (side, commit) {
        (DiffSide::After, None) => None,
        (DiffSide::After, Some(h)) => Some(h.to_string()),
        (DiffSide::Before, None) => match scm.resolve_commit(cwd, "HEAD")? {
            Some(h) => Some(h),
            None => return Ok(None), // empty repo — no prior version
        },
        (DiffSide::Before, Some(h)) => match scm.resolve_commit(cwd, &format!("{h}^"))? {
            Some(h) => Some(h),
            None => return Ok(None), // root commit — no parent version
        },
    };
    scm.file_bytes(cwd, path, rev.as_deref())
}

/// Serve one side of a changed file's inline preview (images in the diff
/// panel). Only recognised image types are returned: the `Content-Type` is
/// sniffed from the leading bytes — never guessed from the path — and anything
/// that isn't a known image is refused, so this can't be turned into an XSS
/// vector via a mislabelled extension. (Whole-file *text* is served as JSON by
/// `content` below, where no Content-Type is under the caller's influence.) A
/// side that has no content (a new file's "before", or a deleted file's "after")
/// is a 404 the client renders as a one-sided diff.
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
    check_commit(commit.as_deref())?;

    let bytes = run_blocking(move || read_side(&*scm, &cwd, &path, side, commit.as_deref()))
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

/// A diff answers "what changed"; reading the file answers "what does this look
/// like now". Rendering it costs a DOM node per line, so cap what we hand the
/// browser well below `file_bytes`' 10 MB ceiling and say when we truncated —
/// a silently short file reads as a corrupt one.
const MAX_TEXT_BYTES: usize = 1024 * 1024;

/// How much of the head of a file to search for a NUL before calling it text.
/// Git's own binary heuristic looks at a comparable prefix.
const BINARY_SNIFF_BYTES: usize = 8000;

/// Whole-file text behind the diff viewer's "view whole file" mode, for both a
/// working-tree file and a file as of a commit.
///
/// Unlike `file` this deliberately serves non-image content, so it is worth
/// being explicit about why that is not the generic file reader `file` refuses
/// to become: it reads through `file_bytes`, which rejects absolute paths and
/// `..`, and confines the canonicalized result to the session's own working
/// directory, so a symlink committed in the repo cannot walk out of it. The
/// content comes back as a JSON string — never as a response body whose
/// `Content-Type` a mislabelled extension could steer — and the same bytes are
/// already reachable through `diff`, which renders an untracked file in full.
pub async fn content(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(params): Query<FileParams>,
) -> Result<Json<serde_json::Value>, AppError> {
    let cwd = session_cwd(&state, &id).await?;
    let scm = state.scm.clone();
    let path = params.path.clone();
    let side = params.side;
    let commit = params.commit.clone();
    check_commit(commit.as_deref())?;

    let bytes = run_blocking(move || read_side(&*scm, &cwd, &path, side, commit.as_deref()))
        .await?
        .ok_or_else(|| AppError(StatusCode::NOT_FOUND, "no such file version".into()))?;

    // A NUL in the head is the signal that rendering this as lines of text would
    // produce noise; report it as binary and let the client say so plainly.
    if bytes.iter().take(BINARY_SNIFF_BYTES).any(|&b| b == 0) {
        return Ok(Json(json!({
            "path": params.path,
            "content": "",
            "binary": true,
            "truncated": false,
        })));
    }

    let truncated = bytes.len() > MAX_TEXT_BYTES;
    // Cut back to the last newline inside the cap so the final line shown is a
    // whole one, rather than a line the file never contained.
    let end = if truncated {
        bytes[..MAX_TEXT_BYTES]
            .iter()
            .rposition(|&b| b == b'\n')
            .map(|i| i + 1)
            .unwrap_or(MAX_TEXT_BYTES)
    } else {
        bytes.len()
    };

    Ok(Json(json!({
        "path": params.path,
        "content": String::from_utf8_lossy(&bytes[..end]),
        "binary": false,
        "truncated": truncated,
    })))
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
