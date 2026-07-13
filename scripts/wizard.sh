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
RELAY_TLS=0; RELAY_TLS_CERT=""; RELAY_TLS_KEY=""; RELAY_CA=""
DAEMON_TLS=0; DAEMON_TLS_CERT=""; DAEMON_TLS_KEY=""

this_ip() { local ip; ip="$(hostname -I 2>/dev/null | awk '{print $1}')"; printf '%s' "${ip:-<this-host-ip>}"; }
client_relay_url() { case "$1" in ws://*) printf 'http://%s' "${1#ws://}" ;; wss://*) printf 'https://%s' "${1#wss://}" ;; *) printf '%s' "$1" ;; esac; }
# A TLS relay URL. https:// counts: the daemon accepts it and translates it to
# wss://, so someone who pasted the browser's URL must still be asked about a
# private CA — otherwise the node boots fine and silently never registers.
_is_tls_relay_url() { case "$1" in wss://*|https://*) return 0 ;; *) return 1 ;; esac; }

# Offer the daemon a certificate. With one it serves https:// and the direct LAN
# path is encrypted like any other; without one it is plaintext — a legitimate
# choice on a network you trust, so this informs rather than interrogates.
ask_daemon_tls() {
  title "TLS for this daemon"
  note "Without a certificate, clients reach it over plain http:// — on that network the"
  note "device token and everything you type is readable by anyone watching the traffic."
  note "With one, they use https://<this-host>:$DAEMON_PORT and the direct path is encrypted."
  if yesno "Do you have a TLS certificate for this host?" n; then
    prompt_required DAEMON_TLS_CERT "Path to the certificate chain (PEM)"
    prompt_required DAEMON_TLS_KEY  "Path to the private key (PEM)"
    FLAGS+=(--tls-cert "$DAEMON_TLS_CERT" --tls-key "$DAEMON_TLS_KEY")
    DAEMON_TLS=1
  else
    note "OK — plaintext. Fine on a network you trust; otherwise use “Behind NAT” with a"
    note "relay that has a certificate, or reach this host through an SSH tunnel."
  fi
}

# The relay is the one component that faces the open internet, and the only one
# that can hold a certificate. Offer it one.
ask_relay_tls() {
  title "TLS for the relay"
  note "The relay carries the device token and the whole terminal stream for every node"
  note "registered to it. With a certificate it serves https:// and wss://; without one,"
  note "all of that crosses it in the clear."
  if yesno "Do you have a TLS certificate for this relay?" n; then
    prompt_required RELAY_TLS_CERT "Path to the certificate chain (PEM)"
    prompt_required RELAY_TLS_KEY  "Path to the private key (PEM)"
    FLAGS+=(--relay-tls-cert "$RELAY_TLS_CERT" --relay-tls-key "$RELAY_TLS_KEY")
    RELAY_TLS=1
  else
    note "OK — a plaintext relay. Fine on a trusted LAN. Before this box is reachable"
    note "from the internet, get a real certificate (any ACME client issues one free):"
    note "browsers cannot be told to trust a self-signed relay, they can only warn."
  fi
}

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
      "Local only — daemon only; you'll use the client on THIS machine (loopback)" \
      "Direct on the LAN — daemon only; clients on your network connect to this host" \
      "Relay host — daemon + relay: sessions run here AND private / NAT'd nodes relay through it" \
      "Relay only — no sessions here; this box just relays for private / NAT'd nodes" \
      "Behind NAT — daemon only; this host can't accept inbound, so it dials OUT to a relay"
    case "$c" in 1) ROLE=local ;; 2) ROLE=lan ;; 3) ROLE=relayhost ;; 4) ROLE=relayonly ;; 5) ROLE=nat ;; esac
  fi

  FLAGS=()
  case "$ROLE" in
    reload | local) : ;;  # defaults — no connectivity flags
    lan)
      prompt_port DAEMON_PORT "Port for network clients to connect to" "4600"
      FLAGS=(--bind "0.0.0.0:$DAEMON_PORT")
      ask_daemon_tls
      ;;
    relayhost)
      prompt_port DAEMON_PORT "Daemon port (for direct LAN clients too)" "4600"
      prompt_required RELAY_KEY "Relay access key (shared secret nodes & clients present)"
      prompt_port RELAY_PORT "Relay port (private nodes register here)" "4700"
      FLAGS=(--bind "0.0.0.0:$DAEMON_PORT" --relay --relay-key "$RELAY_KEY")
      [ "$RELAY_PORT" != "4700" ] && FLAGS+=(--relay-bind "0.0.0.0:$RELAY_PORT")
      ask_daemon_tls
      ask_relay_tls
      ;;
    relayonly)
      prompt_required RELAY_KEY "Relay access key (shared secret nodes & clients present)"
      prompt_port RELAY_PORT "Relay port (private nodes register here)" "4700"
      FLAGS=(--relay-only --relay-key "$RELAY_KEY")
      [ "$RELAY_PORT" != "4700" ] && FLAGS+=(--relay-bind "0.0.0.0:$RELAY_PORT")
      ask_relay_tls
      ;;
    nat)
      prompt_required RELAY_URL "Relay URL to register to (e.g. wss://relay.example.com)"
      prompt_required RELAY_KEY "Relay access key (must match the relay host's)"
      FLAGS=(--register "$RELAY_URL" --relay-key "$RELAY_KEY")
      if _relay_url_is_plaintext_remote "$RELAY_URL"; then
        title "Heads up — that relay URL is unencrypted"
        note "ws:// to a remote host sends the device token and every keystroke in the clear."
        note "If the relay has a certificate, register to it as wss:// instead."
        yesno "Continue with the plaintext relay anyway?" n || { log "cancelled — nothing ran."; exit 0; }
        FLAGS+=(--insecure-relay)
      elif _is_tls_relay_url "$RELAY_URL"; then
        # A public (ACME) cert just works. A private one has to be handed over,
        # or this node will boot fine and then silently never register.
        if yesno "Is that relay's certificate self-signed or privately signed?" n; then
          prompt_required RELAY_CA "Path to the CA / certificate PEM to trust"
          FLAGS+=(--relay-ca "$RELAY_CA")
        fi
      fi
      ;;
  esac

  if yesno "Build/run the release profile (faster; for real use)?" n; then FLAGS+=(--release); fi
}

# ── post-run guidance, tailored to the role ───────────────────────────────
post_tips() {
  # The relay's schemes follow whether it got a certificate above.
  local rws=ws rhttp=http rhost dhttp=http
  if [ "$RELAY_TLS" = 1 ]; then rws=wss; rhttp=https; fi
  if [ "$DAEMON_TLS" = 1 ]; then dhttp=https; fi
  rhost="$(this_ip)"

  case "$ROLE" in
    local)
      title "Next"
      log "open the web client — this host shows up as “This machine”:"
      note "cd client && npm run dev"
      ;;
    lan | relayhost)
      title "Enrolling a client on the network"
      local tok; tok="$("$HERE/token.sh" 2>/dev/null | tail -n1 || true)"
      if [ -n "$tok" ]; then note "enrollment token: $(bold "$tok")"; else note "get the token: scripts/token.sh"; fi
      note "in the client → manage → add a daemon:  $dhttp://$rhost:$DAEMON_PORT   + that token"
      if [ "$DAEMON_TLS" = 1 ]; then
        note "…using the hostname the certificate was issued for, not the bare IP — TLS"
        note "verifies the name. A self-signed cert will prompt the browser once."
      else
        note "the client flags that URL as unencrypted — expected here, and it still connects."
      fi
      ;;
  esac
  case "$ROLE" in
    relayhost | relayonly)
      title "Point a NAT'd node at this relay"
      log "on each private node, run its own wizard and pick “Behind NAT”, or:"
      note "scripts/start.sh --register $rws://$rhost:$RELAY_PORT --relay-key $RELAY_KEY"
      if [ "$RELAY_TLS" = 1 ]; then
        note "…using the hostname your certificate was issued for, not the bare IP —"
        note "TLS verifies the name, so an IP will be rejected."
      fi
      title "In the client"
      note "manage → Relays → add:  $rhttp://$rhost:$RELAY_PORT   + key  $RELAY_KEY"
      [ "$RELAY_TLS" = 1 ] || note "(plaintext relay: the client will ask you to confirm that)"
      ;;
    nat)
      title "Confirm the registration"
      log "this host dials the relay in the background — check it connected:"
      note "grep -a 'registered control stream' $LOG_DIR/asm-daemon.log"
      if _is_tls_relay_url "$RELAY_URL"; then
        note "a TLS failure shows up here too — as 'relay agent connection error'."
      fi
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
    "Everything — daemon, session holder, and relay  (LIVE SESSIONS END)" \
    "Just the daemon — sessions stay alive in the holder" \
    "Just the relay — connected nodes / clients drop"
  case "$c" in
    1) run_script stop.sh all ;;
    2) run_script stop.sh daemon ;;
    3) run_script stop.sh relay ;;
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
