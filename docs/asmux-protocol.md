# asmux Wire Protocol (Frozen Contract)

Status: **draft, pre-freeze** — changeable until asmux ships. After that the
contract is **append-only**: no ordinal is reused, no FlatBuffers field `id` is
removed or renumbered, no enum value repurposed, no error code changed in
meaning. New capability is added only as new ordinals, new trailing fields with
new ids, or new enum/error values.

Companion: [`durable-sessions.md`](durable-sessions.md) (architecture & rationale).

## Failure domain: one holder, bounded, recoverable

asmux is **one process holding every session's PTY + ring** (the data plane
multiplexes all sessions over one socket via `session_id`), rather than one
sidecar per session. A crash or OOM therefore loses *all* live sessions. Three
mechanisms bound that:

1. **Never-crash discipline** (see below) removes panics as a failure source.
2. **Hard total-memory cap** across all rings (`MEMORY_LIMIT`), so the holder
   cannot be OOM-killed by unbounded ring growth. A `create` that would breach
   the cap is refused, not honoured.
3. **Two-tier recovery: a boom loses *liveness*, not *history*.** The daemon
   continuously drains `SessionOutput` into its SQLite cold tier plus periodic
   `(vt100 snapshot, cursor)` pairs. After an asmux crash the live PTYs are gone
   and sessions reconcile to **`indeterminate`** (no completion record was
   persisted; see [`durable-sessions.md`](durable-sessions.md) → Reconciliation
   states), but every session's terminal history and last-known screen remain
   viewable. As a best-effort second line, asmux may
   memory-map / periodically flush each ring + metadata under `<runtime_dir>`
   (see [Never-crash invariants](#never-crash-invariants-asmux-only)) so even
   output the daemon had not yet drained is salvageable. The SQLite tier is
   authoritative; the flush is a bonus, never a correctness dependency.

## Transport

- **Unix domain socket** at `<runtime_dir>/asmux.sock` on Linux/macOS
  (socket file `0600`; parent dir `0700`). On Windows the same framing runs over
  **AF_UNIX where reliable, or a named pipe** as the native fallback; the
  `0600`/`0700` guarantee becomes an equivalent ACL restricting the socket/pipe
  to the owning user. No TCP, ever.
- One connection may drive many sessions (multiplexed): every data-plane and
  event frame carries `session_id`.

## Framing

```
┌─ u32 length (BE) ─┬─ u8 tag ─┬─ u16 ordinal (BE) ─┬─ FlatBuffers body ─┐
│  = 3 + body_len   │  = 0x00  │   message ordinal   │  length - 3 bytes   │
```

- `length` counts `tag + ordinal + body` (not itself). Max frame 16 MiB. On a
  larger frame the receiver sends `Error{code=FRAME_TOO_LARGE, rpc_id=0}` on a
  best-effort basis, then closes the connection — this is the one defined moment
  code 11 is emitted.
- `tag = 0x00` selects the FlatBuffers encoding (the only one defined). `0x01–
  0xFF` are reserved so a future encoding **or a different frame shape** (an
  extended header, a compressed frame) can be introduced without ambiguity — the
  receiver dispatches on `tag` first. The 3-byte header itself never grows;
  envelope evolution happens behind a new `tag`, negotiated via `hello`.
- The `ordinal` names the message; the body is that message's FlatBuffers root
  table. No union — the ordinal fixes the `root_type`.

## RPC discipline

- `hello` MUST be the first frame on a connection. Any other frame before a
  successful `HelloResponse` is a protocol error: the receiver replies
  `Error{PROTOCOL_MISMATCH, rpc_id=0}` and closes.
- Requests carry a client-chosen `rpc_id: uint64`; the matching response echoes
  it. `rpc_id` is monotonic per connection and **MUST be ≥ 1**.
- **`rpc_id = 0` is reserved for unsolicited `Error` frames** (data-plane errors,
  oversized frame, input overflow) — a client's request ids start at 1, so an
  unsolicited error can never be confused with a reply to request 0. Events
  (100-block) have **no `rpc_id` field at all**; they are inherently unsolicited.
- A request ordinal `N` is answered by ordinal `N+1`, or by `Error` (200) with
  the same `rpc_id`.
- Data-plane frames (300-block) have no `rpc_id`; their failures surface as
  unsolicited `Error{rpc_id=0, session_id=…}` (see `INPUT_OVERFLOW`,
  `NOT_ATTACHED`) — the write itself is fire-and-forget.

## Ordinal catalog

| Ordinal | Message | Kind |
| --- | --- | --- |
| 0 / 1 | `HelloRequest` / `HelloResponse` | rpc |
| 2 / 3 | `CreateRequest` / `CreateResponse` | rpc |
| 4 / 5 | `KillRequest` / `KillResponse` | rpc |
| 6 / 7 | `PurgeRequest` / `PurgeResponse` | rpc |
| 8 / 9 | `ListRequest` / `ListResponse` | rpc |
| 10 / 11 | `UpdateMetadataRequest` / `UpdateMetadataResponse` | rpc |
| 12 / 13 | `ResizeRequest` / `ResizeResponse` | rpc |
| 14 / 15 | `ReadBufferRequest` / `ReadBufferResponse` | rpc |
| 16 / 17 | `AttachRequest` / `AttachResponse` | rpc |
| 20 / 21 | `DetachRequest` / `DetachResponse` | rpc |
| 100 | `SessionExited` | event |
| 101 | `SessionDetached` | event (server-initiated eviction) |
| 200 | `Error` | error |
| 300 | `SessionInput` | data (client → asmux) |
| 301 | `SessionOutput` | data (asmux → client) |
| 400 | `Heartbeat` | control (both ways) |

Ordinals 18/19, 22/23 (`status`, `redraw`) and 24/25 (`readLog`) are reserved
for later milestones.

## Schema (`schema/asmux.fbs`)

```fbs
// FROZEN once shipped. Append-only: never remove/renumber an id, never reuse
// an ordinal, never change a field's type. Add new trailing ids / new ordinals.
namespace asmux.wire;

// FromEarliest replays tail..head then goes live — a one-round-trip
// "give me whatever you still have" that never returns BUFFER_GAP.
enum AttachMode : byte { FromCursor = 0, LiveOnly = 1, FromEarliest = 2 }

// Why asmux ended a connection's attachment without a DetachRequest.
enum DetachReason : byte {
  Superseded = 0,     // another attach took over this session (takeover)
  Killed = 1,         // the session was killed
  Backpressure = 2,   // this session's stream fell behind; resync via attach FromCursor
  ServerShutdown = 3,
  Purged = 4,         // the tombstone was purged during an in-flight replay
}

table KV { key: string (id: 0); value: string (id: 1); }

table SessionRecord {
  id: string (id: 0);                 // uuid v4
  alive: bool (id: 1);
  pid: int32 (id: 2);
  exit_code: int32 (id: 3);           // meaningful only when alive==false AND exit_signal==0
  exit_signal: int32 (id: 4);         // 0 if exited normally; when != 0, exit_code is -1 (signalled)
  cols: uint16 (id: 5);
  rows: uint16 (id: 6);
  head_cursor: uint64 (id: 7);        // total bytes ever produced
  tail_cursor: uint64 (id: 8);        // earliest replayable cursor
  ring_capacity: uint64 (id: 9);
  created_at_unix_ms: int64 (id: 10);
  command: string (id: 11);
  metadata: [KV] (id: 12);
}

table Error {                         // 200
  rpc_id: uint64 (id: 0);             // 0 => unsolicited (event-like error), else echoes a request
  code: uint32 (id: 1);               // see Error codes
  message: string (id: 2);            // human text, non-authoritative
  session_id: string (id: 3);         // when applicable
  earliest_cursor: uint64 (id: 4);    // set for BUFFER_GAP
}

table HelloRequest {                  // 0
  rpc_id: uint64 (id: 0);
  client_pid: int32 (id: 1);
  client_name: string (id: 2);
  protocol_min: uint16 (id: 3);
  protocol_max: uint16 (id: 4);
}
table HelloResponse {                 // 1
  rpc_id: uint64 (id: 0);
  server_pid: int32 (id: 1);          // the asmux process pid (the daemon is the *client*)
  binary_sha256: string (id: 2);      // BINARY DRIFT ONLY (soft-reboot); NOT instance identity
  protocol: uint16 (id: 3);           // negotiated version
  session_count: uint32 (id: 4);
  started_at_unix_ms: int64 (id: 5);
  instance_id: string (id: 6);        // random uuid minted at asmux startup; proves same instance
}

table CreateRequest {                 // 2
  rpc_id: uint64 (id: 0);
  command: string (id: 1);
  args: [string] (id: 2);
  cwd: string (id: 3);
  env: [KV] (id: 4);
  cols: uint16 (id: 5);
  rows: uint16 (id: 6);
  metadata: [KV] (id: 7);
  ring_capacity: uint64 (id: 8);      // 0 => server default (2 MiB)
  session_id: string (id: 9);         // optional caller-supplied uuid; else server mints
}
table CreateResponse { rpc_id: uint64 (id: 0); session: SessionRecord (id: 1); }

table KillRequest {                   // 4
  rpc_id: uint64 (id: 0);
  session_id: string (id: 1);
  signal: int32 (id: 2);              // 0 => platform default terminate
}
table KillResponse { rpc_id: uint64 (id: 0); }

table PurgeRequest { rpc_id: uint64 (id: 0); session_id: string (id: 1); }
table PurgeResponse { rpc_id: uint64 (id: 0); }

table ListRequest { rpc_id: uint64 (id: 0); }
table ListResponse { rpc_id: uint64 (id: 0); sessions: [SessionRecord] (id: 1); }

table UpdateMetadataRequest {         // 10
  rpc_id: uint64 (id: 0);
  session_id: string (id: 1);
  patch: [KV] (id: 2);                // value present (incl. "") => set; value null/absent => delete
}
table UpdateMetadataResponse { rpc_id: uint64 (id: 0); session: SessionRecord (id: 1); }

table ResizeRequest {                 // 12
  rpc_id: uint64 (id: 0);
  session_id: string (id: 1);
  cols: uint16 (id: 2);
  rows: uint16 (id: 3);
}
table ResizeResponse { rpc_id: uint64 (id: 0); }

table ReadBufferRequest {             // 14
  rpc_id: uint64 (id: 0);
  session_id: string (id: 1);
  from_cursor: uint64 (id: 2);
  max_bytes: uint64 (id: 3);          // 0 => server chooses; server always caps below the frame limit
}
table ReadBufferResponse {            // 15
  rpc_id: uint64 (id: 0);
  from_cursor: uint64 (id: 1);        // cursor of the first byte in `data`
  head_cursor: uint64 (id: 2);        // current head; keep reading until your cursor reaches this
  data: [ubyte] (id: 3);              // covers from_cursor .. from_cursor+len(data); MAY be a partial read
}

table AttachRequest {                 // 16
  rpc_id: uint64 (id: 0);
  session_id: string (id: 1);
  mode: AttachMode (id: 2);
  from_cursor: uint64 (id: 3);        // used when mode == FromCursor
}
table AttachResponse {                // 17
  rpc_id: uint64 (id: 0);
  head_cursor: uint64 (id: 1);        // live stream continues from here
}

table DetachRequest { rpc_id: uint64 (id: 0); session_id: string (id: 1); }
table DetachResponse { rpc_id: uint64 (id: 0); }

table SessionExited {                 // 100 (event)
  session_id: string (id: 0);
  exit_code: int32 (id: 1);           // valid when exit_signal == 0
  exit_signal: int32 (id: 2);         // 0 if normal; else the child was signalled
  head_cursor: uint64 (id: 3);        // final cursor
}

table SessionDetached {               // 101 (event, server -> evicted client)
  session_id: string (id: 0);
  reason: DetachReason (id: 1);
  last_cursor: uint64 (id: 2);        // resume point for a later attach FromCursor
}

table SessionInput {                  // 300 (data, client -> asmux)
  session_id: string (id: 0);
  data: [ubyte] (id: 1);
}
table SessionOutput {                 // 301 (data, asmux -> client)
  session_id: string (id: 0);
  head_cursor: uint64 (id: 1);        // cursor AFTER this chunk
  data: [ubyte] (id: 2);
}

table Heartbeat { unix_ms: int64 (id: 0); }  // 400
```

## Cursors & replay

`head_cursor` = total bytes ever written to a session (monotonic, from birth;
`saturating_add`, never wraps). `tail_cursor` = oldest still in the ring
(`head - min(head, capacity)`). Not in-ring offsets — global timestamps.

- `attach FromCursor(n)`: if `tail ≤ n ≤ head`, replay `n..head` as
  `SessionOutput` frames, then stream live. If `n < tail` → `Error{BUFFER_GAP,
  earliest_cursor = tail}`. If **`n > head` → `Error{INVALID_ARGUMENT}`**:
  cursors are monotonic per session, so a cursor past head is always client
  corruption and must surface, not be silently clamped (the daemon's adopt path
  relies on `BUFFER_GAP`/`INVALID_ARGUMENT` to detect drift rather than resume
  from a wrong place).
- `attach FromEarliest`: replay `tail..head`, then stream live. The one-round-
  trip "give me whatever you still have"; never returns `BUFFER_GAP`. Use this
  for a fresh client that just wants current state; use `FromCursor` when you
  hold a specific resume point and a gap is meaningful.
- `attach LiveOnly`: stream new bytes from current `head`.
- `AttachResponse` is guaranteed to arrive **before** any replay `SessionOutput`
  frame for that attach.
- **`ReadBufferResponse` may be partial.** A single response never exceeds the
  16 MiB frame cap, and a ring may be larger (up to 32 MiB), so `data` can cover
  fewer than `head_cursor - from_cursor` bytes. The client repeats `readBuffer`
  from `from_cursor + len(data)` until its cursor reaches `head_cursor`. A
  `readBuffer` whose `from_cursor < tail` is **strict `BUFFER_GAP`** (consistent
  with `attach FromCursor`); the server never clamps forward, so `response.
  from_cursor` always equals the request and only advances across the read loop.
- **Zero-flicker resume needs a snapshot, not just a cursor — and asmux never
  synthesizes one** (no `vt100` in asmux). The daemon persists a
  `(vt100 snapshot, snapshot_cursor)` pair and tracks its last consumed cursor;
  adopt = seed `vt100` from the snapshot, replay `snapshot_cursor..consumed` from
  the daemon's **own SQLite cold history**, then `attach FromCursor(consumed)`
  for `consumed..head` off the ring. Exact reconstruction holds **only while the
  ring still covers `consumed`** (`tail ≤ consumed`). If the ring wrapped past it
  (daemon down too long), `attach FromCursor(consumed)` returns `BUFFER_GAP`:
  asmux cannot fill the hole, so the daemon renders an explicit **gap marker**
  for the lost range and resyncs `FromEarliest` — approximate until the live app
  repaints. See [`durable-sessions.md`](durable-sessions.md) → adopt invariant.

## Attach model: single-attacher with takeover

A session has **at most one attached connection at a time**. This mirrors the
product rule "one session, one client" (see `requirements.md` → Single-Device
Active Session): continuing a session on another device forcibly detaches the
existing one.

- A new `AttachRequest` for a session that already has an attacher **supersedes**
  it. asmux sends the previous connection a `SessionDetached{reason=Superseded,
  last_cursor}` event, stops streaming to it, and grants the new attach. No
  error is returned to either side — takeover is the defined behaviour.
- A re-attach on the **same** connection to a session it already holds is
  supersede-self: it resets the replay cursor to the new request and is not an
  error.
- The evicted client may reconnect later and `attach FromCursor(last_cursor)` to
  resume from where it left off (subject to `BUFFER_GAP` if it waited too long).
- This same mechanism covers the daemon-restart case: a fresh daemon attaching
  over a still-half-open previous connection simply takes over.
- Because there is only ever one attacher, input has a single writer — there is
  no interleaving/authority question.

## Session lifecycle & tombstones

- **Alive** (`alive=true`): accepts `input`, `resize`, `attach`, `kill`.
- On child exit → reaper sets `alive=false`, `exit_code`/`exit_signal`, emits
  `SessionExited` (100) to **the attached connection, if any**. Because the event
  reaches only a currently-attached client, a client that was detached at exit
  time (mid-reconnect, suspend/resume) MUST issue `list` after (re)connecting to
  reconcile exits it missed — see [Liveness](#liveness). The **ring buffer is
  retained** (tombstone) so late `readBuffer`/`attach FromCursor` still work.
- **Tombstone** (`alive=false`): allows `readBuffer`, `attach FromCursor`/
  `FromEarliest` (replays to `head`, then completes with no live stream — the
  client derives end-of-replay from `AttachResponse.head_cursor` alone, so when
  the attach cursor already equals `head` zero `SessionOutput` frames are sent,
  not an empty one), `updateMetadata`, `list`.
  Rejects `resize` and `input` with `SESSION_NOT_ALIVE`. `kill` is an idempotent
  success (already dead).
- `purge`: frees the ring and removes the record. Rejects a live session with
  `SESSION_ALIVE` (caller must `kill` first).
- **Tombstone memory is bounded, and its interaction with the cap is ordered.**
  Retained rings count against the same total-memory cap as live ones. On a
  `create` that would breach the cap, asmux first **evicts oldest tombstones
  (LRU)**; only if it is still over budget with live rings alone does `create`
  fail with `MEMORY_LIMIT`. A read of an evicted tombstone returns
  `UNKNOWN_SESSION`, so an owner that never calls `purge` cannot leak a ring per
  exited session. A `purge` arriving during an
  in-flight tombstone replay **wins**: the replaying connection receives
  `SessionDetached{reason=Purged, last_cursor}`.

## Error codes

Authoritative machine codes (message string is advisory only):

| Code | Name | Meaning |
| --- | --- | --- |
| 1 | `UNKNOWN_SESSION` | no session with that id (incl. an evicted tombstone) |
| 2 | `SESSION_NOT_ALIVE` | op needs a live session (resize/input) |
| 3 | `SESSION_ALIVE` | op needs a tombstone (purge) |
| 4 | `BUFFER_GAP` | `from_cursor` older than `tail`; see `earliest_cursor` |
| 5 | `INVALID_ARGUMENT` | bad command/cwd/geometry/capacity, or `from_cursor > head` |
| 6 | `SPAWN_FAILED` | openpty/fork/exec failed |
| 7 | `ALLOC_FAILED` | ring buffer allocation failed (fallible alloc) |
| 8 | `CAPACITY_OUT_OF_RANGE` | ring capacity outside [16 KiB, 32 MiB] |
| 9 | `PROTOCOL_MISMATCH` | no overlapping protocol version, or a frame before `hello` |
| 10 | `NOT_ATTACHED` | detach/input for a session this conn isn't attached to (unsolicited, `rpc_id=0`) |
| 11 | `FRAME_TOO_LARGE` | frame exceeded the 16 MiB cap; sent `rpc_id=0` then the conn closes |
| 12 | `INTERNAL` | last resort; must never be a panic |
| 13 | `SESSION_EXISTS` | caller-supplied `session_id` exists with a different launch fingerprint |
| 14 | `INPUT_OVERFLOW` | per-session input queue full; input dropped (unsolicited, `rpc_id=0`) |
| 15 | `MEMORY_LIMIT` | `create` would breach the total ring-memory cap |

Codes are append-only like ordinals.

## Create idempotency (crash-consistent adopt)

A caller-supplied `session_id` makes `create` idempotent, which is what lets the
daemon write its SQLite row *then* create the PTY and safely retry if it crashes
in between (the basis of M3 adopt):

Idempotency is keyed on an immutable **launch fingerprint** — a stable hash over
`command`, `args`, `cwd`, and `env` (hashed, so secrets in `env` are never
compared or stored in the clear), pinned to the id at first create:

- id not present → create normally, pin its fingerprint.
- id present **and fingerprint matches** → return the existing `SessionRecord`
  (idempotent success — the daemon re-adopts rather than double-spawning).
  Matching the full spec, not `command` alone, means a retry differing in
  `args`/`cwd`/`env` returns `SESSION_EXISTS` rather than binding the wrong process.
- id present with a **different fingerprint** → `Error{SESSION_EXISTS}`.

The returned record may be `alive=false` (the session exited during the daemon's
downtime) — callers MUST check `alive`, which the adopt flow does naturally.

## Input flow control

`SessionInput` is fire-and-forget, but asmux must never block on a PTY write (a
child that stops reading its PTY would otherwise stall the holder). Each session
has a **bounded input queue**; if the child isn't draining and the queue is
full, further input for that session is **dropped** and asmux emits
`Error{INPUT_OVERFLOW, session_id, rpc_id=0}`. asmux never blocks the connection
reader on a slow child.

## Backpressure (per-session fairness + a connection-level floor)

The ring buffer is the source of truth; the socket is best-effort delivery of
it. Two levels, because all sessions share **one** connection and its OS send
buffer — per-session queues give *fairness*, not full isolation:

- **Per session:** each attached session has a bounded output send-queue. A
  writer scheduler drains the queues **round-robin** into the shared socket under
  a global outstanding-bytes bound, so one high-volume session can't monopolise
  the writer. If a single session's queue overflows (its attacher can't keep up
  relative to the others), asmux **evicts just that session** with
  `SessionDetached{reason=Backpressure, last_cursor}` — reserving control-frame
  headroom so the event fits even when data queues are full — and keeps streaming
  the rest. The evicted session resyncs via `attach FromCursor(last_cursor)` (or,
  on `BUFFER_GAP`, the daemon's gap-marker fallback — asmux does not synthesize
  snapshots).
- **Connection-level floor.** If the *socket itself* stalls
  (the reader is gone, or the OS buffer stays full past the global bound), that
  is not a per-session condition — **all** sessions degrade until it clears.
  asmux never grows memory unbounded to hide it; the heartbeat watchdog tears the
  connection down and the daemon reconnects and resyncs every session
  `FromCursor`. So per-session backpressure keeps one session from starving
  another *while the pipe flows*; a fully stalled pipe is a connection event,
  recovered by reconnect, not masked.
- A `Backpressure` eviction also rejects that session's input until re-attach
  (input needs an attachment). The client should **re-attach immediately** on the
  event and tolerate transient `NOT_ATTACHED` errors for input frames that raced
  the eviction.

## Liveness

`Heartbeat` (400) flows both directions at ~1 Hz. To tolerate laptop
suspend/resume and transient daemon stalls (e.g. a slow SQLite checkpoint on the
runtime), the idle-teardown watchdog is **10 s** of no frames of any kind, not
3 s — the ring absorbs all output while detached, so slower detection is nearly
free and avoids needless reconnect churn. asmux sends heartbeats from a
**dedicated OS thread**, not a tokio task, so a busy async runtime can't delay
them. After **any** (re)connect the daemon issues `list` to reconcile sessions
that exited while it was detached (see lifecycle above).

## Never-crash invariants (asmux only)

The process holding every session's PTY must not die. Enforced at the crate
root:

```rust
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic,
        clippy::todo, clippy::unimplemented, clippy::indexing_slicing,
        clippy::integer_division, clippy::arithmetic_side_effects)]
```

- Ring allocation uses fallible reserve (`try_reserve`) → `ALLOC_FAILED`, never
  abort. A total across all rings is capped → `MEMORY_LIMIT` on `create`.
- Every RPC handler returns `Result`; a handler error becomes an `Error` frame,
  never a panic.
- Process control uses **safe wrappers** (`nix`'s `kill`, `portable_pty`'s
  `Child::kill`), never a raw `libc::kill` — `forbid(unsafe_code)` cannot be
  locally overridden.
- A `vt100` parser is **not** present in asmux — terminal interpretation is the
  daemon's job, so a bad escape sequence can never destabilise the holder.
- **No *correctness-gating* state files.** The wire contract requires no files
  on disk. But because asmux *outlives* the daemon, stdout-only logging fails
  exactly when needed (the daemon's pipe loses its reader → `EPIPE`, logs lost).
  So asmux MAY, best-effort: (a) append to a log file, and (b) flush each ring +
  metadata under `<runtime_dir>` for post-crash salvage. Keep this **off the
  reader thread**: a dedicated flush thread *copies* out of the ring and writes.
  Do **not** mmap the ring into the reader path — mmap stores can block on page
  faults / dirty-page writeback precisely under the memory pressure that precedes
  an OOM, i.e. exactly when salvage matters. The flush is best-effort (a
  full/slow disk is skipped, never awaited) and **never authoritative** (the
  daemon's SQLite cold tier is). The salvage file is **version-stamped** — the
  daemon parses it after an asmux crash, so a format mismatch must fail
  detectably rather than mis-parse. A future `readLog` RPC (24/25) can expose an
  in-memory log ring instead of/in addition to the file.

## Version negotiation

`hello` carries `[protocol_min, protocol_max]`; asmux replies with a single
`protocol` in that range or `Error{PROTOCOL_MISMATCH}`. v1 is this document.
Because the schema is append-only, most changes bump nothing; a version bump is
reserved for *semantic* changes to existing ordinals (e.g. if concurrent multi-
attach were ever added, it would need per-attachment cursors — a v2 change, not
an in-place mutation).

## Resolved protocol decisions

1. **Toolchain: `planus`** (pure-Rust FlatBuffers), codegen from `schema/
   asmux.fbs` via `build.rs`. Falls back to `flatc` + the `flatbuffers` crate if
   needed; wire bytes are identical, so this is not part of the frozen contract.
2. **Single holder for all sessions**, bounded by a total-memory cap and made
   recoverable by the two-tier model (see [Failure domain](#failure-domain-one-holder-bounded-recoverable)),
   in place of a per-session sidecar.
3. **Single-attacher with takeover** (not broadcast, not concurrent multi-
   attach). A new attach supersedes the current one via
   `SessionDetached{Superseded}`. One writer, no interleaving.
4. **`kill` on a dead session: idempotent success.** No error.
5. **Backpressure: per-session eviction** via `SessionDetached{Backpressure}`
   (round-robin writer under a shared byte bound), resync `FromCursor`. A fully
   stalled *socket* is a connection-level event recovered by watchdog reconnect —
   per-session queues give fairness, not full isolation.
6. **`create` is idempotent** on a caller-supplied `session_id`, keyed on an
   immutable launch fingerprint (hash of command+args+cwd+env); a mismatch is
   `SESSION_EXISTS`, and the returned record may be `alive=false`.
7. **Input is fire-and-forget with a bounded queue**; overflow →
   `INPUT_OVERFLOW`, never a blocked reader.
8. **`FromEarliest` attach** for one-round-trip "current state"; `FromCursor`
   stays strict (`BUFFER_GAP` / `INVALID_ARGUMENT`) for gap-detecting adopt.
9. **Instance identity is `instance_id`** (random per asmux startup), not
   `binary_sha256` (drift detection only). The daemon uses it to tell "the holder
   I adopted before" from "a fresh holder after a crash/recreate".
10. **Gap recovery is the daemon's job, not asmux's.** On `BUFFER_GAP` asmux
    returns the earliest cursor; the daemon stitches its own cold history and
    renders a gap marker. asmux never synthesizes `vt100` snapshots.

## Deferred (not protocol-frozen)

- **Concurrent multi-device attach** (the old `requirements.md` model) is
  superseded by single-attacher-with-takeover for v1. If revived, it is a v2
  semantic change (per-attachment cursors, resize ownership, per-connection
  backpressure), not an in-place edit.
- **"Multiple sessions on the same branch"** is a *workspace/worktree* decision,
  not an asmux concern — and it collides with Git's rule that a branch can be
  checked out in only one worktree at once (we already surface that as a clean
  error). Tracked in `durable-sessions.md`/the branch model.
- **Windows transport specifics** (AF_UNIX vs named pipe, ACL model) are settled
  in intent above but the concrete Win32 wrappers land in M5.
