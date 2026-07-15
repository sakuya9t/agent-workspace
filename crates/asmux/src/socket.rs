//! Socket-path ownership: never unlink a socket somebody is still holding.
//!
//! asmux binds a well-known UDS path. Binding it blindly (`remove_file` then
//! `bind`) means any *second* asmux — a test that inherited a stale `ASMUX_SOCK`,
//! a stray `cargo run`, a misconfigured unit — silently unlinks a **live**
//! holder's socket and orphans every PTY it is holding. The victim never
//! notices: its listener fd stays open, so it logs nothing while becoming
//! unreachable, and its sessions die the moment anyone restarts it.
//!
//! That is not hypothetical. On 2026-07-12 the durable-restart e2e test
//! inherited the dev host's ambient `ASMUX_SOCK`, took the real holder's path,
//! and six live sessions were lost. See docs/durable-sessions.md.
//!
//! So: probe before you unlink. This is part of the never-crash discipline —
//! the holder must not lose PTYs, and that includes losing them to *us*.

use std::io::ErrorKind;
use std::path::Path;

use anyhow::{bail, Context, Result};
use tokio::net::{UnixListener, UnixStream};

/// What is actually at a socket path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SocketState {
    /// Nothing there — a clean bind.
    Free,
    /// A socket file exists but nobody answers: the owner died without cleaning
    /// up. Safe to unlink.
    Stale,
    /// Someone answered. A live holder owns this path.
    Live,
}

/// Classify a socket path by trying to connect to it.
///
/// A live holder is the only case that can *answer*. `NotFound` means the path
/// is clear; anything else (in practice `ECONNREFUSED`) means the file outlived
/// the process that bound it.
pub async fn probe(path: &Path) -> SocketState {
    match UnixStream::connect(path).await {
        Ok(_) => SocketState::Live,
        Err(e) if e.kind() == ErrorKind::NotFound => SocketState::Free,
        Err(_) => SocketState::Stale,
    }
}

/// Probe, then *confirm* a `Live` verdict before acting on it.
///
/// A single probe can report a phantom `Live` for a listener that was already
/// closed. Mechanism (proven empirically, 2026-07-12): **fd inheritance across
/// fork**. Any child forked while the listener fd is open inherits a copy of the
/// fd table, so the socket stays LISTENING after the owner closes its fd — until
/// the child's `execve` closes the copy (all our sockets are CLOEXEC; verified:
/// an exec'd child does NOT hold it). The phantom is therefore bounded by the
/// [fork, exec) window: µs–ms, stretched under load. asmux forks PTY children as
/// its core job, so the window recurs constantly; a bare bind→close→connect loop
/// with no forking never reproduces it (0 in 244k iterations), while the same
/// loop next to fork+exec churn hits it ~1.4% of the time.
///
/// Acting on a phantom would make a restarting asmux refuse to reclaim its dead
/// predecessor's socket and fail to boot. A real holder answers every probe; a
/// phantom cannot outlive its fork window. So re-probe across a span far longer
/// than any [fork, exec) gap, and only believe `Live` if it holds throughout.
pub async fn probe_confirmed(path: &Path) -> SocketState {
    const CONFIRMATIONS: usize = 3;
    const GAP: std::time::Duration = std::time::Duration::from_millis(100);

    let mut state = probe(path).await;
    for _ in 1..CONFIRMATIONS {
        if state != SocketState::Live {
            return state; // Free/Stale are stable — nothing to double-check.
        }
        tokio::time::sleep(GAP).await;
        state = probe(path).await;
    }
    state
}

/// Decide whether `path` may be unlinked and rebound.
///
/// `Ok(state)` means the caller may proceed. It errors only when a **live**
/// holder owns the path and `takeover` is unset — displacing a holder destroys
/// its sessions, so it must be asked for explicitly, never inferred.
pub async fn ensure_bindable(path: &Path, takeover: bool) -> Result<SocketState> {
    let state = probe_confirmed(path).await;
    match state {
        SocketState::Live if !takeover => bail!(
            "a live asmux already owns {} — refusing to displace it and orphan its sessions.\n\
             Point ASMUX_SOCK at a private path (tests must sandbox it), or set \
             ASMUX_TAKEOVER=1 to override deliberately.",
            path.display()
        ),
        SocketState::Live => tracing::warn!(
            socket = %path.display(),
            "ASMUX_TAKEOVER=1: displacing the live holder on this socket; its sessions will be orphaned"
        ),
        SocketState::Stale => tracing::info!(
            socket = %path.display(),
            "clearing a stale socket file (no listener answered)"
        ),
        SocketState::Free => {}
    }
    Ok(state)
}

/// Bind the holder socket at `path` with owner-only perms, after checking that
/// we are allowed to (see [`ensure_bindable`]). Returns the listener and the
/// inode we bound, which [`was_displaced`] uses to notice if the path is later
/// unlinked or replaced underneath us.
pub async fn bind(path: &Path, takeover: bool) -> Result<(UnixListener, u64)> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).with_context(|| format!("create runtime dir {dir:?}"))?;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
            .with_context(|| format!("chmod 0700 {dir:?}"))?;
    }

    ensure_bindable(path, takeover).await?;

    let _ = std::fs::remove_file(path);
    let listener = UnixListener::bind(path).with_context(|| format!("bind UDS at {path:?}"))?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
        .with_context(|| format!("chmod 0600 {path:?}"))?;

    let ino = std::fs::metadata(path)
        .with_context(|| format!("stat {path:?} after bind"))?
        .ino();
    Ok((listener, ino))
}

/// Has our socket path been taken away from us?
///
/// The holder's listener fd keeps working after someone unlinks its path, so the
/// process notices *nothing* while becoming unreachable — that is precisely how
/// six sessions were lost on 2026-07-12. Comparing the path's current inode to
/// the one we bound is how we detect it: gone (`None`) or a different inode both
/// mean the path no longer routes to us.
pub fn was_displaced(path: &Path, bound_ino: u64) -> bool {
    use std::os::unix::fs::MetadataExt;

    match std::fs::metadata(path) {
        Ok(m) => m.ino() != bound_ino,
        Err(_) => true, // unlinked
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    // A *blocking* std listener on purpose: its drop closes the fd synchronously.
    // tokio's UnixListener defers the close to the reactor, so dropping one and
    // immediately probing the path races (it can still answer, reading as Live).
    // The lifecycle under test is the socket's, not tokio's — keep tokio out of it.
    use std::os::unix::net::UnixListener;

    fn tmp_sock(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("asmux-sock-{tag}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("asmux.sock")
    }

    #[tokio::test]
    async fn absent_path_is_free_and_bindable() {
        let sock = tmp_sock("free");
        assert_eq!(probe(&sock).await, SocketState::Free);
        assert_eq!(ensure_bindable(&sock, false).await.unwrap(), SocketState::Free);
    }

    #[tokio::test]
    async fn live_holder_is_never_displaced_implicitly() {
        let sock = tmp_sock("live");
        // The "real" holder. It never calls accept() — it does not need to: the
        // kernel completes our connect() from the backlog, which is exactly how a
        // busy asmux still reads as alive.
        let _holder = UnixListener::bind(&sock).unwrap();

        assert_eq!(probe(&sock).await, SocketState::Live);

        // THE INCIDENT: a second asmux aimed at a live socket must refuse to bind.
        let err = ensure_bindable(&sock, false).await.unwrap_err().to_string();
        assert!(err.contains("refusing to displace"), "unexpected error: {err}");

        // ...and the victim must be untouched: still there, still answering.
        assert!(sock.exists(), "the live holder's socket was unlinked");
        assert_eq!(probe(&sock).await, SocketState::Live);
    }

    #[tokio::test]
    async fn live_holder_can_be_displaced_deliberately() {
        let sock = tmp_sock("takeover");
        let _holder = UnixListener::bind(&sock).unwrap();
        assert_eq!(ensure_bindable(&sock, true).await.unwrap(), SocketState::Live);
    }

    #[tokio::test]
    async fn stale_socket_is_reclaimable() {
        let sock = tmp_sock("stale");
        // Bind, then close without unlinking — what a SIGKILLed holder leaves
        // behind. The file survives; nobody answers. This must NOT be mistaken
        // for a live holder, or a crashed asmux could never restart.
        drop(UnixListener::bind(&sock).unwrap());
        assert!(sock.exists(), "a closed listener should leave its socket file");

        // A bare `probe()` here is genuinely flaky: sibling tests fork PTY children,
        // and a child forked while our listener fd was open keeps the socket alive
        // through its [fork, exec) window (see `probe_confirmed`). `ensure_bindable`
        // confirms a `Live` verdict precisely so that transient cannot block a
        // reclaim — the behaviour worth asserting: a dead holder's socket IS
        // reclaimable.
        assert_eq!(ensure_bindable(&sock, false).await.unwrap(), SocketState::Stale);
    }

    // TEMPORARY diagnostic (to be removed): the raw, unconfirmed probe at the exact
    // site where the phantom `Live` appears, capturing what the KERNEL thinks at
    // that instant.
    #[tokio::test]
    async fn a_dying_holder_does_not_block_its_successor() {
        // The restart-after-crash path: a phantom `Live` must not make the next
        // asmux refuse to boot.
        let sock = tmp_sock("dying");
        for _ in 0..25 {
            drop(UnixListener::bind(&sock).unwrap());
            let state = ensure_bindable(&sock, false)
                .await
                .expect("a closed listener must never be mistaken for a live holder");
            assert_eq!(state, SocketState::Stale);
            let _ = std::fs::remove_file(&sock);
        }
    }

    #[tokio::test]
    async fn fork_window_phantom_does_not_survive_confirmation() {
        // Regression for the fork-window phantom (see `probe_confirmed`). A child
        // forked while a listener fd is open inherits it, keeping the socket
        // LISTENING briefly after the owner closes it — so a single probe can say
        // `Live` for a dead listener. This originally surfaced as a ~1-in-8 flake
        // in this very module, caused by *sibling tests* forking PTY children; here
        // we create that fork churn deliberately instead of depending on the test
        // scheduler, and assert the confirmed probe never falls for it.
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let churn = {
            let stop = stop.clone();
            std::thread::spawn(move || {
                while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                    // fork+exec, over and over — each child is a [fork, exec) window
                    let _ = std::process::Command::new("/bin/true").status();
                }
            })
        };

        let sock = tmp_sock("forkchurn");
        for _ in 0..150 {
            drop(UnixListener::bind(&sock).unwrap());
            let state = ensure_bindable(&sock, false)
                .await
                .expect("a phantom Live must never block a reclaim");
            assert_eq!(state, SocketState::Stale);
            let _ = std::fs::remove_file(&sock);
        }

        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        let _ = churn.join();
    }
}
