#!/usr/bin/env bash
# Stop the daemon, the asmux holder, and any bundled relay.
#
#   scripts/stop.sh              # stop all
#   scripts/stop.sh daemon       # stop only the daemon (sessions stay live in asmux)
#   scripts/stop.sh asmux        # stop only the holder (kills all live PTYs)
#   scripts/stop.sh relay        # stop only the relay (nodes/clients disconnect)
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=_asm_common.sh
source "$HERE/_asm_common.sh"

what="${1:-all}"
case "$what" in
  daemon) stop_one asm-daemon "$DAEMON_PIDFILE" ;;
  asmux)  stop_one asmux "$ASMUX_PIDFILE" ;;
  relay)  stop_one asm-relay "$RELAY_PIDFILE" ;;
  all)
    # Daemon first (it detaches from the holder), then the holder, then the relay.
    stop_one asm-daemon "$DAEMON_PIDFILE"
    stop_one asmux "$ASMUX_PIDFILE"
    stop_one asm-relay "$RELAY_PIDFILE"
    ;;
  *) err "usage: stop.sh [all|daemon|asmux|relay]"; exit 2 ;;
esac
log "done."
