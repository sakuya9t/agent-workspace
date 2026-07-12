#!/usr/bin/env bash
# Start the durable stack: the asmux holder + the daemon in sidecar mode, and —
# with --relay — a bundled rendezvous relay on this host. --relay-only starts
# JUST the relay (a pure rendezvous box; no sessions run here). Sessions survive
# a daemon restart (see scripts/restart-daemon.sh).
#
# Options are flags (the ASM_* env vars still work as fallbacks). See --help.
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=_asm_common.sh
source "$HERE/_asm_common.sh"

usage() {
  cat <<'USAGE'
Usage: scripts/start.sh [options]

  --bind ADDR          daemon listen address (default 127.0.0.1:4600)
  --data-dir DIR       persistent data dir (default ~/.local/share/asm)
  --runtime-dir DIR    sockets/pidfiles dir (default $XDG_RUNTIME_DIR/asm)
  --label NAME         this node's label (default: hostname)
  --release            build/run the release profile
  -h, --help           show this help

  A flagless run keeps the settings recorded from the previous launch (bind,
  label, relay registration, bundled relay) instead of reverting to defaults;
  recorded settings also beat inherited ASM_* env — pass flags to change them.

  Relay host — a reachable box that NAT'd nodes register to:
  --relay              run a bundled rendezvous relay here (alongside the daemon)
  --relay-only         run ONLY the relay — no holder/daemon, no sessions on this box
  --relay-bind ADDR    relay listen address (default 0.0.0.0:4700; implies --relay)
  --relay-key KEY      shared relay access key

  NAT'd node — registers OUTBOUND to a relay (no relay runs here):
  --register URL       relay to register to, e.g. ws://relay-host:4700
  --relay-key KEY      the relay's access key

Examples:
  scripts/start.sh --bind 0.0.0.0:4600
  scripts/start.sh --relay --relay-key s3cret               # relay host + daemon
  scripts/start.sh --relay-only --relay-key s3cret          # just the relay, no daemon
  scripts/start.sh --register ws://192.168.122.1:4700 --relay-key s3cret
USAGE
}

asm_parse_args "$@" || { usage; exit 2; }
[ "${ASM_SHOW_HELP:-0}" = 1 ] && { usage; exit 0; }
asm_configure

# A flagless run brings the stack up as it was recorded (bind/label/registration,
# bundled relay, relay-only-ness) rather than reverting to defaults.
daemon_load_recorded_config
relay_load_recorded_config

# Relay-only: build and start just the rendezvous relay — no holder, no daemon.
if [ "${ASM_RELAY_ONLY:-0}" = 1 ]; then
  relay_enabled || { err "--relay-only needs --relay-key KEY (the access key nodes present)"; exit 2; }
  log "building asm-relay ($PROFILE)..."
  cargo_build -p asm-relay
  start_relay
  log "ready — relay http://$ASM_RELAY_BIND (nodes register here; no sessions on this box)"
  log "logs   — $LOG_DIR/asm-relay.log"
  log "next   — scripts/status.sh · scripts/stop.sh relay"
  exit 0
fi

build_targets=(-p asm-daemon -p asmux)
relay_enabled && build_targets+=(-p asm-relay)
log "building asm-daemon + asmux$(relay_enabled && echo ' + asm-relay') ($PROFILE)..."
cargo_build "${build_targets[@]}"

start_relay
start_asmux
start_daemon

backend="$(command -v curl >/dev/null 2>&1 && curl -s "http://$ASM_BIND/health" | sed -n 's/.*"backend":"\([^"]*\)".*/\1/p' || true)"
log "ready — http://$ASM_BIND (backend=${backend:-?})"
# The daemon serves the packaged web client itself when ASM_STATIC_DIR points at
# a build (auto-set from client/dist in _asm_common.sh) — no npm/vite needed.
if [ -n "${ASM_STATIC_DIR:-}" ] && [ -d "$ASM_STATIC_DIR" ]; then
  log "web UI — http://$ASM_BIND (served from ${ASM_STATIC_DIR#"$ROOT"/})"
fi
relay_enabled && log "relay  — http://$ASM_RELAY_BIND (nodes register here)"
[ -n "${ASM_RELAY_URL:-}" ] && log "node   — registering to $ASM_RELAY_URL"
log "logs   — $LOG_DIR/{asmux,asm-daemon$(relay_enabled && echo ',asm-relay')}.log"
log "next   — scripts/status.sh · scripts/restart-daemon.sh · scripts/stop.sh"
