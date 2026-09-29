#!/usr/bin/env bash
# Start this host's components — any of: the daemon (with its asmux session
# holder, so sessions survive a daemon restart), a bundled rendezvous relay, and
# a detached Vite web UI. By default the daemon and the UI run and the relay does
# not; each is switched on or off independently. Every managed process survives
# the SSH session that launched it.
#
# Settings come from the install's saved startup config ($ASM_DATA_DIR/
# startup.conf — also editable from the web UI's Config dialog); flags patch it
# and are saved back, so plain `scripts/start.sh` reproduces the last setup and
# applies anything changed since. The ASM_* env vars are fallbacks. See --help.
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=_asm_common.sh
source "$HERE/_asm_common.sh"

usage() {
  cat <<'USAGE'
Usage: scripts/start.sh [options]

  Components — daemon, relay, web UI; turn each on or off independently:
  --daemon / --no-daemon   the daemon + session holder (default on)
  --relay / --no-relay     a bundled rendezvous relay (default off)
  --ui / --no-ui           the managed Vite web UI (default on; --run-ui=BOOL)
  --ui-only            just the UI: turns the daemon and relay off
  --relay-only         just the relay: turns the daemon and UI off

  --data-dir DIR       persistent data dir (default ~/.local/share/asm)
  --runtime-dir DIR    sockets/pidfiles dir (default $XDG_RUNTIME_DIR/asm)
  --release / --debug  build/run the release / debug profile (default debug)
  -h, --help           show this help

  Daemon (each implies --daemon):
  --bind ADDR          listen address (default 127.0.0.1:4600)
  --label NAME         this node's label (default: hostname)
  --enrollment-token TOKEN
                       what new devices enter to enroll (≥16 chars; default: generated)

  Managed web UI (host/port imply --ui):
  --ui-host HOST       UI listen host (default 127.0.0.1)
  --ui-port PORT       UI listen port (default 5273)
  --ui-allowed-host HOST
                       exact reverse-proxy hostname Vite may serve (repeatable)
  --ui-daemon URL      off-host daemon the UI proxies to — only used without a
                       local daemon, so it implies --no-daemon
  --ui-daemon-token TOKEN
                       bearer token injected into that proxy (implies --no-daemon)
  --no-ui-daemon       no proxy target (without a daemon: a client shell)

  Saved settings: every run starts from $ASM_DATA_DIR/startup.conf (default
  ~/.local/share/asm/startup.conf), which the web UI's Config dialog edits too.
  Flags change just the settings they name and are saved back, so a flagless run
  reproduces the last setup (and applies changes saved since, starting,
  restarting, or stopping the daemon/relay/UI as needed — the holder and its
  live sessions are never touched). Saved settings beat inherited ASM_* env.
  stop.sh does not clear them.

  Relay host — a reachable box that NAT'd nodes register to:
  --relay-bind ADDR    relay listen address (default 0.0.0.0:4700; implies --relay)
  --relay-key KEY      shared relay access key

  NAT'd node — registers OUTBOUND to a relay (no relay runs here):
  --register URL       relay to register to, e.g. ws://relay-host:4700
  --relay-key KEY      the relay's access key
  --no-register        stop registering to a relay

Examples:
  scripts/start.sh                                       # the saved setup (first run: daemon + local Vite UI)
  scripts/start.sh --no-ui                               # daemon/holder only
  scripts/start.sh --ui-only                             # UI shell/gateway, no local daemon
  scripts/start.sh --ui-only --ui-daemon http://host:4600
  scripts/start.sh --ui-allowed-host host.example.ts.net
  scripts/start.sh --ui-host 0.0.0.0                     # expose UI on a trusted LAN
  scripts/start.sh --bind 0.0.0.0:4600
  scripts/start.sh --relay --relay-key s3cret               # relay host + daemon
  scripts/start.sh --relay-only --relay-key s3cret          # just the relay, no daemon
  scripts/start.sh --no-daemon --relay --relay-key s3cret   # relay + web UI, no daemon
  scripts/start.sh --register ws://192.168.122.1:4700 --relay-key s3cret
USAGE
}

asm_parse_args "$@" || { usage; exit 2; }
[ "${ASM_SHOW_HELP:-0}" = 1 ] && { usage; exit 0; }
asm_configure || exit 2
asm_validate || exit 2
log_config_origin

components=()
daemon_enabled && components+=(daemon)
relay_enabled && components+=(relay)
ui_enabled && components+=("web UI")
log "components — $(IFS=,; echo "${components[*]}" | sed 's/,/, /g')"

build_targets=()
daemon_enabled && build_targets+=(-p asm-daemon -p asmux)
relay_enabled && build_targets+=(-p asm-relay)
if [ ${#build_targets[@]} -gt 0 ]; then
  log "building$(daemon_enabled && echo ' asm-daemon + asmux')$(daemon_enabled && relay_enabled && echo ' +')$(relay_enabled && echo ' asm-relay') ($PROFILE)..."
  cargo_build "${build_targets[@]}"
fi
config_save

# Converge every component on the settings: start or restart what is on and
# changed, stop what is off. The holder is never stopped here.
sync_relay
if daemon_enabled; then
  start_asmux
  start_daemon
else
  stop_disabled_daemon
fi
sync_ui
applied_save

if daemon_enabled; then
  backend="$(command -v curl >/dev/null 2>&1 && curl -s "http://$ASM_BIND/health" | sed -n 's/.*"backend":"\([^"]*\)".*/\1/p' || true)"
  log "ready — daemon http://$ASM_BIND (backend=${backend:-?})"
  # The daemon serves the packaged web client itself when ASM_STATIC_DIR points
  # at a build (auto-set from client/dist in _asm_common.sh) — no npm/vite needed.
  if [ -n "${ASM_STATIC_DIR:-}" ] && [ -d "$ASM_STATIC_DIR" ]; then
    log "web UI — http://$ASM_BIND (served from ${ASM_STATIC_DIR#"$ROOT"/})"
  fi
  [ -n "${ASM_RELAY_URL:-}" ] && log "node   — registering to $ASM_RELAY_URL"
fi
if ui_enabled; then
  if daemon_enabled || [ -n "$(ui_proxy_daemon)" ]; then
    log "dev UI — $(ui_url) → $(ui_daemon_url) (Vite; managed in the background)"
  else
    log "dev UI — $(ui_url) (Vite; no daemon here — add daemons or relays from manage)"
  fi
fi
relay_enabled && log "relay  — http://$ASM_RELAY_BIND (nodes register here)"
logs=()
daemon_enabled && logs+=(asmux asm-daemon)
ui_enabled && logs+=(asm-ui)
relay_enabled && logs+=(asm-relay)
log "logs   — $LOG_DIR/{$(IFS=,; echo "${logs[*]}")}.log"
log "next   — scripts/status.sh$(daemon_enabled && echo ' · scripts/restart-daemon.sh') · scripts/stop.sh"
