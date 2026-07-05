#!/usr/bin/env bash
# Print this host's device-enrollment token. Reads the SAME data dir the other
# service scripts use, so it matches a daemon started with scripts/start.sh.
# (The token lives in the SQLite DB under $ASM_DATA_DIR — reading a different
# data dir just mints an unrelated identity.)
#
#   scripts/token.sh
#   scripts/token.sh --data-dir DIR    # a non-default install
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=_asm_common.sh
source "$HERE/_asm_common.sh"

asm_parse_args "$@" || { err "usage: token.sh [--data-dir DIR]"; exit 2; }
[ "${ASM_SHOW_HELP:-0}" = 1 ] && { err "usage: token.sh [--data-dir DIR]"; exit 0; }
asm_configure

[ -x "$DAEMON_BIN" ] || cargo_build -p asm-daemon 1>&2
exec "$DAEMON_BIN" token
