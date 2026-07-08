# Codebase analysis & refactoring plan

Analyzed: **2026-07-06**, against `release/next` @ `655f613` (plus the applied
cleanup pass in §1). Status: the §1 cleanup is **applied**; the **RF-\***
refactors in §3 are **designed, not implemented** — each has a row in
[`backlog.md`](backlog.md) (RF-MOB, RF-M4, RF-ERR, RF-QUERY, RF-VT100).

Purpose: keep the codebase cheap to change for the milestones already queued
(MOB, R4, M4, SEC-2, MVP-RICH). Every RF item is scoped as *structure only* —
zero product behavior change unless a line below explicitly says otherwise.

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

**Status (2026-07-07): items 1, 3, 4 landed; item 2 folded into M4.** The three
behavior-free refactors that de-risk M4 are done — the `SessionManager` split
(#1), the `db`/`registry` encapsulation (#3), and the `MockBackend` holder seams
+ `startup_reconcile` branch tests that guard the cold-stitch flip (#4). Item 2
(reconnect-supervisor home + `AsmuxClient` trait) is **deliberately deferred into
M4**: it is not meaningful zero-behavior-change structure on its own — the
"home (dial → hello → re-attach → drain)" *is* M4's reconnect machinery and must
be shaped with the logic it hosts, and the `AsmuxClient` trait exists only to
unit-test that not-yet-written logic (and, because `create`/`attach`/`list` are
async, needs `async-trait`/future-boxing best decided with its consumer). The
seams M4 actually reuses are already pre-placed (`AsmuxClient::detach`,
`instance_id`, `head_cursor`, `backend_cursor`), so nothing is lost by co-landing
#2 with M4's reconnect work.

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
| Adopt reconstructs from holder ring, not exact cold-stitch (seams pre-placed: `get_backend_cursor`, `head_cursor`, `Snapshot` fields, `instance_id`) | durable-sessions.md → M3/M4 | M4 (tests first: RF-M4 #4) |
| No daemon↔asmux reconnect; `Detached` stops draining until restart | durable-sessions.md → M4; `sidecar.rs` comment | RF-M4 #2 → M4 |
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

## 5. Sequencing (mirrors backlog → Suggested order)

1. ~~**RF-MOB**~~ ✅ (landed 2026-07-06) → **MOB** phases 1–3.
2. **R4** — no refactor needed; the daemon side is a `Config` field + probe
   loop; relay/agent scaffolding already complete.
3. ~~**RF-M4** (#1 split, #3 encapsulation, #4 test seams)~~ ✅ landed
   2026-07-07; #2 folded into M4 → **M4** (cold-stitch flip is now guarded by
   #4's `startup_reconcile` branch tests).
4. **SEC-2 + RF-ERR** together, then SEC-1 (direct).
5. **DEC-1** → **RF-QUERY** → MVP-RICH.
6. RF-VT100 opportunistically (or on trigger events above).
