mod api;
mod auth;
mod backend;
mod config;
mod db;
mod domain;
mod plugins;
mod session_manager;
mod source_control;
mod tls;
mod util;
mod workspace;

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use asm_relay::agent::ResolvedDownstream;

use api::AppState;
use backend::asmux_client::{AsmuxClient, ReconnectEvent};
use backend::native::NativePtyBackend;
use backend::sidecar::SidecarBackend;
use backend::SessionBackend;
use config::{BackendKind, Config};
use db::Db;
use plugins::PluginRegistry;
use session_manager::SessionManager;
use util::now_millis;

#[tokio::main]
async fn main() -> Result<()> {
    // Subcommands run without the tracing subscriber so stdout stays clean.
    match std::env::args().nth(1).as_deref() {
        Some("token") | Some("enrollment-token") => return print_enrollment_token(),
        // Validate TLS material and exit. The service scripts call this before
        // stopping a healthy daemon to apply a new certificate: readable is not
        // the same as valid, and a mismatched key would otherwise turn a config
        // typo into an outage.
        Some("check-tls") => {
            let mut args = std::env::args().skip(2);
            let (cert, key) = match (args.next(), args.next()) {
                (Some(c), Some(k)) => (PathBuf::from(c), PathBuf::from(k)),
                _ => bail!("usage: asm-daemon check-tls <cert.pem> <key.pem>"),
            };
            tls::server_config(&cert, &key)?;
            return Ok(());
        }
        Some("help") | Some("--help") | Some("-h") => {
            print_help();
            return Ok(());
        }
        Some(other) if !other.starts_with('-') => {
            eprintln!("unknown command `{other}` — try `asm-daemon help`");
            std::process::exit(2);
        }
        _ => {}
    }

    init_tracing();

    let config = Config::resolve()?;
    tracing::info!(
        bind = %config.bind,
        data_dir = %config.data_dir.display(),
        "starting asm-daemon"
    );

    let db = Db::open(&config.db_path()).context("opening database")?;

    // Server identity + enrollment token (created once, persisted).
    let (server_id, enrollment_token) = db.get_or_create_identity(
        &auth::gen_server_id(),
        &auth::gen_enrollment_token(),
        now_millis(),
    )?;
    let loopback_only = config.bind.ip().is_loopback();
    let tls_on = config.tls_cert.is_some();
    tracing::info!(server_id = %server_id, "server identity ready");
    tracing::info!("enrollment token for new devices: {enrollment_token}");
    tracing::info!("retrieve it anytime with `asm-daemon token`");
    if loopback_only {
        tracing::info!("bound to loopback: local clients are trusted; remote access via SSH port-forward needs no token");
    } else if tls_on {
        tracing::info!(
            "bound off-loopback ({}) over HTTPS. Remote devices must enroll with the token above.",
            config.bind
        );
    } else {
        tracing::warn!(
            "bound off-loopback ({}) and serving PLAIN HTTP — the device token and all terminal \
             traffic are readable by anyone on this network. Remote devices must enroll with the \
             token above. To encrypt this listener, set ASM_TLS_CERT + ASM_TLS_KEY; otherwise \
             prefer the relay (ASM_RELAY_URL) or an SSH port-forward.",
            config.bind
        );
    }
    if !config.trust_loopback {
        tracing::info!("loopback trust disabled: every request needs a device token");
    }

    let registry = Arc::new(PluginRegistry::with_builtins());
    let worktree_root = config.data_dir.join("worktrees");

    // Select the session backend. The out-of-process holder (asmux) is what makes
    // sessions survive a daemon restart; the native in-process backend does not.
    // For the holder backend, the client's reconnect stream drives a `list`
    // reconcile after every reconnect (catches exits missed while detached).
    let mut reconnect_rx: Option<tokio::sync::broadcast::Receiver<ReconnectEvent>> = None;
    let backend: Arc<dyn SessionBackend> = match config.backend {
        BackendKind::Native => {
            tracing::info!("session backend: native (in-process PTYs; do not survive restart)");
            Arc::new(NativePtyBackend::new(db.events()))
        }
        BackendKind::Sidecar => {
            ensure_asmux(&config).await?;
            let client = AsmuxClient::connect(&config.asmux_socket)
                .await
                .context("connecting to asmux holder")?;
            tracing::info!(
                socket = %config.asmux_socket.display(),
                instance_id = %client.instance_id,
                holder_pid = client.server_pid,
                "session backend: asmux holder (sessions survive daemon restart)"
            );
            // Subscribe before the reconcile consumer spawns so a reconnect
            // during startup is buffered, not lost.
            reconnect_rx = Some(client.reconnect_events());
            Arc::new(SidecarBackend::new(client, db.events(), db.clone()))
        }
    };

    let manager = Arc::new(SessionManager::new(db, registry, backend, worktree_root));

    // Reconcile sessions left live by a previous run: adopt survivors from the
    // holder, or mark them failed/indeterminate. (Native marks them `failed`.)
    if let Err(e) = manager.startup_reconcile().await {
        tracing::error!("startup reconcile failed: {e:#}");
    }

    // Re-reconcile after each daemon↔asmux reconnect (the supervisor has already
    // re-attached the live sessions; this catches exits missed while detached).
    if let Some(mut rx) = reconnect_rx {
        let mgr = manager.clone();
        tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(ReconnectEvent::Connected) => {
                        if let Err(e) = mgr.reconcile_after_reconnect() {
                            tracing::warn!("post-reconnect reconcile failed: {e:#}");
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        });
    }

    let state = AppState {
        manager: manager.clone(),
        config: Arc::new(config.clone()),
        scm: Arc::new(source_control::GitSourceControl),
        started_at: now_millis(),
        node_id: server_id.clone(),
        node_label: config.node_label.clone(),
        attachments: api::ws::Attachments::new(),
    };
    let app = api::router(state);

    // Optionally register outbound to a relay so this daemon is reachable from
    // behind NAT. Relayed traffic is served on a separate loopback tunnel
    // listener that is NOT loopback-trusted (a device token is always required).
    let relay_agent = start_relay_if_configured(&config, &server_id, app.clone()).await?;

    let listener = tokio::net::TcpListener::bind(config.bind)
        .await
        .with_context(|| format!("binding {}", config.bind))?;
    let scheme = if tls_on { "https" } else { "http" };
    tracing::info!("listening on {scheme}://{}", config.bind);

    // Connect-info exposes the peer address so auth can trust loopback. The
    // primary listener stamps `Primary`, so genuine loopback here is trusted.
    let primary_app = app.layer(axum::Extension(auth::ListenerKind::Primary));

    // Race the server against a shutdown signal. We do NOT wait for open
    // connections to drain — a live terminal WebSocket would block that
    // indefinitely. Instead, on signal we kill every live child so no PTY (and,
    // for a future out-of-process/tmux backend, no sidecar) is ever leaked, then
    // exit; open sockets die with the process.
    let serve = async {
        match (&config.tls_cert, &config.tls_key) {
            (Some(cert), Some(key)) => {
                let tls_config = tls::server_config(cert, key)?;
                tls::serve_https(listener, tls_config, primary_app).await
            }
            _ => axum::serve(
                listener,
                primary_app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
            )
            .await
            .context("http server error"),
        }
    };

    tokio::select! {
        res = serve => res?,
        _ = shutdown_signal() => {
            let killed = manager.shutdown_all_live();
            tracing::info!("shutdown signal received; stopped {killed} live session(s)");
        }
    }
    if let Some(handle) = relay_agent {
        handle.abort();
    }
    Ok(())
}

/// If `ASM_RELAY_URL` + `ASM_RELAY_KEY` are set, bind a loopback tunnel listener
/// (serving the same API, but stamped `Tunnel` so it is never loopback-trusted)
/// and spawn the relay agent that registers this daemon outbound and dials data
/// streams back to that listener. Returns the agent task handle so shutdown can
/// abort it.
async fn start_relay_if_configured(
    config: &Config,
    server_id: &str,
    app: axum::Router,
) -> Result<Option<tokio::task::JoinHandle<()>>> {
    let (Some(url), Some(key)) = (config.relay_url.clone(), config.relay_key.clone()) else {
        if config.relay_url.is_some() != config.relay_key.is_some() {
            tracing::warn!(
                "set BOTH ASM_RELAY_URL and ASM_RELAY_KEY to enable the relay; disabled"
            );
        }
        return Ok(None);
    };

    let tunnel_listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .context("binding relay tunnel listener")?;
    let tunnel_addr = tunnel_listener
        .local_addr()
        .context("reading relay tunnel listener address")?;
    let tunnel_app = app.layer(axum::Extension(auth::ListenerKind::Tunnel));
    tokio::spawn(async move {
        if let Err(e) = axum::serve(
            tunnel_listener,
            tunnel_app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .await
        {
            tracing::error!("relay tunnel listener error: {e}");
        }
    });

    // Gateway mode (R4): bridge egress-less downstreams. A probe loop discovers
    // each downstream's node_id/label from its /health and tracks reachability,
    // publishing the live set to the agent (which advertises it to the relay and
    // resolves inbound streams against it). For a leaf, `ds_tx` is dropped here
    // and the watch simply never updates.
    let (ds_tx, ds_rx) = tokio::sync::watch::channel(Vec::<ResolvedDownstream>::new());
    let targets = parse_downstream_targets(&config.relay_downstreams);
    if !targets.is_empty() {
        tracing::info!(
            count = targets.len(),
            "relay gateway mode: probing downstreams"
        );
        tokio::spawn(probe_downstreams_loop(
            targets,
            config.relay_probe_interval,
            ds_tx,
        ));
    }

    // Read the CA bundle here rather than in the agent: a missing or unreadable
    // file is a boot-time misconfiguration, and should fail the boot, not
    // disappear into the agent's reconnect loop.
    let relay_ca = config
        .relay_ca
        .as_ref()
        .map(|p| std::fs::read(p).with_context(|| format!("reading ASM_RELAY_CA {}", p.display())))
        .transpose()?;

    let agent_cfg = asm_relay::agent::AgentConfig {
        relay_url: url.clone(),
        relay_key: key,
        node_id: server_id.to_string(),
        label: config.node_label.clone(),
        local_target: tunnel_addr,
        downstreams: ds_rx,
        relay_ca,
    };
    tracing::info!(relay = %url, node = %server_id, tunnel = %tunnel_addr, "registering with relay");
    Ok(Some(tokio::spawn(asm_relay::agent::run(agent_cfg))))
}

/// Parse `host:port` downstream specs into `(authority, addr)` pairs: the
/// authority string drives the probe URL, the resolved address drives the dial.
/// Unparseable/unresolvable specs are logged and skipped.
fn parse_downstream_targets(specs: &[String]) -> Vec<(String, SocketAddr)> {
    use std::net::ToSocketAddrs;
    let mut out = Vec::new();
    for spec in specs {
        match spec.to_socket_addrs() {
            Ok(mut addrs) => match addrs.next() {
                Some(addr) => out.push((spec.clone(), addr)),
                None => {
                    tracing::warn!("relay downstream `{spec}` resolved to no address; skipping")
                }
            },
            Err(e) => tracing::warn!("invalid relay downstream `{spec}`: {e}; skipping"),
        }
    }
    out
}

/// Periodically probe each downstream's `/health` and publish the reachable,
/// identity-annotated set to the relay agent. A downstream that has answered at
/// least once stays advertised (as `reachable: false`) when a later probe fails,
/// so a transient outage surfaces as offline instead of making the node vanish.
async fn probe_downstreams_loop(
    targets: Vec<(String, SocketAddr)>,
    interval: Duration,
    tx: tokio::sync::watch::Sender<Vec<ResolvedDownstream>>,
) {
    // Last identity learned per address, kept across probe failures.
    let mut known: HashMap<SocketAddr, (String, String)> = HashMap::new();
    let mut tick = tokio::time::interval(interval);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tick.tick().await;
        let mut resolved = Vec::with_capacity(targets.len());
        for (authority, addr) in &targets {
            let url = format!("http://{authority}/health");
            let probed = tokio::task::spawn_blocking(move || probe_health(&url))
                .await
                .unwrap_or(None);
            match probed {
                Some((node_id, label)) => {
                    known.insert(*addr, (node_id.clone(), label.clone()));
                    resolved.push(ResolvedDownstream {
                        node_id,
                        label,
                        addr: *addr,
                        reachable: true,
                    });
                }
                None => {
                    if let Some((node_id, label)) = known.get(addr) {
                        resolved.push(ResolvedDownstream {
                            node_id: node_id.clone(),
                            label: label.clone(),
                            addr: *addr,
                            reachable: false,
                        });
                    }
                    // Never-probed downstream: nothing to advertise yet.
                }
            }
        }
        // Publish only on a real change (the agent re-advertises on each update).
        tx.send_if_modified(|cur| {
            if *cur != resolved {
                *cur = resolved;
                true
            } else {
                false
            }
        });
    }
}

/// Blocking `/health` probe: returns `(node_id, label)` when the downstream
/// answers with a `node_id`. Any error (down, timeout, bad body) ⇒ `None`.
fn probe_health(url: &str) -> Option<(String, String)> {
    let body: serde_json::Value = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(3))
        .build()
        .get(url)
        .call()
        .ok()?
        .into_json()
        .ok()?;
    let node_id = body.get("node_id")?.as_str()?.to_string();
    let label = body
        .get("label")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    Some((node_id, label))
}

/// Poll the holder socket until something answers, or `wait` elapses.
async fn wait_for_asmux(socket: &std::path::Path, wait: Duration) -> bool {
    use tokio::net::UnixStream;

    let deadline = tokio::time::Instant::now() + wait;
    loop {
        if UnixStream::connect(socket).await.is_ok() {
            return true;
        }
        if tokio::time::Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

/// Ensure the asmux holder is reachable, auto-spawning it (detached) if the
/// socket is dead and autospawn is enabled.
///
/// This *waits* rather than probing once. A single connect attempt fails the
/// daemon's whole boot the instant the holder is slow (peer container still
/// starting) or briefly absent — which is how a missing socket became a hard,
/// silent boot failure on 2026-07-12 (two dead boots, and not one ERROR line in
/// the log, because `bail!` from `main` only prints a bare `Error:` to stderr).
async fn ensure_asmux(config: &Config) -> Result<()> {
    use tokio::net::UnixStream;

    if UnixStream::connect(&config.asmux_socket).await.is_ok() {
        return Ok(());
    }

    if !config.asmux_autospawn {
        // The holder is somebody else's job (peer container, service script).
        // Give it a chance to appear instead of dying on the first refusal.
        tracing::warn!(
            socket = %config.asmux_socket.display(),
            wait_ms = config.asmux_wait.as_millis() as u64,
            "asmux holder not answering yet (ASM_ASMUX_AUTOSPAWN=0); waiting for it"
        );
        if wait_for_asmux(&config.asmux_socket, config.asmux_wait).await {
            tracing::info!("asmux holder appeared; continuing");
            return Ok(());
        }
        tracing::error!(
            socket = %config.asmux_socket.display(),
            "no asmux holder at this socket and ASM_ASMUX_AUTOSPAWN=0, so the daemon cannot start. \
             Either the holder is not running, or it is running but its socket was unlinked (an \
             orphan: `asmux probe` says not-Live while its pid is alive). Recover with \
             `scripts/start.sh`, which detects both, or set ASM_ASMUX_AUTOSPAWN=1 to let the daemon \
             spawn one."
        );
        bail!(
            "asmux socket {} is unavailable and ASM_ASMUX_AUTOSPAWN=0",
            config.asmux_socket.display()
        );
    }

    let bin = resolve_asmux_bin(config);
    tracing::info!(bin = %bin.display(), "auto-spawning asmux holder");

    let mut cmd = std::process::Command::new(&bin);
    cmd.env("ASM_RUNTIME_DIR", &config.runtime_dir)
        .env("ASMUX_SOCK", &config.asmux_socket)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    // Detach into its own process group so a signal aimed at the daemon's group
    // (or the daemon dying) does not take the holder with it. Escaping a systemd
    // cgroup needs `systemd-run --user --scope` — see docs/deployment.md (M4).
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    cmd.spawn()
        .with_context(|| format!("spawning asmux binary at {}", bin.display()))?;

    if wait_for_asmux(&config.asmux_socket, config.asmux_wait).await {
        return Ok(());
    }
    tracing::error!(
        socket = %config.asmux_socket.display(),
        bin = %bin.display(),
        "spawned asmux but it never listened. If a live holder already owns this socket it refuses \
         to displace it (by design) — check the asmux log."
    );
    bail!(
        "asmux did not start listening at {}",
        config.asmux_socket.display()
    )
}

/// `ASM_ASMUX_BIN`, else a sibling of the daemon binary, else `asmux` on `PATH`.
fn resolve_asmux_bin(config: &Config) -> PathBuf {
    if let Some(p) = &config.asmux_bin {
        return p.clone();
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(sib) = exe.parent().map(|d| d.join("asmux")) {
            if sib.exists() {
                return sib;
            }
        }
    }
    PathBuf::from("asmux")
}

/// Resolve when the process is asked to terminate (Ctrl-C / SIGINT, or SIGTERM
/// from a service manager). SIGTERM is Unix-only.
async fn shutdown_signal() {
    use tokio::signal;

    let ctrl_c = async {
        let _ = signal::ctrl_c().await;
    };

    #[cfg(unix)]
    let terminate = async {
        match signal::unix::signal(signal::unix::SignalKind::terminate()) {
            Ok(mut s) => {
                s.recv().await;
            }
            Err(e) => {
                tracing::warn!("could not install SIGTERM handler: {e}");
                std::future::pending::<()>().await;
            }
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}

fn init_tracing() {
    use tracing_subscriber::{fmt, EnvFilter};
    let filter =
        EnvFilter::try_from_env("ASM_LOG").unwrap_or_else(|_| EnvFilter::new("info,asm_daemon=debug"));
    fmt().with_env_filter(filter).init();
}

/// `asm-daemon token` — print this host's enrollment token to stdout so a user
/// on the machine (or over SSH) can enroll another device.
fn print_enrollment_token() -> Result<()> {
    let config = Config::resolve()?;
    let db = Db::open(&config.db_path()).context("opening database")?;
    let (_, token) = db.get_or_create_identity(
        &auth::gen_server_id(),
        &auth::gen_enrollment_token(),
        now_millis(),
    )?;
    println!("{token}");
    Ok(())
}

fn print_help() {
    println!("asm-daemon — Agent Session Manager daemon\n");
    println!("USAGE:");
    println!("  asm-daemon           run the daemon");
    println!("  asm-daemon token     print this host's enrollment token");
    println!("  asm-daemon help      show this help\n");
    println!("ENV: ASM_BIND, ASM_DATA_DIR, ASM_CONFIG_DIR, ASM_RUNTIME_DIR, ASM_STATIC_DIR, ASM_LOG");
}
