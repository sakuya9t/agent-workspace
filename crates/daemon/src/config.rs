use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use directories::ProjectDirs;

/// Runtime configuration and platform-specific directory resolution.
///
/// This is the seed of the Platform abstraction from the architecture doc:
/// data dir, config dir, and a per-user runtime dir (future sidecar sockets).
/// Which session backend the daemon drives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendKind {
    /// In-process native PTYs (default). PTYs die with the daemon.
    Native,
    /// Out-of-process `asmux` holder. Sessions survive daemon restart (adopt).
    Sidecar,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub bind: SocketAddr,
    pub data_dir: PathBuf,
    /// Reserved platform dirs: `config_dir` for future config files,
    /// `runtime_dir` for per-user sidecar sockets. Created at resolve time.
    #[allow(dead_code)]
    pub config_dir: PathBuf,
    pub runtime_dir: PathBuf,
    /// Optional path to a built web client (client/dist) for packaged serving.
    pub static_dir: Option<PathBuf>,
    /// PEM certificate chain + private key (`ASM_TLS_CERT` / `ASM_TLS_KEY`). Set
    /// both and the daemon serves **https/wss** on `bind` instead of plain HTTP,
    /// which is what makes a direct LAN client (`https://host:4600`) safe.
    pub tls_cert: Option<PathBuf>,
    pub tls_key: Option<PathBuf>,
    /// Trust loopback peers without a token (`ASM_TRUST_LOOPBACK=0` disables).
    ///
    /// Loopback trust is what makes the local client and SSH port-forwards work
    /// with no token. It is also a trap behind a **same-host reverse proxy**:
    /// the proxy connects from `127.0.0.1`, so every request it forwards would
    /// be trusted and the daemon's auth would be silently switched off. Anyone
    /// fronting the daemon that way must set this to 0.
    pub trust_loopback: bool,
    /// Selected session backend (`ASM_BACKEND=native|sidecar`, default native).
    pub backend: BackendKind,
    /// asmux UDS path (`ASMUX_SOCK` override, else `runtime_dir/asmux.sock`).
    pub asmux_socket: PathBuf,
    /// Auto-spawn asmux if its socket is dead (`ASM_ASMUX_AUTOSPAWN=0` disables,
    /// e.g. when asmux is a peer container the daemon only connects to).
    pub asmux_autospawn: bool,
    /// How long to wait for the holder's socket to appear before giving up
    /// (`ASM_ASMUX_WAIT_MS`, default 15000).
    ///
    /// A single connect attempt is wrong in both deployments: as a peer
    /// container asmux may still be starting, and locally the socket may be
    /// briefly absent. Dying on the first refused connect is what turned a
    /// missing socket into a hard boot failure on 2026-07-12.
    pub asmux_wait: Duration,
    /// Explicit asmux binary path (`ASM_ASMUX_BIN`); else a sibling of the
    /// daemon binary, else `asmux` on `PATH`.
    pub asmux_bin: Option<PathBuf>,
    /// Relay base URL to register outbound to (`ASM_RELAY_URL`, e.g.
    /// `wss://relay.example.com`). When set (with a key), the daemon dials the
    /// relay and serves relayed traffic on a loopback tunnel listener so it is
    /// reachable from behind NAT. See docs/connectivity-execution-plan.md.
    pub relay_url: Option<String>,
    /// Relay access key (`ASM_RELAY_KEY`). Required alongside `relay_url`.
    pub relay_key: Option<String>,
    /// Extra PEM trust anchors for the relay's TLS certificate (`ASM_RELAY_CA`),
    /// for a self-hosted relay behind a private CA or a self-signed cert. Unset
    /// trusts the public web PKI, which is all an ACME-certificated relay needs.
    pub relay_ca: Option<PathBuf>,
    /// Human label advertised to the relay and shown in clients
    /// (`ASM_NODE_LABEL`, default: hostname).
    pub node_label: String,
    /// Egress-less downstreams this daemon bridges as a gateway
    /// (`ASM_RELAY_DOWNSTREAMS`, comma-separated `host:port`). Each is probed on
    /// `/health` for its `node_id`/`label`, then advertised to the relay so a
    /// client can reach it through this gateway (R4). Empty ⇒ a leaf node.
    pub relay_downstreams: Vec<String>,
    /// How often to re-probe each downstream's `/health`
    /// (`ASM_RELAY_PROBE_INTERVAL_MS`, default 5000).
    pub relay_probe_interval: Duration,
}

impl Config {
    pub fn resolve() -> Result<Self> {
        // An off-loopback bind without ASM_TLS_CERT is plaintext, but it is also
        // an explicit act: you don't reach `ASM_BIND=0.0.0.0` by accident. It
        // warns loudly at startup (see main.rs) rather than refusing — demanding
        // a second flag to confirm the first one is ceremony, and it broke every
        // container and LAN deployment that had legitimately chosen it. Anyone
        // who wants that listener encrypted sets ASM_TLS_CERT/ASM_TLS_KEY.
        let bind: SocketAddr = std::env::var("ASM_BIND")
            .unwrap_or_else(|_| "127.0.0.1:4600".to_string())
            .parse()
            .context("invalid ASM_BIND address")?;

        let (data_dir, config_dir, runtime_dir) = match ProjectDirs::from("dev", "agentsm", "asm") {
            Some(dirs) => {
                let data = dirs.data_dir().to_path_buf();
                let config = dirs.config_dir().to_path_buf();
                // runtime_dir is None on macOS/Windows; fall back to data_dir/run.
                let runtime = dirs
                    .runtime_dir()
                    .map(|p| p.to_path_buf())
                    .unwrap_or_else(|| data.join("run"));
                (data, config, runtime)
            }
            None => {
                let base = PathBuf::from(".asm");
                (base.join("data"), base.join("config"), base.join("run"))
            }
        };

        // Allow overrides (useful for tests and multi-instance dev).
        let data_dir = env_path("ASM_DATA_DIR").unwrap_or(data_dir);
        let config_dir = env_path("ASM_CONFIG_DIR").unwrap_or(config_dir);
        let runtime_dir = env_path("ASM_RUNTIME_DIR").unwrap_or(runtime_dir);
        let static_dir = env_path("ASM_STATIC_DIR");

        // Half a TLS config is a misconfiguration, not a degraded mode: quietly
        // falling back to plain HTTP is precisely the failure this prevents.
        let (tls_cert, tls_key) = match (env_path("ASM_TLS_CERT"), env_path("ASM_TLS_KEY")) {
            (Some(c), Some(k)) => (Some(c), Some(k)),
            (None, None) => (None, None),
            _ => bail!("set BOTH ASM_TLS_CERT and ASM_TLS_KEY, or neither"),
        };
        let trust_loopback = !matches!(std::env::var("ASM_TRUST_LOOPBACK").as_deref(), Ok("0"));

        std::fs::create_dir_all(&data_dir)
            .with_context(|| format!("creating data dir {}", data_dir.display()))?;
        std::fs::create_dir_all(&config_dir)
            .with_context(|| format!("creating config dir {}", config_dir.display()))?;
        std::fs::create_dir_all(&runtime_dir)
            .with_context(|| format!("creating runtime dir {}", runtime_dir.display()))?;

        let backend = match std::env::var("ASM_BACKEND").as_deref() {
            Ok("sidecar") => BackendKind::Sidecar,
            _ => BackendKind::Native,
        };
        let asmux_socket = env_path("ASMUX_SOCK").unwrap_or_else(|| runtime_dir.join("asmux.sock"));
        let asmux_autospawn = !matches!(std::env::var("ASM_ASMUX_AUTOSPAWN").as_deref(), Ok("0"));
        let asmux_wait = std::env::var("ASM_ASMUX_WAIT_MS")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .map(Duration::from_millis)
            .unwrap_or_else(|| Duration::from_millis(15_000));
        let asmux_bin = env_path("ASM_ASMUX_BIN");

        let relay_url = env_nonempty("ASM_RELAY_URL")
            .map(|u| normalize_relay_url(&u))
            .transpose()?;
        let relay_key = env_nonempty("ASM_RELAY_KEY");
        let relay_ca = env_path("ASM_RELAY_CA");
        if let Some(url) = &relay_url {
            if plaintext_remote_relay(url) && !env_flag("ASM_ALLOW_INSECURE_RELAY") {
                bail!(
                    "ASM_RELAY_URL={url} is plaintext to a remote host. The relay hop carries \
                     the device token and the whole terminal stream, so it must be encrypted: \
                     use wss://. If the relay's certificate is private or self-signed, point \
                     ASM_RELAY_CA at its PEM. Set ASM_ALLOW_INSECURE_RELAY=1 only for a relay \
                     you reach over an already-encrypted channel."
                );
            }
        }
        let node_label = env_nonempty("ASM_NODE_LABEL").unwrap_or_else(hostname_label);
        let relay_downstreams = env_nonempty("ASM_RELAY_DOWNSTREAMS")
            .map(|s| {
                s.split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        let relay_probe_interval = std::env::var("ASM_RELAY_PROBE_INTERVAL_MS")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .filter(|&ms| ms > 0)
            .map(Duration::from_millis)
            .unwrap_or_else(|| Duration::from_millis(5000));

        Ok(Self {
            bind,
            data_dir,
            config_dir,
            runtime_dir,
            static_dir,
            tls_cert,
            tls_key,
            trust_loopback,
            backend,
            asmux_socket,
            asmux_autospawn,
            asmux_wait,
            asmux_bin,
            relay_url,
            relay_key,
            relay_ca,
            node_label,
            relay_downstreams,
            relay_probe_interval,
        })
    }

    pub fn db_path(&self) -> PathBuf {
        self.data_dir.join("asm.sqlite3")
    }
}

/// A path from the environment, treating **empty as unset**.
///
/// Container templates and shell wrappers routinely export `ASM_TLS_CERT=` when
/// a value is absent. Taking that literally produces `Some("")`, which turns
/// "no certificate" into "TLS enabled, now fail reading the file `""`" — and, for
/// `ASM_STATIC_DIR`, silently served the process's working directory instead of
/// disabling static files as documented. The relay binary already reads its paths
/// this way; this makes the daemon agree.
fn env_path(key: &str) -> Option<PathBuf> {
    std::env::var_os(key)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

fn env_nonempty(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|s| !s.is_empty())
}

fn env_flag(key: &str) -> bool {
    matches!(std::env::var(key).as_deref(), Ok("1"))
}

/// Rewrite `ASM_RELAY_URL` into a scheme the agent can actually dial.
///
/// `tokio-tungstenite` dials **only** `ws://` and `wss://` — anything else fails
/// with `UnsupportedUrlScheme`. That failure happens inside the agent's
/// reconnect loop, so an `https://` relay URL boots the daemon perfectly and
/// then never registers, retrying forever. And `https://` is exactly what a user
/// has to hand: it is the URL the browser uses for the same relay, on the same
/// port. So translate it instead of making them learn which half of the product
/// wants which scheme.
fn normalize_relay_url(url: &str) -> Result<String> {
    let (scheme, rest) = url
        .split_once("://")
        .with_context(|| format!("ASM_RELAY_URL `{url}` has no scheme; expected wss://host"))?;
    let ws_scheme = match scheme.to_ascii_lowercase().as_str() {
        "wss" | "https" => "wss",
        "ws" | "http" => "ws",
        other => bail!(
            "ASM_RELAY_URL scheme `{other}://` cannot be dialled; use wss://host (https:// is \
             accepted and translated)"
        ),
    };

    // The agent appends `/register` and `/data` to this, so a trailing slash —
    // which is exactly what a browser copies out of the address bar — would
    // produce `//register`. The relay's router does not serve that path, so the
    // node would dial, 404, and never register.
    let rest = rest.trim_end_matches('/');

    // The *authority* is what has to be there, not merely some remainder:
    // `wss:///relay` has a non-empty rest (`/relay`) and no host at all, which
    // boots the daemon happily and then fails every dial forever.
    let authority = authority_of(rest);
    if authority.is_empty() {
        bail!("ASM_RELAY_URL `{url}` has no host; expected wss://host[:port]");
    }

    // Reject userinfo outright. Nothing uses it (the relay key travels as a query
    // param), and it is a live trap for the plaintext check below:
    // `http://localhost:80@evil.example` reads as the loopback host `localhost`
    // to a naive split, so it would slip past the insecure-relay guard — while
    // tungstenite dials `evil.example` in the clear.
    if authority.contains('@') {
        bail!("ASM_RELAY_URL must not contain credentials (`user@host`); got `{url}`");
    }

    let normalized = format!("{ws_scheme}://{rest}");
    if normalized != url {
        tracing::info!("ASM_RELAY_URL {url} → {normalized} (the relay is dialled over WebSocket)");
    }
    Ok(normalized)
}

/// The authority of a URL's post-scheme remainder: everything before the path,
/// query, or fragment.
fn authority_of(rest: &str) -> &str {
    rest.split(['/', '?', '#']).next().unwrap_or("")
}

/// Is this relay URL a plaintext scheme aimed at a host that is not loopback?
///
/// That is the combination worth refusing: `ws://127.0.0.1:4700` is how the
/// tests and local dev drive a relay on the same box, and stays allowed, while
/// `ws://relay.example.com` would put the device token and every keystroke on
/// the open network. Takes a URL already through [`normalize_relay_url`], so the
/// scheme is known to be `ws`/`wss`.
fn plaintext_remote_relay(url: &str) -> bool {
    match url.split_once("://") {
        Some(("ws", rest)) => !is_loopback_authority(rest),
        _ => false,
    }
}

/// Loopback test for a URL authority: `host`, `host:port`, or `[::1]:port`.
///
/// Userinfo is stripped before the host is read. `normalize_relay_url` already
/// refuses it, but this must not be the kind of function that quietly answers
/// "loopback" for `localhost:80@evil.example` if it is ever called on a raw URL:
/// the real host is whatever follows the LAST `@`, which is what a dialler uses.
fn is_loopback_authority(rest: &str) -> bool {
    let authority = authority_of(rest);
    let hostport = match authority.rsplit_once('@') {
        Some((_userinfo, host)) => host,
        None => authority,
    };
    let host = match hostport.strip_prefix('[') {
        Some(v6) => v6.split(']').next().unwrap_or(""), // IPv6 literal
        None => hostport.split(':').next().unwrap_or(""),
    };
    host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<std::net::IpAddr>()
            .map(|ip| ip.is_loopback())
            .unwrap_or(false)
}

/// Best-effort host label for the node (advertised to the relay / shown in the
/// client) when `ASM_NODE_LABEL` is unset.
fn hostname_label() -> String {
    env_nonempty("HOSTNAME")
        .or_else(|| env_nonempty("COMPUTERNAME"))
        .unwrap_or_else(|| "asm-node".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plaintext_is_refused_only_when_remote() {
        assert!(!plaintext_remote_relay("wss://relay.example.com"));
        assert!(!plaintext_remote_relay("ws://127.0.0.1:4700"));
        assert!(!plaintext_remote_relay("ws://localhost:4700"));
        assert!(!plaintext_remote_relay("ws://[::1]:4700"));
        assert!(plaintext_remote_relay("ws://relay.example.com"));
        assert!(plaintext_remote_relay("ws://10.0.0.5:4700"));
        assert!(plaintext_remote_relay("ws://relay.example.com/path"));
    }

    /// The agent dials only ws/wss, so an https:// relay URL would otherwise
    /// boot fine and then retry forever with UnsupportedUrlScheme.
    #[test]
    fn http_relay_urls_are_translated_to_websocket_schemes() {
        assert_eq!(
            normalize_relay_url("https://relay.example.com").unwrap(),
            "wss://relay.example.com"
        );
        assert_eq!(
            normalize_relay_url("http://127.0.0.1:4700").unwrap(),
            "ws://127.0.0.1:4700"
        );
        // Already-correct URLs pass through untouched.
        assert_eq!(
            normalize_relay_url("wss://relay.example.com:4700").unwrap(),
            "wss://relay.example.com:4700"
        );
        // A translated https:// URL is still subject to the plaintext check.
        assert!(plaintext_remote_relay(
            &normalize_relay_url("http://relay.example.com").unwrap()
        ));
        assert!(normalize_relay_url("relay.example.com").is_err());
        assert!(normalize_relay_url("tcp://relay.example.com").is_err());
    }

    /// A URL copied from a browser keeps its trailing slash. The agent appends
    /// `/register`, so leaving it on produces `//register` — a path the relay
    /// does not route, and the node never registers.
    #[test]
    fn trailing_slashes_are_stripped() {
        assert_eq!(
            normalize_relay_url("https://relay.example.com/").unwrap(),
            "wss://relay.example.com"
        );
        assert_eq!(
            normalize_relay_url("wss://relay.example.com:4700///").unwrap(),
            "wss://relay.example.com:4700"
        );
        assert!(normalize_relay_url("https://").is_err());
    }

    /// A URL with a path but no host parses "fine" and then never dials.
    #[test]
    fn a_url_with_no_authority_is_refused() {
        assert!(normalize_relay_url("wss:///relay").is_err());
        assert!(normalize_relay_url("https:///").is_err());
        assert!(normalize_relay_url("ws://?relay_key=x").is_err());
    }

    /// `http://localhost:80@evil.example` reads as the loopback host `localhost`
    /// to a naive split — so it would slip past the plaintext-relay guard while
    /// the agent actually dials `evil.example` in the clear.
    #[test]
    fn userinfo_cannot_disguise_a_remote_host_as_loopback() {
        assert!(normalize_relay_url("http://localhost:80@evil.example").is_err());
        assert!(normalize_relay_url("ws://user:pass@relay.example.com").is_err());
        // And the loopback test itself is not fooled, wherever it is called from.
        assert!(!is_loopback_authority("localhost:80@evil.example"));
        assert!(!is_loopback_authority("127.0.0.1@evil.example:4700"));
        assert!(is_loopback_authority("127.0.0.1:4700"));
    }
}
