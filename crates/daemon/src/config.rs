use std::net::SocketAddr;
use std::path::PathBuf;

use anyhow::{Context, Result};
use directories::ProjectDirs;

/// Runtime configuration and platform-specific directory resolution.
///
/// This is the seed of the Platform abstraction from the architecture doc:
/// data dir, config dir, and a per-user runtime dir (future sidecar sockets).
#[derive(Debug, Clone)]
pub struct Config {
    pub bind: SocketAddr,
    pub data_dir: PathBuf,
    pub config_dir: PathBuf,
    pub runtime_dir: PathBuf,
    /// Optional path to a built web client (client/dist) for packaged serving.
    pub static_dir: Option<PathBuf>,
}

impl Config {
    pub fn resolve() -> Result<Self> {
        let bind: SocketAddr = std::env::var("ASM_BIND")
            .unwrap_or_else(|_| "127.0.0.1:4600".to_string())
            .parse()
            .context("invalid ASM_BIND address")?;

        let (data_dir, config_dir, runtime_dir) = match ProjectDirs::from("dev", "agentsm", "asm") {
            Some(dirs) => {
                let data = dirs.data_dir().to_path_buf();
                let config = dirs.config_dir().to_path_buf();
                // runtime_dir is None on macOS/Windows; fall back to data_dir/run.
                let runtime = dirs
                    .runtime_dir()
                    .map(|p| p.to_path_buf())
                    .unwrap_or_else(|| data.join("run"));
                (data, config, runtime)
            }
            None => {
                let base = PathBuf::from(".asm");
                (base.join("data"), base.join("config"), base.join("run"))
            }
        };

        // Allow overrides (useful for tests and multi-instance dev).
        let data_dir = env_path("ASM_DATA_DIR").unwrap_or(data_dir);
        let config_dir = env_path("ASM_CONFIG_DIR").unwrap_or(config_dir);
        let runtime_dir = env_path("ASM_RUNTIME_DIR").unwrap_or(runtime_dir);
        let static_dir = env_path("ASM_STATIC_DIR");

        std::fs::create_dir_all(&data_dir)
            .with_context(|| format!("creating data dir {}", data_dir.display()))?;
        std::fs::create_dir_all(&config_dir)
            .with_context(|| format!("creating config dir {}", config_dir.display()))?;
        std::fs::create_dir_all(&runtime_dir)
            .with_context(|| format!("creating runtime dir {}", runtime_dir.display()))?;

        Ok(Self {
            bind,
            data_dir,
            config_dir,
            runtime_dir,
            static_dir,
        })
    }

    pub fn db_path(&self) -> PathBuf {
        self.data_dir.join("asm.sqlite3")
    }
}

fn env_path(key: &str) -> Option<PathBuf> {
    std::env::var_os(key).map(PathBuf::from)
}
