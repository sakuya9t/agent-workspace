use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use serde::Serialize;

pub(crate) mod attention;
pub(crate) mod deck;
pub mod builtin;
pub mod conversation;
pub mod fork;
pub mod title;
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
    /// An explicit model override for this launch, injected as the agent's model
    /// flag (see [`AgentPlugin::model_args`]). `None` = start with no `--model`,
    /// so the agent uses whatever it is configured to default to. Only ever set
    /// when the user picked a model other than that default, so a mis-detected
    /// default can never change how a session launches.
    pub model: Option<String>,
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

/// One model a user can pick for an agent in the new-session / fork dialog.
/// `id` is what gets passed to the agent's model flag (a Claude alias like
/// `sonnet`, a Codex model id, or an opencode `provider/model`); `label` is the
/// human-facing name — often the same string, but a friendlier one when we have it.
#[derive(Debug, Clone, Serialize)]
pub struct AgentModel {
    pub id: String,
    pub label: String,
}

impl AgentModel {
    /// A model whose id doubles as its display label.
    fn plain(id: &str) -> Self {
        AgentModel {
            id: id.to_string(),
            label: id.to_string(),
        }
    }
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

/// A one-shot, non-interactive run of an agent CLI, used to summarize a forked
/// session's digest into a handoff brief (see [`crate::summarize`]).
///
/// This is a *full agent* running headless, not a chat completion: it can call
/// tools and edit files. The caller runs it in a throwaway directory with stdin
/// closed and a deadline — never in the user's worktree.
#[derive(Debug, Clone)]
pub struct HeadlessSpec {
    pub command: String,
    pub args: Vec<String>,
    /// Where the agent writes its final answer, for CLIs that can be told to
    /// (`codex exec -o`). `None` means the answer lands on stdout, which for
    /// some CLIs also carries a banner and ANSI — see [`crate::summarize`].
    pub output_file: Option<PathBuf>,
}

/// A one-shot, non-interactive run of an agent CLI that edits files *in place* in
/// its working directory, used to auto-resolve merge/rebase conflicts (see
/// [`crate::conflict_resolve`]).
///
/// Unlike [`HeadlessSpec`], which the caller runs in a throwaway directory with
/// tools forbidden, this runs in the *real conflicted worktree* with the agent's
/// permission prompts bypassed — that is the whole point: the agent must be free
/// to rewrite the conflicted files and the daemon then verifies the result.
#[derive(Debug, Clone)]
pub struct ConflictResolveSpec {
    pub command: String,
    pub args: Vec<String>,
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

    /// Whether this agent lets the user pick a model at launch, so the dialog
    /// renders a model dropdown for it. Off for a plain shell and for
    /// `custom_command` (whose model, if any, lives in the raw command).
    fn supports_models(&self) -> bool {
        false
    }

    /// Selectable models for the new-session / fork dropdown, in display order.
    /// May be empty even when [`supports_models`](Self::supports_models) is true
    /// (an agent whose models can't be enumerated offers only its detected default
    /// plus a free-text "Custom…"). Called off the request path, so an agent that
    /// has to shell out to list its models — Codex/opencode — may do so here.
    fn models(&self) -> Vec<AgentModel> {
        Vec::new()
    }

    /// The model this agent is currently configured to default to on *this* host,
    /// read from the agent's own config. Preselected in the dropdown and shown as
    /// its default entry; `None` when nothing is configured (the agent falls back
    /// to its own built-in default). This is **display-only** — the default entry
    /// launches with no model flag — so a stale or mis-parsed value here can never
    /// change how a session actually starts.
    fn detect_default_model(&self) -> Option<String> {
        None
    }

    /// The argv fragment that pins this agent to `model`, e.g. `["--model", m]`
    /// for Claude/opencode or `["-m", m]` for Codex. Empty = this agent has no
    /// model selector, so the caller injects nothing.
    fn model_args(&self, _model: &str) -> Vec<String> {
        Vec::new()
    }

    /// Whether a terminal bell (`0x07`) from this agent means "needs your
    /// attention" (agents ring it for an approval prompt or a completed turn).
    /// A plain shell rings the bell as UI noise, so this is **off by default**
    /// and only opted into by agents whose bells are meaningful.
    fn bell_means_attention(&self) -> bool {
        false
    }

    /// Whether the monitor classifies this agent's output into attention
    /// states (working / idle / blocked / error) at all. On for agent TUIs,
    /// whose prompts and turn boundaries the daemon can read; **off for a
    /// plain shell**, where the user drives the terminal themselves and any
    /// state derived from its output is noise (`password:` at a shell prompt
    /// is not an approval gate we manage). A non-tracking session stays at
    /// [`AttentionState::None`] for its whole life — no badge, ever.
    fn tracks_attention(&self) -> bool {
        true
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

    /// Best-effort human title for the session — the name the agent's own LLM
    /// gave it, falling back to the first user prompt (see [`title`]). `None`
    /// for agents with no on-disk record of their own; the session list then
    /// falls back to workspace/directory naming.
    fn title(&self, _cx: &TranscriptContext) -> Option<String> {
        None
    }

    /// This agent's *own* conversation id for the session that ran in `cx.cwd`,
    /// read from the same on-disk transcript [`usage`] reads. `None` = the agent
    /// keeps no resumable conversation (a plain shell), or none can be matched.
    ///
    /// Called repeatedly while the session is alive; the first unique `Some` is
    /// persisted (see [`crate::domain::Session::agent_session_id`]).
    fn native_session_id(&self, _cx: &TranscriptContext) -> Option<String> {
        None
    }

    /// Correct a persisted native id only when the plugin can prove that it
    /// belongs to another session and can identify this session's replacement.
    /// The daemon calls this only for duplicate bindings left by an older
    /// matcher. `None` preserves the existing id: absence of a transcript is not
    /// evidence that a binding is wrong.
    fn replacement_native_session_id(&self, _cx: &TranscriptContext) -> Option<String> {
        None
    }

    /// Launch this agent so it continues the conversation `native_id`, as a
    /// **fork**: a new conversation seeded with the old one, leaving the origin's
    /// transcript untouched. `None` = no native resume; the caller falls back to
    /// [`Continuity::Brief`].
    ///
    /// `seed` is the opening prompt, and every agent that supports this also
    /// accepts one alongside the resume — so a native fork still boots saying
    /// what it understands rather than silently picking up mid-thought.
    ///
    /// `cwd` is the session manager's resolved destination. Native histories may
    /// remember their source directory, so plugins must explicitly apply this
    /// destination when their CLI would otherwise restore the old one.
    fn build_fork(
        &self,
        _ctx: &AgentContext,
        _native_id: &str,
        _seed: &str,
        _cwd: &Path,
    ) -> Option<Result<LaunchSpec>> {
        None
    }

    /// A compact, deterministic digest of the session's conversation: what the
    /// user asked for (verbatim), what the agent changed, and where it left off.
    ///
    /// Deliberately not an LLM summary. It is built straight from the agent's own
    /// transcript, so it is exact, instant, free and offline — and it stays small
    /// (a few thousand tokens even for a session whose transcript runs to tens of
    /// MB), which is what makes it safe to hand to *any* agent, and cheap enough
    /// to summarize further if [`crate::summarize`] is available.
    fn digest(&self, _cx: &TranscriptContext) -> Option<String> {
        None
    }

    /// A non-interactive run of this agent that answers `prompt`, writing its
    /// final answer to `out` if the CLI supports being told to. `None` = this
    /// agent has no headless mode, so it can't be used as a summarizer.
    ///
    /// See [`HeadlessSpec`] — the result is a full agent, and must be run
    /// sandboxed by the caller.
    fn headless(&self, _prompt: &str, _out: &Path) -> Option<HeadlessSpec> {
        None
    }

    /// A non-interactive run of this agent that resolves merge/rebase conflicts by
    /// editing files in its working directory. `None` = this agent can't be driven
    /// that way, so it can't auto-resolve conflicts.
    ///
    /// See [`ConflictResolveSpec`]: this runs in the real conflicted worktree with
    /// permission prompts bypassed, so it must only ever be handed a directory the
    /// caller is willing to let the agent rewrite.
    fn conflict_resolver(&self, _prompt: &str) -> Option<ConflictResolveSpec> {
        None
    }

    /// How this agent takes an opening prompt on its launch line, so a forked
    /// session can start already pointed at its brief. Returns the argv to append
    /// for `prompt`, or `None` if the agent cannot be seeded this way.
    ///
    /// **`None` by default, and that default is load-bearing.** A shell would treat
    /// a trailing argument as a *script to run*, so handing one a seed prompt
    /// would execute the brief instead of reading it.
    ///
    /// The encoding is per-agent, not universal: Claude and Codex read a bare
    /// positional as the prompt, but opencode's positional is a *project
    /// directory* — a prompt there is taken as a path and the launch dies with
    /// "Failed to change directory to …", so it must go through `--prompt`.
    fn seed_prompt_args(&self, _prompt: &str) -> Option<Vec<String>> {
        None
    }

    /// Whether [`build_fork`](Self::build_fork) only finds the origin's
    /// conversation when the fork runs in the origin's own working directory.
    ///
    /// True for Claude Code, which keys transcripts by cwd
    /// (`~/.claude/projects/<encoded-cwd>/`): resuming from a fresh worktree would
    /// look in the wrong directory and find nothing. False for Codex, whose
    /// rollouts are addressed by uuid from anywhere on the box. The fork planner
    /// reads this to decide whether a native fork is even on the table.
    fn native_fork_requires_same_cwd(&self) -> bool {
        false
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

/// The set of models a client can choose for one agent, for the new-session /
/// fork dropdown. Built per-host on request (see `GET /api/plugins/:id/models`).
#[derive(Debug, Clone, Serialize)]
pub struct PluginModels {
    /// Whether this agent offers a model dropdown at all. When false the client
    /// renders no model control and `models`/`default` are empty.
    pub supported: bool,
    /// Enumerable models, in display order. May be empty for a supported agent
    /// whose models can't be listed — the dropdown then offers just the detected
    /// default and a free-text "Custom…".
    pub models: Vec<AgentModel>,
    /// The agent's currently configured default model on this host, preselected
    /// in the dropdown. `None` = fall back to a generic "Default" entry.
    pub default: Option<String>,
}

impl PluginModels {
    /// Describe an agent's models by asking the plugin. Runs the plugin's
    /// (possibly subprocess-backed) `models()` and config detection, so callers
    /// run it off the async runtime.
    pub fn describe(plugin: &dyn AgentPlugin) -> Self {
        if !plugin.supports_models() {
            return PluginModels {
                supported: false,
                models: Vec::new(),
                default: None,
            };
        }
        PluginModels {
            supported: true,
            models: plugin.models(),
            default: plugin.detect_default_model(),
        }
    }
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

    /// A registry with no agents — for tests that need the "no capable agent"
    /// path without depending on which CLIs happen to be installed.
    #[cfg(test)]
    pub fn empty() -> Self {
        Self { agents: Vec::new() }
    }

    pub fn get(&self, id: &str) -> Option<Arc<dyn AgentPlugin>> {
        self.agents.iter().find(|p| p.id() == id).cloned()
    }

    /// Every registered plugin, in registration order.
    pub fn agents(&self) -> &[Arc<dyn AgentPlugin>] {
        &self.agents
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
///
/// On Windows a bare name is tried with each launchable `PATHEXT` extension
/// (`claude` → `claude.exe`, `codex` → `codex.cmd`), and a name that already
/// has one (`git.exe`) is also tried as given.
pub fn find_in_path(program: &str) -> Option<String> {
    let exts = launch_extensions(program);
    let found = |base: &std::path::Path| {
        exts.iter()
            .map(|ext| {
                let mut candidate = base.as_os_str().to_owned();
                candidate.push(ext);
                PathBuf::from(candidate)
            })
            .find(|c| is_executable(c))
            .map(|c| c.to_string_lossy().into_owned())
    };

    // Absolute or relative path: check directly.
    let direct = PathBuf::from(program);
    if direct.components().count() > 1 || direct.is_absolute() {
        return found(&direct);
    }

    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).find_map(|dir| found(&dir.join(program)))
}

/// How to start `command` with `args` so every argument reaches the program
/// verbatim.
///
/// On Windows `codex`, `opencode` and `pi` install through npm as `.cmd`
/// shims, and Windows runs a `.cmd` only through `cmd.exe` — which re-parses
/// the whole command line, so `&`, `|`, `^` and `%VAR%` inside an argument are
/// cmd syntax to it. The daemon passes free text as arguments (workspace
/// instructions, a fork's handoff brief, a summary prompt), so an npm shim is
/// resolved to what it wraps — `node <script>`, or the binary it points at —
/// and that is launched instead. Anything else, and everything on Unix, is
/// returned as given.
pub fn direct_launch(command: &str, args: Vec<String>) -> (String, Vec<String>) {
    #[cfg(windows)]
    if let Some((program, mut lead)) = npm_shim_target(std::path::Path::new(command)) {
        lead.extend(args);
        return (program, lead);
    }
    (command.to_string(), args)
}

/// A `std::process::Command` for an agent binary, via [`direct_launch`].
pub fn agent_command(program: &str, args: &[String]) -> std::process::Command {
    let (program, args) = direct_launch(program, args.to_vec());
    let mut cmd = std::process::Command::new(program);
    cmd.args(args);
    cmd
}

/// The program (and leading arguments) an npm cmd-shim runs, or `None` for a
/// file that is not one we can see through.
#[cfg(windows)]
fn npm_shim_target(shim: &std::path::Path) -> Option<(String, Vec<String>)> {
    let ext = shim.extension()?.to_str()?.to_ascii_lowercase();
    if ext != "cmd" && ext != "bat" {
        return None;
    }
    let text = std::fs::read_to_string(shim).ok()?;
    let dir = shim.parent()?;
    let target = crate::util::canonicalize(&dir.join(parse_npm_shim(&text)?)).ok()?;
    if !target.is_file() {
        return None;
    }
    let target_str = target.to_string_lossy().into_owned();
    if target
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("exe"))
    {
        return Some((target_str, Vec::new()));
    }
    // A script: only a shim that runs it with node is one we can replay
    // (cmd-shim also wraps `sh` scripts, which we leave to the shim).
    if !text.to_ascii_lowercase().contains("node") {
        return None;
    }
    // The shim prefers a node.exe sitting beside it, then PATH's.
    let local = dir.join("node.exe");
    let node = if local.is_file() {
        local.to_string_lossy().into_owned()
    } else {
        find_in_path("node")?
    };
    Some((node, vec![target_str]))
}

/// The target of an npm cmd-shim: the last `"%dp0%\…"` (npm ≥ 7) or
/// `"%~dp0\…"` (older) quoted path on a line that forwards `%*`.
#[cfg(any(windows, test))]
fn parse_npm_shim(text: &str) -> Option<String> {
    for line in text.lines().rev().filter(|l| l.contains("%*")) {
        let found = ["\"%dp0%\\", "\"%~dp0\\"]
            .iter()
            .filter_map(|prefix| line.rfind(prefix).map(|at| (at, prefix.len())))
            .max_by_key(|(at, _)| *at);
        if let Some((at, len)) = found {
            let rest = &line[at + len..];
            let end = rest.find('"')?;
            return Some(rest[..end].to_string());
        }
    }
    None
}

/// The suffixes [`find_in_path`] tries after `program`: just `""` on Unix. On
/// Windows, the `PATHEXT` entries a console can launch directly — never
/// `.js`/`.vbs`/… script associations, which `CreateProcess` cannot run — and
/// `""` first when `program` already ends in one of them.
fn launch_extensions(program: &str) -> Vec<String> {
    if !cfg!(windows) {
        return vec![String::new()];
    }
    const LAUNCHABLE: [&str; 4] = [".COM", ".EXE", ".BAT", ".CMD"];
    let pathext = std::env::var("PATHEXT").unwrap_or_else(|_| LAUNCHABLE.join(";"));
    // Matched case-insensitively, but spelled lowercase: the filesystem doesn't
    // care, and `claude.exe` is what the file is actually called.
    let mut exts: Vec<String> = pathext
        .split(';')
        .map(|e| e.trim().to_ascii_uppercase())
        .filter(|e| LAUNCHABLE.contains(&e.as_str()))
        .map(|e| e.to_ascii_lowercase())
        .collect();
    let upper = program.to_ascii_uppercase();
    if LAUNCHABLE.iter().any(|e| upper.ends_with(e)) {
        exts.insert(0, String::new());
    }
    exts
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
        for id in ["shell", "codex", "claude", "opencode", "pi", "custom_command"] {
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

    #[test]
    fn npm_shim_targets_are_parsed_from_both_shim_formats() {
        // npm ≥ 7 (cmd-shim 4+).
        let modern = "@ECHO off\r\nGOTO start\r\n:find_dp0\r\nSET dp0=%~dp0\r\nEXIT /b\r\n:start\r\nSETLOCAL\r\nCALL :find_dp0\r\n\r\nIF EXIST \"%dp0%\\node.exe\" (\r\n  SET \"_prog=%dp0%\\node.exe\"\r\n) ELSE (\r\n  SET \"_prog=node\"\r\n  SET PATHEXT=%PATHEXT:;.JS;=;%\r\n)\r\n\r\nendLocal & goto #_undefined_# 2>NUL || title %COMSPEC% & \"%_prog%\"  \"%dp0%\\node_modules\\@openai\\codex\\bin\\codex.js\" %*\r\n";
        assert_eq!(
            parse_npm_shim(modern).as_deref(),
            Some("node_modules\\@openai\\codex\\bin\\codex.js")
        );
        // npm 6.
        let legacy = "@IF EXIST \"%~dp0\\node.exe\" (\r\n  \"%~dp0\\node.exe\"  \"%~dp0\\..\\pi\\dist\\cli.js\" %*\r\n) ELSE (\r\n  @SETLOCAL\r\n  @SET PATHEXT=%PATHEXT:;.JS;=;%\r\n  node  \"%~dp0\\..\\pi\\dist\\cli.js\" %*\r\n)\r\n";
        assert_eq!(parse_npm_shim(legacy).as_deref(), Some("..\\pi\\dist\\cli.js"));
        // Not a shim at all.
        assert_eq!(parse_npm_shim("@echo off\r\necho hi %*\r\n"), None);
    }

    #[cfg(windows)]
    #[test]
    fn an_npm_shim_launches_its_script_under_node_with_args_untouched() {
        let dir = std::env::temp_dir().join(format!("asm-npm-shim-{}", uuid::Uuid::new_v4()));
        let pkg = dir.join("node_modules").join("@acme").join("agent").join("bin");
        std::fs::create_dir_all(&pkg).unwrap();
        std::fs::write(pkg.join("agent.js"), "// the agent").unwrap();
        std::fs::write(dir.join("node.exe"), "").unwrap(); // the shim's own node
        std::fs::write(
            dir.join("agent.cmd"),
            "@ECHO off\r\nSETLOCAL\r\nSET dp0=%~dp0\r\nIF EXIST \"%dp0%\\node.exe\" (SET \"_prog=%dp0%\\node.exe\") ELSE (SET \"_prog=node\")\r\n\"%_prog%\"  \"%dp0%\\node_modules\\@acme\\agent\\bin\\agent.js\" %*\r\n",
        )
        .unwrap();
        let shim = dir.join("agent.cmd").to_string_lossy().into_owned();
        let tricky = "a & b | c %PATH% ^ \"quoted\"".to_string();
        let (program, args) = direct_launch(&shim, vec!["-p".into(), tricky.clone()]);
        assert!(program.to_ascii_lowercase().ends_with("node.exe"), "{program}");
        assert_eq!(args.len(), 3);
        assert!(args[0].to_ascii_lowercase().ends_with("agent.js"), "{:?}", args);
        assert_eq!(&args[1..], &["-p".to_string(), tricky]);

        // A plain executable, or a .cmd that is not an npm shim, is left alone.
        assert_eq!(direct_launch("C:\\x\\claude.exe", vec![]).0, "C:\\x\\claude.exe");
        std::fs::write(dir.join("plain.cmd"), "@echo off\r\necho %*\r\n").unwrap();
        let plain = dir.join("plain.cmd").to_string_lossy().into_owned();
        assert_eq!(direct_launch(&plain, vec!["x".into()]), (plain.clone(), vec!["x".to_string()]));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(windows)]
    #[test]
    fn find_in_path_applies_pathext_on_windows() {
        let cmd = find_in_path("cmd").expect("cmd.exe is on every Windows PATH");
        assert!(cmd.to_ascii_lowercase().ends_with("cmd.exe"), "{cmd}");
        // Already carrying its extension: found as given, not as `cmd.exe.EXE`.
        let exact = find_in_path("cmd.exe").expect("cmd.exe by its full name");
        assert!(exact.to_ascii_lowercase().ends_with("cmd.exe"), "{exact}");
        // An absolute path without its extension still resolves.
        let stem = exact.trim_end_matches(|c| c != '.').trim_end_matches('.');
        assert_eq!(find_in_path(stem).map(|s| s.to_ascii_lowercase()), Some(exact.to_ascii_lowercase()));
        assert!(find_in_path(r"C:\nonexistent\binary\xyz").is_none());

        // Script associations in PATHEXT are not launchable by CreateProcess.
        let dir = std::env::temp_dir().join(format!("asm-pathext-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("asm-only-js.js"), "// not a program").unwrap();
        std::fs::write(dir.join("asm-shim.cmd"), "@echo off").unwrap();
        let base = dir.join("asm-only-js");
        assert!(find_in_path(&base.to_string_lossy()).is_none());
        let shim = find_in_path(&dir.join("asm-shim").to_string_lossy()).unwrap();
        assert!(shim.to_ascii_lowercase().ends_with("asm-shim.cmd"), "{shim}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
