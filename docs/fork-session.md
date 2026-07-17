# Forking a session

**Status:** implemented. Supersedes most of [`session-recovery.md`](session-recovery.md)
(backlog row **REC**) — see [What this leaves of REC](#what-this-leaves-of-rec).

A session in flight holds three things worth keeping: a branch, a working
directory, and an agent that has learned the shape of the problem. **Forking**
carries all three into a new session — so you can change direction, hand the work
to a different agent, or try a second approach without losing what the first one
figured out.

Recovery — picking a *dead* session back up — is the same operation with a
stopped origin, so one feature covers both.

## The two decisions

Everything below follows from these.

### 1. Where the fork runs

Git will not check one branch out into two worktrees, and two agents editing one
directory overwrite each other. So the fork dialog offers a checkbox:

| | Where it runs | Safe while the origin is live? |
|---|---|---|
| **unchecked** (default) | a **new** `asm-session/<id>` branch, starting at the origin branch's **tip**, in a worktree of its own | **yes** — the two never touch the same files |
| **checked** | the origin's **own** branch, sharing its worktree in place | only once the origin has stopped |

The unchecked case starts at the origin's tip, not at the repo's `HEAD` — you are
forking the *work*, not the repository. (`BranchSpec::Auto` grew a `base` for
this.)

The checked case reuses the branch-already-checked-out path `resolve_workspace`
already had (`isolation: "shared"`, refcounted). When the origin is **stopped**
this is exactly session recovery. When it is **live**, the dialog shows an orange
warning naming the actual hazard — we allow it, because sometimes it is what you
want, but never quietly.

### 2. How the context gets across

Every fork takes exactly one of two paths (`ForkSeed`):

**Native** — the fork keeps the same agent, so the agent reloads *its own*
conversation: `claude --resume <id> --fork-session`, `codex fork <id>`. Full
fidelity, nothing summarized, ~0 ms, no tokens. It **forks** rather than resumes
in place: the origin's transcript stays byte-identical, because the origin lives
on and may be forked again.

**Brief** — anything else (a different agent; an origin whose conversation was
never captured; Claude forking into a *new* worktree, see the asymmetry below).
The daemon writes `.asm/forked-from-<id>.md` into the fork's working directory and
launches the agent pointed at it.

Either way the fork opens, says what it understands, and **waits**. You forked in
order to decide what happens next; it does not silently resume the old task.

#### The per-agent asymmetry that decides it

| | native fork | finds the conversation by | so a fork onto a **new** worktree… |
|---|---|---|---|
| **claude** | `--resume <id> --fork-session` | **cwd** (`~/.claude/projects/<encoded-cwd>/`) | **cannot** resume — falls back to the brief |
| **codex** | `codex fork <id>` | **uuid**, globally (`~/.codex/sessions/**`) | resumes fine |
| **opencode** | — (see follow-ups) | a SQLite db | brief |

That is what `AgentPlugin::native_fork_requires_same_cwd` encodes, and why a
same-agent fork is *not* automatically a native one.

## The brief, and why there is no big LLM call

The document has three parts, cheapest to read first: a prose **handoff brief**, a
deterministic **digest**, then the **full prior conversation**.

The digest is the load-bearing piece, and it is not an LLM summary. It is read
straight out of the agent's own transcript (`plugins/fork.rs`): the user's
requests **verbatim**, the files changed with edit counts, recent commands, and
the agent's last word.

**It is small, and that is the whole point.** Measured over real sessions on this
host:

| | a session's transcript | rendered conversation | **its digest** |
|---|---|---|---|
| typical | 1–8 MB | 16k–90k tokens | **~1–2k tokens** |
| worst seen | 33 MB | 320k tokens | **~4k tokens** |

A conversation's *intent* is small: the requests are short, the file list is
short, and everything between is the agent working — which the fork does not need
to relive. That compactness is what makes everything else possible. The digest
alone is a usable handoff; it fits any context window; and it is small enough that
summarizing it takes **seconds**, not minutes, and cannot silently truncate.

Summarizing the *conversation* instead would mean chunking 300k tokens through a
map-reduce — and Ollama's default context is 4096 tokens, so the naive version
would quietly drop almost all of it and look like it worked.

### The summarizer is optional, and is an agent you already have

`summarize.rs` runs whichever installed agent CLI has a headless mode — preferring
the one the fork is being handed to, so the brief is written by the model that
will read it. Measured, on the digest:

| | | |
|---|---|---|
| `opencode run` | ~6 s | |
| `codex exec -o <file>` | ~12 s | `-o` writes the final message only, with no progress output or token footer |
| `claude -p` | ~23 s | stdout is already clean |

If none is installed, or it fails, or it overruns its 90 s deadline, the fork
proceeds with the digest alone. **Nothing here can fail a fork.**

Three constraints in `summarize.rs`, each found by testing rather than assumed:

* **`cwd` is a throwaway temp directory.** This is a *full agent*, not a chat
  completion — it can call tools and edit files. It never runs in the worktree.
* **stdin is closed.** Given a prompt argument *and* an open stdin, `codex exec`
  blocks forever on stdin ("Reading additional input from stdin…").
* **the prompt forbids it from discussing its own environment.** Left alone, it
  notices its empty temp cwd, realises the paths in the digest do not exist there,
  and writes the fork an alarming paragraph about it.

A local model was tried and rejected: `qwen3:1.7b` took 17.6 s to produce a
summary that was *factually wrong* about the one bug in the session. Ollama Cloud
needs an account and `ollama signin` — an external dependency and per-device
setup, the two constraints that already killed TLS (backlog **SEC-1**).

## Why the agent is pointed at a *file*

The seed prompt is a fixed-size **pointer**, never the brief's text:

> This session is a fork of an earlier session… Read the file
> `.asm/forked-from-<id>.md` first… then summarize where things stand, and stop and
> wait for my instructions.

Pasting a transcript into a TUI hits bracketed-paste and input-length limits (it
works in testing and fails on a long session), and a brief passed in `argv` would
be readable by any process on the box via `/proc`. A pointer is bounded and safe,
and it degrades: if the agent ignores it, the context is still sitting in the
worktree for a human.

Only agents that can be seeded on their launch line get one, and each encodes it
its own way (`seed_prompt_args`): Claude and Codex read a bare positional as the
opening message, but opencode's positional is a *project directory* — a prompt
there is taken as a path and the launch dies with "Failed to change directory
to …", so it goes through `--prompt`. A shell gets nothing: it would *execute* a
trailing argument as a script rather than read it.

## Capturing the conversation id — while the session is alive

`sessions.agent_session_id` (SCHEMA_V7) is written by the monitor, polling every
5 s until it lands, and **never re-derived**.

This is deliberate. The transcript-matching in `plugins/usage.rs` is a heuristic —
Claude's is literally *"the newest `*.jsonl` in this cwd's directory"* — and two
sessions sharing a working directory (normal with worktree isolation off, and
**guaranteed** for a same-branch fork) collapse onto whichever transcript was
written last. Reporting the wrong token count is survivable. *Resuming the wrong
conversation* is not. So we capture early, when this session is the only recent
writer, and read Claude's `sessionId` **field** rather than trusting the filename.

The id stays on the host: the API exposes only `has_agent_conversation`, which is
all a client needs to say whether a fork carries the whole conversation or a
summary of it.

## Surface

* `POST /api/sessions/:id/fork` — `{ agent_plugin_id, same_branch?, options? }`.
  Runs in `spawn_blocking`: it does `git worktree add` and may run an agent for
  tens of seconds, either of which would stall the whole daemon on the async
  runtime.
* Fork button on any non-`archived` session row (live **or** finished — forking a
  finished session is how you pick work back up). Disabled when the host has no
  coding agent to fork into.
* `sessions.forked_from` gives the lineage, including a chain when a fork is
  itself forked.

## Failure modes

| Condition | Behavior |
|---|---|
| origin is `archived` | refuse — `discard_instance` may already have deleted its worktree and branch |
| origin on a detached HEAD, same-branch asked | refuse, naming why — there is no branch to stay on |
| native id never captured | brief |
| Claude forking onto a new worktree | brief (its transcripts are keyed by cwd) |
| no summarizer installed / it fails / it times out | brief with the digest, no prose section |
| origin has no transcript at all (a shell) | brief saying so — a fork with thin context still beats no fork |

## What this leaves of REC

[`session-recovery.md`](session-recovery.md) is the design this grew out of; forking
a **stopped** session onto its **own branch** *is* recovery, and that path is now
implemented. What remains from that document:

* **The PTY-scrollback brief (REC Stage B).** A `shell` or `custom_command` origin
  keeps no transcript, so it currently gets a brief with no digest. Its real
  context is its scrollback (`terminal_events` → vt100 → prose), which we do not
  render yet. Tracked as **FORK-SHELL**.
* **opencode native fork (REC Stage C).** Its conversation id is behind a SQLite
  read. Pure plugin change; the fork flow does not move. Tracked as **FORK-OC**.
* **"Branch gone" is not pre-flighted** — `git worktree add` fails with git's own
  error rather than a checked refusal.

## Follow-ups

* **FORK-SHELL** — digest a shell origin from its PTY scrollback (REC Stage B).
* **FORK-OC** — opencode `native_session_id` + `build_fork` (REC Stage C).
* The fork icon is an inline SVG; the rest of the action set is hand-drawn PNGs.
* `POST /api/sessions` and `/fork` echo the raw session row, without the `branch` /
  `title` / `has_agent_conversation` fields the list projection adds — harmless
  (the client refetches) but an inconsistency.
