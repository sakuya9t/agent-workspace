# Session recovery

**Status:** designed, not implemented. Backlog row: **REC**.

A session that ended still holds most of its value: a branch with work on it, a
worktree with that work checked out, and a conversation in which an agent
learned the shape of the problem. Today all three are stranded — the only way
forward from a dead session is a new session that knows nothing. This document
specifies **recovery**: continuing a terminated session's work in a fresh
session, on the same branch, in the same worktree, with the old conversation
carried across.

Recovery is explicitly **not** resurrection. The PTY is gone; the child process
is gone. We never pretend otherwise. What we recover is the *place* (branch +
worktree) and the *context* (conversation) — into a new session with a new id.

## Which sessions are recoverable

| Status | Recoverable | Why |
|---|---|---|
| `stopped` (UI: "finished") | **yes** | user ended it deliberately; worktree + branch intact |
| `exited` | **yes** | agent quit on its own; worktree + branch intact |
| `failed` | **yes** | the context leading up to a failure is worth the most, not the least |
| `indeterminate` | **yes** | holder died; worktree + branch intact, outcome unknown (see [durable-sessions.md](durable-sessions.md) → Reconciliation states) |
| `archived` | **no** | `discard_instance` deleted the worktree **and** the branch (`workspaces.rs:398-408`). There is no place to recover *into*. |
| `starting` / `running` | n/a | not terminal; attach to it instead |

The rule is one sentence: **everything but `archived`.** Archive is the act of
saying "I am done with this work" — it is the only irreversible state, and it is
already irreversible in the code.

> **Note.** Archiving does *not* delete `terminal_events`; the byte log survives
> and `read_events_after` would still return it. The 409 at `api/mod.rs:350` is
> a deliberate policy guard, not a consequence of data loss. We keep that policy:
> archived means gone. But be aware the refusal is a choice we could revisit
> without recovering any data — the data is still there.

## Anatomy of a recovery

Three independent problems, solved separately:

1. **Place** — the new session must run on the origin's branch, in the origin's
   worktree.
2. **Continuity** — the new agent must resume the origin's *actual conversation*
   where the provider supports it.
3. **Fallback** — where it doesn't, the new agent gets the origin's transcript as
   a briefing.

(2) and (3) are alternatives; every recovery gets exactly one of them.

### 1. Place: reuse the worktree (already implemented)

`resolve_workspace` already has the short-circuit we need
(`session_manager/workspaces.rs:54-80`). Posting:

```json
{ "workspace_id": "<ws>", "branch": "<origin branch>", "create_branch": false }
```

finds the branch already checked out in a non-main worktree, and **reuses that
worktree in place** — `isolation: "shared"`, no `git worktree add`, no new
directory. The refcount in `count_active_instances_at_path` (`db.rs:353`) means
the origin's instance row and the recovered session's row both count, so
last-one-out cleanup will not pull the directory out from under the survivor.

The origin's branch is on `workspace_instances.branch` for its session id
(`db.get_instance_for_session`).

Degradations, in order:

- **Worktree gone, branch alive** (someone ran `cleanup_instance`, which removes
  the directory but keeps the branch): `BranchSpec::Existing` builds a fresh
  worktree at `worktree_root/<new id>` on the same branch. Same branch, new
  directory. Native resume then breaks for Claude (see below) — the brief takes
  over.
- **Branch gone too**: not recoverable. Refuse, with the reason.
- **Origin had no branch** (`direct` / `plain` / ad-hoc isolation): recovery
  still works — the "place" is just the same `cwd`. Nothing to reuse or refuse.

### 2. Continuity: native resume, per provider

Every agent that keeps a conversation on disk can reload it. The shapes differ
enough that a plugin cannot merely contribute extra argv — **Codex's resume is a
subcommand that must lead argv**, while Claude's and opencode's are flags. So the
plugin owns the whole launch line.

All three providers can **fork** on resume. We always fork. This is an invariant:

> **Recovery never mutates the origin's history.** The recovered session gets its
> own conversation id; the origin's transcript stays byte-identical. This matters
> because the origin's ASM row and transcript live on, and because the same
> session may be recovered more than once.

| Provider | Native id lives in | Resume launch | Stage |
|---|---|---|---|
| **claude** | `sessionId`, both the JSONL filename stem and a field on every line, in `~/.claude/projects/<encoded-cwd>/` — the exact file `claude_transcript_usage` already opens (`usage.rs:109`) | `claude --resume <id> --fork-session` | A |
| **codex** | the UUID in the rollout filename `rollout-<ts>-<uuid>.jsonl`, under `~/.codex/sessions/**` — files `codex_usage` already collects (`usage.rs:403`) | `codex fork <id>` (subcommand leads argv) | A |
| **opencode** | `~/.local/share/opencode/opencode.db` (SQLite) — no per-cwd file to discover | `opencode --session <id> --fork` | C |
| **shell** | no conversation | — | brief only |
| **custom_command** | unknown by definition | — | brief only |

Verified against the installed binaries: `codex fork` accepts
`--dangerously-bypass-approvals-and-sandbox`, and `claude --fork-session` composes
with `--resume`, so the existing danger-flag toggles survive a resume launch on
both.

Opencode is the reason this is a plugin capability and not two `if` branches: it
has native resume, but its id is behind a SQLite read rather than a filesystem
glob. It ships on the brief in Stage A and upgrades to native resume in Stage C
**without the recovery flow changing at all**.

#### The plugin seam

Two methods on `AgentPlugin` (`plugins/mod.rs:57`), both defaulting to "no
capability", exactly like `usage` / `idle_error` / `attention_uses_screen`:

```rust
/// This agent's own conversation id for a session that ran in `cx.cwd` starting
/// at `cx.started_at_ms` — read from the same on-disk transcripts [`usage`]
/// reads. `None` = this agent keeps no resumable conversation.
fn native_session_id(&self, _cx: &UsageContext) -> Option<String> {
    None
}

/// Build the launch for a session continuing `rx`. `None` = no native resume;
/// the caller falls back to the transcript brief. Implementations **fork** —
/// the origin's own history must not be mutated.
fn build_resume(&self, _ctx: &AgentContext, _rx: &ResumeContext) -> Option<Result<LaunchSpec>> {
    None
}
```

```rust
pub struct ResumeContext<'a> {
    /// The origin's agent-native conversation id, if one was ever captured.
    pub native_id: Option<&'a str>,
    /// Transcript brief the daemon wrote into the reused worktree, if any.
    pub brief_path: Option<&'a Path>,
}
```

`native_session_id` reuses `UsageContext` verbatim — the discovery work is already
done and already correct for Claude and Codex.

#### Capture the id while the session is alive, not at recovery time

This is the one place the existing code must not simply be reused as-is.

`usage.rs` re-derives its file match **on every request** from `(cwd, mtime)`,
and that heuristic is fragile in ways that are tolerable for a token counter and
not tolerable for choosing which conversation to resume:

- it has **no identity check** — Claude's match is literally "newest `*.jsonl` in
  the directory", and the `sessionId` in the file is never read;
- two agent sessions in one cwd (normal when worktree isolation is off) collapse
  onto whichever transcript was written last;
- Codex falls back to `files.first()` — *any* newest rollout on the box — when the
  cwd probe misses.

Resuming the wrong conversation is a much worse failure than reporting the wrong
token count. So: **capture the native id once, while the session is alive and the
heuristic is at its most reliable, and persist it.** The monitor already has the
hooks; the first successful `native_session_id` for a session writes
`sessions.agent_session_id` and never re-derives. Recovery then reads a column,
not a filesystem race.

(Persisting it also lets us *verify* rather than guess: for Claude we read the
`sessionId` field out of the JSONL rather than trusting "newest file wins".)

### 3. Fallback: the transcript brief

**Pick the best source available, in this order:**

1. **The agent's own JSONL**, for a provider that keeps one (claude, codex) whose
   native id we captured but whose native *resume* we can't use — e.g. the
   worktree moved, so Claude's cwd-keyed lookup would miss. This is a structured
   conversation: real turns, real tool calls. Render it to Markdown.
2. **The PTY byte log** (`terminal_events`, via `db.read_events_after(id, 0)`) —
   complete and already persisted for every non-archived session. This is the
   only source for a shell or a `custom_command`, and for those it is a *good*
   one (see the alt-screen caveat below for why it is not, for the TUI agents).

The distinction matters and is easy to get wrong: `GET /api/sessions/:id/transcript`
("Save conversation", `b7f071b`) serves the **raw PTY byte stream, ANSI and all**
— it is terminal output, not a conversation. Do not mistake it for a rendered
transcript just because the button says "conversation".

**Do not type the transcript into the new agent's TUI.** A large paste into a TUI
input box is fragile (bracketed-paste limits, chunking, per-agent input caps) —
it works in testing and fails on a long session. Instead:

1. The daemon writes the brief to a file in the reused worktree:
   `.asm/recovered-<origin-id>.txt`.
2. The recovered session launches with a short, fixed-size opening prompt that
   *points* at it: *"A previous session on this branch ended. Its terminal
   transcript is at `<path>`. Read it before continuing."*

Fixed-size input, works for every agent including a plain shell (where it is just
a file to `cat`), and it degrades gracefully — if the agent ignores the pointer,
the context is still sitting in the worktree for the user.

**Format: ANSI-stripped prose** (for the PTY-sourced brief). Same recovery either
way, but the agent doesn't burn context tokens parsing escape sequences and a
human can read it. The raw bytes remain in SQLite regardless. Rendering reuses
`seed_from_cold`
(`sidecar.rs:249-266`) — feed the cold bytes to a `vt100::Parser`, then walk
scrollback taking `.contents()` (plain) instead of the formatted bytes, mirroring
the loop in `repaint_with_history` (`mod.rs:175-181`). Respect the documented
vt100 constraint at `mod.rs:169-174`: above `offset > rows`, only the first
visible row is trustworthy.

**Known weakness, stated plainly.** Alt-screen agents (Claude Code, the Codex TUI)
expose **zero scrollback** to vt100 (`mod.rs:132-134`) — their byte log is a
sequence of full-screen redraws, not a linear conversation. A brief reconstructed
from it is noisy and partial. This is precisely why native resume is the primary
path for those agents and the brief is a fallback, not a peer. The brief is
genuinely good for shells (real scrollback, no alt-screen) and acceptable-but-lossy
for a TUI agent whose native id we failed to capture.

## Schema

`sessions` has no column for either fact. **SCHEMA_V6**:

```sql
ALTER TABLE sessions ADD COLUMN agent_session_id TEXT;    -- native conversation id, captured while live
ALTER TABLE sessions ADD COLUMN recovered_from TEXT;      -- origin session id, NULL for a fresh session
```

`recovered_from` gives the lineage the UI needs ("recovered from …", and a chain
if a recovery is itself recovered) and costs one nullable column.

## Failure modes

| Condition | Behavior |
|---|---|
| origin is `archived` | refuse — no worktree, no branch |
| origin still live | refuse — attach, don't recover |
| branch gone | refuse, naming the branch |
| worktree gone, branch alive | fresh worktree on the branch; native resume degrades to brief for Claude (its transcripts are keyed by cwd) |
| native id never captured | brief |
| agent has no native resume | brief |
| origin has no conversation (shell) | brief (its scrollback is genuinely useful here) |
| brief empty (no `terminal_events`) | recover anyway — place without context beats nothing |

## Staging

- **Stage A — native resume for Claude + Codex.** The trait seam, `SCHEMA_V6`,
  id capture in the monitor, `POST /api/sessions/:id/recover`, and the client
  affordance on a terminal session. Both providers' ids are already discoverable
  by code that exists.
- **Stage B — the brief.** vt100 → prose, the `.asm/recovered-*.txt` file, the
  pointer prompt. Covers shell, `custom_command`, and any Stage-A miss.
- **Stage C — opencode native resume.** Read the id out of `opencode.db`. Pure
  plugin change; the recovery flow is untouched.

Stage A is small: the worktree reuse and the argv passthrough both already work
(`args` flows `CreateSessionBody` → `AgentContext.extra_args` → `cli_launch` →
`BackendSpawnSpec` → argv, `builtin.rs:231`), so most of Stage A is capture,
persistence, and UI rather than new machinery.

## Open questions

- **Recover into a *new* branch?** Recovering onto the origin's branch means the
  new session commits onto the same line of work — usually right. A variant that
  branches off the origin's tip instead would suit "retry this differently".
  Deferred; the branch triple on `CreateSessionRequest` already expresses it if
  we want it.
- **Recovering an `indeterminate` session whose process is secretly still alive.**
  The advisory says it may still be running as an orphan. Recovery would put a
  second agent in the same worktree. M4 Stage C's orphan reconciliation is the
  real fix; until then, the recover affordance on an `indeterminate` session
  should carry the same "check the preserved output first" warning the status
  already carries.
