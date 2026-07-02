# Durable Sessions: Adopt-on-Restart (Design)

Status: **proposed / awaiting approval.** No code yet. This captures the design
for letting live sessions survive a daemon restart and be re-adopted, instead
of being reconciled to `failed`.

## Goal

> On daemon start, pick up stale sessions instead of killing them. Only kill
> (reconcile) the ones that genuinely can't be picked up.

## Why the current backend can't do this

The MVP backend (`backend/native.rs`) runs each PTY **in-process**: the child
process, its `vt100` screen state, the reader thread, and the output broadcast
all live inside the daemon process. When the daemon exits, those children are
its own children and die with it (graceful shutdown now kills them explicitly;
a crash sends SIGHUP as the master fds close). So on the next start there is
nothing left running to adopt — `reconcile_orphans_on_startup` marks lingering
`running`/`starting` rows `failed` because the processes are already gone, not
by choice.

**Re-adoption requires the session's process to outlive the daemon.** That is
the documented "out-of-process sidecar" durability gap.

## Chosen mechanism: tmux backend (Unix MVP)

Each session runs in a detached tmux session named `asm-<session_id>`. tmux's
own long-lived server keeps it alive across daemon restarts. tmux 3.3a is
present on the dev host.

Mapping onto the existing `SessionBackend` / `BackendSession` traits:

| Operation | tmux implementation |
| --- | --- |
| spawn | `tmux new-session -d -s asm-<id> -x <cols> -y <rows> -- <cmd> <args…>` (with `cwd`/`env`) |
| input | `tmux send-keys` for control, or write to a pane pipe for raw bytes |
| resize | `tmux resize-window -t asm-<id> -x <cols> -y <rows>` |
| stop | `tmux kill-session -t asm-<id>` |
| live output | `tmux pipe-pane -o -t asm-<id> 'cat >> <fifo>'` → reader thread feeds the event log + broadcast (same path as today) |
| snapshot / adopt | `tmux capture-pane -p -e -t asm-<id>` rebuilds the `vt100` screen |
| liveness | `tmux has-session -t asm-<id>` + pane `#{pane_dead}` / `#{pane_pid}` |

The reader-thread → `vt100` parser → event-writer → broadcast pipeline stays
intact; only the source of bytes changes from an owned PTY to a tmux
`pipe-pane` fifo.

## Startup state machine (the actual ask)

```
live_rows   = DB sessions in ('starting','running')
tmux_named  = `tmux list-sessions` filtered to the `asm-` prefix

for row in live_rows:
    name = "asm-" + row.id
    if name in tmux_named and pane process alive:
        ADOPT: rebuild snapshot via capture-pane, wire pipe-pane, register live
    else:
        RECONCILE: mark exited (clean pane exit) or failed (vanished)

for sess in tmux_named:
    if sess has no owning DB row:
        KILL: `tmux kill-session` (orphan cleanup — the "no leak" guarantee)
```

This is exactly "adopt what we can, kill only what we can't," and the final
loop is what keeps the earlier *no-leaked-tmux-sessions* property: a leak is
redefined as a tmux session the daemon has no record of, reaped on startup.

## What this changes

1. **Trait:** add an adoption entry point, e.g.
   `SessionBackend::adopt(session_id, geometry) -> Result<Option<Arc<dyn BackendSession>>>`
   returning `None` when nothing adoptable exists. `create` is unchanged.
2. **Shutdown reverses.** Durability means graceful shutdown should **detach
   and leave tmux sessions running** so they can be re-adopted — the opposite
   of last turn's kill-on-shutdown. The "no leak" property moves entirely to
   the startup adopt/reap pass. (Kill-on-shutdown remains correct for the
   in-process `native` backend; it becomes backend-specific.)
3. **DB:** store the backend handle (tmux session name) per session for
   matching — schema v4 migration (`sessions.backend_handle TEXT`).
4. **Startup:** `reconcile_orphans_on_startup` is replaced by an
   `adopt_or_reconcile` pass that runs the backend's adoption first and only
   reconciles the remainder.
5. **Backend selection:** config chooses `native` (default today) vs `tmux`
   (`ASM_BACKEND=tmux`, or auto-detect tmux on Unix). Keep `native` as a
   fallback where tmux is absent.

## Cross-platform

tmux is Unix-only. On Windows the same trait needs a different durable
mechanism (a custom sidecar process over a named pipe, or a ConPTY-based
helper). tmux is the right **MVP** durability path for Linux/macOS; the Windows
sidecar is a later item behind the same `adopt` trait method.

## Verification plan

- Unit: startup pass with a mock backend exposing `adopt` — adopt a live row,
  reconcile a vanished one, kill an orphan.
- Integration (real tmux): start daemon → start an agent that prints a marker
  and keeps running → `SIGTERM` the daemon (sessions detach, stay alive) →
  restart daemon → assert the session is `running`, re-attaches, and the
  terminal snapshot still shows the marker. Confirm an `asm-*` tmux session
  with no DB row is killed on start.

## Open decisions

- Backend for this iteration: **tmux** (recommended) vs custom sidecar.
- Make tmux the default on Unix, or opt-in via `ASM_BACKEND`?
- Raw-input fidelity: `send-keys` vs a pane input pipe (affects control chars,
  bracketed paste). Leaning toward a pane pipe for byte-accuracy.
