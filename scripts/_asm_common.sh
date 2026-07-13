#!/usr/bin/env bash
# Shared config + helpers for the asm service scripts. SOURCED, not run.
#
# Config comes from command-line flags (parsed by asm_parse_args) with the same
# `ASM_*` environment variables kept as fallbacks. A sourcing script does:
#
#     source _asm_common.sh
#     asm_parse_args "$@" || { usage; exit 2; }
#     asm_configure
#
# Both the daemon and the asmux holder get the SAME data/runtime dir so they
# agree on the socket path (<runtime_dir>/asmux.sock) and data lives in one place
# across restarts.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

log() { printf '\033[1;36m[asm]\033[0m %s\n' "$*"; }
err() { printf '\033[1;31m[asm]\033[0m %s\n' "$*" >&2; }

_need() { # _need FLAG VALUE — fail if the flag was given no value
  [ -n "${2:-}" ] && return 0
  err "option $1 requires a value"
  return 2
}

# Translate flags into the ASM_* env vars the binaries read (env stays the
# fallback: a flag only overrides when given). Collects any non-flag args into
# ASM_POSITIONAL and sets ASM_SHOW_HELP. Returns non-zero on a bad flag.
asm_parse_args() {
  ASM_POSITIONAL=()
  ASM_SHOW_HELP=0
  local key="" want_relay=0 reconfig=0
  while [ $# -gt 0 ]; do
    case "$1" in
      --bind)         _need "$1" "${2:-}" || return 2; export ASM_BIND="$2"; reconfig=1; shift 2 ;;
      --tls-cert)     _need "$1" "${2:-}" || return 2; export ASM_TLS_CERT="$2"; reconfig=1; shift 2 ;;
      --tls-key)      _need "$1" "${2:-}" || return 2; export ASM_TLS_KEY="$2"; reconfig=1; shift 2 ;;
      --data-dir)     _need "$1" "${2:-}" || return 2; export ASM_DATA_DIR="$2"; shift 2 ;;
      --runtime-dir)  _need "$1" "${2:-}" || return 2; export ASM_RUNTIME_DIR="$2"; shift 2 ;;
      --label)        _need "$1" "${2:-}" || return 2; export ASM_NODE_LABEL="$2"; reconfig=1; shift 2 ;;
      --release)      PROFILE=release; shift ;;
      --relay)        want_relay=1; shift ;;
      --relay-only)   want_relay=1; export ASM_RELAY_ONLY=1; shift ;;
      --relay-bind)   _need "$1" "${2:-}" || return 2; export ASM_RELAY_BIND="$2"; want_relay=1; shift 2 ;;
      --relay-key)    _need "$1" "${2:-}" || return 2; key="$2"; shift 2 ;;
      --relay-tls-cert) _need "$1" "${2:-}" || return 2; export ASM_RELAY_TLS_CERT="$2"; want_relay=1; shift 2 ;;
      --relay-tls-key)  _need "$1" "${2:-}" || return 2; export ASM_RELAY_TLS_KEY="$2"; want_relay=1; shift 2 ;;
      # For the proxy-terminated deployment: the relay sees plain HTTP, but the
      # browser is on HTTPS and should still be told never to fall back.
      --relay-hsts)     export ASM_RELAY_HSTS=1; want_relay=1; shift ;;
      --register)     _need "$1" "${2:-}" || return 2; export ASM_RELAY_URL="$2"; reconfig=1; shift 2 ;;
      --relay-ca)     _need "$1" "${2:-}" || return 2; export ASM_RELAY_CA="$2"; reconfig=1; shift 2 ;;
      # An off-loopback --bind needs no acknowledgement — choosing it IS the
      # acknowledgement, and the daemon warns at startup. A plaintext relay is
      # different: it is almost never deliberate, and wss:// costs nothing.
      --insecure-relay) export ASM_ALLOW_INSECURE_RELAY=1; reconfig=1; shift ;;
      # Mandatory behind a same-host reverse proxy: the proxy connects from
      # 127.0.0.1, so without this everything it forwards arrives loopback-trusted
      # and the daemon's auth is effectively off.
      --no-loopback-trust) export ASM_TRUST_LOOPBACK=0; reconfig=1; shift ;;
      -h|--help)      ASM_SHOW_HELP=1; shift ;;
      --)             shift; while [ $# -gt 0 ]; do ASM_POSITIONAL+=("$1"); shift; done ;;
      --*)            err "unknown option: $1"; return 2 ;;
      *)              ASM_POSITIONAL+=("$1"); shift ;;
    esac
  done
  # One shared secret feeds whichever role(s) are active: a relay host accepts it
  # (ASM_RELAY_KEYS); a registering node presents it (ASM_RELAY_KEY).
  if [ -n "$key" ]; then
    [ "$want_relay" = 1 ] && export ASM_RELAY_KEYS="$key"
    if [ -n "${ASM_RELAY_URL:-}" ]; then export ASM_RELAY_KEY="$key"; reconfig=1; fi
  fi
  # --relay with no key (and none in the env) would reject every node.
  if [ "$want_relay" = 1 ] && [ -z "${ASM_RELAY_KEYS:-}" ]; then
    err "--relay needs --relay-key KEY (the access key nodes present)"
    return 2
  fi
  # Record whether a daemon-affecting override (bind/label/relay registration)
  # or a relay flag was passed THIS invocation, so start_daemon can re-apply the
  # former to an already-running daemon and the recorded-config loaders know
  # which components this command line re-specified. Always set fresh — they
  # describe this command line, not persisted config, never an env fallback.
  export ASM_DAEMON_RECONFIG="$reconfig"
  export ASM_RELAY_RECONFIG="$want_relay"
  return 0
}

# Apply defaults and derive paths/binaries. Call after asm_parse_args.
asm_configure() {
  if [ "${RELEASE:-0}" = "1" ]; then PROFILE=release; fi
  PROFILE="${PROFILE:-debug}"
  BIN_DIR="$ROOT/target/$PROFILE"
  DAEMON_BIN="$BIN_DIR/asm-daemon"
  ASMUX_BIN="$BIN_DIR/asmux"
  RELAY_BIN="$BIN_DIR/asm-relay"

  export ASM_DATA_DIR="${ASM_DATA_DIR:-$HOME/.local/share/asm}"
  export ASM_RUNTIME_DIR="${ASM_RUNTIME_DIR:-${XDG_RUNTIME_DIR:-/tmp}/asm}"
  export ASM_BIND="${ASM_BIND:-127.0.0.1:4600}"
  # Relay default bind is 0.0.0.0 so both LAN clients and nodes dialing out reach it.
  export ASM_RELAY_BIND="${ASM_RELAY_BIND:-0.0.0.0:4700}"

  # Packaged web client: if a build exists (client/dist), serve it straight from
  # the daemon so a box without npm/vite still gets a browser UI at ASM_BIND —
  # no need to pass ASM_STATIC_DIR by hand. An explicit ASM_STATIC_DIR always
  # wins (set it empty, ASM_STATIC_DIR=, to disable). Only a daemon launched
  # fresh below reads this; if you build client/dist while the daemon is already
  # up, run scripts/restart-daemon.sh to pick it up.
  if [ -z "${ASM_STATIC_DIR+set}" ] && [ -f "$ROOT/client/dist/index.html" ]; then
    export ASM_STATIC_DIR="$ROOT/client/dist"
  fi

  LOG_DIR="$ASM_DATA_DIR/logs"
  ASMUX_SOCK="$ASM_RUNTIME_DIR/asmux.sock"
  ASMUX_PIDFILE="$ASM_RUNTIME_DIR/asmux.pid"
  DAEMON_PIDFILE="$ASM_RUNTIME_DIR/asm-daemon.pid"
  # The config signature the running daemon was launched with, so start.sh can
  # tell when a re-run's flags actually change it (see start_daemon), and so a
  # flagless restart can keep it instead of reverting to defaults.
  DAEMON_STATE_FILE="$ASM_RUNTIME_DIR/asm-daemon.reg"
  RELAY_PIDFILE="$ASM_RUNTIME_DIR/asm-relay.pid"
  # Same for the bundled relay: bind|keys|relay-only, recorded by start_relay.
  RELAY_STATE_FILE="$ASM_RUNTIME_DIR/asm-relay.reg"

  mkdir -p "$ASM_DATA_DIR" "$ASM_RUNTIME_DIR" "$LOG_DIR"
}

# Is the process named in $1 (a pidfile) alive?
pid_alive() {
  local f="$1" p
  [ -f "$f" ] || return 1
  p="$(cat "$f" 2>/dev/null || true)"
  [ -n "$p" ] && kill -0 "$p" 2>/dev/null
}

cargo_build() {
  if [ "$PROFILE" = release ]; then
    ( cd "$ROOT" && cargo build --release "$@" )
  else
    ( cd "$ROOT" && cargo build "$@" )
  fi
}

wait_socket() {
  local i
  for i in $(seq 1 60); do
    if [ -S "$ASMUX_SOCK" ]; then return 0; fi
    sleep 0.1
  done
  return 1
}

# The scheme the daemon serves on: TLS when it holds a certificate.
daemon_scheme() {
  if [ -n "${ASM_TLS_CERT:-}" ]; then printf 'https'; else printf 'http'; fi
}

wait_health() {
  local i scheme
  local args=(-sf)
  command -v curl >/dev/null 2>&1 || { sleep 0.6; return 0; }
  scheme="$(daemon_scheme)"
  # The cert is issued for the daemon's real name, not the bind address this
  # probe dials, so the readiness check skips verification. It answers "is it
  # up?" — the security check is the one real clients make.
  [ "$scheme" = https ] && args+=(-k)
  for i in $(seq 1 60); do
    if curl "${args[@]}" "$scheme://$ASM_BIND/health" >/dev/null 2>&1; then return 0; fi
    sleep 0.1
  done
  return 1
}

# The relay is enabled purely by config (a relay access key is present), never
# by the daemon binary — it is a shared rendezvous, not a per-daemon sidecar.
relay_enabled() { [ -n "${ASM_RELAY_KEYS:-}" ]; }

# ── recorded config ────────────────────────────────────────────────────────
# A flagless start/restart must keep what's running, not silently revert to
# defaults (a 0.0.0.0 bind dropping back to loopback, a --register vanishing).
# The launch config of each component is recorded in a .reg file; these loaders
# re-apply it when THIS invocation's flags didn't re-specify that component.
#
# Precedence: flags > recorded config > environment > defaults. The recording
# deliberately BEATS the environment: a shell inside an asm session inherits the
# daemon's own resolved ASM_* exports (ASM_BIND & co.), so env values cannot be
# read as user intent — trusting them made every in-session restart revert to
# whatever the spawning daemon happened to export. Env still works as a fallback
# when nothing was recorded (e.g. the first start after boot).

# Daemon side (DAEMON_STATE_FILE:
#   bind|label|register-url|register-key|relay-ca|allow-insecure-relay|tls-cert|tls-key|trust-loopback).
# Any daemon-affecting flag (--bind/--label/--register/--relay-ca/--insecure-relay)
# re-specifies the daemon config as a whole — fields you don't pass revert to
# env/defaults. With no such flag, the recording is authoritative, including its
# empty fields. Recordings written before a field existed simply read as empty.
daemon_load_recorded_config() {
  if [ "${ASM_DAEMON_RECONFIG:-0}" = 1 ]; then return 0; fi
  local rec bind label url key ca ins_relay cert tls_key trust
  rec="$(cat "$DAEMON_STATE_FILE" 2>/dev/null || true)"
  if [ -z "$rec" ]; then return 0; fi
  IFS='|' read -r bind label url key ca ins_relay cert tls_key trust <<<"$rec" || true
  if [ -n "$bind" ]; then export ASM_BIND="$bind"; fi
  if [ -n "$label" ]; then export ASM_NODE_LABEL="$label"; else unset ASM_NODE_LABEL; fi
  if [ -n "$url" ];   then export ASM_RELAY_URL="$url";     else unset ASM_RELAY_URL; fi
  if [ -n "$key" ];   then export ASM_RELAY_KEY="$key";     else unset ASM_RELAY_KEY; fi
  # The CA travels with the relay URL it belongs to — a restart that dropped it
  # would leave the node unable to verify a privately-signed relay, so it would
  # boot fine and then never register.
  if [ -n "$ca" ];    then export ASM_RELAY_CA="$ca";       else unset ASM_RELAY_CA; fi
  # The acknowledgement is the one thing the recording must not overrule.
  # Unsetting it here would strip the very variable the daemon's own error
  # message tells you to set, and since the recording beats the environment, that
  # is a dead end with no way out but deleting the .reg file by hand.
  if [ -n "$ins_relay" ]; then export ASM_ALLOW_INSECURE_RELAY="$ins_relay"; fi
  # TLS rides with the bind it protects. A flagless restart that forgot the cert
  # would silently bring the daemon back up in PLAINTEXT on the very address the
  # cert existed to protect.
  if [ -n "$cert" ];    then export ASM_TLS_CERT="$cert";   fi
  if [ -n "$tls_key" ]; then export ASM_TLS_KEY="$tls_key"; fi
  # Loopback trust must survive a restart, and it must fail SAFE. A daemon put
  # behind a reverse proxy with trust disabled that came back up trusting
  # loopback again would silently accept every proxied request with no token —
  # auth off, from a restart nobody thought was a config change. Restore the
  # disabling value; never unset it, so an explicit ASM_TRUST_LOOPBACK=0 in the
  # environment also survives a recording that predates the field.
  if [ "$trust" = "0" ]; then export ASM_TRUST_LOOPBACK=0; fi
}

# Relay side (RELAY_STATE_FILE: bind|keys|relay-only|tls-cert|tls-key|hsts). Skipped
# whenever a --relay* flag was passed — that re-specifies the relay config as a
# whole. Relay-only-ness is restored only on a fully flagless run: passing a
# daemon flag says "I want the daemon here too".
relay_load_recorded_config() {
  if [ "${ASM_RELAY_RECONFIG:-0}" = 1 ]; then return 0; fi
  local rec bind keys only cert key_file hsts
  rec="$(cat "$RELAY_STATE_FILE" 2>/dev/null || true)"
  if [ -z "$rec" ]; then return 0; fi
  IFS='|' read -r bind keys only cert key_file hsts <<<"$rec" || true
  if [ -z "$keys" ]; then return 0; fi
  export ASM_RELAY_KEYS="$keys"
  if [ -n "$bind" ]; then export ASM_RELAY_BIND="$bind"; fi
  if [ "${ASM_DAEMON_RECONFIG:-0}" = 0 ]; then export ASM_RELAY_ONLY="${only:-0}"; fi
  # TLS is recorded like everything else, and for the sharpest reason: a flagless
  # restart that forgot the cert would bring a production relay back up in
  # PLAINTEXT — silently, with every device token on it.
  if [ -n "$cert" ];     then export ASM_RELAY_TLS_CERT="$cert";    fi
  if [ -n "$key_file" ]; then export ASM_RELAY_TLS_KEY="$key_file"; fi
  # HSTS travels with the rest: a proxy-terminated relay revived from its
  # recording would otherwise come back without the header, and the browser would
  # quietly become willing to talk to it over plain HTTP again.
  if [ -n "$hsts" ];     then export ASM_RELAY_HSTS="$hsts";        fi
}

# ── transport preflight ────────────────────────────────────────────────────
# The daemon refuses to speak plaintext to a remote peer. These checks mirror
# that rule (crates/daemon/src/config.rs) so the scripts can fail with the FLAG
# that fixes it, instead of the daemon exiting into its log and the caller seeing
# only "health check failed".

_is_loopback_host() {
  case "$1" in
    localhost|::1|127.*) return 0 ;;
    *)                   return 1 ;;
  esac
}

# Host part of an authority: "0.0.0.0:4600" → 0.0.0.0, "[::1]:4600" → ::1.
_host_of() {
  local a="${1%%/*}"
  case "$a" in
    \[*) a="${a#\[}"; printf '%s' "${a%%\]*}" ;;
    *)   printf '%s' "${a%%:*}" ;;
  esac
}

# True when the relay URL is a plaintext scheme aimed at a host that isn't
# loopback. An unrecognised scheme is left for the daemon to reject.
_relay_url_is_plaintext_remote() {
  local rest
  case "$1" in
    ws://*|http://*) rest="${1#*://}" ;;
    *)               return 1 ;;
  esac
  _is_loopback_host "$(_host_of "$rest")" && return 1
  return 0
}

daemon_transport_preflight() {
  if { [ -n "${ASM_TLS_CERT:-}" ] && [ -z "${ASM_TLS_KEY:-}" ]; } ||
     { [ -z "${ASM_TLS_CERT:-}" ] && [ -n "${ASM_TLS_KEY:-}" ]; }; then
    err "--tls-cert and --tls-key must be given together"
    return 1
  fi
  # Validate the certificate with the binary that will use it, before a
  # reconfiguring start.sh stops the daemon that is currently serving fine.
  # Readable is not valid: a key that doesn't match its cert is perfectly
  # readable and perfectly fatal.
  if [ -n "${ASM_TLS_CERT:-}" ] && [ -x "$DAEMON_BIN" ]; then
    local why
    if ! why="$("$DAEMON_BIN" check-tls "$ASM_TLS_CERT" "$ASM_TLS_KEY" 2>&1)"; then
      err "daemon TLS material is unusable — not touching the running daemon:"
      err "  ${why:-unknown error}"
      return 1
    fi
  fi
  # An off-loopback bind WITHOUT a certificate is plaintext. Still allowed — it
  # is a deliberate choice — but say so, and point at the fix that now exists.
  if [ -z "${ASM_TLS_CERT:-}" ] && ! _is_loopback_host "$(_host_of "$ASM_BIND")"; then
    log "note: $ASM_BIND is off-loopback with no certificate, so the device token and terminal"
    log "      traffic are readable on that network. Give it one with --tls-cert PEM --tls-key PEM"
    log "      (clients then use https://), or use a relay / SSH port-forward."
  fi
  if [ -n "${ASM_RELAY_URL:-}" ] && _relay_url_is_plaintext_remote "$ASM_RELAY_URL" \
     && [ "${ASM_ALLOW_INSECURE_RELAY:-}" != 1 ]; then
    err "--register $ASM_RELAY_URL is plaintext to a remote host. The relay hop carries the"
    err "device token and the whole terminal stream, so the daemon refuses it."
    err "  • use wss:// instead (add --relay-ca PEM if the relay's cert is self-signed)"
    err "  • or, if that hop is already encrypted some other way, add --insecure-relay"
    return 1
  fi
  return 0
}

# The relay reads bind/keys/TLS only at startup, so a running relay can't adopt
# new flags in place. This signature captures those settings; start_relay records
# it on launch and compares it on a re-run to decide whether to restart. It IS
# the .reg file's contents (bind|keys|relay-only|tls-cert|tls-key).
relay_reg_signature() {
  printf '%s|%s|%s|%s|%s|%s' \
    "${ASM_RELAY_BIND:-}" "${ASM_RELAY_KEYS:-}" "${ASM_RELAY_ONLY:-0}" \
    "${ASM_RELAY_TLS_CERT:-}" "${ASM_RELAY_TLS_KEY:-}" "${ASM_RELAY_HSTS:-}"
}

# The scheme the relay serves on: TLS when it holds a certificate.
relay_scheme() {
  if [ -n "${ASM_RELAY_TLS_CERT:-}" ]; then printf 'https'; else printf 'http'; fi
}

# The scheme nodes must use to register with it.
relay_ws_scheme() {
  if [ -n "${ASM_RELAY_TLS_CERT:-}" ]; then printf 'wss'; else printf 'ws'; fi
}

wait_relay() {
  local i host key scheme
  local args=(-sf)
  command -v curl >/dev/null 2>&1 || { sleep 0.6; return 0; }
  host="${ASM_RELAY_BIND/0.0.0.0/127.0.0.1}"
  key="${ASM_RELAY_KEYS%%,*}"
  scheme="$(relay_scheme)"
  # A TLS relay's certificate is issued for its public name, not 127.0.0.1, so
  # this loopback readiness probe skips verification. It answers "is it up?",
  # nothing more — the security check is the one real clients make.
  [ "$scheme" = https ] && args+=(-k)
  for i in $(seq 1 60); do
    if curl "${args[@]}" "$scheme://$host/nodes?relay_key=$key" >/dev/null 2>&1; then return 0; fi
    sleep 0.1
  done
  return 1
}

# Start the rendezvous relay (idempotent), if enabled. Runs detached (nohup);
# nodes and clients reach it over the network.
# Everything that can say "this config is wrong" — checked BEFORE a running relay
# is touched. Killing a healthy relay and only then discovering that a flag was
# malformed turns a typo into an outage, so nothing here may run after stop_one.
relay_preflight() {
  [ -x "$RELAY_BIN" ] || { err "missing $RELAY_BIN — build first (cargo build -p asm-relay)"; return 1; }
  if { [ -n "${ASM_RELAY_TLS_CERT:-}" ] && [ -z "${ASM_RELAY_TLS_KEY:-}" ]; } ||
     { [ -z "${ASM_RELAY_TLS_CERT:-}" ] && [ -n "${ASM_RELAY_TLS_KEY:-}" ]; }; then
    err "--relay-tls-cert and --relay-tls-key must be given together"
    return 1
  fi
  # Readable is NOT valid: a mismatched key, an expired-looking PEM, a file full
  # of the wrong thing — all readable, all fatal at boot. `check-tls` runs the
  # binary's real rustls load path, so whatever it accepts here is exactly what
  # the relay will accept in a moment. Bash cannot answer this question; the thing
  # that will use the cert can.
  if [ -n "${ASM_RELAY_TLS_CERT:-}" ]; then
    local why
    if ! why="$("$RELAY_BIN" check-tls "$ASM_RELAY_TLS_CERT" "$ASM_RELAY_TLS_KEY" 2>&1)"; then
      err "relay TLS material is unusable — not touching the running relay:"
      err "  ${why:-unknown error}"
      return 1
    fi
  fi
  return 0
}

# Apply a recorded relay signature to the environment (the inverse of
# relay_reg_signature). Used to roll back to a known-good config.
_relay_apply_signature() {
  local bind keys only cert key_file hsts
  IFS='|' read -r bind keys only cert key_file hsts <<<"$1" || true
  export ASM_RELAY_BIND="$bind" ASM_RELAY_KEYS="$keys" ASM_RELAY_ONLY="${only:-0}"
  if [ -n "$cert" ];     then export ASM_RELAY_TLS_CERT="$cert";    else unset ASM_RELAY_TLS_CERT; fi
  if [ -n "$key_file" ]; then export ASM_RELAY_TLS_KEY="$key_file"; else unset ASM_RELAY_TLS_KEY; fi
  if [ -n "$hsts" ];     then export ASM_RELAY_HSTS="$hsts";        else unset ASM_RELAY_HSTS; fi
}

# Launch the relay from the current env and wait for it to answer. Records its
# config only once it is actually up, so a failed start never leaves a recording
# describing a relay that isn't running.
_relay_spawn() {
  log "starting asm-relay on $ASM_RELAY_BIND..."
  ASM_RELAY_BIND="$ASM_RELAY_BIND" ASM_RELAY_KEYS="$ASM_RELAY_KEYS" \
  ASM_RELAY_TLS_CERT="${ASM_RELAY_TLS_CERT:-}" ASM_RELAY_TLS_KEY="${ASM_RELAY_TLS_KEY:-}" \
  ASM_RELAY_HSTS="${ASM_RELAY_HSTS:-}" \
    nohup "$RELAY_BIN" >>"$LOG_DIR/asm-relay.log" 2>&1 </dev/null &
  echo $! > "$RELAY_PIDFILE"
  if wait_relay; then
    relay_reg_signature > "$RELAY_STATE_FILE"
    log "relay up (pid $(cat "$RELAY_PIDFILE"))  $(relay_scheme)://$ASM_RELAY_BIND"
    return 0
  fi
  return 1
}

start_relay() {
  relay_enabled || { log "relay disabled (pass --relay --relay-key KEY to bundle one)"; return 0; }
  relay_preflight || return 1
  # The config the running relay is serving on, kept so a failed replacement can
  # be rolled back to it rather than leaving the box with no relay at all.
  local prior=""
  if pid_alive "$RELAY_PIDFILE"; then
    local want recorded
    want="$(relay_reg_signature)"
    recorded="$(cat "$RELAY_STATE_FILE" 2>/dev/null || true)"
    # The relay reads its config only at startup, so a live one cannot adopt new
    # flags. Without this, `--relay-tls-cert …` against a running plaintext relay
    # returned "already running" and the script then cheerfully reported
    # `https://…` while the process stayed in the clear — the exact silent
    # downgrade the recorded config exists to prevent. Restart only when relay
    # flags were passed AND they actually change something: connected nodes and
    # clients drop for a moment, so this must not fire on a flagless re-run.
    if [ "${ASM_RELAY_RECONFIG:-0}" = 1 ] && [ "$want" != "$recorded" ]; then
      log "relay already running (pid $(cat "$RELAY_PIDFILE")) — config changed, restarting to apply it"
      log "  (connected nodes and clients drop briefly; nodes reconnect on their own)"
      prior="$recorded"
      stop_one asm-relay "$RELAY_PIDFILE"
      # fall through and launch a fresh relay with the new config
    else
      log "relay already running (pid $(cat "$RELAY_PIDFILE"))"
      return 0
    fi
  fi
  if [ -z "${ASM_RELAY_TLS_CERT:-}" ] && ! _is_loopback_host "$(_host_of "$ASM_RELAY_BIND")"; then
    log "note: this relay has no TLS certificate, so device tokens and terminal traffic"
    log "      cross it in the clear. For anything but a trusted LAN, pass"
    log "      --relay-tls-cert PEM --relay-tls-key PEM (or front it with a TLS proxy)."
  fi

  _relay_spawn && return 0

  err "relay did not come up; see $LOG_DIR/asm-relay.log"
  # We stopped a working relay to apply this config and the replacement failed:
  # put the old one back rather than leaving the host with no relay at all. Every
  # *predictable* cause was caught by relay_preflight, so reaching here means
  # something environmental (a taken port, a crash) — which the previous config,
  # by definition, did not have.
  if [ -n "$prior" ]; then
    err "rolling back to the previous relay configuration..."
    _relay_apply_signature "$prior"
    if _relay_spawn; then
      err "rolled back: the relay is up on its PREVIOUS config. The new one was NOT applied."
    else
      err "rollback failed too — the relay is DOWN. See $LOG_DIR/asm-relay.log"
    fi
  fi
  return 1
}

# Start the holder (idempotent). It runs detached (nohup) so it outlives the
# daemon — that is what makes sessions durable across a daemon restart.
# Does a LIVE holder answer on the socket? This is the only check that means
# anything: a pid can be alive while the socket is gone (an "orphan" — the holder
# still holds PTYs but nothing can dial it). Gating on pid alone is what let the
# 2026-07-12 incident wedge the stack; `asmux probe` exits 0 only if someone answers.
holder_live() {
  [ -x "$ASMUX_BIN" ] || return 1
  ASMUX_SOCK="$ASMUX_SOCK" ASM_RUNTIME_DIR="$ASM_RUNTIME_DIR" \
    "$ASMUX_BIN" probe >/dev/null 2>&1
}

start_asmux() {
  if holder_live; then
    log "asmux already running (pid $(cat "$ASMUX_PIDFILE" 2>/dev/null || echo '?'))"
    return 0
  fi

  # Pid alive but nobody answering = an orphaned holder: its socket was unlinked
  # out from under it. asmux now rebinds itself within ~5s, so give it a moment
  # before concluding anything.
  if pid_alive "$ASMUX_PIDFILE"; then
    local p i
    p="$(cat "$ASMUX_PIDFILE")"
    log "asmux (pid $p) is alive but not answering on $ASMUX_SOCK — waiting for it to rebind..."
    for i in $(seq 1 20); do
      sleep 0.5
      if holder_live; then log "asmux recovered its socket (pid $p) — sessions intact"; return 0; fi
    done
    err "asmux (pid $p) is ORPHANED: alive, holding live PTYs, but unreachable — its socket path"
    err "is owned by something else, or rebinding failed. Its sessions CANNOT be attached, and"
    err "killing it will lose them. Inspect first:  $LOG_DIR/asmux.log"
    err "To force a clean holder anyway (THIS KILLS ITS SESSIONS):"
    err "    kill $p && scripts/start.sh"
    return 1
  fi

  [ -x "$ASMUX_BIN" ] || { err "missing $ASMUX_BIN — build first (cargo build -p asmux)"; return 1; }
  log "starting asmux holder..."
  ASM_RUNTIME_DIR="$ASM_RUNTIME_DIR" ASMUX_SOCK="$ASMUX_SOCK" \
    nohup "$ASMUX_BIN" >>"$LOG_DIR/asmux.log" 2>&1 </dev/null &
  echo $! > "$ASMUX_PIDFILE"
  if wait_socket; then
    log "asmux up (pid $(cat "$ASMUX_PIDFILE"))  socket=$ASMUX_SOCK"
  else
    err "asmux did not come up; see $LOG_DIR/asmux.log"
    return 1
  fi
}

# The daemon reads bind/label/relay-registration config only at startup, so a
# running daemon can't adopt new flags in place. This signature captures those
# daemon-affecting settings; start_daemon records it on launch and compares it on
# a re-run to decide whether the config actually changed.
daemon_reg_signature() {
  printf '%s|%s|%s|%s|%s|%s|%s|%s|%s' \
    "${ASM_BIND:-}" "${ASM_NODE_LABEL:-}" "${ASM_RELAY_URL:-}" "${ASM_RELAY_KEY:-}" \
    "${ASM_RELAY_CA:-}" "${ASM_ALLOW_INSECURE_RELAY:-}" \
    "${ASM_TLS_CERT:-}" "${ASM_TLS_KEY:-}" "${ASM_TRUST_LOOPBACK:-}"
}

# Start the daemon in sidecar mode (idempotent). Autospawn is off — the scripts
# manage the holder so restarting the daemon never spawns a second one. Any
# ASM_RELAY_URL/ASM_RELAY_KEY (from --register) is inherited, so the daemon
# registers outbound when configured to.
#
# If the daemon is already running but this invocation passed daemon-affecting
# flags (ASM_DAEMON_RECONFIG=1) whose signature differs from what the running
# daemon booted with, restart it to apply them — the asmux holder stays up, so
# live sessions survive. Without this, start.sh silently drops the new flags
# (e.g. a changed --register never taking effect).
start_daemon() {
  if pid_alive "$DAEMON_PIDFILE"; then
    local want recorded
    want="$(daemon_reg_signature)"
    recorded="$(cat "$DAEMON_STATE_FILE" 2>/dev/null || true)"
    if [ "${ASM_DAEMON_RECONFIG:-0}" = 1 ] && [ "$want" != "$recorded" ]; then
      log "daemon already running (pid $(cat "$DAEMON_PIDFILE")) — config changed, restarting to apply it (live sessions survive)"
      stop_one asm-daemon "$DAEMON_PIDFILE"
      # fall through to launch a fresh daemon with the new config
    else
      log "daemon already running (pid $(cat "$DAEMON_PIDFILE"))"
      return 0
    fi
  fi
  [ -x "$DAEMON_BIN" ] || { err "missing $DAEMON_BIN — build first (cargo build -p asm-daemon)"; return 1; }
  log "starting asm-daemon (sidecar) on $ASM_BIND..."
  ASM_BACKEND=sidecar ASM_ASMUX_AUTOSPAWN=0 \
    nohup "$DAEMON_BIN" >>"$LOG_DIR/asm-daemon.log" 2>&1 </dev/null &
  echo $! > "$DAEMON_PIDFILE"
  daemon_reg_signature > "$DAEMON_STATE_FILE"
  if wait_health; then
    log "daemon up (pid $(cat "$DAEMON_PIDFILE"))  $(daemon_scheme)://$ASM_BIND"
  else
    err "daemon health check failed; see $LOG_DIR/asm-daemon.log"
    return 1
  fi
}

# SIGTERM a pidfile's process, escalate to SIGKILL if it lingers.
stop_one() {
  local name="$1" f="$2" p i
  if pid_alive "$f"; then
    p="$(cat "$f")"
    log "stopping $name (pid $p)..."
    kill -TERM "$p" 2>/dev/null || true
    for i in $(seq 1 40); do
      if ! kill -0 "$p" 2>/dev/null; then break; fi
      sleep 0.1
    done
    if kill -0 "$p" 2>/dev/null; then
      err "$name did not stop; sending SIGKILL"
      kill -KILL "$p" 2>/dev/null || true
    fi
  else
    log "$name not running"
  fi
  rm -f "$f"
}
