#!/usr/bin/env bash
# One-shot bootstrap for a CLEAN machine. Takes a box with nothing but a shell
# and gets you to a working build of the Agent Session Manager:
#
#   1. system C toolchain (cc + make) — needed by portable-pty and the bundled
#      SQLite build; installed via your package manager if missing (may sudo).
#   2. Rust toolchain via rustup — into ~/.cargo / ~/.rustup, NO sudo. Adds the
#      clippy + rustfmt components used by the dev flow.
#   3. `cargo build` of the workspace (asm-daemon + asmux + asm-relay).
#   4. web-client deps (`npm install`) if Node is present — optional.
#
# Safe to re-run: every step is skipped when it's already satisfied.
#
#   scripts/setup.sh                  # install prerequisites + debug build
#   RELEASE=1 scripts/setup.sh        # release build instead
#   ASM_NO_BUILD=1 scripts/setup.sh   # install prerequisites only, don't build
#   ASM_NO_CLIENT=1 scripts/setup.sh  # skip the web client (npm) entirely
#
# After it finishes, run `source "$HOME/.cargo/env"` in your current shell (new
# shells pick cargo up automatically) and then `scripts/start.sh`.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

log()  { printf '\033[1;36m[setup]\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33m[setup]\033[0m %s\n' "$*" >&2; }
err()  { printf '\033[1;31m[setup]\033[0m %s\n' "$*" >&2; }
have() { command -v "$1" >/dev/null 2>&1; }

# ---------------------------------------------------------------------------
# sudo + package-manager detection. We only touch the system package manager
# for the C toolchain (and curl/git if they're missing) — everything Rust and
# Node-local stays in $HOME.
# ---------------------------------------------------------------------------
SUDO=""
if [ "$(id -u)" -ne 0 ]; then
  if have sudo; then SUDO="sudo"; fi
fi

# Echo the install command for the detected package manager, given a PM-neutral
# request of "toolchain" (C compiler + make) and/or plain package names.
pm_install() {
  local pm="" cmd=""
  if   have apt-get; then pm=apt
  elif have dnf;     then pm=dnf
  elif have pacman;  then pm=pacman
  elif have zypper;  then pm=zypper
  elif have apk;     then pm=apk
  elif have brew;    then pm=brew
  fi

  # Translate the neutral "toolchain" token per package manager.
  local pkgs=()
  local p
  for p in "$@"; do
    case "$p:$pm" in
      toolchain:apt)    pkgs+=(build-essential) ;;
      toolchain:dnf)    pkgs+=(gcc make) ;;
      toolchain:pacman) pkgs+=(base-devel) ;;
      toolchain:zypper) pkgs+=(gcc make) ;;
      toolchain:apk)    pkgs+=(build-base) ;;
      toolchain:brew)   ;;  # macOS ships cc/make with the Command Line Tools
      toolchain:*)      pkgs+=(gcc make) ;;
      *)                pkgs+=("$p") ;;
    esac
  done
  [ "${#pkgs[@]}" -gt 0 ] || return 0

  case "$pm" in
    apt)    cmd="$SUDO apt-get update && $SUDO apt-get install -y ${pkgs[*]}" ;;
    dnf)    cmd="$SUDO dnf install -y ${pkgs[*]}" ;;
    pacman) cmd="$SUDO pacman -Sy --needed --noconfirm ${pkgs[*]}" ;;
    zypper) cmd="$SUDO zypper install -y ${pkgs[*]}" ;;
    apk)    cmd="$SUDO apk add ${pkgs[*]}" ;;
    brew)   cmd="brew install ${pkgs[*]}" ;;
    *)      return 1 ;;
  esac

  log "installing: ${pkgs[*]}"
  eval "$cmd"
}

# ---------------------------------------------------------------------------
# 1. System prerequisites: a C compiler, make, curl, git.
#    rusqlite uses the `bundled` feature so no system SQLite/pkg-config needed.
# ---------------------------------------------------------------------------
ensure_system_deps() {
  local want=()
  if ! have cc && ! have gcc && ! have clang; then want+=(toolchain); fi
  if ! have make && [[ " ${want[*]-} " != *" toolchain "* ]]; then want+=(toolchain); fi
  if ! have curl && ! have wget; then want+=(curl); fi
  if ! have git; then want+=(git); fi

  if [ "${#want[@]}" -eq 0 ]; then
    log "system toolchain present (cc/make/curl/git) — skipping"
    return 0
  fi

  log "missing system packages for: ${want[*]}"
  if pm_install "${want[@]}"; then
    :
  else
    err "no supported package manager found (apt/dnf/pacman/zypper/apk/brew)."
    err "install a C compiler + make + curl + git manually, then re-run."
    exit 1
  fi

  # Verify the compiler landed — the Rust build hard-depends on it.
  if ! have cc && ! have gcc && ! have clang; then
    err "a C compiler is still not on PATH after install; cannot continue."
    err "on macOS run: xcode-select --install"
    exit 1
  fi
}

# ---------------------------------------------------------------------------
# 2. Rust toolchain via rustup (idempotent). Installs into ~/.cargo + ~/.rustup
#    and, by default, wires cargo into your shell profiles for future shells.
# ---------------------------------------------------------------------------
ensure_rust() {
  # Pick up an existing install that just isn't on PATH in this shell.
  if ! have cargo && [ -f "$HOME/.cargo/env" ]; then
    # shellcheck disable=SC1091
    source "$HOME/.cargo/env"
  fi

  if have cargo; then
    log "rust present — $(cargo --version)"
  else
    log "installing Rust via rustup (into ~/.cargo, no sudo)..."
    if have curl; then
      curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    elif have wget; then
      wget -qO- https://sh.rustup.rs | sh -s -- -y
    else
      err "need curl or wget to fetch rustup."
      exit 1
    fi
    # shellcheck disable=SC1091
    source "$HOME/.cargo/env"
    have cargo || { err "cargo still not available after rustup install."; exit 1; }
    log "installed — $(cargo --version)"
  fi

  # clippy (enforced clean on asmux) + rustfmt are part of the dev flow.
  if have rustup; then
    local missing=()
    rustup component list --installed 2>/dev/null | grep -q '^clippy'  || missing+=(clippy)
    rustup component list --installed 2>/dev/null | grep -q '^rustfmt' || missing+=(rustfmt)
    if [ "${#missing[@]}" -gt 0 ]; then
      log "adding rustup components: ${missing[*]}"
      rustup component add "${missing[@]}" || warn "could not add ${missing[*]} (non-fatal)"
    fi
  fi
}

# ---------------------------------------------------------------------------
# 3. Build the Rust workspace (asm-daemon + asmux + asm-relay).
# ---------------------------------------------------------------------------
build_rust() {
  if [ "${ASM_NO_BUILD:-0}" = "1" ]; then
    log "ASM_NO_BUILD=1 — skipping cargo build"
    return 0
  fi
  local profile=debug
  if [ "${RELEASE:-0}" = "1" ] || [ "${PROFILE:-}" = release ]; then profile=release; fi
  log "building workspace ($profile) — this can take a few minutes the first time..."
  if [ "$profile" = release ]; then
    ( cd "$ROOT" && cargo build --release )
  else
    ( cd "$ROOT" && cargo build )
  fi
  log "build ok — binaries under target/$profile (asm-daemon, asmux, asm-relay)"
}

# ---------------------------------------------------------------------------
# 4. Web client deps (optional). Node is only needed for the browser UI and the
#    .mjs test scripts — the daemon itself runs without it, so a missing Node is
#    a warning, not a failure.
# ---------------------------------------------------------------------------
setup_client() {
  if [ "${ASM_NO_CLIENT:-0}" = "1" ]; then
    log "ASM_NO_CLIENT=1 — skipping web client"
    return 0
  fi
  if ! have node || ! have npm; then
    warn "Node.js/npm not found — skipping the web client."
    warn "install Node 20+ (e.g. https://github.com/nvm-sh/nvm) then run: cd client && npm install"
    return 0
  fi
  local major
  major="$(node -p 'process.versions.node.split(".")[0]' 2>/dev/null || echo 0)"
  if [ "$major" -lt 20 ] 2>/dev/null; then
    warn "Node $(node -v) detected; the client wants Node 20+. Continuing, but the build may fail."
  fi
  log "installing web client deps (npm install)..."
  ( cd "$ROOT/client" && npm install )
  log "client deps installed — run 'cd client && npm run dev' for the UI"
}

# ---------------------------------------------------------------------------
main() {
  log "bootstrapping Agent Session Manager in $ROOT"
  ensure_system_deps
  ensure_rust
  build_rust
  setup_client

  cat <<EOF

$(log "setup complete.")
Next steps:
  1. This shell doesn't have cargo on PATH yet — run:
         source "\$HOME/.cargo/env"
     (new terminals pick it up automatically.)
  2. Start the durable stack (holder + daemon):
         scripts/start.sh
  3. Check it and grab this host's enrollment token:
         scripts/status.sh
         scripts/token.sh
  4. Web UI (dev):  cd client && npm run dev
EOF
}

main "$@"
