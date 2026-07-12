#!/usr/bin/env bash
# Show what the asm service scripts are running.
#
#   scripts/status.sh
#   scripts/status.sh --data-dir DIR --runtime-dir DIR   # a non-default install
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=_asm_common.sh
source "$HERE/_asm_common.sh"

asm_parse_args "$@" || { err "usage: status.sh [--data-dir DIR] [--runtime-dir DIR]"; exit 2; }
[ "${ASM_SHOW_HELP:-0}" = 1 ] && { err "usage: status.sh [--data-dir DIR] [--runtime-dir DIR]"; exit 0; }
asm_configure

# Report against the RECORDED launch config, not bare defaults — otherwise a
# 0.0.0.0-bound daemon shows a loopback URL and a stopped relay isn't mentioned.
daemon_load_recorded_config
relay_load_recorded_config

if pid_alive "$RELAY_PIDFILE"; then
  log "relay   RUNNING  pid=$(cat "$RELAY_PIDFILE")  $(relay_scheme)://$ASM_RELAY_BIND"
elif relay_enabled; then
  log "relay   stopped"
fi

# "RUNNING" must mean *reachable*. A pid-only check reported a healthy holder
# right through the 2026-07-12 outage, while its socket was gone and the daemon
# could not boot. ORPHANED is the state worth shouting about.
if holder_live; then
  log "asmux   RUNNING  pid=$(cat "$ASMUX_PIDFILE" 2>/dev/null || echo '?')  socket=$ASMUX_SOCK"
elif pid_alive "$ASMUX_PIDFILE"; then
  err "asmux   ORPHANED  pid=$(cat "$ASMUX_PIDFILE")  — alive, but NOT answering on $ASMUX_SOCK."
  err "                  It still holds live PTYs that nothing can attach to. See scripts/start.sh."
else
  log "asmux   stopped"
fi

if pid_alive "$DAEMON_PIDFILE"; then
  log "daemon  RUNNING  pid=$(cat "$DAEMON_PIDFILE")  $(daemon_scheme)://$ASM_BIND"
  if command -v curl >/dev/null 2>&1; then
    curl -sk "$(daemon_scheme)://$ASM_BIND/health" 2>/dev/null | sed 's/^/        /' || true
    echo
  fi
else
  log "daemon  stopped"
fi
