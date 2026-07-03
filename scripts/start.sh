#!/usr/bin/env bash
# Start the durable stack: the asmux holder + the daemon in sidecar mode.
# Sessions survive a daemon restart (see scripts/restart-daemon.sh).
#
#   scripts/start.sh                       # debug build, 127.0.0.1:4600
#   RELEASE=1 scripts/start.sh             # release build
#   ASM_BIND=0.0.0.0:4600 scripts/start.sh # bind elsewhere
#
# Both processes run in the background (logs under $ASM_DATA_DIR/logs). The
# holder is detached so it outlives the daemon. Stop with scripts/stop.sh.
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=_asm_common.sh
source "$HERE/_asm_common.sh"

log "building asm-daemon + asmux ($PROFILE)..."
cargo_build -p asm-daemon -p asmux

start_asmux
start_daemon

backend="$(command -v curl >/dev/null 2>&1 && curl -s "http://$ASM_BIND/health" | sed -n 's/.*"backend":"\([^"]*\)".*/\1/p' || true)"
log "ready — http://$ASM_BIND (backend=${backend:-?})"
log "logs   — $LOG_DIR/{asmux,asm-daemon}.log"
log "next   — scripts/status.sh · scripts/restart-daemon.sh · scripts/stop.sh"
