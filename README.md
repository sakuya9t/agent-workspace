# Agent Session Manager

A cross-platform personal tool for managing long-running coding-agent sessions
on remote servers. Start an agent in a workspace, disconnect, reconnect later,
and continue the same live session with no lost terminal output.

See [`docs/`](docs/) for the full requirements, architecture, and MVP plan.

## Status

Runnable MVP core (Alpha Gate). The daemon proves the central product loop:

```
start agent -> disconnect -> agent keeps running -> reconnect -> resume terminal
```

Implemented:

- Rust daemon (`asm-daemon`) with tokio + axum HTTP/WebSocket API.
- SQLite storage (WAL) with a batched terminal-event writer.
- `SessionBackend` trait + built-in native PTY backend (portable-pty) driving a
  headless `vt100` terminal emulator per session.
- Server-side terminal snapshots exported as ANSI repaint streams for
  fresh-client / reconnect resume (no raw-offset replay as the primary path).
- Session lifecycle: create, list, attach, resize, stop, archive, with a
  state machine and structural session summary records on exit.
- Attention signals (activity / likely-blocked / approval-needed / failed).
- Static agent plugin registry: `shell`, `codex`, `claude`, `custom_command`
  (custom commands require explicit approval).
- No silent relaunch: a daemon restart reconciles lingering sessions to
  `failed` rather than pretending they continued.
- Workspace registration + allowlist, guided `git init` for plain folders,
  and per-session Git worktree isolation so concurrent agents on one repo get
  separate working trees; instance cleanup is guarded against dirty/live state.
- Remote connectivity: device enrollment + bearer-token auth with loopback
  trust; the client connects to local, direct-LAN, or SSH-tunnelled daemons.

Next iterations (see `docs/mvp-execution-plan.md`): out-of-process sidecars,
Git worktree isolation + change tracking, the Electron shell, and rich output.

## Layout

```
crates/daemon/   Rust control-plane daemon (asm-daemon)
client/          React 19 + Vite + xterm.js control center (web + Electron later)
docs/            requirements, architecture, MVP execution plan
```

## Running the daemon

```bash
# build + run (listens on 127.0.0.1:4600 by default)
cargo run -p asm-daemon
```

Environment overrides: `ASM_BIND`, `ASM_DATA_DIR`, `ASM_CONFIG_DIR`,
`ASM_RUNTIME_DIR`, `ASM_STATIC_DIR`, `ASM_LOG`.

### HTTP API

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
| POST | `/api/sessions/:id/open-vscode` | open the session's instance in VS Code |
| GET (WS) | `/api/sessions/:id/stream` | terminal stream |
| GET | `/api/sessions/:id/scm/status` | repo status, branch, changed files |
| GET | `/api/sessions/:id/scm/diff?path=&untracked=` | unified diff for a file |
| GET | `/api/sessions/:id/scm/log?limit=` | commit history |

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
  "approve_custom": false
}
```

## Connectivity & auth

The daemon authenticates by connection origin:

- **Loopback is trusted** — local clients (and SSH-forwarded localhost ports)
  need no token.
- **Off-loopback requires a device token** — a direct LAN/remote client must
  enroll first.

`/health` and the auth bootstrap endpoints are always public; everything else
under `/api` is gated.

### Local

Run the daemon and open the client — same-origin, no setup. The connection
button in the header shows `local`.

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
ASM_BIND=0.0.0.0:4600 asm-daemon      # logs the enrollment token on startup
```

The enrollment token is also visible in the client's **Connect** dialog when
you're connected locally (loopback-only endpoint). In the dialog on the remote
device, enter `http://<host>:4600` plus the enrollment token; the client
receives a device token stored locally for future connections. Revoke devices
via `POST /api/auth/devices/:id/revoke`.

> Direct off-loopback traffic is not TLS-encrypted in the MVP — prefer the SSH
> tunnel for untrusted networks. Relay/gateway modes for NAT'd hosts are on the
> roadmap. Known security gaps and the plan to close them are tracked in
> [`docs/security-followups.md`](docs/security-followups.md).

## Running the client

```bash
cd client
npm install
npm run dev   # Vite dev server, proxies /api and /health to the daemon
```

## Tests

```bash
cargo test                 # daemon unit + mock-backend integration tests
```

The integration tests swap a mock `SessionBackend` for the native PTY backend,
exercising the manager lifecycle (create → stop → summary, approval gate,
archive state machine) without spawning real processes.

## End-to-end smoke test

With a daemon running, exercise the full loop (create → attach → run →
disconnect → reconnect snapshot resume → scm status → stop → summary):

```bash
node scripts/smoke.mjs 127.0.0.1:4600 /path/to/a/git/repo
```
