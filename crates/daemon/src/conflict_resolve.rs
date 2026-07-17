//! Agent-driven auto-resolution of git merge/rebase conflicts.
//!
//! When a rebase or merge hits conflicts, the git layer ([`crate::source_control`])
//! doesn't abort straight away — it calls a [`ConflictResolver`], and this module
//! is the one that matters in production: it points an agent CLI at the conflicted
//! worktree and lets it edit the files back into a coherent state. The git layer
//! then stages exactly those files, rejects any leftover conflict markers, and
//! continues the operation. Only if the agent can't be run, or leaves conflicts
//! behind, does the operation abort and fail.
//!
//! ## This runs a real agent, with its guardrails off
//!
//! Unlike the fork summarizer ([`crate::summarize`]) — which runs in an empty temp
//! dir and is told to touch nothing — this runs *in the user's conflicted
//! worktree* with the agent's permission prompts bypassed
//! (`--dangerously-skip-permissions`, `--dangerously-bypass-approvals-and-sandbox`,
//! `--auto`). That is the whole point: the agent must be free to rewrite the
//! conflicted files. Three consequences carry over from the summarizer, all
//! load-bearing:
//!
//! * **stdin is closed**, or `codex exec` waits on it forever.
//! * **stdout/stderr are discarded** — a chatty agent whose pipe filled with
//!   nobody reading would block and burn the whole deadline doing nothing.
//! * **there is a deadline**, after which the agent is killed and the git layer
//!   decides from the actual file state (unresolved files → the op aborts).

use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};

use crate::plugins::{AgentPlugin, PluginRegistry};
use crate::source_control::{ConflictContext, ConflictResolver};

/// How long the agent gets to resolve one conflicted state before it is killed
/// and the operation aborts. Resolving conflicts is real work — read several
/// files, edit several — so this is far longer than the fork summarizer's 90s.
const DEADLINE: Duration = Duration::from_secs(300);

/// How often we check whether the agent has finished.
const POLL: Duration = Duration::from_millis(200);

/// Resolves git conflicts by running an agent CLI in the conflicted worktree.
pub struct AgentConflictResolver {
    registry: Arc<PluginRegistry>,
    /// The agent to prefer — the session's own, so a Claude session's conflicts
    /// are resolved by Claude. `None` (workspace-level ops, which belong to no one
    /// session) means "any installed agent that can".
    preferred: Option<String>,
}

impl AgentConflictResolver {
    pub fn new(registry: Arc<PluginRegistry>, preferred: Option<String>) -> Self {
        Self {
            registry,
            preferred,
        }
    }

    /// The agent that will resolve: the preferred one if it can, else the first
    /// installed agent that can. Mirrors the fork summarizer's choice.
    fn pick(&self) -> Option<Arc<dyn AgentPlugin>> {
        let capable = |p: &Arc<dyn AgentPlugin>| {
            p.detect_binary().is_some() && p.conflict_resolver("probe").is_some()
        };
        if let Some(pref) = &self.preferred {
            if let Some(p) = self.registry.get(pref).filter(capable) {
                return Some(p);
            }
        }
        self.registry.agents().iter().find(|p| capable(p)).cloned()
    }
}

impl ConflictResolver for AgentConflictResolver {
    fn resolve(&self, worktree: &Path, ctx: &ConflictContext) -> Result<()> {
        let plugin = self.pick().ok_or_else(|| {
            anyhow!("no installed agent can auto-resolve conflicts (need claude, codex or opencode)")
        })?;
        let prompt = prompt_for(ctx);
        let spec = plugin
            .conflict_resolver(&prompt)
            .ok_or_else(|| anyhow!("`{}` cannot auto-resolve conflicts", plugin.id()))?;
        tracing::info!(
            agent = plugin.id(),
            op = ctx.operation.noun(),
            files = ctx.conflicted_paths.len(),
            "auto-resolving git conflicts with an agent"
        );
        run(&spec.command, &spec.args, worktree, DEADLINE)
    }
}

/// The instruction handed to the resolving agent. Names the exact files so it
/// fixes those and nothing else, and forbids git state changes so the daemon
/// stays the one thing that stages and continues the operation.
fn prompt_for(ctx: &ConflictContext) -> String {
    let op = ctx.operation.noun();
    let files = ctx
        .conflicted_paths
        .iter()
        .map(|p| format!("- {p}"))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "You are resolving a git {op} conflict in the current repository. The {op} is paused with \
         conflict markers in these files:\n\n{files}\n\n\
         Resolve every conflict by editing ONLY those files so the result is correct and coherent, \
         preserving the intent of BOTH sides wherever possible rather than dropping either. Remove \
         every conflict marker (lines starting with <<<<<<<, =======, or >>>>>>>).\n\n\
         Do NOT run any git commands — do not stage, commit, `git rebase --continue`, or \
         `git merge --continue`. Just leave the files resolved on disk; the system stages the \
         listed files and continues the {op} for you. Do not create or modify any other files. \
         When the listed files are resolved, stop."
    )
}

/// Run the agent CLI to completion or `deadline`, whichever comes first. `Err`
/// only when the agent can't be *started* — a non-zero exit or a timeout returns
/// `Ok`, because the git layer verifies the actual files and is the real arbiter
/// of whether the conflict was resolved.
fn run(command: &str, args: &[String], cwd: &Path, deadline: Duration) -> Result<()> {
    let mut child = Command::new(command)
        .args(args)
        .current_dir(cwd)
        // See the module docs: closed stdin, discarded output.
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| anyhow!("could not start `{command}` to resolve conflicts: {e}"))?;

    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    // Not necessarily failure — the git layer checks the files —
                    // but worth a line when a resolution later turns out empty.
                    tracing::warn!(command, ?status, "conflict-resolver agent exited non-zero");
                }
                return Ok(());
            }
            Ok(None) => {
                if start.elapsed() >= deadline {
                    tracing::warn!(
                        command,
                        "conflict-resolver agent exceeded {}s; killing it",
                        deadline.as_secs()
                    );
                    let _ = child.kill();
                    let _ = child.wait();
                    return Ok(());
                }
                std::thread::sleep(POLL);
            }
            Err(e) => return Err(anyhow!("waiting on `{command}`: {e}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source_control::ConflictOp;

    fn ctx() -> ConflictContext {
        ConflictContext {
            operation: ConflictOp::Merge,
            conflicted_paths: vec!["src/a.rs".into(), "src/b.rs".into()],
        }
    }

    #[test]
    fn the_prompt_names_the_files_and_forbids_git_state_changes() {
        let p = prompt_for(&ctx());
        assert!(p.contains("- src/a.rs"));
        assert!(p.contains("- src/b.rs"));
        assert!(p.contains("merge")); // the operation noun
        assert!(p.contains("Do NOT run any git commands"));
        assert!(p.contains("<<<<<<<"));
    }

    #[test]
    fn a_rebase_prompt_uses_the_rebase_noun() {
        let c = ConflictContext {
            operation: ConflictOp::Rebase,
            conflicted_paths: vec!["x".into()],
        };
        assert!(prompt_for(&c).contains("git rebase --continue"));
    }

    #[test]
    fn resolve_errs_when_no_agent_is_installed() {
        // An empty registry has no capable agent, so resolve fails cleanly — which
        // makes the git layer abort, exactly as an unresolved conflict does.
        let registry = Arc::new(PluginRegistry::empty());
        let resolver = AgentConflictResolver::new(registry, None);
        let err = resolver
            .resolve(Path::new("/nonexistent"), &ctx())
            .unwrap_err();
        assert!(err.to_string().contains("no installed agent"));
    }

    #[test]
    fn a_resolver_that_cannot_start_its_agent_errors() {
        assert!(run(
            "asm-no-such-binary-xyz",
            &[],
            std::path::Path::new("/tmp"),
            DEADLINE
        )
        .is_err());
    }

    #[cfg(unix)]
    #[test]
    fn an_agent_that_overruns_its_deadline_is_killed_not_waited_on() {
        let start = Instant::now();
        let out = run(
            "sleep",
            &["60".to_string()],
            std::path::Path::new("/tmp"),
            Duration::from_millis(300),
        );
        assert!(out.is_ok(), "a timeout is not a start failure");
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "the deadline killed the agent rather than waiting: {:?}",
            start.elapsed()
        );
    }
}
