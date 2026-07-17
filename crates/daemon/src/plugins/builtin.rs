use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};

use super::conversation;
use super::fork;
use super::title;
use super::usage::{self, AgentUsage, TranscriptContext};
use super::{
    attention, find_in_path, AgentContext, AgentModel, AgentOption, AgentPlugin, HeadlessSpec,
    LaunchSpec,
};
use crate::domain::AttentionState;

const ALL_PLATFORMS: &[&str] = &["linux", "macos", "windows"];

/// The full set of built-in agent plugins registered at startup.
pub fn all() -> Vec<Arc<dyn AgentPlugin>> {
    vec![
        Arc::new(ShellPlugin),
        Arc::new(CodexPlugin),
        Arc::new(ClaudePlugin),
        Arc::new(OpencodePlugin),
        Arc::new(CustomCommandPlugin),
    ]
}

/// Default interactive shell. Handy for smoke tests and the alpha loop.
pub struct ShellPlugin;

impl AgentPlugin for ShellPlugin {
    fn id(&self) -> &'static str {
        "shell"
    }
    fn display_name(&self) -> &'static str {
        "Shell"
    }
    fn supported_platforms(&self) -> &'static [&'static str] {
        ALL_PLATFORMS
    }
    fn detect_binary(&self) -> Option<String> {
        Some(default_shell())
    }
    // A shell has no turns to classify — the user is the one driving it.
    fn tracks_attention(&self) -> bool {
        false
    }
    fn build_launch(&self, ctx: &AgentContext) -> Result<LaunchSpec> {
        let command = default_shell();
        let args = if cfg!(target_os = "windows") {
            ctx.extra_args.clone()
        } else {
            // Interactive login shell.
            let mut a = vec!["-i".to_string()];
            a.extend(ctx.extra_args.clone());
            a
        };
        Ok(LaunchSpec {
            command,
            args,
            env: ctx.extra_env.clone(),
            requires_approval: false,
        })
    }
}

/// Codex CLI agent.
pub struct CodexPlugin;

impl AgentPlugin for CodexPlugin {
    fn id(&self) -> &'static str {
        "codex"
    }
    fn display_name(&self) -> &'static str {
        "Codex"
    }
    fn supported_platforms(&self) -> &'static [&'static str] {
        ALL_PLATFORMS
    }
    fn detect_binary(&self) -> Option<String> {
        find_in_path("codex")
    }
    fn supports_models(&self) -> bool {
        true
    }
    fn model_args(&self, model: &str) -> Vec<String> {
        vec!["-m".to_string(), model.to_string()]
    }
    // Codex has no CLI to enumerate its models, so `models()` stays empty: the
    // dropdown offers the configured default plus a free-text "Custom…".
    fn detect_default_model(&self) -> Option<String> {
        detect_codex_model()
    }
    // Codex rings the terminal bell on *turn completion*, not only on approval
    // prompts, so the bell can't be trusted as a "needs you" signal — a finished
    // turn would read as blocked and stick there. Attention is driven off the
    // rendered screen instead (see `codex_attention`), so the bell is left off.
    fn attention_uses_screen(&self) -> bool {
        true
    }
    fn attention(&self, screen: &str, bell: bool) -> (AttentionState, Option<String>) {
        attention::codex_attention(screen, bell)
    }
    // Codex goes quiet while it is still working — blocked in `wait_agent` on a
    // sub-agent, or with a background terminal outliving the turn that started
    // it — so the silence timer alone would settle a busy session to idle.
    fn idle_busy(&self, screen: &str) -> bool {
        attention::codex_still_working(screen)
    }
    fn usage(&self, cx: &TranscriptContext) -> Option<AgentUsage> {
        usage::codex_usage(cx)
    }
    fn conversation(&self, cx: &TranscriptContext) -> Option<String> {
        conversation::codex_conversation(cx)
    }
    fn title(&self, cx: &TranscriptContext) -> Option<String> {
        title::codex_session_title(cx)
    }
    fn native_session_id(&self, cx: &TranscriptContext) -> Option<String> {
        fork::codex_native_id(cx)
    }
    fn digest(&self, cx: &TranscriptContext) -> Option<String> {
        fork::codex_digest(cx)
    }
    fn seed_prompt_args(&self, prompt: &str) -> Option<Vec<String>> {
        // `codex <prompt>` — a bare positional is the opening message.
        Some(vec![prompt.to_string()])
    }
    // `native_fork_requires_same_cwd` stays false: rollouts live under
    // `~/.codex/sessions/**` and are addressed by uuid, so unlike Claude, Codex
    // resumes fine from a brand-new worktree.
    fn options(&self) -> Vec<AgentOption> {
        vec![AgentOption {
            key: "bypass_approvals".into(),
            label: "Bypass approvals & sandbox".into(),
            description:
                "Launch with --dangerously-bypass-approvals-and-sandbox: no approval prompts and no sandbox."
                    .into(),
            danger: true,
            default: false,
        }]
    }
    fn build_launch(&self, ctx: &AgentContext) -> Result<LaunchSpec> {
        cli_launch(self, ctx, "bypass_approvals", "--dangerously-bypass-approvals-and-sandbox")
    }
    /// `codex fork <id> <prompt>` — a subcommand, so unlike Claude's flags it has
    /// to *lead* argv, ahead of the danger flag. This is the reason forking is a
    /// whole plugin method rather than a few extra args contributed to
    /// `build_launch`.
    fn build_fork(
        &self,
        ctx: &AgentContext,
        native_id: &str,
        seed: &str,
    ) -> Option<Result<LaunchSpec>> {
        Some(self.detect_binary().ok_or_else(|| anyhow!("`codex` binary not found in PATH")).map(
            |command| {
                let mut args = vec!["fork".to_string()];
                if ctx.opt("bypass_approvals") {
                    args.push("--dangerously-bypass-approvals-and-sandbox".to_string());
                }
                if let Some(model) = ctx.model.as_deref() {
                    args.extend(self.model_args(model));
                }
                args.push(native_id.to_string());
                args.push(seed.to_string());
                LaunchSpec {
                    command,
                    args,
                    env: ctx.extra_env.clone(),
                    requires_approval: false,
                }
            },
        ))
    }
    fn headless(&self, prompt: &str, out: &Path) -> Option<HeadlessSpec> {
        // `-o` writes just the final message. Without it the answer arrives on
        // stdout wrapped in progress output and a token-usage footer.
        Some(HeadlessSpec {
            command: self.detect_binary()?,
            args: vec![
                "exec".into(),
                "--color".into(),
                "never".into(),
                "-o".into(),
                out.to_string_lossy().into_owned(),
                prompt.to_string(),
            ],
            output_file: Some(out.to_path_buf()),
        })
    }
}

/// Claude Code CLI agent.
pub struct ClaudePlugin;

impl AgentPlugin for ClaudePlugin {
    fn id(&self) -> &'static str {
        "claude"
    }
    fn display_name(&self) -> &'static str {
        "Claude Code"
    }
    fn supported_platforms(&self) -> &'static [&'static str] {
        ALL_PLATFORMS
    }
    fn detect_binary(&self) -> Option<String> {
        find_in_path("claude")
    }
    fn supports_models(&self) -> bool {
        true
    }
    fn model_args(&self, model: &str) -> Vec<String> {
        vec!["--model".to_string(), model.to_string()]
    }
    fn models(&self) -> Vec<AgentModel> {
        // Claude takes a short alias for the latest of each family, or a full model
        // id via "Custom…". Aliases are stable where ids churn, so the curated list
        // is aliases only.
        ["fable", "opus", "sonnet", "haiku"]
            .iter()
            .map(|a| AgentModel::plain(a))
            .collect()
    }
    fn detect_default_model(&self) -> Option<String> {
        detect_claude_model()
    }
    fn bell_means_attention(&self) -> bool {
        true
    }
    fn attention_uses_screen(&self) -> bool {
        true
    }
    fn attention(&self, screen: &str, bell: bool) -> (AttentionState, Option<String>) {
        attention::claude_attention(screen, bell)
    }
    fn idle_error(&self, screen: &str) -> Option<String> {
        attention::claude_idle_error(screen)
    }
    fn usage(&self, cx: &TranscriptContext) -> Option<AgentUsage> {
        usage::claude_usage(cx)
    }
    fn conversation(&self, cx: &TranscriptContext) -> Option<String> {
        conversation::claude_conversation(cx)
    }
    fn title(&self, cx: &TranscriptContext) -> Option<String> {
        title::claude_session_title(cx)
    }
    fn native_session_id(&self, cx: &TranscriptContext) -> Option<String> {
        fork::claude_native_id(cx)
    }
    fn digest(&self, cx: &TranscriptContext) -> Option<String> {
        fork::claude_digest(cx)
    }
    fn seed_prompt_args(&self, prompt: &str) -> Option<Vec<String>> {
        // `claude <prompt>` — a bare positional is the opening message.
        Some(vec![prompt.to_string()])
    }
    // `~/.claude/projects/<encoded-cwd>/`: a resume from a different directory
    // looks in a different project and finds nothing.
    fn native_fork_requires_same_cwd(&self) -> bool {
        true
    }
    fn options(&self) -> Vec<AgentOption> {
        vec![AgentOption {
            key: "skip_permissions".into(),
            label: "Skip permission prompts".into(),
            description:
                "Launch with --dangerously-skip-permissions: Claude Code won't ask before edits or commands."
                    .into(),
            danger: true,
            default: false,
        }]
    }
    fn build_launch(&self, ctx: &AgentContext) -> Result<LaunchSpec> {
        cli_launch(self, ctx, "skip_permissions", "--dangerously-skip-permissions")
    }
    /// `claude --resume <id> --fork-session <prompt>`. `--fork-session` is what
    /// keeps this a fork: it gives the new session its own id, so the origin's
    /// transcript is never appended to.
    ///
    /// Claude finds a resumed conversation by cwd, so this only works when the
    /// fork runs in the origin's directory. A fork onto a *new* worktree is a new
    /// cwd, and the caller falls back to the brief — see `SessionManager::fork_session`.
    fn build_fork(
        &self,
        ctx: &AgentContext,
        native_id: &str,
        seed: &str,
    ) -> Option<Result<LaunchSpec>> {
        Some(self.detect_binary().ok_or_else(|| anyhow!("`claude` binary not found in PATH")).map(
            |command| {
                let mut args = Vec::new();
                if ctx.opt("skip_permissions") {
                    args.push("--dangerously-skip-permissions".to_string());
                }
                if let Some(model) = ctx.model.as_deref() {
                    args.extend(self.model_args(model));
                }
                args.push("--resume".to_string());
                args.push(native_id.to_string());
                args.push("--fork-session".to_string());
                args.push(seed.to_string());
                LaunchSpec {
                    command,
                    args,
                    env: ctx.extra_env.clone(),
                    requires_approval: false,
                }
            },
        ))
    }
    fn headless(&self, prompt: &str, _out: &Path) -> Option<HeadlessSpec> {
        // `-p` prints the final answer to stdout, clean.
        Some(HeadlessSpec {
            command: self.detect_binary()?,
            args: vec!["-p".into(), prompt.to_string()],
            output_file: None,
        })
    }
}

/// opencode CLI agent (the default `opencode` TUI in the session's cwd).
pub struct OpencodePlugin;

impl AgentPlugin for OpencodePlugin {
    fn id(&self) -> &'static str {
        "opencode"
    }
    fn display_name(&self) -> &'static str {
        "opencode"
    }
    fn supported_platforms(&self) -> &'static [&'static str] {
        ALL_PLATFORMS
    }
    fn detect_binary(&self) -> Option<String> {
        find_in_path("opencode")
    }
    fn supports_models(&self) -> bool {
        true
    }
    fn model_args(&self, model: &str) -> Vec<String> {
        vec!["--model".to_string(), model.to_string()]
    }
    // opencode aggregates models across every configured provider and, unlike
    // Claude/Codex, can list them (`opencode models`) — so the dropdown shows the
    // real set on this host (`provider/model`) rather than a curated guess.
    fn models(&self) -> Vec<AgentModel> {
        list_opencode_models(self.detect_binary().as_deref())
    }
    fn detect_default_model(&self) -> Option<String> {
        detect_opencode_model()
    }
    fn bell_means_attention(&self) -> bool {
        true
    }
    fn title(&self, cx: &TranscriptContext) -> Option<String> {
        title::opencode_session_title(cx)
    }
    fn options(&self) -> Vec<AgentOption> {
        vec![AgentOption {
            key: "auto_approve".into(),
            label: "Auto-approve permissions".into(),
            description:
                "Launch with --auto: opencode auto-approves any permission that isn't explicitly denied."
                    .into(),
            danger: true,
            default: false,
        }]
    }
    fn build_launch(&self, ctx: &AgentContext) -> Result<LaunchSpec> {
        cli_launch(self, ctx, "auto_approve", "--auto")
    }
    fn seed_prompt_args(&self, prompt: &str) -> Option<Vec<String>> {
        // opencode's *positional* is a project directory, not a prompt — passing
        // the brief there makes it `chdir` into the prompt text and die with
        // "Failed to change directory to …". The opening message goes through the
        // TUI's `--prompt` flag instead.
        Some(vec!["--prompt".to_string(), prompt.to_string()])
    }
    // No `digest` / `native_session_id`: opencode keeps its conversation in a
    // SQLite db rather than a per-cwd transcript, so both need a schema read we
    // haven't written. A forked opencode session therefore gets the brief, and a
    // fork *of* one falls back to its raw terminal stream. It can still act as
    // the summarizer for other agents' forks, which is what `headless` is for.
    fn headless(&self, prompt: &str, _out: &Path) -> Option<HeadlessSpec> {
        Some(HeadlessSpec {
            command: self.detect_binary()?,
            args: vec!["run".into(), prompt.to_string()],
            output_file: None,
        })
    }
}

/// Arbitrary user-provided command. Requires explicit approval to launch.
pub struct CustomCommandPlugin;

impl AgentPlugin for CustomCommandPlugin {
    fn id(&self) -> &'static str {
        "custom_command"
    }
    fn display_name(&self) -> &'static str {
        "Custom Command"
    }
    fn supported_platforms(&self) -> &'static [&'static str] {
        ALL_PLATFORMS
    }
    fn detect_binary(&self) -> Option<String> {
        None
    }
    fn build_launch(&self, ctx: &AgentContext) -> Result<LaunchSpec> {
        let command = ctx
            .command
            .clone()
            .ok_or_else(|| anyhow!("custom_command requires a `command`"))?;
        let resolved = find_in_path(&command).unwrap_or(command);
        Ok(LaunchSpec {
            command: resolved,
            args: ctx.extra_args.clone(),
            env: ctx.extra_env.clone(),
            requires_approval: true,
        })
    }
}

/// The `build_launch` shape shared by the CLI agents (codex/claude/opencode):
/// the detected binary, one optional danger flag, then the user's extra
/// args/env. These plugins' ids double as their binary names.
fn cli_launch(
    plugin: &dyn AgentPlugin,
    ctx: &AgentContext,
    opt_key: &str,
    flag: &str,
) -> Result<LaunchSpec> {
    let command = plugin
        .detect_binary()
        .ok_or_else(|| anyhow!("`{}` binary not found in PATH", plugin.id()))?;
    let mut args = Vec::new();
    if ctx.opt(opt_key) {
        args.push(flag.to_string());
    }
    if let Some(model) = ctx.model.as_deref() {
        args.extend(plugin.model_args(model));
    }
    args.extend(ctx.extra_args.clone());
    Ok(LaunchSpec {
        command,
        args,
        env: ctx.extra_env.clone(),
        requires_approval: false,
    })
}

fn default_shell() -> String {
    if cfg!(target_os = "windows") {
        std::env::var("COMSPEC").unwrap_or_else(|_| "powershell.exe".to_string())
    } else {
        std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string())
    }
}

// ---- Model detection (which model each agent is configured to default to) ----
//
// All best-effort and display-only: the value only preselects the dropdown's
// "Default" entry, which launches with no model flag, so a stale or wrong read
// here never changes how a session actually starts (see `AgentContext::model`).

/// `$XDG_CONFIG_HOME`, else `~/.config`.
fn config_home() -> Option<PathBuf> {
    match std::env::var("XDG_CONFIG_HOME") {
        Ok(x) if !x.is_empty() => Some(PathBuf::from(x)),
        _ => Some(usage::home_dir()?.join(".config")),
    }
}

/// Claude's default model: `$ANTHROPIC_MODEL` wins (it overrides the CLI at
/// runtime), else the `"model"` field of `~/.claude/settings.json`.
fn detect_claude_model() -> Option<String> {
    if let Ok(m) = std::env::var("ANTHROPIC_MODEL") {
        let m = m.trim();
        if !m.is_empty() {
            return Some(m.to_string());
        }
    }
    let text = std::fs::read_to_string(usage::home_dir()?.join(".claude").join("settings.json"))
        .ok()?;
    parse_claude_settings_model(&text)
}

/// Codex's default model: the top-level `model` key of `~/.codex/config.toml`.
fn detect_codex_model() -> Option<String> {
    let text =
        std::fs::read_to_string(usage::home_dir()?.join(".codex").join("config.toml")).ok()?;
    parse_codex_config_model(&text)
}

/// opencode's default model: the `"model"` field of the user's global config.
/// opencode also reads project-local config and `$OPENCODE_CONFIG`, but the
/// daemon can't know the eventual cwd here, so the user default is the best
/// "current model" to preselect.
fn detect_opencode_model() -> Option<String> {
    let dir = config_home()?.join("opencode");
    for name in ["opencode.json", "opencode.jsonc"] {
        if let Ok(text) = std::fs::read_to_string(dir.join(name)) {
            if let Some(m) = parse_opencode_config_model(&text) {
                return Some(m);
            }
        }
    }
    None
}

/// Pull `"model"` out of Claude's `settings.json`.
fn parse_claude_settings_model(json: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    non_empty(v.get("model")?.as_str()?)
}

/// Pull the top-level `model = "…"` out of Codex's `config.toml` without a full
/// TOML parser: scan until the first table header, since a `model` under a
/// `[profiles.*]`/`[tui.*]` section is not the default one.
fn parse_codex_config_model(toml: &str) -> Option<String> {
    for raw in toml.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') {
            break;
        }
        // Must be exactly the `model` key: after stripping it, the remainder has
        // to begin with `=` (so `model_reasoning_effort` and the like don't match).
        if let Some(rest) = line.strip_prefix("model") {
            let rest = rest.trim_start();
            if let Some(value) = rest.strip_prefix('=') {
                return toml_string_value(value);
            }
        }
    }
    None
}

/// The string inside a TOML `= "…"` / `= '…'` assignment, ignoring any trailing
/// inline comment. Returns `None` for non-string values (which we don't expect
/// for `model`).
fn toml_string_value(s: &str) -> Option<String> {
    let s = s.trim();
    let mut chars = s.chars();
    let quote = match chars.next()? {
        q @ ('"' | '\'') => q,
        _ => return None,
    };
    let rest = &s[quote.len_utf8()..];
    let end = rest.find(quote)?;
    non_empty(&rest[..end])
}

/// Pull `"model"` out of an opencode config, tolerating JSONC comments.
fn parse_opencode_config_model(text: &str) -> Option<String> {
    let stripped = strip_jsonc_comments(text);
    let v: serde_json::Value = serde_json::from_str(&stripped).ok()?;
    non_empty(v.get("model")?.as_str()?)
}

/// Strip `//` and `/* */` comments from JSONC, leaving comment-looking sequences
/// *inside* strings alone (so a `"https://…"` value survives).
fn strip_jsonc_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let bytes = text.as_bytes();
    let mut i = 0;
    let mut in_string = false;
    while i < bytes.len() {
        let c = bytes[i];
        if in_string {
            out.push(c as char);
            if c == b'\\' && i + 1 < bytes.len() {
                out.push(bytes[i + 1] as char);
                i += 2;
                continue;
            }
            if c == b'"' {
                in_string = false;
            }
            i += 1;
            continue;
        }
        match c {
            b'"' => {
                in_string = true;
                out.push('"');
                i += 1;
            }
            b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'/' => {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'*' => {
                i += 2;
                while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                    i += 1;
                }
                i += 2;
            }
            _ => {
                out.push(c as char);
                i += 1;
            }
        }
    }
    out
}

// ---- opencode model enumeration (`opencode models`) ----

/// List opencode's available models by running `opencode models`. Empty on any
/// failure (missing binary, error exit, timeout) — the dropdown then falls back
/// to the detected default plus a free-text "Custom…".
fn list_opencode_models(binary: Option<&str>) -> Vec<AgentModel> {
    let Some(bin) = binary else {
        return Vec::new();
    };
    let mut cmd = Command::new(bin);
    cmd.arg("models");
    match run_capture(cmd, Duration::from_secs(10)) {
        Some(out) => parse_opencode_models_output(&out),
        None => Vec::new(),
    }
}

/// Parse `opencode models` stdout: one `provider/model` per line.
fn parse_opencode_models_output(stdout: &str) -> Vec<AgentModel> {
    stdout
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && l.contains('/') && !l.contains(char::is_whitespace))
        .map(AgentModel::plain)
        .collect()
}

/// Run a command and capture its stdout as a UTF-8 string, killing it if it runs
/// past `timeout`. `None` on spawn failure or timeout. Used off the async runtime.
fn run_capture(mut cmd: Command, timeout: Duration) -> Option<String> {
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut stdout = child.stdout.take()?;
    let (tx, rx) = mpsc::channel();
    let reader = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = stdout.read_to_string(&mut s);
        let _ = tx.send(s);
    });

    let deadline = Instant::now() + timeout;
    let timed_out = loop {
        match child.try_wait() {
            Ok(Some(_)) => break false,
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    break true;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(_) => {
                let _ = child.kill();
                break true;
            }
        }
    };

    // The reader finishes once the pipe closes (on exit or kill).
    let out = rx.recv_timeout(Duration::from_secs(1)).ok();
    let _ = reader.join();
    if timed_out {
        None
    } else {
        out
    }
}

/// `Some(trimmed)` if a string has non-whitespace content, else `None`.
fn non_empty(s: &str) -> Option<String> {
    let s = s.trim();
    (!s.is_empty()).then(|| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx_with_model(model: &str) -> AgentContext {
        AgentContext {
            model: Some(model.to_string()),
            ..Default::default()
        }
    }

    #[test]
    fn model_args_per_agent() {
        assert_eq!(ClaudePlugin.model_args("opus"), vec!["--model", "opus"]);
        assert_eq!(OpencodePlugin.model_args("a/b"), vec!["--model", "a/b"]);
        assert_eq!(CodexPlugin.model_args("gpt-5"), vec!["-m", "gpt-5"]);
        // A plain shell has no model selector.
        assert!(ShellPlugin.model_args("x").is_empty());
        assert!(!ShellPlugin.supports_models());
    }

    #[test]
    fn seed_prompt_encoding_is_per_agent() {
        // Claude and Codex read a bare positional as the opening message.
        assert_eq!(ClaudePlugin.seed_prompt_args("read the brief"), Some(vec!["read the brief".into()]));
        assert_eq!(CodexPlugin.seed_prompt_args("read the brief"), Some(vec!["read the brief".into()]));
        // opencode's positional is a project *directory*: a bare prompt would be
        // taken as a path and the launch would die with "Failed to change
        // directory to …", so its seed must go through `--prompt`.
        assert_eq!(
            OpencodePlugin.seed_prompt_args("read the brief"),
            Some(vec!["--prompt".into(), "read the brief".into()])
        );
        // A shell must not be seeded at all — it would run the brief as a script.
        assert_eq!(ShellPlugin.seed_prompt_args("read the brief"), None);
    }

    #[test]
    fn build_launch_injects_model_after_danger_flag() {
        // Skipping detect_binary-dependent asserts when the CLI is absent: the
        // arg ordering is what we care about, and that needs the binary present.
        if ClaudePlugin.detect_binary().is_none() {
            return;
        }
        let spec = ClaudePlugin.build_launch(&ctx_with_model("sonnet")).unwrap();
        let i = spec.args.iter().position(|a| a == "--model").expect("has --model");
        assert_eq!(spec.args[i + 1], "sonnet");
    }

    #[test]
    fn build_launch_without_model_has_no_flag() {
        if ClaudePlugin.detect_binary().is_none() {
            return;
        }
        let spec = ClaudePlugin.build_launch(&AgentContext::default()).unwrap();
        assert!(!spec.args.iter().any(|a| a == "--model"));
    }

    #[test]
    fn claude_fork_puts_model_before_resume() {
        if ClaudePlugin.detect_binary().is_none() {
            return;
        }
        let spec = ClaudePlugin
            .build_fork(&ctx_with_model("opus"), "conv-1", "hi")
            .unwrap()
            .unwrap();
        let model_at = spec.args.iter().position(|a| a == "--model").unwrap();
        let resume_at = spec.args.iter().position(|a| a == "--resume").unwrap();
        assert!(model_at < resume_at, "model flag must precede --resume");
        assert_eq!(spec.args[model_at + 1], "opus");
    }

    #[test]
    fn codex_fork_puts_model_before_positionals() {
        if CodexPlugin.detect_binary().is_none() {
            return;
        }
        let spec = CodexPlugin
            .build_fork(&ctx_with_model("gpt-x"), "conv-1", "hi")
            .unwrap()
            .unwrap();
        // `codex fork -m <model> <id> <seed>`: -m sits after the `fork` subcommand
        // and before the id/seed positionals.
        let m = spec.args.iter().position(|a| a == "-m").unwrap();
        let id = spec.args.iter().position(|a| a == "conv-1").unwrap();
        assert_eq!(spec.args[0], "fork");
        assert!(m < id);
        assert_eq!(spec.args[m + 1], "gpt-x");
    }

    #[test]
    fn parse_claude_settings_reads_model() {
        assert_eq!(
            parse_claude_settings_model(r#"{"model":"opus[1m]","other":1}"#).as_deref(),
            Some("opus[1m]")
        );
        assert_eq!(parse_claude_settings_model(r#"{"other":1}"#), None);
        assert_eq!(parse_claude_settings_model(r#"{"model":""}"#), None);
        assert_eq!(parse_claude_settings_model("not json"), None);
    }

    #[test]
    fn parse_codex_config_reads_top_level_model() {
        let toml = "model = \"gpt-5.6-sol\"\nmodel_reasoning_effort = \"xhigh\"\n[tui]\nmodel = \"nope\"\n";
        assert_eq!(parse_codex_config_model(toml).as_deref(), Some("gpt-5.6-sol"));
        // `model_reasoning_effort` must not be mistaken for `model`.
        assert_eq!(
            parse_codex_config_model("model_reasoning_effort = \"high\"\n"),
            None
        );
        // A trailing inline comment is ignored.
        assert_eq!(
            parse_codex_config_model("model = 'o3' # default\n").as_deref(),
            Some("o3")
        );
        // A `model` under a section is not the default.
        assert_eq!(parse_codex_config_model("[profiles.x]\nmodel = \"z\"\n"), None);
    }

    #[test]
    fn parse_opencode_config_tolerates_jsonc() {
        let jsonc = "{\n  // a comment with a // slash\n  \"$schema\": \"https://opencode.ai/config.json\",\n  \"model\": \"anthropic/claude-x\" /* inline */\n}\n";
        assert_eq!(
            parse_opencode_config_model(jsonc).as_deref(),
            Some("anthropic/claude-x")
        );
        // No model set (the real default on this box) → None.
        assert_eq!(
            parse_opencode_config_model("{ \"$schema\": \"https://opencode.ai/config.json\" }"),
            None
        );
    }

    #[test]
    fn strip_jsonc_keeps_urls_inside_strings() {
        let s = strip_jsonc_comments("{\"u\":\"https://x/y\"}");
        assert!(s.contains("https://x/y"), "url survived: {s}");
    }

    #[test]
    fn parse_opencode_models_filters_to_provider_slash_model() {
        let out = "opencode/big-pickle\nollama-cloud/glm-5\n\n  \nsome header line\n";
        let models = parse_opencode_models_output(out);
        let ids: Vec<_> = models.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(ids, vec!["opencode/big-pickle", "ollama-cloud/glm-5"]);
    }
}
