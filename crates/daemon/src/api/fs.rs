use std::path::PathBuf;

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::{AppError, AppState};

/// A directory entry returned to the client's directory picker.
#[derive(Debug, Serialize)]
struct FsEntry {
    name: String,
    path: String,
    is_dir: bool,
    is_git: bool,
}

#[derive(Debug, Deserialize)]
pub struct ListParams {
    /// Absolute path to list. Empty defaults to the daemon user's home.
    #[serde(default)]
    path: String,
    #[serde(default)]
    show_hidden: bool,
}

/// Browse the daemon host's filesystem so the client can pick a working
/// directory or workspace root without typing the whole path. Read-only;
/// returns directories only. This intentionally exposes the server's directory
/// tree the way a file-open dialog would — appropriate for a personal daemon.
pub async fn list(
    State(_state): State<AppState>,
    Query(params): Query<ListParams>,
) -> Result<Json<serde_json::Value>, AppError> {
    let base = if params.path.trim().is_empty() {
        home_dir()
    } else {
        PathBuf::from(&params.path)
    };
    let canonical = std::fs::canonicalize(&base).unwrap_or(base);
    if !canonical.is_dir() {
        return Err(AppError(
            StatusCode::BAD_REQUEST,
            format!("not a directory: {}", canonical.display()),
        ));
    }

    let read = std::fs::read_dir(&canonical).map_err(|e| {
        AppError(
            StatusCode::BAD_REQUEST,
            format!("cannot read {}: {e}", canonical.display()),
        )
    })?;

    let mut entries: Vec<FsEntry> = Vec::new();
    for e in read.flatten() {
        let file_type = match e.file_type() {
            Ok(t) => t,
            Err(_) => continue,
        };
        if !file_type.is_dir() {
            continue; // directories only
        }
        let name = e.file_name().to_string_lossy().into_owned();
        if !params.show_hidden && name.starts_with('.') {
            continue;
        }
        let path = e.path();
        let is_git = path.join(".git").exists();
        entries.push(FsEntry {
            name,
            path: path.to_string_lossy().into_owned(),
            is_dir: true,
            is_git,
        });
    }
    entries.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

    let parent = canonical
        .parent()
        .map(|p| p.to_string_lossy().into_owned());

    Ok(Json(json!({
        "path": canonical.to_string_lossy(),
        "parent": parent,
        "entries": entries,
    })))
}

fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}
