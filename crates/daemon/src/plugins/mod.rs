use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use serde::Serialize;

pub(crate) mod attention;
pub mod builtin;
pub mod conversation;
pub mod usage;

use crate::domain::AttentionState;
use usage::{AgentUsage, TranscriptContext};

/// User-supplied launch inputs for an agent. The working directory is resolved
/// separately by the session manager and passed to the backend spawn spec.
#[derive(Debug, Clone, Default)]
pub struct AgentContext {
    /// For `custom_command`: the program to execute.
    pub command: Option<String>,
    pub extra_args: Vec<String>,
    pub extra_env: Vec<(String, String)>,
    /// Selected agent-option toggles (see `AgentPlugin::options`), keyed by option key.
    pub options: Vec<(String, bool)>,
}

impl AgentContext {
    /// Whether a named agent option toggle is enabled.
    pub fn opt(&self, key: &str) -> bool {
        self.options.iter().any(|(k, v)| k == key && *v)
    }
}

/// A user-facing toggle an agent exposes in the new-session UI — e.g. a
/// permission-skipping flag. Selecting it makes the plugin inject the
/// corresponding CLI flag at launch.
#[derive(Debug, Clone, Serialize)]
pub struct AgentOption {
    pub key: String,
    pub label: String,
    pub description: String,
    /// Render with a danger/warning affordance (disables the agent's guardrails).
    pub danger: bool,
    pub default: bool,
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
    /// Optional per-agent toggles surfaced in the new-session dialog (e.g. a
    /// permission-skipping flag). Empty by default.
    fn options(&self) -> Vec<AgentOption> {
        Vec::new()
    }

    /// Whether a terminal bell (`0x07`) from this agent means "needs your
    /// attention" (agents ring it for an approval prompt or a completed turn).
    /// A plain shell rings the bell as UI noise, so this is **off by default**
    /// and only opted into by agents whose bells are meaningful.
    fn bell_means_attention(&self) -> bool {
        false
    }

    /// Classify the session's current attention state (working / idle / blocked)
    /// from recent terminal output. `text` is the rendered **visible screen**
    /// when [`attention_uses_screen`](Self::attention_uses_screen) is true, and
    /// otherwise the raw decoded **output tail**. `bell` is whether the latest
    /// output chunk rang a real terminal bell (already gated by
    /// [`bell_means_attention`](Self::bell_means_attention)).
    ///
    /// The default is the shared prompt/bell heuristic over the tail. A provider
    /// whose approval UI that heuristic can't read — Claude Code's boxed
    /// selection menu — overrides this together with `attention_uses_screen`.
    fn attention(&self, text: &str, bell: bool) -> (AttentionState, Option<String>) {
        attention::default_attention(text, bell)
    }

    /// Whether [`attention`](Self::attention) wants the rendered screen instead
    /// of the raw output tail. Rendering the screen costs a terminal snapshot
    /// per output chunk, so it's opt-in; providers the tail heuristic handles
    /// leave this false.
    fn attention_uses_screen(&self) -> bool {
        false
    }

    /// Called when a *working* session's output goes silent (the idle settle),
    /// with the rendered visible screen: return a reason if the agent stopped
    /// **on an error** (e.g. Claude Code's `API Error: …`, printed with no bell
    /// and no prompt) rather than at a ready prompt. `Some` settles the session
    /// to [`AttentionState::Error`] instead of `Idle`. The default — providers
    /// with no known stop-on-error rendering — never flags.
    fn idle_error(&self, _screen: &str) -> Option<String> {
        None
    }

    /// Also called at the idle settle, with the rendered visible screen: is the
    /// agent quiet because it is **still working** rather than done? The silence
    /// timer reads "no output" as "the turn is over, your move", which is wrong
    /// for an agent parked on a long tool call, blocked waiting on a sub-agent,
    /// or whose turn ended leaving background work running (Codex renders both —
    /// see [`attention::codex_still_working`]). `true` holds the session at
    /// [`AttentionState::Activity`]; the settle is retried on each later tick, so
    /// it lands on idle once the agent really is done. The default — providers
    /// with no such rendering — never holds.
    fn idle_busy(&self, _screen: &str) -> bool {
        false
    }

    /// Best-effort token/context usage for a running session, read from the
    /// agent's own on-disk transcript (mirrors its `/status` / `/usage`). Agents
    /// that don't persist usage return `None`.
    fn usage(&self, _cx: &TranscriptContext) -> Option<AgentUsage> {
        None
    }

    /// The session's conversation, rendered as Markdown from the agent's own
    /// on-disk transcript. `None` for agents that keep no transcript (a plain
    /// shell) or when none can be matched to this session — the transcript
    /// endpoint then serves the raw PTY stream instead.
    fn conversation(&self, _cx: &TranscriptContext) -> Option<String> {
        None
    }
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
    pub options: Vec<AgentOption>,
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
                    options: p.options(),
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
        for id in ["shell", "codex", "claude", "opencode", "custom_command"] {
            assert!(reg.get(id).is_some(), "missing builtin {id}");
        }
        assert!(reg.get("does-not-exist").is_none());
    }

    #[test]
    fn custom_command_requires_approval() {
        let reg = PluginRegistry::with_builtins();
        let plugin = reg.get("custom_command").unwrap();
        let ctx = AgentContext {
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
