#!/usr/bin/env bash
# Shared config + helpers for the asm service scripts. SOURCED, not run.
#
# Settings come from the install's saved startup config ($ASM_DATA_DIR/
# startup.conf, see "saved startup config" below), patched by command-line
# flags (parsed by asm_parse_args), with the same `ASM_*` environment variables
# kept as fallbacks. A sourcing script does:
#
#     source _asm_common.sh
#     asm_parse_args "$@" || { usage; exit 2; }
#     asm_configure
#     asm_validate        # only scripts that launch something
#
# Both the daemon and the asmux holder get the SAME data/runtime dir so they
# agree on the socket path (<runtime_dir>/asmux.sock) and data lives in one place
# across restarts.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

log() { printf '\033[1;36m[asm]\033[0m %s\n' "$*"; }
err() { printf '\033[1;31m[asm]\033[0m %s\n' "$*" >&2; }

_need() { # _need FLAG VALUE — fail if the flag was given no value
  [ -n "${2:-}" ] && return 0
  err "option $1 requires a value"
  return 2
}

_bool() { # _bool FLAG VALUE — print 1/0 for a supported boolean spelling
  case "${2:-}" in
    1|true|yes|on)  printf '1' ;;
    0|false|no|off) printf '0' ;;
    *) err "option $1 expects true or false"; return 2 ;;
  esac
}

_port() { # _port FLAG VALUE — validate a TCP port before doing an expensive build
  if [[ "${2:-}" =~ ^[0-9]+$ ]] && [ "$2" -ge 1 ] && [ "$2" -le 65535 ]; then
    return 0
  fi
  err "option $1 expects a port between 1 and 65535"
  return 2
}

_ui_allowed_host() { # _ui_allowed_host FLAG VALUE — exact DNS hostname, no URL/port
  if [[ "${2:-}" =~ ^[A-Za-z0-9]([A-Za-z0-9.-]*[A-Za-z0-9])?$ ]]; then
    return 0
  fi
  err "option $1 expects an exact hostname without a scheme, port, path, or wildcard"
  return 2
}

# Add a comma-separated set of exact Vite hostnames without duplicating an
# entry. Each item has already passed _ui_allowed_host.
append_ui_allowed_hosts() {
  local csv="${1:-}" item
  local items=()
  [ -n "$csv" ] || return 0
  IFS=',' read -r -a items <<<"$csv"
  for item in "${items[@]}"; do
    _ui_allowed_host ASM_UI_ALLOWED_HOSTS "$item" || return 2
    case ",${ASM_UI_ALLOWED_HOSTS:-}," in
      *",$item,"*) : ;;
      *) export ASM_UI_ALLOWED_HOSTS="${ASM_UI_ALLOWED_HOSTS:+$ASM_UI_ALLOWED_HOSTS,}$item" ;;
    esac
  done
}

# ── saved startup config ───────────────────────────────────────────────────
# Every setting a start can take lives in ONE persistent file per install,
# $ASM_DATA_DIR/startup.conf, so a flagless start/restart reproduces the last
# configuration — across `stop.sh`, reboots, and SSH sessions — instead of
# reverting to defaults. Flags patch it key by key and are saved back; the web
# UI's Config dialog edits the same file (daemon `PUT /api/config`, and the Vite
# server's `/__asm/config` for the UI host). Changes apply on the next run.
#
# Precedence: flags > saved file > environment > defaults. The file deliberately
# BEATS the environment: a shell inside an asm session inherits the daemon's own
# resolved ASM_* exports (ASM_BIND & co.), so env values cannot be read as user
# intent. Env is only a fallback for a key the file lacks (e.g. the first start).
#
# Format: one `ASM_*=value` line per key, `#` comments, no quoting — the keys ARE
# the env vars below. It is parsed line by line, never `source`d. The same rules
# are implemented in crates/daemon/src/startup_config.rs and
# client/src/startupConfig.ts; keep the three in step.
#
# Three independent components, each switched by its ASM_RUN_* key; any
# non-empty subset runs: the daemon (with its asmux session holder), a bundled
# relay, and the managed Vite UI. The only dependencies are between SETTINGS:
# relay registration / downstreams / the enrollment token belong to the daemon,
# a relay needs an access key, and the UI proxies to the local daemon when that
# runs, else to the optional ASM_UI_DAEMON.
ASM_CONFIG_KEYS=(
  ASM_PROFILE
  ASM_RUN_DAEMON ASM_BIND ASM_NODE_LABEL ASM_ENROLLMENT_TOKEN ASM_RELAY_URL ASM_RELAY_KEY ASM_RELAY_DOWNSTREAMS
  ASM_RUN_RELAY ASM_RELAY_BIND ASM_RELAY_KEYS
  ASM_RUN_UI ASM_UI_HOST ASM_UI_PORT ASM_UI_ALLOWED_HOSTS ASM_UI_DAEMON ASM_UI_DAEMON_TOKEN
)

config_is_key() {
  local k
  for k in "${ASM_CONFIG_KEYS[@]}"; do [ "$k" = "$1" ] && return 0; done
  return 1
}

_trim() { # _trim VALUE — strip leading/trailing whitespace
  local v="$1"
  v="${v#"${v%%[![:space:]]*}"}"
  v="${v%"${v##*[![:space:]]}"}"
  printf '%s' "$v"
}

# config_read FILE PREFIX — set ${PREFIX}${KEY} for each valid line of FILE.
# Returns 1 if FILE does not exist. Unknown keys / malformed lines are warned
# about and skipped, never executed.
config_read() {
  local f="$1" prefix="$2" line key value n=0
  [ -f "$f" ] || return 1
  while IFS= read -r line || [ -n "$line" ]; do
    n=$((n + 1))
    line="${line%$'\r'}"
    case "$(_trim "$line")" in ''|'#'*) continue ;; esac
    if [ "${line#*=}" = "$line" ]; then
      err "$f:$n: ignoring a line without KEY=value"
      continue
    fi
    key="$(_trim "${line%%=*}")"
    value="$(_trim "${line#*=}")"
    if config_is_key "$key"; then
      printf -v "$prefix$key" '%s' "$value"
    else
      err "$f:$n: ignoring unknown setting $key"
    fi
  done < "$f"
  return 0
}

# Load the saved config into the environment (overriding inherited env).
config_load() {
  local key var
  for key in "${ASM_CONFIG_KEYS[@]}"; do unset "_asm_cfg_$key"; done
  config_read "$ASM_CONFIG_FILE" _asm_cfg_ || return 1
  for key in "${ASM_CONFIG_KEYS[@]}"; do
    var="_asm_cfg_$key"
    if [ -n "${!var+set}" ]; then export "$key=${!var}"; fi
  done
  return 0
}

config_render() {
  local key
  printf '%s\n' \
    "# Agent Session Manager — saved startup settings for this install." \
    "# scripts/start.sh and scripts/restart-daemon.sh start from these on every run;" \
    "# command-line flags override them and are saved back. Also editable from the" \
    "# web UI (Config). One KEY=value per line, no quotes; an empty value means the" \
    "# default."
  for key in "${ASM_CONFIG_KEYS[@]}"; do printf '%s=%s\n' "$key" "${!key:-}"; done
}

# config_write FILE [KEEP_KEY...] — write the current settings atomically,
# owner-only (it holds relay keys / a bearer token). KEEP_KEYs take their value
# from FILE's previous contents instead (for a partial apply, see applied_save).
config_write() {
  local f="$1" tmp key var
  shift
  tmp="$f.tmp.$$"
  mkdir -p "$(dirname "$f")"
  (
    if [ $# -gt 0 ]; then
      for key in "${ASM_CONFIG_KEYS[@]}"; do unset "_asm_prev_$key"; done
      if config_read "$f" _asm_prev_ 2>/dev/null; then
        for key in "$@"; do
          var="_asm_prev_$key"
          if [ -n "${!var+set}" ]; then export "$key=${!var}"; fi
        done
      fi
    fi
    umask 077
    config_render > "$tmp"
    chmod 600 "$tmp"
  )
  mv -f "$tmp" "$f"
}

# Persist the resolved settings (the saved config + this command line's flags).
config_save() { config_write "$ASM_CONFIG_FILE"; }

# Record what the running stack was just started with, so the Config dialog
# and status.sh can say which saved changes are not applied yet. A script that
# leaves some component alone passes that component's keys to keep.
applied_save() { config_write "$ASM_APPLIED_FILE" "$@"; }

# Keys whose saved value differs from the applied snapshot (space-separated);
# empty when they match or either file is missing.
config_pending() {
  local key a b out=""
  for key in "${ASM_CONFIG_KEYS[@]}"; do unset "_asm_s_$key" "_asm_a_$key"; done
  config_read "$ASM_CONFIG_FILE" _asm_s_ 2>/dev/null || return 0
  config_read "$ASM_APPLIED_FILE" _asm_a_ 2>/dev/null || return 0
  for key in "${ASM_CONFIG_KEYS[@]}"; do
    a="_asm_s_$key"; b="_asm_a_$key"
    [ "${!a:-}" = "${!b:-}" ] || out="${out:+$out }$key"
  done
  printf '%s' "$out"
}

RELAY_KEYS_GROUP=(ASM_RUN_RELAY ASM_RELAY_BIND ASM_RELAY_KEYS)
UI_KEYS_GROUP=(ASM_RUN_UI ASM_UI_HOST ASM_UI_PORT ASM_UI_ALLOWED_HOSTS ASM_UI_DAEMON ASM_UI_DAEMON_TOKEN)

# cli_set KEY VALUE — queue a flag's override; applied after the saved config.
cli_set() { ASM_CLI_SET+=("$1=$2"); }

# Parse flags into queued overrides (see cli_set) — nothing is exported yet,
# because the saved config they patch can only be located once --data-dir is
# known. Collects any non-flag args into ASM_POSITIONAL and sets ASM_SHOW_HELP.
# Returns non-zero on a bad flag.
asm_parse_args() {
  ASM_POSITIONAL=()
  ASM_SHOW_HELP=0
  ASM_CLI_SET=()
  ASM_CLI_PROFILE=""
  # What this command line asks of each component: needs it on / turns it off.
  local key="" register=0 want_relay=0 daemon_on=0 daemon_off=0 relay_off=0 ui_on=0 ui_off=0
  local only="" ui_value ui_allowed_hosts_add=""
  while [ $# -gt 0 ]; do
    case "$1" in
      --daemon)       cli_set ASM_RUN_DAEMON 1; daemon_on=1; shift ;;
      --no-daemon)    cli_set ASM_RUN_DAEMON 0; daemon_off=1; shift ;;
      --bind)         _need "$1" "${2:-}" || return 2; cli_set ASM_BIND "$2"; daemon_on=1; shift 2 ;;
      --data-dir)     _need "$1" "${2:-}" || return 2; export ASM_DATA_DIR="$2"; shift 2 ;;
      --runtime-dir)  _need "$1" "${2:-}" || return 2; export ASM_RUNTIME_DIR="$2"; shift 2 ;;
      --label)        _need "$1" "${2:-}" || return 2; cli_set ASM_NODE_LABEL "$2"; daemon_on=1; shift 2 ;;
      --enrollment-token) _need "$1" "${2:-}" || return 2; cli_set ASM_ENROLLMENT_TOKEN "$2"; daemon_on=1; shift 2 ;;
      --release)      ASM_CLI_PROFILE=release; shift ;;
      --debug)        ASM_CLI_PROFILE=debug; shift ;;
      --relay)        cli_set ASM_RUN_RELAY 1; want_relay=1; shift ;;
      --no-relay)     cli_set ASM_RUN_RELAY 0; relay_off=1; shift ;;
      --relay-only)
        cli_set ASM_RUN_RELAY 1; cli_set ASM_RUN_DAEMON 0; cli_set ASM_RUN_UI 0
        want_relay=1; daemon_off=1; ui_off=1; only="${only:+$only }--relay-only"; shift ;;
      --relay-bind)   _need "$1" "${2:-}" || return 2; cli_set ASM_RELAY_BIND "$2"; cli_set ASM_RUN_RELAY 1; want_relay=1; shift 2 ;;
      --relay-key)    _need "$1" "${2:-}" || return 2; key="$2"; shift 2 ;;
      --register)     _need "$1" "${2:-}" || return 2; cli_set ASM_RELAY_URL "$2"; daemon_on=1; register=1; shift 2 ;;
      --no-register)  cli_set ASM_RELAY_URL ""; cli_set ASM_RELAY_KEY ""; shift ;;
      --ui|--run-ui)  cli_set ASM_RUN_UI 1; ui_on=1; shift ;;
      --no-ui)        cli_set ASM_RUN_UI 0; ui_off=1; shift ;;
      --run-ui=*)
        ui_value="$(_bool --run-ui "${1#*=}")" || return 2
        cli_set ASM_RUN_UI "$ui_value"
        if [ "$ui_value" = 1 ]; then ui_on=1; else ui_off=1; fi
        shift ;;
      --ui-only)
        cli_set ASM_RUN_UI 1; cli_set ASM_RUN_DAEMON 0; cli_set ASM_RUN_RELAY 0
        ui_on=1; daemon_off=1; relay_off=1; only="${only:+$only }--ui-only"; shift ;;
      # The UI's off-host proxy target is only used without a local daemon, so
      # naming one turns the local daemon off.
      --ui-daemon)    _need "$1" "${2:-}" || return 2; cli_set ASM_UI_DAEMON "$2"; cli_set ASM_RUN_UI 1; cli_set ASM_RUN_DAEMON 0; ui_on=1; daemon_off=1; shift 2 ;;
      --ui-daemon-token) _need "$1" "${2:-}" || return 2; cli_set ASM_UI_DAEMON_TOKEN "$2"; cli_set ASM_RUN_UI 1; cli_set ASM_RUN_DAEMON 0; ui_on=1; daemon_off=1; shift 2 ;;
      --no-ui-daemon) cli_set ASM_UI_DAEMON ""; cli_set ASM_UI_DAEMON_TOKEN ""; shift ;;
      --ui-host)      _need "$1" "${2:-}" || return 2; cli_set ASM_UI_HOST "$2"; cli_set ASM_RUN_UI 1; ui_on=1; shift 2 ;;
      --ui-port)      _need "$1" "${2:-}" || return 2; _port "$1" "$2" || return 2; cli_set ASM_UI_PORT "$2"; cli_set ASM_RUN_UI 1; ui_on=1; shift 2 ;;
      --ui-allowed-host)
        _need "$1" "${2:-}" || return 2
        _ui_allowed_host "$1" "$2" || return 2
        case ",$ui_allowed_hosts_add," in
          *",$2,"*) : ;;
          *) ui_allowed_hosts_add="${ui_allowed_hosts_add:+$ui_allowed_hosts_add,}$2" ;;
        esac
        shift 2
        ;;
      -h|--help)      ASM_SHOW_HELP=1; shift ;;
      --)             shift; while [ $# -gt 0 ]; do ASM_POSITIONAL+=("$1"); shift; done ;;
      --*)            err "unknown option: $1"; return 2 ;;
      *)              ASM_POSITIONAL+=("$1"); shift ;;
    esac
  done
  # A command line that both needs a component and turns it off is a mistake.
  if [ "$daemon_on" = 1 ] && [ "$daemon_off" = 1 ]; then
    err "daemon options (--daemon/--bind/--label/--register/--enrollment-token) conflict with ${only:---no-daemon/--ui-daemon}"
    return 2
  fi
  if [ "$want_relay" = 1 ] && [ "$relay_off" = 1 ]; then
    err "relay options (--relay/--relay-bind) conflict with ${only:---no-relay}"; return 2
  fi
  if [ "$ui_on" = 1 ] && [ "$ui_off" = 1 ]; then
    err "UI options (--ui/--ui-host/--ui-port/--ui-daemon) conflict with ${only:---no-ui}"; return 2
  fi
  # Configuring the daemon means running it.
  if [ "$daemon_on" = 1 ]; then cli_set ASM_RUN_DAEMON 1; fi
  # Which components THIS command line touched — describes the invocation, never
  # persisted config or an env fallback, so it is always set fresh.
  export ASM_DAEMON_RECONFIG=$((daemon_on | daemon_off))
  export ASM_RELAY_RECONFIG=$((want_relay | relay_off))
  export ASM_UI_RECONFIG=$((ui_on | ui_off))
  # --relay-key feeds whichever role it goes with; resolved in asm_configure
  # once the saved config says which components run.
  ASM_CLI_RELAY_KEY="$key"
  ASM_CLI_WANT_RELAY="$want_relay"
  ASM_CLI_REGISTER="$register"
  # Unlike the other UI flags, --ui-allowed-host is an additive patch.
  export ASM_UI_ALLOWED_HOSTS_ADD="$ui_allowed_hosts_add"
  return 0
}

# Before the first save, recover the settings of an already-running stack from
# the per-boot runtime records (.reg) older versions of these scripts kept, so
# upgrading does not revert a live 0.0.0.0 bind or a relay registration.
legacy_load() {
  local found=1 rec enabled host port daemon_bind only daemon token allowed_hosts bind label url key keys extra_rest
  rec="$(cat "$UI_STATE_FILE" 2>/dev/null || true)"
  if [ -n "$rec" ]; then
    found=0
    IFS='|' read -r enabled host port daemon_bind only daemon token allowed_hosts <<<"$rec" || true
    export ASM_RUN_UI="${enabled:-0}"
    if [ -n "$host" ]; then export ASM_UI_HOST="$host"; fi
    if [ -n "$port" ]; then export ASM_UI_PORT="$port"; fi
    if [ -n "$daemon_bind" ]; then export ASM_BIND="$daemon_bind"; fi
    # An old UI-only gateway ran no daemon (and no relay of its own).
    if [ "${only:-0}" = 1 ]; then export ASM_RUN_DAEMON=0; fi
    export ASM_UI_DAEMON="${daemon:-}" ASM_UI_DAEMON_TOKEN="${token:-}"
    export ASM_UI_ALLOWED_HOSTS="${allowed_hosts:-}"
  fi
  rec="$(cat "$DAEMON_STATE_FILE" 2>/dev/null || true)"
  if [ -n "$rec" ]; then
    found=0
    local extra=() field
    IFS='|' read -r bind label url key extra_rest <<<"$rec" || true
    if [ -n "$bind" ]; then export ASM_BIND="$bind"; fi
    export ASM_NODE_LABEL="${label:-}" ASM_RELAY_URL="${url:-}" ASM_RELAY_KEY="${key:-}"
    # Optional trailing fields (see daemon_reg_signature).
    IFS='|' read -r -a extra <<<"${extra_rest:-}" || true
    for field in ${extra[@]+"${extra[@]}"}; do
      case "$field" in
        downstreams=*) export ASM_RELAY_DOWNSTREAMS="${field#downstreams=}" ;;
        release)       export ASM_PROFILE=release ;;
      esac
    done
  fi
  rec="$(cat "$RELAY_STATE_FILE" 2>/dev/null || true)"
  if [ -n "$rec" ]; then
    IFS='|' read -r bind keys only <<<"$rec" || true
    if [ -n "$keys" ]; then
      found=0
      export ASM_RUN_RELAY=1 ASM_RELAY_KEYS="$keys"
      # An old relay-only box ran no daemon.
      if [ "${only:-0}" = 1 ]; then export ASM_RUN_DAEMON=0; fi
      if [ -n "$bind" ]; then export ASM_RELAY_BIND="$bind"; fi
    fi
  fi
  return "$found"
}

# Normalize a boolean setting to 1/0 (default when empty). An unrecognized
# spelling is left as is for asm_validate to report.
_normbool() {
  local var="$1" v
  v="${!var:-$2}"
  case "$v" in
    1|true|yes|on)  export "$var=1" ;;
    0|false|no|off) export "$var=0" ;;
    *)              export "$var=$v" ;;
  esac
}

# Resolve dirs, then the settings: saved config (or legacy records) → this
# command line's overrides → env fallbacks → defaults. Call after asm_parse_args.
# Lenient — stop/status must work even with a broken file; the launching
# scripts call asm_validate afterwards.
asm_configure() {
  export ASM_DATA_DIR="${ASM_DATA_DIR:-$HOME/.local/share/asm}"
  export ASM_RUNTIME_DIR="${ASM_RUNTIME_DIR:-${XDG_RUNTIME_DIR:-/tmp}/asm}"

  LOG_DIR="$ASM_DATA_DIR/logs"
  ASMUX_SOCK="$ASM_RUNTIME_DIR/asmux.sock"
  ASMUX_PIDFILE="$ASM_RUNTIME_DIR/asmux.pid"
  DAEMON_PIDFILE="$ASM_RUNTIME_DIR/asm-daemon.pid"
  # Runtime records: the config signature each RUNNING component was launched
  # with, so a re-run can tell whether the settings actually changed (and must
  # restart it to apply them). Per boot, cleared by stop.sh — the persistent
  # source of settings is ASM_CONFIG_FILE.
  DAEMON_STATE_FILE="$ASM_RUNTIME_DIR/asm-daemon.reg"   # bind|label|register-url|register-key[|downstreams=…][|enroll=…][|release]
  RELAY_PIDFILE="$ASM_RUNTIME_DIR/asm-relay.pid"
  RELAY_STATE_FILE="$ASM_RUNTIME_DIR/asm-relay.reg"     # bind|keys|no-local-daemon
  UI_PIDFILE="$ASM_RUNTIME_DIR/asm-ui.pid"
  # enabled|host|port|daemon-bind|no-local-daemon|proxy-url|proxy-token|allowed-hosts.
  # Recorded independently because Vite can stay up while the daemon is rebuilt.
  UI_STATE_FILE="$ASM_RUNTIME_DIR/asm-ui.reg"
  UI_BIN="$ROOT/client/node_modules/.bin/vite"
  ASM_CONFIG_FILE="$ASM_DATA_DIR/startup.conf"
  ASM_APPLIED_FILE="$ASM_RUNTIME_DIR/startup.applied"

  mkdir -p "$ASM_DATA_DIR" "$ASM_RUNTIME_DIR" "$LOG_DIR"

  if config_load; then
    ASM_CONFIG_ORIGIN=saved
  elif legacy_load; then
    ASM_CONFIG_ORIGIN=legacy
  else
    ASM_CONFIG_ORIGIN=none
  fi
  local entry
  for entry in ${ASM_CLI_SET[@]+"${ASM_CLI_SET[@]}"}; do export "$entry"; done

  if [ -n "${ASM_CLI_PROFILE:-}" ]; then
    PROFILE="$ASM_CLI_PROFILE"
  elif [ "${RELEASE:-0}" = "1" ]; then
    PROFILE=release
  else
    PROFILE="${ASM_PROFILE:-${PROFILE:-debug}}"
  fi
  export ASM_PROFILE="$PROFILE"
  BIN_DIR="$ROOT/target/$PROFILE"
  DAEMON_BIN="$BIN_DIR/asm-daemon"
  ASMUX_BIN="$BIN_DIR/asmux"
  RELAY_BIN="$BIN_DIR/asm-relay"

  export ASM_BIND="${ASM_BIND:-127.0.0.1:4600}"
  export ASM_NODE_LABEL="${ASM_NODE_LABEL:-}"
  export ASM_RELAY_URL="${ASM_RELAY_URL:-}"
  export ASM_RELAY_KEY="${ASM_RELAY_KEY:-}"
  export ASM_RELAY_DOWNSTREAMS="${ASM_RELAY_DOWNSTREAMS:-}"
  # Relay default bind is 0.0.0.0 so both LAN clients and nodes dialing out reach it.
  export ASM_RELAY_BIND="${ASM_RELAY_BIND:-0.0.0.0:4700}"
  export ASM_RELAY_KEYS="${ASM_RELAY_KEYS:-}"
  export ASM_ENROLLMENT_TOKEN="${ASM_ENROLLMENT_TOKEN:-}"
  # Older env spellings of the component switches, honoured only as a fallback
  # (nothing saved yet): ASM_UI_ONLY=1 was "just the UI", ASM_RELAY_ONLY=1 "just
  # the relay"; before ASM_RUN_RELAY existed a relay key alone enabled a relay.
  if [ -z "${ASM_RUN_DAEMON:-}" ]; then
    case "${ASM_UI_ONLY:-0}" in
      1|true|yes|on) export ASM_RUN_DAEMON=0; export ASM_RUN_RELAY="${ASM_RUN_RELAY:-0}" ;;
    esac
    case "${ASM_RELAY_ONLY:-0}" in
      1|true|yes|on) export ASM_RUN_DAEMON=0 ASM_RUN_RELAY=1; export ASM_RUN_UI="${ASM_RUN_UI:-0}" ;;
    esac
  fi
  unset ASM_UI_ONLY ASM_RELAY_ONLY
  if [ -z "${ASM_RUN_RELAY:-}" ]; then
    if [ -n "$ASM_RELAY_KEYS" ]; then export ASM_RUN_RELAY=1; else export ASM_RUN_RELAY=0; fi
  fi
  _normbool ASM_RUN_DAEMON 1
  _normbool ASM_RUN_RELAY 0
  _normbool ASM_RUN_UI 1
  export ASM_UI_HOST="${ASM_UI_HOST:-${ASM_CLIENT_HOST:-127.0.0.1}}"
  export ASM_UI_PORT="${ASM_UI_PORT:-5273}"
  export ASM_UI_DAEMON="${ASM_UI_DAEMON:-}"
  export ASM_UI_DAEMON_TOKEN="${ASM_UI_DAEMON_TOKEN:-}"
  export ASM_UI_ALLOWED_HOSTS="${ASM_UI_ALLOWED_HOSTS:-}"
  # Validate both the saved/env value and any CLI additions before either can
  # reach Vite or the pipe-delimited runtime record.
  # A bad saved value is reported (and refused) by asm_validate rather than
  # here, so stop.sh / status.sh still work with a broken file.
  local configured_ui_allowed_hosts="$ASM_UI_ALLOWED_HOSTS"
  export ASM_UI_ALLOWED_HOSTS=""
  ASM_ALLOWED_HOSTS_INVALID=0
  append_ui_allowed_hosts "$configured_ui_allowed_hosts" || ASM_ALLOWED_HOSTS_INVALID=1
  append_ui_allowed_hosts "${ASM_UI_ALLOWED_HOSTS_ADD:-}" || ASM_ALLOWED_HOSTS_INVALID=1

  # One shared secret feeds whichever role(s) it belongs to: a relay host
  # accepts it (ASM_RELAY_KEYS); a registering node presents it (ASM_RELAY_KEY).
  # With neither flag on this command line, it goes to the saved active role(s).
  if [ -n "${ASM_CLI_RELAY_KEY:-}" ]; then
    local applied=0
    if [ "${ASM_CLI_WANT_RELAY:-0}" = 1 ] || { [ "${ASM_CLI_REGISTER:-0}" = 0 ] && relay_enabled; }; then
      export ASM_RELAY_KEYS="$ASM_CLI_RELAY_KEY"; applied=1
    fi
    if [ "${ASM_CLI_REGISTER:-0}" = 1 ] || { [ "${ASM_CLI_WANT_RELAY:-0}" = 0 ] && [ -n "$ASM_RELAY_URL" ]; }; then
      export ASM_RELAY_KEY="$ASM_CLI_RELAY_KEY"; applied=1
    fi
    if [ "$applied" = 0 ]; then
      err "--relay-key needs --relay, --relay-only, or --register (nothing saved uses a relay key)"
      return 2
    fi
  fi

  # Packaged web client: if a build exists (client/dist), serve it straight from
  # the daemon so a box without npm/vite still gets a browser UI at ASM_BIND —
  # no need to pass ASM_STATIC_DIR by hand. An explicit ASM_STATIC_DIR always
  # wins (set it empty, ASM_STATIC_DIR=, to disable). Only a daemon launched
  # fresh below reads this; if you build client/dist while the daemon is already
  # up, run scripts/restart-daemon.sh to pick it up.
  if [ -z "${ASM_STATIC_DIR+set}" ] && [ -f "$ROOT/client/dist/index.html" ]; then
    export ASM_STATIC_DIR="$ROOT/client/dist"
  fi
}

# _bad KEY REASON — report an invalid resolved setting.
_bad() {
  err "invalid startup setting $1=${!1:-}: $2"
  err "  fix it with a flag (see --help), in the web UI's Config dialog, or in $ASM_CONFIG_FILE"
  return 2
}

_is_addr_port() { # IP:port (the daemon/relay parse the IP themselves)
  local v="$1" port
  port="${v##*:}"
  [ "$port" != "$v" ] && [ -n "${v%:*}" ] && [[ "$port" =~ ^[0-9]+$ ]] && [ "$port" -ge 1 ] && [ "$port" -le 65535 ]
}

# Refuse to launch with settings the binaries would reject or misread — the
# file may have been edited by hand. Only the launching scripts call this.
asm_validate() {
  local key
  if [ "${ASM_ALLOWED_HOSTS_INVALID:-0}" = 1 ]; then
    err "  fix ASM_UI_ALLOWED_HOSTS with --ui-allowed-host, in the web UI's Config dialog, or in $ASM_CONFIG_FILE"
    return 2
  fi
  for key in "${ASM_CONFIG_KEYS[@]}"; do
    case "${!key:-}" in
      *'|'*) _bad "$key" "values may not contain '|'"; return 2 ;;
      *[[:cntrl:]]*) _bad "$key" "values must be a single line of text"; return 2 ;;
    esac
  done
  case "$ASM_PROFILE" in debug|release) : ;; *) _bad ASM_PROFILE "expected debug or release"; return 2 ;; esac
  for key in ASM_RUN_DAEMON ASM_RUN_RELAY ASM_RUN_UI; do
    case "${!key}" in 0|1) : ;; *) _bad "$key" "expected true or false"; return 2 ;; esac
  done
  _is_addr_port "$ASM_BIND" || { _bad ASM_BIND "expected ADDR:PORT, e.g. 0.0.0.0:4600"; return 2; }
  _is_addr_port "$ASM_RELAY_BIND" || { _bad ASM_RELAY_BIND "expected ADDR:PORT, e.g. 0.0.0.0:4700"; return 2; }
  _port ASM_UI_PORT "$ASM_UI_PORT" || return 2
  case "$ASM_RELAY_URL" in ''|ws://?*|wss://?*) : ;; *) _bad ASM_RELAY_URL "expected a ws:// or wss:// relay URL"; return 2 ;; esac
  case "$ASM_UI_DAEMON" in ''|http://?*|https://?*) : ;; *) _bad ASM_UI_DAEMON "expected an http:// or https:// URL"; return 2 ;; esac
  for key in ASM_ENROLLMENT_TOKEN ASM_RELAY_KEY ASM_RELAY_KEYS ASM_UI_DAEMON_TOKEN ASM_RELAY_DOWNSTREAMS ASM_UI_HOST; do
    case "${!key}" in *[[:space:]]*) _bad "$key" "may not contain spaces"; return 2 ;; esac
  done
  # The enrollment token is the LAN's only gate to enrolling (same rule as the
  # daemon, which refuses to start with a weak one).
  if [ -n "$ASM_ENROLLMENT_TOKEN" ]; then
    case "$ASM_ENROLLMENT_TOKEN" in
      *,*) _bad ASM_ENROLLMENT_TOKEN "may not contain commas"; return 2 ;;
    esac
    if [ "${#ASM_ENROLLMENT_TOKEN}" -lt 16 ]; then
      err "invalid startup setting ASM_ENROLLMENT_TOKEN: expected at least 16 characters"; return 2
    fi
  fi
  if ! daemon_enabled && ! relay_enabled && ! ui_enabled; then
    err "nothing to start: the daemon, relay, and web UI are all off"
    err "  turn one on with --daemon / --relay / --ui, or in the web UI's Config dialog ($ASM_CONFIG_FILE)"
    return 2
  fi
  if relay_enabled && [ -z "$ASM_RELAY_KEYS" ]; then
    err "the bundled relay needs --relay-key KEY (the access key nodes present)"; return 2
  fi
  if daemon_enabled && [ -n "$ASM_RELAY_URL" ] && [ -z "$ASM_RELAY_KEY" ]; then
    err "registering to $ASM_RELAY_URL needs --relay-key KEY (the relay's access key)"; return 2
  fi
  return 0
}

# One line saying where this run's settings came from.
log_config_origin() {
  case "${ASM_CONFIG_ORIGIN:-none}" in
    saved)  log "settings — $ASM_CONFIG_FILE$([ ${#ASM_CLI_SET[@]} -gt 0 ] || [ -n "${ASM_CLI_RELAY_KEY:-}${ASM_CLI_PROFILE:-}${ASM_UI_ALLOWED_HOSTS_ADD:-}" ] && echo ' + flags (saved back)')" ;;
    legacy) log "settings — recovered from the running stack's records; saving them to $ASM_CONFIG_FILE" ;;
    *)      log "settings — defaults$([ ${#ASM_CLI_SET[@]} -gt 0 ] && echo ' + flags'); saving them to $ASM_CONFIG_FILE" ;;
  esac
}

# Is the process named in $1 (a pidfile) alive?
pid_alive() {
  local f="$1" p
  [ -f "$f" ] || return 1
  p="$(cat "$f" 2>/dev/null || true)"
  [ -n "$p" ] && kill -0 "$p" 2>/dev/null
}

cargo_build() {
  if [ "$PROFILE" = release ]; then
    ( cd "$ROOT" && cargo build --release "$@" )
  else
    ( cd "$ROOT" && cargo build "$@" )
  fi
}

wait_socket() {
  local i
  for i in $(seq 1 60); do
    if [ -S "$ASMUX_SOCK" ]; then return 0; fi
    sleep 0.1
  done
  return 1
}

wait_health() {
  local i
  command -v curl >/dev/null 2>&1 || { sleep 0.6; return 0; }
  for i in $(seq 1 60); do
    if curl -sf "http://$ASM_BIND/health" >/dev/null 2>&1; then return 0; fi
    sleep 0.1
  done
  return 1
}

# The daemon binds BEFORE it adopts the holder's sessions, so `/health` answers
# in milliseconds however many survived (that is why the check above can keep a
# short fuse). Adoption is a serial holder round-trip per session and lands a
# little later; this waits for it, purely so a script can honestly claim the
# sessions are back. 60s: adoption is ~1s/session in the worst case seen.
wait_reconciled() {
  local i
  command -v curl >/dev/null 2>&1 || return 0
  for i in $(seq 1 600); do
    if curl -sf "http://$ASM_BIND/health" 2>/dev/null | grep -q '"reconciling":false'; then
      return 0
    fi
    sleep 0.1
  done
  return 1
}

# Whether this host runs a bundled relay (ASM_RUN_RELAY; the key is validated
# separately). A shared rendezvous, not a per-daemon sidecar.
relay_enabled() { [ "${ASM_RUN_RELAY:-0}" = 1 ]; }

ui_enabled() { [ "${ASM_RUN_UI:-0}" = 1 ]; }
daemon_enabled() { [ "${ASM_RUN_DAEMON:-1}" = 1 ]; }

# The off-host proxy target and its token only mean something when this host
# runs no daemon; the saved config keeps them while it does, and the UI then
# proxies to this host's own daemon instead.
ui_proxy_daemon() { if ! daemon_enabled; then printf '%s' "${ASM_UI_DAEMON:-}"; fi; }
ui_proxy_token() { if ! daemon_enabled; then printf '%s' "${ASM_UI_DAEMON_TOKEN:-}"; fi; }

ui_reg_signature() {
  printf '%s|%s|%s|%s|%s|%s|%s|%s' \
    "${ASM_RUN_UI:-0}" "${ASM_UI_HOST:-}" "${ASM_UI_PORT:-}" "${ASM_BIND:-}" \
    "$(daemon_enabled && echo 0 || echo 1)" "$(ui_proxy_daemon)" "$(ui_proxy_token)" \
    "${ASM_UI_ALLOWED_HOSTS:-}"
}

record_ui_state() {
  # This record may contain a gateway bearer token. Tighten permissions even
  # when replacing a state file created by an older version under a broad umask.
  (
    umask 077
    ui_reg_signature > "$UI_STATE_FILE"
    chmod 600 "$UI_STATE_FILE"
  )
}

# Turn a wildcard listen host into an address curl/the proxy can dial locally.
ui_local_host() {
  case "${1:-}" in
    0.0.0.0|true|'*') printf '127.0.0.1' ;;
    ::|'[::]')         printf '[::1]' ;;
    *)                 printf '%s' "$1" ;;
  esac
}

ui_url() {
  printf 'http://%s:%s' "$(ui_local_host "$ASM_UI_HOST")" "$ASM_UI_PORT"
}

ui_daemon_url() {
  local bind_host_port="$ASM_BIND" proxy
  proxy="$(ui_proxy_daemon)"
  if [ -n "$proxy" ]; then
    printf '%s' "${proxy%/}"
    return 0
  fi
  case "$bind_host_port" in
    0.0.0.0:*) bind_host_port="127.0.0.1:${bind_host_port#*:}" ;;
    \[::\]:*)  bind_host_port="[::1]:${bind_host_port#*]:}" ;;
  esac
  printf 'http://%s' "$bind_host_port"
}

wait_ui() {
  local i
  command -v curl >/dev/null 2>&1 || { sleep 0.8; pid_alive "$UI_PIDFILE"; return; }
  for i in $(seq 1 100); do
    pid_alive "$UI_PIDFILE" || return 1
    if curl -sf "$(ui_url)" >/dev/null 2>&1; then return 0; fi
    sleep 0.1
  done
  return 1
}

# Start the Vite development UI as a detached, directly-managed process. Calling
# the local Vite executable (rather than leaving npm as an extra parent) makes
# the pidfile identify the server we need to stop. Its proxy follows ASM_BIND.
start_ui() {
  local want recorded
  want="$(ui_reg_signature)"
  recorded="$(cat "$UI_STATE_FILE" 2>/dev/null || true)"
  if pid_alive "$UI_PIDFILE"; then
    if [ "$want" != "$recorded" ]; then
      log "web UI already running (pid $(cat "$UI_PIDFILE")) — config changed, restarting it"
      stop_one asm-ui "$UI_PIDFILE"
    else
      log "web UI already running (pid $(cat "$UI_PIDFILE"))  $(ui_url)"
      return 0
    fi
  fi

  case "$(ui_proxy_daemon)" in
    ''|http://*|https://*) : ;;
    *) err "--ui-daemon expects an http:// or https:// URL"; return 2 ;;
  esac
  if ! [[ "$ASM_UI_PORT" =~ ^[0-9]+$ ]] || [ "$ASM_UI_PORT" -lt 1 ] || [ "$ASM_UI_PORT" -gt 65535 ]; then
    err "UI port must be between 1 and 65535 (got: $ASM_UI_PORT)"
    return 2
  fi
  command -v node >/dev/null 2>&1 || {
    err "managed UI needs Node.js; install Node 20+ or pass --no-ui for daemon-only/packaged UI"
    return 1
  }
  [ -x "$UI_BIN" ] || {
    err "missing $UI_BIN — install client dependencies first: (cd client && npm install)"
    return 1
  }

  log "starting Vite web UI on $ASM_UI_HOST:$ASM_UI_PORT..."
  (
    cd "$ROOT/client"
    # ASM_UI_MANAGED tells the UI server's Config endpoint (vite.config-api.ts)
    # that this start script owns it; it finds startup.conf via the inherited
    # ASM_DATA_DIR / ASM_RUNTIME_DIR.
    ASM_DAEMON="$(ui_daemon_url)" ASM_DAEMON_TOKEN="$(ui_proxy_token)" \
      ASM_CLIENT_HOST="$ASM_UI_HOST" ASM_CLIENT_ALLOWED_HOSTS="$ASM_UI_ALLOWED_HOSTS" \
      ASM_UI_MANAGED=1 \
      nohup "$UI_BIN" --host "$ASM_UI_HOST" --port "$ASM_UI_PORT" --strictPort \
      >>"$LOG_DIR/asm-ui.log" 2>&1 </dev/null &
    echo $! > "$UI_PIDFILE"
  )
  record_ui_state
  if wait_ui; then
    log "web UI up (pid $(cat "$UI_PIDFILE"))  $(ui_url)"
  else
    err "web UI did not come up; see $LOG_DIR/asm-ui.log"
    rm -f "$UI_PIDFILE"
    return 1
  fi
}

# Converge the managed UI on the settings: enabled starts, revives, or restarts
# it (when its settings changed); disabled stops a live one.
sync_ui() {
  if ui_enabled; then
    start_ui
  else
    if pid_alive "$UI_PIDFILE"; then stop_one asm-ui "$UI_PIDFILE"; fi
    record_ui_state
  fi
}

wait_relay() {
  local i host key
  command -v curl >/dev/null 2>&1 || { sleep 0.6; return 0; }
  host="${ASM_RELAY_BIND/0.0.0.0/127.0.0.1}"
  key="${ASM_RELAY_KEYS%%,*}"
  for i in $(seq 1 60); do
    if curl -sf "http://$host/nodes?relay_key=$key" >/dev/null 2>&1; then return 0; fi
    sleep 0.1
  done
  return 1
}

# Start the rendezvous relay (idempotent), if enabled. Runs detached (nohup);
# nodes and clients reach it over the network. A live relay whose bind or keys
# changed is restarted (its nodes redial on their own); otherwise it is left
# alone, since a restart drops every connected node and client.
start_relay() {
  relay_enabled || return 0
  if pid_alive "$RELAY_PIDFILE"; then
    local recorded
    recorded="$(cut -d'|' -f1,2 "$RELAY_STATE_FILE" 2>/dev/null || true)"
    if [ "$recorded" = "$ASM_RELAY_BIND|$ASM_RELAY_KEYS" ]; then
      log "relay already running (pid $(cat "$RELAY_PIDFILE"))"
      return 0
    fi
    log "relay already running (pid $(cat "$RELAY_PIDFILE")) — config changed, restarting it (nodes reconnect)"
    stop_one asm-relay "$RELAY_PIDFILE"
  fi
  [ -x "$RELAY_BIN" ] || { err "missing $RELAY_BIN — build first (cargo build -p asm-relay)"; return 1; }
  log "starting asm-relay on $ASM_RELAY_BIND..."
  ASM_RELAY_BIND="$ASM_RELAY_BIND" ASM_RELAY_KEYS="$ASM_RELAY_KEYS" \
    nohup "$RELAY_BIN" >>"$LOG_DIR/asm-relay.log" 2>&1 </dev/null &
  echo $! > "$RELAY_PIDFILE"
  printf '%s|%s|%s' "$ASM_RELAY_BIND" "$ASM_RELAY_KEYS" "$(daemon_enabled && echo 0 || echo 1)" > "$RELAY_STATE_FILE"
  if wait_relay; then
    log "relay up (pid $(cat "$RELAY_PIDFILE"))  http://$ASM_RELAY_BIND"
  else
    err "relay did not come up; see $LOG_DIR/asm-relay.log"
    return 1
  fi
}

# Converge the bundled relay on the settings (start.sh): start or restart it
# when enabled, stop a live one that the settings no longer ask for.
sync_relay() {
  if relay_enabled; then
    start_relay
  elif pid_alive "$RELAY_PIDFILE"; then
    log "relay is no longer enabled in the settings — stopping it"
    stop_one asm-relay "$RELAY_PIDFILE"
    rm -f "$RELAY_STATE_FILE"
  else
    log "relay disabled (pass --relay --relay-key KEY, or enable it in the web UI's Config)"
  fi
}

# Start the holder (idempotent). It runs detached (nohup) so it outlives the
# daemon — that is what makes sessions durable across a daemon restart.
# Does a LIVE holder answer on the socket? This is the only check that means
# anything: a pid can be alive while the socket is gone (an "orphan" — the holder
# still holds PTYs but nothing can dial it). Gating on pid alone is what let the
# 2026-07-12 incident wedge the stack; `asmux probe` exits 0 only if someone answers.
holder_live() {
  [ -x "$ASMUX_BIN" ] || return 1
  ASMUX_SOCK="$ASMUX_SOCK" ASM_RUNTIME_DIR="$ASM_RUNTIME_DIR" \
    "$ASMUX_BIN" probe >/dev/null 2>&1
}

start_asmux() {
  if holder_live; then
    log "asmux already running (pid $(cat "$ASMUX_PIDFILE" 2>/dev/null || echo '?'))"
    return 0
  fi

  # Pid alive but nobody answering = an orphaned holder: its socket was unlinked
  # out from under it. asmux now rebinds itself within ~5s, so give it a moment
  # before concluding anything.
  if pid_alive "$ASMUX_PIDFILE"; then
    local p i
    p="$(cat "$ASMUX_PIDFILE")"
    log "asmux (pid $p) is alive but not answering on $ASMUX_SOCK — waiting for it to rebind..."
    for i in $(seq 1 20); do
      sleep 0.5
      if holder_live; then log "asmux recovered its socket (pid $p) — sessions intact"; return 0; fi
    done
    err "asmux (pid $p) is ORPHANED: alive, holding live PTYs, but unreachable — its socket path"
    err "is owned by something else, or rebinding failed. Its sessions CANNOT be attached, and"
    err "killing it will lose them. Inspect first:  $LOG_DIR/asmux.log"
    err "To force a clean holder anyway (THIS KILLS ITS SESSIONS):"
    err "    kill $p && scripts/start.sh"
    return 1
  fi

  [ -x "$ASMUX_BIN" ] || { err "missing $ASMUX_BIN — build first (cargo build -p asmux)"; return 1; }
  log "starting asmux holder..."
  ASM_RUNTIME_DIR="$ASM_RUNTIME_DIR" ASMUX_SOCK="$ASMUX_SOCK" \
    nohup "$ASMUX_BIN" >>"$LOG_DIR/asmux.log" 2>&1 </dev/null &
  echo $! > "$ASMUX_PIDFILE"
  if wait_socket; then
    log "asmux up (pid $(cat "$ASMUX_PIDFILE"))  socket=$ASMUX_SOCK"
  else
    err "asmux did not come up; see $LOG_DIR/asmux.log"
    return 1
  fi
}

# The daemon reads bind/label/relay-registration config only at startup, so a
# running daemon can't adopt new settings in place. This signature captures
# those daemon-affecting settings (plus the binary's build profile);
# start_daemon records it on launch and compares it on a re-run to decide
# whether the config actually changed. Optional fields are appended only when
# set, so a daemon recorded by an older version of this script still matches.
daemon_reg_signature() {
  printf '%s|%s|%s|%s' \
    "${ASM_BIND:-}" "${ASM_NODE_LABEL:-}" "${ASM_RELAY_URL:-}" "${ASM_RELAY_KEY:-}"
  if [ -n "${ASM_RELAY_DOWNSTREAMS:-}" ]; then printf '|downstreams=%s' "$ASM_RELAY_DOWNSTREAMS"; fi
  # A checksum, not the token: the record is a plain runtime file.
  if [ -n "${ASM_ENROLLMENT_TOKEN:-}" ]; then
    printf '|enroll=%s' "$(printf '%s' "$ASM_ENROLLMENT_TOKEN" | cksum | cut -d' ' -f1)"
  fi
  if [ "${PROFILE:-debug}" = release ]; then printf '|release'; fi
  return 0
}

# The daemon is off in the settings: stop a live one. The holder is left alone —
# it may hold live sessions, which outlive the daemon (turn it back on to reach
# them again); only `stop.sh asmux` ends them.
stop_disabled_daemon() {
  if pid_alive "$DAEMON_PIDFILE"; then
    log "daemon is off in the settings — stopping it (the holder keeps any live sessions)"
    stop_one asm-daemon "$DAEMON_PIDFILE"
  fi
  rm -f "$DAEMON_STATE_FILE"
  if holder_live; then
    log "note   — the session holder is still up; its sessions are unreachable until the daemon is back (scripts/stop.sh asmux ends them)"
  fi
}

# Start the daemon in sidecar mode (idempotent). Autospawn is off — the scripts
# manage the holder so restarting the daemon never spawns a second one. Any
# ASM_RELAY_URL/ASM_RELAY_KEY (from --register) is inherited, so the daemon
# registers outbound when configured to.
#
# If the daemon is already running but its settings (saved config + flags)
# differ from what it booted with, restart it to apply them — the asmux holder
# stays up, so live sessions survive. That is how a change saved in the web
# UI's Config dialog takes effect on the next flagless start.sh.
start_daemon() {
  if pid_alive "$DAEMON_PIDFILE"; then
    local want recorded
    want="$(daemon_reg_signature)"
    recorded="$(cat "$DAEMON_STATE_FILE" 2>/dev/null || true)"
    if [ "$want" != "$recorded" ]; then
      log "daemon already running (pid $(cat "$DAEMON_PIDFILE")) — config changed, restarting to apply it (live sessions survive)"
      stop_one asm-daemon "$DAEMON_PIDFILE"
      # fall through to launch a fresh daemon with the new config
    else
      log "daemon already running (pid $(cat "$DAEMON_PIDFILE"))"
      return 0
    fi
  fi
  [ -x "$DAEMON_BIN" ] || { err "missing $DAEMON_BIN — build first (cargo build -p asm-daemon)"; return 1; }
  log "starting asm-daemon (sidecar) on $ASM_BIND..."
  ASM_BACKEND=sidecar ASM_ASMUX_AUTOSPAWN=0 \
    nohup "$DAEMON_BIN" >>"$LOG_DIR/asm-daemon.log" 2>&1 </dev/null &
  echo $! > "$DAEMON_PIDFILE"
  daemon_reg_signature > "$DAEMON_STATE_FILE"
  if wait_health; then
    log "daemon up (pid $(cat "$DAEMON_PIDFILE"))  http://$ASM_BIND"
  else
    err "daemon health check failed; see $LOG_DIR/asm-daemon.log"
    return 1
  fi
}

# SIGTERM a pidfile's process, escalate to SIGKILL if it lingers.
stop_one() {
  local name="$1" f="$2" p i
  if pid_alive "$f"; then
    p="$(cat "$f")"
    log "stopping $name (pid $p)..."
    kill -TERM "$p" 2>/dev/null || true
    for i in $(seq 1 40); do
      if ! kill -0 "$p" 2>/dev/null; then break; fi
      sleep 0.1
    done
    if kill -0 "$p" 2>/dev/null; then
      err "$name did not stop; sending SIGKILL"
      kill -KILL "$p" 2>/dev/null || true
    fi
  else
    log "$name not running"
  fi
  rm -f "$f"
}
