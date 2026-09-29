//! `GET/PUT /api/config` — this host's saved startup configuration, for the web
//! client's Config dialog. See `crate::startup_config` for the file and its
//! rules. Authenticated like the rest of `/api`: an enrolled device can already
//! run a shell here, so editing what the start script does next is no new power.
//! Secrets are write-only: a read says whether each is set, never its value.

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use parking_lot::Mutex;
use serde::Deserialize;
use serde_json::json;

use super::{hostname, AppError, AppState};
use crate::startup_config::{self as sc, SaveError, Snapshot, Store, Values};

/// Serializes this daemon's own read-modify-write cycles. Other writers (the
/// scripts, the UI server) are caught by the revision check instead.
static WRITE_LOCK: Mutex<()> = Mutex::new(());

fn process_env(key: &str) -> Option<String> {
    std::env::var(key).ok()
}

fn store(state: &AppState) -> Store<'static> {
    Store {
        path: sc::path_in(&state.config.data_dir),
        applied_path: sc::applied_path_in(&state.config.runtime_dir),
        env: &process_env,
    }
}

fn view(store: &Store, snap: Snapshot) -> serde_json::Value {
    let mut values = serde_json::Map::new();
    let mut secrets = serde_json::Map::new();
    for key in sc::KEYS {
        let value = snap.values.get(key);
        if sc::is_secret(key) {
            secrets.insert(key.into(), json!(value.is_some_and(|v| !v.is_empty())));
        } else if let Some(v) = value {
            values.insert(key.into(), json!(v));
        }
    }
    json!({
        "source": "daemon",
        "hostname": hostname(),
        "path": store.path.display().to_string(),
        "exists": snap.exists,
        "revision": snap.revision,
        "values": values,
        "secrets": secrets,
        "pending": snap.pending,
    })
}

pub async fn get(State(state): State<AppState>) -> Result<Json<serde_json::Value>, AppError> {
    let store = store(&state);
    let snap = store
        .load()
        .map_err(|e| AppError(StatusCode::INTERNAL_SERVER_ERROR, format!("reading the startup config: {e}")))?;
    Ok(Json(view(&store, snap)))
}

#[derive(Debug, Deserialize)]
pub struct PutBody {
    /// The revision the edit was based on (`null`: there was no file).
    revision: Option<String>,
    /// Settings to change. A secret key that is absent keeps its value.
    values: Values,
}

pub async fn put(
    State(state): State<AppState>,
    Json(body): Json<PutBody>,
) -> Result<Json<serde_json::Value>, AppError> {
    let store = store(&state);
    let saved = {
        let _guard = WRITE_LOCK.lock();
        store.save(body.revision.as_deref(), &body.values)
    };
    match saved {
        Ok(snap) => {
            tracing::info!(path = %store.path.display(), "startup config saved via API");
            Ok(Json(view(&store, snap)))
        }
        Err(e) => {
            let status = match e {
                SaveError::Conflict => StatusCode::CONFLICT,
                SaveError::UnknownKey(_) | SaveError::Invalid(_) => StatusCode::BAD_REQUEST,
                SaveError::Io(_) => StatusCode::INTERNAL_SERVER_ERROR,
            };
            Err(AppError(status, e.to_string()))
        }
    }
}
