# Session copilots and review gates

**Status:** proposed 2026-07-24. No copilot code or schema exists yet.
Backlog rows: **COPILOT-REVIEW** and **COPILOT-GATE**.

A session may have a second agent inspect the main agent's work while it is in
progress. The reviewer may be:

- another model exposed by the same agent plugin,
- a model from a different installed agent plugin, or
- eventually, a review runner reached through another agent platform or host.

The first release is deliberately smaller: any locally installed agent plugin
that implements the review capability, with an optional explicit model. ASM
does not rank models or decide which one is "stronger"; the user assigns a
reviewer the advisory or gatekeeper role.

The central design decision is:

> A copilot is a persistent review policy attached to a session. Each review is
> a bounded, one-shot job over an immutable repository snapshot. It is not a
> second terminal attachment and it never shares the main agent's writable
> worktree.

That fits the current framework while preserving its two load-bearing rules:
there is one active terminal controller per session, and independent agents do
not write into the same workspace instance.

## Product outcome

The feature supports two progressively stronger modes.

| Mode | What it does | Authority |
| --- | --- | --- |
| **Advisory copilot** | Reviews a stable snapshot, records evidence-backed findings, and lets the user send them to the main agent | None; work may continue |
| **Gatekeeper** | Does the same review, then requires a current passing verdict (and configured deterministic checks) before an ASM-owned promotion action | Blocks only the actions ASM owns |

The first useful slice is manual advisory review. Automatic review and gating
come after snapshot correctness, job supervision, and stale-result handling are
proven.

## What already exists

This does not require a second session system.

- `AgentPlugin` already detects locally installed providers, enumerates models,
  and translates a selected model into provider-specific arguments.
- `HeadlessSpec` already runs a full agent non-interactively for fork summaries,
  with closed stdin, captured output, and a deadline.
- `ConflictResolveSpec` already demonstrates a provider-neutral one-shot agent
  that can inspect and edit a real repository.
- Session fork digests provide compact, deterministic goal and conversation
  context across providers.
- Git worktree helpers already create and clean temporary worktrees for branch
  operations.
- The session monitor already knows when an agent turn has settled to idle.
- `.asm/` already stores ignored session-local handoff artifacts.

The review feature should reuse those seams, but not overload either existing
one-shot contract:

- a fork summarizer runs in an empty directory, cannot select a model, and must
  not inspect the repo;
- a conflict resolver runs in the real worktree with permission guardrails
  bypassed and is expected to edit it.

Review needs a third capability: a model-pinned, non-interactive agent in a
disposable but complete repository, with normal guardrails and a structured
result contract.

## Architecture

```text
main session / worktree
        |
        | idle or "Review now"
        v
ReviewCoordinator -- materialize exact Git tree --+--> disposable worktree
        |                                         |          |
        | persist run + fingerprint               |          v
        |                                         |   reviewer plugin/model
        |                                         |          |
        +<--------------- structured findings ----+----------+
        |
        +--> Copilot panel / optional feedback file
        |
        +--> ReviewGate checks exact fingerprint before an ASM promotion
```

There are four distinct objects:

1. **Copilot configuration** — who reviews, in which role, on which trigger, and
   with what review policy.
2. **Review snapshot** — the exact base, HEAD, and dirty working-tree content the
   reviewer was given.
3. **Review run** — one immutable execution and its structured result.
4. **Gate decision** — a fresh applicability check for a particular ASM-owned
   action. A stored verdict alone is never sufficient.

### Why not a second terminal attachment?

The terminal WebSocket has a same-owner single-attacher takeover policy.
Attaching a copilot would evict the user's client, couple review lifetime to a
PTY, and expose only rendered terminal output rather than a stable code
revision. It also would not solve workspace safety.

The reviewer therefore runs as a daemon job. It may use the main session's
digest as context, but it neither attaches to nor resumes that conversation.

### Why not share the worktree?

Even a nominally read-only coding agent is a full process that may invoke tools
or edit files. A shared worktree also changes while the main agent is working,
so a finding could refer to a mixture of two revisions. Prompt instructions or
provider-specific "read only" flags are useful defense in depth, not an
isolation boundary.

Each review instead gets a disposable Git worktree. Any accidental writes stay
there and are discarded with the run.

## Exact review snapshots

Reviewing only `git diff` is insufficient: the copilot could not inspect
unchanged callers, follow types, or run tests. Creating a normal worktree from
HEAD is also insufficient because it omits staged, unstaged, and untracked
changes. The daemon must materialize the exact visible source tree without
touching the user's index.

For a Git-backed session:

1. Resolve and record the review base and current `HEAD`.
2. Create a private temporary index with `GIT_INDEX_FILE`.
3. Seed it with `git read-tree HEAD`, then run `git add -A -- .`. This captures
   tracked changes, deletions, executable bits, symlinks, and non-ignored
   untracked files while leaving the real index untouched.
4. Run the add/write-tree cycle again. If the resulting tree changes, the main
   worktree was moving during capture; retry a bounded number of times and then
   fail with `source_changing` rather than claim a coherent review.
5. Write the tree as an ephemeral commit whose parent is the recorded HEAD,
   retain it under `refs/asm/reviews/<run-id>`, and create a detached temporary
   worktree from it.
6. Record the original porcelain-v2 status separately so the report can still
   describe staged versus unstaged input even though the review tree presents
   the final combined content.
7. Remove the worktree and private ref after the job. A startup sweeper cleans
   leftovers from interrupted runs.

The default review-base policy resolves the session branch's current
spawn/rebase base commit. If a rebase changes that base, the next run uses the
new base and every earlier verdict becomes stale. If a base cannot be
established, enabling the copilot captures the current HEAD as a fixed fallback.
Reviews cover the full session change from the resolved base, including commits
and dirty changes; incremental "since the last review" scope can be added later
but must not be the gatekeeper default.

The fingerprint is a hash of at least:

```text
repository identity
review base object id
HEAD object id
captured tree object id
reviewer plugin id + selected model
review policy and prompt-schema versions
deterministic-check configuration
```

Ignored files and required local setup state are not captured. Submodules are
represented by their gitlink commit. V1 therefore supports Git-backed sessions
only and reports those limits explicitly. The future non-Git path can use the
workspace isolation provider's reflink/copy mechanism, but must produce the
same immutable-snapshot contract before it can support gating.

This plumbing overlaps the planned **MVP-CKPT** temporary-index/checkpoint work.
Implement one reusable `GitTreeSnapshot` primitive rather than two slightly
different ways to capture a dirty tree. A review ref is ephemeral; a user
checkpoint has longer retention and product semantics.

## Reviewer capability and process contract

Add a dedicated plugin capability, conceptually:

```rust
fn reviewer(
    &self,
    ctx: &AgentContext, // includes explicit model
    prompt: &str,
    output: &Path,
) -> Option<ReviewSpec>;
```

`ReviewSpec` is separate from interactive launch and conflict resolution. The
shared one-shot runner owns:

- closed stdin,
- stdout/stderr draining with hard byte caps,
- a configurable deadline,
- process-tree cancellation,
- exit status and duration,
- output-file versus stdout collection, and
- cleanup on every exit path.

The runner uses normal provider guardrails. Review must never silently inherit
the session's "bypass approvals/sandbox" option. The disposable worktree
protects repository correctness, but it is not a host sandbox: a reviewer
process still runs with the daemon user's credentials and can reach whatever
that account and provider sandbox allow. The UI must say this plainly.

The first implementation spike must verify a provider matrix for Codex, Claude
Code, and opencode:

- selected model is actually passed,
- repository reads work non-interactively,
- configured test commands work under normal guardrails,
- stdin closure does not hang the CLI,
- final output can be captured without progress noise, and
- cancellation kills descendants.

A provider that cannot satisfy the contract is not offered as a reviewer even
if it supports interactive sessions or fork summarization.

### Review input

The prompt contains:

- the target session's deterministic digest when its plugin provides one,
- the review base and captured revision,
- a changed-file/status manifest,
- the user's optional review focus,
- the configured review rubric and deterministic-check results, and
- the required output schema.

The full conversation is not sent by default. The digest carries requests and
work context at a bounded cost; the repository supplies implementation
evidence. A user may explicitly include more context in a later release.

The default rubric asks for correctness, regressions, security and data-loss
risk, concurrency/lifecycle defects, missing validation, and test gaps. It asks
for findings, not a rewrite of the change.

Repository content is untrusted input. Comments and files can contain
instructions aimed at the reviewer. The system prompt must state that repository
text is evidence, never authority, and the product must still treat the model
verdict as fallible.

### Review output

Passing requires valid structured output. An unparseable answer or successful
process with no valid result is `failed`, never an implicit pass.

```json
{
  "schema_version": 1,
  "verdict": "changes_requested",
  "summary": "One high-severity reconnect defect needs correction.",
  "findings": [
    {
      "severity": "high",
      "title": "Cursor can rewind after reconnect",
      "path": "crates/daemon/src/backend/sidecar.rs",
      "line": 219,
      "evidence": "The reconnect branch resets ...",
      "recommendation": "Preserve the last acknowledged cursor ...",
      "confidence": "high"
    }
  ],
  "checks_observed": [
    {
      "command": "cargo test -p asm-daemon",
      "status": "passed"
    }
  ]
}
```

Allowed severities are `blocker`, `high`, `medium`, and `low`. Gate policy—not
the reviewer—maps severities to blocking behavior. Findings need a concrete
path/evidence location when one exists. The daemon preserves the raw bounded
answer for diagnostics but uses only schema-validated fields for UI and policy.

Configured deterministic checks are run by the daemon in the same snapshot
worktree with their own deadlines and logs. A reviewer's claim that it ran a
test is useful evidence but does not replace the daemon's recorded check result.

## Copilot roles and triggers

One session may eventually have several copilots. V1 may expose one active
copilot while using ids and tables that do not hard-code that limit.

Configuration includes:

```text
reviewer plugin + model
role: advisory | gatekeeper
trigger: manual | on_idle | before_promotion
review-base policy
focus/rubric
blocking severities
deterministic checks
automatic feedback: off by default
deadline, output and automatic-run budgets
```

### Manual

`Review now` is the first release. If the main agent is actively writing, the
snapshot stability loop either obtains one coherent tree or asks the user to
retry after the turn settles.

### On idle

The existing monitor may emit a cheap `session_settled_idle` event. It must not
run Git or an agent in the monitor callback. A separate `ReviewCoordinator`
debounces the event, computes whether the source fingerprint changed, and
queues at most one review per session.

Triggers are coalesced:

- one active run per session,
- at most one queued rerun for the newest observed revision,
- never review the same fingerprint twice under the same policy unless the user
  explicitly retries a failed run, and
- global concurrency and per-session cost/run budgets.

### Before promotion

A gatekeeper may enqueue a review when an ASM promotion is requested and no
current verdict exists. The action does not wait on an HTTP request for several
minutes: it returns a typed precondition response naming the queued/running
review, and the user retries after it passes. A later UI can combine this into
one progress flow.

## Feeding findings back to the main agent

Every completed report is visible in a `CopilotPanel`. For a durable,
provider-neutral handoff, the daemon may write:

```text
<main-worktree>/.asm/reviews/<review-run-id>.md
```

The daemon first verifies that the destination is untracked and ignored
(`util::asm_dir` normally adds `.asm/` to the repository's private exclude).
If the repository already tracks or collides with that path, delivery stays in
the UI rather than dirtying user code. The file contains the verdict, structured
findings, evidence, checks, and the reviewed fingerprint.

`Send to agent` submits a short pointer such as:

> A copilot reviewed snapshot `<short fingerprint>`. Read
> `.asm/reviews/<id>.md`, verify each finding against the current code, fix the
> valid issues, and report any finding you reject with evidence.

Manual delivery is the safe default. Automatic terminal injection is opt-in and
comes after the advisory MVP because input can interleave with a human draft.
It may occur only when the session is idle, no terminal client is attached, the
source/input revision still matches, and that review has not been delivered
before. It must use a daemon-owned provider prompt-submission helper rather than
pretend to be a second WebSocket attachment.

To prevent model ping-pong, automatic mode has a bounded review/fix cycle. A
reasonable default is three automatically delivered rounds per session segment;
then it pauses for the user. A new review is triggered only after the main
agent actually changes the source fingerprint.

## Gatekeeper semantics

A model becomes a gatekeeper because the user gives its verdict policy
authority, not because ASM recognizes a "strong" model name.

A gate passes only when all are true:

1. the latest successful run used the configured gatekeeper and policy,
2. its verdict and severity set satisfy the policy,
3. all required deterministic checks passed,
4. a freshly captured source fingerprint exactly matches the reviewed
   fingerprint, and
5. the promoted Git object is the exact reviewed object.

The check and promotion need a per-session SCM unit of work. For a push, ASM
uses the run's reviewed `head_oid` explicitly rather than resolving a moving
branch name after the check; the ephemeral snapshot commit is never pushed.
Dirty worktrees are refused for gated promotion: advisory review may cover dirty
code, but Git cannot push that code until it is committed, and the commit changes
the fingerprint.

The first enforceable action should be **ASM-owned push**. A later merge gate
must review a materialized merge candidate identified by both source and target
oids, then update the target only if neither changed. Rebase and pull transform
the reviewed revision; they invalidate the verdict and require a new review
rather than being treated as promotion.

Every block and explicit user override is recorded with action, fingerprint,
reason, and time.

### Honest enforcement boundary

ASM cannot stop the main agent or user from running `git commit`, `git push
--no-verify`, or another mutation directly in the terminal. Therefore:

- V1 gatekeeping covers only actions invoked through ASM.
- A local Git hook can improve ergonomics but is not a security boundary.
- Universal enforcement requires an external boundary the terminal user cannot
  bypass, such as protected branches and a required remote status/check.

The UI must call the local mode "Gate ASM actions", not imply that the
repository itself is protected. A future policy adapter may publish the review
fingerprint and verdict to a remote forge's required-check API.

## Persistence and state

Suggested tables:

```text
session_copilots
  id, session_id, reviewer_plugin_id, reviewer_model
  role, trigger, base_mode, fallback_base_oid, focus, policy_json
  auto_deliver, enabled, created_at, updated_at

review_runs
  id, copilot_id, session_id
  base_oid, head_oid, tree_oid, fingerprint, prompt_version
  status, verdict, summary, raw_output, error
  queued_at, started_at, finished_at, delivered_at

review_findings
  review_run_id, ordinal, severity, title
  path, line, evidence, recommendation, confidence

review_checks
  review_run_id, ordinal, command, status
  exit_code, duration_ms, bounded_output

review_gate_events
  id, session_id, review_run_id, action, fingerprint
  decision, override_reason, created_at
```

Run execution states are:

```text
queued -> snapshotting -> running -> parsing -> completed
                    \-> failed
queued/running      \-> canceled
```

`pass` or `changes_requested` is a verdict on a completed immutable run.
`current` versus `stale` is derived applicability to the source now; history is
not rewritten when the main agent moves on. Gate code always recomputes
applicability instead of trusting a cached UI flag.

On daemon restart, an in-flight run becomes failed with
`daemon_interrupted`; the cleanup sweep removes its ref/worktree and automatic
policy may enqueue it again. Review agents are not durable PTY sessions.

## API

Initial control endpoints:

```text
GET    /api/sessions/:id/copilots
POST   /api/sessions/:id/copilots
PATCH  /api/copilots/:id
DELETE /api/copilots/:id

POST   /api/sessions/:id/reviews
GET    /api/sessions/:id/reviews
GET    /api/reviews/:id
POST   /api/reviews/:id/cancel
POST   /api/reviews/:id/deliver
```

Creating a review returns `202 Accepted` with the run id. The UI polls while it
is non-terminal using TanStack Query; V1 does not need a new terminal WebSocket
frame. Live server-push notifications can follow **RF-WSPROTO**.

Gate failures use typed errors:

- `412 Precondition Failed`: no review, stale review, failed checks, or
  changes requested;
- `409 Conflict`: a review or conflicting SCM unit of work is already active;
Invalid reviewer output is recorded asynchronously as a failed run with reason
`invalid_result`; it is never converted into a pass. Gate responses include a
machine-readable reason and relevant review id.

## Client surface

The new-session and fork dialogs may optionally configure a copilot:

- provider/plugin,
- model,
- advisory or gatekeeper,
- manual or automatic trigger.

An existing session can add, change, disable, or remove one later. Model
selection reuses the current per-plugin model endpoint; choosing another model
in the same provider and choosing a different provider are the same
configuration shape.

The session detail surface gets a separate `CopilotPanel`, not another block
inside the already oversized `RightPanel.tsx`. It shows:

- reviewer and role,
- queued/running/current/stale/failed state,
- reviewed revision and time,
- pass or changes requested,
- findings grouped by severity with file navigation,
- deterministic checks,
- `Review now`, `Cancel`, `Send to agent`, and gate-override actions.

Session-tree badges for reviewing or blocked-by-review can follow after the
panel proves useful. All new strings go through `client/src/i18n/locales/en.json`.

## Session lifecycle

- Manual review is available for a live or stopped session while its workspace
  still exists. Idle-triggered review applies only to a live tracked agent.
- Archiving cancels queued/running reviews and disables the copilot before any
  owned worktree is reclaimed; completed review history remains.
- A fork does not silently inherit reviewer cost, automatic feedback, or gate
  authority. Its dialog may offer the origin's configuration as an explicit
  starting choice.
- Rebase or base-policy changes invalidate prior verdicts. A new session segment
  resets the automatic feedback-round budget but does not narrow a gatekeeper to
  incremental review.

## Safety and operations

- Review subprocesses have a deadline, output cap, cancellation, and descendant
  cleanup. This should share the **RF-OPS** child-supervision primitive.
- One review runs per session; a daemon-wide semaphore limits total concurrent
  reviewers and test commands.
- Automatic review has per-session run and token/cost budgets. Provider usage is
  recorded when available; wall time and invocation count are always recorded.
- Snapshot refs and worktrees live under daemon-owned, validated paths. No user
  string becomes a ref or filesystem path.
- The reviewer gets no inherited bypass/danger flags.
- Secrets in tracked or non-ignored files are visible to the selected provider
  just as they are to the main coding agent. The confirmation surface names the
  provider before enabling cross-provider review.
- Deterministic checks execute repository code and are opt-in workspace policy,
  with the same warning and deadline treatment as setup hooks.
- A model pass never replaces compiler, test, security, remote-protection, or
  human policy.

## Delivery plan

### Stage 0 — contracts and snapshot spike

- Extract a bounded one-shot process runner with child-tree cancellation.
- Implement and test `GitTreeSnapshot` with dirty/staged/untracked files,
  symlinks, executable bits, concurrent-write detection, cleanup, and startup
  recovery.
- Verify the Codex/Claude/opencode review capability matrix, including explicit
  model selection.
- Freeze review-result schema v1 and prompt versioning.

Exit: two different installed providers can review the same immutable fixture
without changing the source worktree, and produce a parsed result or an explicit
failure.

### Stage 1 — manual advisory copilot

- Add copilot/run/finding/check persistence and migrations.
- Add `ReviewCoordinator`, manual review API, cancellation, and polling.
- Supply session digest plus full-session diff context.
- Add the standalone `CopilotPanel`, report history, and manual feedback file /
  `Send to agent`.

Exit: a user selects a same- or cross-provider model, reviews dirty ongoing
work, sees evidence-backed findings, and can hand them to the main agent. A
changed source visibly makes the result stale.

### Stage 2 — automatic review loop

- Add debounced idle events, fingerprint deduplication, trigger coalescing,
  budgets, and bounded retry.
- Add opt-in guarded automatic delivery and the review/fix round limit.
- Surface automatic pauses and costs.

Exit: after a main-agent turn settles, changed work is reviewed once; findings
can be delivered without interleaved terminal input or an unbounded agent loop.

### Stage 3 — gate ASM promotions

- Add typed gate errors and audit records.
- Add per-session SCM serialization and fresh fingerprint checks.
- Gate push of the exact reviewed, clean commit plus deterministic checks.
- Add explicit, reasoned owner override.
- Design merge-candidate gating before applying the gate to merge.

Exit: ASM cannot push an unreviewed or stale revision through its own action,
and every block/override is explainable. Documentation and UI retain the direct
terminal bypass warning.

### Stage 4 — broader orchestration

- Multiple reviewers, specialized rubrics, quorum/consensus policy.
- Remote review runners using the same snapshot/result contract.
- Required-check integration with protected remote branches.
- Incremental review with periodic full-review checkpoints.

This stage is intentionally outside the first feature. A panel of agents before
one reviewer is reliable would multiply cost and ambiguity rather than quality.

## Dependencies and sequencing

- Land **FIX** and the minimal **RF-GATE** router/migration/CI harness first.
- Share dirty-tree capture with the minimum Git plumbing slice of
  **MVP-CKPT**; the rest of checkpoint UI does not block review.
- Reuse **RF-OPS** deadlines, output bounds, and process-tree supervision.
- Create `CopilotPanel` separately from day one; pair its query/API work with
  **RF-REC** and **RF-QUERY** rather than expanding `RightPanel.tsx` and
  `api.ts`.
- Advisory review can ship before **RF-WSPROTO** by polling.
- Gatekeeper errors should use **RF-ERR**, and gate/action serialization should
  use the relevant **RF-LIFE** unit-of-work slice.

## Decisions intentionally deferred

- Whether automatic feedback should ever be on by default.
- Provider-specific token accounting and a common monetary budget.
- Rubric presets beyond general correctness/security and user-supplied focus.
- Remote status-provider integrations and attestation format.
- Non-Git snapshot implementation.
- Quorum rules for multiple copilots.

These do not block a safe manual advisory MVP.
