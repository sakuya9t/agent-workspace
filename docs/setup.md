# Setup & Operations

Everything you need to build, run, connect to, and test Agent Session Manager. For
the product overview and architecture picture, see the [README](../README.md); for
the design rationale start with [`architecture.md`](architecture.md) and
[`durable-sessions.md`](durable-sessions.md).

## Contents

- [Clean-machine setup](#clean-machine-setup)
- [Windows](#windows)
- [Running the daemon](#running-the-daemon)
  - [Native (quick start)](#native-quick-start)
  - [Durable sessions (daemon + asmux)](#durable-sessions-daemon--asmux)
- [Running the client](#running-the-client)
- [Saved startup config](#saved-startup-config)
- [Connectivity & auth](#connectivity--auth)
- [HTTP API](#http-api)
- [Tests](#tests)

## Clean-machine setup

On a fresh box with nothing but a shell, bootstrap the whole toolchain (a C
compiler + make via your package manager, Rust via rustup into `~/.cargo`, then
a first build) with one idempotent script:

```bash
scripts/setup.sh                  # install prerequisites + debug build
RELEASE=1 scripts/setup.sh        # release build instead
ASM_NO_CLIENT=1 scripts/setup.sh  # skip the web client (npm) step
```

Afterwards run `source "$HOME/.cargo/env"` in your current shell (new shells get
cargo automatically), then `scripts/start.sh`. If you already have Rust, skip
straight to the sections below. On Windows, see [Windows](#windows) instead.

## Windows

Everything runs natively on Windows 10 1809+ / Windows 11 — no WSL. The holder
keeps its sessions in ConPTY pseudoconsoles and the daemon reaches it over a
named pipe, so sessions survive daemon restarts exactly as on Linux/macOS.

**Setup.** From PowerShell, one idempotent script installs what is missing via
winget — Git for Windows (git + Git Bash), Visual Studio 2022 Build Tools (the
MSVC C++ workload Rust links with), rustup, and Node.js LTS — then builds the
workspace and the web client:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\setup.ps1             # prerequisites + debug build
powershell -ExecutionPolicy Bypass -File scripts\setup.ps1 -Release    # release build
powershell -ExecutionPolicy Bypass -File scripts\setup.ps1 -NoClient   # skip Node / the web client
```

The Build Tools, Git and Node installers each raise a UAC prompt. Open a new
terminal afterwards so it sees the updated `PATH`.

**Running.** The service scripts are the same bash scripts as everywhere else,
run under Git Bash. From PowerShell or cmd, `scripts\asm` forwards to them (it
finds Git Bash itself and works under the default script execution policy):

```powershell
scripts\asm start            # = scripts/start.sh  (daemon + holder + Vite UI)
scripts\asm start --no-ui    # every start.sh flag passes through
scripts\asm status
scripts\asm restart-daemon
scripts\asm token
scripts\asm stop
```

From a Git Bash prompt, run `scripts/start.sh` and friends directly. Do not use
a bare `bash scripts/start.sh` from PowerShell: `bash` there can be the WSL
launcher, which would run the scripts inside Linux.

Defaults are the same as on Unix, read through Git Bash's eyes: data in
`%USERPROFILE%\.local\share\asm`, runtime files (pidfiles, `.reg` records) in
`%TEMP%\asm`. The Shell agent is PowerShell (7 if installed, else Windows
PowerShell); use a custom command for Git Bash or cmd sessions.

**How it differs under the hood** (useful when diagnosing):

- *Holder transport.* The socket path (`<runtime_dir>\asmux.sock`) names a
  named pipe, `\\.\pipe\asmux-<hash of the path>`; no file is created. The
  holder's log line shows both. `asmux probe` works as on Unix. A pipe vanishes
  with its owner, so there is no stale-socket state, and the kernel refuses a
  second holder on the same name (`ASMUX_TAKEOVER` cannot apply).
- *Starting and stopping.* `nohup … &` has no Windows equivalent, so the
  scripts start each component through `asm-launch`, which gives it a console
  of its own (no window), its own process group and, where the parent job
  allows, its own job — Ctrl-C in your terminal, or closing it, leaves the
  stack running. Pidfiles hold real Windows pids, checked against the expected
  executable so a recycled pid is never mistaken for ours. There is no
  SIGTERM: `stop.sh` terminates outright, which every component tolerates (on
  the holder backend the daemon's clean exit does nothing extra anyway).
- *Rebuilding while running.* Windows will not overwrite a running `.exe`, but
  will rename one. Before each build the scripts rename any locked binary to
  `*.exe.running-*` (the process carries on from it) and delete leftovers once
  their process has exited — so `restart-daemon.sh` can rebuild the daemon,
  and `start.sh` the holder, while they run.
- *Agent CLIs installed by npm* (`codex`, `opencode`, `pi`) are `.cmd` shims;
  the daemon launches the `node <script>` they wrap directly, so prompts and
  instructions passed as arguments never go through `cmd.exe`'s parser.

## Running the daemon

The daemon runs one of two session backends (see
[`durable-sessions.md`](durable-sessions.md)):

- **`native`** (default) — PTYs run in-process. Simplest to run, but a daemon
  restart loses live sessions (they reconcile to `failed`).
- **`sidecar`** — PTYs are held by a separate **asmux** holder process, so
  sessions **survive a daemon restart**: the daemon re-adopts them on start.

### Native (quick start)

```bash
# build + run (listens on 127.0.0.1:4600 by default)
cargo run -p asm-daemon
```

### Durable sessions (daemon + asmux)

**Easiest — guided wizard.** Don't want to remember flags? Run:

```bash
scripts/wizard.sh   # choose UI, relay/connectivity, then start / restart / stop
```

It asks a few plain questions, then shows the exact `start.sh` /
`restart-daemon.sh` / `stop.sh` command and runs it once you confirm — so you can
learn the flags as you go.

**Or drive the scripts directly** (they build, run both processes in the
background under `$ASM_DATA_DIR/logs`, and manage the lifecycle):

```bash
scripts/start.sh            # holder + daemon + local Vite UI (all detached)
scripts/start.sh --no-ui    # opt out of managed Vite
scripts/status.sh           # what's running + /health
scripts/restart-daemon.sh   # restart only the daemon — sessions survive (adopt)
scripts/stop.sh             # stop all (stop.sh daemon|asmux|ui|relay for one)
scripts/token.sh            # print this host's device-enrollment token
```

Every run starts from the install's **saved startup config**
(`~/.local/share/asm/startup.conf`; see [Saved startup config](#saved-startup-config)):
flags change just the settings they name and are saved back, so a **flagless**
`start.sh` / `restart-daemon.sh` reproduces the last setup — a `0.0.0.0` bind, a
`--register`, a bundled relay, relay-/UI-only-ness, the managed UI, the build
profile — across `stop.sh`, reboots, and SSH sessions, and applies anything
changed since (for example in the web UI's **Config** dialog). The saved file
beats inherited `ASM_*` env (shells inside an asm session inherit the daemon's
own exports); env is only a fallback before the first save, e.g.
`ASM_BIND=0.0.0.0:4600 RELEASE=1 scripts/start.sh` (`RELEASE=1` always selects
the release profile, like `--release`). The rest of this section is the manual
equivalent, for when you want the processes in the foreground or wired into
your own supervisor.

Build both binaries first — `cargo run -p asm-daemon` only builds the asmux
*library*, not the `asmux` holder binary:

```bash
cargo build                       # builds asm-daemon AND asmux
```

Then start the daemon in sidecar mode. It **auto-spawns** the asmux holder (in
its own process group, so the holder outlives the daemon) when the socket is dead:

```bash
ASM_BACKEND=sidecar cargo run -p asm-daemon
```

Or run the holder yourself and point the daemon at it (disable auto-spawn) — this
gives you independent control of each process:

```bash
# terminal 1 — the holder: owns every PTY; keep it running across daemon restarts
./target/debug/asmux

# terminal 2 — the daemon
ASM_BACKEND=sidecar ASM_ASMUX_AUTOSPAWN=0 cargo run -p asm-daemon
```

Both sides find the socket at `<runtime_dir>/asmux.sock` (override with
`ASMUX_SOCK`). Confirm the backend is active:

```bash
curl -s localhost:4600/health      # -> "backend":"asmux-sidecar"
```

#### Restart the daemon — sessions survive

Stop the daemon (`Ctrl-C` / `SIGTERM`) and start it again. On shutdown the
sidecar daemon **detaches** and leaves the children running in asmux; on restart
it reconnects, lists the holder's sessions, and **adopts** the live ones — they
stay `running` with their terminal screen reconstructed. asmux keeps running the
whole time, so the restarted daemon connects to the existing holder rather than
spawning a new one.

```bash
# with the daemon under cargo:  Ctrl-C, then re-run the same command
ASM_BACKEND=sidecar cargo run -p asm-daemon
```

#### Restart asmux — live sessions are lost (by design)

asmux holds the live PTYs, so restarting it kills every child. This is the
holder's failure domain: **a holder restart loses liveness, not history.** There
is no auto-reconnect yet (that lands in M4), so restart the daemon afterward; on
start it finds the old sessions gone from the holder and marks them
**`indeterminate`** — outcome unknown, preserved output still viewable — never a
silent `failed`.

```bash
pkill -TERM asmux                 # terminates all child PTYs; unlinks its socket
./target/debug/asmux &            # fresh, empty holder
# then restart the daemon so it reconnects and reconciles
```

#### Stop everything

The auto-spawned holder is detached and outlives the daemon by design, so stop
both:

```bash
pkill -TERM asm-daemon            # daemon detaches (leaves sessions running)
pkill -TERM asmux                 # then stop the holder
```

Environment overrides: `ASM_BIND`, `ASM_DATA_DIR`, `ASM_CONFIG_DIR`,
`ASM_RUNTIME_DIR`, `ASM_STATIC_DIR`, `ASM_LOG`, `ASM_ENROLLMENT_TOKEN`; the
component switches `ASM_RUN_DAEMON` / `ASM_RUN_RELAY` / `ASM_RUN_UI`; managed
Vite uses `ASM_UI_HOST`, `ASM_UI_PORT`, `ASM_UI_DAEMON`,
`ASM_UI_DAEMON_TOKEN`, and `ASM_UI_ALLOWED_HOSTS` (comma-separated exact
reverse-proxy hostnames) — for the scripts, all of these are fallbacks behind
the saved startup config; and for the holder:
`ASM_BACKEND` (`native`|`sidecar`; unset means `native`, and any other value is
rejected at startup rather than falling back to non-durable native),
`ASM_ASMUX_AUTOSPAWN` (`0` disables
auto-spawn), `ASM_ASMUX_BIN` (explicit holder binary path), `ASMUX_SOCK` (holder
socket path), `ASMUX_MEMORY_LIMIT` (holder ring-memory cap, bytes).

## Running the client

There are two ways to get the browser UI in front of users.

**Packaged (no Node/npm/vite on the serving box).** The daemon serves a
pre-built client itself. Build the bundle once on any machine that has Node
20+, then point the daemon at it:

```bash
cd client && npm install && npm run build   # produces client/dist/
```

`scripts/setup.sh` runs this build for you when Node is present. `scripts/start.sh`
then auto-serves `client/dist` if it exists (via `ASM_STATIC_DIR`), so the UI is
live at the daemon's own address (`http://<host>:4600`) with **no dev server and
no Node toolchain on the serving host**. On a headless server without Node, copy
a `client/dist/` built elsewhere and start with:

```bash
ASM_STATIC_DIR=/path/to/client/dist scripts/start.sh --no-ui
```

Set `ASM_STATIC_DIR=` (empty) to disable packaged serving. If you build
`client/dist` while the daemon is already running, `scripts/restart-daemon.sh`
picks it up.

**Dev (live reload, default).** The Vite dev server proxies `/api` and `/health`
to the daemon. The service scripts detach and manage it alongside the daemon,
so it survives an SSH logout (requires Node and `client/node_modules`, which
`scripts/setup.sh` installs when Node is available):

```bash
scripts/start.sh                              # UI on 127.0.0.1:5273
scripts/start.sh --ui-host 0.0.0.0           # expose UI on a trusted LAN
scripts/start.sh --ui-port 8080              # custom port; implies --ui
scripts/start.sh --no-ui                     # stop and disable managed Vite
scripts/status.sh                            # reports daemon, holder, UI, relay
```

`--run-ui=true` and `--run-ui=false` are accepted as explicit boolean aliases.
The selection is saved, so later flagless `start.sh` and
`restart-daemon.sh` runs preserve it. UI output goes to
`$ASM_DATA_DIR/logs/asm-ui.log`; `scripts/stop.sh ui` stops only Vite.

**UI-only gateway.** To serve the web client without running a local daemon,
session holder, relay, or agent/SSH layer:

```bash
scripts/start.sh --ui-only
scripts/start.sh --ui-only --ui-daemon http://machine:4600
scripts/start.sh --ui-only --ui-daemon http://machine:4600 \
  --ui-daemon-token DEVICE_BEARER_TOKEN --ui-host 0.0.0.0

# Permit an HTTPS reverse proxy such as Tailscale Serve to retain its public
# Host header. The option is repeatable and is persisted with the UI config.
scripts/start.sh --ui-allowed-host raspberrypi.example-tailnet.ts.net
```

`--ui-only` turns the daemon and relay off (any other combination works too —
see [Saved startup config](#saved-startup-config)). Without `--ui-daemon` (or
after `--no-ui-daemon`), the client shell still starts and users can add
daemons or relays from **manage**. With a target,
same-origin HTTP and WebSocket traffic is proxied to that daemon. Off-host
daemons normally require an enrolled device bearer token; it is saved in
`startup.conf` (owner-only, `0600`) and injected by Vite, never sent to browser
storage, placed in the WebSocket URL, or read back by the Config dialog.

Vite rejects unfamiliar `Host` headers to prevent DNS-rebinding attacks. When
the gateway sits behind an HTTPS reverse proxy, add each exact public hostname
with `--ui-allowed-host`. This option is additive: it keeps the saved list
(and the UI-only daemon URL and bearer token), restarts the UI if needed, and is
retained by later flagless `start.sh` runs. Do not use a broad wildcard or
disable Vite's host check.

Remote daemon and relay URLs added through **manage** are also carried through
the managed UI host. This matters when opening the UI through Tailscale: a URL
such as `http://192.168.1.25:4600` names an address on the UI node's LAN, not on
the phone's current LAN, and an HTTPS Tailscale page cannot fetch that HTTP URL
directly anyway. The browser therefore keeps HTTP and terminal WebSocket
traffic on the UI origin, while Vite dials the private address from the
connected node. The gateway accepts only ASM's `/health`, `/api`, relay
`/nodes`, and `/n/<node_id>` routes; it is not a general-purpose HTTP proxy.
As with `--ui-host 0.0.0.0`, expose the managed UI only on a trusted LAN or
through an access-controlled reverse proxy.

You can still run Vite manually in the foreground on a development workstation:

```bash
cd client
npm install
npm run dev   # Vite dev server, proxies /api and /health to the daemon
```

## Saved startup config

Every setting `scripts/start.sh` takes is kept in **one persistent file per
install**, `$ASM_DATA_DIR/startup.conf` (default
`~/.local/share/asm/startup.conf`). It lives in the data dir — not the
tmpfs runtime dir — so it survives reboots, and a second install
(`--data-dir`, a test sandbox) never shares it. It is outside the checkout by
default, and `.gitignore` excludes `startup.conf` / `startup.applied` anywhere
in the tree in case a data dir points into one.

A host runs any non-empty combination of three **components**, each switched
independently (`--daemon`/`--no-daemon`, `--relay`/`--no-relay`,
`--ui`/`--no-ui`, or the Config dialog):

| Component | Default | What it is |
| --- | --- | --- |
| daemon | on | sessions and agents, with the asmux holder that keeps them alive across daemon restarts |
| relay | off | a rendezvous NAT'd nodes and clients dial out to |
| web UI | on | the managed Vite client |

There is no dependency between them — only between settings: relay
registration, gateway downstreams, and the enrollment token belong to the
daemon; a relay needs an access key; the UI proxies to this host's daemon when
it runs, else to the optional `ASM_UI_DAEMON` (or is a bare client shell). So
"UI + relay, no daemon" is as valid as the old UI-only and relay-only setups
(`--ui-only` / `--relay-only` remain as shortcuts). Turning the daemon off
stops it on the next `start.sh` but leaves the holder — and its live sessions —
running; turn the daemon back on and it re-adopts them.

- **Flagless runs use it.** `start.sh` and `restart-daemon.sh` start from the
  file; a flagless run reproduces the last setup and applies anything changed
  since, restarting only what changed (the holder never restarts, so live
  sessions survive). `restart-daemon.sh` applies daemon and UI changes but
  leaves a running relay alone; `start.sh` converges everything, including
  starting, restarting, or stopping the bundled relay.
- **Flags patch it.** A flag changes only the setting it names and is saved
  back: `start.sh --label box` keeps the saved bind, registration, relay, and
  UI. Switches that turn something off: `--no-daemon`, `--no-relay`,
  `--no-ui`, `--no-register`, `--no-ui-daemon`, `--debug`. A daemon setting
  (`--bind`, `--label`, `--register`, `--enrollment-token`) turns the daemon on;
  `--ui-daemon` turns it off (the proxy target is only used without one). A
  command line that both needs and disables a component is refused.
- **Stopping keeps it.** `stop.sh` never touches the file: the next `start.sh`
  brings the configured stack back. `stop.sh ui|relay` also clears that
  component's runtime record, so `restart-daemon.sh` will not revive it — only
  `start.sh` does. To stop running a relay or UI for good, turn it off in the
  Config dialog or pass `--no-relay` / `--no-ui`.
- **Edit it from the browser.** The web client's **config** button (top bar;
  next to **daemons** on phones) opens a dialog listing the UI host (the server
  that served the page) and every connected daemon, including ones reached
  through a relay. Pick a host, edit, save: the file on *that* host changes and
  applies the next time the scripts run there. The keys — the daemon's
  enrollment token, its relay-registration key, the relay's accepted keys, and
  the UI proxy token — can be set, replaced, or cleared there but are
  write-only: the dialog shows whether each is set, never its value
  (`scripts/token.sh` prints the enrollment token on the host). A host that both
  serves the page and runs the daemon it proxies to shares one file and is
  listed once.
- **"Not applied yet."** Each run records what it applied in
  `$ASM_RUNTIME_DIR/startup.applied`; the dialog and `status.sh` list the saved
  settings that differ ("run scripts/start.sh on this host to apply them").
- **Precedence:** flags > saved file > inherited `ASM_*` env > defaults. An
  install started by an older version of the scripts is migrated on its first
  run from the per-boot `.reg` records it left in the runtime dir. (The legacy
  env switches `ASM_UI_ONLY=1` / `ASM_RELAY_ONLY=1` still work before the first
  save.)
- **Secrets stay out of sessions.** The scripts export these settings to what
  they launch; the holder strips the keys and tokens from every agent's
  environment.

The format is the lowest common denominator of its three readers (bash, Rust,
Node): one `KEY=value` per line, `#` comments, no quoting, never `source`d. The
keys are the scripts' environment variables:

| Key | Flag | Meaning (empty = default) |
| --- | --- | --- |
| `ASM_PROFILE` | `--release` / `--debug` | build profile, `debug` (default) or `release` |
| `ASM_RUN_DAEMON` | `--daemon` / `--no-daemon` | run the daemon + holder (`1`) |
| `ASM_BIND` | `--bind` | daemon listen address (`127.0.0.1:4600`) |
| `ASM_NODE_LABEL` | `--label` | node label (hostname) |
| `ASM_ENROLLMENT_TOKEN` | `--enrollment-token` | what new devices enter to enroll, ≥16 chars (the daemon's current, generated one); applied to its database at startup |
| `ASM_RELAY_URL` / `ASM_RELAY_KEY` | `--register` / `--relay-key` | relay this daemon registers to, and its key |
| `ASM_RELAY_DOWNSTREAMS` | — | gateway-mode downstreams, `host:port,…` |
| `ASM_RUN_RELAY` | `--relay` / `--no-relay` | run a bundled relay (`0`) |
| `ASM_RELAY_BIND` / `ASM_RELAY_KEYS` | `--relay-bind` / `--relay-key` | bundled relay address (`0.0.0.0:4700`) and accepted keys |
| `ASM_RUN_UI` | `--ui` / `--no-ui` | run the managed Vite UI (`1`) |
| `ASM_UI_HOST` / `ASM_UI_PORT` | `--ui-host` / `--ui-port` | Vite listen host (`127.0.0.1`) and port (`5273`) |
| `ASM_UI_ALLOWED_HOSTS` | `--ui-allowed-host` | exact reverse-proxy hostnames, comma-separated |
| `ASM_UI_DAEMON` / `ASM_UI_DAEMON_TOKEN` | `--ui-daemon` / `--ui-daemon-token` | the UI's proxy target and bearer token when this host runs no daemon |

The file is written owner-only (`0600`). A hand edit is validated on the next
run: an invalid value stops `start.sh` with the key named (while `stop.sh` and
`status.sh` keep working); unknown keys are warned about and dropped on the next
save. The same rules are implemented in `scripts/_asm_common.sh`,
`crates/daemon/src/startup_config.rs`, and `client/src/startupConfig.ts`.

## Connectivity & auth

The daemon authenticates by connection origin:

- **Loopback is trusted** — local clients (and SSH-forwarded localhost ports)
  need no token.
- **Off-loopback requires a device token** — a direct LAN/remote client must
  enroll first.

`/health` and the auth bootstrap endpoints are always public; everything else
under `/api` is gated.

The client can hold **several daemons at once** — the left panel shows one host
node per daemon, each with its own workspaces and sessions. Manage them from the
header's **manage** button (the local daemon is always present).

### Local

Run the daemon and open the client — same-origin, no setup. It appears as
"This machine" in the tree.

### Remote via SSH local port-forward (recommended for private hosts)

Keep the remote daemon bound to loopback (the default) and tunnel to it:

```bash
ssh -L 4600:127.0.0.1:4600 user@remote-host
```

Then in the client's **Connect** dialog, use `http://localhost:4600` with **no
enrollment token** — the daemon sees the forwarded connection as loopback and
trusts it, and SSH provides the encryption.

### Remote via direct LAN

Bind the daemon off-loopback and enroll a device:

```bash
ASM_BIND=0.0.0.0:4600 scripts/start.sh   # logs the enrollment token on startup
```

Retrieve the enrollment token in any of these ways:

```bash
scripts/token.sh                          # reads the service scripts' data dir
./target/debug/asm-daemon token           # or the built binary directly
cargo run -q -p asm-daemon -- token       # or via cargo (no binary on PATH)
```

> `asm-daemon` is **not on `PATH`** — it lives at `target/debug/asm-daemon` after
> a build. `token` reads the enrollment token from the SQLite DB under
> `ASM_DATA_DIR`, so run it with the **same** `ASM_DATA_DIR` as the daemon (the
> service scripts do this for you). To get `asm-daemon` on your `PATH`, install
> it with `cargo install --path crates/daemon`.

It's also logged on startup and shown in the client's **Connect** dialog when
you're connected locally (a loopback-only endpoint). On the remote device,
enter `http://<host>:4600` plus the enrollment token in the Connect dialog; the
client receives a device token stored locally for future connections. Revoke
devices via `POST /api/auth/devices/:id/revoke`.

### Remote via relay (NAT'd hosts, no tunnels)

When a host can't accept inbound connections (behind NAT/CGNAT, no port-forward,
no SSH), it **dials out** to a relay — a rendezvous box both sides can reach. The
relay multiplexes the client's plain HTTP(S)/WSS to the node over that outbound
connection, so the client needs **no tunnel** and this works from any device,
including mobile. All three parties — relay host, node, and client — share one
**relay access key**.

**1. On the relay host** (a box with a reachable address), bundle a relay
alongside its own daemon:

```bash
scripts/start.sh --bind 0.0.0.0:4600 --relay --relay-key meow
# [asm] relay  — http://0.0.0.0:4700 (nodes register here)
```

The relay listens on `0.0.0.0:4700` by default (`--relay-bind ADDR` to change it).

**2. On the NAT'd node**, register outbound to that relay:

```bash
scripts/start.sh --register ws://<relay-host>:4700 --relay-key meow
```

> If a daemon is already running, `start.sh` compares this registration against
> what it booted with and, when it differs, restarts the daemon to apply the
> change — the asmux holder stays up, so live sessions survive (you'll see
> `config changed, restarting to apply it`). An unchanged re-run stays a no-op.
> `scripts/restart-daemon.sh --register … --relay-key …` forces the same restart
> explicitly.
>
> Confirm it registered: the node's daemon log
> (`~/.local/share/asm/logs/asm-daemon.log`) shows `registered control stream
> with relay`, or on the relay host
> `curl 'http://127.0.0.1:4700/nodes?relay_key=meow'` lists the node.

**3. In the client**, open the header's **manage** dialog → **Relays** → add the
relay:

- **URL** `http://<relay-host>:4700` — HTTP, not `ws://` (the client speaks
  HTTP/WSS to the relay; the URL is stored with scheme + host, no path)
- **Key** `meow`

The client polls the relay's `/nodes` and lists each registered node; enter that
node's enrollment token and **Connect** to enroll a device through the relay. The
node then appears in the tree like any other daemon (reached via `/n/<node_id>`).
A wrong key shows the relay as **unreachable** rather than as an empty list.

The one key must match across all three — the relay host's `--relay-key`, the
node's `--relay-key`, and the client's relay entry. A mismatched node is rejected
at registration; a mismatched client entry reads as unreachable.

> Neither direct off-loopback traffic nor the relay hop is TLS-encrypted in the
> MVP — prefer the SSH tunnel on untrusted networks, and keep the relay on a
> trusted network for now. Known security gaps and the plan to close them are
> tracked in [`security-followups.md`](security-followups.md).

## HTTP API

| Method | Path | Purpose |
| --- | --- | --- |
| GET | `/health` | version, hostname, platform, uptime, backend, active sessions |
| GET | `/api/auth/status` | server id + auth policy (public) |
| POST | `/api/auth/enroll` | exchange enrollment token for a device token (public) |
| GET | `/api/auth/enrollment-token` | reveal enrollment token (loopback only) |
| GET | `/api/auth/devices` | list enrolled devices |
| POST | `/api/auth/devices/:id/revoke` | revoke a device |
| GET | `/api/config` | this host's saved startup config (`startup.conf`); secrets reported only as set/unset, plus keys saved but not yet applied |
| PUT | `/api/config` | change it (`{revision, values}`; a secret key left out keeps its value, `""` clears) — `409` if the file changed since `revision`, `400` on an invalid setting |
| GET | `/api/fs/list?path=&show_hidden=` | browse host directories (for the picker) |
| GET | `/api/plugins` | list agent plugins + binary detection |
| GET | `/api/workspaces` | list registered workspaces |
| POST | `/api/workspaces` | register a workspace (`{name, root_path}`) |
| POST | `/api/workspaces/:id/init-git` | guided `git init` for a plain folder |
| GET | `/api/workspaces/:id/branches` | local branches + current HEAD (for the branch picker) |
| GET | `/api/sessions` | list sessions |
| POST | `/api/sessions` | create a session |
| GET | `/api/sessions/:id` | session detail |
| GET | `/api/sessions/:id/summary` | structural summary record |
| GET | `/api/sessions/:id/workspace` | this session's isolated instance |
| POST | `/api/sessions/:id/stop?force=` | stop a live session — `409` while the agent is working or blocked (a turn is in flight) unless `force=true` |
| POST | `/api/sessions/:id/archive` | archive a terminal session |
| POST | `/api/sessions/:id/cleanup?force=` | remove the session's worktree |
| POST | `/api/sessions/:id/resize` | resize (`{rows, cols}`) |
| POST | `/api/sessions/:id/ack` | acknowledge/clear attention |
| GET | `/api/sessions/:id/deck` | structured, button-addressable approval prompt |
| POST | `/api/sessions/:id/deck/respond` | revision-checked approval option response |
| GET | `/api/sessions/:id/vscode-target` | path/user/host for the client's `vscode://` deep link |
| GET (WS) | `/api/sessions/:id/stream` | terminal stream |
| GET | `/api/sessions/:id/scm/status` | repo status, branch, changed files |
| GET | `/api/sessions/:id/scm/diff?path=&untracked=` | unified diff for a file |
| GET | `/api/sessions/:id/scm/log?limit=` | commit history |
| GET | `/api/sessions/:id/scm/branches` | local branches + current HEAD |
| POST | `/api/sessions/:id/scm/pull` | fast-forward-only pull for the session branch |
| POST | `/api/sessions/:id/scm/rebase` | rebase the session branch onto a target branch |
| POST | `/api/sessions/:id/scm/merge` | merge the session branch into a target branch |

WebSocket protocol: the server sends binary frames of terminal output (the
first frame is the snapshot repaint). The client sends terminal input as binary
frames or as JSON control frames: `{"t":"i","d":"..."}` (input) and
`{"t":"r","rows":R,"cols":C}` (resize).

Create-session body:

```json
{
  "agent_plugin_id": "shell",
  "cwd": "/absolute/path",
  "command": null,
  "args": [],
  "env": {},
  "rows": 24,
  "cols": 80,
  "approve_custom": false,
  "workspace_id": null,
  "branch": null,
  "create_branch": false,
  "base_ref": null
}
```

For a Git workspace, the isolated worktree's branch is chosen with `branch` +
`create_branch`: omit `branch` to auto-generate an `asm-session/<id>` branch off
HEAD (the default); set `branch` with `create_branch: true` to create it off
`base_ref` (defaults to HEAD); or set `branch` with `create_branch: false` to
check out an existing branch. `direct_checkout: true` runs in the source
checkout with no worktree.

## Tests

```bash
cargo test                 # daemon unit + mock-backend integration tests
```

The integration tests swap a mock `SessionBackend` for the native PTY backend,
exercising the manager lifecycle (create → stop → summary, approval gate,
archive state machine) without spawning real processes.

### The e2e scripts are sandboxed — never point them at your daemon

Every script in `scripts/*.mjs` spawns **its own** daemon in a fresh tmpdir, on a
kernel-assigned free port, with its own data dir and its own asmux socket. Just
run them; nothing needs to be up first:

```bash
cargo build
node scripts/smoke.mjs                  # core loop: create → attach → run → reconnect → stop
node scripts/durable-restart-test.mjs   # sessions survive a daemon restart (asmux adopt)
node scripts/holder-theft-test.mjs      # the 2026-07-12 incident, as a regression test
node scripts/worktree-test.mjs          # per-session git worktrees + cleanup guards
node scripts/paste-test.mjs             # image paste into the session cwd
node scripts/relay-test.mjs             # a NAT'd daemon driven through the relay
node scripts/gateway-test.mjs           # an egress-less downstream through a gateway
node scripts/termscroll-test.mjs        # attach scrollback / alt-screen
node scripts/startup-config-test.mjs    # saved startup config through the real start/restart/stop scripts
```

The headless-Chrome tests drive the real client bundle, so build it once
(`cd client && npm run build`); they then spawn their own daemon *and* their own
Chrome:

```bash
node scripts/attach-button-test.mjs        # 📎 button → upload → path injected into the PTY
node scripts/mobile-shell-test.mjs         # mobile adaptive shell at a phone viewport
node scripts/copy-paste-test.mjs           # copy/paste personas (T1 + T2)
node scripts/archive-kickout-test.mjs      # archiving kicks every client out of the session view
node scripts/confirm-destructive-test.mjs  # stop / archive ask before they act
node scripts/stop-protection-test.mjs      # working/blocked sessions refuse an unforced stop
node scripts/config-ui-test.mjs            # Config dialog over a real Vite + 2 daemons (needs client/node_modules, not dist)
```

`copy-paste`'s **T3** needs a genuinely insecure origin (a LAN IP), which a
sandbox cannot fabricate, so it is opt-in and *skips* rather than silently
passing. It prints the vite command to run, then:
`node scripts/copy-paste-test.mjs http://<LAN-IP>:5199/`

Isolation comes from `scripts/lib/testenv.mjs` — take a sandbox from
`createSandbox()`, never hand-roll the child env:

```js
import { createSandbox, checker, sleep } from "./lib/testenv.mjs";
const sb = await createSandbox("my-test");
await sb.startDaemon();            // sb.base, sb.http, sb.api(), sb.ws(id), sb.cwd
```

**Why this is not optional.** This dev host exports `ASMUX_SOCK`,
`ASM_RUNTIME_DIR`, `ASM_DATA_DIR` and `ASM_BIND` globally, and the daemon
resolves `ASMUX_SOCK` *before* falling back to `ASM_RUNTIME_DIR`. A test that
spreads `...process.env` into the daemon it spawns therefore inherits the **live
holder's socket** even if it set a private runtime dir — and asmux used to
`remove_file` + rebind that path unconditionally. On **2026-07-12** exactly that
happened: an e2e run unlinked the real holder's socket, the holder kept running
but became unreachable, the next daemon restart could not find it, and **six live
sessions were lost**. A private TCP port does not protect you; the collision is on
the socket and the data dir.

Three defences now exist, and you want all of them:

- `createSandbox()` / `hermeticChildEnv()` strip every inherited `ASM_*` / `ASMUX_*`,
  repoint `XDG_RUNTIME_DIR` into the tmpdir, pin `ASMUX_SOCK` explicitly, and throw
  if a resolved path ever escapes the sandbox.
- **asmux refuses to displace a live holder**: it probes the socket before
  unlinking it and exits non-zero if anyone answers (`ASMUX_TAKEOVER=1` overrides,
  deliberately). See `crates/asmux/src/socket.rs`.
- **asmux heals an unlinked socket**: a watchdog notices its path was removed or
  replaced and rebinds it, so the holder stays reachable and its PTYs survive.
  `scripts/holder-theft-test.mjs` replays the whole incident and asserts that no
  sessions are lost.

### Diagnosing the holder

A holder's *pid* being alive means nothing — during the outage asmux was alive
the whole time while its socket was gone, and `status.sh` cheerfully reported
"RUNNING". What matters is whether it **answers**:

```bash
./target/debug/asmux probe   # Live (exit 0) | Stale | Free (exit 1)
scripts/status.sh            # reports ORPHANED when the pid is alive but nothing answers
```

An **orphaned** holder is alive, still holding live PTYs, and unreachable. Its
sessions cannot be attached, and killing it loses them — so `start.sh` waits for
the self-heal, and only then tells you what it found rather than quietly killing
anything.

If the daemon starts before the holder (peer containers, service ordering), it
now **waits** for it — `ASM_ASMUX_WAIT_MS` (default 15000) — instead of dying on
the first refused connect, and logs at ERROR when it truly gives up.

### Diagnosing a stale "in use" badge

The badge is driven by whether the daemon holds an open `/stream` WebSocket for
that session — nothing is cached client-side, and the session list is polled
every 1.5s, so a badge that lingers means the *attachment* lingered. The daemon
logs every attachment's whole life at DEBUG (on by default —
`info,asm_daemon=debug`), in `~/.local/share/asm/logs/asm-daemon.log`:

```bash
grep 'ws attach\|ws release\|ws recv error' ~/.local/share/asm/logs/asm-daemon.log
```

```text
ws attach  session=46ca420b… conn=1 took_over=false
ws release session=46ca420b… conn=1 reason="client close frame" held_ms=601 idle_ms=0
```

`reason` is what tells you where a stale badge came from:

| `reason` | what happened |
| --- | --- |
| `client close frame` | the client said goodbye — the normal path, releases instantly |
| `client stream ended` | the socket ended with no close frame (client killed, network dropped it) |
| `recv error` | the transport failed; the preceding `ws recv error` line carries the cause (e.g. `Connection reset without closing handshake`) |
| `idle timeout` | no frame — not even a pong — for `IDLE_TIMEOUT`; the client vanished silently and the heartbeat reaped it ~40–55s later |
| `superseded` | another client took over; the new attach logs `took_over=true` |
| `session output closed` | the session itself ended |

`held_ms` is how long the attachment was held. Read `idle_ms` against it: a
client that sends nothing has no `last_seen` until our first ping draws a pong,
so on an attachment shorter than `PING_INTERVAL` (15s) the two are equal by
construction and `idle_ms` carries no information.

The useful discrimination when a badge outlived a navigation: `client close
frame` with a large `held_ms` means the goodbye arrived late (a network
problem), while `recv error` or `client stream ended` means it never arrived and
the socket died some other way.

### Running against a real daemon (attended)

Some scripts accept a `host:port` to smoke an already-running daemon. This is
opt-in and prints a warning, because every session it creates lands in *that*
daemon's data dir — and `worktree-test` will create and force-remove worktrees in
the repo you give it:

```bash
node scripts/smoke.mjs 127.0.0.1:4600 /path/to/a/git/repo   # ATTENDED
```
