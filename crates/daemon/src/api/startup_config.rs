//! `GET/PUT /api/config` — this host's saved startup configuration, for the web
//! client's Config dialog — and `POST /api/config/apply`, which applies it now.
//! See `crate::startup_config` for the file and its rules. Authenticated like
//! the rest of `/api`: an enrolled device can already run a shell here, so
//! editing (or applying) what the start script does is no new power. Secrets
//! are write-only: a read says whether each is set, never its value.

use std::path::PathBuf;
use std::process::{Command, Stdio};

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use parking_lot::Mutex;
use serde::Deserialize;
use serde_json::json;

use super::{hostname, AppError, AppState};
use crate::startup_config::{self as sc, SaveError, Snapshot, Store, Values};
use crate::util::now_millis;

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

fn now_secs() -> i64 {
    now_millis() / 1000
}

/// `apply-config.sh`, when this daemon was started by the scripts (which say
/// where they live) — the only case in which it can restart itself.
fn apply_script(state: &AppState) -> Option<PathBuf> {
    let script = state.config.scripts_dir.as_ref()?.join(sc::APPLY_SCRIPT);
    script.is_file().then_some(script)
}

fn view(state: &AppState, store: &Store, snap: Snapshot) -> serde_json::Value {
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
        "can_apply": apply_script(state).is_some(),
        "apply": sc::read_apply_status(&state.config.runtime_dir, &state.config.data_dir, now_secs()),
    })
}

pub async fn get(State(state): State<AppState>) -> Result<Json<serde_json::Value>, AppError> {
    let store = store(&state);
    let snap = store
        .load()
        .map_err(|e| AppError(StatusCode::INTERNAL_SERVER_ERROR, format!("reading the startup config: {e}")))?;
    Ok(Json(view(&state, &store, snap)))
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
            Ok(Json(view(&state, &store, snap)))
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

/// Apply the saved config now: run `apply-config.sh` (a flagless
/// `scripts/start.sh` for this install) detached, and return at once. The script
/// may restart this very daemon — it runs in its own process group so it
/// outlives us — and records progress that `GET /api/config` reports as `apply`.
pub async fn apply(
    State(state): State<AppState>,
) -> Result<(StatusCode, Json<serde_json::Value>), AppError> {
    let Some(script) = apply_script(&state) else {
        return Err(AppError(
            StatusCode::CONFLICT,
            "this daemon was not started by scripts/start.sh, so it cannot apply its settings \
             itself — run scripts/start.sh on its host"
                .into(),
        ));
    };
    let (data_dir, runtime_dir) = (&state.config.data_dir, &state.config.runtime_dir);
    let now = now_secs();
    {
        let _guard = WRITE_LOCK.lock();
        if sc::read_apply_status(runtime_dir, data_dir, now).is_some_and(|s| s.state == "running") {
            return Err(AppError(
                StatusCode::CONFLICT,
                "an apply is already running on this host".into(),
            ));
        }
        sc::write_apply_started(runtime_dir, now).map_err(|e| {
            AppError(StatusCode::INTERNAL_SERVER_ERROR, format!("recording the apply: {e}"))
        })?;
        let mut cmd = Command::new("bash");
        cmd.arg(&script)
            .arg("--data-dir")
            .arg(data_dir)
            .arg("--runtime-dir")
            .arg(runtime_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            cmd.process_group(0);
        }
        match cmd.spawn() {
            // Reap it whenever it ends, so a daemon that outlives it keeps no zombie.
            Ok(mut child) => {
                std::thread::spawn(move || {
                    let _ = child.wait();
                });
            }
            Err(e) => {
                let msg = format!("could not run {}: {e}", script.display());
                let _ = std::fs::write(data_dir.join(sc::APPLY_LOG), &msg);
                let _ = sc::write_atomic(
                    &runtime_dir.join(sc::APPLY_STATUS_FILE_NAME),
                    &format!("state=failed\npid=0\nstarted={now}\nexit=127\nfinished={now}\n"),
                );
                return Err(AppError(StatusCode::INTERNAL_SERVER_ERROR, msg));
            }
        }
    }
    tracing::info!(script = %script.display(), "applying the saved startup config");
    Ok((
        StatusCode::ACCEPTED,
        Json(json!({ "apply": sc::read_apply_status(runtime_dir, data_dir, now) })),
    ))
}
