use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use serde::Serialize;

pub mod builtin;

/// Where the agent should run and any user-supplied launch inputs.
#[derive(Debug, Clone, Default)]
pub struct AgentContext {
    pub cwd: String,
    /// For `custom_command`: the program to execute.
    pub command: Option<String>,
    pub extra_args: Vec<String>,
    pub extra_env: Vec<(String, String)>,
}

/// A resolved launch command produced by an agent plugin.
#[derive(Debug, Clone)]
pub struct LaunchSpec {
    pub command: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    /// Arbitrary custom commands require explicit user approval before launch.
    pub requires_approval: bool,
}

/// Compiled-in agent plugin. MVP uses static traits; no dynamic loading.
pub trait AgentPlugin: Send + Sync {
    fn id(&self) -> &'static str;
    fn display_name(&self) -> &'static str;
    fn supported_platforms(&self) -> &'static [&'static str];
    /// Resolve the agent binary path if present on this host.
    fn detect_binary(&self) -> Option<String>;
    fn build_launch(&self, ctx: &AgentContext) -> Result<LaunchSpec>;
}

/// Serializable plugin metadata for the client.
#[derive(Debug, Clone, Serialize)]
pub struct PluginInfo {
    pub id: String,
    pub display_name: String,
    pub supported_platforms: Vec<String>,
    pub available: bool,
    pub binary_path: Option<String>,
    pub supported_on_this_platform: bool,
}

/// Static registry of built-in agent plugins.
pub struct PluginRegistry {
    agents: Vec<Arc<dyn AgentPlugin>>,
}

impl PluginRegistry {
    pub fn with_builtins() -> Self {
        Self {
            agents: builtin::all(),
        }
    }

    pub fn agents(&self) -> &[Arc<dyn AgentPlugin>] {
        &self.agents
    }

    pub fn get(&self, id: &str) -> Option<Arc<dyn AgentPlugin>> {
        self.agents.iter().find(|p| p.id() == id).cloned()
    }

    pub fn describe(&self) -> Vec<PluginInfo> {
        let platform = current_platform();
        self.agents
            .iter()
            .map(|p| {
                let supported = p.supported_platforms().contains(&platform);
                let binary_path = p.detect_binary();
                PluginInfo {
                    id: p.id().to_string(),
                    display_name: p.display_name().to_string(),
                    supported_platforms: p
                        .supported_platforms()
                        .iter()
                        .map(|s| s.to_string())
                        .collect(),
                    available: supported && binary_path.is_some(),
                    binary_path,
                    supported_on_this_platform: supported,
                }
            })
            .collect()
    }
}

/// This host's platform tag, matching `supported_platforms` entries.
pub fn current_platform() -> &'static str {
    if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        "unknown"
    }
}

/// Minimal cross-platform PATH search (the `which` behaviour we need).
pub fn find_in_path(program: &str) -> Option<String> {
    // Absolute or relative path: check directly.
    let direct = PathBuf::from(program);
    if direct.components().count() > 1 || direct.is_absolute() {
        return is_executable(&direct).then(|| direct.to_string_lossy().into_owned());
    }

    let path = std::env::var_os("PATH")?;
    let exts: Vec<String> = if cfg!(target_os = "windows") {
        std::env::var("PATHEXT")
            .unwrap_or_else(|_| ".EXE;.CMD;.BAT;.COM".to_string())
            .split(';')
            .map(|s| s.to_string())
            .collect()
    } else {
        vec![String::new()]
    };

    for dir in std::env::split_paths(&path) {
        for ext in &exts {
            let candidate = dir.join(format!("{program}{ext}"));
            if is_executable(&candidate) {
                return Some(candidate.to_string_lossy().into_owned());
            }
        }
    }
    None
}

fn is_executable(path: &std::path::Path) -> bool {
    if !path.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        match std::fs::metadata(path) {
            Ok(m) => m.permissions().mode() & 0o111 != 0,
            Err(_) => false,
        }
    }
    #[cfg(not(unix))]
    {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_has_all_builtins() {
        let reg = PluginRegistry::with_builtins();
        for id in ["shell", "codex", "claude", "custom_command"] {
            assert!(reg.get(id).is_some(), "missing builtin {id}");
        }
        assert!(reg.get("does-not-exist").is_none());
    }

    #[test]
    fn custom_command_requires_approval() {
        let reg = PluginRegistry::with_builtins();
        let plugin = reg.get("custom_command").unwrap();
        let ctx = AgentContext {
            cwd: "/tmp".into(),
            command: Some("echo hi".into()),
            ..Default::default()
        };
        let launch = plugin.build_launch(&ctx).unwrap();
        assert!(launch.requires_approval);
    }

    #[test]
    fn shell_does_not_require_approval() {
        let reg = PluginRegistry::with_builtins();
        let plugin = reg.get("shell").unwrap();
        let launch = plugin.build_launch(&AgentContext::default()).unwrap();
        assert!(!launch.requires_approval);
    }

    #[test]
    fn custom_command_without_command_errors() {
        let reg = PluginRegistry::with_builtins();
        let plugin = reg.get("custom_command").unwrap();
        assert!(plugin.build_launch(&AgentContext::default()).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn find_in_path_resolves_absolute() {
        // /bin/sh exists on any POSIX host.
        assert_eq!(find_in_path("/bin/sh"), Some("/bin/sh".to_string()));
        assert!(find_in_path("/nonexistent/binary/xyz").is_none());
    }
}
