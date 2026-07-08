# Setup & Operations

Everything you need to build, run, connect to, and test Agent Session Manager. For
the product overview and architecture picture, see the [README](../README.md); for
the design rationale start with [`architecture.md`](architecture.md) and
[`durable-sessions.md`](durable-sessions.md).

## Contents

- [Clean-machine setup](#clean-machine-setup)
- [Running the daemon](#running-the-daemon)
  - [Native (quick start)](#native-quick-start)
  - [Durable sessions (daemon + asmux)](#durable-sessions-daemon--asmux)
- [Running the client](#running-the-client)
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
straight to the sections below.

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
scripts/wizard.sh   # start / restart / stop, and how clients reach this host
```

It asks a few plain questions, then shows the exact `start.sh` /
`restart-daemon.sh` / `stop.sh` command and runs it once you confirm — so you can
learn the flags as you go.

**Or drive the scripts directly** (they build, run both processes in the
background under `$ASM_DATA_DIR/logs`, and manage the lifecycle):

```bash
scripts/start.sh            # build + start the holder and the daemon (sidecar)
scripts/status.sh           # what's running + /health
scripts/restart-daemon.sh   # restart only the daemon — sessions survive (adopt)
scripts/stop.sh             # stop both (stop.sh daemon|asmux for just one)
scripts/token.sh            # print this host's device-enrollment token
```

Override with env, e.g. `ASM_BIND=0.0.0.0:4600 RELEASE=1 scripts/start.sh`. The
rest of this section is the manual equivalent, for when you want the processes in
the foreground or wired into your own supervisor.

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
`ASM_RUNTIME_DIR`, `ASM_STATIC_DIR`, `ASM_LOG`, and for the holder:
`ASM_BACKEND` (`native`|`sidecar`), `ASM_ASMUX_AUTOSPAWN` (`0` disables
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
ASM_STATIC_DIR=/path/to/client/dist scripts/start.sh
```

Set `ASM_STATIC_DIR=` (empty) to disable packaged serving. If you build
`client/dist` while the daemon is already running, `scripts/restart-daemon.sh`
picks it up.

**Dev (live reload).** The Vite dev server proxies `/api` and `/health` to the
daemon — needs Node/npm on your workstation:

```bash
cd client
npm install
npm run dev   # Vite dev server, proxies /api and /health to the daemon
```

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
| POST | `/api/sessions/:id/stop` | stop a live session |
| POST | `/api/sessions/:id/archive` | archive a terminal session |
| POST | `/api/sessions/:id/cleanup?force=` | remove the session's worktree |
| POST | `/api/sessions/:id/resize` | resize (`{rows, cols}`) |
| POST | `/api/sessions/:id/ack` | acknowledge/clear attention |
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

### End-to-end smoke test

With a daemon running, exercise the full loop (create → attach → run →
disconnect → reconnect snapshot resume → scm status → stop → summary):

```bash
node scripts/smoke.mjs 127.0.0.1:4600 /path/to/a/git/repo
```

### Durable-restart test (asmux)

Proves sessions survive a daemon restart. Self-contained — it starts asmux and
two daemon generations itself (no running daemon needed), creates a session,
`SIGTERM`s the daemon, restarts it, and asserts the session was adopted
(`running`, screen reconstructed, still accepts input):

```bash
cargo build && node scripts/durable-restart-test.mjs
```
