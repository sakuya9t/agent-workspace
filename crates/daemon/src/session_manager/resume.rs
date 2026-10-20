//! Bringing sessions back after the host restarts under them.
//!
//! The holder keeps a session alive across a daemon restart, but a host reboot
//! takes everything down with it: the agent, its PTY, the holder. Startup then
//! finds each of those sessions with no process behind it and records it
//! `indeterminate` — the status for "the holder lost track of this, and it may
//! still be running as an orphan".
//!
//! After a reboot that "may" is settled: nothing from the previous boot is
//! running (see [`crate::host_boot`]). Each such session is relaunched **in
//! place** — the same session, id, worktree and branch, with its terminal
//! history intact:
//!
//! * an agent that keeps its own conversation resumes it natively
//!   ([`AgentPlugin::build_resume`]) — `claude --resume <id>`, `codex resume
//!   <id>`, `pi --session <id>`;
//! * any other agent, and a shell, starts fresh in the same directory, with the
//!   old output still above it.
//!
//! Either way the relaunch only ever *waits*. No opening prompt is sent, so a
//! resumed agent shows its history and stops; whatever it was doing when the
//! host went down, carrying on is the user's call. That is also why a custom
//! command is never relaunched: running one is itself an action.
//!
//! A same-boot holder loss (it crashed, or was killed) is deliberately left
//! alone. Its children can outlive it, and relaunching then would put a second
//! agent in a worktree the first may still be editing.

use super::*;

use crate::plugins::LaunchSpec;

/// How a session came back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Resumed {
    /// The agent reloaded its own conversation.
    Conversation,
    /// The agent (or shell) started over in the same place.
    Fresh,
}

impl SessionManager {
    /// Relaunch, in place, every `indeterminate` session whose process provably
    /// died with an earlier boot of this host. Runs at startup, after the
    /// reconcile has marked the sessions the (new) holder doesn't know.
    ///
    /// A session that cannot be relaunched stays `indeterminate`, with the reason
    /// as its advisory, and is tried again on the next start: most failures here
    /// (an agent binary not on the daemon's `PATH` yet, a full holder) are
    /// transient, and nothing better is known about the session meanwhile.
    pub(super) fn resume_lost_sessions(self: &Arc<Self>, holder_ids: &HashSet<String>) -> Result<()> {
        let mut resumed = 0usize;
        let mut not_resumed = 0usize;
        for id in self.db.indeterminate_session_ids()? {
            // Known to the holder: its process may be alive. Not ours to replace.
            if holder_ids.contains(&id) {
                continue;
            }
            let Some(sess) = self.db.get_session(&id)? else {
                continue;
            };
            let boot_id = self.db.get_boot_id(&id)?;
            if !self.boot.outlived(boot_id.as_deref(), sess.created_at) {
                continue;
            }
            match self.resume_in_place(&sess) {
                Ok(how) => {
                    resumed += 1;
                    tracing::info!(
                        session = %id,
                        agent = %sess.agent_plugin_id,
                        ?how,
                        "resumed a session lost to a host restart"
                    );
                }
                Err(e) => {
                    not_resumed += 1;
                    tracing::warn!(session = %id, "could not resume a session lost to a host restart: {e:#}");
                    let tracks = self
                        .registry
                        .get(&sess.agent_plugin_id)
                        .is_none_or(|p| p.tracks_attention());
                    if tracks {
                        self.db.set_attention(
                            &id,
                            AttentionState::LikelyBlocked,
                            Some(&format!(
                                "the host restarted and this session was not resumed: {e:#}"
                            )),
                            now_millis(),
                        )?;
                    }
                }
            }
        }
        if resumed + not_resumed > 0 {
            tracing::info!(
                "host restart: resumed {resumed} session(s), {not_resumed} left indeterminate"
            );
        }
        Ok(())
    }

    /// Relaunch one session whose process is gone, under its own id.
    fn resume_in_place(self: &Arc<Self>, sess: &Session) -> Result<Resumed> {
        let plugin = self
            .registry
            .get(&sess.agent_plugin_id)
            .ok_or_else(|| anyhow!("unknown agent plugin `{}`", sess.agent_plugin_id))?;
        let cwd = Path::new(&sess.working_directory);
        if !cwd.is_dir() {
            bail!("its working directory {} is gone", cwd.display());
        }

        let ctx = recovered_context(plugin.as_ref(), sess);
        let native = sess
            .agent_session_id
            .as_deref()
            .and_then(|native_id| plugin.build_resume(&ctx, native_id, cwd));
        let (mut launch, how) = match native {
            Some(spec) => (spec?, Resumed::Conversation),
            None => (plugin.build_launch(&ctx)?, Resumed::Fresh),
        };
        if launch.requires_approval {
            bail!("a custom command is never re-run unattended — it acts as soon as it starts");
        }

        let instance = self.db.get_instance_for_session(&sess.id)?;
        workspace_instructions::apply(
            plugin.id(),
            &mut launch,
            cwd,
            instance.as_ref().and_then(|inst| inst.branch.as_deref()),
            &self
                .worktree_root
                .join(".instructions")
                .join(format!("{}.md", sess.id)),
        )?;

        let LaunchSpec {
            command, args, env, ..
        } = launch;
        let spec = BackendSpawnSpec {
            session_id: sess.id.clone(),
            command: command.clone(),
            args: args.clone(),
            env: env.clone(),
            cwd: sess.working_directory.clone(),
            rows: sess.rows,
            cols: sess.cols,
        };
        let handle = self
            .backend
            .respawn(spec, &banner(plugin.display_name(), how))?;

        // Only now that it runs: until here a failure leaves the row exactly as
        // it was, so the next start tries again.
        self.db
            .update_launch(&sess.id, &command, &args, &env)?;
        self.stamp_boot(&sess.id);
        self.live.lock().insert(sess.id.clone(), handle.clone());
        let now = now_millis();
        self.db
            .update_status(&sess.id, SessionStatus::Running, None, now)?;
        self.db
            .set_attention(&sess.id, AttentionState::None, None, now)?;
        self.clone()
            .spawn_monitor(sess.id.clone(), handle, sess.created_at, Some(plugin));
        Ok(how)
    }
}

/// The launch inputs a session was started with, read back from the launch line
/// it persisted: which option toggles were on, and an explicit model. Each is
/// matched by the exact argv its plugin emits for it ([`AgentPlugin::option_args`],
/// [`AgentPlugin::model_args`]), never by parsing the agent's CLI — and argv after
/// a `--` is the agent's own, so it is not searched.
///
/// Extra args are not recovered: no client sends them for an agent, and a
/// launch line has no way to tell them apart from what the daemon added (the
/// workspace instructions, a fork's resume and seed), which must not be replayed.
fn recovered_context(plugin: &dyn AgentPlugin, sess: &Session) -> AgentContext {
    let args = match sess.args.iter().position(|a| a == "--") {
        Some(end) => &sess.args[..end],
        None => &sess.args[..],
    };
    let options = plugin
        .options()
        .into_iter()
        .map(|o| {
            let flag = plugin.option_args(&o.key);
            let on = !flag.is_empty() && args.windows(flag.len()).any(|w| w == flag.as_slice());
            (o.key, on)
        })
        .collect();
    let model = if plugin.supports_models() {
        plugin.model_args("").first().and_then(|flag| {
            args.windows(2)
                .find(|w| &w[0] == flag)
                .map(|w| w[1].clone())
        })
    } else {
        None
    };
    AgentContext {
        // Only a custom command reads this; it is what lets that launch get as
        // far as saying it needs approval, and so be refused for that reason.
        command: Some(sess.command.clone()),
        extra_args: Vec::new(),
        extra_env: sess.env.clone(),
        options,
        model,
    }
}

/// Written between a session's old output and its new process. It first puts
/// the terminal back in plain mode — main screen, full scroll region, no mouse
/// or paste reporting — since the old process died in whatever mode it was in
/// and the new one starts expecting none. Not a full reset: that would wipe the
/// scrollback this is careful to keep. The scroll-region reset also homes the
/// cursor, so it is bracketed by a cursor save/restore; otherwise the banner
/// would be written over the top of the old output.
fn banner(agent: &str, how: Resumed) -> Vec<u8> {
    let what = match how {
        Resumed::Conversation => format!("resumed the {agent} conversation where it left off"),
        Resumed::Fresh => format!("started {agent} again here; the output above is from before"),
    };
    format!(
        "\x1b[?1049l\x1b7\x1b[r\x1b8\x1b[m\x1b[?25h\x1b[?1000l\x1b[?1002l\x1b[?1003l\
         \x1b[?1006l\x1b[?2004l\x1b[?1l\x1b>\r\n\x1b[2m[asm: the host restarted · {what}]\x1b[m\r\n"
    )
    .into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::builtin::{ClaudePlugin, CodexPlugin, PiPlugin, ShellPlugin};

    fn session_with_args(agent: &str, args: &[&str]) -> Session {
        Session {
            id: "s".into(),
            agent_plugin_id: agent.into(),
            command: agent.into(),
            args: args.iter().map(|a| a.to_string()).collect(),
            env: vec![("K".into(), "V".into())],
            working_directory: "/w".into(),
            workspace_id: None,
            status: SessionStatus::Indeterminate,
            rows: 24,
            cols: 80,
            last_event_seq: 0,
            exit_code: None,
            attention_state: AttentionState::None,
            attention_reason: None,
            created_at: 0,
            updated_at: 0,
            last_activity_at: 0,
            risky: false,
            agent_session_id: None,
            forked_from: None,
            state_since: 0,
        }
    }

    #[test]
    fn options_and_model_are_read_back_from_a_fresh_launch_line() {
        // What a skip-permissions Claude session on `opus` persisted, with the
        // workspace instructions the daemon put in front.
        let sess = session_with_args(
            "claude",
            &[
                "--append-system-prompt",
                "This session is managed by ASM… --model is not a flag in here",
                "--dangerously-skip-permissions",
                "--model",
                "opus",
            ],
        );
        let ctx = recovered_context(&ClaudePlugin, &sess);
        assert!(ctx.opt("skip_permissions"));
        assert_eq!(ctx.model.as_deref(), Some("opus"));
        assert_eq!(ctx.extra_env, sess.env);
        assert!(ctx.extra_args.is_empty(), "daemon-added argv must not be replayed");
    }

    #[test]
    fn nothing_is_read_back_that_was_not_there() {
        let sess = session_with_args("claude", &["--append-system-prompt", "x"]);
        let ctx = recovered_context(&ClaudePlugin, &sess);
        assert!(!ctx.opt("skip_permissions"));
        assert_eq!(ctx.model, None);
        // A shell has no options and no models.
        let ctx = recovered_context(&ShellPlugin, &session_with_args("shell", &["-i"]));
        assert!(ctx.options.is_empty());
        assert_eq!(ctx.model, None);
    }

    #[test]
    fn a_forked_launch_line_reads_back_the_same_way() {
        // `codex -c … -C <cwd> fork --dangerously-… -m <model> <origin> <seed>`.
        let sess = session_with_args(
            "codex",
            &[
                "-c",
                "developer_instructions=\"…\"",
                "-C",
                "/w",
                "fork",
                "--dangerously-bypass-approvals-and-sandbox",
                "-m",
                "gpt-x",
                "origin-conv",
                "seed prompt",
            ],
        );
        let ctx = recovered_context(&CodexPlugin, &sess);
        assert!(ctx.opt("bypass_approvals"));
        assert_eq!(ctx.model.as_deref(), Some("gpt-x"));
    }

    #[test]
    fn a_multi_token_option_must_match_as_a_run() {
        // pi's read-only toggle is `--tools read,grep,find,ls`; `--tools` with a
        // different list is some other launch, not this toggle.
        let on = session_with_args("pi", &["--tools", "read,grep,find,ls", "--model", "m"]);
        assert!(recovered_context(&PiPlugin, &on).opt("read_only"));
        let other = session_with_args("pi", &["--tools", "read,bash"]);
        assert!(!recovered_context(&PiPlugin, &other).opt("read_only"));
    }

    #[test]
    fn argv_after_a_double_dash_is_the_agents_own() {
        let sess = session_with_args("claude", &["--", "--dangerously-skip-permissions"]);
        assert!(!recovered_context(&ClaudePlugin, &sess).opt("skip_permissions"));
    }

    #[test]
    fn the_banner_keeps_scrollback_and_says_what_happened() {
        let b = String::from_utf8(banner("Claude Code", Resumed::Conversation)).unwrap();
        assert!(b.contains("host restarted"));
        assert!(b.contains("resumed the Claude Code conversation"));
        assert!(!b.contains("\x1bc"), "a full reset would erase the history kept above");
        assert!(b.starts_with("\x1b[?1049l"), "the new process must start on the main screen");

        // Fed through an emulator after some history, the history survives —
        // above the banner, not under it — and the old process's alt screen and
        // mouse reporting are gone.
        let mut p = vt100::Parser::new(10, 120, 100);
        p.process(b"line-before-1\r\nline-before-2\r\n\x1b[?1049h\x1b[?1000h");
        p.process(banner("Shell", Resumed::Fresh).as_slice());
        let screen = p.screen();
        assert!(!screen.alternate_screen());
        assert_eq!(screen.mouse_protocol_mode(), vt100::MouseProtocolMode::None);
        let rows: Vec<String> = screen.rows(0, 120).collect();
        assert_eq!(rows[0], "line-before-1");
        assert_eq!(rows[1], "line-before-2");
        assert!(rows[3].contains("started Shell again here"), "{rows:?}");
    }

    /// The round trip the next restart depends on: what a resume persists reads
    /// back as the same inputs, so a session resumed twice keeps its flags.
    #[test]
    fn a_resume_launch_line_reads_back_as_the_inputs_that_built_it() {
        let ctx = AgentContext {
            options: vec![("read_only".into(), true)],
            model: Some("openai/gpt-4o".into()),
            ..Default::default()
        };
        let Some(Ok(spec)) = PiPlugin.build_resume(&ctx, "conv", Path::new("/w")) else {
            return; // binary absent on this host
        };
        let args: Vec<&str> = spec.args.iter().map(String::as_str).collect();
        let back = recovered_context(&PiPlugin, &session_with_args("pi", &args));
        assert!(back.opt("read_only"));
        assert_eq!(back.model.as_deref(), Some("openai/gpt-4o"));
    }
}
