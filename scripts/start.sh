#!/usr/bin/env bash
# Start the durable stack: the asmux holder + the daemon in sidecar mode, plus
# a default detached Vite UI and optional rendezvous relay. --relay-only starts
# JUST the relay (a pure rendezvous box; no sessions run here). Sessions survive
# a daemon restart (see scripts/restart-daemon.sh), and every managed process
# survives the SSH session that launched it.
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

  Managed development UI (detached; enabled by default):
  --ui                 explicitly enable/reconfigure the default UI
  --no-ui              disable/stop it (same as --run-ui=false)
  --ui-only            run ONLY the UI gateway; no relay, holder, or daemon
  --ui-daemon URL      backend for UI-only mode (implies --ui-only)
  --ui-daemon-token TOKEN
                       bearer token injected into that backend proxy
  --ui-host HOST       UI listen host (default 127.0.0.1; implies --ui)
  --ui-port PORT       UI listen port (default 5273; implies --ui)

  A flagless run keeps the settings recorded from the previous launch (bind,
  label, relay registration, bundled relay, managed UI) instead of reverting to
  defaults; recorded settings beat inherited ASM_* env — pass flags to change them.

  Relay host — a reachable box that NAT'd nodes register to:
  --relay              run a bundled rendezvous relay here (alongside the daemon)
  --relay-only         run ONLY the relay — no holder/daemon, no sessions on this box
  --relay-bind ADDR    relay listen address (default 0.0.0.0:4700; implies --relay)
  --relay-key KEY      shared relay access key

  NAT'd node — registers OUTBOUND to a relay (no relay runs here):
  --register URL       relay to register to, e.g. ws://relay-host:4700
  --relay-key KEY      the relay's access key

Examples:
  scripts/start.sh                                       # daemon + local persistent Vite UI
  scripts/start.sh --no-ui                               # daemon/holder only
  scripts/start.sh --ui-only                             # UI shell/gateway, no local daemon
  scripts/start.sh --ui-only --ui-daemon http://host:4600
  scripts/start.sh --ui-host 0.0.0.0                     # expose UI on a trusted LAN
  scripts/start.sh --bind 0.0.0.0:4600
  scripts/start.sh --relay --relay-key s3cret               # relay host + daemon
  scripts/start.sh --relay-only --relay-key s3cret          # just the relay, no daemon
  scripts/start.sh --register ws://192.168.122.1:4700 --relay-key s3cret
USAGE
}

asm_parse_args "$@" || { usage; exit 2; }
[ "${ASM_SHOW_HELP:-0}" = 1 ] && { usage; exit 0; }
asm_configure

# UI-only is a complete startup role, so restore it before daemon/relay state and
# return without building or launching any Rust service.
ui_load_recorded_config
if ui_only; then
  ui_enabled || { err "UI-only mode requires the UI to be enabled"; exit 2; }
  [ "${ASM_RELAY_RECONFIG:-0}" = 1 ] && {
    err "--ui-only cannot be combined with bundled relay options"
    exit 2
  }
  [ "${ASM_DAEMON_RECONFIG:-0}" = 1 ] && {
    err "--ui-only cannot be combined with daemon bind/label/registration options; use --ui-daemon URL"
    exit 2
  }
  sync_ui
  log "ready — UI-only gateway $(ui_url)  →  $(ui_daemon_url)"
  log "logs  — $LOG_DIR/asm-ui.log"
  log "next  — scripts/status.sh · scripts/stop.sh ui"
  exit 0
fi

# A normal flagless run restores bind/label/registration, bundled relay, and
# relay-only-ness rather than reverting to defaults.
daemon_load_recorded_config
relay_load_recorded_config

# Relay-only: build and start just the rendezvous relay — no holder, no daemon.
if [ "${ASM_RELAY_ONLY:-0}" = 1 ]; then
  relay_enabled || { err "--relay-only needs --relay-key KEY (the access key nodes present)"; exit 2; }
  if ui_enabled; then
    [ "${ASM_UI_RECONFIG:-0}" = 1 ] && {
      err "--relay-only cannot run a UI because there is no daemon to proxy to"
      exit 2
    }
    # Switching a previously UI-enabled full stack to relay-only implicitly
    # retires that UI; there is no API on this host for it to use.
    export ASM_RUN_UI=0 ASM_UI_RECONFIG=1
  fi
  # --no-ui, or the implicit transition above, retires an earlier managed UI.
  [ "${ASM_UI_RECONFIG:-0}" = 1 ] && sync_ui
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
sync_ui

backend="$(command -v curl >/dev/null 2>&1 && curl -s "http://$ASM_BIND/health" | sed -n 's/.*"backend":"\([^"]*\)".*/\1/p' || true)"
log "ready — http://$ASM_BIND (backend=${backend:-?})"
# The daemon serves the packaged web client itself when ASM_STATIC_DIR points at
# a build (auto-set from client/dist in _asm_common.sh) — no npm/vite needed.
if [ -n "${ASM_STATIC_DIR:-}" ] && [ -d "$ASM_STATIC_DIR" ]; then
  log "web UI — http://$ASM_BIND (served from ${ASM_STATIC_DIR#"$ROOT"/})"
fi
ui_enabled && log "dev UI — $(ui_url) → $(ui_daemon_url) (Vite; managed in the background)"
relay_enabled && log "relay  — http://$ASM_RELAY_BIND (nodes register here)"
[ -n "${ASM_RELAY_URL:-}" ] && log "node   — registering to $ASM_RELAY_URL"
log "logs   — $LOG_DIR/{asmux,asm-daemon$(ui_enabled && echo ',asm-ui')$(relay_enabled && echo ',asm-relay')}.log"
log "next   — scripts/status.sh · scripts/restart-daemon.sh · scripts/stop.sh"
