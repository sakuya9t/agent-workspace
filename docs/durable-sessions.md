# Durable Sessions via an Out-of-Process Session Holder ("asmux")

Status: **design, refining before code.** No implementation yet. Adapts the
"acmux" design (from the agent-conductor project) to this codebase.

Locked decisions: sidecar crate/binary named **`asmux`**; wire encoding is
**FlatBuffers** (schema frozen once shipped); **one holder for all sessions**
(bounded + recoverable, see [Failure domain](#failure-domain-blast-radius));
**single-attacher with takeover**; `vt100` lives in the **daemon**, not asmux.
The frozen wire contract lives in [`asmux-protocol.md`](asmux-protocol.md).
Deployment topology (two-container Docker) lives in
[`deployment.md`](deployment.md).

## The two questions this answers

1. **Can we persist metadata to pick up where we left off?** Yes — but the
   metadata alone isn't enough; the *live PTY process* must also survive. The
   answer is a two-tier model: SQLite (cold, survives everything) plus a
   long-lived sidecar (hot, holds the live PTYs across daemon restarts).
2. **Can we separate the daemon from a tmux-like process that never stops?**
   Yes. A dedicated `asmux` sidecar owns the PTY master fds and stays up while
   `asm-daemon` restarts (crash / upgrade / hash rotation).

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
that panics `vt100` must never take down the process holding everyone's PTYs
(`native.rs` already wraps `vt100` in `catch_unwind` for this reason).

## Failure domain (blast radius)

One holder for all sessions means one crash or OOM loses **all** live PTYs,
rather than one sidecar per session. The cost is bounded so a boom loses
**liveness, not history**:

- **Never-crash discipline** (see [`asmux-protocol.md`](asmux-protocol.md) →
  Never-crash invariants) removes panics; process control via `nix`/`portable_pty`
  under `#![forbid(unsafe_code)]`, fallible `try_reserve`, no `vt100` in asmux.
- **Hard total-memory cap** across all rings (`MEMORY_LIMIT` on `create`) so the
  holder can't be OOM-killed by unbounded ring growth.
- **Two-tier recovery.** The daemon continuously drains `SessionOutput` into
  SQLite (cold history) + periodic `(vt100 snapshot, cursor)` pairs, so after an
  asmux crash the live PTYs are gone (sessions reconcile to **`indeterminate`**;
  see [Reconciliation states](#reconciliation-states)) but every session's
  history and last-known screen remain viewable.
  As a best-effort second line, a **dedicated flush thread** (never the reader
  path — mmap stores can block under the very memory pressure that precedes an
  OOM) copies each ring + metadata to a **version-stamped** file under
  `<runtime_dir>` (non-blocking, never authoritative) so even output the daemon
  hadn't drained is salvageable.

## Persistence tiers (answers Q1)

| Tier | Holds | Survives daemon restart | Survives host reboot |
| --- | --- | --- | --- |
| SQLite (`asm-daemon`) | session records, metadata, cold terminal history, `(snapshot, cursor)` pairs | yes | yes |
| asmux ring buffer | live PTY + recent raw output + live metadata | yes | no |
| asmux best-effort flush | last ring tail + metadata (crash salvage only) | n/a | no |

On daemon start: reconnect to asmux → `session.list` → match each live session
against its SQLite row → **adopt** (see below); reconcile the rest. If asmux
itself is gone (host reboot), live sessions are truly dead → reconcile to
`indeterminate` (the mid-flight outcome is unknown, not a proven failure — see
[Reconciliation states](#reconciliation-states)).

## Adopt invariant (the headline "terminal intact" promise)

"Zero-flicker resume" needs **more than a persisted cursor.** After a daemon
restart the `vt100` state at that cursor died with the daemon; replaying raw
bytes from the last cursor cannot rebuild a full-screen TUI (alt-screen entry,
modes, colours, cursor position all happened earlier). Replaying from `tail`
instead starts mid-escape-sequence and misses everything pre-tail. So the adopt
contract is explicit:

> The daemon persists a **`(vt100 snapshot, snapshot_cursor)` pair atomically**
> and tracks its **last consumed cursor** (`consumed ≥ snapshot_cursor`, since it
> drains output continuously). To adopt: seed `vt100` from the latest snapshot,
> replay `snapshot_cursor..consumed` from the daemon's **own SQLite cold
> history**, then `attach FromCursor(consumed)` for `consumed..head` off the
> ring. The screen is reconstructed **exactly** — *provided the ring still covers
> `consumed`* (`tail ≤ consumed`).

- **Attach from `consumed`, not `snapshot_cursor`.** The daemon already holds
  `snapshot_cursor..consumed` in cold storage, so it only needs the ring to still
  hold from `consumed` forward — minimising the window in which a gap can occur.
- **Gap fallback.** If the daemon was
  down long enough that the ring **wrapped past `consumed`** (`tail > consumed`),
  the bytes `consumed..tail` are gone from *both* tiers — asmux dropped them and
  the daemon never saw them. `attach FromCursor(consumed)` then returns
  `BUFFER_GAP`; the daemon renders an explicit **gap marker** (the existing
  dropped-range mechanism, `requirements.md` → gap markers) for the lost span and
  resyncs `FromEarliest` into a fresh `vt100`. That is **approximate** (it starts
  mid-escape-sequence) until the live app repaints — the daemon can nudge one via
  the resize-repaint trigger. asmux never synthesizes a snapshot; gap recovery is
  entirely daemon-side.
- **Cadence (write-amplification):** snapshot **periodically** — every ~N KiB of
  output or ~T seconds, whichever first — and **on clean detach**, *not* per
  output chunk. Between snapshots, adopt replays at most one interval's worth of
  bytes, which is cheap and bounded.
- The snapshot is the daemon's existing ANSI-repaint form (already emitted by
  `BackendSession::attach() -> (Snapshot, …)` and stored for exited-session
  history), written durably and paired with the ring cursor it corresponds to.
- **This is an M3 acceptance criterion** (scoped to the no-gap path), because
  "terminal intact after restart"
  hinges entirely on it (see M3 below).

## Fit against the existing `SessionBackend` trait

acmux maps cleanly onto our traits — a new `SidecarBackend` implements
`SessionBackend`; a `SidecarSession` implements `BackendSession`:

| Trait method | asmux RPC |
| --- | --- |
| `create(spec)` | `session.create` → uuid + record |
| `adopt(id)` | `session.list` + load `(snapshot, cursor)` + `session.attach FromCursor` |
| `attach()` | `session.attach` (cursor replay → seed `vt100`) then live `SessionOutput` |
| `send_input()` | `SessionInput` (data-plane frame) |
| `resize()` | `session.resize` (SIGWINCH) |
| `stop()` | `session.kill` |
| `watch_status()` | `session.exited` event → `BackendStatus` |
| `last_seq()` | ring buffer `head_cursor` |

The daemon-side event log (`db.events()`), attention classifier, monitor task,
summaries, worktrees, and the whole HTTP/WS API stay **backend-agnostic**.

## Wire protocol

Length-prefixed binary FlatBuffers frames over a **Unix domain socket** at
`<runtime_dir>/asmux.sock` (`0600`, parent dir `0700`). Full framing, schema,
RPC semantics, error codes, cursor/replay rules, backpressure, and the
never-crash lints are specified in [`asmux-protocol.md`](asmux-protocol.md) —
that document is the frozen contract; this one is the rationale.

Key semantics that matter to the daemon:

- **Single-attacher with takeover** — the daemon is the one attacher; a new
  attach (e.g. from a fresh daemon after restart) supersedes the old one.
- **`SessionExited` reaches only the attached connection**, so the daemon issues
  `list` after **any** (re)connect — not just at startup — to catch exits it
  missed while detached (the 10 s heartbeat watchdog + laptop suspend/resume make
  brief detaches routine, not rare).
- **Per-session backpressure**: a slow session is evicted with
  `SessionDetached{Backpressure}` and resynced via `attach FromCursor(last)`;
  one noisy session never disturbs the others.

## Ring buffer & cursors (seamless reconnect)

Per session: a fixed-capacity ring (default 2 MiB, range 16 KiB–32 MiB) with a
monotonic `head_cursor` = total bytes ever written (never an in-ring offset).
`attach FromCursor(n)` replays `n..head`; `n` older than `tail` returns
`BUFFER_GAP`, `n > head` returns `INVALID_ARGUMENT` (drift detection). The daemon
remembers the last cursor it consumed and pairs it with a `vt100` snapshot (see
[Adopt invariant](#adopt-invariant-the-headline-terminal-intact-promise)); after
a restart it seeds `vt100` from the snapshot and re-attaches from that cursor, so
the client terminal continues with zero flicker.

## Lifecycle matrix

| Event | Effect |
| --- | --- |
| daemon crash / hash rotation | asmux untouched; daemon reconnects, `list`, re-adopts every session from `(snapshot, cursor)` |
| asmux crash | live PTYs lost → rows reconciled **`indeterminate`** (no completion record; outcome unknown), **but history + last screen preserved** (SQLite cold tier; best-effort ring flush) |
| user stops a session | `session.kill`; ring kept as a tombstone (bounded, LRU-evicted) for late reads until `purge` |
| soft-reboot (binary drift) | daemon detects `binary_sha256` mismatch, warns "restart loses sessions", user confirms → `SIGTERM` asmux → respawn |
| host reboot | everything hot gone; live SQLite records reconcile to **`indeterminate`** |

## Reconciliation states

When the daemon can't confirm how a live session ended, it must not fake
certainty. Three outcomes:

- **`exited` / `failed`** — a real completion record exists: a `kill` tombstone or
  a `SessionExited` with an `exit_code`. The outcome is known.
- **`indeterminate`** — the holder died while the session was running
  (asmux crash, host reboot), so **no completion record was ever persisted.** The
  child may have finished successfully microseconds before the crash, been
  killed, or — rarely — still be running as an orphan. We can't tell, so we don't
  assert `failed`.

An `indeterminate` session carries an advisory:

> *No completion record — the session holder exited while this was running. It may
> have finished, been killed, or (rarely) still be running. Check the preserved
> output before assuming.*

The two-tier recovery is what makes "check the preserved output" actionable: the
SQLite cold tier (and the best-effort ring flush) hold the last-known screen and
recent bytes to inspect. Normal `kill`/exit paths never get this marker — they
have a record. This adds one value to the daemon-side `SessionStatus` vocabulary
(`starting/running/exited/failed/stopped/archived` **+ `indeterminate`**) with a
matching UI treatment.

## Startup / reconnect state machine

```
live_rows = DB sessions in ('starting','running')
sidecar   = session.list over asmux (spawn asmux first if the socket is dead)
# HelloResponse.instance_id distinguishes "same holder I adopted before" from a
# fresh one after a crash/recreate (binary_sha256 is drift detection only).

for row in live_rows:
    if row.id in sidecar and its child is alive:
        seed vt100 from persisted (snapshot, snapshot_cursor)
        stitch snapshot_cursor..consumed from cold history
        attach FromCursor(consumed)            # BUFFER_GAP -> gap marker + FromEarliest
    elif row.id in sidecar:                    # child exited; asmux has a real record
        reconcile from exit_code (exited/failed)
    else:                                      # asmux has no record (crash/reboot)
        reconcile (indeterminate)              # no completion record -> outcome unknown
for s in sidecar:
    if s has no owning DB row:  surface as orphan; owner adopts or kills
# on ANY later reconnect (not just startup): re-run session.list to catch missed exits
```

## Cross-platform

asmux runs on all three OSes, which is a concrete win over tmux:

| | Linux/macOS | Windows |
| --- | --- | --- |
| PTY | `portable_pty` openpty | ConPTY |
| IPC | `tokio::net::UnixListener` | AF_UNIX via `uds_windows`/`std` **or a named pipe** (tokio has no AF_UNIX on Windows); `0600` becomes an owner-only ACL |
| detach | `setsid` via `pre_exec` | `DETACHED_PROCESS` + new group |
| kill | `nix::sys::signal::kill` (safe wrapper; **not** `libc::kill` under `forbid(unsafe)`) | `portable_pty` `Child::kill` / `TerminateProcess` |

A `tmux` backend can still exist as a *peer* (not nested) behind the same trait
for users who prefer tmux's server; asmux is the default.

The same daemon-restarts-freely / asmux-never-restarts split maps onto a
**two-container** Docker topology so that a *daemon image update* re-adopts live
sessions instead of dropping them — see [`deployment.md`](deployment.md).

## Incremental milestones

- **M1 — asmux core (standalone).** New crate `crates/asmux`. UDS server;
  `Session` = portable_pty master + cursor ring buffer + child pid; reader
  thread + reaper; total-memory cap; RPCs `hello/create/list/attach/input/
  output/resize/kill/exited/heartbeat`; bounded input queue; per-session
  backpressure. Verifiable with a tiny throwaway client; no daemon changes.
- **M2 — SidecarBackend in asm-daemon.** Implement `SessionBackend` over the
  asmux client; `vt100` rebuilt from ring-buffer replay. Behind
  `ASM_BACKEND=sidecar` (default stays `native`). Auto-spawn asmux if the socket
  is dead — **and spawn it outside the daemon's kill zone.** Under systemd,
  `KillMode=control-group` (the default) tears down the daemon's whole cgroup on
  stop/restart, which would take asmux with it. Spawn via `systemd-run --user
  --scope` (or a separate user unit); plain `setsid` is the non-systemd fallback
  and **does not** escape a cgroup, so it is not sufficient on systemd. In the
  two-container deployment asmux is a *peer* container, not a child — the daemon
  connects and retries, and `ASM_ASMUX_AUTOSPAWN=0` disables the spawn path.
- **M3 — adopt-on-restart.** Add `SessionBackend::adopt`; schema **v5**
  `sessions.backend_handle` (the DB already reaches `user_version 4` via the
  `risky` column); persist `(vt100 snapshot, cursor)` pairs; replace
  `reconcile_orphans_on_startup` with adopt-or-reconcile. **Acceptance (no-gap
  path):** start a full-screen-TUI agent → emit output → `SIGTERM` daemon →
  restart → session still `running`, **screen reconstructed exactly** from
  snapshot + cold-stitch + ring replay, zero-flicker reconnect. **Acceptance (gap
  path):** keep the daemon down until the ring wraps past `consumed` → restart →
  session `running` with an explicit **gap marker** and an approximate screen that
  becomes exact on the next repaint (no crash, no silent wrong state). Also:
  duplicate `create` retry returns the existing record; an asmux kill
  mid-session reconciles to **`indeterminate`**, not `failed`.
- **M4 — hardening.** Soft-reboot (hash drift + confirm), orphan surfacing/adopt,
  `purge`, metadata RPCs, `readLog`, heartbeat/watchdog reconnect with backoff,
  slow-attacher drop + resync, `list`-after-reconnect reconciliation.
- **M5 — Windows.** ConPTY + AF_UNIX/named-pipe transport + ACL socket perms.

## Decisions

Settled: **asmux** name; **FlatBuffers** encoding; **single holder** (bounded +
recoverable); **single-attacher with takeover**; **`vt100` in the daemon**. Ring
default **2 MiB** (range 16 KiB–32 MiB) with a total-memory cap. asmux ring =
hot/live replay; SQLite = cold/exited history + `(snapshot, cursor)` pairs —
keep both (the ring is not durable across host reboot and shouldn't try to be).

Also settled at the protocol layer (see
[`asmux-protocol.md`](asmux-protocol.md) → Resolved protocol decisions):
`planus` codegen; per-session backpressure; idempotent `create`; bounded input
queue; `FromEarliest` attach; `kill` idempotent on a dead session.

Deferred: whether/how **multiple sessions may share one branch** (collides with
Git's one-worktree-per-branch rule — tracked in the branch/worktree model, not
asmux); **concurrent multi-device attach** (superseded by single-attacher for
v1; a v2 semantic change if revived). Done: `architecture.md` updated to move the
VT emulator into the daemon and record the single-holder / single-attacher
divergences.
