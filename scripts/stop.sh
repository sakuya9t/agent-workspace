#!/usr/bin/env bash
# Stop the daemon, the asmux holder, and any bundled relay.
#
#   scripts/stop.sh                    # stop all
#   scripts/stop.sh daemon             # only the daemon (sessions stay live in asmux)
#   scripts/stop.sh asmux              # only the holder (kills all live PTYs)
#   scripts/stop.sh relay              # only the relay (nodes/clients disconnect)
#   scripts/stop.sh --data-dir DIR ... # target a non-default install
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=_asm_common.sh
source "$HERE/_asm_common.sh"

usage() { err "usage: stop.sh [--data-dir DIR] [--runtime-dir DIR] [all|daemon|asmux|relay]"; }

asm_parse_args "$@" || { usage; exit 2; }
[ "${ASM_SHOW_HELP:-0}" = 1 ] && { usage; exit 0; }
asm_configure

what="${ASM_POSITIONAL[0]:-all}"
case "$what" in
  daemon) stop_one asm-daemon "$DAEMON_PIDFILE"; rm -f "$DAEMON_STATE_FILE" ;;
  asmux)  stop_one asmux "$ASMUX_PIDFILE" ;;
  relay)  stop_one asm-relay "$RELAY_PIDFILE" ;;
  all)
    # Daemon first (it detaches from the holder), then the holder, then the relay.
    stop_one asm-daemon "$DAEMON_PIDFILE"; rm -f "$DAEMON_STATE_FILE"
    stop_one asmux "$ASMUX_PIDFILE"
    stop_one asm-relay "$RELAY_PIDFILE"
    ;;
  *) usage; exit 2 ;;
esac
log "done."
