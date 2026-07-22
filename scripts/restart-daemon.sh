#!/usr/bin/env bash
# Restart ONLY the daemon. The asmux holder keeps running, so live sessions
# survive and the new daemon re-adopts them on start. This is the durability
# story in one command (rebuild + restart the control plane, keep the sessions).
#
# It force-restarts the daemon unconditionally — even when config is unchanged —
# which is how you load a freshly-built binary. (start.sh only restarts a running
# daemon when its bind/label/relay flags actually change; use this when you want a
# restart regardless.) Passing --register/--relay-key here re-applies the relay
# registration: the old daemon is stopped and a fresh one starts with the flags.
#
# A FLAGLESS run keeps the daemon's recorded launch config (bind/label/relay
# registration) — it does not revert a 0.0.0.0 bind or a --register to defaults,
# and the recording also beats inherited ASM_* env (an asm session leaks the
# daemon's own exports). Passing any daemon flag re-specifies that config as a
# whole instead. A bundled relay recorded on this box is left running (its
# connections survive the daemon reload); if it died, it is rebuilt and revived
# with its recorded settings. A recorded managed Vite UI also stays running (or
# is revived if it died), and follows the daemon if its bind address changes.
#
# Options (the ASM_* env vars still work as fallbacks):
#   --bind ADDR          daemon listen address (default 127.0.0.1:4600)
#   --data-dir DIR       persistent data dir (default ~/.local/share/asm)
#   --runtime-dir DIR    sockets/pidfiles dir (default $XDG_RUNTIME_DIR/asm)
#   --label NAME         this node's label (default: hostname)
#   --release            rebuild/run the release profile
#   --ui / --no-ui       enable (the default) or disable managed Vite
#   --ui-host HOST       UI listen host (default 127.0.0.1; implies --ui)
#   --ui-port PORT       UI listen port (default 5273; implies --ui)
#
#   Register this (NAT'd) daemon OUTBOUND to a relay — no relay runs here:
#   --register URL       relay to register to, e.g. ws://relay-host:4700
#   --relay-key KEY      the relay's access key (must match the relay host's)
#
#   scripts/restart-daemon.sh
#   scripts/restart-daemon.sh --data-dir DIR   # a non-default install
#   scripts/restart-daemon.sh --register ws://192.168.0.159:4700 --relay-key meow
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=_asm_common.sh
source "$HERE/_asm_common.sh"

usage() { err "usage: restart-daemon.sh [--bind ADDR] [--data-dir DIR] [--runtime-dir DIR] [--label NAME] [--release] [--ui|--no-ui] [--ui-host HOST] [--ui-port PORT] [--register URL --relay-key KEY]"; }

asm_parse_args "$@" || { usage; exit 2; }
[ "${ASM_SHOW_HELP:-0}" = 1 ] && { usage; exit 0; }
asm_configure

# A flagless restart keeps what's running: re-load the daemon config recorded at
# its launch, plus any bundled relay recorded on this box (see _asm_common.sh).
ui_load_recorded_config
if ui_only; then
  err "this installation is in UI-only mode, so there is no daemon to restart"
  err "use scripts/start.sh to start/revive the UI gateway"
  exit 2
fi
daemon_load_recorded_config
relay_load_recorded_config

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

# Re-assert a recorded bundled relay, but only if it DIED — restarting a live
# relay would drop every connected node and client for no reason.
build_targets=(-p asm-daemon)
restart_relay=0
if relay_enabled && ! pid_alive "$RELAY_PIDFILE"; then
  restart_relay=1
  build_targets+=(-p asm-relay)
fi

log "rebuilding asm-daemon$([ "$restart_relay" = 1 ] && echo ' + asm-relay') ($PROFILE)..."
cargo_build "${build_targets[@]}"

if [ "$restart_relay" = 1 ]; then
  log "bundled relay is down — reviving it with its recorded settings..."
  start_relay
fi

# Stop the daemon: it detaches and leaves the holder's children running.
stop_one asm-daemon "$DAEMON_PIDFILE"

# Bring it back. The daemon binds first and adopts the still-live holder sessions
# behind the listener (so its health check never races a long adopt), which means
# "up" and "sessions are back" are now two distinct moments — wait for the second
# one before claiming it.
start_daemon
sync_ui
if wait_reconciled; then
  log "daemon restarted — sessions re-adopted from the holder."
else
  err "daemon is up, but the holder adopt pass has not finished — see $LOG_DIR/asm-daemon.log"
  exit 1
fi
