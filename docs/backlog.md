# Backlog

**2026-07-26 update (not a reconcile):** the minimal **RF-GATE** harness landed:
a real router/AppState auth + health harness, attachment takeover units, a
scripted `MockHolder` driving sidecar create/adopt/resync/end paths, asmux e2e
coverage for readBuffer/detach/backpressure/takeover/malformed bodies, and CI
running Rust + client gates with smoke/mobile-shell e2e. This clears the gate in
front of **RF-FLOW** and **RF-LIFE**; RF-GATE remains open at P2 for the client
lint expansion, generated-schema drift check, and migration/version-skew guard.

**2026-07-25 update (not a reconcile):** the **FIX** row landed in full — all
eight verified defects, each with a regression test that fails without its fix.
Its row and sequencing entry below are struck through; the 2026-07-24 reconcile
narrative that follows is left as written, so "all eight … remain open" there
should be read as of that date.

Last reconciled: **2026-07-24**, row by row against the code and history through
`99814d9` (not against this table's own claims). The table has 42 rows: two
completed rows retained for context, one decision folded into R5, and 39 open or
explicitly deferred rows.

**What this reconcile changed:**

- **REC landed as the more general session-fork feature** on 2026-07-14. The
  stale reliability chain ending in `→ REC` is gone. `RF-REC` keeps its legacy
  id but is now P2, non-blocking Git/API/UI consolidation; the two real recovery
  follow-ups remain **FORK-SHELL** and **FORK-OC**.
- **Post-reconcile product growth is now credited:** session/fork model choice,
  workspace branch management, agent-driven commits, agent conflict resolution,
  Agent Deck, managed UI + UI-only gateway startup, workspace uploads,
  directory creation, and the mobile/tablet/session-attachment polish through
  2026-07-23.
- **The formal release status is explicit:** the Unix/web central loop is a
  runnable alpha, but the written Alpha/MVP gates are not met while M5 Windows,
  rich rendering, setup hooks/checkpoints, and Electron packaging remain.
- **All eight FIX defects were re-verified in current code and remain open.**
  RF-GATE is also still open: one pure `needs_live_session` unit test now exists,
  but there is still no router/AppState harness, WS attachment unit suite, CI,
  `MockHolder`, migration ladder, or generated-schema drift gate.
- **DOC-1 landed in this reconcile:** architecture and the relay protocol comment
  now describe the shipped dial-out-per-stream relay rather than yamux.
- **Growth triggers were updated:** `RightPanel.tsx` is now 1,209 lines,
  `Terminal.tsx` 1,087, and `api.ts` 837. The RF-REC/RF-HYG/RF-QUERY splits are
  no longer hypothetical preparation; they are active maintenance debt.
- **Session copilots are now designed, not implemented:** **COPILOT-REVIEW**
  adds isolated same- or cross-provider review of an exact dirty-tree snapshot;
  **COPILOT-GATE** adds stale-proof policy over ASM-owned promotion actions.
  The design and honest enforcement boundary are in
  [`copilot.md`](copilot.md).

**2026-07-12 addition (rows added, not a full reconcile):** a five-subsystem
tech-debt review ([`refactoring-plan.md`](refactoring-plan.md) → §6; every
finding carries file:line evidence, the defect bundle hand-verified) added
rows **FIX** (verified latent defects, P1), **RF-REC** (pre-REC refactor
bundle when introduced; now generic consolidation at P2), **RF-GATE** (build
gate & test safety net, P1/P2),
**RF-WSPROTO** (client↔daemon WS contract, P2) and **RF-HYG** (hygiene
bundle, P3), and annotated the REC / M4-C / SEC-2 / R5 / M5 / MEAS /
MOB-PUSH / MVP-RICH rows with its findings.

**Independent 2026-07-12 review:** added **RF-FLOW** (bounded terminal flow +
durable persistence, P1), **RF-LIFE** (explicit lifecycle state machine + units
of work, P1) and **RF-OPS** (truthful health, deadlines and task supervision,
P2); expanded FIX with the server/client `indeterminate` contradiction and the
backend-config fail-open; and corrected stale transcript prose. See
`refactoring-plan.md` → §7.

Earlier reconciles (context): 2026-07-06 added the **RF-\*** rows from
[`refactoring-plan.md`](refactoring-plan.md); 2026-07-07 landed **R4** (gateway
mode) and **RF-M4 #1/#3/#4**; 2026-07-08 landed **TERM-SCROLL**; 2026-07-11
landed M4 Stage A+B and designed REC; 2026-07-12 added the reliability reviews.

This is the single cross-track index of work that is **designed but not yet
implemented**. The detailed designs stay in their own documents
([`connectivity-execution-plan.md`](connectivity-execution-plan.md),
[`durable-sessions.md`](durable-sessions.md),
[`vscode-over-relay-plan.md`](vscode-over-relay-plan.md),
[`mobile-ui.md`](mobile-ui.md),
[`security-followups.md`](security-followups.md),
[`copilot.md`](copilot.md),
[`mvp-execution-plan.md`](mvp-execution-plan.md),
[`refactoring-plan.md`](refactoring-plan.md),
[`classifier-measurement.md`](classifier-measurement.md)); this file only records
**what is pending, why it matters, what it depends on, and in what order to
pick it up**.

**Maintenance rules**

- When a milestone lands, mark it done here (and in its source doc) in the
  same commit.
- **Anything newly designed gets a row here before implementation starts** —
  new plans do not live only in their own doc.
- Priorities: **P1** = next up / on the product path, **P2** = important but
  gated or second in line, **P3** = valuable, not urgent, **P4** = deferred by
  explicit decision.

## Already done (context, do not re-plan)

- **Agent Deck (2026-07-22).** `/deck` is a phone/Stream Deck-shaped,
  button-first surface for live working/blocked sessions and provider-neutral
  approval choices. `GET /api/sessions/:id/deck` parses the current screen;
  revision-checked `POST …/deck/respond` sends only the arrow/Enter sequence
  needed for the selected option and does not steal the terminal attachment.
  The web UI supports 2×4, 4×4, and 4×6 layouts. Design + controller contract:
  [`deck.md`](deck.md).
- **Managed UI + UI-only gateway startup (2026-07-22).** `scripts/start.sh`
  now manages the persistent Vite UI with the daemon/holder and records its
  settings; `--ui-only` serves the same client without starting a local daemon,
  optionally proxying one daemon same-origin. This gives a browser-only gateway
  host a supported startup path. Setup and lifecycle commands are in
  [`setup.md`](setup.md).
- **Agent-driven commit action (2026-07-17).** The Git history panel can ask the
  session's own coding agent to inspect and commit its changes; bounded launch,
  provider-specific prompt submission, and discrete Enter handling avoid
  borrowing a different agent or losing the prompt to Codex input suppression.
- **Per-session model choice (2026-07-14, corrected 2026-07-23).** New sessions
  and forks may select a discovered provider model or keep the provider default.
  Codex discovery uses its initialized app-server model list (including
  pagination); fork launch preserves the resolved worktree and model argument
  ordering.
- **Session/mobile/operations polish (2026-07-13 → 2026-07-23).** Sessions get
  topic-derived titles; stop/archive confirm; archive navigates away; the
  directory picker can create a folder; unborn repos get a real initial commit;
  iPad gets copy/paste, a collapsible control panel and jump-to-end; session
  restore has an explicit loading state; stale terminal attachments are released
  and every attachment lifetime is logged. These are shipped refinements, not
  pending roadmap work.
- **Agent-driven conflict auto-resolution (2026-07-17).** A rebase/merge that hits
  conflicts no longer aborts on the spot: the daemon points an agent at the
  conflicted worktree (permission prompts bypassed) to resolve in place, then
  stages exactly the conflicted files, rejects any leftover conflict markers via
  `git diff --check`, and continues (`rebase --continue`/`--skip` in a bounded
  loop, or the merge commit). Only an unresolvable conflict — or no capable agent —
  aborts and fails as before (`MergeConflict` → 409). Covers all four paths:
  session `scm/{rebase,merge}` and workspace `branches/{merge,rebase}`. Gated on
  the session being an **agent** session: a session op is bound to exactly its own
  agent with no fallback, so a **shell** (or `custom_command`) session gets no
  auto-resolve and its conflicts stay manual; only the workspace ops, which belong
  to no session, use any installed claude/codex/opencode. New
  `plugins::AgentPlugin::conflict_resolver` (+ `ConflictResolveSpec`),
  `source_control::{ConflictResolver, ConflictContext, resolve_merge,
  resolve_rebase}`, and `conflict_resolve::AgentConflictResolver`. Proof: Rust
  units (a `FakeResolver` drives both loops; a marker-leaving resolver aborts) +
  sandbox E2E (claude resolved a real merge, codex a real rebase — both preserved
  each side, no markers, clean worktree).
- **Workspace branch management (2026-07-15).** A workspace-level git branch view,
  opened from an **(i)** icon next to each Git workspace in the tree
  (`SessionList.tsx` `.tree-actions`, gated on `is_git`), rendered by a new global
  `BranchManagerDialog`. For every local branch it shows (1) the sessions attached
  to it (from `workspace_instances.branch`, the sole session→branch link — new
  `db.list_active_instances_for_workspace`), (2) how far it is ahead of **its base**
  — the *same* reflog-derived `BaseCommit` the right panel shows, so `base_commit`
  was generalized to be branch-relative (not HEAD-relative) — and (3) how many of
  its commits are **merged nowhere else** (new `source_control::unmerged_commit_count`
  = `rev-list <branch> ^<every other ref>`). Management: **delete** (guarded — refused
  while checked out in a worktree; 409+force for unmerged), **merge** and **rebase**
  of arbitrary branches (new `source_control::{merge_branches,rebase_branch}` reuse
  the existing temp-worktree + abort/cleanup machinery; both refuse a branch a *live*
  session sits on). Endpoints `GET /api/workspaces/:id/branches/overview` +
  `POST …/branches/{delete,merge,rebase}` (branch in the body — names contain `/`).
  Proof: `scripts/branch-mgmt-test.mjs` (23 checks) + Rust units in `source_control.rs`.
- **Fork a session (2026-07-14)** — [`fork-session.md`](fork-session.md).
  Continue a session's work in a new one, on its branch or a branch off it, with
  its context. **This landed row REC**: forking a *stopped* session onto its own
  branch is recovery, and forking also does what REC could not — a **live**
  origin, and a **different** target agent. Same-agent forks resume the agent's
  own conversation natively (`claude --resume … --fork-session`, `codex fork`);
  everything else gets a brief. The brief's core is a **deterministic digest**
  read from the agent's transcript — ~1–4k tokens even from a 33 MB session — so
  no big LLM call is needed; an installed agent CLI optionally turns it into prose
  in seconds (`summarize.rs`), and a fork never fails if it can't. `agent_session_id`
  is captured **while the session lives** (SCHEMA_V7), because the transcript
  heuristics are too loose to pick a conversation to resume at fork time.
  Follow-ups: **FORK-SHELL**, **FORK-OC**.

- **Holder-theft hardening (2026-07-12 incident).** A test inherited the dev
  host's ambient `ASMUX_SOCK`, unlinked the live holder's socket, and six live
  sessions were lost. Fixed on three levels:
  1. **asmux will not displace a live holder** — it probes before unlinking and
     exits non-zero if anyone answers (`ASMUX_TAKEOVER=1` to override).
     `crates/asmux/src/socket.rs`.
  2. **asmux heals an unlinked socket** — a watchdog notices its path was removed
     or replaced and rebinds, so the PTYs survive instead of being orphaned.
     `server::serve_watched`.
  3. **Tests are sandboxed** — `scripts/lib/testenv.mjs` (`createSandbox()`); the
     then-existing 11 e2e scripts were migrated and the current 25-script suite
     follows the same isolated-daemon/holder/Chrome rule.
     `scripts/holder-theft-test.mjs` replays the incident as a regression.
  Also: `asmux probe` (Live/Stale/Free), daemon waits for the holder instead of
  dying on the first refused connect (`ASM_ASMUX_WAIT_MS`) and logs at ERROR, and
  `start.sh`/`status.sh`/`restart-daemon.sh` check the *socket* rather than the
  pid — a pid-only check reported a healthy holder right through the outage.
- MVP core loop end-to-end: sessions, attach/replay, snapshots, attention
  signals, agent plugins (shell/codex/claude/**opencode**/custom), Git SCM panel
  (status/diff/log/pull/rebase/**merge**/**push**), workspaces + per-session
  worktree isolation, device enrollment + bearer auth, multi-daemon client, i18n
  infrastructure (en-only), usage endpoint.
- Durable sessions **M1–M3**: `asmux` holder, `SidecarBackend`, adopt-on-restart
  (ring-replay), `indeterminate` state incl. client badge
  (`scripts/durable-restart-test.mjs` proves it).
- Connectivity **R1–R4**: `asm-relay` (dial-out-per-stream), daemon
  register-out + tunnel listener (loopback trust defeated), client relay
  support, and **gateway mode** — a NAT'd leaf *and* an egress-less downstream
  behind a gateway are both fully controllable from the browser with zero
  client tooling. Gateway (**R4**, 2026-07-07): the daemon probes
  `ASM_RELAY_DOWNSTREAMS` `/health` (cadence `ASM_RELAY_PROBE_INTERVAL_MS`) and
  feeds the relay agent a live reachable-annotated set over a `watch` channel;
  the relay attributes each downstream `via` its gateway and fast-fails
  `downstream_unreachable`; the client shows "D · via C". Proofs
  `scripts/relay-test.mjs` + `scripts/gateway-test.mjs` (15 checks). Loopback
  token-enforcement caveat: [`security-followups.md`](security-followups.md) → 11.
- VS Code correctness fix: relayed hosts get a disabled button + honest hint
  instead of a misdirected Remote-SSH deep link.
- File attachments (incl. image/screenshot paste): paste, drag-drop, or the 📎
  button feed **any file ≤ 10 MiB** into a live terminal → daemon stores it under
  `<cwd>/.asm/pastes/<stem>-<uuid>.<ext>` (`POST /api/sessions/:id/paste`, size
  validated; the filename is sanitised, the directory is server-derived) → client
  injects `[attached file <path>]` (or `[pasted image <path>]`) over the existing
  WS input frame → the agent reads it on submit. Widened from images-only on
  2026-07-12 so a PDF or a zip can be handed to an agent; the magic-byte
  allowlist is gone and size is the only bound. Design + as-built:
  [`image-paste.md`](image-paste.md); proofs `scripts/paste-test.mjs` + a
  headless-Chrome click-through of the 📎 button (PNG *and* PDF).
- Workspace upload (2026-07-21): an "Upload files" button + panel-wide drop zone
  in Details copies files to `<cwd>/uploads/<name>` (`POST
  /api/sessions/:id/upload`), so the agent finds them by listing a directory
  instead of needing a path pasted into the prompt. Keeps the user's filename —
  no uuid — which makes a collision meaningful, so the daemon answers `409` and
  the client confirms before replacing; a forced replace unlinks first so a
  planted symlink can't be written through. Design + as-built:
  [`image-paste.md`](image-paste.md) → *Workspace upload*; proofs
  `scripts/workspace-upload-test.mjs` (22 checks) +
  `scripts/workspace-upload-ui-test.mjs` (15 checks, headless Chrome).
- Code-quality cleanup pass (2026-07-06, zero functional change): shared
  backend snapshot/attach helpers, one git runner + `current_branch`, one CLI
  `build_launch` helper, db.rs idiom alignment, client `api.ts` error/URL
  helpers + shared `shortPath`; clippy silent, all suites green. Analysis and
  the follow-up RF-\* refactors: [`refactoring-plan.md`](refactoring-plan.md).
- **RF-M4 #1/#3/#4** — pre-M4 daemon refactor (2026-07-07, zero behavior change):
  `SessionManager` split into `session_manager/{mod,workspaces,monitor}.rs` (#1,
  method bodies verbatim, only `pub(super)` on the four cross-module methods);
  `db`/`registry` made private behind `db()`/`registry()` accessors (#3); and
  `MockBackend` holder seams + six `startup_reconcile` branch tests (#4) that
  guard M4's ring-replay→cold-stitch flip before it lands. 86 daemon tests pass;
  clippy clean; full smoke loop green. **#2 (reconnect-supervisor home +
  `AsmuxClient` trait) folded into M4** — see refactoring-plan.md → RF-M4 status.
- **TERM-SCROLL** — codex (normal-buffer agents) attach scrollback (2026-07-08,
  daemon-only, user-visible bug fix). Attaching to a running **codex** session,
  the client could scroll only a few lines and never reach the conversation
  start; **claude** was fine. Cause: codex renders its transcript in the normal
  buffer inside a bottom-margin `DECSTBM` scroll region (`ESC[1;47r`), and the
  daemon's snapshot emulator `vt100` drops lines scrolled off the top of such a
  region (real terminals + the client's xterm.js keep them — measured 0 vs 907 vs
  ~994), so `repaint_with_history` carried no history. Fix: a per-buffer-model
  attach strategy — alt-screen/self-scrolling apps (claude) keep the rendered
  repaint **unchanged**; normal-buffer apps replay a bounded (8 MB) in-memory
  raw-byte ring, appended under the broadcast lock in `reader_loop`/`drain_loop`,
  so the client's own emulator reconstructs scrollback + screen. Verified by the
  emulator matrix, `backend::tests`, and self-contained
  `scripts/termscroll-test.mjs` (codex-shape history delivered on attach; claude
  path unchanged). Design + as-built:
  [`terminal-scrollback.md`](terminal-scrollback.md). Adjacent to M4 cold-stitch
  (history beyond the ring) and RF-VT100 (the concrete defect motivating it).
- **M4 Stage A + B** — durable-session hardening (2026-07-11, daemon-only).
  **Stage A:** the daemon↔asmux connection gained a single reconnect owner — a
  supervisor task in `AsmuxClient` (dial → `hello` → re-attach every routed
  session `FromCursor(last_cursor)` → drain) with exponential backoff, a 10 s idle
  watchdog + heartbeat, in-place backpressure resync (`sidecar.rs`'s `Detached`
  arm no longer breaks), and a `list`-reconcile after every reconnect
  (`reconcile_after_reconnect` → shared `reconcile_from_holder`). `AsmuxClient`
  now implements a `Holder` trait so it is unit-testable (reconcile-branch tests +
  an in-process-asmux forced-drop → reconnect → resume test). Absorbs RF-M4 #2.
  **Stage B:** adopt is exact via **cold-stitch** — `backend_cursor` made exact in
  the event-batch transaction (`EventMsg.head_cursor`), `adopt` seeds `vt100` + the
  raw-history ring from SQLite cold history and `attach FromCursor(consumed)` for
  the tail, with a visible **gap marker** when the ring wrapped. A session whose
  output outgrew the 2 MiB ring now reconstructs exactly after restart
  (`durable-restart-test.mjs` cold-stitch discriminator; 117 daemon tests green).
  Remaining M4 work is **Stage C** (table row M4-C). Design + as-built:
  [`durable-sessions.md`](durable-sessions.md) → M4.
- **RF-MOB** — client shell prep for the MOB phase-1 split (2026-07-06,
  client-only, zero behavior change): `src/status.ts` unifies the three drifted
  `isLive` predicates (+ `isTerminal`; the RightPanel ended-list's omission of
  `indeterminate` was decided deliberately — it is **neither** live nor
  terminal, matching all prior call sites); `showUsage` moved into `useUiStore`;
  `useActiveSession()` extracted from `App.tsx` so both shells share one wiring.
  RF-MOB ride-along #4 (clipboard-with-fallback → `src/clipboard.ts`) was
  delivered independently by the terminal-selection-copy feature (`7a56cd3` on
  `release/next`), which hoisted the same `copyText()` out of `RightPanel` and
  wired it into both `RightPanel` and `Terminal`; on rebase the duplicate hoist
  was dropped and the shared util kept. `MobileShell` can now mount with no
  copied wiring. Full build gate (tsc + eslint + check-locales + vite build) and
  proxy tests green.
- **Attention classifier — per-provider split + the `error` state** (uncredited
  until the 2026-07-11 reconcile). `f4e81af` added `AttentionState::Error` for a
  turn that died mid-response (an API failure aborts the turn; the process lives,
  so it must not read as a calm `Idle` — `domain.rs:93,104`). `057d986` split
  classification per provider and stopped misreading codex's turn-complete as
  blocked; `2b8119f` (AskUserQuestion) and `2a31905` (Claude's plan-approval
  prompt) classify as **blocked**, not idle. The old "attention signals" bullet
  above predates all four.
- **SCM growth beyond the original panel:** `fddd528` **git push** (creates the
  remote branch when absent — `source_control.rs:127,473`) + `27e2a83` its icon;
  `a8f872e` **merge**; `13a6d93` + `2de284a` git-op feedback and dismissable
  status.
- **Save conversation** (`b7f071b`, upgraded by `78437a9`):
  `GET /api/sessions/:id/transcript` serves rendered provider Markdown by
  default; `?format=raw` serves the complete PTY byte stream (ANSI included),
  and raw is the automatic fallback for plugins with no structured transcript.
  There is no delta; 409 on archived, 404 on unknown. REC's byte-log fallback
  must explicitly select the raw source rather than assuming the user-facing
  download is raw.
- **Replay hygiene** (`1ee08a3`): `strip_terminal_queries()` removes DSR/DA/
  OSC-color *queries* from replayed scrollback (`backend/mod.rs:241-352`) so a
  reattach doesn't make the terminal answer questions the app never asked. A
  follow-on to TERM-SCROLL; **not** the SEC-8 escape policy (which stays pending).
- **Terminal copy/paste under TUI mouse reporting:** `7e908b7` copy-selection to
  the OS clipboard; `b682d0d` + `1e40c88` keep selection and copy/paste working
  while an app has mouse reporting on.
- **Clipboard image paste on Windows/Linux:** Ctrl-V now runs the browser's own
  paste instead of being swallowed as `^V` — until then only macOS (⌘-V) could
  paste an image at all, since Ctrl-Shift-V is paste-as-plain-text and arrives
  with the image stripped. See [`image-paste.md`](image-paste.md).
- **Terminal text selection on touch:** long-press selects a word, drag extends
  it (auto-scrolling at the edges), tap dismisses; the gesture drives xterm's own
  selection model, so the key bar's `Copy` needed no change. Also fixes a drag
  starting *on text* scrolling only one row — the DOM renderer replaced the span
  the touch had latched onto, silently starving the listener; the gesture now
  rides on pointer capture. `scripts/touch-select-test.mjs`, `mobile-ui.md`.
- **Jump to latest output on a phone** (2026-07-13, client-only): a `.term-jump`
  pill floats over the mobile terminal whenever the view has left the live tail,
  and returns it. It has to serve the two scroll models `terminal-scrollback.md`
  diagnosed, which is the whole of the design: for terminal-owned scrollback
  (codex, shell, replays) `viewportY < baseY` is exact and `scrollToBottom()` is
  the way back; for an **app-owned** scroll (claude — alt screen + mouse
  reporting) xterm's buffer never moves at all, so the wheel-UP *reports* the app
  was handed are counted and handed back as wheel-DOWNs (it clamps at its own
  bottom, so overshoot is free — which is what makes an approximate counter
  safe). `scripts/term-jump-test.mjs` drives a real shell into each shape.
- **Soft return** (2026-07-24, client-only): **Shift+Enter** puts a newline in
  the agent's composer instead of sending the message; plain Enter still sends.
  xterm maps the chord to a bare CR — byte-identical to Enter — so the key every
  agent's own footer advertises for "newline" was silently submitting. The wire
  form is `SOFT_RETURN` = **ESC+CR** (`terminalTypes.ts`), which is Claude Code's
  own choice (`/terminal-setup` writes exactly that sequence into VS Code's
  keybindings) and which codex + opencode also read as a newline. A bare LF works
  in all three too and is still wrong: a `shell` session has no composer, and to
  readline LF *is* accept-line — Shift+Enter would RUN the half-typed command,
  where ESC+CR is merely unbound. Soft keyboards can't type the chord at all, so
  the phone key bar and the iPad control panel grow a `⇧⏎` key.
  `scripts/soft-return-test.mjs` proves both halves: the frame on the wire (and
  that xterm's CR is swallowed, not sent alongside) and the ESC arriving at the
  pty, via a `cat -v` session where control bytes are visible.
- **Client polish, uncredited:** `97cfe0d`/`3dad62e` two-level connection dialog
  (Existing/Add × Daemon/Relay); `f7c7640` app icons + blocked-session favicon
  blink and `d291a4e` tab-title blink (`f7c7640` also shipped most of MOB-PWA —
  see that row); `7e11381` image before/after previews in the diff panel;
  `8cb9e08` swipe-scrollable mobile terminal; `3b08874` hide "Continue in VS Code"
  on phones; `ee1d352` action icons.
- **`c6ad936` daemon-served client** (works on hosts with no npm/vite) and
  **`dda8354`** frontend bound to `0.0.0.0`. Flagged here because they *widen the
  exposure surface* the SEC track is about: the daemon now serves a UI to any
  reachable host, which raises the stakes on **SEC-1** (plaintext off-loopback;
  since decided as by-design — see its row), **SEC-5** (permissive CORS), and
  **SEC-6** (unconditional loopback trust).
- **MOB phases 1–3** — mobile adaptive shell (2026-07-06, client-only, no daemon
  changes). Phase 1: `useIsPhone()` (PHONE_MQ) switches the root between the
  extracted `DesktopShell` and a new stacked `MobileShell` (Sessions home →
  full-screen Terminal → Details sheet overlay keeping the WS mounted);
  `App.tsx` is now the device switch + shared dialogs + `#s=` deep-link; browser
  history mirrors the layer stack via pushState so system-back unwinds one layer;
  new store flag `showDetails`. Phase 2: a `@media PHONE_MQ` block gives the
  shared components touch targets (≥44px rows) and turns modals into bottom
  sheets; 100dvh + safe-area insets; `viewport-fit=cover`. Phase 3: `TermKeyBar`
  (Esc/Tab/⇧Tab/Ctrl-latch/^C/arrows/⌨/Paste/Copy) drives a new `TerminalView`
  input handle (write/focus/getSelection) over the same WS path; the Ctrl latch
  transforms the next soft-keyboard key; `useVisualViewportHeight` keeps the bar
  above the keyboard (interactive-widget meta). All shared components reused
  unchanged — parity is structural. Verified end-to-end at 390×844 against a live
  shell session (`scripts/mobile-shell-test.mjs`, ALL PASS) + desktop regression.
  Remaining: **MOB-PWA** (phase 4) and **MOB-PUSH** below.

## Backlog summary

| ID | Item | Priority | Depends on | Source (design) |
| --- | --- | --- | --- | --- |
| ~~REC~~ | **Session recovery — landed, as the more general [fork](fork-session.md).** Forking a *stopped* session onto its *own* branch is exactly recovery, so one feature covers both, and it also does what REC could not: fork a **live** session, and fork onto a **different agent**. Shipped: the `AgentPlugin` fork seam, SCHEMA_V7 (`agent_session_id` + `forked_from`), native-id capture while live, `POST /api/sessions/:id/fork`, native fork for claude + codex, the digest/summary brief, and the UI. Two pieces of the old design remain, split out as **FORK-SHELL** and **FORK-OC** below. | ~~P1~~ **done** | — | fork-session.md |
| FORK-SHELL | Fork a **shell / custom_command** origin usefully: it keeps no transcript, so it gets a brief with no digest today. Its real context is its scrollback (`terminal_events` → vt100 → ANSI-stripped prose) — the one source where a shell's brief is genuinely *good* (a shell has real scrollback; the TUI agents have none, which is why they use their own transcripts). This is REC Stage B. | **P3** | fork (done) | session-recovery.md → §3; fork-session.md |
| FORK-OC | **opencode native fork**: read its conversation id out of `~/.local/share/opencode/opencode.db` and launch `opencode --session <id> --fork`. Pure plugin change — `native_session_id` + `build_fork`; the fork flow does not move. An opencode fork gets the brief meanwhile. This is REC Stage C. | **P3** | fork (done) | session-recovery.md → §2; fork-session.md |
| ~~FIX~~ | **Verified latent defects — all eight landed 2026-07-25**, each with a regression test: `pull` credential-prompt hang (one `git_command` builder owns `GIT_TERMINAL_PROMPT=0`, guarded by a test that no git child bypasses it); adopt→reconnect cursor-0 rewind (routes are seeded with the cursor their attach uses, and the drain's duplicate tracker is gone); worktree created before create-validation (validate first, roll the instance back on any later failure incl. a failed spawn); asmux silent ring-alloc output drop (logged, counted, and reported to the attacher as `ALLOC_FAILED`); `exit_signal` never populated (`waitpid` decomposes the real wait status); fabricated adopt defaults (a vanished row is logged and skipped); `indeterminate` authorizing destruction (new `is_definitively_ended` gates archive / cleanup / workspace-unregister, matching the client); invalid `ASM_BACKEND` (rejected at startup instead of silently selecting non-durable native) | ~~P1~~ **done** | — | refactoring-plan.md → §6.1 |
| RF-FLOW | **Bounded terminal flow + durable persistence:** byte-budget output/command queues with explicit overload semantics; writer health + retry/degraded state + shutdown flush; acknowledged input/kill semantics; streaming history/transcripts; retention/compaction; disk-full/slow-consumer fault tests | **P1** | FIX + minimal RF-GATE harness | refactoring-plan.md → §7.1 |
| RF-LIFE | **Explicit lifecycle state machine + units of work:** named status capabilities, conditional transitions/per-session serialization, atomic metadata/migrations, create+teardown sagas, central `go_live`/`finish`, concurrency/failure-step tests | **P1** | FIX + minimal RF-GATE harness | refactoring-plan.md → §7.2 |
| RF-REC | **Generic Git/API/UI consolidation (legacy id; no longer blocks recovery):** one deadline/output-bounded `GitRunner`; `require_session` + typed `run_blocking`; split `ScmPanel` / session metadata / VS Code blocks out of the now-1,209-line `RightPanel`. | **P2** | FIX #1 for the immediate pull guard; pairs with RF-ERR on API helpers | refactoring-plan.md → §6.2 |
| M4-C | Holder hardening **Stage C**: soft-reboot (hash drift + confirm), orphan surfacing/adopt UI, `purge`, metadata RPCs, `readBuffer`, periodic `(snapshot, cursor)` store (bounds cold-stitch replay cost). *(Slow-attacher drop + resync — previously listed here — landed with Stage A.)* | **P2/P3** | M4 A/B (done) | durable-sessions.md → M4 Stage C |
| MOB-PWA | Mobile UI phase 4: **iOS `apple-mobile-web-app-*` meta tags only** — the web manifest, maskable icons, apple-touch-icon and theme-color already shipped in `f7c7640` | **P3** (was P2; mostly done) | MOB (done) | mobile-ui.md → Packaging path |
| MOB-PUSH | Web Push for attention states | **P3** | MOB (done); RF-WSPROTO (server→client frame type); daemon push plumbing (relay as carrier) | mobile-ui.md → Follow-ups |
| IMG-2 | Attachment follow-ups: `.asm/pastes/` cleanup policy (more pressing now that a 10 MiB zip can land there), multi-file select/drop **on the 📎/paste path** (the Details-panel workspace upload already takes several at once), per-agent capability hint | **P3** | attachments + 📎 button (done) | image-paste.md → Follow-ups |
| RF-ERR | Typed daemon error → HTTP status mapping (RelayError-style) | **P2** | — (pair with SEC-2 or RF-REC) | refactoring-plan.md → RF-ERR |
| RF-GATE | **Minimal P1 harness landed 2026-07-26:** CI; real router/AppState + attachment tests; scripted `MockHolder`; asmux e2e for readBuffer/detach/backpressure/takeover/malformed bodies. Remaining: react-hooks + recommended eslint, `generated.rs` drift check, migration-ladder test + `user_version` guard. | **P2 remainder** | — (remaining guards before their affected work) | refactoring-plan.md → §6.4 |
| RF-OPS | **Truthful health, deadlines and task supervision:** liveness/readiness probes for DB writer + holder; cancellation tree for background tasks; blocking-work boundary; child/client request deadlines; jittered reconnect; structured reliability metrics | **P2** | minimal RF-GATE; Git deadline implementation shares RF-REC | refactoring-plan.md → §7.3 |
| COPILOT-REVIEW | **Session copilot, manual advisory MVP:** select any capable installed agent plugin + model; capture committed/staged/unstaged/untracked code into an immutable disposable Git worktree; run a bounded structured review; persist evidence/findings/checks; show current/stale history in a standalone panel; explicitly send a review file to the main agent | **P2** | FIX + minimal RF-GATE; shared `GitTreeSnapshot` slice of MVP-CKPT; RF-OPS child runner; pair UI/API work with RF-REC + RF-QUERY | copilot.md → Stages 0–1 |
| COPILOT-GATE | **Automatic copilot + gatekeeper:** debounced idle review, fingerprint deduplication, budgets and bounded feedback; then require a current passing review + deterministic checks before ASM promotes the exact reviewed commit. Gate ASM actions only; direct terminal Git remains bypassable unless a remote required check enforces it. | **P2** after advisory proof | COPILOT-REVIEW; RF-ERR; relevant RF-LIFE SCM serialization; exact-commit promotion | copilot.md → Stages 2–4 |
| SEC-2 | Constrain `/api/fs/list` + workspace roots | **P1** | RF-ERR recommended (403 mapping) | security-followups.md → 2 (HIGH) |
| SEC-1 | Transport encryption off-loopback. **DECIDED 2026-07-12: the LAN direct path is plaintext by design.** TLS was built end to end (`1dcb15e`: agent `wss://`, relay rustls, daemon HTTPS) and **reverted** (`a36fdfa`): a self-signed daemon cert is refused by the browser on the client's cross-origin `fetch` (no interstitial, no API) — unreachable by construction — and both escapes (a public name + ACME; a private CA installed per device) violate the product constraints: no external dependencies, no per-device setup, journey immutable. Encryption returns at the **relay** when R5 gives it a real name + ACME cert (resurrect the relay half of `1dcb15e`); SSH port-forward is the encrypted path meanwhile | **folded into R5** (was P1/P2) | R5 | security-followups.md → 1 |
| V0 | Web-editor de-risking spike (scratchpad only) | **P2** | R1–R3 (done) | vscode-over-relay-plan.md → V0 |
| V1 | Relay cookie auth + daemon editor proxy | **P2** | V0 go decision | vscode-over-relay-plan.md → V1 |
| V2 | IDE launcher, detection, editor tickets | **P2** | V1 | vscode-over-relay-plan.md → V2 |
| V3 | Web-editor client wiring, lifecycle, polish | **P2** | V2 | vscode-over-relay-plan.md → V3 |
| R5 | Relay hardening & productization (itemized) | **P2** | R4 for full scope; several items standalone | connectivity-execution-plan.md → R5 |
| SEC-3 | Enrollment token expiry/rotation/rate limit | **P2** | overlaps R5 pairing-code item | security-followups.md → 3 (MEDIUM) |
| SEC-4 | Hash device tokens at rest | **P2** | — | security-followups.md → 4 (MEDIUM) |
| SEC-5 | Restrict CORS origins | **P2** | check against V1 cookie design first | security-followups.md → 5 (MEDIUM) |
| SEC-6 | Optional "always require token" (no loopback trust) mode | **P2** | — | security-followups.md → 6 (MEDIUM) |
| RF-WSPROTO | Client↔daemon WS contract: written spec (mirroring asmux-protocol.md) + shared `terminalProtocol.ts` (frame union, server-frame demux with reserved control branch, hello/version) — while there are exactly two frame types | **P2** | — ; before whichever of MOB-PUSH / MVP-RICH / V3 comes first | refactoring-plan.md → §6.3 |
| RF-QUERY | Client data-layer consolidation (query-key factory, `useDaemonMutation`, api.ts split) | **P2** | DEC-1; before MVP-RICH | refactoring-plan.md → RF-QUERY |
| MVP-RICH | Rich output pipeline (viewer/diff/markdown/transcripts) | **P2** | client-stack decision DEC-1; RF-QUERY; RF-WSPROTO (server→client frame type) | mvp-execution-plan.md → Workstream 4 |
| MVP-HOOKS | Workspace setup hooks (copy/symlink/bootstrap) | **P3** | — | mvp-execution-plan.md → Workstream 6 |
| MVP-CKPT | Checkpoints + "New segment" | **P3** | — | mvp-execution-plan.md → Workstream 7 |
| M5 | Windows support (ConPTY, transport, ACLs) | **P3** | M4 (sequenced) | durable-sessions.md → M5 |
| MVP-PKG | Electron shell + packaging + service install | **P3** | DEC-1; M5 for the Windows gate | mvp-execution-plan.md → Workstreams 3/11 |
| SEC-7 | Auth rate limiting + lifecycle audit log | **P3** | overlaps R5 | security-followups.md → 7 (LOW) |
| SEC-8 | Terminal-escape policy at capture/replay + fuzzing | **P3** | — | security-followups.md → 8 (LOW) |
| DEC-1 | Decide: adopt planned client stack (shadcn/Tailwind/Dockview/Electron) or amend the plan | **P2** (decision, cheap) | — | mvp-execution-plan.md → Baseline Technology |
| RF-HYG | Hygiene bundle (hours-each, pick per need): backend constants hoist, `AttentionState::is_sticky()`, classifier parser dedup, **`MonitorState` extraction (before MEAS)**, write-loop + attach-resync dedup, asmux consistency items, `theme.ts` + dead-CSS sweep, `TerminalHeader`, **overdue Terminal.tsx split** (the file reached 1,087 lines after more terminal features), `createTopology()` e2e helper | **P3** | — (`MonitorState` blocks MEAS; Terminal split trigger has fired) | refactoring-plan.md → §6.5 |
| RF-VT100 | Terminal emulator dependency review (`vt100` 0.15 unmaintained) | **P3** | — (trigger: M4 cold-stitch work or upstream CVE) | refactoring-plan.md → RF-VT100 |
| MEAS | Classifier measurement: local-LLM shadow classification of any registered heuristic (attention first), disagreement snapshots + triage (dev-only, default-off) | **P2** | RF-HYG's `MonitorState` extraction first (RF-M4 landed; the hooks observe its `Classification` output); needs a local Ollama/llama-server on the dev host | classifier-measurement.md → Milestones |
| ~~DOC-1~~ | Relay framing docs corrected to the shipped **dial-out-per-stream** design; stale yamux references removed from architecture and the protocol comment. | ~~P3~~ **done 2026-07-24** | — | architecture.md; asm-relay/src/protocol.rs |
| I18N-2 | Additional locales beyond `en` | **P4** (deferred by user) | — | i18n.md → Adding a locale |

## Detail

### REC — session recovery via fork (done)

Full as-built design: [`fork-session.md`](fork-session.md). Historical recovery
reasoning: [`session-recovery.md`](session-recovery.md).

Recovery shipped on 2026-07-14 as a strict superset: fork a stopped session onto
its own branch to recover it, or fork a live/stopped origin onto a new branch or
different agent. SCHEMA_V7 persists `agent_session_id` and `forked_from`; the
monitor captures provider identity while the origin lives; Claude and Codex
resume/fork natively; other paths get a bounded deterministic transcript digest
plus an optional locally generated prose summary. The API is
`POST /api/sessions/:id/fork`.

Only two pieces of the old REC staging remain: **FORK-SHELL** (derive a useful
brief from PTY scrollback when no agent transcript exists) and **FORK-OC**
(opencode native conversation fork). Neither blocks the shipped recovery path.

### FIX — verified latent defects (P1)

Eight defects found and hand-verified by the 2026-07-12 reviews; small
individually (~2–3 days total including regression tests), and several sit
directly under REC:

1. `pull` lacks the `GIT_TERMINAL_PROMPT=0` guard `fetch`/`push` deliberately
   set — a missing/expired credential wedges a `spawn_blocking` worker on an
   interactive prompt that never gets answered.
2. Adopt→reconnect rewinds to cursor 0: `Route.last_cursor` is seeded 0 and
   only advanced by live output, so a socket drop before the first
   post-adopt chunk re-attaches `FromCursor(0)` and renders duplicated
   scrollback on the exact path M4 Stage B made exact (the drain loop keeps
   a second, correctly-seeded tracker — collapse them).
3. `create_session` physically creates the worktree **before** the
   launch/approval/cwd validation and never rolls back — every rejected
   create leaks a worktree + branch with no DB row (the orphan class
   `cleanup_orphan_worktrees` exists to sweep); the spawn-failure arm also
   leaves the instance `active`.
4. asmux silently drops PTY output on ring-alloc failure; `ALLOC_FAILED` is
   defined but never emitted — the never-crash invariants promise the
   opposite.
5. `exit_signal` is never populated: `kill -9` reads as a normal exit 137.
   Decide (populate on Unix, or mark the field reserved) **before** REC
   reads exit status.
6. `reconcile_from_holder` fabricates 24×80 geometry + `created_at = now`
   when a session row vanishes mid-reconcile — silent corruption; log +
   skip instead.
7. The client defines `indeterminate` as unresolved (neither live nor
   definitively terminal), while the daemon's broad `is_terminal()` includes it.
   Direct API calls can therefore archive/delete its worktree or unregister its
   workspace even though the UI intentionally withholds those actions.
8. Any explicit `ASM_BACKEND` value other than exact `sidecar` silently selects
   the in-process native backend, so a typo boots successfully without durable
   sessions. Unknown explicit values must fail config validation.

Evidence, line refs, and fixes: refactoring-plan.md → §6.1.

### RF-FLOW / RF-LIFE — reliability prerequisites (P1)

RF-FLOW closes the missing end-to-end durability contract: byte-budgeted
queues, explicit overload behavior, persistence writer health/retry/flush,
acknowledged input/kill, streamed history and a retention policy. RF-LIFE
replaces overloaded status predicates and best-effort multi-resource updates
with a checked transition table, conditional writes/per-session serialization,
atomic metadata/migrations, retryable create/teardown sagas and central
`go_live`/`finish` seams. Both include fault/concurrency tests. Full evidence and
acceptance criteria: refactoring-plan.md → §7.1–§7.2.

### RF-REC — generic Git/API/UI consolidation (legacy id, P2)

REC shipped without this bundle, so it is no longer a recovery prerequisite.
The work remains valuable and has grown: a deadline/output-bounded
**`GitRunner`** (also closes FIX #1 structurally), **`require_session` + typed
`run_blocking`** for the repeated API spine, and `ScmPanel` / session metadata /
VS Code extraction from the now-1,209-line `RightPanel`. Keep the legacy id so
existing references do not churn; sequence it as consolidation after the
immediate P1 defect and lifecycle work. Detail: refactoring-plan.md → §6.2.

### RF-OPS — operability and bounded waits (P2)

Make `/health` truthful about DB writer/holder readiness; supervise and cancel
long-lived tasks; isolate synchronous DB/Git/filesystem work from async workers;
apply deadlines/kill/output caps to children and requests; pass TanStack abort
signals through the client; use capped jittered WS reconnect; expose reliability
metrics. Detail and acceptance criteria: refactoring-plan.md → §7.3.

### COPILOT-REVIEW / COPILOT-GATE — independent session review (P2)

Full design: [`copilot.md`](copilot.md).

A copilot is a policy attached to a session, not a second PTY or terminal
attacher. Each run materializes the exact Git tree—including committed, staged,
unstaged, and non-ignored untracked work—through a private temporary index, then
gives a disposable detached worktree plus a deterministic session digest to a
selected plugin/model. The reviewer cannot race or rewrite the main agent's
worktree. Parsed findings carry severity, path/line evidence, recommendations,
and daemon-recorded deterministic checks; a changed base/HEAD/tree or policy
makes the prior verdict stale.

Stage 1 is deliberately manual and advisory. Stage 2 adds debounced idle
triggers and an opt-in, bounded feedback loop. Stage 3 gives a configured
gatekeeper authority over ASM-owned promotion, beginning with pushing the exact
reviewed clean commit. This is not universal repository protection: a user or
agent can still run Git directly in the terminal. Protected remote branches and
a required status adapter are the later enforcement boundary.

The snapshot capture is the minimum shared Git plumbing slice of MVP-CKPT, not
a reason to build all checkpoint UI first. The reviewer needs a dedicated
model-aware `AgentPlugin` capability: the existing fork summarizer has an empty
cwd/no model selection, while conflict resolution deliberately bypasses
guardrails in the real worktree. Reuse RF-OPS process supervision and keep the
new client surface out of `RightPanel.tsx` from its first commit.

### M4-C — Holder hardening Stage C (P2/P3)

**Stages A + B landed 2026-07-11** (see "Already done"): the daemon↔asmux
reconnect supervisor + idle watchdog + `list`-after-reconnect reconciliation +
`Holder` trait (Stage A, absorbing RF-M4 #2), and the exact cold-stitch adopt +
gap marker (Stage B). The headline "terminal intact after restart" promise now
holds for long-lived sessions whose output outgrew the ring.

Orthogonal to the R-track (explicitly: R code must not assume M4 exists).
**Stage C — remaining scope:** soft-reboot on `binary_sha256` drift (warn +
confirm — the holder's hash field is still "empty for now", `asmux/src/server.rs:42`),
orphan surfacing/adopt UI (the client has only the `indeterminate` badge today),
`purge` / metadata / `readBuffer` RPCs (**holder-side already implemented** —
`asmux/src/server.rs:380,464`, `registry.rs:133` — but the daemon never issues
them: its `Holder` trait encodes only CREATE/LIST/RESIZE/KILL/HELLO/ATTACH, so
this is daemon-side wiring, not protocol work), and the **periodic
`(snapshot, cursor)` store** that would bound cold-stitch's full cold-history
replay on adopt (Stage B replays all of it — correct, and fine for realistic
session sizes on a one-time adopt). None are on the critical durability path;
pick per need.

**Corrected 2026-07-11:** *slow-attacher drop + resync* used to be listed here as
"the protocol/eviction plumbing exists; the daemon policy does not". That is no
longer true — it landed with Stage A: `sidecar.rs:389-404` handles
`DETACH_BACKPRESSURE` by re-attaching `FromCursor(last_cursor)`.

**2026-07-12 review notes:** the daemon demux already pre-wires the
purge/updateMetadata/readBuffer/detach **response** arms
(`asmux_client.rs:682-688`) that nothing can trigger — don't mistake them
for a request path — and `HolderSessionInfo.head_cursor`'s "reserved for
exact cold-stitch" comment is stale (Stage B landed without it; correct it
to "unused; M4-C readBuffer/orphan surfacing"). The holder-side
`handle_read_buffer`/`handle_detach` have **no test and no caller**; land
RF-GATE's asmux e2e for them *before* wiring the daemon side. Detail:
refactoring-plan.md → §6.6.

### MOB follow-ups — phases 4–5 remaining (P2/P3)

Phases 1–3 shipped (see "Already done"); the mobile app is usable and the
terminal genuinely workable on a phone, verified against a live session. What's
left of the `mobile-ui.md` plan:

- **MOB-PWA** (phase 4): **mostly already shipped, unnoticed.** `f7c7640` (app
  icons) added `client/public/site.webmanifest` — `display: standalone`, theme/bg
  `#07111f`, 192/512/1024 maskable icons — plus the `<link rel="manifest">`,
  apple-touch-icon and theme-color tags (`client/index.html:9,26,27`). What is
  genuinely missing is only the iOS meta tags (`apple-mobile-web-app-capable`,
  `-status-bar-style`, `-title`; no match anywhere in `client/`). Demoted to P3:
  "Add to Home Screen" already largely works.
- **MOB-PUSH**: Web Push for `approval_needed`/`likely_blocked` — its own row
  because it needs daemon-side push plumbing with the relay as carrier; design
  that before building.
- Smaller follow-ups noted in the doc (attention-pinned home group, font-size
  control, tap-and-hold tooltips) slot in later.

### RF-ERR — typed daemon error mapping (P2, pairs with SEC-2)

A `DaemonError` enum with one `IntoResponse` (modeled on the relay's
`RelayError`), replacing the blanket `anyhow → 400`, per-handler 404
hand-builds, and the one-off `NeedsForce → 409` downcast. Gives SEC-2 its
403 and M4 `purge` its conflict codes for free. One deliberate behavior
decision inside (aligning the two cleanup endpoints' `NeedsForce` to 409) —
see refactoring-plan.md → RF-ERR. 2026-07-12: pair with RF-REC #3
(`require_session` + typed `run_blocking`) — same files, same migration.

### RF-GATE — build gate & test safety net (P1/P2)

The net everything else in this table relies on, and it is much thinner than
it looks: client eslint enforces a **single** i18n rule (no
`react-hooks/exhaustive-deps`, no recommended set — in a codebase leaning on
the ref-to-dodge-a-dep pattern); there is **no CI at all** (`npm test` runs
only the proxy test; the 25 e2e scripts run when a human remembers); one pure
unit test covers `needs_live_session`, but **no Rust test constructs the
`api/` router/AppState or covers `ws.rs`'s takeover logic**; the `Holder` trait
has **no mock**, so the
drain-loop / adopt / backpressure branches are unit-untested (the one
integration test would not catch FIX #2); asmux's
readBuffer/detach/backpressure-eviction/takeover paths have **no e2e** (the
first two also have no caller — land these tests before M4-C wires the
daemon side); the committed `generated.rs` has **no schema-drift check**
(the protocol doc now correctly records manual generation); the DB
migration ladder is untested and a forward-rolled `user_version` is silently
accepted. ~2–3 days. Detail: refactoring-plan.md → §6.4.

### RF-WSPROTO — client↔daemon WS contract (P2; before MOB-PUSH / MVP-RICH / V3)

The one protocol in the system with no written contract, no version field,
and no room for a new frame type: close codes and `{"t":"i"|"r"}` frame
shapes are hand-mirrored between `api/ws.rs` and `Terminal.tsx` (plus ~16
literal sites across the e2e scripts), and the client treats **every**
server→client message as terminal bytes — a control frame has nowhere to
land; it would be written into the screen as garbage. Deliverables: a
contract doc mirroring `asmux-protocol.md` (close codes, control frames,
snapshot/history/sentinel semantics, reserved tag space, hello/version) and
a shared `client/src/terminalProtocol.ts` (a `ClientFrame` union +
`encode()`, and a `ServerFrame` demux with a reserved control branch)
consumed by the client **and** the e2e scripts. ~1 day now, while there are
exactly two frame types; MOB-PUSH, MVP-RICH and V3 each need a
server→client frame and would otherwise each invent one. Detail:
refactoring-plan.md → §6.3.

### SEC-1 (decided) / SEC-2 — the security items

Gate exposing ASM beyond a trusted LAN. SEC-2 (fs-list browses the whole host
filesystem; any client can register any workspace root) is self-contained
daemon work: server-side allowed-roots config enforced for both browsing and
registration — **now the only HIGH item**.

SEC-1 is **decided, not open** (2026-07-12): the LAN direct path is plaintext
by design. The full TLS implementation (agent `wss://`, relay rustls,
daemon-terminated HTTPS — `1dcb15e`) was built, found to make the daemon
*unreachable* in the product's own journey (browsers refuse a self-signed cert
on the client's cross-origin `fetch`, with no interstitial and no API), and
reverted (`a36fdfa`) after both escapes — a public name + ACME cert, or a
private CA installed on every device — were rejected as violating the product
constraints (no external dependencies; no per-device setup; the connect
journey is immutable). Full reasoning: security-followups.md → 1. Encryption
returns at the **relay**, which can hold a real ACME cert for a real hostname
with zero journey change — that is an R5 work item, and the relay half of
`1dcb15e` should be resurrected from history for it, not rebuilt. Until then
the SSH-tunnel recommendation stays prominent.

SEC-2 code-level substrate (2026-07-12 review): there are today **three**
divergent notions of allowed root — `fs::list` enforces none;
`register_workspace` self-widens the set; and `resolve_workspace`'s inline
check is **skipped entirely when no workspace is registered** and rides on
`canonical()`'s fall-back-to-raw-path on error, so `..` segments survive
into a textual `starts_with`. Plan SEC-2 as one new `allowed_roots` module
consumed by all three call sites, not a patch. Detail:
refactoring-plan.md → §6.6.

### V0–V3 — browser VS Code over the relay (P2)

Design complete in `vscode-over-relay-plan.md`; **V0 gates everything** (flag
matrix for `code serve-web` / `openvscode-server` under
`--server-base-path=/n/<id>/editor`, service-worker/IndexedDB behavior through
the relay, connection-token mechanics — go/no-go on the canonical-base-path
design). V1 is the only relay change (cookie key auth) + the daemon editor
proxy; V2 launcher/tickets; V3 client wiring. This is also the universal
remote-editing path for non-relayed hosts without SSH, so it upgrades the
currently disabled button everywhere. Start after R4 unless the editor is
needed sooner — they don't conflict.

### R5 — relay hardening & productization (P2, itemized — pick per need)

- **Decision gate: splice-point confidentiality** — relay/gateway see stream
  plaintext today. Accept for personal deployment vs build app-layer crypto
  (WebCrypto keyed at enrollment). Update security-followups.md either way.
  The web editor (V-track) raises the value of this exposure — revisit when
  V-track ships.
- Per-owner/per-node relay ACLs, key rotation, rate limiting on `/register`
  and auth failures (overlaps SEC-3/SEC-7).
- Pairing-code enrollment brokered through the relay (replaces token paste;
  also listed as an architecture.md open decision).
- Ops: deployment.md relay section (systemd, TLS, 443), metrics/log surface,
  `--version`/health endpoint. Also (2026-07-12 review): the relay's `nodes`
  map never evicts offline entries (`snapshot()` lists them forever), and a
  per-connection heartbeat thread + writer task linger after a
  protocol-error return until the peer closes — fold both into this item.
- Client polish: relay health row, per-node latency hint, reconnect toasts.

### RF-QUERY — client data-layer consolidation (P2, before MVP-RICH)

`RightPanel`'s 5-query/3-mutation shape is the template every MVP-RICH panel
will clone — fix the template first: a `queryKeys` factory (keys are ad-hoc
string arrays across ~8 files today), a `useDaemonMutation` centralizing the
invalidation calls hand-rolled at 6+ sites, and an `api.ts` split into types
+ domain-grouped modules. Detail: refactoring-plan.md → RF-QUERY.

### MVP-RICH — rich output pipeline (P2)

Workstream 4 of the MVP plan is essentially unbuilt (no CodeMirror, Marked,
DOMPurify, Shiki, Mermaid, KaTeX in `client/package.json`; diffs render
plain). Scope: read-only file viewer + proper diff viewer, repo markdown
preview, agent transcript preview via plugin parsers, session summary
renderer, sanitization boundary, lazy loading. High user-visible value;
independent of R/M/V tracks; blocked only on DEC-1 (don't pull in a component
stack before deciding whether the plan's baseline still stands).

### MVP-HOOKS — workspace setup hooks (P3)

Copy rules (`.env` and friends), symlink rules (dependency caches), bootstrap
command execution, hook status surfaced in the UI. Makes worktree isolation
practical for real projects whose builds need untracked local files.

### MVP-CKPT — checkpoints + "New segment" (P3)

App-managed checkpoint refs/objects, manual checkpoint update, explicit "New
segment" action, optional plugin boundary-detection hook. No checkpoint code
exists in the daemon today.

### M5 — Windows (P3)

ConPTY via portable_pty, AF_UNIX-or-named-pipe transport (tokio has no AF_UNIX
on Windows), `0600` → owner-only ACL. Note the R-track deliberately added no
new UDS surface, so M5 scope did not grow. Prerequisite for the MVP gate's
"works on Windows" and for MVP-PKG's Windows packaging. 2026-07-12 note:
AF_UNIX is hard-typed on the daemon side with no transport seam
(`main.rs:344-433` probe/spawn; `asmux_client.rs` read/write halves) —
budget a small `HolderTransport` (probe/connect) abstraction into the
estimate; not worth pre-building at P3. refactoring-plan.md → §6.6.

### MVP-PKG — Electron + packaging (P3)

Electron shell (renderer hardening checklist), daemon + client packages for
three OSes, first-run setup, user-scoped service install (systemd user unit /
LaunchAgent / Windows per-user startup — remember the holder must live outside
the daemon's cgroup: `systemd-run --user --scope`), upgrade path, diagnostics
export, quickstart docs. Gated on DEC-1; the Windows leg on M5.

### SEC-3..8 — remaining security follow-ups (P2/P3)

See `security-followups.md` for full guidance; keep that doc and this table in
sync as items land. SEC-5 (CORS) interacts with the V1 cookie design — decide
them together. SEC-9 (danger toggles) and SEC-10 (usage endpoint outbound
call) are accepted-as-designed; revisit only if a host-policy/no-egress mode
is added.

### DEC-1 — client-stack decision (P2, cheap, unblocks two items)

The MVP plan's frontend baseline (shadcn/ui, Tailwind, Dockview, CodeMirror,
Electron) was never adopted — the shipped client is plain React 19 + Vite +
xterm.js and has gone quite far on that footing. Decide: adopt the planned
stack incrementally (starting with MVP-RICH's CodeMirror), or amend
`mvp-execution-plan.md` to match reality. Blocks MVP-RICH and MVP-PKG from
starting with confidence.

### RF-HYG — hygiene bundle (P3, opportunistic; two items have ordering triggers)

Hours-each items from the 2026-07-12 review, pick per need. Daemon/backend:
backend constants hoist (`BROADCAST_CAP`/`SCROLLBACK` duplicated verbatim
between the two backends, bare RPC-timeout literals, a hardcoded wire-enum
value); `AttentionState::is_sticky()` (the sticky-state set is a
copy-pasted `matches!` at two hot sites); attention-classifier parser dedup
(three near-identical "selected option" parsers — the module doc's drop-in
provider promise currently means a third copy-paste); **`MonitorState`
extraction — do before MEAS** (`on_output` is an 11-argument function whose
classifier inputs are ephemeral locals; the extraction returns the
`Classification` MEAS observes); write-side `feed_and_broadcast` dedup
(native vs sidecar forked copies of the emulator-feed/ring/broadcast loop);
`attach_or_resync` consolidation (the Ok/Gap/Conn/Code dance hand-rolled at
four subtly different sites — REC/M4-C would clone a fifth). asmux/relay
consistency: answer malformed RPC bodies with `Error` instead of 10 silent
drops; narrow `Registry::create`'s lock (held across `openpty`/fork —
spawning stalls keystrokes to live sessions); `WATCHDOG_IDLE_MS`
implement-or-delete; `Superseded.last_cursor` semantics; the "TEMPORARY
diagnostic" test label. Client/scripts: `theme.ts`
(palette tri-defined: CSS vars + ~40 raw hexes + three TS color maps) +
dead-CSS sweep (structural CSS split gated on DEC-1); `TerminalHeader`
extraction (the header/UsageModal block is pasted into both shells);
**Terminal.tsx split — before the next terminal feature or xterm bump**
(trigger fired: the file is now 1,087 lines and its main effect spans roughly
198–1001, mixing five-plus subsystems and three private `_core` monkeypatches
that silently no-op if an xterm upgrade moves the internals — wrap in one typed
shim with a dev-time assertion);
`createTopology()` e2e helper (the three multi-node tests hand-roll ~120
lines of process lifecycle each — the exact code class the holder-theft
incident hardened). Detail: refactoring-plan.md → §6.5.

### RF-VT100 — terminal emulator dependency review (P3, trigger-based)

`vt100` 0.15 is unmaintained; the workspace disables its overflow-checks
(root `Cargo.toml` comment) and `repaint_with_history` carries a
deep-scrollback invariant. Evaluate `termwiz`/`alacritty_terminal` behind the
existing snapshot interface. Trigger: M4 cold-stitch work stressing the
snapshot path, or an upstream CVE. **Concrete defect on record (2026-07-07):**
`vt100` drops lines scrolled off the top of a sub-screen bottom-margin
`DECSTBM` region, unlike real terminals / xterm.js — this was the codex-scrollback
bug, now worked around by raw-history replay in **TERM-SCROLL**
([`terminal-scrollback.md`](terminal-scrollback.md); the `repaint_with_history`
history branch is now dead in production, exercised only by tests). A replacement
with correct region-scrollback would let TERM-SCROLL's normal-buffer branch fold
back to a compact rendered repaint. Detail: refactoring-plan.md → RF-VT100.

### DOC-1 — relay-framing doc sync (done 2026-07-24)

`architecture.md` and `asm-relay/src/protocol.rs` now describe R1's shipped
**dial-out-per-stream** design. The obsolete yamux-default language is gone.

### MEAS — classifier measurement / shadow classification (P2, dev-only)

A general shadow-classification harness: any classification heuristic in
the project registers a `TaskSpec` (labels, prompt, projection, replay) and
calls `observe()`; a local 1–2 B LLM (Ollama / llama-server on the dev
host, never a cloud API) re-classifies the same snapshotted inputs in
**shadow**. Agreement is recorded; disagreements persist a replayable
snapshot + both labels + the LLM's reasoning into `measure_samples`. Triage
loop turns `heuristic_gap` rows into regression-test fixtures and pattern
fixes, and `llm_wrong` rows into banked training data for a future
distilled classifier. Attention is the first registered task;
`exit_outcome` and remote (client/asmux) submitters follow in MEAS-3.
Default-off (`ASM_MEASURE=1`), zero effect on live state, rate-capped for
CPU inference budgets. Full design incl. schema, sampling policy, adoption
recipe, and the two-pass label/reasoning protocol:
[`classifier-measurement.md`](classifier-measurement.md). Milestones
MEAS-1..3 (task-agnostic core + attention → reasoning + triage/export →
second task + remote observe). 2026-07-12: do RF-HYG's `MonitorState`
extraction first — today `on_output`'s classifier inputs are ephemeral
`&mut` locals with no observable seam; the extraction returns the
`Classification` that `observe()` hooks.

### I18N-2 — additional locales (P4)

Deferred by explicit user choice. Infrastructure is ready (`check-locales.mjs`
parity gate, typed keys); adding a locale is the 3-step recipe in `i18n.md`.

## Suggested order (cross-track)

Completed path, retained as sequencing context: ~~**MOB phases 1–3**~~ ✅
2026-07-06; ~~**R4 gateway mode**~~ ✅ 2026-07-07; ~~**RF-M4 + M4 Stage
A/B**~~ ✅ 2026-07-07 → 07-11; ~~**TERM-SCROLL**~~ ✅ 2026-07-08; ~~**REC
via fork**~~ ✅ 2026-07-14.

1. ~~**FIX**~~ (landed 2026-07-25) → ~~the minimal **RF-GATE** harness~~
   (landed 2026-07-26) → **RF-FLOW** → **RF-LIFE**. RF-FLOW now makes
   cold-history/input promises explicit under overload and disk failure;
   RF-LIFE owns transitions, atomicity, and compensation. Recovery is already
   shipped and is not the end of this chain. The P2 RF-GATE remainder can land
   alongside the work it guards.
2. **SEC-2 + RF-ERR** together — before exposure beyond trusted networks.
   The daemon-managed UI and UI-only gateway make the allowed-roots gap more
   consequential. SEC-1(direct) is decided: LAN direct is plaintext by design;
   relay-side encryption rides with R5.
3. **DEC-1** → **RF-QUERY** + **RF-WSPROTO** → **MVP-RICH**. The shipped client
   has committed to plain React/Vite in practice without resolving the planned
   shadcn/Tailwind/Dockview/Electron baseline. Decide that first. RF-WSPROTO
   must precede whichever of MOB-PUSH / MVP-RICH / V3 adds the first new
   server→client frame.
4. **RF-REC** (legacy id) is now generic consolidation, not a feature gate.
   Fold the immediate pull-prompt fix into FIX, then take the bounded
   `GitRunner`, typed API helpers, and overdue `RightPanel` split alongside
   RF-QUERY/RF-ERR or the next work touching those files.
5. **RF-OPS** can run after the minimal gate or alongside feature work. Land
   truthful readiness before deployment/packaging and bounded child/client
   requests before adding more polling or agent-driven operations.
6. **COPILOT-REVIEW** after its `GitTreeSnapshot` + bounded child-runner slices.
   Ship manual advisory review first. Add automatic idle review only after
   fingerprint deduplication/budgets; add **COPILOT-GATE** only after RF-ERR and
   the relevant RF-LIFE SCM unit-of-work, beginning with exact-commit push.
7. **MEAS-1** after RF-HYG's `MonitorState` extraction. It is dev-only and can
   accrue classifier-disagreement data while later feature work proceeds;
   MEAS-2/3 remain opportunistic.
8. **V0** spike → V1–V3 as a block when browser editing becomes a product goal.
9. **MOB-PWA** (only the iOS metas remain), then **MOB-PUSH** after
   RF-WSPROTO and a daemon push-transport design.
10. Take **R5** by deployment need: TLS/443 + ops first, ACL/key rotation next,
   pairing and the splice-point confidentiality decision with the V-track.
11. **M4-C**, **MVP-HOOKS**, the remaining **MVP-CKPT** product surface,
    **FORK-SHELL**, **FORK-OC**, and
    smaller **RF-HYG** items opportunistically. The RF-HYG Terminal split trigger
    has already fired; do it before another substantial terminal feature or
    xterm upgrade.
12. **M5** → **MVP-PKG** when formal cross-platform/packaging gates become the
    goal. Additional locales (**I18N-2**) stay explicitly deferred.

Constraints to respect when reordering: nothing in R-track may assume M4
features exist; V1's relay change must not add daemon-API parsing to the
relay; all new client strings go through `en.json` (verify `client/src/i18n/`
exists on the working branch before starting client work).
