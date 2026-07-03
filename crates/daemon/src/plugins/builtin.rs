use std::sync::Arc;

use anyhow::{anyhow, Result};

use super::usage::{self, AgentUsage, UsageContext};
use super::{find_in_path, AgentContext, AgentOption, AgentPlugin, LaunchSpec};

const ALL_PLATFORMS: &[&str] = &["linux", "macos", "windows"];

/// The full set of built-in agent plugins registered at startup.
pub fn all() -> Vec<Arc<dyn AgentPlugin>> {
    vec![
        Arc::new(ShellPlugin),
        Arc::new(CodexPlugin),
        Arc::new(ClaudePlugin),
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
    fn bell_means_attention(&self) -> bool {
        true
    }
    fn usage(&self, cx: &UsageContext) -> Option<AgentUsage> {
        usage::codex_usage(cx)
    }
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
        let command = self
            .detect_binary()
            .ok_or_else(|| anyhow!("`codex` binary not found in PATH"))?;
        let mut args = Vec::new();
        if ctx.opt("bypass_approvals") {
            args.push("--dangerously-bypass-approvals-and-sandbox".to_string());
        }
        args.extend(ctx.extra_args.clone());
        Ok(LaunchSpec {
            command,
            args,
            env: ctx.extra_env.clone(),
            requires_approval: false,
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
    fn usage(&self, cx: &UsageContext) -> Option<AgentUsage> {
        usage::claude_usage(cx)
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
        let command = self
            .detect_binary()
            .ok_or_else(|| anyhow!("`claude` binary not found in PATH"))?;
        let mut args = Vec::new();
        if ctx.opt("skip_permissions") {
            args.push("--dangerously-skip-permissions".to_string());
        }
        args.extend(ctx.extra_args.clone());
        Ok(LaunchSpec {
            command,
            args,
            env: ctx.extra_env.clone(),
            requires_approval: false,
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

fn default_shell() -> String {
    if cfg!(target_os = "windows") {
        std::env::var("COMSPEC").unwrap_or_else(|_| "powershell.exe".to_string())
    } else {
        std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string())
    }
}
