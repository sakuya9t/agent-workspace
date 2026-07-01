# Agent Session Manager Requirements

## Overview

The product is a cross-platform personal tool for managing long-running coding agent sessions on remote servers. Users should be able to start an agent in a remote directory, disconnect their client, reconnect later, and continue the exact same session with no lost terminal output.

The product intentionally does not provide shared team sessions. If teammates collaborate, they do so through repository workflows and each person maintains their own private agent sessions.

Supported agent targets include Claude Code, Codex, opencode, myclaw, Hermes, and custom terminal commands.

## Product Principles

- Sessions are durable remote objects.
- Client connections are temporary views into those sessions.
- Remote workspaces may be Git repositories or plain directories.
- Networking must work for public servers, private servers, and multi-hop private networks.
- Server-side components should be efficient, lightweight, and safe to run continuously.
- Server daemons should run natively on Linux, macOS, and Windows.
- Windows server support must not require WSL.
- Long term, users should manage a personal hybrid pool of machines instead of manually choosing a host for every agent session.
- New agent sessions should be placed automatically on the most suitable available machine when possible.
- The first client should be useful as a real control center, not just a terminal wrapper.
- Long-term agent memory should survive terminal session boundaries.
- Agent sessions are private and personal.
- Collaboration happens through repositories, not shared live sessions.
- Agent memory can be exported and imported, but memory is not a live shared team resource.
- Agent TUIs are integrated through plugins.
- Source-control panels are integrated through plugins.
- Changed-file diffing is Git-backed in the MVP, including folders that the app initializes as local Git repositories.
- Concurrent agents working on the same repo should be isolated in separate workspace instances by default.
- Mature existing components should be used when they satisfy the requirement better than custom implementation.

## User Personas

### Solo Developer

Runs agents on a workstation, cloud VM, homelab server, or laptop and wants to check progress from multiple devices.

### Power User With Private Networks

Runs agents on machines behind NAT, bastion hosts, VPNs, or nested private networks.

## Primary User Stories

### Session Management

- As a user, I can register a remote server.
- As a user, I can register or select a remote workspace directory.
- As a user, I can start an agent session in a selected workspace.
- As a user, I can see all active, exited, and archived sessions.
- As a user, I can attach to an existing session.
- As a user, I can detach from a session without stopping it.
- As a user, I can stop or archive a session intentionally.
- As a user, I can resize a terminal and have the remote PTY reflect the new size.
- As a user, I can eventually start an agent from intent, such as "run Codex on this repo", without choosing a host manually.
- As a user, I can see which machine was selected for a new session and why.
- As a user, I can override automatic placement when I want direct control.

### Reconnect And Continuity

- As a user, I can disconnect my client while an agent keeps running on the server.
- As a user, I can reconnect and see output produced while I was away.
- As a user, I can resume the same session rather than creating a new one.
- As a user, I can switch devices and continue controlling the same session.
- As a user, I can recover the terminal screen after a client crash.

### Agent Support

- As a user, I can launch Codex from a workspace.
- As a user, I can launch Claude Code from a workspace.
- As a user, I can launch opencode from a workspace.
- As a user, I can launch myclaw from a workspace.
- As a user, I can launch Hermes from a workspace.
- As a user, I can define a custom agent command.
- As a user, I can pass environment variables and launch arguments safely.
- As a user, I can add support for future agent-like TUIs through plugins.
- As a user, I can configure an agent plugin without changing the core app.

### Workspace And Source Control

- As a user, I can run an agent in any allowed remote directory.
- As a user, I can use the tool even if the directory is not connected to version control.
- As a user, I can see Git branch, status, changed files, and diffs when Git is available.
- As a user, I can view a complete Git commit graph with branches and tags.
- As a user, I can understand what changed while the agent was working.
- As a user, I can collaborate with teammates by committing, pushing, pulling, and reviewing repository changes outside the private agent session.
- As a user, I can add support for other source-control systems such as SVN through plugins.
- As a user, I can click any changed file and see what changed.
- As a user, I can let the app initialize a plain folder as a local Git repository to enable change tracking.
- As a user, I can see changes since the session opened the folder after the app creates a Git-backed baseline.
- As a user, I can create a new session with `/new` and have the change checkpoint update for the next session segment.
- As a user, I can run multiple agents against the same repository without them sharing one writable working tree.
- As a user, I can see which isolated workspace instance or worktree each agent session is using.
- As a user, I can merge, rebase, or apply changes between isolated agent workspaces through source-control flows.

### TUI Control Center

- As a user, I can use a three-panel interface:
  - left: sessions and workspaces,
  - middle: large agent TUI,
  - right: changed files, source-control history, and workspace state.
- As a user, I can switch between sessions quickly.
- As a user, I can search or inspect terminal history.
- As a user, I can tell which sessions have new activity.

### Human-In-The-Loop Mode

- As a user, I can continue the current session in VS Code.
- As a user, I can open the same isolated workspace instance while keeping the agent session alive.
- As a user, I can eventually pair program in a richer GUI with editor, terminal, file tree, and diffs.

### Networking

- As a user, I can connect directly to a server with a reachable IP or hostname.
- As a user, I can connect to a server without a public IP through a reverse relay.
- As a user, I can connect through a gateway host into a private network.
- As a user, I can support routes such as `local -> 10.0.0.5 -> 192.168.122.10`.
- As a user, I can use SSH or a jump host as an early fallback.

### Personal Hybrid Agent Pool

- As a user, I can enroll multiple personal machines into a pool.
- As a user, I can include local, LAN, private, and cloud machines in that pool.
- As a user, I can see health, reachability, platform, installed agent plugins, and capacity for each machine.
- As a user, I can set placement preferences such as prefer local, avoid battery-powered machines, or allow cloud machines.
- As a user, I can let the app choose the best machine for a new agent session.
- As a user, I can pin a workspace or agent profile to a preferred machine when needed.

### Long-Term Memory

- As a user, I can create a logical agent profile that survives multiple terminal sessions.
- As a user, I can start a new chat/session with `/new` while preserving useful project context.
- As a user, I can review or clear remembered information.
- As a user, I can keep memory scoped to a workspace, agent profile, and owner.
- As a user, I can export an agent's memory into a portable bundle.
- As a user, I can import a memory bundle into another personal agent profile.
- As a user, I can duplicate useful project context for a teammate without exposing my live sessions.
- As a user, I can choose whether exported memory includes only durable memory entries or also selected session summaries.

### Personal Collaboration Model

- As a user, my agent sessions are private to me.
- As a user, I do not share live terminal control with teammates.
- As a user, I collaborate with teammates through the repository and normal code review workflows.
- As a user, I can choose to export memory so another person can import a copy into their own agent.
- As a user, importing another person's memory bundle creates a private copy for my agent.

## Functional Requirements

### Server Daemon

The server daemon must:

- run as a native background process on Linux, macOS, and Windows,
- expose an API for clients,
- create and supervise PTY-backed agent sessions,
- use native PTY mechanisms, including ConPTY on Windows and Unix PTYs on Linux/macOS,
- continue running sessions after client disconnect,
- persist session metadata,
- persist terminal output events,
- produce terminal snapshots for recovery,
- support workspace registration,
- support agent plugins,
- support source-control plugins,
- support Git inspection through the built-in Git plugin,
- support guided local Git initialization when no source-control plugin applies,
- support Git-backed checkpoint change tracking,
- support isolated workspace instances for concurrent sessions,
- prevent unrelated sessions from writing into the same working tree by default,
- support server/device authentication,
- provide logs and basic health information.

The Windows daemon must be Windows-native. It should not require WSL, a Linux userspace, or POSIX-only shell behavior. If a user wants to manage a WSL workspace later, that should be an optional workspace adapter rather than the foundation of Windows support.

The daemon should isolate platform-specific behavior behind internal abstractions for:

- PTY management,
- process supervision and termination,
- background service installation,
- filesystem watching,
- path normalization,
- permissions,
- local IPC.

### Dependency Strategy

The system should prefer proven existing components over custom implementation when they satisfy the product requirements.

Dependency candidates include:

- terminal rendering,
- PTY handling,
- source-control operations,
- storage,
- filesystem watching,
- service installation,
- relay/tunnel transport,
- diff rendering,
- UI framework components.

Dependencies must:

- support required target platforms,
- avoid making Windows server support depend on WSL,
- preserve product-owned session lifecycle and reconnect semantics,
- preserve private personal-session behavior,
- support plugin and replacement boundaries,
- have acceptable licensing,
- have acceptable security and maintenance posture,
- be observable and debuggable enough for long-running daemon use.

If a component satisfies most requirements but not all, the architecture should wrap it behind an internal interface and document the gap, fallback, or replacement path.

### Session Lifecycle

The system must support these session states:

```text
starting
running
detached
exited
failed
stopped
archived
```

State rules:

- A running session may have zero or more attached clients owned by the same user.
- A detached session is still running but has no active client.
- An exited session preserves metadata, terminal history, and summary.
- A stopped session was intentionally terminated by a user or policy.
- An archived session is hidden from default active views but can be reopened for history.

### Terminal Persistence

The system must:

- capture all PTY output while the server daemon is running,
- assign sequence numbers to terminal events,
- allow clients to resume from their last seen sequence number,
- store periodic terminal snapshots,
- recover from missing old events by sending a snapshot,
- preserve enough scrollback for useful review,
- handle terminal resize events.

The system should:

- compress older terminal logs,
- support retention policies per workspace or server,
- provide terminal output search,
- redact obvious secrets where possible.

### Client

The first client should:

- run on macOS, Windows, and Linux through Electron,
- use the same core UI in a web browser where possible,
- render terminals through `xterm.js`,
- provide session list, TUI panel, and changes/source-control panel,
- support reconnect and resume,
- support launching configured agents,
- support opening a session or workspace in VS Code.

Mobile should initially be supported through a responsive web/PWA client unless there is a strong reason to build native mobile clients earlier.

### Agent Plugins

The system must support agent integrations through plugins.

Agent plugins must be able to define:

- agent display metadata,
- binary detection,
- launch command and arguments,
- supported operating systems,
- environment requirements,
- PTY startup behavior,
- optional memory injection behavior,
- optional session-boundary detection such as `/new`.

Agent plugins must be able to provide platform-specific launch behavior without forcing Windows users through WSL.

The MVP should ship built-in plugins for Codex and Claude Code. Additional built-in plugins for opencode, myclaw, and Hermes should follow the same plugin interface.

### Source Control And Change Tracking

The system must support source-control integrations through plugins.

The MVP must support a Git plugin with read and workspace-isolation operations:

- repository detection,
- per-session Git worktree creation and cleanup,
- current branch,
- HEAD commit,
- remotes,
- working tree status,
- staged and unstaged changes,
- commit graph,
- file and commit diffs.

The source-control plugin model should support other providers later, including SVN.

Source-control plugins must handle platform-specific executable discovery, path formats, and line endings.

Source-control plugins should declare their workspace isolation strategy. If a provider cannot create isolated working directories itself, the system should use an explicit clone/copy fallback or mark concurrent isolated sessions unsupported for that provider.

Every changed file in the UI must be clickable and show a diff.

Diff behavior:

- If a source-control plugin applies, use that provider's status and diff capabilities.
- If no source-control plugin applies and no supported repository is detected, offer to initialize the folder as a local Git repository.
- After Git initialization, compare against the active Git-backed workspace checkpoint.
- The default checkpoint is the folder state when the session opened the workspace.
- When the agent creates a new session segment with `/new`, the active checkpoint updates.
- Users should be able to manually update the active checkpoint.

For Git-backed checkpoints, the system must:

- track created, modified, and deleted files since the checkpoint,
- show text diffs for changed files,
- avoid tracking ignored, generated, or very large files by default,
- preserve prior session history when a checkpoint is updated,
- avoid creating user-facing commits unless the user explicitly asks,
- use app-managed Git checkpoint refs, commits, or tree objects for baseline state.

If a user declines Git initialization for a plain folder, agent sessions should still run, but full changed-file diff tracking is unavailable until Git is initialized.

User-facing Git write operations such as commit, checkout, rebase, merge, and stash are optional after MVP. Internal worktree and checkpoint management are part of the MVP infrastructure.

### Workspace Isolation

The system must isolate concurrent agent sessions that target the same repository on the same host.

Definitions:

- Source workspace: the registered repository or folder selected by the user.
- Workspace instance: the isolated working directory assigned to one agent session.

The system must:

- create a separate writable workspace instance for each independent agent session by default,
- allow multiple clients owned by the same user to attach to the same session and same workspace instance,
- prevent a new independent session from writing into another running session's workspace instance by default,
- show the workspace instance path or label in the session UI,
- retain stopped or exited workspace instances until changes are merged, exported, discarded, or explicitly cleaned up.

For Git-backed workspaces, the MVP should use Git worktrees as the default isolation mechanism.

Git worktree isolation must:

- give each agent session a separate working directory and Git index,
- avoid relying on branch selection alone for isolation,
- create app-managed branches, detached heads, or refs when needed,
- support merge/rebase/apply workflows between isolated instances,
- detect and surface worktree creation failures clearly.

Fallback isolation options may include local clones, reflink/copy-on-write copies, or full directory copies. These fallbacks should be explicit because they can use more disk space and may have different cleanup behavior.

### Networking

The system must support:

- direct client-to-server connections,
- reverse relay for NAT/private servers,
- gateway routing for private subnets,
- connection resumption after transient network loss,
- authenticated and encrypted transport.

The system should support:

- route discovery,
- route health checks,
- fallback to alternate routes,
- SSH transport for early advanced-user workflows.

### Personal Hybrid Pool And Placement

Long term, the system should support automatic placement across a user's personal hybrid machine pool.

Each enrolled node should report:

- platform and architecture,
- reachable routes,
- installed agent plugins,
- source-control plugin support,
- configured workspace roots,
- CPU, memory, disk, and process load,
- power state where available,
- recent health and heartbeat data.

When launching a new agent session, the placement service should consider:

- requested agent plugin,
- workspace or repository location,
- whether the repo already exists on a node,
- cost of cloning, fetching, or syncing the repo,
- availability of required credentials and secrets,
- node load and disk availability,
- route quality and latency,
- operating system compatibility,
- user placement preferences,
- recent node failures.

The system should:

- preview where a session will run before launch when useful,
- explain why a node was selected,
- support manual override,
- provide fallback candidates if the selected node fails,
- keep placement personal to the user-owned pool.

### Memory

The system must separate:

- terminal session,
- agent profile,
- workspace,
- owner.

The memory system should:

- summarize completed or crossed sessions,
- preserve project decisions,
- preserve user preferences,
- link summaries to file changes and commits when available,
- inject relevant memory into future sessions,
- allow memory review and deletion,
- export memory as a portable bundle,
- import memory into a different personal agent profile,
- record provenance for imported memory.

### Security

The system must:

- authenticate clients,
- authenticate servers,
- encrypt client-server transport,
- restrict accessible workspaces,
- require explicit user approval for dangerous setup actions,
- log local session lifecycle events,
- avoid exposing secrets through relay infrastructure.

The system should:

- support device revocation,
- support workspace allowlists,
- support secret redaction in logs and summaries.

## Non-Functional Requirements

### Performance

- Server daemon should be lightweight enough to run on small VMs.
- Idle sessions should consume minimal CPU.
- Terminal streaming should feel interactive over normal network latency.
- Reconnect should restore a typical session within a few seconds.
- Git graph generation should be cached or incremental for large repositories.

### Reliability

- Agent sessions should survive client disconnects.
- Server daemon restart recovery should restore metadata and terminal history.
- If an agent process exits, the session record should preserve exit status and output.
- Relay disconnects should not kill local server sessions.

### Portability

- Server target: native Linux, macOS, and Windows.
- Windows server target must use native Windows APIs and ConPTY rather than WSL.
- Service management should support systemd on Linux, launchd on macOS, and Windows Service Control Manager on Windows.
- Plugin APIs should expose OS/platform capabilities so plugins can adapt launch and diff behavior per platform.
- MVP desktop target: macOS, Windows, Linux.
- Web client should work in modern Chromium, Firefox, and Safari.
- Mobile web should be usable on iOS and Android.

### Observability

The server should expose:

- health endpoint,
- active session count,
- session state,
- last activity time,
- relay/gateway connection state,
- node capability and health summary,
- placement eligibility status,
- storage usage,
- recent errors.

### Data Retention

The product should define retention policies for:

- terminal event logs,
- terminal snapshots,
- session summaries,
- local event logs,
- memory entries,
- workspace instances,
- workspace leases,
- workspace checkpoints,
- app-managed Git checkpoint refs,
- node health history,
- placement decisions.

MVP can use conservative local retention defaults with manual cleanup.

## Excluded Product Scope

The product should not support:

- shared live agent sessions,
- multi-person terminal control,
- team permission models,
- organization administration,
- live session transfer between teammates.

## MVP Scope

### Included

- Native server daemon for Linux, macOS, and Windows.
- Electron desktop client.
- Web client from shared UI.
- Direct local/LAN connection.
- Persistent PTY sessions.
- Reconnect and replay terminal output.
- Basic terminal snapshot recovery.
- Workspace registration.
- Agent plugin registry.
- Built-in agent plugins for at least Codex and Claude Code.
- Custom command launch.
- Source-control plugin interface.
- Built-in Git plugin with status, diffs, and commit graph.
- Git-worktree isolation for concurrent sessions on the same repo.
- Changed-file list with click-to-diff behavior.
- Local Git initialization for folders without Git.
- Git-backed workspace checkpoints.
- Basic server/device auth.
- Early "Continue in VS Code" action.

### Deferred

- Native mobile apps.
- Full relay service production hardening.
- Full gateway route manager.
- Automatic personal pool placement.
- Repository sync orchestration across multiple nodes.
- Browser-based full editor replacement.
- User-facing Git write operations.
- Advanced memory UI.

## Suggested Milestones

### Milestone 1: Local Session Prototype

- Evaluate ready-to-go components for PTY handling, terminal rendering, storage, filesystem watching, and service installation.
- Server daemon creates PTY sessions through a platform abstraction.
- Prototype native PTY support for Unix PTYs and Windows ConPTY.
- Client lists and attaches to sessions.
- Session survives client disconnect.
- Terminal output is replayed after reconnect.

### Milestone 2: Durable Resume

- Add SQLite metadata store.
- Add terminal event persistence.
- Add terminal snapshots.
- Add session state transitions.

### Milestone 3: Plugin Foundation And Control Center UI

- Add three-panel Electron UI.
- Add session search/filter.
- Add workspace registration.
- Add agent plugin registry.
- Add built-in Codex and Claude Code plugins.

### Milestone 4: Source Control And Change Tracking

- Add source-control plugin interface.
- Add Git repository detection.
- Add Git worktree creation for isolated workspace instances.
- Add workspace leases to prevent accidental shared working tree writes.
- Add status and diff views.
- Add commit graph view.
- Add changed-file click-to-diff behavior.
- Add guided local Git initialization for folders without Git.
- Add Git-backed workspace checkpoints.
- Handle plain directories gracefully when users decline Git initialization.

### Milestone 5: Remote Connectivity

- Add authenticated remote server connections.
- Add reverse relay MVP.
- Add gateway or SSH-jump proof of concept.

### Milestone 6: Human-In-The-Loop

- Add VS Code continuation.
- Add extension or deep-link flow.
- Preserve session identity between client and editor.
- Open the editor in the isolated workspace instance for that session.

### Milestone 7: Memory

- Add agent profiles.
- Add session summaries.
- Add memory review/edit/delete.
- Add memory injection adapter for supported agents.
- Add memory export/import bundles.

### Milestone 8: Personal Pool Placement

- Add node capability and health inventory.
- Add route-aware placement preview.
- Add placement scoring based on agent support, repo locality, load, route quality, and user policy.
- Add automatic placement for new sessions.
- Add manual placement override and placement explanation.
- Add repository sync/clone workflow for nodes that do not already have the workspace.

## Open Decisions

- Use WebSocket, gRPC, or QUIC for the first streaming protocol?
- Store terminal output as raw PTY bytes, parsed terminal operations, or both?
- Use SQLite only, or add log segment files for large terminal histories?
- Should the first relay be self-hosted, managed, or both?
- Should same-owner multi-device attach use a single active input lock?
- How much user-facing Git write functionality belongs in the app?
- How should memory be injected for each supported agent?
- What is the initial agent plugin manifest and capability API?
- What is the initial source-control plugin manifest and panel API?
- How should checkpoint updates interact with `/new` for each agent plugin?
- How should app-managed Git checkpoint refs be named, retained, and garbage-collected?
- How should Git-backed checkpoints handle binary files, large files, generated files, and ignored paths?
- How should app-managed Git worktrees, branches, and refs be named, retained, and garbage-collected?
- When should workspace isolation fall back to a local clone, reflink/copy-on-write copy, full copy, or unsupported state?
- Which Rust PTY/process library gives the best native Windows ConPTY behavior without weakening Unix support?
- How should Windows service installation, upgrades, and log collection work?
- What should the default placement policy optimize for: repo locality, lowest latency, lowest load, local-first, or cost?
- How should repository sync and secret availability affect placement eligibility?
- How much placement explanation should be shown by default versus tucked into diagnostics?
- Which requirements are best satisfied by existing components versus product-owned implementation?
- What replacement boundary is required for each major dependency?
