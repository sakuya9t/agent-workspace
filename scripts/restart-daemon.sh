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
# Options (the ASM_* env vars still work as fallbacks):
#   --bind ADDR          daemon listen address (default 127.0.0.1:4600)
#   --data-dir DIR       persistent data dir (default ~/.local/share/asm)
#   --runtime-dir DIR    sockets/pidfiles dir (default $XDG_RUNTIME_DIR/asm)
#   --label NAME         this node's label (default: hostname)
#   --release            rebuild/run the release profile
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

usage() { err "usage: restart-daemon.sh [--bind ADDR] [--data-dir DIR] [--runtime-dir DIR] [--label NAME] [--release] [--register URL --relay-key KEY]"; }

asm_parse_args "$@" || { usage; exit 2; }
[ "${ASM_SHOW_HELP:-0}" = 1 ] && { usage; exit 0; }
asm_configure

if ! pid_alive "$ASMUX_PIDFILE"; then
  err "asmux is not running — start the full stack first: scripts/start.sh"
  exit 1
fi

log "rebuilding asm-daemon ($PROFILE)..."
cargo_build -p asm-daemon

# Stop the daemon: it detaches and leaves the holder's children running.
stop_one asm-daemon "$DAEMON_PIDFILE"

# Bring it back; startup adopts the still-live holder sessions.
start_daemon
log "daemon restarted — sessions re-adopted from the holder."
