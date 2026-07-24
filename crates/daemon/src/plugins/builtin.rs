use std::io::{BufRead, BufReader, Read, Write};
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
    attention, find_in_path, AgentContext, AgentModel, AgentOption, AgentPlugin,
    ConflictResolveSpec, HeadlessSpec, LaunchSpec,
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
    fn models(&self) -> Vec<AgentModel> {
        // This is the same catalog the TUI's `/model` picker reads. The app
        // server accounts for the installed Codex version, auth and rollout
        // state, so a curated list here would inevitably drift.
        list_codex_models(self.detect_binary().as_deref())
    }
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
    /// `codex -C <cwd> fork <id> <prompt>` — `fork` is a subcommand (after the
    /// global cwd override), so unlike Claude's flags it has to precede the
    /// danger/model flags and positionals. This is the reason forking is a whole
    /// plugin method rather than a few extra args contributed to `build_launch`.
    fn build_fork(
        &self,
        ctx: &AgentContext,
        native_id: &str,
        seed: &str,
        cwd: &Path,
    ) -> Option<Result<LaunchSpec>> {
        Some(self.detect_binary().ok_or_else(|| anyhow!("`codex` binary not found in PATH")).map(
            |command| {
                LaunchSpec {
                    command,
                    args: codex_fork_args(ctx, native_id, seed, cwd),
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
    fn conflict_resolver(&self, prompt: &str) -> Option<ConflictResolveSpec> {
        // `exec` is codex's non-interactive mode; the bypass flag lets it edit the
        // conflicted files and run git in the worktree without stopping to ask.
        Some(ConflictResolveSpec {
            command: self.detect_binary()?,
            args: vec![
                "exec".into(),
                "--dangerously-bypass-approvals-and-sandbox".into(),
                "--color".into(),
                "never".into(),
                prompt.to_string(),
            ],
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
        _cwd: &Path,
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
    fn conflict_resolver(&self, prompt: &str) -> Option<ConflictResolveSpec> {
        // `-p` runs headless; skip-permissions lets it rewrite the conflicted
        // files and run git in the worktree without prompting.
        Some(ConflictResolveSpec {
            command: self.detect_binary()?,
            args: vec![
                "-p".into(),
                prompt.to_string(),
                "--dangerously-skip-permissions".into(),
            ],
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
    fn conflict_resolver(&self, prompt: &str) -> Option<ConflictResolveSpec> {
        // `run` is opencode's non-interactive mode; `--auto` auto-approves the
        // edits and git commands it needs to resolve the conflict in the worktree.
        Some(ConflictResolveSpec {
            command: self.detect_binary()?,
            args: vec!["run".into(), "--auto".into(), prompt.to_string()],
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

fn codex_fork_args(ctx: &AgentContext, native_id: &str, seed: &str, cwd: &Path) -> Vec<String> {
    // A persisted Codex conversation includes its original cwd. Without an
    // explicit override, `codex fork` restores that cwd even though the daemon
    // spawned the process in a new worktree: the TUI displays the source
    // directory and the agent edits and commits there. `-C` makes the session
    // manager's resolved fork destination win.
    let mut args = vec![
        "-C".to_string(),
        cwd.to_string_lossy().into_owned(),
        "fork".to_string(),
    ];
    if ctx.opt("bypass_approvals") {
        args.push("--dangerously-bypass-approvals-and-sandbox".to_string());
    }
    if let Some(model) = ctx.model.as_deref() {
        args.extend(CodexPlugin.model_args(model));
    }
    args.push(native_id.to_string());
    args.push(seed.to_string());
    args
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

// ---- Codex model enumeration (`codex app-server`, `model/list`) ----

/// Ask Codex's app server for the picker-visible catalog used by `/model`.
///
/// The server is a long-lived JSONL process, so this cannot use `run_capture`:
/// initialize the connection, page through `model/list`, then terminate it once
/// all pages arrive. Empty on an absent/older Codex, protocol error or timeout;
/// the dialog still retains its detected default and "Custom…" escape hatch.
fn list_codex_models(binary: Option<&str>) -> Vec<AgentModel> {
    let Some(bin) = binary else {
        return Vec::new();
    };

    let mut child = match Command::new(bin)
        .arg("app-server")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(_) => return Vec::new(),
    };
    let Some(mut stdin) = child.stdin.take() else {
        let _ = child.kill();
        let _ = child.wait();
        return Vec::new();
    };
    let Some(stdout) = child.stdout.take() else {
        let _ = child.kill();
        let _ = child.wait();
        return Vec::new();
    };

    let (tx, rx) = mpsc::channel();
    let reader = std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if let Ok(message) = serde_json::from_str::<serde_json::Value>(&line) {
                if tx.send(message).is_err() {
                    break;
                }
            }
        }
    });

    let result = (|| {
        write_json_line(
            &mut stdin,
            &serde_json::json!({
                "method": "initialize",
                "id": 0,
                "params": {
                    "clientInfo": {
                        "name": "agent_session_manager",
                        "title": "Agent Session Manager",
                        "version": env!("CARGO_PKG_VERSION")
                    }
                }
            }),
        )?;
        write_json_line(
            &mut stdin,
            &serde_json::json!({ "method": "initialized", "params": {} }),
        )?;

        let deadline = Instant::now() + Duration::from_secs(15);
        let mut request_id = 1_i64;
        let mut cursor: Option<String> = None;
        let mut models = Vec::new();

        loop {
            let mut params = serde_json::json!({
                "limit": 100,
                "includeHidden": false
            });
            if let Some(cursor) = &cursor {
                params["cursor"] = serde_json::Value::String(cursor.clone());
            }
            write_json_line(
                &mut stdin,
                &serde_json::json!({
                    "method": "model/list",
                    "id": request_id,
                    "params": params
                }),
            )?;

            let response = loop {
                let remaining = deadline.checked_duration_since(Instant::now())?;
                let message = rx.recv_timeout(remaining).ok()?;
                if message.get("id").and_then(|id| id.as_i64()) == Some(request_id) {
                    break message;
                }
            };
            if response.get("error").is_some() {
                return None;
            }

            let (page, next_cursor) = parse_codex_model_page(response.get("result")?)?;
            for model in page {
                if !models.iter().any(|existing: &AgentModel| existing.id == model.id) {
                    models.push(model);
                }
            }
            let Some(next_cursor) = next_cursor else {
                return Some(models);
            };
            cursor = Some(next_cursor);
            request_id += 1;
        }
    })();

    // Closing stdin and killing the long-lived server both unblock its stdout
    // reader. Always reap it so a model-dropdown request cannot leak a process.
    drop(stdin);
    let _ = child.kill();
    let _ = child.wait();
    let _ = reader.join();
    result.unwrap_or_default()
}

fn write_json_line(stdin: &mut impl Write, value: &serde_json::Value) -> Option<()> {
    serde_json::to_writer(&mut *stdin, value).ok()?;
    stdin.write_all(b"\n").ok()?;
    stdin.flush().ok()
}

/// Parse one `model/list` result. Use `model` as the CLI flag value (falling
/// back to `id` for older catalogs) and `displayName` as the friendly label.
fn parse_codex_model_page(
    result: &serde_json::Value,
) -> Option<(Vec<AgentModel>, Option<String>)> {
    let models = result
        .get("data")?
        .as_array()?
        .iter()
        .filter(|entry| entry.get("hidden").and_then(|v| v.as_bool()) != Some(true))
        .filter_map(|entry| {
            let id = entry
                .get("model")
                .and_then(|v| v.as_str())
                .or_else(|| entry.get("id").and_then(|v| v.as_str()))
                .and_then(non_empty)?;
            let label = entry
                .get("displayName")
                .and_then(|v| v.as_str())
                .and_then(non_empty)
                .unwrap_or_else(|| id.clone());
            Some(AgentModel { id, label })
        })
        .collect();
    let cursor = result
        .get("nextCursor")
        .and_then(|v| v.as_str())
        .and_then(non_empty);
    Some((models, cursor))
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
            .build_fork(&ctx_with_model("opus"), "conv-1", "hi", Path::new("/workspace/fork"))
            .unwrap()
            .unwrap();
        let model_at = spec.args.iter().position(|a| a == "--model").unwrap();
        let resume_at = spec.args.iter().position(|a| a == "--resume").unwrap();
        assert!(model_at < resume_at, "model flag must precede --resume");
        assert_eq!(spec.args[model_at + 1], "opus");
    }

    #[test]
    fn codex_fork_forces_the_resolved_cwd_and_puts_model_before_positionals() {
        let args = codex_fork_args(
            &ctx_with_model("gpt-x"),
            "conv-1",
            "hi",
            Path::new("/workspace/fork"),
        );
        // `codex -C <new-worktree> fork -m <model> <id> <seed>`: the explicit
        // cwd overrides the source conversation's persisted cwd, and -m remains
        // before the id/seed positionals.
        let m = args.iter().position(|a| a == "-m").unwrap();
        let id = args.iter().position(|a| a == "conv-1").unwrap();
        assert_eq!(&args[..3], ["-C", "/workspace/fork", "fork"]);
        assert!(m < id);
        assert_eq!(args[m + 1], "gpt-x");
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

    #[test]
    fn parse_codex_model_list_uses_cli_ids_and_display_names() {
        let result = serde_json::json!({
            "data": [
                {
                    "id": "catalog-entry-1",
                    "model": "gpt-5.4",
                    "displayName": "GPT-5.4",
                    "hidden": false,
                    "isDefault": true
                },
                {
                    "id": "gpt-5.4-mini",
                    "displayName": null,
                    "hidden": false
                },
                {
                    "id": "internal-model",
                    "displayName": "Internal",
                    "hidden": true
                },
                {
                    "id": "",
                    "displayName": "Invalid"
                }
            ],
            "nextCursor": "page-2"
        });
        let (models, cursor) = parse_codex_model_page(&result).unwrap();
        assert_eq!(cursor.as_deref(), Some("page-2"));
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].id, "gpt-5.4");
        assert_eq!(models[0].label, "GPT-5.4");
        assert_eq!(models[1].id, "gpt-5.4-mini");
        assert_eq!(models[1].label, "gpt-5.4-mini");
    }

    #[cfg(unix)]
    #[test]
    fn codex_models_uses_initialized_app_server_and_follows_pages() {
        use std::os::unix::fs::PermissionsExt;

        let dir = std::env::temp_dir().join(format!("asm-codex-models-{}", uuid::Uuid::new_v4()));
        let binary = dir.join("codex");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            &binary,
            r#"#!/bin/sh
state=0
while IFS= read -r line; do
  case "$line" in
    *'"method":"initialize"'*) state=1 ;;
    *'"method":"initialized"'*) [ "$state" = 1 ] || exit 2; state=2 ;;
    *'"method":"model/list"'*)
      [ "$state" = 2 ] || exit 3
      case "$line" in
        *'"cursor":"page-2"'*)
          printf '%s\n' '{"id":2,"result":{"data":[{"id":"gpt-b","displayName":"GPT B"}],"nextCursor":null}}'
          exit 0
          ;;
        *)
          printf '%s\n' '{"id":1,"result":{"data":[{"id":"gpt-a","displayName":"GPT A"}],"nextCursor":"page-2"}}'
          ;;
      esac
      ;;
  esac
done
"#,
        )
        .unwrap();
        let mut permissions = std::fs::metadata(&binary).unwrap().permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&binary, permissions).unwrap();

        let models = list_codex_models(binary.to_str());
        let _ = std::fs::remove_dir_all(&dir);

        let ids: Vec<_> = models.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(ids, vec!["gpt-a", "gpt-b"]);
        assert_eq!(models[0].label, "GPT A");
        assert_eq!(models[1].label, "GPT B");
    }
}
