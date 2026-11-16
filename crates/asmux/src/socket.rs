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
//!
//! # Windows
//!
//! tokio has no AF_UNIX on Windows, so there the same framing runs over a
//! **named pipe** (the transport docs/asmux-protocol.md names as the native
//! choice). Everything still speaks in socket *paths* — `ASMUX_SOCK`,
//! `<runtime_dir>/asmux.sock` — and [`pipe_name`] maps a path to its pipe, so
//! the sandboxing the tests and service scripts do with private runtime dirs
//! carries over unchanged. The ownership rules map like this:
//!
//! - A pipe exists only while its server holds a handle to it. There is no file
//!   left behind by a dead holder, so [`SocketState::Stale`] never occurs: a
//!   probe sees `Free` or `Live`.
//! - The first instance is created with `FILE_FLAG_FIRST_PIPE_INSTANCE`, so the
//!   kernel refuses a second holder outright. Nobody can unlink a pipe out from
//!   under its owner either, so [`was_displaced`] is always `false` — and a
//!   takeover is impossible, which [`bind`] reports instead of attempting.
//! - Remote (SMB) clients are rejected. The default pipe DACL grants full
//!   access to the creating user, SYSTEM and Administrators and only *read*
//!   access to everyone else. A client needs write access to send `hello`, so
//!   no other non-admin account can drive the holder — the practical
//!   equivalent of the `0600` socket. (Squatting the name *before* the holder
//!   starts is possible on a shared machine; see docs/security-followups.md.)

use std::path::Path;

use anyhow::{bail, Result};

pub use imp::{
    bind, connect, endpoint, probe, split_client, split_server, was_displaced, ClientRead,
    ClientStream, ClientWrite, Listener, ServerRead, ServerStream, ServerWrite,
};
#[cfg(windows)]
pub use imp::pipe_name;

/// What is actually at a socket path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SocketState {
    /// Nothing there — a clean bind.
    Free,
    /// A socket file exists but nobody answers: the owner died without cleaning
    /// up. Safe to unlink. (Unix only — a named pipe vanishes with its owner.)
    Stale,
    /// Someone answered. A live holder owns this path.
    Live,
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
/// (Windows has no fork, but the confirmation costs nothing there either.)
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

#[cfg(unix)]
mod imp {
    use std::io::ErrorKind;
    use std::path::Path;

    use anyhow::{Context, Result};
    use tokio::net::{UnixListener, UnixStream};

    use super::{ensure_bindable, SocketState};

    pub type ServerStream = UnixStream;
    pub type ServerRead = tokio::net::unix::OwnedReadHalf;
    pub type ServerWrite = tokio::net::unix::OwnedWriteHalf;
    pub type ClientStream = UnixStream;
    pub type ClientRead = tokio::net::unix::OwnedReadHalf;
    pub type ClientWrite = tokio::net::unix::OwnedWriteHalf;

    /// The holder's accept side: a bound UDS.
    pub struct Listener(UnixListener);

    impl Listener {
        pub async fn accept(&mut self) -> std::io::Result<(ServerStream, tokio::net::unix::SocketAddr)> {
            self.0.accept().await
        }
    }

    pub fn split_server(stream: ServerStream) -> (ServerRead, ServerWrite) {
        stream.into_split()
    }

    pub fn split_client(stream: ClientStream) -> (ClientRead, ClientWrite) {
        stream.into_split()
    }

    /// Dial the holder at `path`.
    pub async fn connect(path: &Path) -> std::io::Result<ClientStream> {
        UnixStream::connect(path).await
    }

    /// How to name this socket in a log line.
    pub fn endpoint(path: &Path) -> String {
        path.display().to_string()
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

    /// Bind the holder socket at `path` with owner-only perms, after checking that
    /// we are allowed to (see [`ensure_bindable`]). Returns the listener and the
    /// inode we bound, which [`was_displaced`] uses to notice if the path is later
    /// unlinked or replaced underneath us.
    pub async fn bind(path: &Path, takeover: bool) -> Result<(Listener, u64)> {
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
        Ok((Listener(listener), ino))
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
}

#[cfg(windows)]
mod imp {
    use std::io::ErrorKind;
    use std::path::{Path, PathBuf};
    use std::time::Duration;

    use anyhow::{bail, Context, Result};
    use tokio::net::windows::named_pipe::{
        ClientOptions, NamedPipeClient, NamedPipeServer, ServerOptions,
    };

    use super::{ensure_bindable, SocketState};

    pub type ServerStream = NamedPipeServer;
    pub type ServerRead = tokio::io::ReadHalf<NamedPipeServer>;
    pub type ServerWrite = tokio::io::WriteHalf<NamedPipeServer>;
    pub type ClientStream = NamedPipeClient;
    pub type ClientRead = tokio::io::ReadHalf<NamedPipeClient>;
    pub type ClientWrite = tokio::io::WriteHalf<NamedPipeClient>;

    const PIPE_PREFIX: &str = r"\\.\pipe\";
    /// `ERROR_PIPE_BUSY`: every instance is connected and the server has not
    /// created the next one yet. The pipe exists — its owner is alive.
    const ERROR_PIPE_BUSY: i32 = 231;
    /// How long a dial keeps retrying a busy pipe (the server opens the next
    /// instance right after each connect, so busy windows are microseconds).
    const BUSY_RETRIES: u32 = 100;
    const BUSY_RETRY_GAP: Duration = Duration::from_millis(20);

    /// The holder's accept side. A named pipe has no listening socket: each
    /// client connects to its own *instance*, so the listener keeps one instance
    /// waiting at all times and hands it off once a client takes it.
    ///
    /// The waiting instance is what keeps the name claimed — between two
    /// connections there must never be a moment with zero instances, or a probe
    /// would read `Free` and a second holder could start. So the replacement is
    /// always created *before* the connected instance is handed off.
    pub struct Listener {
        name: String,
        next: NamedPipeServer,
    }

    impl Listener {
        /// Wait for the next client. Cancellation-safe (it is raced in a
        /// `select!`): until a client arrives, the waiting instance stays put.
        pub async fn accept(&mut self) -> std::io::Result<(ServerStream, ())> {
            if let Err(e) = self.next.connect().await {
                // A client that came and went before we saw it can leave this
                // instance unusable. Replace it (new first, so the name never
                // lapses) and report; the caller's loop accepts again.
                let fresh = instance(&self.name, false)?;
                drop(std::mem::replace(&mut self.next, fresh));
                tokio::time::sleep(Duration::from_millis(50)).await;
                return Err(e);
            }
            let fresh = match instance(&self.name, false) {
                Ok(f) => f,
                Err(e) => {
                    // The connected client stays parked in `next`: the next
                    // `connect()` returns at once and we try again to open its
                    // successor. Back off so a persistent failure doesn't spin.
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    return Err(e);
                }
            };
            Ok((std::mem::replace(&mut self.next, fresh), ()))
        }
    }

    fn instance(name: &str, first: bool) -> std::io::Result<NamedPipeServer> {
        ServerOptions::new()
            .first_pipe_instance(first)
            .reject_remote_clients(true)
            .create(name)
    }

    pub fn split_server(stream: ServerStream) -> (ServerRead, ServerWrite) {
        tokio::io::split(stream)
    }

    pub fn split_client(stream: ClientStream) -> (ClientRead, ClientWrite) {
        tokio::io::split(stream)
    }

    /// Dial the holder that owns `path`'s pipe, riding out the brief busy window
    /// between two of its instances.
    pub async fn connect(path: &Path) -> std::io::Result<ClientStream> {
        let name = pipe_name(path);
        let mut tries: u32 = 0;
        loop {
            match ClientOptions::new().open(&name) {
                Err(e) if e.raw_os_error() == Some(ERROR_PIPE_BUSY) && tries < BUSY_RETRIES => {
                    tries = tries.saturating_add(1);
                    tokio::time::sleep(BUSY_RETRY_GAP).await;
                }
                other => return other,
            }
        }
    }

    /// How to name this socket in a log line: the path *and* the pipe it maps to.
    pub fn endpoint(path: &Path) -> String {
        format!("{} (pipe {})", path.display(), pipe_name(path))
    }

    /// The named pipe that stands in for the socket at `path`.
    ///
    /// An explicit `\\.\pipe\…` name is used as is. Any other path is hashed:
    /// pipe names are a flat namespace, and the point is only that the same
    /// *location* always yields the same pipe while different runtime dirs never
    /// collide. The location is normalised first — the runtime dir canonicalised
    /// (it exists by the time anyone dials: the holder creates it before
    /// binding, the daemon and scripts when they resolve it), separators unified,
    /// case folded — because MSYS hands native programs `C:/x/asmux.sock` while
    /// Rust code joins `C:\x\asmux.sock`, and both must reach one holder.
    ///
    /// FNV-1a rather than `DefaultHasher`, whose output is not promised to be
    /// stable across Rust releases: a holder outlives daemon upgrades, so a
    /// rebuilt daemon must still compute its predecessor's pipe name.
    pub fn pipe_name(path: &Path) -> String {
        let raw = path.to_string_lossy();
        if raw.len() > PIPE_PREFIX.len()
            && raw
                .get(..PIPE_PREFIX.len())
                .is_some_and(|p| p.eq_ignore_ascii_case(PIPE_PREFIX))
        {
            return raw.into_owned();
        }
        format!("{PIPE_PREFIX}asmux-{:016x}", fnv1a64(location_key(path).as_bytes()))
    }

    fn location_key(path: &Path) -> String {
        let abs = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
        let located: PathBuf = match (abs.parent(), abs.file_name()) {
            (Some(dir), Some(file)) => match std::fs::canonicalize(dir) {
                Ok(dir) => dir.join(file),
                Err(_) => abs.clone(),
            },
            _ => abs.clone(),
        };
        let s = located.to_string_lossy().replace('/', "\\");
        let s = if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
            format!(r"\\{rest}")
        } else if let Some(rest) = s.strip_prefix(r"\\?\") {
            rest.to_string()
        } else {
            s
        };
        s.to_lowercase()
    }

    fn fnv1a64(bytes: &[u8]) -> u64 {
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for b in bytes {
            hash ^= u64::from(*b);
            hash = hash.wrapping_mul(0x0100_0000_01b3);
        }
        hash
    }

    /// Classify a socket path by trying to open its pipe.
    ///
    /// `NotFound` means no instance exists, so no holder owns it. Anything else
    /// — connected, `ERROR_PIPE_BUSY`, or access denied (another account's
    /// pipe) — means a live process holds the name, and we could not create it
    /// anyway.
    pub async fn probe(path: &Path) -> SocketState {
        match ClientOptions::new().open(pipe_name(path)) {
            Ok(_) => SocketState::Live,
            Err(e) if e.kind() == ErrorKind::NotFound => SocketState::Free,
            Err(_) => SocketState::Live,
        }
    }

    /// Create the holder's pipe for `path`, after checking that we are allowed to
    /// (see [`ensure_bindable`]). The returned "inode" is meaningless here — see
    /// [`was_displaced`].
    pub async fn bind(path: &Path, takeover: bool) -> Result<(Listener, u64)> {
        if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
            if !path.to_string_lossy().to_ascii_lowercase().starts_with(PIPE_PREFIX) {
                std::fs::create_dir_all(dir)
                    .with_context(|| format!("create runtime dir {dir:?}"))?;
            }
        }

        // Never ask for a takeover: there is no way to perform one, so a live
        // owner is an error either way — just a more specific one if asked.
        if let Err(e) = ensure_bindable(path, false).await {
            if takeover {
                bail!(
                    "a live asmux owns {}, and a named pipe cannot be taken from its owner \
                     (ASMUX_TAKEOVER has no effect on Windows). Stop that holder first.",
                    endpoint(path)
                );
            }
            return Err(e);
        }

        let name = pipe_name(path);
        let first = instance(&name, true).with_context(|| {
            format!(
                "create named pipe {name} for {} (another process may own it)",
                path.display()
            )
        })?;
        Ok((Listener { name, next: first }, 0))
    }

    /// A named pipe cannot be unlinked or replaced while its owner holds it, so
    /// the failure mode the Unix watchdog exists for cannot happen here.
    pub fn was_displaced(_path: &Path, _bound_ino: u64) -> bool {
        false
    }
}

#[cfg(all(test, unix))]
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

#[cfg(all(test, windows))]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

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
    async fn live_holder_is_never_displaced() {
        let sock = tmp_sock("live");
        let (_holder, _) = bind(&sock, false).await.unwrap();
        assert_eq!(probe(&sock).await, SocketState::Live);

        // The incident guard: a second holder must refuse...
        let err = ensure_bindable(&sock, false).await.unwrap_err().to_string();
        assert!(err.contains("refusing to displace"), "unexpected error: {err}");
        let err = bind(&sock, false).await.err().expect("second bind must fail").to_string();
        assert!(err.contains("refusing to displace"), "unexpected error: {err}");
        // ...and a deliberate takeover is reported as impossible, not attempted.
        let err = bind(&sock, true).await.err().expect("takeover must fail").to_string();
        assert!(err.contains("cannot be taken from its owner"), "unexpected error: {err}");

        // The victim is untouched.
        assert_eq!(probe(&sock).await, SocketState::Live);
    }

    #[tokio::test]
    async fn a_dead_holders_pipe_is_free_for_its_successor() {
        let sock = tmp_sock("dying");
        for _ in 0..10 {
            let (holder, _) = bind(&sock, false).await.unwrap();
            drop(holder);
            // No file is left behind, so there is no Stale state to reclaim.
            assert_eq!(ensure_bindable(&sock, false).await.unwrap(), SocketState::Free);
        }
    }

    #[tokio::test]
    async fn every_spelling_of_a_location_names_one_pipe() {
        let sock = tmp_sock("spelling");
        let forward = std::path::PathBuf::from(sock.to_string_lossy().replace('\\', "/"));
        let shouty = std::path::PathBuf::from(sock.to_string_lossy().to_uppercase());
        assert_eq!(pipe_name(&sock), pipe_name(&forward));
        assert_eq!(pipe_name(&sock), pipe_name(&shouty));
        // Different runtime dirs must never share a holder.
        assert_ne!(pipe_name(&sock), pipe_name(&tmp_sock("other")));
        // An explicit pipe name is taken as given.
        let explicit = std::path::Path::new(r"\\.\pipe\my-asmux");
        assert_eq!(pipe_name(explicit), r"\\.\pipe\my-asmux");
    }

    #[tokio::test]
    async fn concurrent_clients_are_each_accepted() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let sock = tmp_sock("accept");
        let (mut listener, _) = bind(&sock, false).await.unwrap();
        tokio::spawn(async move {
            while let Ok((stream, ())) = listener.accept().await {
                tokio::spawn(async move {
                    let (mut rd, mut wr) = split_server(stream);
                    let mut b = [0u8; 1];
                    if rd.read_exact(&mut b).await.is_ok() {
                        let _ = wr.write_all(&b).await;
                    }
                });
            }
        });

        let mut clients = Vec::new();
        for i in 0..8u8 {
            clients.push(tokio::spawn({
                let sock = sock.clone();
                async move {
                    let (mut rd, mut wr) = split_client(connect(&sock).await.unwrap());
                    wr.write_all(&[i]).await.unwrap();
                    let mut b = [0u8; 1];
                    rd.read_exact(&mut b).await.unwrap();
                    assert_eq!(b[0], i);
                }
            }));
        }
        for c in clients {
            c.await.unwrap();
        }
        // The name stayed claimed throughout.
        assert_eq!(probe(&sock).await, SocketState::Live);
    }
}
