#!/usr/bin/env bash
# Apply the saved startup config NOW — the web UI's "Save & apply".
#
# Runs a flagless scripts/start.sh for one install, which converges its daemon,
# relay, and UI on startup.conf: it starts, restarts, or stops only what changed
# and never touches the asmux holder, so live sessions survive. Progress goes to
# <runtime_dir>/startup.apply and output to <data_dir>/logs/apply.log, where the
# daemon (`GET /api/config`) and the Vite UI server (`GET /__asm/config`) report
# it to the Config dialog.
#
# Launched detached by those two (`POST …/apply`), because start.sh may restart
# the very process that launched it. Runnable by hand too:
#
#   scripts/apply-config.sh --data-dir DIR --runtime-dir DIR
set -uo pipefail # no -e: a failed apply is recorded, not fatal
HERE="$(cd "$(dirname "$0")" && pwd)"

data="" runtime=""
while [ $# -gt 0 ]; do
  case "$1" in
    --data-dir)    data="${2:-}"; shift 2 ;;
    --runtime-dir) runtime="${2:-}"; shift 2 ;;
    *) echo "usage: apply-config.sh --data-dir DIR --runtime-dir DIR" >&2; exit 2 ;;
  esac
done
if [ -z "$data" ] || [ -z "$runtime" ]; then
  echo "usage: apply-config.sh --data-dir DIR --runtime-dir DIR" >&2
  exit 2
fi

status="$runtime/startup.apply"
log="$data/logs/apply.log"
mkdir -p "$data/logs" "$runtime"
started="$(date +%s)"

write_status() { # write_status STATE [KEY=VALUE...] — replaced atomically
  local tmp="$status.tmp.$$"
  {
    printf 'state=%s\npid=%s\nstarted=%s\n' "$1" "$$" "$started"
    shift
    printf '%s\n' "$@"
  } > "$tmp" && mv -f "$tmp" "$status"
}

write_status running
: > "$log"

# The launcher's own settings must not leak into what this run starts: the Vite
# server's proxy target/token (start_ui sets them again for Vite itself).
unset ASM_DAEMON ASM_DAEMON_TOKEN ASM_CLIENT_HOST ASM_CLIENT_ALLOWED_HOSTS ASM_UI_MANAGED
# start.sh builds with cargo, which a daemon's inherited PATH may lack.
if [ -d "$HOME/.cargo/bin" ]; then PATH="$HOME/.cargo/bin:$PATH"; fi

"$HERE/start.sh" --data-dir "$data" --runtime-dir "$runtime" >>"$log" 2>&1 </dev/null
rc=$?
if [ "$rc" = 0 ]; then state=ok; else state=failed; fi
write_status "$state" "exit=$rc" "finished=$(date +%s)"
exit "$rc"
