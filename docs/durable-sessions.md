# Durable Sessions via an Out-of-Process Session Holder ("asmux")

Status: **design, refining before code.** No implementation yet. Adapts the
"acmux" design (from the agent-conductor project) to this codebase.

Locked decisions: sidecar crate/binary named **`asmux`**; wire encoding is
**FlatBuffers** (schema frozen once shipped). The frozen contract lives in
[`asmux-protocol.md`](asmux-protocol.md).

## The two questions this answers

1. **Can we persist metadata to pick up where we left off?** Yes — but the
   metadata alone isn't enough; the *live PTY process* must also survive. The
   answer is a two-tier model: SQLite (cold, survives everything) plus a
   long-lived sidecar (hot, holds the live PTYs across daemon restarts).
2. **Can we separate the daemon from a tmux-like process that never stops?**
   Yes. A dedicated `asmux` sidecar owns the PTY master fds and stays up while
   `asm-daemon` restarts (crash / upgrade / hash rotation). This is exactly the
   "out-of-process sidecar" `architecture.md` already specifies.

## Why the current backend can't do it

`backend/native.rs` runs each PTY **in-process**. When the daemon exits, its
PTY master fds close, the kernel sends `SIGHUP`, and the agent/shell dies. The
`vt100` state and reader thread die with it. So on restart there is nothing to
adopt — `reconcile_orphans_on_startup` marks live rows `failed` because the
processes are already gone.

## The split (the important part for *this* repo)

acmux keeps the sidecar a **dumb buffered pipe** and puts the smart terminal
state in the client. We already have the smart part — the `vt100` parser and
ANSI-repaint snapshots live in the daemon. So:

| | asmux sidecar (never restarts) | asm-daemon (restarts freely) |
| --- | --- | --- |
| owns | PTY master fd, child pid, **raw-byte cursor ring buffer**, k/v metadata | `vt100` parser, ANSI-repaint snapshots, SQLite event log, all lifecycle logic |
| survives daemon restart? | **yes** | n/a |
| survives its own crash? | no (that's why it must never panic) | yes |
| survives host reboot? | no | records persist; live PTYs gone |

Crucially, **`vt100` stays in the daemon, not the sidecar.** A malformed escape
that panics `vt100` must never take down the process holding everyone's PTYs.
(`native.rs` already wraps `vt100` in `catch_unwind` for this reason.) This is a
deliberate refinement of `architecture.md`'s "sidecar runs the VT emulator"
line — worth updating there once agreed.

The daemon rebuilds `vt100` after reconnect by **replaying the ring buffer from
a cursor** (see below), so snapshot/resume semantics are unchanged from the
client's perspective.

## Persistence tiers (answers Q1)

| Tier | Holds | Survives daemon restart | Survives host reboot |
| --- | --- | --- | --- |
| SQLite (`asm-daemon`) | session records, metadata, cold terminal history | yes | yes |
| asmux ring buffer | live PTY + recent raw output + live metadata | yes | no |

On daemon start: reconnect to asmux → `session.list` → match each live session
against its SQLite row → **adopt** (rebuild `vt100` from the ring buffer);
reconcile the rest. If asmux itself is gone (host reboot), live sessions are
truly dead → reconcile to `failed` exactly as today.

## Fit against the existing `SessionBackend` trait

acmux maps cleanly onto our traits — a new `SidecarBackend` implements
`SessionBackend`; a `SidecarSession` implements `BackendSession`:

| Trait method | asmux RPC |
| --- | --- |
| `create(spec)` | `session.create` → uuid + record |
| (new) `adopt(id)` | `session.list` + `session.attach FROM_CURSOR` |
| `attach()` | `session.attach` (cursor replay → seed `vt100`) then live `SessionOutput` |
| `send_input()` | `SessionInput` (data-plane frame) |
| `resize()` | `session.resize` (SIGWINCH) |
| `stop()` | `session.kill` |
| `watch_status()` | `session.exited` event → `BackendStatus` |
| `last_seq()` | ring buffer `head_cursor` |

The daemon-side event log (`db.events()`), attention classifier, monitor task,
summaries, worktrees, and the whole HTTP/WS API stay **backend-agnostic**.

## Wire protocol

Length-prefixed binary frames over a **Unix domain socket** (AF_UNIX on Windows
too), at `<runtime_dir>/asmux.sock`, `0600`, parent dir `0700`:

```
┌─ u32 len (BE) ─┬─ u8 tag ─┬─ u16 ordinal (BE) ─┬─ body ─┐
tag 0x00 = the frozen encoding; 0x01–0xFF reserved for future encodings.
```

Discipline (from acmux, and right for us): **the wire format is
append-only/frozen** once shipped; ordinals and fields are never removed or
renumbered. MVP RPC subset:

- `0/1 hello` (pid, binary_sha256 for drift detection)
- `2/3 session.create`, `4/5 session.kill`, `8/9 session.list`
- `12/13 session.resize`, `16/17 session.attach`, `20/21 session.detach`
- `100 session.exited` (event), `200 error`
- `300 SessionInput`, `301 SessionOutput` (with `head_cursor`)
- `400 Heartbeat` (1 Hz, 3 s watchdog, on a dedicated OS thread so a busy
  runtime can't stall it)

Later: `6/7 purge`, `10/11 updateMetadata`, `14/15 readBuffer`, `18/19 status`,
`22/23 redraw`.

**Encoding: FlatBuffers** (`tag 0x00`), matching acmux — zero-copy,
language-neutral, and schema-versioned from day one. The full frozen schema,
framing, RPC semantics, error codes, and never-crash lints are specified in
[`asmux-protocol.md`](asmux-protocol.md).

## Ring buffer & cursors (seamless reconnect)

Per session: a fixed-capacity ring (default 2 MiB, range 16 KiB–32 MiB) with a
monotonic `head_cursor` = total bytes ever written (never an in-ring offset).
`attach FROM_CURSOR(n)` replays `n..head`; `n` older than `tail` returns
`buffer_gap` with the earliest cursor. The daemon remembers the last cursor it
consumed, so after a restart it re-attaches from there and the client terminal
continues with zero flicker.

## Lifecycle matrix

| Event | Effect |
| --- | --- |
| daemon crash / hash rotation | asmux untouched; daemon reconnects and re-adopts every session |
| asmux crash | all live sessions lost → their rows reconciled `failed` (hence never-panic discipline in asmux) |
| user stops a session | `session.kill`; ring buffer kept as a tombstone for late reads until `purge` |
| soft-reboot (binary drift) | daemon detects `binary_sha256` mismatch, warns "restart loses sessions", user confirms → `SIGTERM` asmux → respawn |
| host reboot | everything gone; SQLite records reconcile to `failed` |

## Startup state machine

```
live_rows = DB sessions in ('starting','running')
sidecar   = session.list over asmux (spawn asmux first if the socket is dead)

for row in live_rows:
    if row.id in sidecar and its child is alive:  ADOPT (attach FROM_CURSOR, rebuild vt100)
    else:                                          reconcile (exited/failed)
for s in sidecar:
    if s has no owning DB row:                     surface as orphan; owner adopts or kills
```

## Cross-platform

asmux runs on all three OSes, which is a concrete win over tmux:

| | Linux/macOS | Windows |
| --- | --- | --- |
| PTY | `portable_pty` openpty | ConPTY |
| IPC | `tokio::net::UnixListener` | AF_UNIX (Win10+) |
| detach | `setsid` via `pre_exec` | `DETACHED_PROCESS` + new group |
| kill | `libc::kill` | `TerminateProcess` |

A `tmux` backend can still exist as a *peer* (not nested) behind the same trait
for users who prefer tmux's server; asmux is the default.

## Incremental milestones

- **M1 — asmux core (standalone).** New crate `crates/asmux`. UDS server;
  `Session` = portable_pty master + cursor ring buffer + child pid; reader
  thread + reaper; RPCs `hello/create/list/attach/input/output/resize/kill/
  exited/heartbeat`. Verifiable with a tiny throwaway client; no daemon changes.
- **M2 — SidecarBackend in asm-daemon.** Implement `SessionBackend` over the
  asmux client; `vt100` rebuilt from ring-buffer replay. Behind
  `ASM_BACKEND=sidecar` (default stays `native`). Auto-spawn asmux if the socket
  is dead.
- **M3 — adopt-on-restart.** Add `SessionBackend::adopt`; schema v4
  `sessions.backend_handle`; replace `reconcile_orphans_on_startup` with
  adopt-or-reconcile. **Acceptance:** start agent → `SIGTERM` daemon → restart →
  session still `running`, terminal intact, zero-flicker reconnect.
- **M4 — hardening.** Soft-reboot (hash drift + confirm), orphan surfacing/adopt,
  `purge`, metadata RPCs, heartbeat/watchdog reconnect with backoff.
- **M5 — Windows.** ConPTY + AF_UNIX.

## Decisions

Settled: **asmux** name; **FlatBuffers** encoding. Ring default **2 MiB**
(range 16 KiB–32 MiB). asmux ring = hot/live replay; SQLite = cold/exited
history — keep both (the ring is not durable across host reboot and shouldn't
try to be).

Also settled at the protocol layer (see
[`asmux-protocol.md`](asmux-protocol.md) → Resolved protocol decisions):
`planus` for FlatBuffers codegen; single-attacher **with takeover** (a new
attach evicts the old one — this is how "continue on another device" force-
closes the prior client); `kill` on a dead session is idempotent; slow clients
are dropped and resync via `attach FromCursor`.

Deferred: whether/how **multiple sessions may share one branch** (collides with
Git's one-worktree-per-branch rule — tracked in the branch/worktree model, not
asmux). Pending: update `architecture.md` to move the VT emulator from sidecar
to daemon.
