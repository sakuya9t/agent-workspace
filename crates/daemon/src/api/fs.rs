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
    entries.sort_by_key(|e| e.name.to_lowercase());

    let parent = canonical
        .parent()
        .map(|p| p.to_string_lossy().into_owned());

    Ok(Json(json!({
        "path": canonical.to_string_lossy(),
        "parent": parent,
        "entries": entries,
    })))
}

#[derive(Debug, Deserialize)]
pub struct MkdirBody {
    /// Absolute path of the directory to create the folder in.
    parent: String,
    /// Name of the new folder — a single path component, no separators.
    name: String,
}

/// Create a folder inside an existing directory, so the picker can offer a
/// "new folder" action like a native save dialog. Same trust model as `list`:
/// the daemon user's own filesystem, gated by the API auth layer.
pub async fn mkdir(
    State(_state): State<AppState>,
    Json(body): Json<MkdirBody>,
) -> Result<Json<serde_json::Value>, AppError> {
    let name = body.name.trim();
    if name.is_empty() {
        return Err(AppError(
            StatusCode::BAD_REQUEST,
            "folder name is empty".into(),
        ));
    }
    // One path component only: the picker names a folder, it doesn't build paths.
    if name == "." || name == ".." || name.contains('/') || name.contains('\\') {
        return Err(AppError(
            StatusCode::BAD_REQUEST,
            format!("invalid folder name: {name}"),
        ));
    }

    let parent = PathBuf::from(&body.parent);
    let canonical = std::fs::canonicalize(&parent).map_err(|e| {
        AppError(
            StatusCode::BAD_REQUEST,
            format!("cannot resolve {}: {e}", parent.display()),
        )
    })?;
    if !canonical.is_dir() {
        return Err(AppError(
            StatusCode::BAD_REQUEST,
            format!("not a directory: {}", canonical.display()),
        ));
    }

    let path = canonical.join(name);
    std::fs::create_dir(&path).map_err(|e| {
        let msg = if e.kind() == std::io::ErrorKind::AlreadyExists {
            format!("already exists: {}", path.display())
        } else {
            format!("cannot create {}: {e}", path.display())
        };
        AppError(StatusCode::BAD_REQUEST, msg)
    })?;

    Ok(Json(json!({ "path": path.to_string_lossy() })))
}

fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}
