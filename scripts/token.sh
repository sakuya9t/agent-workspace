#!/usr/bin/env bash
# Print this host's device-enrollment token. Reads the SAME data dir the other
# service scripts use, so it matches a daemon started with scripts/start.sh.
# (The token lives in the SQLite DB under $ASM_DATA_DIR — reading a different
# data dir just mints an unrelated identity.)
#
#   scripts/token.sh
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=_asm_common.sh
source "$HERE/_asm_common.sh"

[ -x "$DAEMON_BIN" ] || cargo_build -p asm-daemon 1>&2
exec "$DAEMON_BIN" token
