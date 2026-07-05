#![forbid(unsafe_code)]

//! asm-relay — the public rendezvous for the Agent Session Manager.
//!
//! A NAT'd daemon dials the relay outbound over WSS and holds the connection
//! open; the relay multiplexes client traffic back down it. The relay routes by
//! opaque path prefix (`/n/<node_id>/...`) and **never parses the daemon API** —
//! client and daemon speak the daemon protocol end to end through the tunnel.
//!
//! Two independent credentials meet here: the *relay access key* (gates use of
//! the relay) and the daemon *device token* (validated end to end by the target
//! daemon, passed through untouched). The relay authenticates only the former.
//!
//! The wire contract is frozen in [`protocol`] and mirrors
//! `docs/connectivity-execution-plan.md`. The crate is a library (the shared
//! protocol plus the reusable node-side agent, which the daemon embeds in R2)
//! with a thin binary (the standalone relay server).

pub mod agent;
pub mod protocol;
pub mod server;
pub mod transport;
