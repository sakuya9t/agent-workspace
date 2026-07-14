//! Per-session monitor for [`SessionManager`] (RF-M4 split).
//!
//! The monitor task and its attention/idle/exit state machine: `spawn_monitor`
//! (the select loop) plus `on_output` / `on_idle` / `on_exit`. Moved verbatim
//! out of `mod.rs`; the logic is unchanged. `scan_bell`, `trim_tail`, the
//! `Interaction` signal, the idle/echo consts, and `attention` arrive via
//! `super::*`.

use super::*;

impl SessionManager {
    pub(super) fn spawn_monitor(
        self: Arc<Self>,
        id: String,
        handle: Arc<dyn BackendSession>,
        started_at: i64,
        plugin: Option<Arc<dyn AgentPlugin>>,
    ) {
        let sig = Arc::new(Interaction::default());
        self.interactions.lock().insert(id.clone(), sig.clone());
        tokio::spawn(async move {
            let mut status_rx = handle.watch_status();
            let (_snap, mut out_rx) = handle.attach();
            let mut tail = String::new();
            let mut last_activity_write = 0i64;
            let mut last_attn = AttentionState::None;
            // Carries OSC-escape state across chunks so a window-title update
            // split over two reads isn't miscounted as a bell (see `scan_bell`).
            let mut in_osc = false;
            // Whether this session's agent-native conversation id is on record
            // yet. Captured while the session is alive so that a fork can resume
            // the *right* conversation later — see `capture_native_id`.
            let mut native_captured = false;

            loop {
                // Only a *working* session needs the close idle watch; a blocked
                // session is sticky (stays until viewed/answered) and silence
                // never demotes it to idle.
                let idle_delay = if last_attn == AttentionState::Activity {
                    IDLE_AFTER
                } else {
                    Duration::from_secs(60)
                };
                let idle_tick = tokio::time::sleep(idle_delay);

                // Poll for the agent's own conversation id until we have it, then
                // never again. An agent writes its transcript a moment after it
                // starts, not at exec time, so this cannot be a one-shot at spawn.
                let capture_tick = async {
                    if native_captured {
                        std::future::pending::<()>().await
                    } else {
                        tokio::time::sleep(CAPTURE_EVERY).await
                    }
                };

                tokio::select! {
                    changed = status_rx.changed() => {
                        if changed.is_err() {
                            break;
                        }
                        let st = status_rx.borrow().clone();
                        if st.is_terminal() {
                            // Last chance: a session that died inside the first
                            // capture tick still has a transcript on disk, and is
                            // exactly the kind of session someone wants to fork.
                            if !native_captured {
                                self.clone().capture_native_id(&id, plugin.clone(), started_at).await;
                            }
                            self.on_exit(&id, &handle, started_at, st, plugin.as_ref()).await;
                            break;
                        }
                    }
                    _ = capture_tick => {
                        native_captured =
                            self.clone().capture_native_id(&id, plugin.clone(), started_at).await;
                    }
                    recv = out_rx.recv() => {
                        match recv {
                            Ok(bytes) => {
                                // If the user viewed/answered since the last chunk,
                                // drop a sticky *block* so fresh output reclassifies.
                                // A plain idle prompt is left untouched here — its
                                // keystroke echo must not read as work, which
                                // `on_output` handles via the input timing below.
                                if sig.reset.swap(false, Ordering::Relaxed)
                                    && matches!(
                                        last_attn,
                                        AttentionState::LikelyBlocked
                                            | AttentionState::ApprovalNeeded
                                            | AttentionState::Error
                                    )
                                {
                                    last_attn = AttentionState::None;
                                }
                                let last_input_ms = sig.last_input_ms.load(Ordering::Relaxed);
                                let submitted = sig.submitted.load(Ordering::Relaxed);
                                self.on_output(&id, &handle, &bytes, plugin.as_ref(), &mut tail, &mut last_activity_write, &mut last_attn, &mut in_osc, last_input_ms, submitted);
                            }
                            Err(RecvError::Lagged(_)) => { /* attention is best-effort */ }
                            Err(RecvError::Closed) => {
                                // Backend gone; the status watch drives the exit.
                            }
                        }
                    }
                    _ = idle_tick => {
                        self.on_idle(&id, &handle, plugin.as_ref(), &mut last_attn, &sig);
                    }
                }
            }
            self.interactions.lock().remove(&id);
        });
    }

    /// Record the agent's own conversation id for a live session, so that a fork
    /// can later resume *that* conversation rather than guess at one.
    ///
    /// This is deliberately done while the session runs, not at fork time. The
    /// transcript-matching in [`crate::plugins::usage`] is a heuristic — Claude's
    /// is literally "the newest `*.jsonl` in this cwd's directory" — and two
    /// sessions sharing a working directory (normal once worktree isolation is
    /// off, and *guaranteed* for a same-branch fork) can collapse onto whichever
    /// transcript was written last. Reporting the wrong token count is survivable;
    /// resuming the wrong conversation is not. Capturing early, while this session
    /// is the only recent writer, is when that heuristic is at its most reliable —
    /// and once captured the id is never re-derived.
    ///
    /// Returns whether the id is now on record (including when a previous tick
    /// wrote it), so the caller can stop polling.
    async fn capture_native_id(
        self: Arc<Self>,
        id: &str,
        plugin: Option<Arc<dyn AgentPlugin>>,
        started_at: i64,
    ) -> bool {
        let Some(plugin) = plugin else {
            return true; // no plugin, nothing to capture, stop asking
        };
        let Ok(Some(session)) = self.db.get_session(id) else {
            return false;
        };
        if session.agent_session_id.is_some() {
            return true;
        }

        let id = id.to_string();
        let this = self.clone();
        // Reads a transcript off disk: keep it off the async runtime.
        tokio::task::spawn_blocking(move || {
            let cx = TranscriptContext {
                cwd: PathBuf::from(&session.working_directory),
                started_at_ms: started_at,
            };
            let Some(native_id) = plugin.native_session_id(&cx) else {
                return false;
            };
            match this.db.set_agent_session_id(&id, &native_id) {
                Ok(_) => {
                    tracing::debug!(session = %id, native_id = %native_id, "captured the agent's conversation id");
                    true
                }
                Err(e) => {
                    tracing::warn!(session = %id, "could not record the agent's conversation id: {e:#}");
                    false
                }
            }
        })
        .await
        .unwrap_or(false)
    }

    /// Output has been silent for [`IDLE_AFTER`]: a *working* session is now idle,
    /// waiting for the next input — unless the agent's own screen says otherwise.
    /// Silence is not proof the turn ended, so before settling we let the plugin
    /// read the screen for the two states it would otherwise misread:
    ///
    /// * it stopped **on an error** — Claude Code's "API Error: …" prints with no
    ///   bell and no prompt, so this settle is the only moment that distinguishes
    ///   "finished, waiting" from "died mid-turn"
    ///   ([`idle_error`](AgentPlugin::idle_error));
    /// * it is **still working** — Codex goes quiet while blocked on a sub-agent,
    ///   and leaves background terminals running past the end of a turn
    ///   ([`idle_busy`](AgentPlugin::idle_busy)). That holds the session at
    ///   `Activity`, and the settle is retried on the next tick.
    ///
    /// A blocked/errored session is sticky and stays that way — silence doesn't
    /// mean it stopped needing you.
    pub(super) fn on_idle(
        &self,
        id: &str,
        handle: &Arc<dyn BackendSession>,
        plugin: Option<&Arc<dyn AgentPlugin>>,
        last_attn: &mut AttentionState,
        sig: &Interaction,
    ) {
        if *last_attn != AttentionState::Activity {
            return;
        }
        // Rendered once, and only for a plugin that can read it: a screen snapshot
        // costs a terminal render, and this tick repeats for as long as it's quiet.
        let stopped_on_error = match plugin {
            Some(p) => {
                let screen = handle.screen_text();
                if p.idle_busy(&screen) {
                    return; // still working — this silence isn't the end of the turn
                }
                p.idle_error(&screen)
            }
            None => None,
        };
        // Fresh idle prompt: whatever the user types next is composing again,
        // so clear the submit latch — their keystroke echo is suppressed until
        // they submit the next line.
        sig.submitted.store(false, Ordering::Relaxed);
        let (state, reason) = match stopped_on_error {
            Some(reason) => (AttentionState::Error, reason),
            None => (AttentionState::Idle, "idle — waiting for input".to_string()),
        };
        *last_attn = state;
        let _ = self.db.set_attention(id, state, Some(&reason), now_millis());
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn on_output(
        &self,
        id: &str,
        handle: &Arc<dyn BackendSession>,
        bytes: &[u8],
        plugin: Option<&Arc<dyn AgentPlugin>>,
        tail: &mut String,
        last_write: &mut i64,
        last_attn: &mut AttentionState,
        in_osc: &mut bool,
        last_input_ms: i64,
        submitted: bool,
    ) {
        // A non-tracking agent (plain shell): the user drives and reads the
        // terminal themselves, so derived working/idle/blocked states are
        // noise — a shell prompt showing `password:` is not an approval gate
        // we manage. Record activity so "last active" stays truthful; nothing
        // else. `last_attn` never leaves None, so the idle settle is inert too.
        if plugin.is_some_and(|p| !p.tracks_attention()) {
            let now = now_millis();
            if now - *last_write >= 400 {
                *last_write = now;
                let _ =
                    self.db
                        .update_activity(id, handle.last_seq(), now, AttentionState::None, None);
            }
            return;
        }
        // Maintain a small decoded tail for the default (tail-based) classifier.
        tail.push_str(&String::from_utf8_lossy(bytes));
        trim_tail(tail, 4096);
        // Only trust the bell as an attention signal for agents that opt in
        // (a plain shell rings it as UI noise), and only a *real* bell — not the
        // BEL that terminates an OSC window-title update, which agents like
        // Claude Code emit constantly while working (`ESC ] 0 ; <title> BEL`).
        let bell = plugin.is_some_and(|p| p.bell_means_attention()) && scan_bell(bytes, in_osc);
        // Classification is per-provider. Most agents read the raw output tail;
        // one whose approval UI the tail can't see (Claude Code's boxed menu)
        // asks for the rendered screen instead — bounded to the visible grid and
        // always current, so a prompt buried above a footer / redraw frames is
        // still seen. An unknown plugin falls back to the default heuristic.
        let screen;
        let (raw, reason) = match plugin {
            Some(p) if p.attention_uses_screen() => {
                screen = handle.screen_text();
                p.attention(&screen, bell)
            }
            Some(p) => p.attention(tail, bell),
            None => attention::default_attention(tail, bell),
        };
        let now = now_millis();

        // Keystroke echo at an idle prompt: the user is composing their next
        // command, the agent is not working. Output that lands within
        // [`ECHO_WINDOW`] of their last input — and that hasn't yet submitted a
        // line — is that echo, so the prompt stays idle. A submit (CR/LF) hands
        // off to the agent, so its output *is* real work and falls through. This
        // only guards the idle state; spontaneous agent output (no recent input)
        // is outside the window and reads as activity as before.
        if raw == AttentionState::Activity
            && *last_attn == AttentionState::Idle
            && !submitted
            && last_input_ms != 0
            && now.saturating_sub(last_input_ms) < ECHO_WINDOW.as_millis() as i64
        {
            return;
        }

        // Sticky "blocked"/"error": agents ring the bell / show a prompt when
        // they need you, then keep redrawing (TUIs) — plain redraw output (or a
        // still-running background shell's noise under a dead turn) must NOT
        // demote that back to "working". It clears when the user views or
        // answers (which resets `last_attn` in the monitor loop).
        let was_blocked = matches!(
            *last_attn,
            AttentionState::LikelyBlocked | AttentionState::ApprovalNeeded | AttentionState::Error
        );
        let attention = if raw == AttentionState::Activity && was_blocked {
            *last_attn
        } else {
            raw
        };
        *last_attn = attention;

        // Debounce activity writes, but always flush a blocking/approval signal.
        if attention != AttentionState::Activity || now - *last_write >= 400 {
            *last_write = now;
            let _ = self.db.update_activity(
                id,
                handle.last_seq(),
                now,
                attention,
                reason.as_deref(),
            );
        }
    }

    async fn on_exit(
        &self,
        id: &str,
        handle: &Arc<dyn BackendSession>,
        started_at: i64,
        status: BackendStatus,
        plugin: Option<&Arc<dyn AgentPlugin>>,
    ) {
        let now = now_millis();
        let last_seq = handle.last_seq();

        // Respect an explicit stop/archive already recorded.
        let existing = self.db.get_session(id).ok().flatten();
        let already = existing.as_ref().map(|s| s.status);

        let (final_status, exit_code, attention, reason, exit_label) = match status {
            BackendStatus::Exited(0) => (
                SessionStatus::Exited,
                Some(0),
                AttentionState::None,
                None,
                "exited(0)".to_string(),
            ),
            BackendStatus::Exited(code) => (
                SessionStatus::Exited,
                Some(code),
                AttentionState::Failed,
                Some(format!("exited with code {code}")),
                format!("exited({code})"),
            ),
            BackendStatus::Failed(msg) => (
                SessionStatus::Failed,
                None,
                AttentionState::Failed,
                Some(msg.clone()),
                format!("failed: {msg}"),
            ),
            BackendStatus::Running => return, // not terminal; ignore
        };

        // If the user explicitly stopped/archived, preserve that status.
        let status_to_write = match already {
            Some(SessionStatus::Stopped) => SessionStatus::Stopped,
            Some(SessionStatus::Archived) => SessionStatus::Archived,
            _ => final_status,
        };

        // A user-ended session is not a failure. Stopping kills the child (a
        // non-zero/​signalled exit), which would otherwise show as `failed` with a
        // scary exit code — clear both and label the summary by the user action.
        let user_ended = matches!(
            status_to_write,
            SessionStatus::Stopped | SessionStatus::Archived
        );
        let (exit_code, attention, reason, exit_label) = if user_ended {
            (None, AttentionState::None, None, status_to_write.as_str().to_string())
        } else {
            (exit_code, attention, reason, exit_label)
        };

        // A non-tracking agent (plain shell) never carries an attention badge:
        // a non-zero exit is routine there (it's the last command's status),
        // and the row's ended-status already shows the code.
        let (attention, reason) = if plugin.is_some_and(|p| !p.tracks_attention()) {
            (AttentionState::None, None)
        } else {
            (attention, reason)
        };

        let _ = self
            .db
            .update_status(id, status_to_write, exit_code, now);
        let _ = self
            .db
            .set_attention(id, attention, reason.as_deref(), now);

        // Structural session summary (deterministic metadata, no LLM).
        let summary = SessionSummary {
            id: Uuid::new_v4().to_string(),
            session_id: id.to_string(),
            agent_plugin_id: existing
                .as_ref()
                .map(|s| s.agent_plugin_id.clone())
                .unwrap_or_default(),
            started_at,
            ended_at: now,
            duration_ms: (now - started_at).max(0),
            exit_status: exit_label,
            terminal_event_start: 1,
            terminal_event_end: last_seq,
        };
        if let Err(e) = self.db.insert_summary(&summary) {
            tracing::warn!(session = %id, "failed to write session summary: {e:#}");
        }

        self.live.lock().remove(id);
        tracing::info!(session = %id, status = %status_to_write.as_str(), "session finalized");
    }
}
