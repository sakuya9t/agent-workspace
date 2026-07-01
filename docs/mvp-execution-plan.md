# MVP Execution Plan

## Purpose

This document turns the architecture and requirements into an execution plan for the first shippable MVP of the personal agent session manager.

The MVP should prove the core product promise:

> A user can start an agent in a remote workspace, disconnect, reconnect later, resume the same live session with no lost terminal output, inspect code changes, and keep concurrent agents isolated from each other.

## MVP Definition

### Included

- Native daemon builds for Linux, macOS, and Windows.
- Windows daemon uses native Windows APIs and ConPTY, not WSL.
- Direct local/LAN client-to-server connection.
- Basic server/device authentication.
- Persistent PTY-backed sessions.
- Terminal event replay and basic snapshot restore.
- Electron desktop client with shared web UI.
- Session list, attach/detach, resize, stop, and archive.
- Agent plugin registry.
- Built-in agent plugins for Codex and Claude Code.
- Custom command plugin.
- Workspace registration and allowlist.
- Git-backed workspace model.
- Guided `git init` for plain folders.
- Per-session Git worktree isolation for concurrent agents on the same repo.
- Source-control plugin interface.
- Built-in Git plugin with status, branch, commit graph, changed files, and diffs.
- Click changed file to view diff.
- Early "Continue in VS Code" action that opens the isolated workspace instance.
- Local logs, health endpoint, and basic diagnostics.

### Not Included

- Native mobile apps.
- Production relay/gateway service.
- Automatic personal pool placement.
- Repository sync orchestration across multiple machines.
- Browser-based full editor replacement.
- User-facing Git write workflows such as commit, checkout, rebase, merge, and stash.
- Team/session sharing.
- Advanced memory UI.
- Production-grade secret redaction.

## Delivery Strategy

Build the MVP around four product-owned boundaries:

- Session lifecycle and reconnect semantics.
- Terminal event log and snapshot state.
- Workspace instance isolation.
- Plugin contracts for agents and source control.

Use proven dependencies wherever they fit, but keep these boundaries under product control.

Early component decisions should be made through short spikes, then wrapped behind internal interfaces so replacements remain possible.

## Workstreams

### 1. Platform And Daemon Foundation

Goal: create a small native daemon that can run on Linux, macOS, and Windows.

Deliverables:

- Rust workspace scaffold.
- Cross-platform daemon binary.
- Internal platform abstraction layer.
- Local config directory and data directory resolution.
- Structured logging.
- Health endpoint.
- Local HTTP/WebSocket API.
- SQLite database connection and migration runner.
- Basic service install strategy for systemd, launchd, and Windows Service Control Manager.

Key interfaces:

```text
Platform
  get_data_dir()
  get_config_dir()
  spawn_process()
  spawn_pty()
  kill_process_tree()
  watch_files()
  install_service()
  open_local_ipc()
```

Acceptance criteria:

- Daemon starts on Linux, macOS, and Windows.
- Daemon exposes `/health`.
- Daemon can read/write SQLite state.
- Daemon logs startup, shutdown, and errors.
- Windows path handling works with drive-letter and space-containing paths.
- No Windows path requires WSL.

Primary risks:

- PTY behavior differs across platforms.
- Windows service and ConPTY behavior may require deeper platform-specific code.
- Some dependencies may not support all targets cleanly.

### 2. PTY Session Engine

Goal: durable agent sessions that survive client disconnect.

Deliverables:

- Session create/list/get/stop/archive APIs.
- PTY spawn and resize.
- Terminal input API.
- Terminal output WebSocket stream.
- Client attach/detach model.
- Session state machine.
- Process exit tracking.
- Terminal event sequence numbers.
- Append-only terminal event persistence.
- Replay from `last_seen_event_seq`.
- Basic terminal snapshot creation and restore.

Session states:

```text
starting
running
detached
exited
failed
stopped
archived
```

Acceptance criteria:

- Start a shell-backed session from the API.
- Attach from a client and see live output.
- Detach the client while the process keeps running.
- Reconnect with a cursor and receive missed output.
- Reconnect without a cursor and receive latest snapshot plus newer events.
- Resize terminal from the client and propagate size to PTY.
- Stop session and preserve exit status/output.
- Restarting the client does not affect server sessions.

Primary risks:

- Terminal event volume can grow quickly.
- Snapshot fidelity may be imperfect at first.
- Some TUIs may use terminal features that need iterative support.

### 3. Desktop/Web Client Shell

Goal: first usable control center.

Deliverables:

- Electron app.
- Shared web app shell.
- Connection setup to local/LAN daemon.
- Basic device/server enrollment UI.
- Three-panel layout:
  - left: sessions and workspaces,
  - middle: xterm.js TUI,
  - right: changes/source-control panel.
- Session list with status, agent plugin, workspace, branch, and last activity.
- Attach/detach flow.
- Terminal input/output via xterm.js.
- Terminal resize handling.
- Reconnect behavior after app restart.
- Basic error and loading states.

Acceptance criteria:

- User can connect to a daemon.
- User can create a session.
- User can attach to an existing session.
- User can close the app, reopen it, and resume the same session.
- User sees session status changes without refreshing.
- UI remains usable at common desktop sizes.

Primary risks:

- Terminal fit/resize bugs.
- Electron packaging differences across operating systems.
- UI complexity can expand too early.

### 4. Plugin Foundation

Goal: agent and source-control behavior is extensible from the beginning.

Deliverables:

- Plugin manifest schema.
- Plugin registry table.
- Built-in plugin loading.
- Agent plugin interface.
- Source-control plugin interface.
- Platform capability checks.
- Plugin config validation.
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

- Built-in plugins can be listed by API.
- Codex and Claude Code plugins can detect missing binaries and report actionable errors.
- Custom command plugin can launch arbitrary configured commands after explicit approval.
- Plugin launch behavior can vary by OS.
- Adding another built-in agent does not require changes to session engine code.

Primary risks:

- Over-designing the plugin API before real usage.
- Agent tools may differ in CLI behavior across platforms.

### 5. Workspace Registration And Isolation

Goal: independent agent sessions never collide in one writable working tree by default.

Deliverables:

- Workspace registration API.
- Workspace allowlist.
- Workspace inspection.
- Git repository detection.
- Guided `git init` for plain folders.
- Workspace instance table.
- Workspace lease table.
- Per-session Git worktree creation.
- Worktree cleanup flow.
- Direct-source-working-tree override with explicit warning.
- Session metadata links to `workspace_instance_id`.

Default behavior:

- Registered source workspace is the source of truth.
- Independent sessions get isolated workspace instances.
- Multiple clients attached to the same session share the same workspace instance.
- Existing workspace instances are retained until user cleanup or retention policy allows deletion.

Acceptance criteria:

- Register an existing Git repo.
- Start first agent session and receive an isolated worktree.
- Start second agent session for the same repo and receive a different worktree.
- The two sessions can edit the same file without overwriting each other.
- UI shows the workspace instance for each session.
- Attempting to run directly in the source working tree requires explicit override.
- Worktree with uncommitted changes is not deleted silently.

Primary risks:

- Git worktree limitations with submodules, bare repos, nested repos, or locked worktrees.
- Disk usage from retained instances.
- Users may expect direct edits in the original checkout.

### 6. Git Plugin And Change Tracking

Goal: changed files and diffs are visible for every MVP workspace.

Deliverables:

- Git source-control plugin.
- Repository status.
- Current branch and HEAD.
- Remotes.
- Changed files list.
- Staged/unstaged distinction.
- File diff API.
- Commit graph API.
- App-managed checkpoint refs/objects.
- Checkpoint update on explicit action.
- Checkpoint update hook for future `/new` detection.
- Right-panel changed-file UI.
- Click changed file to open diff.

Acceptance criteria:

- Git repo shows branch, status, changed files, and diff.
- Plain folder can be initialized as a local Git repo.
- App initialization does not require a remote.
- Checkpoint refs do not appear as normal user-facing commits.
- Clicking a changed file opens a readable diff.
- Created, modified, and deleted files are represented.
- Basic commit graph renders for a typical repo.

Primary risks:

- Commit graph performance on large repos.
- Hidden refs/checkpoints need careful naming and cleanup.
- Binary and very large file behavior needs clear UX.

### 7. Basic Auth And Security

Goal: personal but not wide-open.

Deliverables:

- Server enrollment token.
- Device enrollment.
- Local credential storage in client.
- Scoped session token.
- Workspace allowlist enforcement.
- Explicit approval for custom command launch.
- Local lifecycle event log.
- TLS/mTLS or equivalent encrypted transport plan for non-local connections.

Acceptance criteria:

- Unknown client cannot attach to server sessions.
- Enrolled device can reconnect without re-enrolling each launch.
- Revoking a device prevents future attach.
- Session creation fails outside allowlisted workspace roots.
- Custom command launch requires visible approval.

Primary risks:

- Security can sprawl quickly.
- Local/LAN transport needs a pragmatic MVP posture without blocking later relay.

### 8. VS Code Continuation

Goal: user can move from TUI control center to editor workflow.

Deliverables:

- "Continue in VS Code" button.
- Open VS Code at the session's isolated workspace instance.
- Pass session/server context through a deep link, local file, or extension command.
- Minimal VS Code extension or documented deep-link fallback.

Acceptance criteria:

- Button opens VS Code in the correct workspace instance.
- Opening VS Code does not create a new agent session.
- Session remains visible and attachable in the control center.
- The original source workspace is not opened accidentally when an isolated instance exists.

Primary risks:

- VS Code URI behavior differs across platforms.
- Extension packaging may not be worth MVP complexity; fallback may be enough.

### 9. Packaging And Install

Goal: a user can install and run the MVP on their own machines.

Deliverables:

- Daemon binary packages for Linux, macOS, Windows.
- Electron desktop packages for Linux, macOS, Windows.
- First-run setup flow.
- Local/LAN connection instructions.
- Basic upgrade path.
- Logs and diagnostics export.

Acceptance criteria:

- Fresh install can enroll a local server and launch a test shell session.
- Linux service can run in background.
- macOS launchd service can run in background.
- Windows service can run in background.
- User can find logs from the UI.

Primary risks:

- Platform signing/notarization can take time.
- Service installation permissions differ by OS.

## Suggested Sequence

### Phase 0: Component Evaluation

Duration: 1-2 weeks.

Tasks:

- Evaluate PTY libraries with Linux/macOS PTY and Windows ConPTY.
- Evaluate xterm.js integration and serialization options.
- Evaluate SQLite layer.
- Evaluate Git implementation strategy: shelling out to `git` versus library.
- Evaluate file watching library.
- Evaluate service installation helpers.
- Evaluate diff rendering component.
- Decide internal wrapper interfaces.

Exit criteria:

- Dependency decision record for each major component.
- Prototype proves PTY output on Linux, macOS, and Windows.
- Prototype proves xterm.js attach to daemon stream.

### Phase 1: Session Engine Prototype

Duration: 2-3 weeks.

Tasks:

- Build daemon skeleton.
- Implement local API.
- Implement platform PTY abstraction.
- Implement session create/list/attach/resize/stop.
- Implement terminal stream WebSocket.
- Build minimal web page with xterm.js attach.

Exit criteria:

- Start a shell session.
- Disconnect browser/client.
- Session keeps running.
- Reconnect and continue interacting.

### Phase 2: Durable Resume

Duration: 2-3 weeks.

Tasks:

- Add SQLite migrations.
- Add sessions table.
- Add terminal event log table.
- Add sequence-number replay.
- Add terminal snapshots.
- Add archive and exited session states.
- Add restart recovery behavior.

Exit criteria:

- Reconnect receives missed output.
- Server restart preserves session history and exited state.
- Snapshot restore works when replay window is unavailable.

### Phase 3: Control Center Client

Duration: 2-3 weeks.

Tasks:

- Build Electron app shell.
- Add shared web UI.
- Add device/server enrollment flow.
- Add session list.
- Add TUI center panel.
- Add placeholder source-control panel.
- Add reconnect and error states.

Exit criteria:

- User can create and resume sessions from desktop app.
- UI supports at least two simultaneous sessions.
- Terminal sizing works reliably.

### Phase 4: Plugins And Agent Launch

Duration: 2 weeks.

Tasks:

- Add plugin manifest schema.
- Add plugin registry.
- Add custom command plugin.
- Add Codex plugin.
- Add Claude Code plugin.
- Add binary detection and launch validation.
- Add plugin-specific env/config hooks.

Exit criteria:

- User can launch Codex if installed.
- User can launch Claude Code if installed.
- User can launch a custom command after approval.
- Missing binary errors are clear.

### Phase 5: Workspace Isolation

Duration: 2-3 weeks.

Tasks:

- Add workspace registration and allowlist.
- Add Git detection.
- Add `git init` flow for plain folders.
- Add workspace instance and lease tables.
- Add managed Git worktree creation.
- Link sessions to workspace instances.
- Add cleanup guard for dirty instances.
- Add UI labels for source workspace and active instance.

Exit criteria:

- Two agents on the same repo run in separate worktrees.
- Dirty worktree cleanup is blocked without confirmation.
- Direct source checkout mode requires explicit override.

### Phase 6: Git Panel And Diffs

Duration: 2-3 weeks.

Tasks:

- Add Git status API.
- Add changed files API.
- Add file diff API.
- Add commit graph API.
- Add checkpoint refs/objects.
- Add right-panel changed-files UI.
- Add diff viewer.
- Add commit graph view.

Exit criteria:

- User can click any changed file and see the diff.
- Created/modified/deleted files display correctly.
- Plain folder initialized by the app gets the same diff workflow.
- Commit graph is usable on a medium-sized repo.

### Phase 7: VS Code Continuation And Packaging

Duration: 2-3 weeks.

Tasks:

- Add VS Code continuation button.
- Open isolated workspace instance.
- Add optional VS Code extension/deep-link support.
- Package daemon and desktop app.
- Add service installation scripts.
- Add diagnostics export.
- Write install and quickstart docs.

Exit criteria:

- Fresh install can launch daemon and desktop client.
- User can start an agent, inspect diffs, open VS Code, disconnect, and reconnect.
- MVP works on Linux, macOS, and Windows at smoke-test level.

## Verification Plan

### Automated Tests

- Unit tests for session state transitions.
- Unit tests for plugin manifest parsing.
- Unit tests for workspace lease rules.
- Unit tests for Git worktree path generation.
- Unit tests for checkpoint ref naming.
- Integration tests for PTY spawn and resize.
- Integration tests for replay from event sequence.
- Integration tests for Git status/diff/checkpoint APIs.
- API tests for auth, workspace allowlist, and session lifecycle.

### Manual Smoke Tests

- Launch shell session on Linux.
- Launch shell session on macOS.
- Launch shell session on Windows native ConPTY.
- Launch Codex plugin.
- Launch Claude Code plugin.
- Disconnect desktop client during active output.
- Reconnect and confirm missed output appears.
- Start two sessions on same repo and edit same file in both.
- Confirm separate worktrees and separate diffs.
- Initialize plain folder as Git and show changed-file diff.
- Open session workspace in VS Code.

### Performance Checks

- Idle daemon CPU usage.
- Memory usage with 1, 5, and 20 sessions.
- Terminal output throughput.
- Reconnect time with large scrollback.
- Git graph generation time on small, medium, and large repos.
- Disk usage from terminal logs and retained worktrees.

## Release Gates

### Alpha Gate

- Linux/macOS/Windows daemon starts.
- Basic shell sessions work.
- Client attach/detach works.
- Terminal replay works.
- No Git or plugin polish required.

### Beta Gate

- Codex and Claude plugins work.
- Workspace registration works.
- Git worktree isolation works.
- Changed-file diffs work.
- Basic auth works.
- VS Code continuation works.

### MVP Gate

- End-to-end workflow works on Linux, macOS, and Windows.
- Two concurrent agents on same repo are isolated.
- Reconnect restores live session output.
- Plain folders can be initialized as Git for diff tracking.
- Packaging and quickstart docs are complete.
- Known limitations are documented.

## Key Risks And Mitigations

| Risk | Mitigation |
| --- | --- |
| Windows ConPTY behavior is inconsistent | Spike first, wrap PTY backend, keep platform-specific tests |
| Terminal snapshot fidelity is hard | Start with replay-first recovery and simple snapshots |
| Git worktrees fail for edge-case repos | Detect clearly, offer clone/copy fallback, document unsupported cases |
| Plugin API over-design | Ship built-in plugins first, keep manifest small |
| Commit graph is slow | Cache results and defer advanced graph features |
| Service installation is painful | Provide foreground dev mode and background service mode separately |
| Security scope expands | Keep MVP to personal auth, device enrollment, and workspace allowlist |
| Electron packaging takes longer than expected | Keep web client runnable independently for testing |

## MVP Cut Rules

If schedule pressure appears, keep:

- durable PTY sessions,
- reconnect/replay,
- native Windows daemon support,
- basic desktop client,
- workspace isolation,
- changed-file diffs.

Cut or defer:

- commit graph polish,
- VS Code extension beyond a simple open action,
- advanced terminal search,
- production relay/gateway,
- personal pool placement,
- memory UI,
- user-facing Git write workflows.

The MVP wins only if the core loop feels reliable:

```text
start agent -> disconnect -> agent keeps working -> reconnect -> inspect terminal and code changes
```
