//! Forking a session: continuing its work in a new session, with its context.
//!
//! A running session holds three things worth keeping — a branch, a working
//! directory, and an agent that has learned the shape of the problem. Forking
//! carries all three into a new session, so you can change direction, hand the
//! work to a different agent, or explore a second approach without losing what
//! the first one figured out.
//!
//! Unlike recovery (`docs/session-recovery.md`), the origin is usually **still
//! alive**. That is the whole difficulty, and it drives the two decisions below.
//!
//! ## Where the fork runs
//!
//! Git will not check one branch out into two worktrees, and two agents editing
//! one directory will overwrite each other. So:
//!
//! * **A new branch** (the default) starts at the origin branch's tip in a
//!   worktree of its own. The two sessions never touch the same files. This is
//!   the only choice that is safe while the origin is running.
//! * **The same branch** reuses the origin's worktree in place, via the sharing
//!   path `resolve_workspace` already has. Safe once the origin has stopped — and
//!   that case *is* session recovery, which is why this one feature covers both.
//!   While the origin is live, the caller is expected to have warned the user.
//!
//! ## How the context gets across
//!
//! Two paths, and every fork takes exactly one ([`ForkSeed`]):
//!
//! * **Native.** If the fork keeps the same agent, that agent reloads its own
//!   conversation — `claude --resume … --fork-session`, `codex fork …`. Perfect
//!   fidelity, nothing summarized, nothing lost. It *forks*: the origin's
//!   transcript is never appended to, because the origin lives on and may be
//!   forked again.
//! * **Brief.** Otherwise — a different agent, or an origin whose native id we
//!   never captured — the daemon writes a Markdown brief into the fork's working
//!   directory and launches the agent pointed at it. The brief is a digest
//!   ([`crate::plugins::fork`]) plus, if any agent CLI on this host can write one,
//!   a prose handoff ([`crate::summarize`]), plus the full prior conversation.
//!
//! Either way the fork opens, states what it understands, and **waits**. It does
//! not silently pick up the old task: you forked in order to decide what happens
//! next.

use super::*;

use anyhow::Context;

use crate::plugins::LaunchSpec;
use crate::summarize;

/// Where a fork's inherited context is written, relative to its working
/// directory. `.asm/` is already where session-local artifacts live (see the
/// paste handler).
const BRIEF_DIR: &str = ".asm";

/// What the caller asks for when forking. The branch triple, cwd and workspace
/// are *not* here: a fork inherits its origin's place, and only the checkbox
/// below chooses between the two ways of inheriting it.
#[derive(Debug, Clone)]
pub struct ForkRequest {
    /// The agent to fork *into* — the same one, or a different one.
    pub agent_plugin_id: String,
    pub options: Vec<(String, bool)>,
    /// Continue on the origin's own branch, in its worktree, rather than branching
    /// off it into a new one.
    pub same_branch: bool,
    pub rows: u16,
    pub cols: u16,
}

/// Told to the fork when it already *has* the conversation (a native fork).
const SEED_NATIVE: &str = "This session is a fork of the conversation above — you already have its \
full history. Do not carry on with the previous task automatically. In two or three lines, \
summarize where things stand, then stop and wait for my instructions.";

/// Told to the fork when its context is in a file. Deliberately short: it is a
/// *pointer*, not the context itself. A transcript typed into a TUI runs into
/// bracketed-paste and input-length limits, and one passed in argv would be
/// readable by any process on the box via `/proc`.
fn seed_brief(path: &str) -> String {
    format!(
        "This session is a fork of an earlier session, and continues its work. Read the file \
         `{path}` first — it holds a handoff brief, a digest of what was asked and done, and the \
         full prior conversation. Then, in two or three lines, summarize where things stand, and \
         stop and wait for my instructions. Do not change any files until I ask you to."
    )
}

impl SessionManager {
    /// Fork `origin_id` into a new session. See the module docs.
    ///
    /// **Blocking**, and slowly so: it may run an agent CLI for tens of seconds to
    /// write the handoff brief. Call it from `spawn_blocking`, never on the async
    /// runtime.
    pub fn fork_session(self: &Arc<Self>, origin_id: &str, req: ForkRequest) -> Result<Session> {
        let origin = self
            .db
            .get_session(origin_id)?
            .ok_or_else(|| anyhow!("unknown session `{origin_id}`"))?;

        // Archiving may already have deleted the worktree and branch
        // (`discard_instance`), so there is no place left to fork into.
        if origin.status == SessionStatus::Archived {
            bail!("cannot fork an archived session: its worktree and branch may be gone");
        }

        let target = self
            .registry
            .get(&req.agent_plugin_id)
            .ok_or_else(|| anyhow!("unknown agent plugin `{}`", req.agent_plugin_id))?;

        let instance = self.db.get_instance_for_session(origin_id)?;
        let origin_branch = instance.as_ref().and_then(|i| i.branch.clone());
        let isolation = instance.as_ref().map(|i| i.isolation.as_str()).unwrap_or("");
        let in_worktree = matches!(isolation, "worktree" | "shared");

        // A worktree session with no branch is on a detached HEAD (the collision
        // fallback in `create_worktree`). There is no branch to stay on, and its
        // directory is under the daemon's worktree root, which the raw-path
        // allowlist won't accept — so say so rather than silently forking
        // somewhere else.
        if req.same_branch && in_worktree && origin_branch.is_none() {
            bail!("the source session is not on a branch (detached HEAD), so a fork cannot stay on it");
        }

        // Reusing the origin's directory is what makes a same-branch fork "the
        // same place" — and it is also the condition under which Claude can find
        // the conversation to resume (it keys transcripts by cwd).
        let reuses_cwd = req.same_branch && Path::new(&origin.working_directory).is_dir();

        let seed = self.plan_seed(&origin, &target, reuses_cwd);

        let (branch, base_ref, direct_checkout) = match (&origin_branch, req.same_branch) {
            // Stay on the origin's branch: name it and do *not* create it.
            // `resolve_workspace` then finds it already checked out and shares
            // that worktree in place, refcounted, rather than failing the way a
            // second `git worktree add` of one branch would.
            (Some(b), true) => (Some(b.clone()), None, false),
            // Branch off it: name no branch, so the auto `asm-session/<id>` form
            // is used (unique by construction, and already understood by the
            // orphan sweep and by archive-time branch cleanup) — but base it on
            // the origin's branch instead of the repo's HEAD.
            (Some(b), false) => (None, Some(b.clone()), false),
            // The origin runs in the source checkout itself, so "the same branch"
            // means "the same directory".
            (None, true) => (None, None, true),
            // Nothing to fork from: a new branch starts wherever the repo is. A
            // non-git workspace ignores all of this and uses its root.
            (None, false) => (None, None, false),
        };

        self.create_session(CreateSessionRequest {
            agent_plugin_id: req.agent_plugin_id,
            cwd: origin.working_directory.clone(),
            command: None,
            args: vec![],
            env: vec![],
            rows: req.rows,
            cols: req.cols,
            workspace_id: origin.workspace_id.clone(),
            approve_custom: false,
            direct_checkout,
            branch,
            create_branch: false,
            base_ref,
            options: req.options,
            fork: Some(ForkPlan {
                origin_id: origin.id.clone(),
                seed,
            }),
        })
    }

    /// Decide how the fork inherits context. Native when we can, brief otherwise —
    /// and the brief is built *here*, because it is slow (it may run an agent) and
    /// must not happen on the path that native forks take.
    fn plan_seed(
        &self,
        origin: &Session,
        target: &Arc<dyn AgentPlugin>,
        reuses_cwd: bool,
    ) -> ForkSeed {
        let same_agent = target.id() == origin.agent_plugin_id;
        // `build_fork` returning `Some` *is* the capability — probing it keeps
        // "can this agent fork natively" from drifting away from "how".
        let can_fork_natively = target
            .build_fork(&AgentContext::default(), "probe", "probe")
            .is_some();
        // Claude finds a conversation by cwd, so it can only resume one when the
        // fork lands in the origin's directory. Codex addresses rollouts by uuid
        // and resumes from anywhere.
        let cwd_ok = reuses_cwd || !target.native_fork_requires_same_cwd();

        if same_agent && can_fork_natively && cwd_ok {
            if let Some(native_id) = &origin.agent_session_id {
                tracing::info!(
                    origin = %origin.id,
                    agent = target.id(),
                    "forking natively from the agent's own conversation"
                );
                return ForkSeed::Native {
                    native_id: native_id.clone(),
                };
            }
        }

        ForkSeed::Brief {
            markdown: self.build_brief(origin, target.id()),
        }
    }

    /// The document a non-native fork is pointed at: a prose handoff brief (if any
    /// installed agent can write one), the deterministic digest, and the full
    /// prior conversation.
    ///
    /// Nothing here can fail the fork. A missing transcript, an absent summarizer
    /// and a summarizer that times out all degrade to a thinner document — a fork
    /// with less context still beats no fork.
    fn build_brief(&self, origin: &Session, target_id: &str) -> String {
        let origin_plugin = self.registry.get(&origin.agent_plugin_id);
        let cx = TranscriptContext {
            cwd: PathBuf::from(&origin.working_directory),
            started_at_ms: origin.created_at,
        };

        let digest = origin_plugin.as_ref().and_then(|p| p.digest(&cx));
        let conversation = origin_plugin.as_ref().and_then(|p| p.conversation(&cx));
        let agent_name = origin_plugin
            .as_ref()
            .map(|p| p.display_name())
            .unwrap_or("agent");

        // Summarize the *digest*, never the conversation: a few thousand tokens
        // rather than tens or hundreds of thousands. That is what makes this take
        // seconds instead of minutes, and what keeps it inside a small context.
        let summary = digest
            .as_deref()
            .and_then(|d| summarize::handoff_brief(&self.registry, target_id, d));

        if digest.is_none() {
            tracing::info!(
                origin = %origin.id,
                agent = %origin.agent_plugin_id,
                "no digest for the forked session; its brief carries what context there is"
            );
        }

        crate::plugins::fork::brief(
            &origin.id,
            agent_name,
            summary.as_deref(),
            digest.as_deref(),
            conversation.as_deref(),
        )
    }

    /// Build a fork's launch line, once its working directory is known.
    pub(super) fn fork_launch(
        &self,
        plugin: &Arc<dyn AgentPlugin>,
        ctx: &mut AgentContext,
        plan: &ForkPlan,
        cwd: &str,
    ) -> Result<LaunchSpec> {
        match &plan.seed {
            ForkSeed::Native { native_id } => plugin
                .build_fork(ctx, native_id, SEED_NATIVE)
                .ok_or_else(|| anyhow!("`{}` cannot fork a conversation natively", plugin.id()))?,
            ForkSeed::Brief { markdown } => {
                let rel = write_brief(Path::new(cwd), &plan.origin_id, markdown)?;
                // A shell would run a trailing argument as a script rather than
                // read it, so only agents that take an opening prompt get one. The
                // brief is on disk either way, which is exactly what a shell needs:
                // a file to `cat`.
                if plugin.accepts_seed_prompt() {
                    ctx.extra_args.push(seed_brief(&rel));
                }
                plugin.build_launch(ctx)
            }
        }
    }
}

/// Write the brief into the fork's working directory, returning the path to put
/// in the seed prompt — *relative*, because the agent's cwd is this directory and
/// a short path keeps the prompt short.
fn write_brief(cwd: &Path, origin_id: &str, markdown: &str) -> Result<String> {
    let short = &origin_id[..8.min(origin_id.len())];
    let name = format!("forked-from-{short}.md");
    // `asm_dir` also makes `.asm/` ignore itself. Without that the brief lands in
    // `git status` as an untracked file in the fork's own worktree — where an
    // agent tidying up before a commit would happily commit it.
    let dir = crate::util::asm_dir(cwd)
        .with_context(|| format!("creating {}/{BRIEF_DIR}", cwd.display()))?;
    std::fs::write(dir.join(&name), markdown)
        .with_context(|| format!("writing the fork brief to {}", dir.display()))?;
    Ok(format!("{BRIEF_DIR}/{name}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_brief_lands_in_the_worktree_and_the_prompt_points_at_it_relatively() {
        let dir = std::env::temp_dir().join(format!("asm-fork-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();

        let rel = write_brief(&dir, "0123456789abcdef", "# context").unwrap();
        assert_eq!(rel, ".asm/forked-from-01234567.md");
        assert_eq!(
            std::fs::read_to_string(dir.join(&rel)).unwrap(),
            "# context"
        );

        // The prompt is a pointer, and stays small no matter how big the brief is.
        let prompt = seed_brief(&rel);
        assert!(prompt.contains(".asm/forked-from-01234567.md"));
        assert!(prompt.len() < 600, "the seed prompt must stay fixed-size");
        assert!(prompt.contains("wait for my instructions"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_short_origin_id_does_not_panic_the_filename() {
        let dir = std::env::temp_dir().join(format!("asm-fork-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let rel = write_brief(&dir, "abc", "x").unwrap();
        assert_eq!(rel, ".asm/forked-from-abc.md");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn both_seeds_tell_the_fork_to_stop_and_wait() {
        // The one behaviour both paths must share: a fork loads context and then
        // hands control back, rather than silently resuming the old task.
        assert!(SEED_NATIVE.contains("wait for my instructions"));
        assert!(SEED_NATIVE.contains("Do not carry on"));
        assert!(seed_brief("x.md").contains("wait for my instructions"));
    }
}
