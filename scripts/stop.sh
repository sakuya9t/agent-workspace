#!/usr/bin/env bash
# Stop the daemon, the asmux holder, the managed Vite UI, and any bundled relay.
#
#   scripts/stop.sh                    # stop all
#   scripts/stop.sh daemon             # only the daemon (sessions stay live in asmux)
#   scripts/stop.sh asmux              # only the holder (kills all live PTYs)
#   scripts/stop.sh ui                 # only the managed Vite UI
#   scripts/stop.sh relay              # only the relay (nodes/clients disconnect)
#   scripts/stop.sh --data-dir DIR ... # target a non-default install
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=_asm_common.sh
source "$HERE/_asm_common.sh"

usage() { err "usage: stop.sh [--data-dir DIR] [--runtime-dir DIR] [all|daemon|asmux|ui|relay]"; }

asm_parse_args "$@" || { usage; exit 2; }
[ "${ASM_SHOW_HELP:-0}" = 1 ] && { usage; exit 0; }
asm_configure

what="${ASM_POSITIONAL[0]:-all}"
# An explicit stop also clears the component's recorded launch config (.reg) —
# a later flagless start.sh/restart-daemon.sh must not resurrect it.
case "$what" in
  daemon) stop_one asm-daemon "$DAEMON_PIDFILE"; rm -f "$DAEMON_STATE_FILE" ;;
  asmux)  stop_one asmux "$ASMUX_PIDFILE" ;;
  ui)     stop_one asm-ui "$UI_PIDFILE"; rm -f "$UI_STATE_FILE" ;;
  relay)  stop_one asm-relay "$RELAY_PIDFILE"; rm -f "$RELAY_STATE_FILE" ;;
  all)
    # UI first, then daemon/holder, then relay.
    stop_one asm-ui "$UI_PIDFILE"; rm -f "$UI_STATE_FILE"
    stop_one asm-daemon "$DAEMON_PIDFILE"; rm -f "$DAEMON_STATE_FILE"
    stop_one asmux "$ASMUX_PIDFILE"
    stop_one asm-relay "$RELAY_PIDFILE"; rm -f "$RELAY_STATE_FILE"
    ;;
  *) usage; exit 2 ;;
esac
log "done."
