#!/usr/bin/env bash
# Shared config + helpers for the asm service scripts. SOURCED, not run.
#
# Both the daemon and the asmux holder get the SAME ASM_DATA_DIR / ASM_RUNTIME_DIR
# here so they agree on the socket path (<runtime_dir>/asmux.sock) and the data
# lives in one place across restarts. Override any of ASM_DATA_DIR,
# ASM_RUNTIME_DIR, ASM_BIND, or PROFILE/RELEASE from the environment.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# Build profile: `RELEASE=1` or `PROFILE=release` uses target/release.
if [ "${RELEASE:-0}" = "1" ]; then PROFILE=release; fi
PROFILE="${PROFILE:-debug}"
BIN_DIR="$ROOT/target/$PROFILE"
DAEMON_BIN="$BIN_DIR/asm-daemon"
ASMUX_BIN="$BIN_DIR/asmux"

# Persistent locations (shared by daemon + asmux).
export ASM_DATA_DIR="${ASM_DATA_DIR:-$HOME/.local/share/asm}"
export ASM_RUNTIME_DIR="${ASM_RUNTIME_DIR:-${XDG_RUNTIME_DIR:-/tmp}/asm}"
export ASM_BIND="${ASM_BIND:-127.0.0.1:4600}"

LOG_DIR="$ASM_DATA_DIR/logs"
ASMUX_SOCK="$ASM_RUNTIME_DIR/asmux.sock"
ASMUX_PIDFILE="$ASM_RUNTIME_DIR/asmux.pid"
DAEMON_PIDFILE="$ASM_RUNTIME_DIR/asm-daemon.pid"

mkdir -p "$ASM_DATA_DIR" "$ASM_RUNTIME_DIR" "$LOG_DIR"

log() { printf '\033[1;36m[asm]\033[0m %s\n' "$*"; }
err() { printf '\033[1;31m[asm]\033[0m %s\n' "$*" >&2; }

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

# Start the daemon in sidecar mode (idempotent). Autospawn is off — the scripts
# manage the holder so restarting the daemon never spawns a second one.
start_daemon() {
  if pid_alive "$DAEMON_PIDFILE"; then
    log "daemon already running (pid $(cat "$DAEMON_PIDFILE"))"
    return 0
  fi
  [ -x "$DAEMON_BIN" ] || { err "missing $DAEMON_BIN — build first (cargo build -p asm-daemon)"; return 1; }
  log "starting asm-daemon (sidecar) on $ASM_BIND..."
  ASM_BACKEND=sidecar ASM_ASMUX_AUTOSPAWN=0 \
    nohup "$DAEMON_BIN" >>"$LOG_DIR/asm-daemon.log" 2>&1 </dev/null &
  echo $! > "$DAEMON_PIDFILE"
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
