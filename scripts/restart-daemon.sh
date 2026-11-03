#!/usr/bin/env bash
# Restart ONLY the daemon. The asmux holder keeps running, so live sessions
# survive and the new daemon re-adopts them on start. This is the durability
# story in one command (rebuild + restart the control plane, keep the sessions).
#
# It force-restarts the daemon unconditionally — even when config is unchanged —
# which is how you load a freshly-built binary. (start.sh only restarts a running
# daemon when its settings actually change; use this when you want a restart
# regardless.)
#
# Settings come from the install's saved startup config ($ASM_DATA_DIR/
# startup.conf, editable from the web UI's Config dialog); flags patch it and are
# saved back. So a FLAGLESS run restarts the daemon with the saved settings —
# including anything changed in the Config dialog since — and never reverts a
# 0.0.0.0 bind or a --register to defaults; the saved file also beats inherited
# ASM_* env (an asm session leaks the daemon's own exports). A bundled relay on
# this box is left running (its connections survive the daemon reload); if it
# died, it is rebuilt and revived. A managed Vite UI is re-converged (revived,
# or restarted to follow a changed bind/UI setting) unless you stopped it with
# stop.sh ui. Relay changes need scripts/start.sh.
#
# Options (the ASM_* env vars still work as fallbacks):
#   --bind ADDR          daemon listen address (default 127.0.0.1:4600)
#   --data-dir DIR       persistent data dir (default ~/.local/share/asm)
#   --runtime-dir DIR    sockets/pidfiles dir (default $XDG_RUNTIME_DIR/asm)
#   --label NAME         this node's label (default: hostname)
#   --enrollment-token TOKEN  what new devices enter to enroll (≥16 chars)
#   --release / --debug  rebuild/run the release / debug profile (saved)
#   --ui / --no-ui       enable (the default) or disable managed Vite
#   --ui-host HOST       UI listen host (default 127.0.0.1; implies --ui)
#   --ui-port PORT       UI listen port (default 5273; implies --ui)
#   --ui-allowed-host HOST
#                        exact reverse-proxy hostname Vite may serve (repeatable)
#
#   Register this (NAT'd) daemon OUTBOUND to a relay — no relay runs here:
#   --register URL       relay to register to, e.g. ws://relay-host:4700
#   --relay-key KEY      the relay's access key (must match the relay host's)
#   --no-register        stop registering to a relay
#
#   scripts/restart-daemon.sh
#   scripts/restart-daemon.sh --data-dir DIR   # a non-default install
#   scripts/restart-daemon.sh --register ws://192.168.0.159:4700 --relay-key meow
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=_asm_common.sh
source "$HERE/_asm_common.sh"

usage() { err "usage: restart-daemon.sh [--bind ADDR] [--data-dir DIR] [--runtime-dir DIR] [--label NAME] [--release|--debug] [--ui|--no-ui] [--ui-host HOST] [--ui-port PORT] [--ui-allowed-host HOST] [--register URL --relay-key KEY | --no-register]"; }

asm_parse_args "$@" || { usage; exit 2; }
[ "${ASM_SHOW_HELP:-0}" = 1 ] && { usage; exit 0; }
asm_configure || exit 2
asm_validate || exit 2
log_config_origin

if ! daemon_enabled; then
  err "the daemon is off in this install's settings, so there is none to restart"
  err "turn it on with --daemon (or in the web UI's Config dialog), or use scripts/start.sh"
  exit 2
fi

# Check the SOCKET, not just the pid: a holder whose socket was unlinked is alive
# but unreachable, and restarting the daemon into that state just fails the boot
# (exactly what happened on 2026-07-12). start.sh knows how to diagnose/recover.
if ! holder_live; then
  if pid_alive "$ASMUX_PIDFILE"; then
    err "asmux (pid $(cat "$ASMUX_PIDFILE")) is alive but not answering on $ASMUX_SOCK."
    err "Restarting the daemon now would just fail to boot. Run: scripts/start.sh"
  else
    err "asmux is not running — start the full stack first: scripts/start.sh"
  fi
  exit 1
fi

# The bundled relay is start.sh's to converge: a live one is left alone (a
# restart would drop every connected node), one that DIED while recorded is
# revived, and one that was stopped (no record) stays stopped.
build_targets=(-p asm-daemon)
# Windows starts the daemon (and any revived relay/UI) through asm-launch.
if [ "$ASM_WINDOWS" = 1 ]; then build_targets+=(-p asm-launch); fi
restart_relay=0
if relay_enabled && [ -f "$RELAY_STATE_FILE" ] && ! pid_alive "$RELAY_PIDFILE"; then
  restart_relay=1
  build_targets+=(-p asm-relay)
fi

log "rebuilding asm-daemon$([ "$restart_relay" = 1 ] && echo ' + asm-relay') ($PROFILE)..."
cargo_build "${build_targets[@]}"
config_save

if [ "$restart_relay" = 1 ]; then
  log "bundled relay is down — reviving it with its settings..."
  start_relay
fi

# Stop the daemon: it detaches and leaves the holder's children running.
stop_one asm-daemon "$DAEMON_PIDFILE"

# Bring it back. The daemon binds first and adopts the still-live holder sessions
# behind the listener (so its health check never races a long adopt), which means
# "up" and "sessions are back" are now two distinct moments — wait for the second
# one before claiming it.
start_daemon
# The managed UI follows the daemon (its proxy targets ASM_BIND): re-converge it
# when it is recorded as managed here or a UI flag was passed — but not after an
# explicit `stop.sh ui`, which removed its record.
ui_synced=0
if [ "${ASM_UI_RECONFIG:-0}" = 1 ] || [ -f "$UI_STATE_FILE" ]; then
  sync_ui
  ui_synced=1
fi

# Record what is now applied. Components this script left alone keep their
# previously applied settings, so the Config dialog still flags their changes.
keep=()
[ "$restart_relay" = 1 ] || keep+=("${RELAY_KEYS_GROUP[@]}")
[ "$ui_synced" = 1 ] || keep+=("${UI_KEYS_GROUP[@]}")
applied_save ${keep[@]+"${keep[@]}"}
pending="$(config_pending)"
if [ -n "$pending" ]; then
  log "note — saved settings not applied by a daemon restart: $pending"
  log "       run scripts/start.sh to apply them (it converges the relay and UI too)"
fi

if wait_reconciled; then
  log "daemon restarted — sessions re-adopted from the holder."
else
  err "daemon is up, but the holder adopt pass has not finished — see $LOG_DIR/asm-daemon.log"
  exit 1
fi
