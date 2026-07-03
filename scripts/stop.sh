#!/usr/bin/env bash
# Stop the daemon and the asmux holder.
#
#   scripts/stop.sh              # stop both
#   scripts/stop.sh daemon       # stop only the daemon (sessions stay live in asmux)
#   scripts/stop.sh asmux        # stop only the holder (kills all live PTYs)
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=_asm_common.sh
source "$HERE/_asm_common.sh"

what="${1:-all}"
case "$what" in
  daemon) stop_one asm-daemon "$DAEMON_PIDFILE" ;;
  asmux)  stop_one asmux "$ASMUX_PIDFILE" ;;
  all)
    # Daemon first (it detaches from the holder), then the holder.
    stop_one asm-daemon "$DAEMON_PIDFILE"
    stop_one asmux "$ASMUX_PIDFILE"
    ;;
  *) err "usage: stop.sh [all|daemon|asmux]"; exit 2 ;;
esac
log "done."
