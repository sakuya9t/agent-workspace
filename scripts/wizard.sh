#!/usr/bin/env bash
# Interactive setup wizard for the asm stack — a friendly front-end over
# start.sh / restart-daemon.sh / stop.sh so you don't have to remember flags.
#
# It asks a few plain questions (what to do, how this host is reached), then
# shows the EXACT underlying command and runs it once you confirm. Nothing here
# is magic: it only ever calls those three scripts, which you can run directly.
#
#   scripts/wizard.sh
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=_asm_common.sh
source "$HERE/_asm_common.sh"   # log/err + pid_alive + asm_configure + ROOT

case "${1:-}" in
  -h|--help) echo "Usage: scripts/wizard.sh   (interactive; no flags)"; exit 0 ;;
esac

# The wizard is all prompts — bail clearly if there's no terminal to read from.
if [ ! -t 0 ] || [ ! -t 1 ]; then
  err "the wizard needs an interactive terminal."
  err "non-interactive? use scripts/start.sh --help for the flag-based path."
  exit 1
fi

# Resolve default paths (pidfiles etc.) without parsing any flags.
asm_configure

# ── small prompt helpers ──────────────────────────────────────────────────
bold()  { printf '\033[1m%s\033[0m' "$*"; }
title() { printf '\n\033[1;36m%s\033[0m\n' "$*"; }
note()  { printf '    %s\n' "$*"; }

# prompt VAR "question" [default]  — Ctrl-D / Ctrl-C exits the wizard.
prompt() {
  local __v="$1" __q="$2" __d="${3:-}" __a
  if [ -n "$__d" ]; then
    read -rp "$__q [$__d]: " __a || { echo; exit 130; }
    __a="${__a:-$__d}"
  else
    read -rp "$__q: " __a || { echo; exit 130; }
  fi
  printf -v "$__v" '%s' "$__a"
}

prompt_required() {
  while :; do
    prompt "$1" "$2"
    if [ -n "${!1}" ]; then return 0; fi
    err "  ↳ required — please enter a value."
  done
}

prompt_port() {
  while :; do
    prompt "$1" "$2" "$3"
    local v="${!1}"
    if [[ "$v" =~ ^[0-9]+$ ]] && [ "$v" -ge 1 ] && [ "$v" -le 65535 ]; then return 0; fi
    err "  ↳ enter a port between 1 and 65535."
  done
}

# menu VAR "title" "opt1" "opt2" ...  — sets VAR to the 1-based choice.
menu() {
  local __v="$1"; shift
  local __t="$1"; shift
  local __opts=("$@") __i __sel
  title "$__t"
  for __i in "${!__opts[@]}"; do printf '  %d) %s\n' "$((__i + 1))" "${__opts[$__i]}"; done
  while :; do
    read -rp "> " __sel || { echo; exit 130; }
    if [[ "$__sel" =~ ^[0-9]+$ ]] && [ "$__sel" -ge 1 ] && [ "$__sel" -le "${#__opts[@]}" ]; then
      printf -v "$__v" '%s' "$__sel"; return 0
    fi
    err "  ↳ enter a number 1-${#__opts[@]}."
  done
}

# yesno "question" [default y|n]  — returns 0 for yes. Call only in a condition.
yesno() {
  local __a __hint="[Y/n]"
  [ "${2:-y}" = n ] && __hint="[y/N]"
  read -rp "$1 $__hint: " __a || { echo; exit 130; }
  case "${__a:-${2:-y}}" in [Yy]*) return 0 ;; *) return 1 ;; esac
}

# Show the resolved command (relative path, shell-quoted), confirm, then run it.
run_script() {
  local script="$1"; shift
  local shown="scripts/$script" a
  for a in "$@"; do shown+=" $(printf '%q' "$a")"; done
  title "Ready to run"
  printf '  %s\n' "$(bold "$shown")"
  yesno "Run this now?" y || { log "cancelled — nothing ran."; exit 0; }
  echo
  "$HERE/$script" "$@"
}

# ── connectivity → flags ──────────────────────────────────────────────────
FLAGS=(); ROLE=""; RELAY_KEY=""; RELAY_URL=""; DAEMON_PORT="4600"; RELAY_PORT="4700"
UI_WANTED=0; UI_HOST="127.0.0.1"; UI_PORT="5273"
UI_DAEMON=""; UI_DAEMON_TOKEN=""

# Defaults for a restart should reflect the last wizard/script selection.
RECORDED_UI_ENABLED=1; RECORDED_UI_HOST="127.0.0.1"; RECORDED_UI_PORT="5273"
if [ -f "$UI_STATE_FILE" ]; then
  IFS='|' read -r RECORDED_UI_ENABLED RECORDED_UI_HOST RECORDED_UI_PORT _ < "$UI_STATE_FILE" || true
  RECORDED_UI_ENABLED="${RECORDED_UI_ENABLED:-0}"
  RECORDED_UI_HOST="${RECORDED_UI_HOST:-127.0.0.1}"
  RECORDED_UI_PORT="${RECORDED_UI_PORT:-5273}"
fi

this_ip() { local ip; ip="$(hostname -I 2>/dev/null | awk '{print $1}')"; printf '%s' "${ip:-<this-host-ip>}"; }
client_relay_url() { case "$1" in ws://*) printf 'http://%s' "${1#ws://}" ;; wss://*) printf 'https://%s' "${1#wss://}" ;; *) printf '%s' "$1" ;; esac; }

# Ask how this host is reached and build FLAGS + ROLE for a start/restart.
choose_connectivity() {
  local mode="$1" c
  if [ "$mode" = restart ]; then
    menu c "What should this restart do?" \
      "Just reload the daemon — keep current settings & live sessions (a bundled relay stays up; revived if it died)" \
      "Switch to: direct on the LAN (network clients connect here)" \
      "Switch to: behind NAT (register out to a relay)"
    case "$c" in 1) ROLE=reload ;; 2) ROLE=lan ;; 3) ROLE=nat ;; esac
  else
    menu c "How will clients reach this host?" \
      "Local only — daemon + default loopback UI on THIS machine" \
      "UI-only gateway — web client here, but no local daemon, holder, or SSH/session layer" \
      "Direct on the LAN — daemon + UI; clients on your network connect to this host" \
      "Relay host — daemon + relay: sessions run here AND private / NAT'd nodes relay through it" \
      "Relay only — no sessions here; this box just relays for private / NAT'd nodes" \
      "Behind NAT — daemon + local UI; this host dials OUT to a relay"
    case "$c" in 1) ROLE=local ;; 2) ROLE=uionly ;; 3) ROLE=lan ;; 4) ROLE=relayhost ;; 5) ROLE=relayonly ;; 6) ROLE=nat ;; esac
  fi

  FLAGS=()
  case "$ROLE" in
    reload | local) : ;;  # defaults — no connectivity flags
    uionly)
      FLAGS=(--ui-only)
      prompt UI_DAEMON "Daemon URL to proxy as the same-origin target (blank for client shell only)"
      if [ -n "$UI_DAEMON" ]; then
        FLAGS+=(--ui-daemon "$UI_DAEMON")
        prompt UI_DAEMON_TOKEN "Enrolled device bearer token (blank if target trusts this gateway)"
        [ -n "$UI_DAEMON_TOKEN" ] && FLAGS+=(--ui-daemon-token "$UI_DAEMON_TOKEN")
      fi
      ;;
    lan)
      prompt_port DAEMON_PORT "Port for network clients to connect to" "4600"
      FLAGS=(--bind "0.0.0.0:$DAEMON_PORT")
      ;;
    relayhost)
      prompt_port DAEMON_PORT "Daemon port (for direct LAN clients too)" "4600"
      prompt_required RELAY_KEY "Relay access key (shared secret nodes & clients present)"
      prompt_port RELAY_PORT "Relay port (private nodes register here)" "4700"
      FLAGS=(--bind "0.0.0.0:$DAEMON_PORT" --relay --relay-key "$RELAY_KEY")
      [ "$RELAY_PORT" != "4700" ] && FLAGS+=(--relay-bind "0.0.0.0:$RELAY_PORT")
      ;;
    relayonly)
      prompt_required RELAY_KEY "Relay access key (shared secret nodes & clients present)"
      prompt_port RELAY_PORT "Relay port (private nodes register here)" "4700"
      FLAGS=(--relay-only --relay-key "$RELAY_KEY")
      [ "$RELAY_PORT" != "4700" ] && FLAGS+=(--relay-bind "0.0.0.0:$RELAY_PORT")
      ;;
    nat)
      prompt_required RELAY_URL "Relay URL to register to (e.g. ws://relay-host:4700)"
      prompt_required RELAY_KEY "Relay access key (must match the relay host's)"
      FLAGS=(--register "$RELAY_URL" --relay-key "$RELAY_KEY")
      ;;
  esac

  choose_ui "$mode"
  if [ "$ROLE" != uionly ] && yesno "Build/run the release profile (faster; for real use)?" n; then FLAGS+=(--release); fi
}

# UI is independent of relay topology: any host that runs a daemon can also run
# a detached Vite server. Relay-only hosts have no daemon/API to proxy to.
choose_ui() {
  local mode="$1" enabled_default=n network_default=n
  if [ "$ROLE" = relayonly ]; then
    UI_WANTED=0
    FLAGS+=(--no-ui)
    note "No web UI will run on this relay-only host (there is no local daemon)."
    return 0
  fi

  if [ "$ROLE" != uionly ]; then
    if [ "$mode" = start ] || [ "$RECORDED_UI_ENABLED" = 1 ]; then enabled_default=y; fi
    if ! yesno "Run the live-reload web UI as a managed background service?" "$enabled_default"; then
      UI_WANTED=0
      FLAGS+=(--no-ui)
      return 0
    fi
  fi

  UI_WANTED=1
  if [ "$mode" = restart ] && [ "$RECORDED_UI_ENABLED" = 1 ]; then
    UI_PORT="$RECORDED_UI_PORT"
    UI_HOST="$RECORDED_UI_HOST"
  else
    UI_PORT="5273"
    UI_HOST="127.0.0.1"
  fi
  prompt_port UI_PORT "Web UI port" "$UI_PORT"

  case "$ROLE" in
    lan|relayhost) network_default=y ;;
    reload)
      case "$UI_HOST" in 127.0.0.1|localhost|::1|'[::1]') : ;; *) network_default=y ;; esac
      ;;
  esac
  if yesno "Make the web UI reachable from other machines on this network?" "$network_default"; then
    UI_HOST="0.0.0.0"
  else
    UI_HOST="127.0.0.1"
  fi
  if [ "$ROLE" = uionly ]; then
    FLAGS+=(--ui-host "$UI_HOST" --ui-port "$UI_PORT")
  else
    FLAGS+=(--ui --ui-host "$UI_HOST" --ui-port "$UI_PORT")
  fi
}

# ── post-run guidance, tailored to the role ───────────────────────────────
post_tips() {
  if [ "$UI_WANTED" = 1 ]; then
    local display_host="$UI_HOST"
    [ "$display_host" = "0.0.0.0" ] && display_host="$(this_ip)"
    title "Web UI"
    log "Vite is managed in the background and survives SSH logout:"
    note "open  http://$display_host:$UI_PORT"
    note "check/stop it:  scripts/status.sh  ·  scripts/stop.sh ui"
  fi
  case "$ROLE" in
    uionly)
      title "UI-only gateway"
      if [ -n "$UI_DAEMON" ]; then
        note "same-origin API/WebSocket proxy:  $UI_DAEMON"
      else
        note "client shell only — add daemons or relays from manage in the UI"
      fi
      note "no daemon, holder, relay, or agent/SSH session layer runs on this host"
      ;;
    local)
      if [ "$UI_WANTED" != 1 ]; then
        title "Web UI"
        if [ -f "$ROOT/client/dist/index.html" ]; then
          note "packaged UI:  http://127.0.0.1:$DAEMON_PORT"
        else
          note "no UI selected; enable it later with:  scripts/start.sh --ui --ui-host 127.0.0.1"
        fi
      fi
      ;;
    lan | relayhost)
      title "Enrolling a client on the network"
      local tok; tok="$("$HERE/token.sh" 2>/dev/null | tail -n1 || true)"
      if [ -n "$tok" ]; then note "enrollment token: $(bold "$tok")"; else note "get the token: scripts/token.sh"; fi
      note "in the client → manage → add a daemon:  http://$(this_ip):$DAEMON_PORT   + that token"
      ;;
  esac
  case "$ROLE" in
    relayhost | relayonly)
      title "Point a NAT'd node at this relay"
      log "on each private node, run its own wizard and pick “Behind NAT”, or:"
      note "scripts/start.sh --register ws://$(this_ip):$RELAY_PORT --relay-key $RELAY_KEY"
      title "In the client"
      note "manage → Relays → add:  http://$(this_ip):$RELAY_PORT   + key  $RELAY_KEY"
      ;;
    nat)
      title "Confirm the registration"
      log "this host dials the relay in the background — check it connected:"
      note "grep -a 'registered control stream' $LOG_DIR/asm-daemon.log"
      title "In the client"
      note "manage → Relays → add:  $(client_relay_url "$RELAY_URL")   + key  $RELAY_KEY"
      note "your node then appears under that relay, ready to connect."
      ;;
  esac
}

# ── actions ───────────────────────────────────────────────────────────────
do_start() {
  choose_connectivity start
  run_script start.sh "${FLAGS[@]}"
  post_tips
}

do_restart() {
  if ! pid_alive "$ASMUX_PIDFILE"; then
    err "nothing to restart — the session holder isn't running."
    log "pick “Start the service” first (it brings up the holder + daemon)."
    exit 1
  fi
  choose_connectivity restart
  run_script restart-daemon.sh "${FLAGS[@]}"
  post_tips
}

do_stop() {
  local c
  menu c "Stop what?" \
    "Everything — UI, daemon, session holder, and relay  (LIVE SESSIONS END)" \
    "Just the daemon — sessions and UI stay alive" \
    "Just the web UI" \
    "Just the relay — connected nodes / clients drop"
  case "$c" in
    1) run_script stop.sh all ;;
    2) run_script stop.sh daemon ;;
    3) run_script stop.sh ui ;;
    4) run_script stop.sh relay ;;
  esac
}

# ── main ──────────────────────────────────────────────────────────────────
printf '\n\033[1;36m%s\033[0m\n' "asm setup wizard"
printf '  a friendly front-end over start.sh / restart-daemon.sh / stop.sh\n'

menu action "What would you like to do?" \
  "Start the service" \
  "Restart the daemon (reload; keep live sessions)" \
  "Stop the service"

case "$action" in
  1) do_start ;;
  2) do_restart ;;
  3) do_stop ;;
esac
