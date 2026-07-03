//! FlatBuffers wire types for the asmux protocol.
//!
//! This crate is nothing but the code generated from `schema/asmux.fbs` by
//! `planus rust`. It is split out from the `asmux` crate so the holder can keep
//! `#![forbid(unsafe_code)]` on all of its own logic while the generated
//! zero-copy accessors (which use `unsafe` internally, like any FlatBuffers
//! runtime) live here. The same types are consumed by the daemon's client-side
//! backend in a later milestone.
//!
//! Regenerate with:
//! `planus rust -o crates/asmux-wire/src/generated.rs crates/asmux-wire/schema/asmux.fbs`
//!
//! The schema is FROZEN once shipped — see `docs/asmux-protocol.md`.
#[allow(clippy::all, clippy::pedantic, clippy::nursery, dead_code, unused)]
mod generated;

/// The `asmux.wire` message namespace (all frozen tables and enums).
pub use generated::asmux::wire;
