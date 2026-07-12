# Codebase analysis & refactoring plan

Analyzed: **2026-07-06**, against `release/next` @ `655f613` (§1–§5); **second
pass 2026-07-12**, against `release/next` @ `d11cc28` (§6 — five parallel
subsystem reviews; every finding below carries file:line evidence and the
FIX defects were re-verified by hand against the working tree); **independent
review 2026-07-12** (§7 — lifecycle semantics, end-to-end flow control,
persistence failure, async boundaries and operability). Status: the
§1 cleanup is **applied**; RF-MOB and RF-M4 have **landed**; the remaining
items — RF-ERR, RF-QUERY, RF-VT100 (§3) and FIX, RF-REC, RF-WSPROTO, RF-GATE,
RF-HYG (§6), plus RF-FLOW, RF-LIFE and RF-OPS (§7) — are **designed, not
implemented**, each with a row in
[`backlog.md`](backlog.md).

Purpose: keep the codebase cheap to change for the milestones already queued
(REC, SEC-2, MVP-RICH, V-track and packaging). Every RF item is scoped as
*structure only* — zero product behavior change unless a line below explicitly
says otherwise.

## 1. Applied cleanup pass (context, already on this branch)

Zero-functional-change pass, verified with `cargo clippy` (silent), the full
test suite (88/88), and client `tsc`/`eslint`/locale-check/proxy tests. Net
−102 lines. Highlights:

- Shared `attach_with_history` / `snapshot_screen` in `backend/mod.rs`
  (previously duplicated verbatim in the native and sidecar backends).
- One git runner + `current_branch()` shared by `source_control.rs` and
  `workspace.rs` (previously byte-identical duplicates).
- `plugins/builtin.rs`: one `cli_launch` helper for the codex/claude/opencode
  launch shape — the next CLI agent plugin is ~15 lines.
- `db.rs`: shared identity read; `collect`/`transpose` idiom aligned.
- Client: shared `shortPath` (`src/paths.ts`); `api.ts` error/URL helpers
  (`unreachableError`, `errorMessage`, `baseOf`); `localTarget()` reuse.

Deliberately **not** done: `cargo fmt` (105 divergences — see §4 rustfmt row),
the client `basename` trio (their empty-input fallbacks genuinely differ), and
anything touching behavior.

## 2. Architecture assessment

**Strengths (preserve these):** real trait boundaries proven by two
implementations (`SessionBackend`/`BackendSession`; also `AgentPlugin`,
`SourceControl`); asmux's never-crash discipline matching its blast radius;
the append-only wire contract; docs-as-debt-registry (zero TODO markers in
source — deferrals live in docs and explicit "Reserved for M4" comments, and
the M4 seams were pre-placed before M4 exists).

**Weak spots, in order of expected pain:**

1. **`SessionManager` is a god-struct** (~1,075 production lines, six
   responsibilities): DB pass-throughs; session creation; a ~600-line
   workspace/worktree/Git cluster; the ~400-line monitor/attention state
   machine; startup reconcile/adopt; live ops. M4 must add reconnect *and*
   mid-flight reconciliation exactly here. → RF-M4.
2. **Daemon↔asmux connection loss has no owner.** Handled in three
   disconnected layers that clear state and stop
   (`asmux_client.rs` reader/writer exit, `sidecar.rs` `Detached` → log +
   `break`, one-shot `connect` in `main.rs`). M4's headline "reconnect with
   backoff" has no home; only `AsmuxClient::detach` is pre-placed. → RF-M4.
3. **Leaky encapsulation:** `SessionManager.db` / `.registry` are `pub` and
   API handlers reach through them (`api/ws.rs`, `api/mod.rs`). Inconsistent
   persistence boundary: `SidecarBackend` holds the whole `Db`; the native
   backend gets only the narrow `EventSink`. → RF-M4 (item 3).
4. **Ad-hoc error mapping:** blanket `anyhow → 400`, hand-built 404s per
   handler, and the typed `NeedsForce → 409` downcast exists in only one of
   the three handlers that can produce it. The relay already has the right
   pattern (`RelayError::code()/status()`). → RF-ERR.
5. **Client:** `App.tsx` entangles shared data wiring with desktop-only
   layout (blocks a clean MOB shell split); query keys / invalidations are
   hand-rolled at ~10 sites; `api.ts` is the whole type registry + client in
   one file. → RF-MOB, RF-QUERY.
6. **Test seams:** `MockBackend` doesn't override `holder_list`/`adopt`, so
   the startup adopt path — exactly where M4's cold-stitch lands — is
   untested and currently untestable. `AsmuxClient` is a concrete struct over
   a real socket, so reconnect logic would be integration-only. → RF-M4
   (item 4).

## 3. Refactors

### RF-MOB — client shell prep ✅ landed 2026-07-06

MOB's only real structural obstacle. All items are hours, not days. **Done**
(client-only, zero behavior change; full build gate + proxy tests green) — as
built, per item below:

1. `useActiveSession()` → `client/src/useActiveSession.ts` (poll + derivation +
   health counts; App destructures it).
2. `showUsage` → `useUiStore` (`client/src/store.ts`).
3. `isLive`/`isTerminal` → `client/src/status.ts`. Decision on the
   `indeterminate` semantics: **neither** live nor terminal (a session the
   daemon lost track of across a restart is unresolved, not ended) — this
   matched all three prior call sites, so the unification was behavior-
   preserving.
4. Clipboard-with-fallback → `copyText()` in `client/src/clipboard.ts` —
   delivered by the terminal-selection-copy feature (`7a56cd3`), which hoisted
   it out of `RightPanel` and reuses it for the xterm copy path too. Both call
   sites (RightPanel CLI-copy, Terminal Ctrl-Shift-C/⌘-C/right-click) now share
   the one util; the MOB key bar's Paste will build on it.

Original scope (for reference):

1. Extract **`useActiveSession()`** from `App.tsx` (the `useDaemonStates`
   poll, the `activeState`/`activeSession`/`target`/`live` derivation, and
   the health counts, `App.tsx:32,46-57`) so `MobileShell` consumes the hook
   instead of duplicating the wiring verbatim.
2. Move **`showUsage`** from App-local `useState` into `useUiStore` — the
   only dialog flag not in the store; the mobile terminal header needs it.
3. Unify the **three divergent `isLive` definitions** (`App.tsx:17`,
   `SessionList.tsx:29`, and `RightPanel.tsx:48`'s inverse ended-list) into
   one status-predicate module. ⚠ `RightPanel`'s ended-list omits
   `"indeterminate"` — decide the intended semantics while unifying; pinning
   it one way or the other is a (tiny) behavior decision, not blind cleanup.
4. Ride-along: hoist `copyCli`'s clipboard-with-fallback (`RightPanel.tsx`)
   to a util — the MOB key bar's Paste needs exactly it.

Acceptance: desktop renders identically; MOB phase 1's `MobileShell` mounts
with no copied wiring.

### RF-M4 — pre-M4 daemon refactor bundle (P1; land immediately before M4)

**Status (2026-07-11): all four items landed.** Items 1, 3, 4 landed 2026-07-07
(the `SessionManager` split, the `db`/`registry` encapsulation, and the
`MockBackend` holder seams + `startup_reconcile` branch tests). **Item 2 landed
with M4 Stage A** (2026-07-11): the reconnect-supervisor home *is* M4's reconnect
machinery (dial → hello → re-attach → drain + backoff + idle watchdog), so it was
shaped together with the logic it hosts, and `AsmuxClient` now implements a
`Holder` trait (`#[async_trait]`) so the reconnect/reconcile paths are
unit-testable. See durable-sessions.md → M4 Stage A.

M4's features land in the two worst structural spots; restructure first so M4
is additive.

1. **Split `SessionManager` into three** (module moves + visibility only, no
   logic edits): a workspace service (worktree/Git cluster:
   `resolve_workspace`, register/list/remove, instances, cleanup,
   archive/discard, orphan-branch helpers), a session monitor (the
   `spawn_monitor` select loop, `on_idle`/`on_output`/`on_exit`,
   `Interaction`, `scan_bell`/`trim_tail`), and the remaining lifecycle core
   (create/stop/resize, reconcile, shutdown, the `live` map). The pieces are
   already self-contained via `self.db`/`self.live`; the
   `too_many_arguments` allows are the same extraction-pressure signal.
2. **Create the reconnect-supervisor home**: one task owning the
   `AsmuxClient` lifecycle (dial → hello → re-route/re-attach live sessions →
   drain), with `sidecar.rs`'s `Detached` arm reporting into it instead of
   silently breaking. Put `AsmuxClient` behind a small trait (or injectable
   factory) at the same time so M4's backoff/reconciliation is unit-testable.
   Pre-placed seams to reuse: `AsmuxClient::detach`; reply routing for
   `purge`/`updateMetadata`/`readBuffer` already exists on both ends — M4's
   "new RPCs" are ~25-line client methods each, not protocol work.
3. **Tighten encapsulation**: make `SessionManager.db`/`registry` private and
   give `api/` the two narrow accessors it actually uses; optionally narrow
   `SidecarBackend` from a full `Db` to a persistence facade (event sink +
   cursor store) matching the native backend's `EventSink` posture.
4. **Test seams**: extend `MockBackend` with
   `keep_sessions_on_shutdown`/`holder_list`/`adopt` overrides and add
   `startup_reconcile` branch tests (adopt-ok / adopt-fail → indeterminate /
   dead entry / absent entry) **before** flipping adopt from ring-replay to
   cold-stitch — that flip is currently unguarded by any test.

Acceptance: all existing tests pass (moved verbatim where relocated); new
reconcile tests cover the four branches; daemon behavior byte-identical.

### RF-ERR — typed daemon error → HTTP status mapping (P2; pairs with SEC-2)

Model on `asm-relay`'s `RelayError`. A `DaemonError` enum (NotFound,
NeedsForce/Conflict, Forbidden, BadRequest, Internal) with one
`IntoResponse`, keeping the blanket `anyhow → 400` as the fallback during
migration. Kills the per-handler 404 hand-builds and the one-off `NeedsForce`
downcast; gives SEC-2 its 403 and M4 `purge` its conflict codes for free.
⚠ One deliberate behavior decision inside: `cleanup_instance` /
`cleanup_workspace_worktrees` currently collapse `NeedsForce` to a generic
400 while `archive_session` maps it to 409 — aligning them to 409 is a small
API behavior change; check the client's confirm-and-retry handlers first.

### RF-QUERY — client data-layer consolidation (P2; after DEC-1, before MVP-RICH)

`RightPanel`'s 5-query/3-mutation shape is the template every MVP-RICH panel
will clone; fix the template before cloning it.

1. A `queryKeys` factory scoping keys by daemon `baseUrl` (today ad-hoc
   string arrays across ~8 files).
2. A `useDaemonMutation` helper centralizing the `["daemon"]` /
   `["workspaces", baseUrl]` invalidations (hand-rolled at 6+ sites).
3. Split `api.ts` into `api/types.ts` + domain-grouped call modules
   (sessions / scm / workspaces / fs / relay).

### RF-VT100 — terminal emulator dependency review (P3)

`vt100` 0.15 is unmaintained; the workspace disables `overflow-checks` for it
(root `Cargo.toml`) and `repaint_with_history` carries a deep-scrollback
invariant ("only the FIRST visible row may be read at offsets deeper than the
screen height"). Correct today, but it is the sharpest hidden edge in the
codebase. Action: evaluate `termwiz` / `alacritty_terminal` behind the
existing snapshot interface (the §1 helpers centralize the swap surface).
Until then: any change to the repaint walk must re-read that invariant note.
Revisit when M4 cold-stitch stresses the snapshot path, or on upstream CVE.

## 4. Tech-debt & compromises register

Deliberate, documented compromises live in their owning docs — this table is
the index, not a duplicate. "Untangle" names the milestone or RF item that
closes each.

| Debt / compromise | Recorded in | Untangle via |
| --- | --- | --- |
| ~~Adopt reconstructs from holder ring, not exact cold-stitch~~ **RESOLVED (M4 Stage B, 2026-07-11)**: cold-stitch adopt is the default (`backend_cursor` exact, seed from cold history, `attach FromCursor(consumed)`, gap marker) | durable-sessions.md → M4 Stage B | — |
| ~~No daemon↔asmux reconnect; `Detached` stops draining until restart~~ **RESOLVED (M4 Stage A, 2026-07-11)**: reconnect supervisor + watchdog; `Detached` backpressure resyncs in place | durable-sessions.md → M4 Stage A | — |
| Plaintext HTTP/WS off loopback (direct); relay/gateway see stream plaintext | security-followups.md #1; connectivity plan → R5 gate | SEC-1; R5 decision gate |
| `/api/fs/list` browses whole host; any root registrable | security-followups.md #2 | SEC-2 (+ RF-ERR for 403; reconcile with the workspace-derived allowlist in `resolve_workspace` so there is **one** notion of allowed root) |
| Tokens plaintext at rest; enrollment token never expires; permissive CORS | security-followups.md #3–5 | SEC-3/4/5 |
| Terminal-escape policy client-side only; capture stores raw bytes; parser unfuzzed | security-followups.md #8 | SEC-8 |
| MVP client stack (shadcn/Tailwind/Dockview/Electron) never adopted; shipped client outgrew the plan | backlog → DEC-1 | DEC-1 (decision, then RF-QUERY) |
| architecture.md still names yamux as relay framing default | backlog → DOC-1 | DOC-1 one-liner |
| `vt100` 0.15 unmaintained + overflow-checks workaround + repaint invariant | root `Cargo.toml` comment; `backend/mod.rs` | RF-VT100 |
| ~~Client `isLive` ×3 divergence; `RightPanel` ended-list omits `indeterminate`~~ | this doc | ✅ RF-MOB #3 (2026-07-06): `src/status.ts`; `indeterminate` decided neither-live-nor-terminal |
| rustfmt divergence (105 sites; compact style is hand-managed, no rustfmt.toml) | this doc | Decide once: run `cargo fmt` in a dedicated commit + enforce, **or** record "hand-formatted — don't run fmt" in the repo docs so contributors/agents stop re-litigating it |
| Accepted micro-duplication: `now_ms` in asmux lib+bin; client `basename` trio (divergent fallbacks) | this doc | Accepted — do not "fix" without need |
| Latent defects: `pull` credential-prompt hang; adopt→reconnect cursor-0 rewind; worktree created before create-validation; silent ring-alloc output drop; `exit_signal` never populated; fabricated 24×80 adopt fallback; server/client disagreement over whether `indeterminate` is terminal; invalid `ASM_BACKEND` silently selects non-durable native mode | §6.1 | FIX |
| Client↔daemon WS contract implicit + unversioned; constants hand-mirrored (`ws.rs:19` ↔ `Terminal.tsx:13`); server→client has no frame type at all | §6.3 | RF-WSPROTO |
| Client lint gate is one i18n rule (no react-hooks, no tseslint recommended); no CI anywhere; zero Rust tests for `api/`+WS; `Holder` trait has no mock; M4-C holder RPCs kept warm by nothing | §6.4 | RF-GATE |
| `generated.rs` regenerated by hand; `asmux-protocol.md` claims a `build.rs` that does not exist; no schema↔generated drift check | §6.4 #6 | RF-GATE (+ doc fix) |
| AF_UNIX hard-typed on the daemon side (`main.rs:344-433`, `asmux_client.rs:26-28`), no transport seam | §6.6 | M5 |
| xterm private `_core` monkeypatches ×3 in Terminal.tsx, silently no-op on an xterm upgrade; palette tri-defined (CSS vars + ~40 raw hexes + 3 TS maps) | §6.5 | RF-HYG (structural CSS split gated on DEC-1) |
| End-to-end terminal path has three unbounded queues; DB writer failure drops whole batches; writer death is silent; shutdown has no flush/join; history is materialized as one `Vec`/WS frame | §7.1 | RF-FLOW |
| Lifecycle predicates and transitions are implicit; `create`/`stop`/`archive`/reconcile can race and multi-resource operations rely on best-effort compensation | §7.2 | RF-LIFE |
| `/health` always reports DB `ok`; background tasks are detached; synchronous DB/Git paths run in async handlers; Git/fetch/WS operations lack a coherent deadline policy | §7.3 | RF-OPS |

## 5. Sequencing (mirrors backlog → Suggested order)

1. ~~**RF-MOB**~~ ✅ (landed 2026-07-06) → **MOB** phases 1–3.
2. ~~**R4**~~ ✅ — no refactor was needed.
3. ~~**RF-M4** (#1 split, #3 encapsulation, #4 test seams)~~ ✅ landed
   2026-07-07; #2 folded into M4 → ~~**M4 Stage A + B**~~ ✅ landed 2026-07-11.
4. **FIX** (§6.1) — the verified-defect bundle; before anything else builds on
   the affected paths.
5. **RF-GATE** (§6.4) — at least the router harness + `MockHolder` + asmux
   e2e halves; it is the net every later item relies on.
6. **RF-FLOW** (§7.1) — make overload and persistence failure explicit before
   relying on the cold tier for more recovery behavior.
7. **RF-LIFE** (§7.2) + **RF-REC** (§6.2) → **REC**. The two can be separate
   commits, but RF-LIFE owns state transitions/atomicity and RF-REC owns the
   recovery-specific Git/API/UI seams.
8. **SEC-2 + RF-ERR** together (RF-ERR now carries §6.2's typed
   `run_blocking`), then SEC-1 (direct).
9. **RF-WSPROTO** (§6.3) — before whichever of MOB-PUSH / MVP-RICH / V3 comes
   first; cheap while there are only two frame types, expensive after a third
   ships.
10. **DEC-1** → **RF-QUERY** → MVP-RICH.
11. **RF-OPS** (§7.3) after the minimal gate, or in parallel with feature work;
    land truthful health before deployment/packaging and client deadlines before
    adding more polling endpoints.
12. **RF-HYG** (§6.5) opportunistically; its `MonitorState` item **before
    MEAS**, its Terminal.tsx split before the next significant terminal
    feature or xterm upgrade.
13. RF-VT100 on trigger events (unchanged).

## 6. Second pass — 2026-07-12 five-subsystem review

Method: five parallel reviewers over (a) daemon core, (b) backend + plugins,
(c) `api/` + `source_control.rs`, (d) asmux / asmux-wire / asm-relay, (e)
client + scripts — each briefed on §1–§5 and the backlog so only **new** debt
is reported. Every §6.1 defect and every HIGH structural claim was then
re-verified by hand against the working tree before being recorded here.

**Assessment.** §2's strengths still hold — and two were *re-proven* this
pass: asmux's never-crash discipline is real (every `unwrap`/`expect`/
`panic`/raw-index in all three infra crates is inside `#[cfg(test)]`; no
production `#[allow(clippy::unwrap_used)]` anywhere), and the `AgentPlugin`
boundary is clean (no per-provider `if/else` outside the plugins; the
default-to-no-capability shape is ready for REC's resume seam). The new debt
is not uniform. It concentrates in four places:

1. the **create/adopt/teardown lifecycle + the non-transactional metadata DB**
   — exactly where REC lands (→ FIX, RF-LIFE, RF-REC);
2. the **implicit client↔daemon WS contract** — exactly where MOB-PUSH,
   MVP-RICH and V3 land (→ RF-WSPROTO);
3. the **build gate**, which is much thinner than it looks — client lint is a
   single i18n rule, there is no CI, the `api/` layer has zero Rust tests, and
   the M4-C holder RPCs are kept warm by nothing (→ RF-GATE);
4. **eight latent defects** that pre-date this review (→ FIX).

### 6.1 FIX — verified latent defects (P1)

All verified by hand. Small individually (~2–3 days total including regression
tests); listed most severe first.

1. **`pull` can hang a daemon worker on a credential prompt.** `fetch`
   (`source_control.rs:412`) and `push` (`:600`) set `GIT_TERMINAL_PROMPT=0`,
   each with a comment explaining exactly this hazard — but `pull`
   (`:423-464`) goes through `git_output()` (`:946`), which sets no env. A
   pull against a remote with a missing/expired credential blocks the git
   child on an interactive prompt, wedging a `spawn_blocking` worker
   (`api/scm.rs:224`) instead of failing fast. Fix: set the env in
   `git_output` (or land RF-REC #1's runner, which owns it for all network
   ops).
2. **Adopt→reconnect window rewinds to cursor 0.** `route()` always seeds
   `Route.last_cursor = 0` (`asmux_client.rs:330`); only live output advances
   it (`:599`). `adopt` attaches `FromCursor(consumed)` (`sidecar.rs:129-132`)
   but never seeds the route — so a socket drop before the first post-adopt
   output makes `reattach_all` (`asmux_client.rs:419`) re-attach
   `FromCursor(0)` (or gap-fallback to `FromEarliest`), and the drain loop
   feeds the emulator/broadcast/ring the whole replay unconditionally
   (`sidecar.rs:342-362`; only the SQLite persist is gated) — duplicated
   scrollback on the exact path M4 Stage B made exact. Meanwhile the drain
   task keeps its **own**, correctly-seeded tracker (`sidecar.rs:338`) for
   backpressure resync — two owners for one concept. Fix: seed
   `Route.last_cursor` at attach/adopt time and collapse the two trackers;
   extend the forced-drop integration test to drop before first post-adopt
   output.
3. **Worktree creation precedes create-validation, with no rollback.** In
   `create_session` (`session_manager/mod.rs:165-260`), `resolve_workspace`
   (`:174`) physically creates the worktree; the `build_launch` error
   (`:182`), the approval bail (`:184-186`) and the cwd check (`:195-197`)
   all run **after** it, while `insert_session`/`insert_instance` happen at
   `:220-221` — any bail in between leaks a worktree + branch with no DB row,
   manufacturing the orphan class `cleanup_orphan_worktrees`
   (`workspaces.rs:271`) exists to sweep. The backend-spawn-failure arm
   (`:235-248`) also leaves the instance `active` and the worktree on disk.
   Fix: run all cheap validation before `resolve_workspace`; add a
   commit-or-rollback guard around instance creation; extend the spawn-fail
   arm to discard the instance.
4. **asmux drops PTY output silently under memory pressure.** `reader_loop`
   discards a failed ring push (`session.rs:441-444`, `let _ = …push(chunk)`;
   `ring.rs` returns before `head` advances), and `frame::code::ALLOC_FAILED`
   (`frame.rs:54`) is **never emitted anywhere** — the never-crash invariants
   promise the opposite ("fallible reserve → `ALLOC_FAILED`"). Under the
   exact pressure the design is meant to survive, undrained output vanishes
   with no log and no signal. Fix: at minimum `tracing::error!` + decide gap
   semantics; ideally an unsolicited `Error{ALLOC_FAILED, session_id}` to the
   attacher.
5. **`exit_signal` is never populated.** Both exit arms record `signal: 0`
   and the raw code (`session.rs:452-472`), so `kill -9` surfaces as
   `Exited{code:137, signal:0}` — "exited normally" — while the wire schema
   (`asmux.fbs`) promises `exit_signal != 0` ⇒ signalled. REC and the
   attention exit-outcome classification consume exit status. Fix: decompose
   the wait status on Unix, **or** explicitly mark the field
   reserved-until-M5 in the schema — decide before REC reads it.
6. **Adopt fallback fabricates geometry and start time.** `reconcile_from_holder`
   re-reads a session row it got from the DB moments earlier and, if `None`,
   silently adopts at 24×80 with `created_at = now`
   (`session_manager/mod.rs:395-398`) — converting a genuine mid-reconcile
   invariant violation into wrong geometry and a bogus `duration_ms`. Fix:
   log + skip the id instead of fabricating defaults.
7. **`indeterminate` has contradictory server/client semantics.** The client
   deliberately defines it as neither live nor definitively terminal
   (`client/src/status.ts:7-30`), so it withholds ended-summary and worktree
   cleanup. The daemon includes it in `SessionStatus::is_terminal`
   (`domain.rs:48-59`), and that broad predicate authorizes both workspace
   unregister (`workspaces.rs:192-203`) and archive/worktree deletion
   (`:350-360`). A direct API call can therefore do exactly what the UI says is
   unsafe. Fix the immediate authorization checks to require a **definitively
   ended** state; RF-LIFE then replaces the overloaded boolean predicate with
   named capabilities and pins every transition in a table test.
8. **An invalid backend setting silently disables durability.** Config parsing
   maps only the exact string `sidecar` to asmux and maps every other value —
   including `native`, unset, and typos such as `sidcar` — to the in-process
   backend (`config.rs:107-110`). A deployment typo therefore boots green while
   losing the restart-survival guarantee. Preserve the documented unset default
   if desired, but reject every unknown explicit value and cover it with config
   parser tests.

### 6.2 RF-REC — recovery-specific pre-REC bundle (P1; after RF-LIFE)

The independent review moved the generic lifecycle work (the former
`go_live()`, transaction-grouping and ordered-teardown items) to RF-LIFE, where
it protects every caller rather than being framed as recovery prep. What remains
here is recovery-specific structure. ~2 days.

1. **One `GitRunner`.** `source_control.rs` calls git in seven divergent
   forms — four helpers (`git` `:927` err-on-nonzero, `git_output` `:946`
   raw, `git_allow_diff` `:1048` tolerates exit 1, `git_ok` `:921` bool) plus
   four inline `Command::new("git")` blocks (`file_bytes` `:266`,
   `resolve_commit` `:309`, `fetch` `:410`, `push` `:598`). The error string
   `failed to run git` is copy-pasted at 7 sites; `combined_output`
   success/failure branching is re-implemented in fetch/pull/rebase/push;
   `merge_to_branch` (`:495-566`) owns a temp-worktree lifecycle with three
   separate cleanup call sites. A thin `Git { cwd }` runner owning env
   (`GIT_TERMINAL_PROMPT=0` for network ops — closes FIX #1 structurally), a
   deadline/kill policy, bounded captured output, success policy (`run` /
   `run_allow` / `output` / `ok`), plus an RAII temp-worktree guard.
   **Explicitly not libgit2/gix**: the pain is
   invocation ergonomics and human-readable combined output for the panel,
   which a linked library makes worse.
2. **`require_session` + typed `run_blocking`.** The get-session-or-404
   block is hand-copied six times (`api/scm.rs:19`, `api/mod.rs:324,366,445,525`,
   `api/paste.rs:70`); every SCM handler repeats the same
   lookup→clone→`run_blocking` spine; and `merge` (`api/scm.rs:272-282`)
   cannot use the shared `run_blocking` (`:23`) because it collapses all
   errors to 400, so it hand-rolls `spawn_blocking` + `downcast_ref` — the
   `NeedsForce` inconsistency's second instance. One `require_session`
   helper (or extractor) + a `run_blocking` generic over a typed error.
   Pairs with RF-ERR; REC's `POST /recover` and every MVP-RICH endpoint
   clone whatever spine exists when they land.
3. **Client: split `ScmPanel` out of `RightPanel`.** `RightPanel.tsx` (943
   lines) fuses the SCM subsystem — now grown to 5 queries / 6 mutations,
   past RF-QUERY's "5-query/3-mutation" description — with the VS Code
   launcher, metadata, cleanup, and summary blocks. REC's "Recover"
   affordance has no home; MVP-RICH clones whatever shape exists. Split
   `ScmPanel` / `SessionMeta` / `VscodeLauncher` (pure prop-drill of
   `target`+`session`) before both.

Not an RF item, recorded for REC's design: the plugin seam is **ready** (the
default-None capability shape fits `native_session_id`/`build_resume`), but
nothing captures identity today — `usage.rs` matches transcripts by
`(cwd, mtime)` with two per-provider strategies (`claude_transcript_path`
`:114-122` newest-jsonl; `codex_rollout_path` `:414` recursive scan with an
any-newest fallback) and no shared "locate this session's transcript"
abstraction. That is REC Stage A work per
[`session-recovery.md`](session-recovery.md), not a refactor.

### 6.3 RF-WSPROTO — client↔daemon WS contract (P2; before MOB-PUSH / MVP-RICH / V3)

The one protocol in the system with no written contract, no version field,
and no room for a new frame type. The daemon defines the wire shape entirely
in code — `CLOSE_SUPERSEDED = 4001` (`api/ws.rs:19`), the `{"t":"i"|"r"}`
client frames (`:74-81`), a binary-first-frame snapshot convention (`:121`),
and two out-of-band text sentinels (`:216`, `:222`) — and the client
re-hardcodes the same constants independently (`Terminal.tsx:13`, frame
literals at `:263,:311`, ~16 `{t:"…"}` sites across client + e2e scripts).
The receiving path treats **every** server→client message as terminal bytes
(`Terminal.tsx:284-287`), so a control message has literally nowhere to land
— it would be written into the screen as garbage. Contrast
[`asmux-protocol.md`](asmux-protocol.md), which gives its leg opcodes, caps,
reserved tag ranges, and a hello-first rule.

Deliverables: (i) a contract doc mirroring asmux-protocol.md — close codes,
control frames, snapshot/history/sentinel semantics, a reserved tag space and
a hello/version field; (ii) a shared `client/src/terminalProtocol.ts` — a
`ClientFrame` union + `encode()`, and the load-bearing half, a `ServerFrame`
demux on `onmessage` that routes output to `term.write` and reserves a
control branch — consumed by the client **and** the e2e scripts. ~1 day now,
while there are exactly two frame types; three backlog items (MOB-PUSH,
MVP-RICH structured output, V3's editor channel) each need a server→client
frame and will otherwise each invent one.

### 6.4 RF-GATE — build gate & test safety net (P1/P2)

The net every other item in this document relies on. ~2–3 days.

1. **Client lint is a single rule.** `eslint.config.js` enforces only
   `i18next/no-literal-string`; it imports `typescript-eslint` but never
   spreads a recommended config, and `eslint-plugin-react-hooks` is not
   installed — in a codebase leaning heavily on the ref-to-dodge-a-dep
   pattern, `exhaustive-deps` is exactly the missing police. One real catch
   already known: `Terminal.tsx:769`'s dep array omits `target.relayKey`
   (stale-WS if the key rotates without a baseUrl change). Add
   `tseslint.configs.recommended` + react-hooks (error); expect and burn
   down a findings backlog. Optionally `noUncheckedIndexedAccess` (the
   color-map/array indexing MVP-RICH will multiply).
2. **There is no CI.** `.github/workflows` does not exist; `npm test` runs
   only the proxy test; the 11 e2e scripts run when a human remembers. A
   minimal job: cargo build + clippy + test → client tsc/eslint/
   check-locales/vite build → 2–3 fast e2e scripts (smoke, mobile-shell).
3. **Zero Rust tests for `api/` + WS.** No test constructs `api::router` or
   an `AppState`; `ws.rs`'s `Attachments` single-attacher/takeover logic
   (`:55-69`) is untested despite needing no I/O. Stand up a
   `tower::ServiceExt::oneshot` harness against the router (the RF-M4 #4
   `MockBackend` seams already exist); RF-ERR's status-mapping change and
   REC's handler then get fast guards.
4. **No `MockHolder`.** The `Holder` trait was added for testability but
   only `AsmuxClient` implements it; `SidecarBackend::create`/`adopt`, the
   whole `drain_loop` including the `DETACH_BACKPRESSURE` resync arm
   (`sidecar.rs:389-411`), and `end_session_stream` have zero unit coverage
   — the one integration test asserts "output resumes", not cursor
   correctness, and would not catch FIX #2. Add a `MockHolder` (canned
   attach results incl. `Gap`, scripted `StreamEvent`s) and drive the
   branches.
5. **asmux e2e for the dead/half-wired paths.** `handle_read_buffer`
   (`asmux/src/server.rs:546`) and `handle_detach` (`:659`) have no test and
   no caller; backpressure eviction (`:729-737`) and cross-connection
   takeover are entirely unexercised; malformed RPC bodies are silently
   dropped by 10 `Err(_) => return` arms (contract says answer with
   `Error`). Add cases for readBuffer (partial / `BUFFER_GAP` / invalid),
   detach ownership, eviction, takeover, and truncated-body dispatch —
   cheap insurance **before** M4-C wires the daemon side.
6. **`generated.rs` has no drift check.** There is no `build.rs` in the
   workspace — `asmux-protocol.md` claims codegen runs via one (two doc
   lines to fix); regeneration is the manual command in
   `asmux-wire/src/lib.rs:11`, and nothing verifies the committed 12k-line
   `generated.rs` matches `asmux.fbs`. An edited schema with a forgotten
   regen compiles silently stale. CI: run `planus rust` to a temp file and
   diff.
7. **Migration ladder + version-skew guards.** No test opens an old-version
   DB and migrates; `user_version` **newer** than the binary is silently
   accepted; unknown persisted statuses coerce to `Failed`
   (`domain.rs:35-45`) — the one state REC treats as a real failure. Reject
   forward-rolled DBs with a clear error; add one ladder test; decide the
   unknown-status posture.

### 6.5 RF-HYG — hygiene bundle (P3; opportunistic, items are hours each unless noted)

Daemon/backend:

- **Constants hoist**: `BROADCAST_CAP`/`SCROLLBACK` duplicated verbatim
  (`native.rs:18-19` = `sidecar.rs:36-37`) despite `HISTORY_RING_BYTES`
  having been deliberately centralized; the 10 s RPC timeout is a bare
  literal twice (`asmux_client.rs:360,457`); `DETACH_BACKPRESSURE: i8 = 2`
  (`sidecar.rs:40`) hardcodes a wire-enum value.
- **`AttentionState::is_sticky()`**: the `LikelyBlocked | ApprovalNeeded |
  Error` set is a copy-pasted `matches!` at `monitor.rs:61-68` and
  `:178-181`; adding a state must hit both or blocks silently demote.
- **Attention classifier dedup**: three near-identical "selected numbered
  option" parsers (`attention.rs:102`, `attention/claude.rs:159`,
  `attention/codex.rs:55`) + a duplicated approval skeleton — the module doc
  promises drop-in providers; today that means a third copy-paste. Shared
  `is_selected_option(line, pointers)` + `menu_approval(screen, pointers,
  phrases)`.
- **`MonitorState` extraction** (do **before MEAS**): `on_output` is an
  11-argument function threading monitor state as loose `&mut` locals
  (`monitor.rs:119-132`); the classifier inputs MEAS must observe are
  ephemeral. A `MonitorState` struct + a returned `Classification { state,
  reason, inputs }` collapses the signature and gives MEAS its hook.
  ~0.5–1 day.
- **Write-side loop dedup**: the §1 cleanup unified the read side; the write
  side is still forked — `native.rs:200-234` ≈ `sidecar.rs:342-378`
  (catch_unwind feed, ring push + broadcast under the ring lock, event
  send). An RF-VT100 swap or a MEAS tap silently skips one copy. Extract
  `feed_and_broadcast(…)` into `backend/mod.rs`.
- **`attach_or_resync` consolidation**: the Ok/Gap/Conn/Code dance is
  hand-rolled at four sites (`sidecar.rs:73-84`, `:130-158`, `:395-409`;
  `asmux_client.rs:424-437`), each subtly different; REC resume and M4-C
  `readBuffer` would clone a fifth.

asmux/relay consistency (fold into M4-C or R5 work when convenient):

- Malformed RPC bodies → answer `Error{INVALID_ARGUMENT}` instead of the 10
  silent `Err(_) => return` arms (today a corrupt body is indistinguishable
  from a dead holder: 10 s stall + reconnect).
- `Registry::create` holds the global registry lock across `openpty`/fork
  (`registry.rs:115-124`) — spawning session A stalls keystroke delivery to
  live session B. Reserve under lock, spawn outside, finalize/rollback.
- `WATCHDOG_IDLE_MS` (`asmux/src/lib.rs:66`) is referenced by nothing —
  implement the server-side idle teardown it advertises, or delete it.
- Takeover's `Superseded.last_cursor` reports `head` (`server.rs:629-634`)
  while backpressure eviction reports the true stream position (`:731-736`)
  — document as an upper bound or track the attacher's cursor.
- Stale `yamux` reference in the frozen-contract module's own doc comment
  (`asm-relay/src/protocol.rs:49`) — DOC-1's sibling, one line.
- The self-labelled "TEMPORARY diagnostic" test (`asmux/src/socket.rs:216`)
  — keep or delete, drop the label.

Client/scripts:

- **`theme.ts`**: the tokyo-night palette is defined three ways — `:root`
  CSS vars, ~40 raw hexes scattered through `styles.css` that never
  reference them, and three independent TS color maps
  (`SessionList.tsx:12-31`, `RightPanel.tsx:28-36`, `Terminal.tsx:143-148`)
  plus inline hexes. MVP-RICH's diff/markdown renderers would copy a fourth.
  Dead rules confirmed: `.conn-current` (`styles.css:309`), `.enroll-token`
  (`:316`), `.tree-node.lvl1` + dependents (`:598,649,652,1907`). Tokens +
  dead-rule sweep now (~2 h); the structural CSS split is gated on DEC-1.
- **`TerminalHeader` extraction**: the terminal-header + `UsageModal` block
  is copy-pasted between the shells (`DesktopShell.tsx:121-128` ≈
  `MobileShell.tsx:180-187`) — the one break in "one shared component, both
  shells free".
- **Terminal.tsx split** (~1.5 days — the largest item): one ~640-line
  effect (`:126-769`) mixes xterm setup, three private-`_core` monkeypatches
  cast through `as unknown` (viewport sync, `shouldForceSelection`, the
  passive-input guard — all of which **silently no-op** if an xterm upgrade
  moves the internals), WS lifecycle, copy/paste, image paste, and a
  ~250-line touch-gesture state machine. Extract `attachTouchGestures` /
  `attachClipboard` / `connectStream` returning disposers; wrap the `_core`
  patches in one typed shim with a dev-time existence assertion so an
  upgrade fails loudly. Trigger: before the next significant terminal
  feature or any xterm bump.
- **`createTopology()` e2e helper**: `createSandbox` is single-daemon-shaped,
  so the three multi-node tests (`relay-test`, `gateway-test`,
  `termscroll-test`) abandon it and hand-roll ~110–125 lines of process
  lifecycle each — the exact code class the holder-theft incident hardened.
  Also: `startChrome` lacks console-tap/auto-dialog options, forcing
  `copy-paste-test` to build its own CDP client.

### 6.6 Findings routed to existing rows (annotations, not new items)

- **SEC-2**: there are today *three* divergent notions of allowed root —
  `fs::list` enforces none; `register_workspace` (`workspaces.rs:161`)
  self-widens the set; and `resolve_workspace`'s inline check
  (`workspaces.rs:147-151`) is **skipped entirely when no workspace is
  registered** and rides on `canonical()`'s fall-back-to-raw-path on error
  (`mod.rs:516-518`), so `..` segments survive into a textual `starts_with`.
  SEC-2 is a new `allowed_roots` module consumed by all three call sites,
  not a patch.
- **M4-C**: the daemon demux pre-wires PURGE/UPDATE_METADATA/READ_BUFFER/
  DETACH response arms (`asmux_client.rs:682-688`) that nothing can trigger,
  and `HolderSessionInfo.head_cursor`'s comment (`:66-70`) still says
  "reserved for exact cold-stitch" although Stage B landed without it —
  correct the comment to "unused; M4-C readBuffer/orphan surfacing". The
  holder-side handlers' test gap is RF-GATE #5.
- **M5**: AF_UNIX is hard-typed on the daemon side with no transport seam —
  `wait_for_asmux`/`ensure_asmux` (`main.rs:344-433`) and the client
  read/write halves (`asmux_client.rs:26-28`, `OwnedReadHalf`/
  `OwnedWriteHalf`). Budget a `HolderTransport` (probe/connect) abstraction
  into M5's estimate; not worth pre-building at P3.
- **R5 (ops)**: the relay's `nodes` map never evicts — `disconnect` only
  flips `connected=false` (`asm-relay/src/server.rs:196-202`), so
  `snapshot()` lists offline nodes forever; and a per-connection heartbeat
  thread + writer task linger after a protocol-error return until the peer
  closes. Both fit R5's ops/hardening list.
- **MEAS**: depends on RF-HYG's `MonitorState` extraction (the hooks observe
  its returned `Classification`); landing MEAS first just makes MEAS carry
  the refactor.

### 6.7 Verified healthy — do not churn

- asmux never-crash discipline (see Assessment above); parking_lot
  throughout, so no lock-poisoning surface.
- `AgentPlugin` dispatch: every call site goes through `registry().get(id)`
  + trait method; defaults-None capability shape is REC-ready.
- No mutex held across an `.await` anywhere in the backend files (checked:
  `pending`/`routes`/`parser`/`history` locks all drop before awaits).
- The reconnect→reconcile wiring subscribes before the consumer spawns
  (`main.rs:103-135`) — a startup-race reconnect is buffered, not lost.
- `api/paste.rs` (byte-sniffing, server-derived paths, shared
  `sniff_image_mime`) and `auth.rs` are clean; `source_control.rs`'s
  *parsing* + guard functions are well-tested (`:1160-1689`) — the debt there
  is invocation scatter (RF-REC #1), not logic.
- `scripts/_asm_common.sh` is well-factored with consistent
  `set -euo pipefail` discipline; `setup.sh`'s self-contained duplication is
  deliberate (clean-machine bootstrap).
- Client i18n discipline genuinely holds — no literal bypasses found.

## 7. Independent review — missing reliability and lifecycle work

The second pass is strong on file structure, duplication and near-term feature
seams. Its main blind spot is **failure semantics across boundaries**: it treats
the database writer, async queues, background tasks and status predicates as
implementation details, even though those decide whether output is lost and
whether a retry is safe. The following items are additions, not rewrites of the
useful §6 findings.

### 7.1 RF-FLOW — bounded terminal flow + durable persistence (P1)

The hot path has three unbounded queues. Output crosses the first two before it
is cold-durable; commands use the third in the opposite direction:

1. asmux client route (`asmux_client.rs:93-95,324-333`),
2. sidecar/native drain → `EventSink` (`db.rs:38-48,61`), and
3. fire-and-forget holder commands (`asmux_client.rs:120-130,291-321`).

This defeats the holder's bounded backpressure: once the daemon reads a frame,
an overloaded parser/SQLite writer can grow process memory without limit. The
failure side is worse: `EventSink::send` quietly discards a closed-writer error
(`db.rs:44-48`), `event_writer_loop` logs and drops an entire failed batch with
no retry or degraded state (`:753-767`), and daemon shutdown neither flushes nor
joins the writer (`main.rs:166-181`). The cold-stitch invariant therefore has no
way to distinguish “persisted” from “accepted into an unbounded queue.”

The command direction has the same ambiguity. `SidecarSession` reports success
for input/resize/stop (`sidecar.rs:281-295`) although the `AsmuxClient` ignores a
closed command channel and holder `INPUT_OVERFLOW` is only logged. While the
holder is reconnecting, stale keystrokes and kills accumulate and replay later.

Deliverables:

- bounded byte-budgeted queues (not only item-count caps) with an explicit
  per-message policy: output backpressures; resize coalesces; input fails visibly;
  stop/kill is acknowledged and idempotently retried;
- bound the relay control queues too (`asm-relay/server.rs:326`, `agent.rs:125`):
  coalesce heartbeat/downstream-state updates and fail/close on an `Open` backlog
  instead of buffering an unlimited number of dial requests;
- a persistence service with a writer-health state, bounded retry/backoff,
  explicit fatal/degraded signaling, and `flush(deadline)`/join on shutdown;
- stream/chunk `read_events_after` and transcript/history responses rather than
  materializing all events into one `Vec` and one WS frame
  (`db.rs:199-213`, `api/ws.rs:202-222`, `api/mod.rs:401-418`);
- a documented retention/compaction policy. `terminal_events` is append-only and
  never deleted today, including after archive; disk exhaustion is a predictable
  operating state, not an exceptional edge;
- fault tests for writer death, `SQLITE_FULL`/busy timeout, slow parser, reconnect
  with queued input, and shutdown with an in-flight batch. Assert bounded RSS,
  visible degradation, monotonic cursor/sequence, and no acknowledged-byte loss.

Acceptance: every byte/command is either acknowledged at its promised durability
level or rejected visibly; there is no unbounded queue on a daemon/relay hot path;
shutdown reports whether the persistence flush completed.

### 7.2 RF-LIFE — explicit lifecycle state machine + units of work (P1)

`SessionStatus::is_terminal()` currently answers several different questions:
“has no live handle,” “is definitively ended,” “may archive,” and “does not block
workspace removal.” Those are not equivalent — FIX #7 is the concrete proof.
The operations themselves also mix DB rows, the `live` map, holder RPCs, Git
worktrees and monitor tasks without a concurrency owner. Transactions alone do
not prevent a reconnect reconcile from racing stop/archive, or two destructive
requests from both passing their precondition check.

Deliverables:

1. A checked transition table with named predicates (`accepts_input`,
   `definitively_ended`, `recoverable`, `archivable`, `blocks_workspace_removal`)
   instead of one overloaded `is_terminal` boolean. Unknown persisted values are
   errors, not silently `Failed`.
2. Per-session operation serialization plus conditional DB transitions
   (`UPDATE ... WHERE status IN (...)`) so stale reconcilers and duplicate HTTP
   requests cannot overwrite a newer intent. Keep backend callbacks event-like;
   only the lifecycle coordinator commits state.
3. A `Db::with_tx`/repository unit of work for session+instance creation and
   exit status+attention+summary. Make each schema migration atomic with its
   `user_version` update; a half-applied ALTER must not brick the next boot.
4. A create saga: validate first, reserve metadata, create the worktree, spawn,
   then commit Running; compensate in reverse order on every failure. A teardown
   saga records intent/progress, is idempotent, and leaves enough state to retry
   worktree/branch cleanup after a crash.
5. Central `go_live()`/`finish()` seams owning the `live` map and monitor handle.
   Track/abort/join monitor tasks instead of relying on process exit.

This item absorbs the former generic RF-REC `go_live`, transaction-grouping and
ordered-teardown bullets. RF-REC stays focused on recovery's Git/API/UI seams.

Acceptance: a table test covers every allowed/forbidden transition; concurrent
stop/archive/reconcile and repeated-request tests prove the final state is
deterministic; injected failure after every saga step is retryable and leaves no
untracked worktree or contradictory DB/live state.

### 7.3 RF-OPS — truthful health, deadlines and task supervision (P2)

`/health` hardcodes `"database": "ok"` and reports only the backend id
(`api/mod.rs:115-127`). It remains green when the event writer has died, SQLite is
stuck, or asmux is reconnecting. Several long-lived tasks are detached
(`main.rs:120-134,208-232`; backend/relay writer and probe tasks), so task death
does not change readiness or restart the component. Separately, synchronous
SQLite and workspace/Git methods run directly in async handlers, while browser
`fetch` calls have no abort deadline and the terminal reconnects forever at a
fixed one-second cadence.

Deliverables:

- separate liveness and readiness: DB read/write probe, event-writer state,
  holder connection/reconnect age, queue pressure and last successful persist;
  expose `ok` / `degraded` / `not_ready` with machine-readable reason codes;
- one task supervisor/cancellation tree for holder reconnect, reconcile,
  per-session monitors, relay tunnel/agent and downstream probes; task exits are
  observed, classified and either restarted or made visible;
- a blocking-work boundary for SQLite/Git/filesystem work. Extend `GitRunner`
  with timeouts, child kill/reap and output caps; do not let concurrent requests
  exhaust Tokio's blocking pool indefinitely;
- shared client request helpers that accept TanStack's `AbortSignal`, apply
  operation-specific deadlines, distinguish timeout/offline/HTTP errors, and use
  capped jittered WS reconnect with an offline pause;
- structured metrics/log fields for queue depth/bytes, dropped/retried work,
  SQLite latency, reconnect count/age and lifecycle transition failures.

Acceptance: killing each supervised task or wedging each dependency changes
readiness within a bounded time; no request or child process can wait forever;
clean shutdown has a deadline and reports unfinished components.

### 7.4 Corrections to the existing change set

- The REC/backlog prose says the transcript endpoint serves raw PTY bytes. Since
  `78437a9`, the default is rendered provider Markdown; raw PTY is only
  `?format=raw` or fallback for agents without a provider transcript
  (`api/mod.rs:338-403`). The dependent docs are corrected with this review.
  The handler's own comment at `api/mod.rs:355-357` also incorrectly says archive
  discards the bytes; it does not, although policy returns 409.
- REC called its next migration `SCHEMA_V6`, but V6 already landed for
  workspace-instance ownership (`db.rs:627-630,743-750`). The recovery design now
  uses V7; RF-GATE's migration ladder should prevent future version collisions.
- RF-GATE should run Clippy with warnings denied. `cargo clippy --workspace
  --all-targets -- -D warnings` currently finds one test-only `type_complexity`
  warning (`backend/mod.rs:629`); ordinary Clippy and all 168 Rust tests pass.
- The client build could not be re-run in this review environment because `npm`
  is not installed. CI remains the authoritative fix for machine-dependent
  “works on my checkout” verification.
