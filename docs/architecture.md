# Agent Session Manager Architecture

## Purpose

This document defines the target architecture for a personal, cross-platform tool that manages long-running coding agent sessions on remote machines.

The core promise:

```text
start agent -> disconnect -> agent keeps running -> reconnect -> resume the same live session -> inspect code changes
```

Supported agent targets include Codex, Claude Code, opencode, myclaw, Hermes, and custom terminal commands.

## Design Principles

- Server-side session ownership: clients attach to sessions, never own them.
- No silent restart: a terminated agent is recorded as exited or failed, not relaunched as the same session.
- Server-side terminal state: fresh clients resume from a backend-maintained terminal screen model.
- Sidecar-owned PTY: the native session backend holds live PTY handles outside the daemon process.
- Personal scope: sessions are private to one owner; collaboration happens through the repository.
- Replaceable execution: agent runtimes, session backends, and source-control panels use plugin boundaries.
- Native portability: the daemon runs natively on Linux, macOS, and Windows; Windows support does not depend on WSL.
- Workspace isolation: independent agents never share one writable working tree by default.
- Workspace setup: isolated instances receive explicit setup hooks for secrets, caches, dependencies, and repo-specific bootstrap.
- Git-backed change tracking: MVP diff and checkpoint behavior is unified through Git.
- Component leverage: mature dependencies are used behind product-owned interfaces.
- Transparent operations: long-term placement chooses a suitable personal machine without forcing host selection into the launch flow.

## Product Scope

### Personal Model

A session belongs to one owner. The same owner can attach from multiple devices. The architecture does not support multi-person live terminal control, team roles, organization policy, or live session transfer.

Repository collaboration remains outside the session layer:

- each teammate runs private agents,
- source changes move through Git or another VCS,
- memory export/import creates a copy,
- imported memory is private to the importing owner.

### Baseline And Extension Points

| Area | MVP Baseline | Extension Point |
| --- | --- | --- |
| Desktop client | Electron shared web UI | Native mobile, richer editor UI |
| Terminal renderer | xterm.js | Addons and renderer tuning |
| Session backend | Single out-of-process holder (`asmux`) owning all PTYs; VT emulator in the daemon | tmux backend, future platform backends |
| Agent runtime | Built-in Codex, Claude Code, opencode, custom command | myclaw, Hermes, third-party plugins |
| Source control | Git plugin | SVN, Mercurial, Perforce, custom panels |
| Workspace isolation | Git worktrees | Clone, reflink, full-copy, provider-specific isolation |
| Change tracking | Git checkpoints | Provider-specific checkpoint models |
| Connectivity | Direct local/LAN and SSH tunnel | Relay, gateway, route manager |
| Placement | Manual server/workspace selection | Personal hybrid pool scheduler |
| Attention | Activity and needs-attention signals | Agent-specific prompt/block detection |
| Memory | Agent profile data model | Review UI, import/export, injection adapters |

## High-Level System

```text
+--------------------+      +--------------------+      +--------------------+
| Client             |      | Relay / Gateway    |      | Server Daemon      |
|                    |      | extension          |      |                    |
| - Electron desktop | <--> | - reverse tunnel   | <--> | - session router   |
| - Web app          |      | - NAT traversal    |      | - session store    |
| - VS Code bridge   |      | - multi-hop route  |      | - workspace svc    |
| - Mobile web       |      |                    |      | - source control   |
+--------------------+      +--------------------+      +---------+----------+
          |                                                     |
          v                                                     | local IPC
+--------------------+                                         | UDS / pipe
| Personal Pool      |                                         v
| Control            |                               +--------------------+
|                    |                               | Session Backend    |
| - node inventory   |                               | Plugin             |
| - route selection  |                               |                    |
| - placement        |                               | native PTY / tmux  |
+--------------------+                               +---------+----------+
                                                                |
                                                                v
                                                      +--------------------+
                                                      | Agent Process      |
                                                      |                    |
                                                      | codex / claude /   |
                                                      | opencode / etc.    |
                                                      +--------------------+
```

The daemon is the control plane. The session backend is the live terminal data plane. Clients render and control views.

## Client Architecture

The client lists servers, workspaces, sessions, terminals, diffs, and source-control state. It also opens the isolated workspace instance in VS Code.

### Frontend Stack

| Layer | Decision | Use |
| --- | --- | --- |
| App framework | React 19 + TypeScript + Vite | Shared Electron and web UI |
| Desktop shell | Electron | macOS, Windows, Linux desktop client |
| Terminal | xterm.js | Agent TUI rendering, terminal input, resize, search addons |
| Local UI state | Zustand | Active session, selected file, layout state, transient connection state |
| Server state | TanStack Query | Sessions, workspaces, Git status, diffs, health, cache invalidation |
| Components | shadcn/ui + Tailwind | Owned accessible UI primitives and styling |
| Layout | Dockview | IDE-like tabs, docking, side panels, serialized layouts |
| Code view | CodeMirror | Read-only code, diff views, lightweight editing surfaces |
| Markdown | Marked | Fast markdown parsing for agent output and docs |
| Highlighting | Shiki | VS Code-grade code highlighting |
| Diagrams | Mermaid | Markdown diagrams |
| Math | KaTeX | Markdown math rendering |
| Sanitization | DOMPurify | Sanitized HTML boundary for markdown and rich output |
| Icons | lucide-react | Toolbars, actions, status indicators |

Frontend ownership rules:

- xterm.js renders terminal bytes; the server owns terminal history and resume semantics.
- Zustand stores local UI state only; TanStack Query owns server-derived state.
- Markdown, Mermaid, and KaTeX render repo files, agent transcripts exposed by agent plugins, session summary records, and memory summary records.
- Markdown, Mermaid, and KaTeX output is treated as untrusted content and rendered through a sanitization boundary.
- Dockview layout state is persisted per user/device and does not affect server session state.
- Electron runs with `contextIsolation: true`, `nodeIntegration: false`, a strict preload API, and a restrictive content security policy.
- Terminal links, clipboard writes, and window-title escape sequences are gated by terminal security policy.
- Persistent device enrollment targets Electron for MVP. The browser build uses session-only enrollment.

### UI Modes

TUI control center:

```text
+------------------+------------------------------+----------------------+
| Sessions         | Agent TUI                    | Changes / VCS        |
| Workspaces       | xterm.js terminal            | Changed files        |
| Agent profiles   | Input/output                 | Graph/status/diff    |
+------------------+------------------------------+----------------------+
```

Human-in-the-loop mode:

- "Continue in VS Code" opens the session's isolated workspace instance.
- The action attaches editor context to the existing session.
- The original source checkout is not opened when an isolated instance exists.

## Server Daemon

Recommended implementation language: Rust.

Responsibilities:

- expose authenticated APIs for clients,
- coordinate session backends,
- persist session metadata, terminal events, and snapshots,
- enforce workspace allowlists,
- coordinate workspace instances and source-control providers,
- expose health, logs, and diagnostics,
- maintain relay or gateway connections in extended deployments.

Candidate implementation choices:

- async runtime: `tokio`,
- database: SQLite through `rusqlite` behind a batched writer task with WAL mode,
- built-in native backend: a single out-of-process holder (`asmux`) owning all sessions' Unix PTYs on Linux/macOS and ConPTY handles on Windows (see docs/durable-sessions.md),
- headless terminal emulator: `vt100` (or equivalent) behind an internal terminal-state interface **in the daemon**, not the holder — a bad escape sequence must never destabilise the process holding every PTY,
- PTY library candidate: `portable-pty` or native wrappers inside the holder,
- Git operations: `git` CLI for MVP, `gix` evaluation behind the Git provider interface,
- transport: HTTP for control APIs and WebSocket for terminal output and input streams,
- serialization: JSON for control APIs, binary frames or protobuf-compatible shapes for high-volume streams.

## Platform Abstraction

The daemon hides OS differences behind internal interfaces:

- PTY and ConPTY operations,
- process supervision and process-tree termination,
- user-scoped background installation through systemd user units, launchd LaunchAgents, and Windows per-user startup mechanisms,
- filesystem watching,
- path normalization,
- file permissions and ACLs,
- local IPC.

Local IPC options:

- Unix domain sockets on Linux and macOS,
- Windows AF_UNIX where reliable,
- Windows named pipes as the Windows-native fallback,
- TCP loopback only for development or constrained environments.

Service scope:

- the default install runs as the enrolled OS user,
- Linux uses `systemd --user`; `loginctl enable-linger` is required when sessions need to survive logout,
- macOS uses a LaunchAgent,
- Windows uses a per-user scheduled task or equivalent user-session startup mechanism with restart-on-failure recovery,
- classic system services remain an advanced deployment mode.

User-scoped services preserve the user's home directory, shell environment, PATH, agent credentials, and browser-based login flows for tools such as Codex and Claude Code.

## Plugin Architecture

The plugin registry has three plugin kinds:

```text
agent
session_backend
source_control
```

Shared plugin metadata:

```text
plugin_id
plugin_kind
name
version
supported_platforms
capabilities
config_schema
enabled
```

### Agent Plugins

Agent plugins define:

- display metadata,
- binary detection and install hints,
- launch command and arguments,
- environment rules,
- terminal size preferences,
- session-boundary detection such as `/new`,
- transcript location and parser behavior when the agent writes structured transcripts,
- memory injection adapter,
- readiness and health detection,
- UI actions.

Built-in agent plugins:

```text
codex
claude
custom_command
```

Planned built-ins:

```text
opencode
myclaw
hermes
```

Agent plugins do not own session lifetime. They provide launch and interpretation behavior to the daemon and selected session backend.

### Session Backend Plugins

> **Superseded in part by asmux** (see docs/durable-sessions.md and
> docs/asmux-protocol.md): the native backend is a **single out-of-process holder
> for all sessions** (`asmux`), not one sidecar per session; the **VT emulator
> lives in the daemon**, not the holder; and attach is **single-client with
> takeover**. The per-session-sidecar prose below predates that decision; where it
> conflicts with the asmux docs, the asmux docs are authoritative.

Session backend plugins own live terminal mechanics under daemon policy. The native backend uses one out-of-process sidecar per live session. Each sidecar holds that session's PTY master or ConPTY handle, child process handle, terminal emulator state, and backend-local output spool.

Backend responsibilities:

- create or attach backend sessions,
- spawn agent processes or attach external terminal sessions,
- stream terminal output,
- accept terminal input,
- resize terminals,
- maintain a headless terminal emulator screen model,
- export terminal screen snapshots,
- report health and exit status,
- preserve backend-local continuity while clients are disconnected,
- provide durable event drain or a backend-local append-only output spool.

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

No-gap rules:

- client disconnect never stops a backend session,
- daemon restart reattaches to live per-session sidecars,
- daemon upgrade leaves existing sidecars and live PTYs running,
- backend output generated during daemon reconnect is drained into the terminal event store,
- exited or failed processes are recorded as terminal states,
- rerunning an agent creates an explicit follow-up session.

Native sidecar lifetime:

- one sidecar per live session is the default MVP model,
- daemon restart leaves per-session sidecars and live PTYs running,
- daemon upgrade leaves existing sidecars running on their original binary until their sessions exit,
- new sessions use the currently installed sidecar binary,
- systemd user units keep sidecar processes outside daemon restart kill paths,
- Windows user-scoped startup avoids placing live session processes in a daemon job object that is torn down on daemon restart,
- sidecar crash marks its owned session failed,
- sidecar maintenance that terminates a live session is explicit and session-scoped.

Sidecar rendezvous:

- sidecar IPC sockets live in a well-known per-user runtime directory,
- socket names are derived from session IDs,
- session records persist the expected sidecar socket path and sidecar process metadata,
- daemon startup scans the runtime directory before reconciling session records,
- live sidecars without matching session records become orphaned sidecar records,
- orphaned sidecars are surfaced in diagnostics and the session list,
- the owner can adopt an orphan as a recovered session or terminate it.

Resume model:

- the sidecar runs a headless VT emulator for every live session,
- live attach requests fetch a fresh emulator snapshot from the session sidecar,
- terminal snapshots are exported as a synthesized ANSI repaint stream with cursor and mode metadata,
- fresh clients resume by writing the repaint stream through the same xterm.js path as live output,
- event replay fills only the tail after the snapshot,
- persisted snapshots support exited-session history, inspection, search, diagnostics, and fallback recovery,
- replay from an arbitrary raw byte offset is not a resume mechanism,
- resize repaint requests are available as a pragmatic TUI refresh trigger.

The native PTY sidecar is the MVP backend. A tmux backend is a normal backend plugin that maps product sessions to tmux sessions, windows, or panes and uses tmux's server-side screen state. Windows-native operation does not depend on tmux or WSL.

### Source-Control Plugins

Source-control plugins define:

- repository detection,
- status model,
- branch, tag, or revision model,
- graph or history model,
- changed-file provider,
- file diff provider,
- workspace isolation strategy,
- optional write actions,
- UI panel contributions.

The right-side source-control panel consumes provider-neutral shapes. Providers without a Git-like commit graph expose their closest history model.

## Session Model

Session metadata:

```text
session_id
agent_profile_id
agent_plugin_id
session_backend_id
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
sidecar_socket_path
sidecar_process_id
attention_state
attention_reason
attention_updated_at
```

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

Reconnect flow:

```text
1. Client attaches with session_id and last_seen_event_seq.
2. Daemon validates auth and same-owner attach policy.
3. Daemon requests a fresh emulator snapshot from the live session sidecar.
4. Daemon sends the snapshot as a synthesized ANSI repaint stream.
5. Daemon replays retained events after the snapshot sequence.
6. Daemon requests a backend repaint when the agent TUI supports resize-triggered refresh.
7. Client resumes input against the same backend session.
```

Same-owner single-active-session policy (takeover):

```text
attach: at most one attached client per session
takeover: a new device attaching supersedes and forcibly detaches the prior client
evicted client: receives a taken-over signal; may re-attach later and resume
resize: terminal size follows the single active client
repaint: backend repaint is requested after resize
```

Concurrent multi-client input is superseded by takeover; see
docs/asmux-protocol.md → Attach model.

Daemon restart flow:

```text
1. Daemon loads running and detached session records.
2. Daemon scans the per-user runtime directory for sidecar sockets.
3. Daemon reconnects to matching live session sidecars over local IPC.
4. Sidecars report live handles and event cursors.
5. Daemon drains backend output spools.
6. Missing backend sessions become exited or failed records.
7. Live sidecars without session records become orphaned sidecar records.
```

## Terminal Event Store

Terminal events are append-only and sequence-numbered:

```text
event_seq
session_id
backend_event_seq
timestamp
stream
bytes
terminal_size
```

Terminal snapshots are the authoritative resume source for fresh clients:

```text
snapshot_id
session_id
event_seq
rows
cols
ansi_repaint_bytes
cursor_state_metadata
terminal_mode_metadata
scrollback_window_metadata
alternate_screen_metadata
title
```

Live attach uses a fresh sidecar snapshot. Persisted snapshots are retained for exited sessions, search and inspection, diagnostics, and fallback recovery. Event replay fills gaps after the snapshot sequence. Full replay from session start remains a diagnostic and recovery tool, not the normal attach path.

Backpressure policy:

- the sidecar reads PTY output continuously,
- event writes are batched through bounded buffers into SQLite WAL or append-only segments,
- writer stalls apply backpressure to the PTY reader after the bounded buffer is full,
- the default policy stalls the agent rather than dropping output,
- dropped output is allowed only for unrecoverable storage failure or an explicit per-session never-stall policy,
- every dropped range is represented with an explicit gap marker and health event,
- clients surface gap markers in terminal history.

Terminal escape security:

- OSC 52 clipboard writes are disabled by default,
- OSC 8 hyperlinks require explicit link handling policy,
- title-setting sequences do not control trusted app chrome,
- terminal output is treated as untrusted content,
- dangerous control-sequence policy is enforced at capture, replay, or xterm configuration boundaries,
- the terminal parser and snapshot exporter receive fuzz testing with hostile escape-sequence input.

Older logs can be compacted, compressed, and retained according to server or workspace policy.

## Workspace And Source Control

Workspace concepts:

- source workspace: the registered repository or folder selected by the user,
- workspace instance: the isolated working directory assigned to one independent agent session.

Default isolation behavior:

- each independent session gets a separate workspace instance,
- multiple clients attached to the same session share the same instance,
- a new session does not write into another running session's instance,
- direct source-checkout execution requires explicit override,
- dirty instances are retained until merged, exported, discarded, or explicitly cleaned up.

MVP isolation uses Git worktrees for Git-backed workspaces:

- managed per-session worktree,
- separate working directory and Git index,
- app-managed branch, detached HEAD, or ref,
- predictable storage path,
- workspace setup hook after worktree creation,
- configured copy rules for files such as `.env`,
- configured symlink rules for caches such as dependency stores,
- configured bootstrap command for dependencies or generated assets,
- merge, rebase, and apply flows handled through Git.

Worktree setup metadata:

```text
workspace_setup_id
workspace_id
copy_globs
symlink_globs
bootstrap_command
bootstrap_env
secret_file_policy
cache_policy
last_run_status
```

Known Git worktree edge cases:

- submodules,
- Git LFS,
- custom `core.hooksPath`,
- nested repositories,
- generated files,
- ignored required local files.

Fallback isolation strategies are provider-specific. The generic fallback order is local clone, reflink/copy-on-write copy, then full copy.

## Change Tracking

MVP change tracking is Git-backed.

Git workspace behavior:

- existing Git repositories use the Git plugin for status and diffs,
- plain folders go through guided local `git init`,
- no remote is required,
- app-managed checkpoints avoid user-facing commits,
- checkpoint refs use a private namespace such as `refs/agent-session/checkpoints/*`,
- the explicit "New segment" UI action sends the agent command and advances the active checkpoint,
- plugin input sniffing is an optional session-boundary capability,
- dirty checkpoint capture uses temporary-index plumbing to write a tree and update an app-managed ref,
- checkpoint policy defines whether untracked files are captured,
- changed files are clickable and open a diff.

Folders that remain non-Git can run agents. Full changed-file diff tracking requires Git initialization.

## Session Summary Records

The MVP writes a structural session summary record on session exit and explicit segment boundaries. The record is deterministic metadata, not an LLM-generated narrative.

```text
session_summary_id
session_id
agent_plugin_id
workspace_id
workspace_instance_id
started_at
ended_at
duration_ms
exit_status
final_checkpoint_id
changed_file_count
created_file_count
modified_file_count
deleted_file_count
renamed_file_count
terminal_event_range
```

Summary records preserve lifecycle context for history, diagnostics, rich-output rendering, and future memory enrichment. Long-term memory can attach project decisions, preferences, and narrative summaries to these records later.

## Memory

Long-term memory belongs to an agent profile, not to a terminal process.

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

Memory behavior:

- session summaries preserve continuity across `/new` and follow-up sessions,
- durable decisions and preferences are scoped to owner, workspace, and agent profile,
- memory can be reviewed, edited, deleted, exported, and imported,
- imported memory is a private copy with provenance metadata,
- supported injection paths include MCP, workspace memory files, prompt prelude, and agent-specific config.

## Connectivity

Connectivity answers exactly one question per node: **at what URL can this
client reach that daemon?** Sessions, terminals, and aggregation are identical
across all modes.

This section is the contract; the implementation plan (wire details, crate
layout, milestones R1–R5) lives in
[`connectivity-execution-plan.md`](connectivity-execution-plan.md).

### Contract

Invariants that hold in every mode:

- Every mode terminates in a `baseUrl` the client can reach. A connected node
  is one more daemon connection entry; the client aggregates all of them
  identically. Direct, tunnelled, and relayed nodes are indistinguishable
  above the URL.
- The client speaks plain HTTP(S) and WebSocket in every mode. Routing
  complexity is encoded in the URL shape, never in client networking code.
- Network roles live in the server daemon and the relay only. The session
  backend holder (`asmux`) is local IPC and is never exposed to any network.
- Connections are initiated from the less-reachable side toward the
  more-reachable side: clients dial daemons; NAT'd daemons dial out and
  register.

### Modes

```text
direct:       client -> server daemon                                  (MVP)
ssh tunnel:   client -> ssh -L local_port:daemon_port -> daemon        (MVP)
ssh via hop:  client -> ssh -J hop ... -> daemon                       (MVP, recipe)
relay:        daemon --register--> relay <--connect-- client           (Phase 2)
gateway:      gateway daemon --register--> relay <--connect-- client
                     \--forward inward--> private daemon               (Phase 3)
```

Direct local/LAN and SSH port forwarding are MVP modes. Relay and gateway
modes preserve the same authenticated daemon API and session model.

### Topology Cases

The reference topologies, in deployment order:

```text
case 1:  A -> B -> C        C reachable only from B (VM behind B's NAT,
                            host on B's private net)         -> ssh recipes
case 2:  A -> B <- C        B public; C NAT'd, dials out     -> relay
case 3:  A -> B <- C -> D   D has no internet egress; gateway C is its
                            only entrypoint (e.g. company-internal
                            server behind a bastion)          -> gateway
```

All cases reduce to two composable primitives:

- **forward inward** — a reachable intermediate carries connections onward to
  a node it can reach (`ssh -L`/`-J` today; gateway stream splice in Phase 3).
- **register outward** — an unreachable node dials out to a reachable
  rendezvous and parks a persistent connection there (relay registration).

Any node may be both a leaf (registered upstream) and a hub (forwarding
downstream), so the primitives compose to arbitrary depth. Depth is invisible
to the client. A NAT'd-but-egress-capable node registers with the relay
directly, however many NAT layers it sits behind — the registry stays flat;
gateway forwarding is only for nodes with no egress at all.

**Decision (2026-07-04):** ASM owns the relay/gateway path (`asm-relay` +
daemon gateway mode) rather than delegating to a third-party overlay mesh
(Tailscale/headscale, Nebula). Target deployments include entrypoints where
overlay software cannot be installed but the ASM daemon can. SSH recipes
remain the supported zero-infrastructure fallback.

### Phase 1: SSH Recipes (works today)

```text
# case 1 — C reachable only via B
ssh -J user@B user@C -L 4602:127.0.0.1:4600 -N   # lands on C loopback: no token
ssh user@B -L 4602:<C_ip>:4600 -N                # no sshd on C; C sees B: token

# case 3 without a relay — egress-less D behind reachable C
ssh user@C -L 4603:<D_ip>:4600 -N                # through public C: token
# C itself NAT'd: on C keep `autossh -R 4600:<D_ip>:4600 user@B` alive,
# then on A: ssh user@B -L 4603:127.0.0.1:4600 -N
```

The forwarded local port is added in the client as an ordinary daemon URL.

### Phase 2: Relay (`asm-relay`)

- `asm-relay` is a standalone, hardened, internet-facing binary on a public
  host. Registry and router only: no sessions, no workspaces, no asmux.
- Registration: a daemon dials `wss://relay/register` outbound (443-friendly,
  survives restrictive egress), authenticates with a relay access key, and
  holds the connection open; reconnect with backoff. Registration carries the
  node id, label, and advertised downstreams.
- Node identity: each daemon generates a stable `node_id` on first run and
  persists it.
- Stream model: one physical WSS connection per registered node, carrying
  multiplexed logical streams. Each inbound client request or WebSocket is one
  logical stream.
- Opaque payload: the relay routes by path prefix — `/n/<node_id>/...` (HTTP
  and WS upgrade alike) is spliced down the target node's registration
  connection as raw bytes. The relay never parses the daemon API, so new
  daemon endpoints need no relay changes and the client keeps speaking the
  daemon protocol end to end.
- Discovery: `GET /nodes` (relay access key required) returns registered
  nodes: `node_id`, label, kind (leaf | gateway), `via` (gateway `node_id`
  for advertised downstreams), liveness.

### Phase 3: Gateway Mode

- A daemon configured with `downstreams` registers itself with the relay and
  advertises each downstream target it can reach on its private network.
- Logical streams addressed to a downstream are spliced by the gateway onward
  to `host:port` on its network. Downstream daemons need no internet egress
  and no software beyond the ASM daemon — they are reached inward only.
- Gateways compose: a downstream may itself be a gateway. Neither the client
  nor the relay routing model grows with depth beyond the `via` attribution.

### Authentication

Two independent credentials, one per layer:

- **Relay access key** — gates use of the relay itself: registration,
  discovery, and routing. Scoped per owner; a relay is not an open proxy.
- **Node enrollment / device token** — the existing daemon enrollment flow,
  performed through the relay URL and validated end to end by the target
  daemon. Neither relay nor gateway can mint or bypass node access.
- **Relayed traffic is never loopback-trusted.** Tunnel streams delivered to a
  daemon (including a gateway's own daemon) count as remote; a device token is
  always required on relayed paths. Blank-token loopback trust remains
  exclusive to genuine loopback connections (local clients and real SSH
  tunnels).

### Client Connection Establishment

The client gains one new entity: a **relay**, which contributes discovered
nodes rather than sessions. Client networking is unchanged — a relayed node is
an ordinary daemon connection whose `baseUrl` happens to route through the
relay.

```text
1. add relay (once)      relay URL + relay access key
                         -> client calls GET /nodes, lists discovered nodes
                            grouped under the relay with liveness
2. connect node (once    supply that node's enrollment token
   per node)             -> client enrolls against https://relay/n/<node_id>
                            (validated end to end by the node), stores the
                            device token
3. steady state          node is a normal daemon connection:
                           { baseUrl: "https://relay/n/<node_id>",
                             token: <device_token>, via: <relay_id> }
                         polled and aggregated identically to direct nodes;
                         terminal attach upgrades wss://relay/n/<node_id>/...
```

Failure states the client distinguishes:

- relay unreachable — every node under that relay is marked unreachable as a
  group; cached session data is retained,
- node offline — registration dropped, or the gateway reports its downstream
  unreachable; only that node is marked, distinct from relay-down.

### Relay Security Goals

- the relay routes opaque streams and does not parse the daemon API,
- client and daemon authenticate end to end; relay and gateway cannot mint
  access,
- routes are authorized per owner and node,
- splice points (relay, gateway) can observe stream plaintext until nested
  end-to-end TLS (client-to-daemon, carried through the tunnel) lands; nested
  TLS is the hardening item that also closes the "no TLS off loopback" gap in
  docs/security-followups.md for relayed paths.

## Attention Signals

The control center tracks whether a session needs user attention.

Signal sources:

- new output after idle,
- terminal bell,
- plugin-provided prompt or approval patterns,
- explicit backend health events,
- agent exit or failure,
- long-running inactivity after an active burst.

Attention state:

```text
attention_state: none | activity | likely_blocked | approval_needed | failed
attention_reason
attention_updated_at
```

Attention detection runs in the daemon over terminal events, recent decoded output text, bell events, backend health events, and process lifecycle events. Agent plugins contribute prompt and approval patterns against recent output text. MVP attention plugins do not require direct access to sidecar terminal screen state.

## Personal Pool Placement

The long-term product manages a personal hybrid pool of compute locations:

- local machine,
- LAN machines,
- private servers,
- cloud VMs,
- hosts reachable through relay, gateway, or SSH.

Node health and capability data:

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

Placement evaluates agent support, repo locality, sync cost, credentials, load, disk, route quality, OS compatibility, user policy, and recent failures.

Cross-machine repository identity:

```text
repo_identity_id
canonical_remote_url
remote_url_fingerprint
provider
default_branch
last_seen_commit
```

Repo identity lets placement reason about the same repository across multiple personal machines.

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

PluginService
  ListPlugins
  GetPlugin
  EnablePlugin
  DisablePlugin
  ValidatePluginConfig

SessionBackendService
  ListSessionBackends
  GetSessionBackend
  ValidateSessionBackendConfig
  GetBackendHealth
  ReconnectBackend
  DrainBackendEvents
  ListOrphanedSidecars
  AdoptOrphanedSidecar
  TerminateOrphanedSidecar

SessionService
  ListSessions
  CreateSession
  AttachSession
  DetachSession
  ResizeSession
  StopSession
  CreateFollowupSession
  CreateSessionSegment
  AcknowledgeAttention
  ArchiveSession

TerminalStream
  SubscribeOutput
  SendInput
  ReplayFrom
  GetSnapshot

WorkspaceService
  ListWorkspaces
  AddWorkspace
  GetWorkspace
  InspectWorkspace
  GetWorkspaceSetup
  UpdateWorkspaceSetup
  RunWorkspaceSetup
  GetWorkspaceCheckpoint
  UpdateWorkspaceCheckpoint

WorkspaceIsolationService
  ListWorkspaceInstances
  CreateWorkspaceInstance
  GetWorkspaceInstance
  ReleaseWorkspaceInstance
  CleanupWorkspaceInstance
  GetWorkspaceLeaseStatus

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
```

## Storage Model

MVP storage uses SQLite on each daemon host.

Suggested tables:

```text
servers
devices
plugins
session_backends
session_backend_instances
orphaned_sidecars
workspaces
workspace_instances
workspace_leases
workspace_checkpoints
agent_profiles
sessions
terminal_events
terminal_backend_spool
terminal_snapshots
session_summaries
memory_entries
memory_exports
source_control_cache
file_change_index
local_events
attention_events
workspace_setup_rules
nodes
node_capabilities
node_health
routes
placement_decisions
repo_identities
```

## Security Model

Baseline security:

- device enrollment,
- per-server identity,
- encrypted transport,
- scoped access tokens,
- workspace allowlist,
- explicit approval before arbitrary custom commands,
- lifecycle audit events for session create, attach, input, stop, and delete,
- terminal log and memory secret-handling policy,
- per-device keys for enrollment,
- relayed traffic never inherits loopback trust,
- API and protocol version negotiation.

MVP storage disclosure:

- terminal logs can contain secrets printed by agents or commands,
- local logs are stored on the daemon host,
- retention defaults are conservative,
- encryption-at-rest and production-grade redaction are separate hardening items.

## Open Decisions

- Initial streaming protocol after MVP WebSocket: gRPC or QUIC.
- Terminal event representation: raw bytes, parsed operations, or both.
- Terminal log storage: SQLite rows, append-only log segments, or hybrid.
- Persisted terminal snapshot cadence for inspection, search, and fallback recovery.
- Production tmux backend scope after MVP validation spike.
- Backend output spool storage: SQLite, log files, or backend-specific storage.
- Secret redaction depth for terminal logs and memory summaries.
- API and protocol compatibility across client/daemon version skew.
- Cross-machine repository identity and clone/sync metadata.
- Git checkpoint and worktree naming, retention, and garbage collection.
- Binary, large-file, generated-file, and ignored-path diff behavior.
- Placement policy defaults across locality, latency, load, cost, and power state.
- Relay stream-multiplexing framing: default is yamux over the registration WSS, with frp-style dial-out-per-stream as the documented fallback (see connectivity-execution-plan.md → Locked decisions).
- Pairing-code enrollment brokered through the relay to replace manual token paste.
