# Agent Session Manager Architecture

## Purpose

This document describes a draft architecture for a cross-platform tool that lets users create, monitor, resume, and control long-running agent sessions on remote servers.

The system should support agent TUIs such as Claude Code, Codex, opencode, myclaw, and Hermes. A client connection must never own the lifetime of a session. Agent sessions live on the server and continue running when clients disconnect.

## Goals

- Run agent sessions inside remote workspaces.
- Preserve terminal output, input state, and scrollback across disconnects.
- Allow multiple client surfaces: web, mobile, Windows, macOS, Linux, Electron, and editor integrations.
- Support servers with and without public IP addresses.
- Support multi-hop routes such as `client -> gateway -> private server`.
- Support a personal pool of local, LAN, private, and cloud servers.
- Allow the app to choose the best server for a new agent session through transparent placement.
- Keep the server daemon lightweight and efficient.
- Run the server daemon natively on Linux, macOS, and Windows.
- Provide source-control visibility when a workspace is backed by Git or another VCS.
- Provide long-term memory across multiple sessions for the same logical agent.
- Keep agent sessions personal and private to their owner.
- Allow agent memory to be exported and imported for explicit transfer or duplication.
- Treat agent runtimes and source-control panels as plugins.
- Use Git-backed checkpoints for changed-file tracking, including folders that did not start as Git repositories.
- Isolate concurrent agents working on the same repository so they do not share one writable working tree.
- Prefer proven off-the-shelf components when they satisfy product, platform, security, and extensibility requirements.

## Product Non-Goals

- Building a full VS Code replacement.
- Supporting every possible terminal emulator feature perfectly on day one.
- Providing team administration, billing, or enterprise policy controls.
- Sharing, co-editing, or transferring live agent sessions between teammates.
- Replacing Git hosting services.
- Requiring all users to install a custom shell, custom tmux, or custom agent binary.
- Rebuilding mature infrastructure from scratch when an existing component cleanly satisfies the requirement.

## High-Level System

```text
+--------------------+      +--------------------+      +--------------------+
| Client             |      | Relay / Gateway    |      | Server Daemon      |
|                    |      | optional           |      |                    |
| - Electron desktop | <--> | - reverse tunnel   | <--> | - PTY manager      |
| - Web app          |      | - NAT traversal    |      | - session store    |
| - Mobile app       |      | - multi-hop route  |      | - workspace service|
| - VS Code bridge   |      |                    |      | - source control   |
+--------------------+      +--------------------+      +--------------------+
          |
          v
 +--------------------+
 | Personal Pool      |
 | Control            |
 |                    |
 | - node inventory   |
 | - placement        |
 | - route selection  |
 +--------------------+
                                                               |
                                                               v
                                                     +--------------------+
                                                     | Agent Process      |
                                                     |                    |
                                                     | codex / claude /   |
                                                     | opencode / etc.    |
                                                     +--------------------+
```

The central design rule is:

> The server owns sessions. Clients attach, detach, and resume views of those sessions.

## Product Model

This is a personal agent manager. A session belongs to one owner and is not a shared team object.

Collaboration should happen at the repository level:

- teammates maintain their own servers, workspaces, sessions, and agent profiles,
- source changes are shared through Git or another VCS,
- memory is transferred only through explicit export/import,
- live terminal sessions remain private.

This keeps the core system simpler, safer, and more predictable. It also avoids needing team presence, shared input arbitration, organization roles, or live-session transfer permissions.

In this document, owner means the personal identity that enrolled the server and devices. The same owner may attach from multiple devices, but the architecture does not support multiple people attaching to or controlling the same live session.

## Build-Vs-Buy Strategy

The system should not be built from the ground up where mature components already satisfy the requirements. The preferred approach is to compose strong existing building blocks behind product-owned interfaces.

Good dependency candidates include:

- terminal rendering,
- PTY abstraction,
- source-control operations,
- SQLite/storage layers,
- filesystem watching,
- native service installation helpers,
- relay/tunnel transport libraries,
- UI framework components,
- diff and text rendering components.

Adoption criteria:

- satisfies the core product requirement without weakening session durability,
- runs natively on Linux, macOS, and Windows where needed,
- does not require WSL for Windows server support,
- allows the product to keep ownership of session identity, terminal event persistence, reconnect semantics, workspace isolation, and memory policy,
- has a compatible license,
- has an acceptable security and maintenance posture,
- can be wrapped behind an internal interface so it can be replaced later,
- does not force shared team semantics or cloud-only operation,
- performs well enough for long-running daemon use.

Core product semantics should remain ours even when implementation is delegated. For example, a terminal or PTY library can handle low-level terminal mechanics, but the server still owns session metadata, event logs, snapshots, reconnect, input policy, and lifecycle decisions.

## Main Components

### Client Applications

The client is a control surface. It should be able to:

- List servers, workspaces, and agent sessions.
- Attach to an existing TUI session.
- Launch a new agent session.
- Send terminal input.
- Receive terminal output and state snapshots.
- Show workspace state, including source-control history and file changes.
- Open or continue a session in VS Code or another editor.

Recommended client stack:

- Desktop: Electron with shared web UI.
- Terminal rendering: `xterm.js`.
- Web: same UI core as Electron.
- Mobile: responsive web/PWA first, native wrapper later if needed.
- Editor integration: VS Code extension or URI/deep-link bridge.

### Server Daemon

The server daemon is the durable owner of agent sessions.

Recommended implementation language: Rust.

The daemon should be cross-platform by design and should run natively on Linux, macOS, and Windows. Windows support must not depend on WSL. A WSL workspace can be supported as an optional target later, but the Windows daemon itself should use native Windows APIs.

Rust remains the preferred daemon language because it can produce small native binaries across the target platforms while keeping async networking, process supervision, and storage in one codebase. Dependency choices should be screened for native Linux, macOS, and Windows support before adoption.

Responsibilities:

- Manage agent processes in PTYs.
- Persist session metadata.
- Persist terminal event logs and snapshots.
- Continue sessions while clients are disconnected.
- Enforce device authentication and workspace allowlists.
- Expose APIs for terminal streaming, session management, workspace inspection, source-control state, change tracking, and memory.
- Maintain outbound relay/gateway connections when inbound connections are unavailable.

Candidate Rust libraries:

- Async runtime: `tokio`.
- PTY handling: native Unix PTYs on Linux/macOS and Windows ConPTY on Windows, either directly or through a library such as `portable-pty`.
- Local database: SQLite via `sqlx`, `rusqlite`, or similar.
- Source-control operations: shell out to `git` for the built-in Git plugin, consider `gix` later.
- RPC/transport: WebSocket, gRPC, or QUIC.
- Serialization: protobuf, JSON, or MessagePack depending on protocol choices.

### Platform Abstraction

The server should isolate OS-specific behavior behind internal interfaces before the daemon grows many features.

Platform-specific areas:

- PTY backend: Unix PTY on Linux/macOS, ConPTY on Windows.
- Process supervision: process groups/signals on Unix, Job Objects and process handles on Windows.
- Service installation: systemd on Linux, launchd on macOS, Windows Service Control Manager on Windows.
- Filesystem watching: platform-native watchers through a cross-platform abstraction.
- Path handling: Windows drive letters, UNC paths, case sensitivity, symlinks, and line endings.
- Shell behavior: do not assume Bash; launch commands directly where possible and use configured shells only when needed.
- Permissions: Unix file modes and ownership differ from Windows ACLs.
- Local IPC: Unix sockets on Unix, named pipes or TCP loopback on Windows.

The plugin system should receive normalized platform capabilities and paths from the core daemon. Plugins may declare platform-specific launch behavior, but they should not require WSL for Windows operation.

### Agent Plugin System

An agent session is a supervised process running in a remote workspace. Agent-specific behavior should be provided through plugins so new agent-like TUIs can be added without changing the core server.

Built-in agent plugins should include:

- `codex`
- `claude`
- `opencode`
- `myclaw`
- `hermes`
- custom command

Agent plugins define:

- display name, icon, and supported platforms,
- binary detection and install hints,
- launch command, default arguments, and environment rules,
- PTY expectations such as preferred terminal size and startup behavior,
- optional session-boundary detection such as `/new`,
- optional memory injection strategy,
- optional health/readiness detection,
- optional actions exposed to the UI.

The core server owns process supervision, PTY persistence, authentication, reconnect, and storage. Plugins should not own session lifetime.

Plugin metadata should be represented separately from session metadata:

```text
plugin_id
plugin_kind: agent
name
version
entrypoint
capabilities
config_schema
enabled
```

Each session should be represented as durable metadata:

```text
session_id
agent_profile_id
agent_plugin_id
workspace_id
workspace_instance_id
working_directory
command
args
environment
status
created_at
updated_at
last_activity_at
terminal_size
terminal_snapshot_id
last_event_seq
exit_code
checkpoint_id
```

### Session Maintainer

The session maintainer is the core server subsystem. It replaces the need to make the client socket responsible for session lifetime.

Responsibilities:

- Spawn agent processes inside PTYs.
- Read PTY output continuously.
- Write user input into the PTY.
- Assign monotonic sequence numbers to terminal output events.
- Persist output events.
- Maintain a current terminal screen snapshot.
- Support reconnect by event replay or snapshot restore.
- Track process health and exit state.

The MVP should build this directly instead of modifying `tmux`. `tmux` can be supported later as an optional backend, but a product-owned session maintainer gives better control over reconnect behavior, same-owner multi-device attach, local event history, and session metadata.

### Terminal Event Store

The terminal event store preserves what happened while clients were disconnected.

At minimum, store:

```text
event_seq
session_id
timestamp
stream
bytes
terminal_size
```

The server also periodically stores terminal snapshots:

```text
snapshot_id
session_id
event_seq
rows
cols
screen_grid
scrollback_window
cursor_state
terminal_modes
```

Reconnect flow:

```text
1. Client connects with session_id and last_seen_event_seq.
2. Server checks whether all missed events are still available.
3. If yes, server replays missed events.
4. If no, server sends latest snapshot and events after that snapshot.
5. Client resumes sending input to the same PTY.
```

### Workspace Service

The workspace service models registered remote directories and the isolated workspace instances where agents actually run.

Responsibilities:

- Register and list workspaces.
- Validate workspace paths.
- Launch sessions in isolated workspace instances.
- Provide basic file metadata.
- Detect source-control providers.
- Store workspace-specific preferences and memory.
- Create and update Git-backed workspace checkpoints for change tracking.
- Prevent unrelated agent sessions from sharing the same writable working tree by default.
- Create, list, and clean up per-session workspace instances.

The system should not require a workspace to start with Git. A plain directory is valid, but the app should offer to initialize it as a local Git repository so changed-file tracking can use one universal implementation. No remote is required.

### Workspace Isolation Service

Multiple agents may work on the same repository on the same host. They must not run against the same writable local checkout by default. Branch selection alone is not enough because one physical working copy still shares the same files, Git index, lock files, generated artifacts, and in-progress edits.

The system should distinguish:

- source workspace: the registered repo or folder the user selected,
- workspace instance: the isolated working directory assigned to one agent session.

Default behavior:

- Each independent agent session gets a separate workspace instance by default.
- The selected source working tree is treated as the source of truth, not the default writable runtime directory.
- Attaching another client to the same session reuses that session's workspace instance.
- Starting a separate agent session never writes into another running session's workspace instance unless the user explicitly overrides the safety check.
- Directly running inside the selected source working tree should require an explicit user override.

MVP isolation should use Git worktrees for Git-backed workspaces:

- create a managed per-session worktree from the source repository,
- give each worktree its own working directory and index,
- create an app-managed branch, detached HEAD, or checkpoint ref as needed,
- keep worktree paths under a predictable managed location,
- surface merge/rebase/apply flows through Git rather than by sharing files directly.

If Git worktrees are not available or not suitable for a repository, fallback options can include a local clone, a reflink/copy-on-write directory copy, or a full copy. The fallback should be explicit because it may use more disk space. Future source-control plugins should provide their own isolation strategy or declare that the generic clone/copy fallback is required.

Isolation lifecycle:

- Create a workspace instance before launching the agent process.
- Store the instance path in the session metadata.
- Track whether the instance is active, stopped, archived, or ready for cleanup.
- Keep exited session instances until retention policy or user cleanup allows deletion.
- Never delete an instance with uncommitted or unexported changes without explicit user confirmation.

### Source Control Plugin System

Source-control behavior should also be plugin-based. Git should be the first plugin, but the architecture should allow SVN, Mercurial, Perforce, or custom source-control panels later.

Source-control plugins define:

- repository detection,
- status model,
- branch/tag/revision model where applicable,
- graph or history model where applicable,
- file diff provider,
- change list provider,
- workspace isolation strategy,
- optional write actions,
- UI panel contributions.

The right panel should consume a generic source-control shape rather than Git-specific objects. If a provider does not support a Git-like commit graph, it can expose the closest equivalent history view.

MVP Git plugin capabilities:

- Detect whether the workspace is a Git repository.
- Create and manage per-session Git worktrees for workspace isolation.
- Show current branch, remotes, and HEAD.
- Show working tree status.
- Show staged and unstaged file changes.
- Show commit graph with branches and tags.
- Show diffs for selected files and commits.
- Provide file-level changes when the user clicks any changed file.

Later capabilities:

- Commit, amend, rebase, branch, checkout, stash.
- Pull request integration.
- Multiple VCS providers.
- Submodule support.

### Git-Backed Checkpoint And Change Tracking Service

The system must show what changed in a file. To simplify implementation, changed-file tracking should be Git-backed in the MVP.

Every session has a workspace checkpoint:

- For an existing Git workspace, the Git plugin provides the authoritative status and diff behavior and can represent session checkpoints with Git objects or app-managed refs.
- For a plain directory, the server asks the user to initialize a local Git repository.
- After initialization, the server creates an app-managed baseline checkpoint when the folder is opened by the session.
- All file changes made after that checkpoint are tracked as session changes.
- When the agent crosses into a new conversation with `/new`, the active checkpoint is updated.

The default checkpoint for a session is the workspace state at session start. A checkpoint update should preserve prior session history but make future "changed since last edit/checkpoint" views compare against the new baseline.

For app-initialized repositories and session checkpoints in existing repositories, the server should avoid polluting the user's visible branch history. Prefer app-managed Git refs, checkpoint commits, or tree objects under a private namespace such as `refs/agent-session/checkpoints/*`. The implementation should not create user-facing commits unless the user explicitly asks.

Required capabilities:

- list changed files since the active checkpoint,
- show a file diff when the user clicks a changed file,
- distinguish created, modified, deleted, and renamed files where possible,
- update checkpoint on explicit user action,
- update checkpoint when an agent plugin reports a session boundary such as `/new`,
- avoid tracking files excluded by workspace ignore rules.

If a folder is not a Git repository and the user declines initialization, agent sessions can still run, but the changed-file panel should explain that Git initialization is required for full diff tracking.

### Memory Service

Long-term memory should be attached to a logical agent profile, not to a single terminal process.

```text
agent_profile_id
owner_id
workspace_id
agent_plugin_id
memory_policy
project_summary
decisions
preferences
recent_session_summaries
important_files
```

When a user starts a new session or the agent crosses a session boundary with `/new`, the system should preserve useful continuity by:

- summarizing the previous session,
- extracting durable decisions and preferences,
- linking file changes with source-control or Git-backed checkpoint state,
- making relevant memory available to the next session.

Memory injection options:

- MCP server exposed to compatible agents.
- Workspace memory files.
- Prompt prelude or wrapper script.
- Agent-specific config adapters.

Memory portability should be explicit:

- Export an agent profile's memory into a portable bundle.
- Import a memory bundle into another personal agent profile.
- Treat imported memory as a copy, not a live shared object.
- Include provenance metadata such as source workspace, export time, and optional notes.
- Keep exported bundles independent from the source session history unless the owner explicitly includes summaries.

### Relay And Gateway Service

Many servers will not have public IP addresses. The architecture should support direct, relay, and gateway modes.

#### Direct Mode

```text
client -> server daemon
```

Used when the server has a reachable address or the client is on the same network.

#### Reverse Relay Mode

```text
server daemon -> relay <- client
```

The server daemon establishes an outbound encrypted connection to a relay. The client connects to the same relay and attaches to the server through that path.

The relay should ideally route encrypted streams without being able to decrypt terminal content.

#### Gateway Mode

```text
client -> gateway daemon -> private server daemon
```

Used for multi-hop networks, including:

```text
local -> 10.0.0.5 -> 192.168.122.10
```

The gateway daemon can route traffic to private servers it can reach. A future route manager should choose the best available path automatically.

#### SSH Fallback

SSH should be considered as an MVP-friendly transport fallback:

```text
client -> ssh jump host -> server daemon socket
```

This gives technically advanced users a familiar path before a full relay/gateway service is complete.

### Personal Hybrid Agent Pool And Placement Service

Long term, the product should manage a personal hybrid pool of compute locations. The user should not need to decide which instance should run a new agent session unless they want to override the placement.

The pool can include:

- the local machine,
- LAN machines,
- private servers behind gateways,
- cloud VMs,
- Windows, macOS, and Linux hosts,
- machines reachable only through relay or reverse tunnel.

Each server daemon should report a capability and health summary:

```text
node_id
owner_id
hostname
platform
architecture
reachable_routes
agent_plugins_available
source_control_plugins_available
workspace_roots
cpu_load
memory_available
disk_available
battery_or_power_state
network_latency
last_heartbeat_at
labels
```

When the user launches a new agent, the placement service should evaluate:

- requested agent plugin and platform support,
- workspace or repository location,
- whether the repo already exists on a node,
- cost of cloning, fetching, or syncing the repo,
- availability of required binaries, credentials, and secrets,
- current CPU, memory, disk, and process load,
- network route quality through direct, gateway, relay, or SSH paths,
- operating system compatibility,
- user policies such as "prefer local", "avoid battery", or "cloud allowed",
- recent failures or unhealthy nodes.

Placement output:

```text
placement_decision_id
target_node_id
target_workspace_id
target_workspace_instance_id
route_id
reason_summary
score
fallback_nodes
```

The UX should be intent-based:

```text
Start Codex on repo X
```

Instead of:

```text
Pick host A, find path B, create worktree C, then start Codex
```

Users should still be able to inspect the selected node, see why it was chosen, pin a preferred node, or manually override placement.

This placement layer is long-term infrastructure, not required for the first local MVP. Early server registration and route metadata should still be designed so this scheduler can be added without replacing the session model.

## API Surface

Draft API groups:

```text
AuthService
  EnrollServer
  EnrollDevice
  IssueSessionToken
  RevokeDevice

ServerService
  ListServers
  GetServerStatus
  UpdateServerConfig

PoolService
  ListNodes
  GetNode
  UpdateNodeLabels
  GetNodeHealth
  ListRoutes

PlacementService
  PreviewPlacement
  CreatePlacementDecision
  ExplainPlacementDecision
  OverridePlacement

PluginService
  ListPlugins
  GetPlugin
  EnablePlugin
  DisablePlugin
  ValidatePluginConfig

WorkspaceService
  ListWorkspaces
  AddWorkspace
  GetWorkspace
  InspectWorkspace
  GetWorkspaceCheckpoint
  UpdateWorkspaceCheckpoint

WorkspaceIsolationService
  ListWorkspaceInstances
  CreateWorkspaceInstance
  GetWorkspaceInstance
  ReleaseWorkspaceInstance
  CleanupWorkspaceInstance
  GetWorkspaceLeaseStatus

SessionService
  ListSessions
  CreateSession
  AttachSession
  DetachSession
  ResizeSession
  SendInput
  StopSession
  RestartSession
  ArchiveSession

TerminalStream
  SubscribeOutput
  ReplayFrom
  GetSnapshot

SourceControlService
  DetectProvider
  GetRepositoryStatus
  GetHistoryGraph
  GetDiff
  GetBranches
  GetChangedFiles

ChangeTrackingService
  GetChangedFiles
  GetFileDiff
  CreateCheckpoint
  UpdateCheckpoint

MemoryService
  GetAgentProfile
  UpdateAgentMemory
  SummarizeSession
  BuildSessionContext
  ExportAgentMemory
  ImportAgentMemory
```

## Client UI Modes

### TUI Control Center Mode

Primary layout:

```text
+------------------+------------------------------+----------------------+
| Sessions         | Agent TUI                    | Changes / VCS        |
|                  |                              |                      |
| Active sessions  | xterm.js terminal             | Changed files        |
| Workspaces       | Input/output                  | Graph/history        |
| Agent profiles   | Attach/detach                 | Status/diff          |
+------------------+------------------------------+----------------------+
```

Expected behaviors:

- Users can switch between sessions without killing them.
- Disconnecting the client does not stop the agent.
- Reconnecting restores the same terminal state.
- Session list shows agent type, source workspace, isolated workspace instance, status, branch, and last activity.
- Source-control panel shows repository state when a provider is available.
- Changed files are clickable and open a diff.
- Plain directories are offered local Git initialization, then use Git-backed checkpoints to show changes since the active checkpoint.
- Concurrent sessions on the same repo show separate workspace instances or worktrees.

### Human-In-The-Loop Mode

First implementation:

- Provide a "Continue in VS Code" action.
- Launch VS Code with the same isolated workspace instance and session context.
- Use a VS Code extension or deep link to attach to the existing server session.

Later implementation:

- Build an integrated pair-programming GUI.
- Include editor tabs, file tree, diffs, terminal, and inline agent actions.
- Use Monaco for browser/Electron editing.

## Storage Model

Recommended MVP: SQLite on each server daemon.

Suggested tables:

```text
servers
nodes
node_capabilities
node_health
routes
placement_decisions
devices
plugins
workspaces
workspace_instances
workspace_leases
workspace_checkpoints
agent_profiles
sessions
terminal_events
terminal_snapshots
session_summaries
memory_entries
memory_exports
source_control_cache
file_change_index
local_events
```

Large terminal logs may eventually need rotation, compression, or object storage.

## Security Model

Baseline requirements:

- Device enrollment.
- Per-server identity.
- Strong transport encryption.
- Scoped access tokens.
- Workspace allowlist.
- Explicit user permission before launching arbitrary commands.
- Local event log for session creation, attach, input, stop, and deletion.
- Secret handling policy for terminal logs and memory summaries.

Relay security goals:

- Relay should not need plaintext terminal access.
- Server and client should mutually authenticate.
- Routes should be authorized per owner/server pair.

## MVP Implementation Sequence

1. Build Rust server daemon with cross-platform platform interfaces and local-only HTTP/WebSocket API.
2. Implement native PTY backends for Unix PTYs on Linux/macOS and ConPTY on Windows.
3. Implement session create/list/attach around the platform PTY abstraction.
4. Add terminal event log and reconnect replay.
5. Add terminal snapshots for durable resume.
6. Add native service installation paths for systemd, launchd, and Windows Service Control Manager.
7. Build Electron/web client with session list and xterm.js panel.
8. Add workspace registration.
9. Add agent plugin registry and built-in plugins for Codex, Claude Code, opencode, myclaw, and Hermes.
10. Add source-control plugin interface and Git plugin.
11. Add Git-worktree workspace isolation for concurrent sessions on the same repo.
12. Add changed-file list and click-to-diff UI.
13. Add local Git initialization and Git-backed checkpoints for folders without Git.
14. Add reverse relay for servers without public IPs.
15. Add VS Code continuation path.
16. Add memory service and session summaries.
17. Add memory export/import.
18. Add personal node pool inventory and placement preview.
19. Add automatic placement for new agent sessions.

## Open Questions

- Should the first network protocol be WebSocket for simplicity or QUIC/gRPC for long-term streaming ergonomics?
- Which ready-to-go components should be adopted for PTY handling, terminal rendering, storage, filesystem watching, service installation, relay/tunnel transport, and diff rendering?
- What internal interface boundary is required so each major dependency can be replaced if it stops fitting the product?
- Should terminal events be stored as raw bytes, parsed terminal operations, or both?
- What terminal scrollback retention policy is acceptable by default?
- Should the same owner be allowed to attach multiple devices to a session, and should only one device hold the active input lock?
- How should secrets in terminal output be detected and redacted?
- Should memory be stored per workspace, per agent profile, per owner, or a combination?
- Which agent tools support MCP or memory injection today?
- How much user-facing Git editing should be allowed from the first release versus read-only Git visibility?
- What is the stable plugin API boundary for agent runtimes?
- What is the stable plugin API boundary for source-control providers?
- How should app-managed Git checkpoint refs be named, retained, and garbage-collected?
- How should app-managed Git worktrees, branches, and refs be named, retained, and garbage-collected?
- When should workspace isolation fall back to a local clone, reflink/copy-on-write copy, full copy, or unsupported state?
- What placement policy should be the default: prefer local, prefer fastest, prefer least loaded, or prefer repo locality?
- How should the scheduler balance transparent automation with user override and debuggability?
- How should repository sync, secrets availability, and agent plugin availability affect placement scores?
