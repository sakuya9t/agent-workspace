# Agent Session Manager Requirements

## Overview

The product is a cross-platform personal tool for managing long-running coding agent sessions on remote servers. A user can start an agent in a remote workspace, disconnect, reconnect later, and continue the same live session with no lost terminal output.

The product is not a team session system. Teammates collaborate through repository workflows and maintain their own private agents, sessions, servers, and memory copies.

Supported agent targets:

```text
Codex
Claude Code
opencode
myclaw
Hermes
custom terminal command
```

## Product Principles

- Sessions are durable server-side objects.
- Client connections are temporary views into sessions.
- Session continuity is never simulated through automatic relaunch.
- Fresh-client resume uses server-side terminal emulator state.
- Native sessions are owned by a single out-of-process holder (`asmux`) that survives daemon restart.
- Remote workspaces can start as Git repositories or plain directories.
- Plain directories use guided local `git init` for full change tracking.
- Server daemons run natively on Linux, macOS, and Windows.
- Windows server support does not require WSL.
- Agent TUIs, session backends, and source-control panels use plugin boundaries.
- Concurrent agents on the same repo use separate workspace instances by default.
- Isolated workspace instances support setup hooks for secrets, caches, dependency installation, and generated files.
- The first client is a real control center, not a thin terminal wrapper.
- The control center highlights sessions with new activity or likely user-blocking prompts.
- Long-term memory belongs to an agent profile and survives terminal session boundaries.
- Mature dependencies are preferred behind replaceable internal interfaces.
- Long-term operations are transparent through a personal hybrid machine pool.

## Personas

### Solo Developer

Runs agents on a workstation, laptop, cloud VM, or homelab server and checks progress from multiple devices.

### Power User With Private Networks

Runs agents behind NAT, bastion hosts, VPNs, or nested private networks such as `local -> 10.0.0.5 -> 192.168.122.10`.

## User Stories

### Session Management

- As a user, I can register a server.
- As a user, I can register or select a remote workspace directory.
- As a user, I can start an agent session in a selected workspace.
- As a user, I can see active, detached, exited, failed, stopped, and archived sessions.
- As a user, I can attach to an existing session.
- As a user, I can detach without stopping the session.
- As a user, I can stop or archive a session intentionally.
- As a user, I can resize a terminal and have the backend session resize.
- As a user, I can trust that the app never silently restarts an agent as the same session.
- As a user, I can create an explicit follow-up session from the same profile and workspace.

### Reconnect And Continuity

- As a user, I can disconnect while an agent keeps running.
- As a user, I can reconnect and see output produced while I was away.
- As a user, I can switch devices and continue the same session.
- As a user, I can recover the terminal screen after a client crash.
- As a user, I can recover session history after daemon restart when the backend session is still alive.
- As a user, I can attach from a fresh device and see the current TUI screen without replaying the whole session from the beginning.

### Agent Support

- As a user, I can launch Codex from a workspace.
- As a user, I can launch Claude Code from a workspace.
- As a user, I can define a custom agent command.
- As a user, I can pass launch arguments and environment variables safely.
- As a user, I can add future agent-like TUIs through plugins.
- As a user, I can configure agent plugins without changing the core app.

### Workspace And Source Control

- As a user, I can run an agent in any allowed remote directory.
- As a user, I can initialize a plain folder as a local Git repository.
- As a user, I can see Git branch, status, changed files, diffs, and commit graph.
- As a user, I can click any changed file and see what changed.
- As a user, I can see changes since the active session checkpoint.
- As a user, I can update a checkpoint manually.
- As a user, I can create a new session segment with `/new` and advance the checkpoint.
- As a user, I can run multiple agents against the same repository without sharing one writable working tree.
- As a user, I can see which isolated workspace instance each session uses.
- As a user, I can merge, rebase, apply, or export changes through source-control workflows.
- As a user, I can configure workspace setup rules for copied local files, linked caches, and bootstrap commands.

### TUI Control Center

- As a user, I can use a three-panel control center:
  - left: sessions, workspaces, agent profiles,
  - middle: large agent TUI,
  - right: changed files, source-control history, status, and diffs.
- As a user, I can switch sessions quickly.
- As a user, I can search or inspect terminal history.
- As a user, I can tell which sessions have new activity.
- As a user, I can tell which sessions likely need my approval or input.

### Human-In-The-Loop

- As a user, I can continue the current session in VS Code.
- As a user, I can open the session's isolated workspace instance in VS Code.
- As a user, I can keep the agent session alive while editing files.

### Networking

- As a user, I can connect directly to a reachable server.
- As a user, I can connect to a remote server through an SSH local port-forward.
- As a user, I can connect to private servers through relay, gateway, or SSH routes.
- As a user, I can resume after transient network loss.

### Long-Term Memory

- As a user, I can create an agent profile that survives multiple terminal sessions.
- As a user, I can preserve project context across `/new` and follow-up sessions.
- As a user, I can review, edit, or clear remembered information.
- As a user, I can export an agent memory bundle.
- As a user, I can import a memory bundle into a private agent profile.
- As a user, I can duplicate useful project context for a teammate without exposing live sessions.

### Personal Pool Placement

- As a user, I can enroll multiple personal machines into a pool.
- As a user, I can see health, reachability, platform, installed plugins, and capacity for each machine.
- As a user, I can launch by intent, such as `Start Codex on repo X`.
- As a user, I can inspect why a machine was selected.
- As a user, I can override automatic placement.

## Functional Requirements

### Server Daemon

The daemon must:

- run as a native background process on Linux, macOS, and Windows,
- run as a user-scoped background process by default,
- expose authenticated APIs for clients,
- coordinate sessions through pluggable session backends,
- ship a native PTY backend via a single out-of-process holder (`asmux`) using Unix PTYs on Linux/macOS and ConPTY on Windows,
- reattach to the live holder-owned backend sessions after daemon restart,
- persist session metadata,
- persist terminal output events,
- persist terminal emulator snapshots for history, inspection, diagnostics, and fallback recovery,
- persist structural session summary records,
- enforce workspace allowlists,
- register workspaces,
- create isolated workspace instances,
- support agent plugins,
- support session backend plugins,
- support source-control plugins,
- support the built-in Git plugin,
- support guided local Git initialization,
- provide logs, health, and diagnostics.

The daemon must not:

- require WSL for Windows-native operation,
- bind live session lifetime to a client socket,
- silently relaunch an exited or failed agent as the same session,
- allow unrelated sessions to write into the same workspace instance by default.

### Session Backend Plugins

The system must support replaceable session backend plugins.

The native backend must:

- use Unix PTYs on Linux and macOS,
- use ConPTY on Windows,
- run out of process from the daemon,
- use a single out-of-process holder (`asmux`) for all live sessions, not one sidecar per session (see docs/durable-sessions.md),
- own live PTY masters or ConPTY handles and child process handles,
- keep the headless terminal emulator in the daemon, not the holder,
- expose backend operations over local IPC,
- publish one holder IPC socket in a well-known per-user runtime directory,
- maintain terminal continuity across client disconnects,
- drain output generated while the daemon reconnects.

Backend plugins must provide:

- create session,
- attach session,
- query session,
- stream output,
- drain events,
- send input,
- resize session,
- stop session,
- export snapshot,
- report health,
- report exit state.

Backend plugin constraints:

- no client socket owns backend lifetime,
- no automatic relaunch as continuity,
- no workspace allowlist bypass,
- no workspace isolation bypass.

Holder lifetime requirements (single out-of-process holder, `asmux` — see
docs/durable-sessions.md):

- daemon restart leaves the holder and its sessions running,
- daemon upgrade leaves the holder and its sessions running,
- the holder keeps its original binary until a soft-reboot rotates it,
- newly created sessions use the currently installed holder binary,
- daemon startup scans the holder runtime directory and reconciles live sessions with session records,
- live holder sessions without matching session records become orphaned session records,
- orphaned sessions are visible to the owner and can be adopted or terminated,
- a holder crash loses all live sessions but preserves history, reconciling them to `indeterminate` (not `failed`); the terminal emulator lives in the daemon, so a parser panic never takes the holder down,
- holder maintenance that terminates a live session is explicit and session-scoped.

### Terminal Persistence

The system must:

- capture terminal output while the backend session is alive,
- assign monotonic sequence numbers to terminal events,
- allow resume from `last_seen_event_seq`,
- store terminal emulator snapshots,
- restore fresh clients from a daemon-side terminal snapshot plus later events,
- deliver terminal snapshots as synthesized ANSI repaint streams with cursor and mode metadata,
- retain useful scrollback,
- handle terminal resize events.

The system must not:

- replay raw bytes from an arbitrary mid-stream offset as the primary resume mechanism,
- treat full event replay from session start as the normal attach path,
- lose output silently during writer stalls or disk-full conditions.

The system must support retention policy for:

- terminal events,
- terminal snapshots,
- backend output spools,
- local lifecycle events.

Backpressure and gap requirements:

- event writes use bounded buffers,
- stalled event writes apply backpressure to PTY reading after buffers are full,
- default behavior stalls the agent rather than dropping output,
- dropped output occurs only for unrecoverable storage failure or an explicit per-session never-stall policy,
- every dropped range creates an explicit gap marker,
- clients display gap markers in terminal history,
- soak tests cover long-running high-volume output.

### Single-Device Active Session (Takeover)

A session has at most one attached client at a time. The system must:

- allow the same owner to attach from any of their devices,
- when a new device attaches to a session that already has an active client,
  forcibly detach the previous client (takeover) and grant the new one — so
  continuing a session on another device closes it on the old device,
- give the evicted client a clear "taken over on another device" signal and let
  it re-attach later, resuming from where it left off,
- resize the backend terminal to match the single active client,
- request a backend repaint after resize,
- surface the active device and terminal size in diagnostics.

Concurrent multi-client input on one session is out of MVP scope (superseded by
takeover); see docs/asmux-protocol.md → Attach model.

### Session Summary Records

The system must write a structural session summary record on session exit and explicit segment boundaries.

The MVP summary record must include:

- session ID,
- agent plugin,
- workspace and workspace instance,
- start and end timestamps,
- duration,
- exit status,
- final checkpoint,
- terminal event range,
- changed-file counts by basic change type.

The MVP summary record must not require LLM summarization.

### Client

The first client must be an Electron desktop app with a shared web UI.

Chosen MVP frontend stack:

| Layer | Requirement |
| --- | --- |
| App framework | React 19 + TypeScript + Vite |
| Desktop shell | Electron |
| Terminal | xterm.js |
| Local UI state | Zustand |
| Server state | TanStack Query |
| Components | shadcn/ui + Tailwind |
| Layout | Dockview |
| Code and diff view | CodeMirror |
| Markdown parsing | Marked |
| Syntax highlighting | Shiki |
| Diagrams | Mermaid |
| Math | KaTeX |
| Sanitization | DOMPurify |
| Icons | lucide-react |

Client requirements:

- run on macOS, Windows, and Linux,
- share UI code with a browser client,
- store persistent device enrollment credentials in Electron for MVP,
- use session-only browser enrollment in MVP,
- render terminals through xterm.js,
- support the three-panel control center,
- persist Dockview layout locally,
- use Zustand for local UI state,
- use TanStack Query for server-derived state,
- render code and diffs with CodeMirror,
- render markdown, syntax highlighting, diagrams, and math through a sanitized rich-output pipeline,
- support reconnect and resume,
- support launching configured agents,
- support opening the isolated workspace instance in VS Code.

Electron requirements:

- `contextIsolation` enabled,
- `nodeIntegration` disabled,
- narrow preload API,
- strict content security policy,
- no direct renderer access to daemon credentials beyond scoped client APIs.

Markdown and rich output requirements:

- define rich-output sources as repo markdown files, agent transcripts exposed by agent plugins, session summary records, and memory summary records,
- render session summary and memory summary records as rich-output inputs,
- sanitize untrusted markdown HTML before rendering,
- run Mermaid with a strict security posture by default,
- lazy-load Shiki, Mermaid, and KaTeX,
- avoid blocking terminal rendering on markdown rendering.

Terminal output security requirements:

- disable OSC 52 clipboard writes by default,
- gate OSC 8 hyperlinks through link policy,
- prevent terminal title sequences from controlling trusted app chrome,
- treat terminal output as untrusted content,
- expose terminal escape policy in diagnostics.

### Agent Plugins

Agent plugins must define:

- display metadata,
- binary detection,
- launch command and arguments,
- supported operating systems,
- environment requirements,
- terminal startup behavior,
- session-boundary detection such as `/new`,
- transcript location and parser behavior when the agent writes structured transcripts,
- memory injection behavior,
- readiness and health detection.

MVP plugin implementation:

- compiled-in Rust traits,
- static plugin registry,
- no untrusted plugin package loading,
- no dynamic code loading.

MVP built-ins:

```text
codex
claude
custom_command
```

Post-MVP built-ins:

```text
opencode
myclaw
hermes
```

### Source Control And Change Tracking

The source-control plugin interface must support:

- repository detection,
- status,
- branch, tag, or revision metadata,
- history graph or provider-specific history,
- changed-file list,
- file diff,
- workspace isolation strategy,
- checkpoint creation and update.

The Git plugin must support:

- repository detection,
- per-session Git worktree creation and cleanup,
- current branch,
- HEAD commit,
- remotes,
- working tree status,
- staged and unstaged changes,
- commit graph,
- file and commit diffs,
- app-managed checkpoint refs or tree objects.

Change tracking requirements:

- every changed file in the UI is clickable,
- existing Git repositories use Git status and diff,
- plain folders can be initialized with local `git init`,
- checkpoint refs stay out of user-facing branch history,
- checkpoint updates preserve prior session history,
- explicit "New segment" action advances the active checkpoint,
- the "New segment" action sends the configured agent command such as `/new`,
- plugin-provided input sniffing augments explicit segment actions,
- ignored, generated, binary, and large files use explicit display policy.

Folders without Git support agent sessions. Full diff tracking requires Git initialization.

### Workspace Isolation

The system must distinguish:

- source workspace: registered repo or folder selected by the user,
- workspace instance: isolated working directory assigned to one independent agent session.

The system must:

- create one writable workspace instance per independent session by default,
- attach multiple same-owner clients to the same session and instance,
- block accidental writes into another running session's instance,
- show the workspace instance path or label in the UI,
- retain dirty instances until merged, exported, discarded, or cleaned up by explicit action,
- run configured setup hooks after instance creation.

Workspace setup hooks must support:

- copied files such as `.env`,
- symlinked caches such as dependency stores,
- bootstrap commands such as install or code generation,
- environment variables for bootstrap commands,
- per-workspace setup status in the UI.

Git worktree isolation must:

- use a separate working directory and index per session,
- avoid relying on branch selection alone,
- create app-managed branches, detached heads, or refs as needed,
- surface worktree failures clearly,
- detect submodule, LFS, custom hook path, nested repo, and required ignored-file issues.

Fallback isolation order:

```text
local clone -> reflink/copy-on-write copy -> full copy -> unsupported
```

### Networking

The system must support:

- direct local/LAN client-to-server connection,
- SSH local port-forward connection for MVP remote access,
- authenticated and encrypted transport,
- connection resume after transient loss.

Connectivity extensions:

- reverse relay for NAT/private servers,
- gateway routing for private subnets,
- SSH jump route for advanced users,
- route discovery,
- route health checks,
- alternate route fallback.

### Attention Signals

The daemon must compute MVP attention signals from:

- terminal event stream activity,
- decoded recent output text,
- bell events,
- backend health events,
- process exit or failure events.

Agent plugins may contribute prompt and approval patterns that match recent output text. MVP attention detection must not require direct plugin access to daemon terminal screen state.

### Personal Hybrid Pool And Placement

Each enrolled node reports:

- platform and architecture,
- reachable routes,
- installed agent plugins,
- source-control plugin support,
- configured workspace roots,
- CPU, memory, disk, and process load,
- power state,
- recent health and heartbeat data.

Placement evaluates:

- requested agent plugin,
- workspace or repository location,
- repo locality,
- clone or sync cost,
- required credentials and secrets,
- load and disk availability,
- route quality and latency,
- OS compatibility,
- user placement policy,
- recent failures.

### Memory

The memory system must separate:

- terminal session,
- agent profile,
- workspace,
- owner.

The memory system must:

- summarize completed sessions and `/new` boundaries,
- preserve project decisions,
- preserve user preferences,
- link summaries to file changes and commits,
- inject relevant memory into future sessions,
- support review, edit, and delete,
- export memory bundles,
- import memory bundles as private copies,
- record provenance for imported memory.

### Security

The system must:

- authenticate clients,
- authenticate servers,
- encrypt client-server transport,
- enroll devices with per-device keys,
- restrict accessible workspaces,
- require explicit approval for arbitrary custom commands,
- log session lifecycle events,
- prevent relay infrastructure from requiring plaintext terminal access.

MVP secrets-at-rest disclosure:

- terminal logs can contain secrets printed by agents or commands,
- logs are stored on the daemon host,
- retention defaults limit stored history,
- encryption-at-rest and production-grade redaction remain hardening items.

The system supports:

- device revocation,
- workspace allowlists,
- secret redaction in logs and summaries.

## Non-Functional Requirements

### Performance

- The daemon runs comfortably on small VMs.
- Idle sessions consume minimal CPU.
- Terminal streaming remains interactive over normal network latency.
- Reconnect restores a typical session within a few seconds.
- Git graph generation is cached or incremental for large repositories.
- Rich markdown rendering does not block terminal interaction.

### Reliability

- Agent sessions survive client disconnects.
- Daemon restart reattaches to live session backends when available.
- Daemon restart restores metadata and terminal history.
- Agent exit preserves status, output, and summary.
- Relay disconnects do not kill local server sessions.

### Portability

- Server target: native Linux, macOS, and Windows.
- Windows server target uses native Windows APIs and ConPTY.
- Desktop target: macOS, Windows, Linux.
- Web target: modern Chromium, Firefox, and Safari.
- Mobile target: responsive web/PWA.
- Service management target: systemd user units, launchd LaunchAgents, and Windows per-user startup mechanisms.
- Client and daemon APIs carry explicit protocol versions.

### Installed Service Scope

- Default daemon installation runs as the enrolled OS user.
- Linux uses `systemd --user`; sessions that survive logout require `loginctl enable-linger`.
- macOS uses LaunchAgents.
- Windows uses a per-user scheduled task or equivalent user-session startup path with restart-on-failure recovery.
- Agent login and authentication flows must work from the installed daemon context on each supported OS.

### Observability

The server exposes:

- health endpoint,
- active session count,
- session state,
- last activity time,
- backend health,
- relay/gateway connection state,
- node capability and health summary,
- placement eligibility,
- storage usage,
- recent errors.

### Data Retention

The product defines retention policies for:

- terminal event logs,
- terminal snapshots,
- backend output spools,
- session summaries,
- local event logs,
- memory entries,
- workspace instances,
- workspace leases,
- workspace checkpoints,
- app-managed Git checkpoint refs,
- node health history,
- placement decisions.

## Excluded Product Scope

The product does not support:

- shared live agent sessions,
- multi-person terminal control,
- team permission models,
- organization administration,
- live session transfer between teammates,
- Windows-native daemon dependency on WSL,
- automatic relaunch presented as session continuity.

## MVP Scope

### Included

- Native server daemon for Linux, macOS, and Windows.
- Electron desktop client with shared web UI.
- React 19 + TypeScript frontend stack.
- Direct local/LAN connection.
- SSH local port-forward connection.
- Basic server/device auth.
- Persistent sessions through the session backend interface.
- Built-in native PTY backend via a single out-of-process holder (`asmux`).
- Reconnect with emulator snapshot resume and tail replay.
- Structural session summary records.
- Workspace registration and allowlist.
- Agent plugin registry.
- Built-in Codex, Claude Code, opencode, and custom command plugins.
- Source-control plugin interface.
- Built-in Git plugin with status, diffs, and commit graph.
- Git-worktree isolation for concurrent sessions on the same repo.
- Changed-file list with click-to-diff behavior.
- Local Git initialization for plain folders.
- Git-backed workspace checkpoints.
- Workspace setup hooks.
- Session activity and needs-attention signals.
- Terminal escape-sequence security policy.
- Early "Continue in VS Code" action.

### Deferred

- Native mobile apps.
- Production relay service.
- Production gateway route manager.
- Production tmux backend.
- Automatic personal pool placement.
- Repository sync orchestration across nodes.
- Browser-based full editor replacement.
- User-facing Git write operations.
- Advanced memory UI.

## Milestones

1. Local session prototype.
2. Durable resume.
3. Plugin foundation and control center UI.
4. Source control and change tracking.
5. Remote connectivity.
6. Human-in-the-loop continuation.
7. Memory.
8. Personal pool placement.

## Open Decisions

- First streaming protocol after WebSocket.
- Terminal event representation.
- Terminal log segment storage.
- Memory injection strategy per agent.
- Agent plugin trait and capability API.
- Source-control plugin trait and panel API.
- Session backend process manifest and local IPC protocol.
- Production tmux backend scope after MVP validation spike.
- Backend-local output spool storage.
- Server-side terminal emulator crate selection.
- Persisted terminal snapshot cadence.
- Checkpoint naming, retention, and garbage collection.
- Worktree naming, retention, and garbage collection.
- Large-file, binary-file, generated-file, and ignored-path diff policy.
- User-scoped service installation, upgrades, and log collection.
- Client/daemon protocol compatibility during version skew.
- Cross-machine repository identity for future placement.
- Placement scoring defaults.
- Repository sync and secret availability rules for placement.
