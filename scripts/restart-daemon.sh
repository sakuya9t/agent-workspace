#!/usr/bin/env bash
# Restart ONLY the daemon. The asmux holder keeps running, so live sessions
# survive and the new daemon re-adopts them on start. This is the durability
# story in one command (rebuild + restart the control plane, keep the sessions).
#
#   scripts/restart-daemon.sh
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=_asm_common.sh
source "$HERE/_asm_common.sh"

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
