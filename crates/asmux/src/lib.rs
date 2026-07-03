//! asmux — the out-of-process PTY-holding session sidecar.
//!
//! One process holds every session's PTY master fd plus a raw-byte ring buffer,
//! so the sessions survive `asm-daemon` restarts (crash / upgrade / hash
//! rotation). The daemon is the *client*; asmux is the *server*. The wire
//! contract is frozen in `docs/asmux-protocol.md`; the mirror schema is
//! `schema/asmux.fbs`.
//!
//! # Never-crash discipline
//!
//! This process holds *everyone's* PTYs, so a panic here loses all live
//! sessions. The lints below (from the frozen contract's "Never-crash
//! invariants") make the compiler enforce the discipline: no `unsafe`, no
//! `unwrap`/`expect`/`panic`, no raw indexing or unchecked integer arithmetic.
//! Ring growth uses fallible allocation and a hard total-memory cap; every RPC
//! handler returns a `Result` that becomes an `Error` frame, never a panic.
#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented,
    clippy::indexing_slicing,
    clippy::integer_division,
    clippy::arithmetic_side_effects
)]

/// The frozen FlatBuffers message types (`asmux.wire` namespace). The generated
/// zero-copy accessors use `unsafe` internally, so they live in the sibling
/// `asmux-wire` crate; this crate stays `forbid(unsafe_code)`.
pub use asmux_wire::wire;

pub mod frame;
pub mod ring;
pub mod session;
pub mod registry;
pub mod server;

/// Negotiated wire-protocol version implemented by this build. v1 == the frozen
/// `docs/asmux-protocol.md`.
pub const PROTOCOL_V1: u16 = 1;

/// Hard ceiling on a single frame (`length` field counts tag+ordinal+body).
pub const MAX_FRAME_BYTES: usize = 16 * 1024 * 1024;

/// Default per-session ring capacity when `CreateRequest.ring_capacity == 0`.
pub const RING_DEFAULT_BYTES: u64 = 2 * 1024 * 1024;
/// Inclusive lower bound on a per-session ring capacity.
pub const RING_MIN_BYTES: u64 = 16 * 1024;
/// Inclusive upper bound on a per-session ring capacity.
pub const RING_MAX_BYTES: u64 = 32 * 1024 * 1024;

/// Default hard cap on the sum of all ring capacities (live + tombstone).
/// Overridable via `ASMUX_MEMORY_LIMIT` (bytes). A `create` that would breach
/// it evicts tombstones LRU, then fails `MEMORY_LIMIT`.
pub const MEMORY_LIMIT_DEFAULT_BYTES: u64 = 256 * 1024 * 1024;

/// Per-session bounded input queue. Overflow drops input and emits
/// `INPUT_OVERFLOW`; asmux never blocks the connection reader on a slow child.
pub const INPUT_QUEUE_BYTES: usize = 1024 * 1024;

/// Heartbeat cadence and idle-teardown watchdog (see contract → Liveness).
pub const HEARTBEAT_INTERVAL_MS: u64 = 1000;
pub const WATCHDOG_IDLE_MS: u64 = 10_000;
