#!/usr/bin/env bash
# Show what the asm service scripts are running.
#
#   scripts/status.sh
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=_asm_common.sh
source "$HERE/_asm_common.sh"

if pid_alive "$ASMUX_PIDFILE"; then
  log "asmux   RUNNING  pid=$(cat "$ASMUX_PIDFILE")  socket=$ASMUX_SOCK"
else
  log "asmux   stopped"
fi

if pid_alive "$DAEMON_PIDFILE"; then
  log "daemon  RUNNING  pid=$(cat "$DAEMON_PIDFILE")  http://$ASM_BIND"
  if command -v curl >/dev/null 2>&1; then
    curl -s "http://$ASM_BIND/health" 2>/dev/null | sed 's/^/        /' || true
    echo
  fi
else
  log "daemon  stopped"
fi
