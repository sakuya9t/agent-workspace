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
| GET | `/health` | version, platform, uptime, backend, active sessions |
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
