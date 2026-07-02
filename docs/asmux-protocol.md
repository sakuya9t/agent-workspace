# asmux Wire Protocol (Frozen Contract)

Status: **draft, pre-freeze.** Once asmux ships, this contract is
**append-only**: no ordinal is reused, no FlatBuffers field `id` is removed or
renumbered, no enum value is repurposed. New capability is added only as new
ordinals or new trailing fields with new ids. Review carefully before the first
release; after that, only additions.

Companion: [`durable-sessions.md`](durable-sessions.md) (architecture & rationale).

## Transport

- **Unix domain socket** at `<runtime_dir>/asmux.sock` (AF_UNIX on Windows too).
  Socket file `0600`; parent dir `0700`. No TCP, ever.
- One connection may drive many sessions (multiplexed): every data-plane and
  event frame carries `session_id`.

## Framing

```
┌─ u32 length (BE) ─┬─ u8 tag ─┬─ u16 ordinal (BE) ─┬─ FlatBuffers body ─┐
│  = 3 + body_len   │  = 0x00  │   message ordinal   │  length - 3 bytes   │
```

- `length` counts `tag + ordinal + body` (not itself). Max frame 16 MiB;
  larger is a protocol error and the connection is closed.
- `tag = 0x00` selects the FlatBuffers encoding (the only one defined). `0x01–
  0xFF` are reserved so a future encoding can be introduced without ambiguity.
- The `ordinal` names the message; the body is that message's FlatBuffers root
  table. No union — the ordinal fixes the `root_type`.

## RPC discipline

- Requests carry a client-chosen `rpc_id: uint64`; the matching response echoes
  it. `rpc_id` is unique per connection and monotonic.
- A request ordinal `N` is answered by ordinal `N+1`, or by `Error` (200) with
  the same `rpc_id`.
- Events (100-block) and data-plane frames (300-block) have no `rpc_id`.

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
| 200 | `Error` | error |
| 300 | `SessionInput` | data (client → asmux) |
| 301 | `SessionOutput` | data (asmux → client) |
| 400 | `Heartbeat` | control (both ways) |

Ordinals 18/19, 22/23 (`status`, `redraw`) are reserved for later milestones.

## Schema (`schema/asmux.fbs`)

```fbs
// FROZEN once shipped. Append-only: never remove/renumber an id, never reuse
// an ordinal, never change a field's type. Add new trailing ids / new ordinals.
namespace asmux.wire;

enum AttachMode : byte { FromCursor = 0, LiveOnly = 1 }

table KV { key: string (id: 0); value: string (id: 1); }

table SessionRecord {
  id: string (id: 0);                 // uuid v4
  alive: bool (id: 1);
  pid: int32 (id: 2);
  exit_code: int32 (id: 3);           // meaningful when alive == false
  exit_signal: int32 (id: 4);         // 0 if exited normally
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
  rpc_id: uint64 (id: 0);
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
  daemon_pid: int32 (id: 1);
  binary_sha256: string (id: 2);      // drift detection for soft-reboot
  protocol: uint16 (id: 3);           // negotiated version
  session_count: uint32 (id: 4);
  started_at_unix_ms: int64 (id: 5);
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
  patch: [KV] (id: 2);                // KV with empty value => delete key
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
}
table ReadBufferResponse {            // 15
  rpc_id: uint64 (id: 0);
  from_cursor: uint64 (id: 1);
  head_cursor: uint64 (id: 2);
  data: [ubyte] (id: 3);
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
  exit_code: int32 (id: 1);
  exit_signal: int32 (id: 2);
  head_cursor: uint64 (id: 3);        // final cursor
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
  earliest_cursor = tail}`. If `n > head`, start live from `head`.
- `attach LiveOnly`: stream new bytes from current `head`.
- The daemon persists the last cursor it consumed per session; after a daemon
  restart it re-attaches `FromCursor(last)` for a zero-flicker resume.

## Session lifecycle & tombstones

- **Alive** (`alive=true`): accepts `input`, `resize`, `attach`, `kill`.
- On child exit → reaper sets `alive=false`, `exit_code`/`exit_signal`, emits
  `SessionExited` (100) to all attached connections. The **ring buffer is
  retained** (tombstone) so late `readBuffer`/`attach FromCursor` still work.
- **Tombstone** (`alive=false`): allows `readBuffer`, `attach FromCursor`
  (replays to `head`, then completes — no live stream), `updateMetadata`,
  `list`. Rejects `resize` and `input` with `SESSION_NOT_ALIVE`. `kill` is an
  idempotent success (already dead).
- `purge`: frees the ring and removes the record. Rejects a live session with
  `SESSION_ALIVE` (caller must `kill` first).

## Error codes

Authoritative machine codes (message string is advisory only):

| Code | Name | Meaning |
| --- | --- | --- |
| 1 | `UNKNOWN_SESSION` | no session with that id |
| 2 | `SESSION_NOT_ALIVE` | op needs a live session (resize/input) |
| 3 | `SESSION_ALIVE` | op needs a tombstone (purge) |
| 4 | `BUFFER_GAP` | `from_cursor` older than `tail`; see `earliest_cursor` |
| 5 | `INVALID_ARGUMENT` | bad command/cwd/geometry/capacity |
| 6 | `SPAWN_FAILED` | openpty/fork/exec failed |
| 7 | `ALLOC_FAILED` | ring buffer allocation failed (fallible alloc) |
| 8 | `CAPACITY_OUT_OF_RANGE` | ring capacity outside [16 KiB, 32 MiB] |
| 9 | `PROTOCOL_MISMATCH` | no overlapping protocol version in `hello` |
| 10 | `NOT_ATTACHED` | detach/input for a session this conn isn't attached to |
| 11 | `FRAME_TOO_LARGE` | frame exceeded the 16 MiB cap |
| 12 | `INTERNAL` | last resort; must never be a panic |

Codes are append-only like ordinals.

## Liveness

`Heartbeat` (400) flows both directions at ~1 Hz. Either side that sees no frame
of any kind for 3 s treats the connection as broken and tears it down (the
daemon then reconnects with backoff). asmux sends heartbeats from a **dedicated
OS thread**, not a tokio task, so a busy async runtime can't delay them.

## Version negotiation

`hello` carries `[protocol_min, protocol_max]`; asmux replies with a single
`protocol` in that range or `Error{PROTOCOL_MISMATCH}`. v1 is this document.
Because the schema is append-only, most changes bump nothing; a version bump is
reserved for semantic changes to existing ordinals.

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
  abort.
- Every RPC handler returns `Result`; a handler error becomes an `Error` frame,
  never a panic.
- A `vt100` parser is **not** present in asmux — terminal interpretation is the
  daemon's job, so a bad escape sequence can never destabilise the holder.
- No file writes (no logs/PID/state files); state is in-memory, logs go to
  stdout for the daemon to capture.

## Open protocol questions

1. **FlatBuffers Rust toolchain:** `flatc` + `flatbuffers` crate (canonical,
   needs the `flatc` binary at build time) vs pure-Rust `planus` (no external
   toolchain, generates from `.fbs` in `build.rs`). Leaning `planus` for a
   self-contained build.
2. **Attach fan-out to multiple connections** of the same session: allowed
   (broadcast) — confirm that's desired vs single-attacher.
3. **Input authority:** if two clients attach and both send input, asmux
   interleaves bytes as received (no locking). Acceptable?
4. **`kill` on an already-dead session:** idempotent success (proposed) vs
   `UNKNOWN`/`NOT_ALIVE`.
5. **Backpressure:** if a client can't drain `SessionOutput` fast enough, do we
   drop the connection, or drop frames and force a `readBuffer` resync? (acmux
   relies on cursor resync.)
