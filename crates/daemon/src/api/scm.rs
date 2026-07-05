use std::path::PathBuf;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use serde_json::json;

use super::{AppError, AppState};

async fn session_cwd(state: &AppState, id: &str) -> Result<PathBuf, AppError> {
    let session = state
        .manager
        .get_session(id)?
        .ok_or_else(|| AppError(StatusCode::NOT_FOUND, "no such session".into()))?;
    Ok(PathBuf::from(session.working_directory))
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
    let diff =
        run_blocking(move || scm.diff(&cwd, &path, untracked, commit.as_deref())).await?;
    Ok(Json(json!({ "path": params.path, "diff": diff })))
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

#[derive(Debug, Deserialize)]
pub struct RebaseBody {
    onto: String,
}

/// Rebase the session's current branch onto another local branch.
pub async fn rebase(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<RebaseBody>,
) -> Result<Json<serde_json::Value>, AppError> {
    let cwd = session_cwd(&state, &id).await?;
    let scm = state.scm.clone();
    let onto = body.onto;
    let output = run_blocking(move || scm.rebase(&cwd, &onto)).await?;
    Ok(Json(json!({ "output": output })))
}
