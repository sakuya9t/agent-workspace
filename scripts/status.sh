#!/usr/bin/env bash
# Show which relay, holder, daemon, and managed Vite UI processes are running.
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

# Report RUNNING components against the settings they were launched with (their
# runtime records), not the saved config — that may hold edits not applied yet,
# e.g. a new bind the daemon is not listening on. Components with no record fall
# back to the saved settings (so a configured-but-stopped relay is mentioned).
pending="$(config_pending)"
legacy_load || true

if pid_alive "$RELAY_PIDFILE"; then
  log "relay   RUNNING  pid=$(cat "$RELAY_PIDFILE")  http://$ASM_RELAY_BIND"
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
  log "daemon  RUNNING  pid=$(cat "$DAEMON_PIDFILE")  http://$ASM_BIND"
  if command -v curl >/dev/null 2>&1; then
    curl -s "http://$ASM_BIND/health" 2>/dev/null | sed 's/^/        /' || true
    echo
  fi
elif daemon_enabled; then
  log "daemon  stopped"
else
  log "daemon  off (not run on this host)"
fi

if pid_alive "$UI_PIDFILE"; then
  if ! daemon_enabled; then
    log "web UI  RUNNING  pid=$(cat "$UI_PIDFILE")  $(ui_url) → $(ui_daemon_url) (no local daemon)"
  else
    log "web UI  RUNNING  pid=$(cat "$UI_PIDFILE")  $(ui_url) → $(ui_daemon_url) (Vite)"
  fi
  if [ -n "${ASM_UI_ALLOWED_HOSTS:-}" ]; then
    log "        allowed reverse-proxy host(s): $ASM_UI_ALLOWED_HOSTS"
  fi
elif ui_enabled; then
  log "web UI  stopped  (enabled; proxy target $(ui_daemon_url); scripts/start.sh will revive it)"
elif [ -f "$UI_STATE_FILE" ]; then
  log "web UI  disabled"
fi

if [ -f "$ASM_CONFIG_FILE" ]; then
  log "config  $ASM_CONFIG_FILE"
  if [ -n "$pending" ]; then
    log "        saved changes not applied yet: $pending"
    log "        run scripts/start.sh to apply them"
  fi
fi
