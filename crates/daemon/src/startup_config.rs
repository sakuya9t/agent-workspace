//! The host's saved startup configuration: `<data_dir>/startup.conf`.
//!
//! `scripts/start.sh` and `scripts/restart-daemon.sh` start from this file on
//! every run (command-line flags patch it and are saved back), so a flagless
//! start reproduces the last configuration instead of reverting to defaults.
//! The daemon exposes it at `GET/PUT /api/config` so the web client's Config
//! dialog can edit the startup settings of any host it is connected to; the
//! Vite dev server serves the same file for the UI host (`client/vite.config-api.ts`).
//!
//! The format is deliberately the lowest common denominator of its three
//! readers (bash, Rust, Node): one `ASM_*=value` line per setting, `#` comments,
//! no quoting, no escapes. The keys ARE the environment variables the scripts
//! export, so the file reads as "the env this install starts with". Values may
//! not contain control characters (a newline would split a line) or `|` (the
//! scripts' runtime `.reg` records are pipe-delimited).
//!
//! Nothing here restarts anything: a saved change applies the next time the
//! scripts run on that host. To say whether that has happened, the scripts write
//! the configuration they last applied to `<runtime_dir>/startup.applied` (same
//! format); the keys where the two differ are reported as `pending`.
//!
//! The same rules live in `client/src/startupConfig.ts` and
//! `scripts/_asm_common.sh`; keep the three in step.

use std::collections::BTreeMap;
use std::io::{self, Write};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

pub const FILE_NAME: &str = "startup.conf";
pub const APPLIED_FILE_NAME: &str = "startup.applied";

/// Every setting, in file order. Three independent components — the daemon
/// (with its session holder), a bundled relay, and the managed web UI — each
/// switched by its `ASM_RUN_*` key; any non-empty subset is valid.
pub const KEYS: [&str; 17] = [
    "ASM_PROFILE",
    "ASM_RUN_DAEMON",
    "ASM_BIND",
    "ASM_NODE_LABEL",
    "ASM_ENROLLMENT_TOKEN",
    "ASM_RELAY_URL",
    "ASM_RELAY_KEY",
    "ASM_RELAY_DOWNSTREAMS",
    "ASM_RUN_RELAY",
    "ASM_RELAY_BIND",
    "ASM_RELAY_KEYS",
    "ASM_RUN_UI",
    "ASM_UI_HOST",
    "ASM_UI_PORT",
    "ASM_UI_ALLOWED_HOSTS",
    "ASM_UI_DAEMON",
    "ASM_UI_DAEMON_TOKEN",
];

/// Write-only over the API: a read reports only whether each is set.
pub const SECRET_KEYS: [&str; 4] = [
    "ASM_ENROLLMENT_TOKEN",
    "ASM_RELAY_KEY",
    "ASM_RELAY_KEYS",
    "ASM_UI_DAEMON_TOKEN",
];

/// Shortest enrollment token accepted: it is the LAN's only gate to enrolling.
pub const MIN_ENROLLMENT_TOKEN_LEN: usize = 16;

const BOOL_KEYS: [&str; 3] = ["ASM_RUN_DAEMON", "ASM_RUN_RELAY", "ASM_RUN_UI"];

/// Longest value accepted — generous for a key list, small enough that a
/// runaway paste is refused rather than written.
const MAX_VALUE_LEN: usize = 4096;

const HEADER: &str = "\
# Agent Session Manager — saved startup settings for this install.
# scripts/start.sh and scripts/restart-daemon.sh start from these on every run;
# command-line flags override them and are saved back. Also editable from the
# web UI (Config). One KEY=value per line, no quotes; an empty value means the
# default.
";

pub type Values = BTreeMap<String, String>;

pub fn is_key(key: &str) -> bool {
    KEYS.contains(&key)
}

pub fn is_secret(key: &str) -> bool {
    SECRET_KEYS.contains(&key)
}

/// Where this install keeps the file: per data dir, so a second install on the
/// same machine (`--data-dir`, a test sandbox) never shares or clobbers it.
pub fn path_in(data_dir: &Path) -> PathBuf {
    data_dir.join(FILE_NAME)
}

pub fn applied_path_in(runtime_dir: &Path) -> PathBuf {
    runtime_dir.join(APPLIED_FILE_NAME)
}

/// Parse the file. Comments, blank lines, malformed lines and unknown keys are
/// skipped — the scripts warn about them; a reader here has no one to tell.
/// Key and value are trimmed, and a trailing `\r` (a CRLF edit) is dropped.
pub fn parse(content: &str) -> Values {
    let mut out = Values::new();
    for line in content.lines() {
        let line = line.trim_end_matches('\r');
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        if is_key(key) {
            out.insert(key.to_string(), value.trim().to_string());
        }
    }
    out
}

/// Render every key in file order; a key with no value is written empty.
pub fn render(values: &Values) -> String {
    let mut out = String::from(HEADER);
    for key in KEYS {
        out.push_str(key);
        out.push('=');
        out.push_str(values.get(key).map(String::as_str).unwrap_or(""));
        out.push('\n');
    }
    out
}

/// Opaque change token: FNV-1a 64 over the file bytes. The client's UI-server
/// endpoint uses the same function, so either server accepts the other's token.
pub fn revision(content: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in content.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

/// Keys whose saved value differs from the applied one. Absent ≡ empty.
pub fn pending(saved: &Values, applied: &Values) -> Vec<String> {
    let get = |v: &Values, k: &str| v.get(k).cloned().unwrap_or_default();
    KEYS.iter()
        .filter(|k| get(saved, k) != get(applied, k))
        .map(|k| k.to_string())
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidSetting {
    pub key: String,
    pub message: String,
}

impl std::fmt::Display for InvalidSetting {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.key, self.message)
    }
}

fn invalid(key: &str, message: impl Into<String>) -> InvalidSetting {
    InvalidSetting {
        key: key.to_string(),
        message: message.into(),
    }
}

fn is_port(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 5
        && s.bytes().all(|b| b.is_ascii_digit())
        && s.parse::<u32>().is_ok_and(|p| (1..=65535).contains(&p))
}

/// `IP:port` exactly as the daemon/relay parse it, with a real (non-zero) port.
fn is_socket_addr(s: &str) -> bool {
    s.parse::<SocketAddr>().is_ok_and(|a| a.port() != 0)
}

fn has_scheme_and_authority(s: &str, schemes: &[&str]) -> bool {
    schemes.iter().any(|scheme| {
        s.strip_prefix(scheme)
            .is_some_and(|rest| !rest.is_empty() && !rest.starts_with('/'))
    }) && !s.chars().any(char::is_whitespace)
}

fn is_token(s: &str) -> bool {
    !s.chars().any(|c| c.is_whitespace() || c == ',')
}

/// An exact DNS hostname, as Vite's `allowedHosts` takes it.
fn is_hostname(s: &str) -> bool {
    let b = s.as_bytes();
    !b.is_empty()
        && b[0].is_ascii_alphanumeric()
        && b[b.len() - 1].is_ascii_alphanumeric()
        && b.iter().all(|c| c.is_ascii_alphanumeric() || *c == b'.' || *c == b'-')
}

/// `host:port` or `[v6]:port` — resolved by the daemon at startup.
fn is_host_port(s: &str) -> bool {
    let Some((host, port)) = s.rsplit_once(':') else {
        return false;
    };
    let host_ok = if let Some(inner) = host.strip_prefix('[').and_then(|h| h.strip_suffix(']')) {
        !inner.is_empty() && inner.bytes().all(|c| c.is_ascii_hexdigit() || c == b':' || c == b'.')
    } else {
        !host.is_empty()
            && host
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'.' || c == b'-')
    };
    host_ok && is_port(port)
}

fn list(s: &str) -> impl Iterator<Item = &str> {
    s.split(',').map(str::trim)
}

/// One value on its own. Empty always passes: it means "the default".
pub fn validate_value(key: &str, value: &str) -> Result<(), InvalidSetting> {
    if value.chars().any(char::is_control) {
        return Err(invalid(key, "must be a single line of text"));
    }
    if value.contains('|') {
        return Err(invalid(key, "may not contain `|`"));
    }
    if value.len() > MAX_VALUE_LEN {
        return Err(invalid(key, "is too long"));
    }
    if value.is_empty() {
        return Ok(());
    }
    let ok = match key {
        "ASM_PROFILE" => matches!(value, "debug" | "release"),
        k if BOOL_KEYS.contains(&k) => matches!(value, "0" | "1"),
        "ASM_BIND" | "ASM_RELAY_BIND" => is_socket_addr(value),
        "ASM_NODE_LABEL" => value.chars().count() <= 100,
        "ASM_RELAY_URL" => has_scheme_and_authority(value, &["ws://", "wss://"]),
        "ASM_UI_DAEMON" => has_scheme_and_authority(value, &["http://", "https://"]),
        "ASM_RELAY_KEY" | "ASM_UI_DAEMON_TOKEN" => is_token(value),
        "ASM_ENROLLMENT_TOKEN" => is_token(value) && value.len() >= MIN_ENROLLMENT_TOKEN_LEN,
        "ASM_RELAY_KEYS" => list(value).all(|k| !k.is_empty() && is_token(k)),
        "ASM_RELAY_DOWNSTREAMS" => list(value).all(is_host_port),
        "ASM_UI_HOST" => value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b".:-_[]*".contains(&c)),
        "ASM_UI_PORT" => is_port(value),
        "ASM_UI_ALLOWED_HOSTS" => list(value).all(is_hostname),
        _ => return Err(invalid(key, "is not a startup setting")),
    };
    if ok {
        return Ok(());
    }
    Err(invalid(
        key,
        match key {
            "ASM_PROFILE" => "expected `debug` or `release`",
            "ASM_BIND" | "ASM_RELAY_BIND" => "expected an IP address and port, e.g. 0.0.0.0:4600",
            "ASM_NODE_LABEL" => "must be at most 100 characters",
            "ASM_RELAY_URL" => "expected a ws:// or wss:// relay URL",
            "ASM_UI_DAEMON" => "expected an http:// or https:// daemon URL",
            "ASM_RELAY_KEY" | "ASM_UI_DAEMON_TOKEN" => "may not contain spaces or commas",
            "ASM_ENROLLMENT_TOKEN" => "expected at least 16 characters, without spaces or commas",
            "ASM_RELAY_KEYS" => "expected comma-separated keys without spaces",
            "ASM_RELAY_DOWNSTREAMS" => "expected comma-separated host:port entries",
            "ASM_UI_HOST" => "expected a hostname or IP address",
            "ASM_UI_PORT" => "expected a port between 1 and 65535",
            "ASM_UI_ALLOWED_HOSTS" => {
                "expected comma-separated exact hostnames (no scheme, port, path, or wildcard)"
            }
            _ => "expected 0 or 1",
        },
    ))
}

/// Whether a component runs, applying its default (daemon and UI on, relay off)
/// to an empty or missing switch.
pub fn component_on(values: &Values, key: &str) -> bool {
    match values.get(key).map(String::as_str).unwrap_or("") {
        "" => key != "ASM_RUN_RELAY",
        v => v == "1",
    }
}

/// The whole configuration: each value, then the dependencies between
/// settings. The components themselves do not depend on each other — only the
/// settings that belong to a component that is on are checked.
pub fn validate(values: &Values) -> Result<(), InvalidSetting> {
    for (key, value) in values {
        validate_value(key, value)?;
    }
    let get = |k: &str| values.get(k).map(String::as_str).unwrap_or("");
    let daemon = component_on(values, "ASM_RUN_DAEMON");
    let relay = component_on(values, "ASM_RUN_RELAY");
    let ui = component_on(values, "ASM_RUN_UI");
    if !daemon && !relay && !ui {
        return Err(invalid("ASM_RUN_DAEMON", "enable at least one of the daemon, relay, or web UI"));
    }
    if relay && get("ASM_RELAY_KEYS").is_empty() {
        return Err(invalid("ASM_RELAY_KEYS", "a relay needs at least one access key"));
    }
    if daemon && !get("ASM_RELAY_URL").is_empty() && get("ASM_RELAY_KEY").is_empty() {
        return Err(invalid("ASM_RELAY_KEY", "registering to a relay needs its access key"));
    }
    Ok(())
}

/// Replace `path` atomically with owner-only permissions — it holds secrets.
pub fn write_atomic(path: &Path, content: &str) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    // `<name>.tmp.<pid>`, as the scripts and the UI server name theirs.
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(format!(".tmp.{}", std::process::id()));
    let tmp = PathBuf::from(tmp);
    let result = (|| {
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        let mut file = opts.open(&tmp)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            // `mode` only applies on create; a stale tmp could be broader.
            file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
        }
        file.write_all(content.as_bytes())?;
        file.sync_all()?;
        std::fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

fn read_optional(path: &Path) -> io::Result<Option<String>> {
    match std::fs::read_to_string(path) {
        Ok(s) => Ok(Some(s)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

/// The configuration as a reader sees it.
#[derive(Debug, Clone)]
pub struct Snapshot {
    pub exists: bool,
    /// `None` when there is no file yet.
    pub revision: Option<String>,
    /// Effective values: the file's, falling back to the process environment
    /// for keys the file lacks — the scripts' own precedence (saved > env >
    /// default). Keys set nowhere are absent and mean the default.
    pub values: Values,
    /// Keys saved but not yet applied by the scripts; `None` when either file
    /// is missing (nothing to compare).
    pub pending: Option<Vec<String>>,
}

#[derive(Debug)]
pub enum SaveError {
    /// The file changed since the caller read `revision`.
    Conflict,
    UnknownKey(String),
    Invalid(InvalidSetting),
    Io(io::Error),
}

impl std::fmt::Display for SaveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SaveError::Conflict => write!(
                f,
                "the saved config changed since it was loaded — reload it and reapply your edits"
            ),
            SaveError::UnknownKey(k) => write!(f, "{k} is not a startup setting"),
            SaveError::Invalid(e) => write!(f, "{e}"),
            SaveError::Io(e) => write!(f, "writing the startup config: {e}"),
        }
    }
}

/// The file plus its applied snapshot, with the reader's environment injected
/// so tests do not depend on the ambient `ASM_*` exports of the host.
pub struct Store<'a> {
    pub path: PathBuf,
    pub applied_path: PathBuf,
    pub env: &'a dyn Fn(&str) -> Option<String>,
}

impl Store<'_> {
    pub fn load(&self) -> io::Result<Snapshot> {
        let content = read_optional(&self.path)?;
        let saved = content.as_deref().map(parse);
        let mut values = Values::new();
        for key in KEYS {
            let value = match &saved {
                Some(file) if file.contains_key(key) => file.get(key).cloned(),
                _ => (self.env)(key),
            };
            if let Some(v) = value {
                values.insert(key.to_string(), v);
            }
        }
        let pending = match (&saved, read_optional(&self.applied_path)?) {
            (Some(saved), Some(applied)) => Some(pending(saved, &parse(&applied))),
            _ => None,
        };
        Ok(Snapshot {
            exists: content.is_some(),
            revision: content.as_deref().map(revision),
            values,
            pending,
        })
    }

    /// Apply `patch` on top of the current effective values and write the
    /// result. Secret keys absent from `patch` keep their value; `""` clears.
    pub fn save(&self, expected_revision: Option<&str>, patch: &Values) -> Result<Snapshot, SaveError> {
        let current = self.load().map_err(SaveError::Io)?;
        if current.revision.as_deref() != expected_revision {
            return Err(SaveError::Conflict);
        }
        let mut values = current.values;
        for (key, value) in patch {
            if !is_key(key) {
                return Err(SaveError::UnknownKey(key.clone()));
            }
            values.insert(key.clone(), value.trim().to_string());
        }
        validate(&values).map_err(SaveError::Invalid)?;
        write_atomic(&self.path, &render(&values)).map_err(SaveError::Io)?;
        self.load().map_err(SaveError::Io)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vals(pairs: &[(&str, &str)]) -> Values {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    fn no_env(_: &str) -> Option<String> {
        None
    }

    fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("asm-startup-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn parse_skips_noise_and_unknown_keys_and_trims() {
        let parsed = parse(
            "# comment\n\n  # indented comment\nASM_BIND = 0.0.0.0:4600 \r\n\
             ASM_NODE_LABEL=box = one\nNOT_OURS=1\nno equals sign\nASM_RELAY_URL=\n",
        );
        assert_eq!(
            parsed,
            vals(&[
                ("ASM_BIND", "0.0.0.0:4600"),
                ("ASM_NODE_LABEL", "box = one"),
                ("ASM_RELAY_URL", ""),
            ])
        );
    }

    #[test]
    fn render_writes_every_key_in_order_and_round_trips() {
        let values = vals(&[("ASM_UI_PORT", "8080"), ("ASM_BIND", "127.0.0.1:4600")]);
        let text = render(&values);
        let keys: Vec<&str> = text
            .lines()
            .filter(|l| !l.starts_with('#'))
            .map(|l| l.split_once('=').unwrap().0)
            .collect();
        assert_eq!(keys, KEYS);
        let back = parse(&text);
        assert_eq!(back.get("ASM_UI_PORT").unwrap(), "8080");
        assert_eq!(back.get("ASM_RUN_UI").unwrap(), "");
    }

    #[test]
    fn revision_is_fnv1a64_hex() {
        // Reference vectors for FNV-1a 64; the client computes the same thing.
        assert_eq!(revision(""), "cbf29ce484222325");
        assert_eq!(revision("a"), "af63dc4c8601ec8c");
        assert_ne!(revision("ASM_BIND=1"), revision("ASM_BIND=2"));
    }

    #[test]
    fn value_rules() {
        let ok = [
            ("ASM_PROFILE", "release"),
            ("ASM_BIND", "0.0.0.0:4600"),
            ("ASM_BIND", "[::1]:4600"),
            ("ASM_RELAY_URL", "wss://relay.example.com"),
            ("ASM_ENROLLMENT_TOKEN", "0123456789abcdef"),
            ("ASM_RELAY_KEYS", "a,b , c"),
            ("ASM_RELAY_DOWNSTREAMS", "10.0.0.5:4600,box.lan:4601,[fd00::1]:4600"),
            ("ASM_UI_HOST", "0.0.0.0"),
            ("ASM_UI_HOST", "[::]"),
            ("ASM_UI_ALLOWED_HOSTS", "pi.tail.ts.net,box"),
            ("ASM_UI_DAEMON", "http://10.0.0.2:4600"),
            ("ASM_RUN_UI", "0"),
            ("ASM_NODE_LABEL", "my box ✓"),
            ("ASM_BIND", ""),
        ];
        for (k, v) in ok {
            assert!(validate_value(k, v).is_ok(), "{k}={v} should pass");
        }
        let bad = [
            ("ASM_PROFILE", "fast"),
            ("ASM_BIND", "localhost:4600"),
            ("ASM_BIND", "0.0.0.0"),
            ("ASM_BIND", "0.0.0.0:0"),
            ("ASM_RELAY_URL", "http://relay:4700"),
            ("ASM_RELAY_URL", "ws://"),
            ("ASM_RELAY_KEY", "two words"),
            ("ASM_ENROLLMENT_TOKEN", "short"),
            ("ASM_ENROLLMENT_TOKEN", "sixteen chars but spaced"),
            ("ASM_RELAY_KEY", "a,b"),
            ("ASM_RELAY_KEYS", "a,,b"),
            ("ASM_RELAY_DOWNSTREAMS", "box"),
            ("ASM_UI_PORT", "70000"),
            ("ASM_UI_HOST", "bad host"),
            ("ASM_UI_ALLOWED_HOSTS", "*.ts.net"),
            ("ASM_UI_ALLOWED_HOSTS", "https://pi"),
            ("ASM_UI_DAEMON", "ftp://x"),
            ("ASM_RUN_UI", "true"),
            ("ASM_RUN_DAEMON", "yes"),
            ("ASM_NODE_LABEL", "line\nbreak"),
            ("ASM_NODE_LABEL", "a|b"),
            ("ASM_NOT_A_KEY", "x"),
        ];
        for (k, v) in bad {
            assert!(validate_value(k, v).is_err(), "{k}={v:?} should fail");
        }
    }

    #[test]
    fn any_non_empty_set_of_components_is_valid() {
        let key = ("ASM_RELAY_KEYS", "k");
        for daemon in ["0", "1"] {
            for relay in ["0", "1"] {
                for ui in ["0", "1"] {
                    let v = vals(&[
                        ("ASM_RUN_DAEMON", daemon),
                        ("ASM_RUN_RELAY", relay),
                        ("ASM_RUN_UI", ui),
                        key,
                    ]);
                    let none = daemon == "0" && relay == "0" && ui == "0";
                    assert_eq!(validate(&v).is_ok(), !none, "daemon={daemon} relay={relay} ui={ui}");
                }
            }
        }
        // Empty switches are the defaults: daemon + UI on, relay off.
        assert!(component_on(&Values::new(), "ASM_RUN_DAEMON"));
        assert!(component_on(&Values::new(), "ASM_RUN_UI"));
        assert!(!component_on(&Values::new(), "ASM_RUN_RELAY"));
    }

    #[test]
    fn settings_are_checked_only_for_components_that_run() {
        assert_eq!(validate(&vals(&[("ASM_RUN_RELAY", "1")])).unwrap_err().key, "ASM_RELAY_KEYS");
        let register = [("ASM_RELAY_URL", "ws://r:4700")];
        assert_eq!(validate(&vals(&register)).unwrap_err().key, "ASM_RELAY_KEY");
        assert!(validate(&vals(&[register[0], ("ASM_RELAY_KEY", "k")])).is_ok());
        // With the daemon off, a leftover registration (or relay key) is kept but unused.
        assert!(validate(&vals(&[register[0], ("ASM_RUN_DAEMON", "0")])).is_ok());
        assert!(validate(&vals(&[("ASM_RUN_RELAY", "0")])).is_ok());
    }

    #[test]
    fn load_prefers_the_file_then_env_and_reports_pending() {
        let dir = temp_dir();
        let env = |k: &str| match k {
            "ASM_BIND" => Some("10.0.0.1:4600".to_string()),
            "ASM_NODE_LABEL" => Some("from-env".to_string()),
            _ => None,
        };
        let store = Store {
            path: dir.join(FILE_NAME),
            applied_path: dir.join(APPLIED_FILE_NAME),
            env: &env,
        };

        let fresh = store.load().unwrap();
        assert!(!fresh.exists);
        assert_eq!(fresh.revision, None);
        assert_eq!(fresh.values.get("ASM_BIND").unwrap(), "10.0.0.1:4600");
        assert_eq!(fresh.pending, None);

        // An empty value in the file is authoritative over the env.
        std::fs::write(&store.path, "ASM_BIND=0.0.0.0:4600\nASM_NODE_LABEL=\n").unwrap();
        let loaded = store.load().unwrap();
        assert!(loaded.exists);
        assert_eq!(loaded.values.get("ASM_BIND").unwrap(), "0.0.0.0:4600");
        assert_eq!(loaded.values.get("ASM_NODE_LABEL").unwrap(), "");
        assert_eq!(loaded.pending, None);

        std::fs::write(&store.applied_path, "ASM_BIND=127.0.0.1:4600\nASM_NODE_LABEL=\n").unwrap();
        assert_eq!(store.load().unwrap().pending, Some(vec!["ASM_BIND".to_string()]));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn save_checks_revision_keeps_secrets_and_writes_owner_only() {
        let dir = temp_dir();
        let store = Store {
            path: dir.join(FILE_NAME),
            applied_path: dir.join(APPLIED_FILE_NAME),
            env: &no_env,
        };
        let first = store
            .save(
                None,
                &vals(&[
                    ("ASM_RELAY_URL", "ws://relay:4700"),
                    ("ASM_RELAY_KEY", "s3cret"),
                ]),
            )
            .unwrap();
        assert!(first.exists);

        // A second writer holding no revision (it saw no file) now conflicts.
        assert!(matches!(
            store.save(None, &vals(&[("ASM_UI_PORT", "8080")])),
            Err(SaveError::Conflict)
        ));

        // Omitting the secret keeps it; the other edit lands.
        let second = store
            .save(first.revision.as_deref(), &vals(&[("ASM_NODE_LABEL", "box")]))
            .unwrap();
        assert_eq!(second.values.get("ASM_RELAY_KEY").unwrap(), "s3cret");
        assert_eq!(second.values.get("ASM_NODE_LABEL").unwrap(), "box");

        // Clearing the key while still registering is refused, file untouched.
        let before = std::fs::read_to_string(&store.path).unwrap();
        assert!(matches!(
            store.save(second.revision.as_deref(), &vals(&[("ASM_RELAY_KEY", "")])),
            Err(SaveError::Invalid(e)) if e.key == "ASM_RELAY_KEY"
        ));
        assert!(matches!(
            store.save(second.revision.as_deref(), &vals(&[("ASM_EVIL", "1")])),
            Err(SaveError::UnknownKey(_))
        ));
        assert_eq!(std::fs::read_to_string(&store.path).unwrap(), before);

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&store.path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        let _ = std::fs::remove_dir_all(dir);
    }
}
