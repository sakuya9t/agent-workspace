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

_bool() { # _bool FLAG VALUE — print 1/0 for a supported boolean spelling
  case "${2:-}" in
    1|true|yes|on)  printf '1' ;;
    0|false|no|off) printf '0' ;;
    *) err "option $1 expects true or false"; return 2 ;;
  esac
}

_port() { # _port FLAG VALUE — validate a TCP port before doing an expensive build
  if [[ "${2:-}" =~ ^[0-9]+$ ]] && [ "$2" -ge 1 ] && [ "$2" -le 65535 ]; then
    return 0
  fi
  err "option $1 expects a port between 1 and 65535"
  return 2
}

# Translate flags into the ASM_* env vars the binaries read (env stays the
# fallback: a flag only overrides when given). Collects any non-flag args into
# ASM_POSITIONAL and sets ASM_SHOW_HELP. Returns non-zero on a bad flag.
asm_parse_args() {
  ASM_POSITIONAL=()
  ASM_SHOW_HELP=0
  local key="" want_relay=0 reconfig=0 ui_reconfig=0 ui_value
  while [ $# -gt 0 ]; do
    case "$1" in
      --bind)         _need "$1" "${2:-}" || return 2; export ASM_BIND="$2"; reconfig=1; shift 2 ;;
      --data-dir)     _need "$1" "${2:-}" || return 2; export ASM_DATA_DIR="$2"; shift 2 ;;
      --runtime-dir)  _need "$1" "${2:-}" || return 2; export ASM_RUNTIME_DIR="$2"; shift 2 ;;
      --label)        _need "$1" "${2:-}" || return 2; export ASM_NODE_LABEL="$2"; reconfig=1; shift 2 ;;
      --release)      PROFILE=release; shift ;;
      --relay)        want_relay=1; shift ;;
      --relay-only)   want_relay=1; export ASM_RELAY_ONLY=1; shift ;;
      --relay-bind)   _need "$1" "${2:-}" || return 2; export ASM_RELAY_BIND="$2"; want_relay=1; shift 2 ;;
      --relay-key)    _need "$1" "${2:-}" || return 2; key="$2"; shift 2 ;;
      --register)     _need "$1" "${2:-}" || return 2; export ASM_RELAY_URL="$2"; reconfig=1; shift 2 ;;
      --ui|--run-ui)  export ASM_RUN_UI=1 ASM_UI_ONLY=0; unset ASM_UI_DAEMON ASM_UI_DAEMON_TOKEN; ui_reconfig=1; shift ;;
      --no-ui)        export ASM_RUN_UI=0 ASM_UI_ONLY=0; unset ASM_UI_DAEMON ASM_UI_DAEMON_TOKEN; ui_reconfig=1; shift ;;
      --run-ui=*)     ui_value="$(_bool --run-ui "${1#*=}")" || return 2; export ASM_RUN_UI="$ui_value" ASM_UI_ONLY=0; unset ASM_UI_DAEMON ASM_UI_DAEMON_TOKEN; ui_reconfig=1; shift ;;
      --ui-only)      export ASM_RUN_UI=1 ASM_UI_ONLY=1; ui_reconfig=1; shift ;;
      --ui-daemon)    _need "$1" "${2:-}" || return 2; export ASM_UI_DAEMON="$2" ASM_RUN_UI=1 ASM_UI_ONLY=1; ui_reconfig=1; shift 2 ;;
      --ui-daemon-token) _need "$1" "${2:-}" || return 2; export ASM_UI_DAEMON_TOKEN="$2" ASM_RUN_UI=1 ASM_UI_ONLY=1; ui_reconfig=1; shift 2 ;;
      --ui-host)      _need "$1" "${2:-}" || return 2; export ASM_UI_HOST="$2" ASM_RUN_UI=1; ui_reconfig=1; shift 2 ;;
      --ui-port)      _need "$1" "${2:-}" || return 2; _port "$1" "$2" || return 2; export ASM_UI_PORT="$2" ASM_RUN_UI=1; ui_reconfig=1; shift 2 ;;
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
  export ASM_UI_RECONFIG="$ui_reconfig"
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
  case "${ASM_RUN_UI:-1}" in
    1|true|yes|on)  export ASM_RUN_UI=1 ;;
    0|false|no|off) export ASM_RUN_UI=0 ;;
    *) err "ASM_RUN_UI expects true or false"; return 2 ;;
  esac
  case "${ASM_UI_ONLY:-0}" in
    1|true|yes|on)  export ASM_UI_ONLY=1 ;;
    0|false|no|off) export ASM_UI_ONLY=0 ;;
    *) err "ASM_UI_ONLY expects true or false"; return 2 ;;
  esac
  export ASM_UI_HOST="${ASM_UI_HOST:-${ASM_CLIENT_HOST:-127.0.0.1}}"
  export ASM_UI_PORT="${ASM_UI_PORT:-5273}"
  export ASM_UI_DAEMON="${ASM_UI_DAEMON:-}"
  export ASM_UI_DAEMON_TOKEN="${ASM_UI_DAEMON_TOKEN:-}"

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
  UI_PIDFILE="$ASM_RUNTIME_DIR/asm-ui.pid"
  # enabled|host|port|daemon-bind|ui-only|proxy-url|proxy-token. Recorded
  # independently because Vite can stay up while the daemon is rebuilt.
  UI_STATE_FILE="$ASM_RUNTIME_DIR/asm-ui.reg"
  UI_BIN="$ROOT/client/node_modules/.bin/vite"

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

wait_health() {
  local i
  command -v curl >/dev/null 2>&1 || { sleep 0.6; return 0; }
  for i in $(seq 1 60); do
    if curl -sf "http://$ASM_BIND/health" >/dev/null 2>&1; then return 0; fi
    sleep 0.1
  done
  return 1
}

# The daemon binds BEFORE it adopts the holder's sessions, so `/health` answers
# in milliseconds however many survived (that is why the check above can keep a
# short fuse). Adoption is a serial holder round-trip per session and lands a
# little later; this waits for it, purely so a script can honestly claim the
# sessions are back. 60s: adoption is ~1s/session in the worst case seen.
wait_reconciled() {
  local i
  command -v curl >/dev/null 2>&1 || return 0
  for i in $(seq 1 600); do
    if curl -sf "http://$ASM_BIND/health" 2>/dev/null | grep -q '"reconciling":false'; then
      return 0
    fi
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

# Daemon side (DAEMON_STATE_FILE: bind|label|register-url|register-key).
# Any daemon-affecting flag (--bind/--label/--register) re-specifies the daemon
# config as a whole — fields you don't pass revert to env/defaults. With no such
# flag, the recording is authoritative, including its empty fields.
daemon_load_recorded_config() {
  if [ "${ASM_DAEMON_RECONFIG:-0}" = 1 ]; then return 0; fi
  local rec bind label url key
  rec="$(cat "$DAEMON_STATE_FILE" 2>/dev/null || true)"
  if [ -z "$rec" ]; then return 0; fi
  IFS='|' read -r bind label url key <<<"$rec" || true
  if [ -n "$bind" ]; then export ASM_BIND="$bind"; fi
  if [ -n "$label" ]; then export ASM_NODE_LABEL="$label"; else unset ASM_NODE_LABEL; fi
  if [ -n "$url" ];   then export ASM_RELAY_URL="$url";     else unset ASM_RELAY_URL; fi
  if [ -n "$key" ];   then export ASM_RELAY_KEY="$key";     else unset ASM_RELAY_KEY; fi
}

# Relay side (RELAY_STATE_FILE: bind|keys|relay-only). Skipped whenever a
# --relay* flag was passed — that re-specifies the relay config as a whole.
# Relay-only-ness is restored only on a fully flagless run: passing a daemon
# flag says "I want the daemon here too".
relay_load_recorded_config() {
  if [ "${ASM_RELAY_RECONFIG:-0}" = 1 ]; then return 0; fi
  local rec bind keys only
  rec="$(cat "$RELAY_STATE_FILE" 2>/dev/null || true)"
  if [ -z "$rec" ]; then return 0; fi
  IFS='|' read -r bind keys only <<<"$rec" || true
  if [ -z "$keys" ]; then return 0; fi
  export ASM_RELAY_KEYS="$keys"
  if [ -n "$bind" ]; then export ASM_RELAY_BIND="$bind"; fi
  if [ "${ASM_DAEMON_RECONFIG:-0}" = 0 ]; then export ASM_RELAY_ONLY="${only:-0}"; fi
}

# UI side (enabled|host|port|daemon-bind|ui-only|proxy-url|proxy-token). As with
# daemon config, explicit UI flags win; otherwise a flagless start restores the
# last selection and revives Vite if it died. The first four fields preserve
# compatibility with UI records created before UI-only gateway mode existed.
ui_load_recorded_config() {
  if [ "${ASM_UI_RECONFIG:-0}" = 1 ]; then return 0; fi
  local rec enabled host port daemon_bind only daemon token
  rec="$(cat "$UI_STATE_FILE" 2>/dev/null || true)"
  if [ -z "$rec" ]; then return 0; fi
  IFS='|' read -r enabled host port daemon_bind only daemon token <<<"$rec" || true
  export ASM_RUN_UI="${enabled:-0}"
  if [ -n "$host" ]; then export ASM_UI_HOST="$host"; fi
  if [ -n "$port" ]; then export ASM_UI_PORT="$port"; fi
  if [ -n "$daemon_bind" ]; then export ASM_BIND="$daemon_bind"; fi
  export ASM_UI_ONLY="${only:-0}"
  export ASM_UI_DAEMON="${daemon:-}"
  export ASM_UI_DAEMON_TOKEN="${token:-}"
}

ui_enabled() { [ "${ASM_RUN_UI:-0}" = 1 ]; }
ui_only() { [ "${ASM_UI_ONLY:-0}" = 1 ]; }

ui_reg_signature() {
  printf '%s|%s|%s|%s|%s|%s|%s' \
    "${ASM_RUN_UI:-0}" "${ASM_UI_HOST:-}" "${ASM_UI_PORT:-}" "${ASM_BIND:-}" \
    "${ASM_UI_ONLY:-0}" "${ASM_UI_DAEMON:-}" "${ASM_UI_DAEMON_TOKEN:-}"
}

record_ui_state() {
  # This record may contain a gateway bearer token. Tighten permissions even
  # when replacing a state file created by an older version under a broad umask.
  (
    umask 077
    ui_reg_signature > "$UI_STATE_FILE"
    chmod 600 "$UI_STATE_FILE"
  )
}

# Turn a wildcard listen host into an address curl/the proxy can dial locally.
ui_local_host() {
  case "${1:-}" in
    0.0.0.0|true|'*') printf '127.0.0.1' ;;
    ::|'[::]')         printf '[::1]' ;;
    *)                 printf '%s' "$1" ;;
  esac
}

ui_url() {
  printf 'http://%s:%s' "$(ui_local_host "$ASM_UI_HOST")" "$ASM_UI_PORT"
}

ui_daemon_url() {
  local bind_host_port="$ASM_BIND"
  if [ -n "${ASM_UI_DAEMON:-}" ]; then
    printf '%s' "${ASM_UI_DAEMON%/}"
    return 0
  fi
  case "$bind_host_port" in
    0.0.0.0:*) bind_host_port="127.0.0.1:${bind_host_port#*:}" ;;
    \[::\]:*)  bind_host_port="[::1]:${bind_host_port#*]:}" ;;
  esac
  printf 'http://%s' "$bind_host_port"
}

wait_ui() {
  local i
  command -v curl >/dev/null 2>&1 || { sleep 0.8; pid_alive "$UI_PIDFILE"; return; }
  for i in $(seq 1 100); do
    pid_alive "$UI_PIDFILE" || return 1
    if curl -sf "$(ui_url)" >/dev/null 2>&1; then return 0; fi
    sleep 0.1
  done
  return 1
}

# Start the Vite development UI as a detached, directly-managed process. Calling
# the local Vite executable (rather than leaving npm as an extra parent) makes
# the pidfile identify the server we need to stop. Its proxy follows ASM_BIND.
start_ui() {
  local want recorded
  want="$(ui_reg_signature)"
  recorded="$(cat "$UI_STATE_FILE" 2>/dev/null || true)"
  if pid_alive "$UI_PIDFILE"; then
    if [ "$want" != "$recorded" ]; then
      log "web UI already running (pid $(cat "$UI_PIDFILE")) — config changed, restarting it"
      stop_one asm-ui "$UI_PIDFILE"
    else
      log "web UI already running (pid $(cat "$UI_PIDFILE"))  $(ui_url)"
      return 0
    fi
  fi

  if [ -n "${ASM_UI_DAEMON:-}" ]; then
    case "$ASM_UI_DAEMON" in
      http://*|https://*) : ;;
      *) err "--ui-daemon expects an http:// or https:// URL"; return 2 ;;
    esac
  fi
  if ! [[ "$ASM_UI_PORT" =~ ^[0-9]+$ ]] || [ "$ASM_UI_PORT" -lt 1 ] || [ "$ASM_UI_PORT" -gt 65535 ]; then
    err "UI port must be between 1 and 65535 (got: $ASM_UI_PORT)"
    return 2
  fi
  command -v node >/dev/null 2>&1 || {
    err "managed UI needs Node.js; install Node 20+ or pass --no-ui for daemon-only/packaged UI"
    return 1
  }
  [ -x "$UI_BIN" ] || {
    err "missing $UI_BIN — install client dependencies first: (cd client && npm install)"
    return 1
  }

  log "starting Vite web UI on $ASM_UI_HOST:$ASM_UI_PORT..."
  (
    cd "$ROOT/client"
    ASM_DAEMON="$(ui_daemon_url)" ASM_DAEMON_TOKEN="$ASM_UI_DAEMON_TOKEN" \
      ASM_CLIENT_HOST="$ASM_UI_HOST" \
      nohup "$UI_BIN" --host "$ASM_UI_HOST" --port "$ASM_UI_PORT" --strictPort \
      >>"$LOG_DIR/asm-ui.log" 2>&1 </dev/null &
    echo $! > "$UI_PIDFILE"
  )
  record_ui_state
  if wait_ui; then
    log "web UI up (pid $(cat "$UI_PIDFILE"))  $(ui_url)"
  else
    err "web UI did not come up; see $LOG_DIR/asm-ui.log"
    rm -f "$UI_PIDFILE"
    return 1
  fi
}

# Apply the desired UI state. A recorded/explicit enabled state starts or revives
# it; only an explicit disabling flag stops a live managed UI.
sync_ui() {
  if ui_enabled; then
    start_ui
  elif [ "${ASM_UI_RECONFIG:-0}" = 1 ]; then
    stop_one asm-ui "$UI_PIDFILE"
    record_ui_state
  fi
}

wait_relay() {
  local i host key
  command -v curl >/dev/null 2>&1 || { sleep 0.6; return 0; }
  host="${ASM_RELAY_BIND/0.0.0.0/127.0.0.1}"
  key="${ASM_RELAY_KEYS%%,*}"
  for i in $(seq 1 60); do
    if curl -sf "http://$host/nodes?relay_key=$key" >/dev/null 2>&1; then return 0; fi
    sleep 0.1
  done
  return 1
}

# Start the rendezvous relay (idempotent), if enabled. Runs detached (nohup);
# nodes and clients reach it over the network.
start_relay() {
  relay_enabled || { log "relay disabled (pass --relay --relay-key KEY to bundle one)"; return 0; }
  if pid_alive "$RELAY_PIDFILE"; then
    log "relay already running (pid $(cat "$RELAY_PIDFILE"))"
    return 0
  fi
  [ -x "$RELAY_BIN" ] || { err "missing $RELAY_BIN — build first (cargo build -p asm-relay)"; return 1; }
  log "starting asm-relay on $ASM_RELAY_BIND..."
  ASM_RELAY_BIND="$ASM_RELAY_BIND" ASM_RELAY_KEYS="$ASM_RELAY_KEYS" \
    nohup "$RELAY_BIN" >>"$LOG_DIR/asm-relay.log" 2>&1 </dev/null &
  echo $! > "$RELAY_PIDFILE"
  printf '%s|%s|%s' "$ASM_RELAY_BIND" "$ASM_RELAY_KEYS" "${ASM_RELAY_ONLY:-0}" > "$RELAY_STATE_FILE"
  if wait_relay; then
    log "relay up (pid $(cat "$RELAY_PIDFILE"))  http://$ASM_RELAY_BIND"
  else
    err "relay did not come up; see $LOG_DIR/asm-relay.log"
    return 1
  fi
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
  printf '%s|%s|%s|%s' \
    "${ASM_BIND:-}" "${ASM_NODE_LABEL:-}" "${ASM_RELAY_URL:-}" "${ASM_RELAY_KEY:-}"
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
    log "daemon up (pid $(cat "$DAEMON_PIDFILE"))  http://$ASM_BIND"
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
