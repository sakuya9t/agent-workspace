# MVP Execution Plan

## Purpose

This document turns the architecture and requirements into an execution plan for the first shippable MVP.

The MVP proves one product loop:

```text
start agent -> disconnect -> agent keeps working -> reconnect -> resume terminal -> inspect changed files
```

## MVP Definition

### Included

- Native daemon builds for Linux, macOS, and Windows.
- Windows daemon uses native APIs and ConPTY.
- Direct local/LAN client-to-server connection.
- SSH local port-forward remote connection.
- Basic server/device authentication.
- Persistent sessions through a pluggable session backend interface.
- Built-in native PTY per-session sidecar backend.
- Local daemon-to-backend IPC.
- Backend event drain or backend-local output spool.
- Terminal event replay.
- Server-side terminal emulator snapshots.
- Structural session summary records.
- Electron desktop client with shared web UI.
- React 19 + TypeScript frontend stack.
- Session list, attach, detach, resize, stop, and archive.
- Agent plugin registry.
- Built-in agent plugins for Codex and Claude Code.
- Custom command plugin.
- Workspace registration and allowlist.
- Git-backed workspace model.
- Guided `git init` for plain folders.
- Per-session Git worktree isolation.
- Workspace setup hooks.
- Source-control plugin interface.
- Built-in Git plugin with status, branch, commit graph, changed files, and diffs.
- Click changed file to view diff.
- Session activity and needs-attention signals.
- Early "Continue in VS Code" action.
- Electron hardening and terminal escape-sequence policy.
- Local logs, health endpoint, and diagnostics export.

### Not Included

- Native mobile apps.
- Production relay service.
- Production gateway route manager.
- Production tmux backend.
- Automatic personal pool placement.
- Repository sync orchestration across machines.
- Browser-based full editor replacement.
- User-facing Git write workflows.
- Team/session sharing.
- Advanced memory UI.
- Production-grade secret redaction.

## Product-Owned Boundaries

The MVP keeps these semantics under product control:

- session identity and lifecycle,
- reconnect and terminal replay,
- terminal event log and snapshots,
- server-side terminal emulator state,
- session backend contract,
- workspace instance isolation,
- agent and source-control plugin contracts,
- Git-backed checkpoints,
- rich-output sanitization,
- terminal escape-sequence policy.

Dependencies are wrapped behind internal interfaces.

## Baseline Technology Decisions

### Daemon

| Area | Decision |
| --- | --- |
| Language | Rust |
| Runtime | tokio |
| Local database | SQLite with WAL and batched writer |
| SQLite access | `rusqlite` behind daemon-owned storage workers |
| Native PTY | Out-of-process per-session sidecars owning Unix PTYs and ConPTY handles |
| Terminal state | Headless VT emulator snapshots in each session sidecar |
| API | HTTP control API + WebSocket terminal input/output stream |
| Local IPC | Unix domain sockets, Windows AF_UNIX where reliable, Windows named pipes fallback |
| Git | `git` CLI behind Git plugin interface |

### Frontend

| Area | Decision |
| --- | --- |
| App framework | React 19 + TypeScript + Vite |
| Desktop shell | Electron |
| Terminal | xterm.js |
| Local UI state | Zustand |
| Server state | TanStack Query |
| Components | shadcn/ui + Tailwind |
| Layout | Dockview |
| Code and diff view | CodeMirror |
| Markdown | Marked |
| Syntax highlighting | Shiki |
| Diagrams | Mermaid |
| Math | KaTeX |
| Sanitization | DOMPurify |
| Icons | lucide-react |

## Workstreams

### 1. Platform And Daemon Foundation

Goal: create a small native daemon that starts reliably on Linux, macOS, and Windows.

Deliverables:

- Rust workspace scaffold.
- Daemon binary.
- Platform abstraction layer.
- Config and data directory resolution.
- Structured logging.
- Health endpoint.
- HTTP/WebSocket API skeleton.
- SQLite connection and migrations.
- User-scoped service install approach for systemd user units, launchd LaunchAgents, and Windows per-user startup.
- Windows user-mode restart-on-failure recovery.

Key interface:

```text
Platform
  get_data_dir()
  get_config_dir()
  spawn_process()
  spawn_pty()
  kill_process_tree()
  watch_files()
  install_user_service()
  open_local_ipc()
```

Acceptance criteria:

- Daemon starts on Linux, macOS, and Windows.
- `/health` returns version, platform, uptime, and database status.
- SQLite migrations run on startup.
- Windows paths with drive letters and spaces work.
- No Windows path depends on WSL.
- Installed daemon can launch Codex and Claude Code authentication flows in the enrolled user's context.
- Installed daemon restarts after a simulated crash on each supported OS.

### 2. Session Backend Engine

Goal: durable sessions survive client disconnect and use a replaceable backend contract.

Deliverables:

- Session create/list/get/stop/archive APIs.
- Session backend process manifest schema.
- Session backend registry.
- Daemon-to-backend IPC.
- Built-in native PTY per-session sidecar backend.
- Sidecar process lifetime model.
- Per-user sidecar runtime directory.
- Sidecar socket naming by session ID.
- Daemon startup sidecar scan.
- Orphaned sidecar detection.
- Orphaned sidecar adopt and terminate actions.
- Sidecar-owned PTY and ConPTY handles.
- Headless VT emulator in the sidecar.
- Emulator snapshot export.
- Backend session handle mapping.
- Backend health API.
- Backend event cursor tracking.
- Backend event drain or append-only output spool.
- Backpressure and gap marker policy.
- Terminal input over WebSocket.
- Terminal output over WebSocket.
- Attach/detach flow.
- Session state machine.
- Process exit tracking.
- Terminal event sequence numbers.
- Append-only terminal event persistence.
- Replay after snapshot sequence.
- Emulator snapshot creation and restore.
- Structural session summary record on exit and segment boundary.

Backend IPC operations:

```text
RegisterBackend
CreateBackendSession
AttachBackendSession
QueryBackendSession
StreamBackendOutput
DrainBackendEvents
SendBackendInput
ResizeBackendSession
StopBackendSession
ExportBackendSnapshot
DetachDaemon
```

Acceptance criteria:

- Start a shell-backed session through the native backend.
- Attach and see live output.
- Detach while the process keeps running.
- Fresh client attach receives a current sidecar emulator snapshot as an ANSI repaint stream.
- Reconnect receives snapshot plus newer events.
- Resize propagates to the backend PTY.
- Stop preserves exit status and terminal output.
- Client restart does not affect server sessions.
- Daemon restart reattaches to a live backend session.
- Daemon startup detects orphaned sidecars.
- Agent or backend exit records `exited` or `failed` without relaunch.
- Agent exit writes a structural session summary record.
- A mock backend can replace the native backend in tests.
- Mid-session attach to a full-screen TUI renders a coherent current screen.
- A terminal parser panic or sidecar crash affects only the owned session.

### 3. Desktop/Web Client Shell

Goal: build the first usable control center.

Deliverables:

- Electron app shell.
- Vite React 19 + TypeScript app.
- shadcn/ui and Tailwind setup.
- Dockview layout shell.
- Zustand stores for UI state.
- TanStack Query client for daemon state.
- Connection setup for local/LAN daemon.
- Device/server enrollment UI.
- Three-panel layout:
  - left: sessions, workspaces, agent profiles,
  - middle: xterm.js terminal,
  - right: changed files and source-control panel.
- Session list with status, agent plugin, workspace, branch, and last activity.
- Attach/detach flow.
- Terminal input/output through xterm.js.
- Terminal resize handling.
- Basic error, loading, and reconnect states.

Acceptance criteria:

- User connects to a daemon.
- User creates a session.
- User attaches to an existing session.
- User closes and reopens the app, then resumes the session.
- Session status updates without manual refresh.
- Terminal sizing works at common desktop sizes.
- Dockview layout persists locally.

### 4. Rich Output And Code Viewing

Goal: render agent output, files, and diffs without compromising terminal responsiveness or security.

Deliverables:

- CodeMirror read-only file viewer.
- CodeMirror diff viewer or focused diff component.
- Repo markdown file preview.
- Agent transcript preview through agent plugin transcript parsers.
- Session summary record renderer.
- Marked markdown parser.
- DOMPurify sanitization boundary.
- Shiki syntax highlighting with lazy-loaded themes/languages.
- Mermaid rendering with strict security defaults.
- KaTeX math rendering.
- Markdown render worker or deferred render path for large content.

Acceptance criteria:

- Repo markdown files render safely.
- Agent transcript content renders safely when a plugin exposes a transcript parser.
- Session summary records render safely.
- Markdown output renders safely.
- Raw HTML from untrusted markdown is sanitized.
- Shiki highlighting loads on demand.
- Mermaid diagrams render without enabling unsafe HTML by default.
- KaTeX renders math blocks and inline math.
- Large markdown output does not freeze terminal input.
- File and diff views render with line numbers and copy/select behavior.

### 5. Plugin Foundation

Goal: agent, session backend, and source-control behavior are extensible from the beginning.

Deliverables:

- Rust trait interfaces for built-in plugin kinds.
- Static plugin registry.
- Built-in plugin loading.
- Session backend plugin interface.
- Agent plugin interface.
- Source-control plugin interface.
- Platform capability checks.
- Typed plugin configuration validation.
- Custom command plugin.
- Codex plugin.
- Claude Code plugin.

Agent plugin capabilities:

```text
id
display_name
supported_platforms
detect_binary
build_launch_command
default_env
memory_injection_support
session_boundary_detection
transcript_location
transcript_parser
```

Session backend plugin capabilities:

```text
id
display_name
supported_platforms
ipc_transport
create_session
attach_session
stream_output
send_input
resize_session
stop_session
drain_events
export_snapshot
```

Source-control plugin capabilities:

```text
id
detect_repository
get_status
get_history_graph
get_changed_files
get_diff
create_workspace_instance
cleanup_workspace_instance
create_checkpoint
update_checkpoint
```

Acceptance criteria:

- Built-in plugins are listed by API.
- Codex and Claude Code plugins detect missing binaries.
- Custom command launch requires approval.
- Plugin launch behavior can vary by OS.
- Native backend is registered through the plugin registry.
- Adding another agent plugin does not change session engine code.
- Dynamic third-party plugin loading is not part of MVP.

### 6. Workspace Registration And Isolation

Goal: independent agent sessions never collide in one writable working tree.

Deliverables:

- Workspace registration API.
- Workspace allowlist.
- Workspace inspection.
- Git repository detection.
- Guided `git init` flow for plain folders.
- Workspace instance table.
- Workspace lease table.
- Per-session Git worktree creation.
- Workspace setup hook model.
- Copy rules for local files such as `.env`.
- Symlink rules for dependency caches.
- Bootstrap command execution.
- Worktree cleanup flow.
- Direct-source-working-tree override.
- Session metadata linked to `workspace_instance_id`.

Acceptance criteria:

- Register an existing Git repo.
- Start first agent session in an isolated worktree.
- Start second agent session for the same repo in a different worktree.
- Two sessions can edit the same file without overwriting each other.
- UI shows each session's workspace instance.
- Setup hook status is visible in the UI.
- Direct source checkout mode requires explicit override.
- Dirty worktree cleanup is blocked without confirmation.
- Submodule, LFS, custom hook path, and required ignored-file issues are detected or surfaced.

### 7. Git Plugin And Change Tracking

Goal: changed files and diffs are visible for every MVP workspace that uses Git.

Deliverables:

- Git source-control plugin.
- Repository status API.
- Current branch and HEAD API.
- Remotes API.
- Changed files API.
- Staged and unstaged distinction.
- File diff API.
- Commit graph API.
- App-managed checkpoint refs or objects.
- Manual checkpoint update.
- Explicit "New segment" checkpoint update.
- Optional plugin boundary detection hook.
- Right-panel changed-file UI.
- Click-to-diff behavior.

Acceptance criteria:

- Git repo shows branch, status, changed files, and diffs.
- Plain folder can be initialized as a local Git repo.
- App initialization does not require a remote.
- Checkpoint refs do not appear as normal user-facing commits.
- Clicking a changed file opens a readable diff.
- Created, modified, deleted, and renamed files are represented.
- Basic commit graph renders for a typical repo.

### 8. Basic Auth And Security

Goal: keep personal servers private.

Deliverables:

- Server enrollment token.
- Device enrollment.
- Local credential storage in client.
- Scoped session token.
- Workspace allowlist enforcement.
- Custom command approval.
- Local lifecycle event log.
- TLS/mTLS or equivalent plan for non-local connections.
- Markdown and Mermaid security defaults.
- Electron hardening checklist.
- Terminal escape-sequence policy.

Acceptance criteria:

- Unknown clients cannot attach to sessions.
- Enrolled devices reconnect without re-enrollment each launch.
- Revoked devices cannot attach.
- Session creation fails outside allowlisted workspace roots.
- Custom command launch requires visible approval.
- Untrusted markdown is sanitized.
- Electron renderer has no Node integration.
- OSC 52 clipboard writes are disabled by default.

### 9. Attention Signals

Goal: surface sessions that need user action.

Deliverables:

- Attention state field on session records.
- New-output-after-idle detection.
- Bell-character detection.
- Plugin-provided prompt or approval pattern hook.
- Agent exit/failure attention event.
- Session list badges and filters.

Acceptance criteria:

- Sessions with new activity are visible in the list.
- Sessions likely waiting for user approval are highlighted.
- Failed sessions are highlighted.
- Attention state clears when the user views or acknowledges the session.

### 10. VS Code Continuation

Goal: user can move from control center to editor workflow.

Deliverables:

- "Continue in VS Code" action.
- Open VS Code at the session's isolated workspace instance.
- Pass server/session context through deep link, local file, or extension command.
- Minimal VS Code extension or documented deep-link fallback.
- Local daemon continuation path.
- Remote daemon continuation path using VS Code Remote-SSH prerequisites.

Acceptance criteria:

- Button opens VS Code in the correct workspace instance.
- Opening VS Code does not create a new agent session.
- Session remains visible and attachable in the control center.
- Original source workspace is not opened when an isolated instance exists.
- Remote continuation displays Remote-SSH setup requirements when unavailable.

### 11. Packaging And Install

Goal: user can install and run the MVP on personal machines.

Deliverables:

- Daemon packages for Linux, macOS, and Windows.
- Electron packages for Linux, macOS, and Windows.
- First-run setup flow.
- Local/LAN connection instructions.
- User-scoped service installation scripts.
- Basic upgrade path.
- Diagnostics export.
- Quickstart docs.

Acceptance criteria:

- Fresh install enrolls a local server and launches a test shell session.
- Linux user-scoped background service works.
- macOS LaunchAgent works.
- Windows per-user startup works.
- Windows daemon crash recovery restarts the daemon without killing live sidecars.
- User can find and export logs from the UI.

## Suggested Sequence

Planning baseline for a solo developer is 8-12 months. The first three phases carry the highest technical risk because they establish the cross-platform session engine, terminal snapshot model, and installed service behavior.

### Phase 0: Component Evaluation

Duration: 2-3 weeks.

Critical path tasks:

- Evaluate local IPC: Unix domain sockets, Windows AF_UNIX, Windows named pipes.
- Evaluate backend process lifetime under user-scoped systemd, launchd, and Windows startup mechanisms.
- Evaluate sidecar-owned PTY survival across daemon restart.
- Evaluate PTY libraries for Unix PTY and Windows ConPTY.
- Evaluate headless terminal emulator crates with alternate-screen TUIs.
- Spike fresh-client mid-session attach against Codex or a representative full-screen TUI.
- Spike terminal parser fuzzing with hostile escape-sequence input.
- Smoke-test Codex and Claude Code login/auth flows from installed daemon context.
- Evaluate service installation helpers, including Windows user-mode restart-on-failure recovery.

Backend-contract validation:

- Timebox tmux control-mode or capture spike to one day.

Phase-local validation tasks:

- Evaluate xterm.js attach, resize, fit, search, and WebGL addons.
- Evaluate Dockview layout persistence.
- Evaluate CodeMirror diff approach.
- Evaluate markdown pipeline: Marked, DOMPurify, Shiki, Mermaid, KaTeX.
- Evaluate terminal escape-sequence policy in xterm.js.
- Validate SQLite WAL and batched `rusqlite` writer behavior.
- Validate `git` CLI behavior behind the Git plugin interface.
- Evaluate file watching library.

Exit criteria:

- Critical path decision records are complete.
- Dependency decision record for each major component.
- Decision record for session backend IPC and process lifetime.
- Decision record for terminal emulator snapshot strategy.
- Prototype proves PTY output on Linux, macOS, and Windows.
- Prototype proves daemon-to-backend communication.
- Prototype proves fresh-client TUI attach from emulator snapshot.
- Prototype proves xterm.js attach to daemon stream.
- Prototype proves Dockview + xterm.js sizing.
- Prototype proves sanitized markdown rendering.

### Phase 1: Session Engine Prototype

Duration: 4-6 weeks.

Tasks:

- Build daemon skeleton.
- Implement local API.
- Implement session backend registry.
- Implement backend IPC client/server.
- Implement native PTY per-session sidecar backend.
- Implement sidecar-owned PTY lifetime.
- Implement per-user sidecar runtime directory and socket naming.
- Implement headless terminal emulator snapshot export as ANSI repaint stream.
- Implement session create/list/attach/resize/stop.
- Implement terminal stream WebSocket.
- Build minimal React page with xterm.js attach.

Exit criteria:

- Start a shell session.
- Disconnect browser/client.
- Session keeps running.
- Reconnect and continue interacting.
- Mock backend replaces native backend in tests.
- Fresh client attach restores a coherent full-screen TUI.

### Phase 2: Durable Resume

Duration: 4-6 weeks.

Tasks:

- Add SQLite migrations.
- Add sessions table.
- Add backend session table.
- Add terminal event log table.
- Add backend event cursor/spool storage.
- Add terminal emulator snapshot table.
- Add sequence-number replay.
- Add backend event drain.
- Add daemon startup sidecar scan.
- Add orphaned sidecar records.
- Add emulator snapshot restore.
- Add backpressure and gap marker behavior.
- Add archive and exited session states.
- Add daemon restart reattach.
- Add explicit follow-up session creation.
- Add structural session summary record writer.

Exit criteria:

- Reconnect receives missed output.
- Daemon restart preserves session history and exited state.
- Daemon restart does not silently relaunch sessions.
- Orphaned sidecars are surfaced for adopt or terminate.
- Snapshot restore works when replay window is unavailable.
- Writer stall and disk-full behavior produces explicit health events or gap markers.
- Session exit writes a structural summary record.

### Phase 3: Control Center Client

Duration: 3-4 weeks.

Tasks:

- Build Electron app shell.
- Add React 19 + TypeScript + Vite.
- Add shadcn/ui, Tailwind, and lucide-react.
- Add Dockview shell.
- Add Zustand stores.
- Add TanStack Query client.
- Add device/server enrollment flow.
- Add SSH tunnel connection profile flow.
- Add session list.
- Add activity and attention badges.
- Add xterm.js center panel.
- Add placeholder source-control panel.
- Add reconnect and error states.

Exit criteria:

- User creates and resumes sessions from the desktop app.
- User connects through a documented SSH local port-forward profile.
- UI supports at least two simultaneous sessions.
- Sessions with new activity are visible.
- Terminal sizing works reliably in Dockview.
- Layout persists across app restart.

### Phase 4: Rich Output And Source Viewing

Duration: 2 weeks.

Tasks:

- Add CodeMirror file viewer.
- Add diff viewer.
- Add repo markdown preview.
- Add agent transcript preview through plugin-provided transcript locators and parsers.
- Add session summary record renderer.
- Add Marked parser.
- Add DOMPurify sanitization.
- Add Shiki highlighting.
- Add Mermaid renderer.
- Add KaTeX renderer.
- Lazy-load rich-output dependencies.

Exit criteria:

- Markdown output renders safely.
- Repo markdown files render safely.
- Agent transcript files render safely when exposed by an agent plugin.
- Code blocks are highlighted.
- Diagrams and math render on demand.
- Terminal responsiveness remains intact during large markdown renders.
- Diff viewer is usable for changed files.

### Phase 5: Plugins And Agent Launch

Duration: 2-3 weeks.

Tasks:

- Add Rust trait interfaces for plugin kinds.
- Add static plugin registry.
- Add native session backend plugin registration.
- Add custom command plugin.
- Add Codex plugin.
- Add Claude Code plugin.
- Add binary detection and launch validation.
- Add plugin-specific env/config hooks.

Exit criteria:

- User launches Codex when installed.
- User launches Claude Code when installed.
- User launches a custom command after approval.
- Missing binary errors are clear.
- Dynamic third-party plugin package loading remains out of MVP.

### Phase 6: Workspace Isolation

Duration: 3-4 weeks.

Tasks:

- Add workspace registration and allowlist.
- Add Git detection.
- Add `git init` flow for plain folders.
- Add workspace instance and lease tables.
- Add managed Git worktree creation.
- Add workspace setup hook model.
- Add copy rules for `.env` and similar local files.
- Add symlink rules for dependency caches.
- Add bootstrap command execution.
- Link sessions to workspace instances.
- Add cleanup guard for dirty instances.
- Add UI labels for source workspace and active instance.

Exit criteria:

- Two agents on the same repo run in separate worktrees.
- Workspace setup hooks run and report status.
- Dirty worktree cleanup is blocked without confirmation.
- Direct source checkout mode requires explicit override.
- Submodule, LFS, and required ignored-file issues are surfaced.

### Phase 7: Git Panel And Diffs

Duration: 3-4 weeks.

Tasks:

- Add Git status API.
- Add changed files API.
- Add file diff API.
- Add commit graph API.
- Add checkpoint refs/objects.
- Add explicit "New segment" checkpoint action.
- Add optional plugin boundary detection hook.
- Add right-panel changed-files UI.
- Wire changed files to diff viewer.
- Add commit graph view.

Exit criteria:

- User clicks any changed file and sees the diff.
- Created, modified, deleted, and renamed files display correctly.
- Plain folder initialized by the app gets the same diff workflow.
- Commit graph is usable on a medium-sized repo.
- New segment advances the active checkpoint.

### Phase 8: Attention Signals

Duration: 1-2 weeks.

Tasks:

- Add attention state to session records.
- Add new-output-after-idle detection.
- Add terminal bell detection.
- Add daemon-side recent-output text matcher for plugin prompt patterns.
- Add approval-needed and failed-state badges.
- Add attention filters to the session list.

Exit criteria:

- New activity appears in the session list.
- Likely blocked sessions are highlighted.
- Plugin prompt patterns match recent output text without direct sidecar screen access.
- Failed sessions are highlighted.
- Viewing or acknowledging a session clears attention state.

### Phase 9: VS Code Continuation And Packaging

Duration: 3-4 weeks.

Tasks:

- Add VS Code continuation action.
- Open isolated workspace instance.
- Add local-daemon deep-link or minimal extension support.
- Add Remote-SSH prerequisite messaging for remote daemon workspaces.
- Package daemon and desktop app.
- Add user-scoped service installation scripts.
- Add diagnostics export.
- Write install and quickstart docs.

Exit criteria:

- Fresh install launches daemon and desktop client.
- User starts an agent, inspects diffs, opens VS Code, disconnects, and reconnects.
- Local daemon continuation opens the isolated workspace instance.
- Remote daemon continuation shows Remote-SSH requirements when unavailable.
- MVP works on Linux, macOS, and Windows at smoke-test level.

## Verification Plan

### Automated Tests

- Session state transitions.
- Session backend process manifest parsing.
- Backend handle/session mapping.
- Sidecar runtime directory scan.
- Orphaned sidecar detection.
- Static plugin registry behavior.
- Workspace lease rules.
- Git worktree path generation.
- Checkpoint ref naming.
- Workspace setup hook rules.
- Frontend store selectors and layout persistence.
- Markdown sanitization.
- Terminal escape-sequence policy.
- Terminal parser fuzz cases.
- Terminal attach and resize.
- Terminal emulator snapshot restore.
- Daemon-to-backend IPC.
- Backend event drain after daemon reconnect.
- Replay from event sequence.
- Backpressure and gap marker behavior.
- Git status, diff, and checkpoint APIs.
- Attention state transitions.
- Structural session summary record creation.
- Auth, workspace allowlist, and session lifecycle APIs.

### Manual Smoke Tests

- Launch shell session on Linux.
- Launch shell session on macOS.
- Launch shell session on Windows native ConPTY.
- Launch Codex plugin.
- Launch Claude Code plugin.
- Complete Codex and Claude Code authentication flows from installed daemon context.
- Disconnect desktop client during active output.
- Reconnect and confirm missed output appears.
- Attach a fresh client mid-session to a full-screen TUI and confirm coherent screen state.
- Restart daemon while backend session remains alive.
- Restart daemon on Windows and confirm sidecar reattach.
- Delete a session row, restart daemon, and confirm orphaned sidecar detection.
- Adopt or terminate an orphaned sidecar from the UI.
- Crash the installed Windows daemon and confirm user-mode recovery without killing live sidecars.
- Kill an agent process and confirm no automatic relaunch.
- Connect through an SSH local port-forward.
- Attach two clients to one session and verify shared input plus most-recently-active resize policy.
- Confirm attach and terminal input update the active client for resize policy.
- Start two sessions on the same repo and edit the same file.
- Confirm separate worktrees and separate diffs.
- Run workspace setup hooks for a repo with required local files.
- Initialize plain folder as Git and show changed-file diff.
- Trigger an attention signal for a blocked or approval-needed session.
- Confirm plugin attention patterns match recent output text without sidecar screen access.
- Confirm session exit writes a structural summary record.
- Render markdown with code, Mermaid, and KaTeX.
- Verify OSC 52 clipboard writes are disabled.
- Open session workspace in VS Code.

### Performance Checks

- Idle daemon CPU usage.
- Memory usage with 1, 5, and 20 sessions.
- Terminal output throughput.
- 24-hour session soak with high event count.
- Backpressure behavior during slow storage writes.
- Reconnect time with large scrollback.
- Git graph generation time on small, medium, and large repos.
- Disk usage from terminal logs and retained worktrees.
- Frontend responsiveness during terminal output and rich markdown rendering.

## Release Gates

### Alpha Gate

- Linux/macOS/Windows daemon starts.
- Basic shell sessions work.
- Client attach/detach works.
- Terminal emulator snapshot attach works.
- Tail replay after snapshot works.
- Minimal React/xterm.js client works.

### Beta Gate

- Codex and Claude plugins work.
- Workspace registration works.
- Git worktree isolation works.
- Changed-file diffs work.
- Basic auth works.
- SSH tunnel connection works.
- Attention signals work.
- Rich markdown rendering is sanitized.
- VS Code continuation works.

### MVP Gate

- End-to-end workflow works on Linux, macOS, and Windows.
- Two concurrent agents on the same repo are isolated.
- Reconnect restores live session output.
- Fresh-client attach restores full-screen TUI state.
- Orphaned sidecar recovery works.
- Structural session summaries are written and rendered.
- Plain folders can be initialized as Git for diff tracking.
- Workspace setup hooks work on a real project.
- Frontend stack is packaged in Electron.
- Packaging and quickstart docs are complete.
- Known limitations are documented.

## Key Risks And Mitigations

| Risk | Mitigation |
| --- | --- |
| Windows ConPTY behavior is inconsistent | Spike first, wrap backend, keep Windows smoke tests |
| Sidecar process lifetime differs by OS/service manager | Use per-session sidecars and test restart behavior per OS |
| Sidecars outlive daemon metadata | Scan the sidecar runtime directory and surface orphaned sidecars |
| Local IPC differs across platforms | Wrap IPC transport and test UDS, AF_UNIX, and named pipes |
| Terminal emulator fidelity is hard | Spike full-screen TUI attach in Phase 0, fuzz parser input, and keep emulator snapshots as the primary resume path |
| Event storage stalls or disk fills | Apply backpressure, emit health events, and mark output gaps explicitly |
| Dockview and xterm.js sizing is fragile | Prototype layout and resize behavior in Phase 0 |
| Markdown/diagram rendering creates XSS risk | Sanitize HTML and keep Mermaid strict by default |
| Terminal escape sequences create client-side risk | Disable OSC 52 by default and gate links/title behavior |
| Rich rendering hurts terminal responsiveness | Lazy-load and defer markdown rendering |
| Git worktrees fail for edge-case repos | Detect clearly, run setup hooks, and use explicit isolation fallback |
| Plugin API over-design | Ship static built-in traits first |
| Commit graph is slow | Cache results and defer graph polish |
| Service installation is painful | Provide foreground dev mode, user-scoped background mode, and crash recovery |
| Electron packaging takes longer than expected | Keep web client runnable independently |

## MVP Cut Rules

Keep:

- durable backend-managed sessions,
- per-session sidecar-owned PTY model,
- sidecar orphan detection,
- server-side terminal emulator snapshots,
- structural session summary records,
- session backend contract,
- reconnect and tail replay,
- native Windows daemon support,
- basic Electron client,
- xterm.js terminal panel,
- workspace isolation,
- workspace setup hooks,
- changed-file diffs,
- attention signals,
- safe markdown rendering.

Cut under schedule pressure:

- commit graph polish,
- user-scoped service install polish,
- VS Code extension beyond open action,
- advanced terminal search,
- production relay/gateway,
- personal pool placement,
- memory UI,
- user-facing Git write workflows,
- advanced markdown rendering features.
