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
      --data-dir)     _need "$1" "${2:-}" || return 2; export ASM_DATA_DIR="$2"; shift 2 ;;
      --runtime-dir)  _need "$1" "${2:-}" || return 2; export ASM_RUNTIME_DIR="$2"; shift 2 ;;
      --label)        _need "$1" "${2:-}" || return 2; export ASM_NODE_LABEL="$2"; reconfig=1; shift 2 ;;
      --release)      PROFILE=release; shift ;;
      --relay)        want_relay=1; shift ;;
      --relay-bind)   _need "$1" "${2:-}" || return 2; export ASM_RELAY_BIND="$2"; want_relay=1; shift 2 ;;
      --relay-key)    _need "$1" "${2:-}" || return 2; key="$2"; shift 2 ;;
      --register)     _need "$1" "${2:-}" || return 2; export ASM_RELAY_URL="$2"; reconfig=1; shift 2 ;;
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
  # Record whether a daemon-affecting override (bind/label/relay registration) was
  # passed THIS invocation, so start_daemon can re-apply it to an already-running
  # daemon instead of silently dropping it. Always set fresh — it describes this
  # command line, not persisted config, so it is never an env fallback.
  export ASM_DAEMON_RECONFIG="$reconfig"
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
  # tell when a re-run's flags actually change it (see start_daemon).
  DAEMON_STATE_FILE="$ASM_RUNTIME_DIR/asm-daemon.reg"
  RELAY_PIDFILE="$ASM_RUNTIME_DIR/asm-relay.pid"

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

# The relay is enabled purely by config (a relay access key is present), never
# by the daemon binary — it is a shared rendezvous, not a per-daemon sidecar.
relay_enabled() { [ -n "${ASM_RELAY_KEYS:-}" ]; }

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
  if wait_relay; then
    log "relay up (pid $(cat "$RELAY_PIDFILE"))  http://$ASM_RELAY_BIND"
  else
    err "relay did not come up; see $LOG_DIR/asm-relay.log"
    return 1
  fi
}

# Start the holder (idempotent). It runs detached (nohup) so it outlives the
# daemon — that is what makes sessions durable across a daemon restart.
start_asmux() {
  if pid_alive "$ASMUX_PIDFILE"; then
    log "asmux already running (pid $(cat "$ASMUX_PIDFILE"))"
    return 0
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
