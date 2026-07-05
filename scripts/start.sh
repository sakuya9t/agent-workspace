#!/usr/bin/env bash
# Start the durable stack: the asmux holder + the daemon in sidecar mode, and —
# when ASM_RELAY_KEYS is set — a bundled rendezvous relay on this host.
# Sessions survive a daemon restart (see scripts/restart-daemon.sh).
#
#   scripts/start.sh                       # debug build, 127.0.0.1:4600
#   RELEASE=1 scripts/start.sh             # release build
#   ASM_BIND=0.0.0.0:4600 scripts/start.sh # bind elsewhere
#
# Bundle a relay on this (reachable) host so NAT'd nodes can register to it:
#   ASM_RELAY_KEYS=my-secret scripts/start.sh          # + relay on 0.0.0.0:4700
#   ASM_RELAY_KEYS=k ASM_RELAY_BIND=0.0.0.0:4700 scripts/start.sh
# On a NAT'd node instead, register OUTBOUND (no relay here):
#   ASM_RELAY_URL=ws://relay-host:4700 ASM_RELAY_KEY=my-secret scripts/start.sh
#
# Processes run in the background (logs under $ASM_DATA_DIR/logs). The holder is
# detached so it outlives the daemon. Stop with scripts/stop.sh.
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=_asm_common.sh
source "$HERE/_asm_common.sh"

build_targets=(-p asm-daemon -p asmux)
relay_enabled && build_targets+=(-p asm-relay)
log "building asm-daemon + asmux$(relay_enabled && echo ' + asm-relay') ($PROFILE)..."
cargo_build "${build_targets[@]}"

start_relay
start_asmux
start_daemon

backend="$(command -v curl >/dev/null 2>&1 && curl -s "http://$ASM_BIND/health" | sed -n 's/.*"backend":"\([^"]*\)".*/\1/p' || true)"
log "ready — http://$ASM_BIND (backend=${backend:-?})"
relay_enabled && log "relay  — http://$ASM_RELAY_BIND (nodes register here)"
log "logs   — $LOG_DIR/{asmux,asm-daemon$(relay_enabled && echo ',asm-relay')}.log"
log "next   — scripts/status.sh · scripts/restart-daemon.sh · scripts/stop.sh"
