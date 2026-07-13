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

Override with flags, e.g. `scripts/start.sh --bind 0.0.0.0:4600`
(`RELEASE=1` / the `ASM_*` env still work as fallbacks). Once
a component has launched, its settings are recorded (`asm-daemon.reg` /
`asm-relay.reg` in the runtime dir): a **flagless** `start.sh` /
`restart-daemon.sh` keeps those recorded settings — including a `0.0.0.0` bind, a
`--register`, relay-only-ness, the relay's TLS cert, and the plaintext-relay
acknowledgement — rather than reverting to defaults, and the
recording beats inherited `ASM_*` env (shells inside an asm session inherit the
daemon's own exports). Pass flags to actually change settings; an explicit
`stop.sh` clears the component's recording. The rest of this section is the
manual equivalent, for when you want the processes in the foreground or wired
into your own supervisor.

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

`ASM_RELAY_URL` accepts `https://`/`http://` too and translates them to
`wss://`/`ws://` — the daemon dials the relay over WebSocket, but the URL you
have to hand is usually the one the browser uses.

Transport: `ASM_TLS_CERT` / `ASM_TLS_KEY` (PEM chain + key — set both and the
daemon serves **https/wss** on `ASM_BIND`, which is what makes a direct
`https://host:4600` client safe), `ASM_TRUST_LOOPBACK=0` / `--no-loopback-trust`
(require a token even from loopback — **mandatory** behind a same-host reverse
proxy, whose requests would otherwise arrive pre-trusted), `ASM_RELAY_URL` (`wss://…`), `ASM_RELAY_KEY`,
`ASM_RELAY_CA` (PEM
anchors for a self-hosted relay with a private or self-signed cert), and the one
acknowledgement the daemon requires before it will register to a **plaintext
relay on a remote host**: `ASM_ALLOW_INSECURE_RELAY=1`. (An off-loopback
`ASM_BIND` needs no such flag — it is plaintext too, but choosing it is the
acknowledgement; the daemon warns at startup.) The service scripts expose these
as flags — `--tls-cert` / `--tls-key`, `--register`, `--relay-ca`,
`--insecure-relay`, and `--relay-tls-cert` / `--relay-tls-key` for a relay you
run yourself — and record them, so a flagless restart keeps them.
`scripts/wizard.sh` asks in plain language.

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

Bind the daemon off-loopback and enroll a device. Give it a certificate and the
whole channel is encrypted — clients then use `https://<host>:4600`:

```bash
scripts/start.sh --bind 0.0.0.0:4600 \
  --tls-cert /etc/asm/cert.pem --tls-key /etc/asm/key.pem
```

Without `--tls-cert`/`--tls-key` it still starts — choosing an off-loopback bind
*is* the acknowledgement — but the channel is **plaintext**: the device token and
every keystroke are readable by anyone on that LAN. The daemon says so at
startup, and the client flags the `http://` URL as unencrypted when you add it.

A LAN host rarely has a public certificate. A **self-signed** one works, with two
caveats: it must be a *leaf* (`CA:FALSE` — see
[`deployment.md`](deployment.md#the-relay-tls-on-a-public-host)), and the browser
will prompt once per host until you trust it. That is still strictly better than
plaintext, and the daemon deliberately does **not** send HSTS, so the prompt stays
click-through-able.

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
```

The headless-Chrome tests drive the real client bundle, so build it once
(`cd client && npm run build`); they then spawn their own daemon *and* their own
Chrome:

```bash
node scripts/attach-button-test.mjs     # 📎 button → upload → path injected into the PTY
node scripts/mobile-shell-test.mjs      # mobile adaptive shell at a phone viewport
node scripts/copy-paste-test.mjs        # copy/paste personas (T1 + T2)
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

### Running against a real daemon (attended)

Some scripts accept a `host:port` to smoke an already-running daemon. This is
opt-in and prints a warning, because every session it creates lands in *that*
daemon's data dir — and `worktree-test` will create and force-remove worktrees in
the repo you give it:

```bash
node scripts/smoke.mjs 127.0.0.1:4600 /path/to/a/git/repo   # ATTENDED
```
