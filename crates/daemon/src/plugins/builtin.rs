use std::path::Path;
use std::sync::Arc;

use anyhow::{anyhow, Result};

use super::conversation;
use super::fork;
use super::title;
use super::usage::{self, AgentUsage, TranscriptContext};
use super::{
    attention, find_in_path, AgentContext, AgentOption, AgentPlugin, HeadlessSpec, LaunchSpec,
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
    fn accepts_seed_prompt(&self) -> bool {
        true
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
    fn accepts_seed_prompt(&self) -> bool {
        true
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
    fn accepts_seed_prompt(&self) -> bool {
        true
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
