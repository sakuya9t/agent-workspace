# Backlog

Last reconciled: **2026-07-05** (against `release/next` through `7d071b0`,
which added `mobile-ui.md`). Since then, **image/screenshot paste + the 📎
button** landed on session branch `asm-session/3160afbb` (see "Already done"
below and [`image-paste.md`](image-paste.md)) — not yet merged to `release/next`.

This is the single cross-track index of work that is **designed but not yet
implemented**. The detailed designs stay in their own documents
([`connectivity-execution-plan.md`](connectivity-execution-plan.md),
[`durable-sessions.md`](durable-sessions.md),
[`vscode-over-relay-plan.md`](vscode-over-relay-plan.md),
[`mobile-ui.md`](mobile-ui.md),
[`security-followups.md`](security-followups.md),
[`mvp-execution-plan.md`](mvp-execution-plan.md)); this file only records
**what is pending, why it matters, what it depends on, and in what order to
pick it up**.

**Maintenance rules**

- When a milestone lands, mark it done here (and in its source doc) in the
  same commit.
- **Anything newly designed gets a row here before implementation starts** —
  new plans do not live only in their own doc.
- Priorities: **P1** = next up / on the product path, **P2** = important but
  gated or second in line, **P3** = valuable, not urgent, **P4** = deferred by
  explicit decision.

## Already done (context, do not re-plan)

- MVP core loop end-to-end: sessions, attach/replay, snapshots, attention
  signals, agent plugins (shell/codex/claude/custom), Git SCM panel
  (status/diff/log/pull/rebase), workspaces + per-session worktree isolation,
  device enrollment + bearer auth, multi-daemon client, i18n infrastructure
  (en-only), usage endpoint.
- Durable sessions **M1–M3**: `asmux` holder, `SidecarBackend`, adopt-on-restart
  (ring-replay), `indeterminate` state incl. client badge
  (`scripts/durable-restart-test.mjs` proves it).
- Connectivity **R1–R3**: `asm-relay` (dial-out-per-stream), daemon
  register-out + tunnel listener (loopback trust defeated), client relay
  support — a NAT'd node is fully controllable from the browser with zero
  client tooling (`scripts/relay-test.mjs`, R3 CDP harness).
- VS Code correctness fix: relayed hosts get a disabled button + honest hint
  instead of a misdirected Remote-SSH deep link.
- Image/screenshot paste: paste, drag-drop, or the 📎 button feed an image into
  a live terminal → daemon stores it under `<cwd>/.asm/pastes/`
  (`POST /api/sessions/:id/paste`, magic-byte + size validated) → client injects
  `[pasted image <path>]` over the existing WS input frame → the agent loads it
  on submit. Design + as-built: [`image-paste.md`](image-paste.md); proofs
  `scripts/paste-test.mjs` + a headless-Chrome click-through of the 📎 button.

## Backlog summary

| ID | Item | Priority | Depends on | Source (design) |
| --- | --- | --- | --- | --- |
| R4 | Gateway mode (egress-less downstreams) | **P1** | R1–R3 (done) | connectivity-execution-plan.md → R4 |
| M4 | Holder hardening + exact cold-stitch adopt | **P1** | M1–M3 (done) | durable-sessions.md → M4 |
| MOB | Mobile UI phases 1–3 + verify (adaptive shell, sheet CSS, terminal key bar) | **P1** | — (client-only) | mobile-ui.md → Execution plan |
| MOB-PWA | Mobile UI phase 4: PWA manifest + iOS metas | **P2** | MOB | mobile-ui.md → Packaging path |
| MOB-PUSH | Web Push for attention states | **P3** | MOB; daemon push plumbing (relay as carrier) | mobile-ui.md → Follow-ups |
| IMG-2 | Image paste follow-ups: `.asm/pastes/` cleanup policy, per-agent capability hint | **P3** | image paste + 📎 button (done) | image-paste.md → Follow-ups |
| SEC-2 | Constrain `/api/fs/list` + workspace roots | **P1** | — | security-followups.md → 2 (HIGH) |
| SEC-1 | Transport encryption off-loopback (direct mode TLS; relay TLS ops) | **P1/P2** | partially ties to R5 | security-followups.md → 1 (HIGH) |
| V0 | Web-editor de-risking spike (scratchpad only) | **P2** | R1–R3 (done) | vscode-over-relay-plan.md → V0 |
| V1 | Relay cookie auth + daemon editor proxy | **P2** | V0 go decision | vscode-over-relay-plan.md → V1 |
| V2 | IDE launcher, detection, editor tickets | **P2** | V1 | vscode-over-relay-plan.md → V2 |
| V3 | Web-editor client wiring, lifecycle, polish | **P2** | V2 | vscode-over-relay-plan.md → V3 |
| R5 | Relay hardening & productization (itemized) | **P2** | R4 for full scope; several items standalone | connectivity-execution-plan.md → R5 |
| SEC-3 | Enrollment token expiry/rotation/rate limit | **P2** | overlaps R5 pairing-code item | security-followups.md → 3 (MEDIUM) |
| SEC-4 | Hash device tokens at rest | **P2** | — | security-followups.md → 4 (MEDIUM) |
| SEC-5 | Restrict CORS origins | **P2** | check against V1 cookie design first | security-followups.md → 5 (MEDIUM) |
| SEC-6 | Optional "always require token" (no loopback trust) mode | **P2** | — | security-followups.md → 6 (MEDIUM) |
| MVP-RICH | Rich output pipeline (viewer/diff/markdown/transcripts) | **P2** | client-stack decision DEC-1 | mvp-execution-plan.md → Workstream 4 |
| MVP-HOOKS | Workspace setup hooks (copy/symlink/bootstrap) | **P3** | — | mvp-execution-plan.md → Workstream 6 |
| MVP-CKPT | Checkpoints + "New segment" | **P3** | — | mvp-execution-plan.md → Workstream 7 |
| M5 | Windows support (ConPTY, transport, ACLs) | **P3** | M4 (sequenced) | durable-sessions.md → M5 |
| MVP-PKG | Electron shell + packaging + service install | **P3** | DEC-1; M5 for the Windows gate | mvp-execution-plan.md → Workstreams 3/11 |
| SEC-7 | Auth rate limiting + lifecycle audit log | **P3** | overlaps R5 | security-followups.md → 7 (LOW) |
| SEC-8 | Terminal-escape policy at capture/replay + fuzzing | **P3** | — | security-followups.md → 8 (LOW) |
| DEC-1 | Decide: adopt planned client stack (shadcn/Tailwind/Dockview/Electron) or amend the plan | **P2** (decision, cheap) | — | mvp-execution-plan.md → Baseline Technology |
| DOC-1 | Doc sync: architecture.md still calls yamux the relay default | **P3** (one-liner) | — | architecture.md → Open Decisions |
| I18N-2 | Additional locales beyond `en` | **P4** (deferred by user) | — | i18n.md → Adding a locale |

## Detail

### R4 — Gateway mode (P1)

The next milestone on the product path (relay = the connection model; see the
locked zero-client-tooling principle). Daemon parses `ASM_RELAY_DOWNSTREAMS`,
probes downstream `/health` for `node_id`/`label`, advertises them over the
control stream, and serves `open` frames targeting a downstream by dialing
that host:port; relay routes downstream node_ids via the owning gateway and
reports `via`; client renders "D · via C". Plumbing already exists on both
sides (`downstreams` in the protocol, `via` in `/nodes`). Acceptance: the
gateway-test script described in the plan (relay + gateway C + downstream D on
distinct loopback addresses; 5 checks).

### M4 — Holder hardening (P1)

Orthogonal to the R-track (explicitly: R code must not assume M4 exists).
Scope: idle watchdog + daemon↔asmux reconnect with backoff,
`list`-after-reconnect reconciliation, soft-reboot on `binary_sha256` drift
(warn + confirm), orphan surfacing/adopt UI, `purge`, metadata RPCs,
`readLog`, slow-attacher drop + resync — **plus the M3 follow-up**: make the
exact cold-stitch adopt the default (seed vt100 from persisted snapshot,
stitch from SQLite cold history, `attach FromCursor(backend_cursor)`, real gap
marker on `BUFFER_GAP`; everything is scaffolded, ring-replay is currently the
default). This closes the headline "terminal intact after restart" promise for
long-lived sessions whose output outgrew the ring.

### MOB — mobile adaptive shell (P1, client-only)

Newly designed 2026-07-05 (`mobile-ui.md`). One codebase: `useIsPhone()`
(`PHONE_MQ`) switches between the existing 3-pane `DesktopShell` and a new
stacked `MobileShell` (Sessions home → full-screen Terminal → Details sheet
over the live terminal); all panels/dialogs/stores/queries shared, so feature
parity is structural. Phases: (1) shell split + pushState navigation,
(2) touch-target + modal→bottom-sheet CSS, (3) terminal key bar
(Esc/Tab/⇧Tab/Ctrl-latch/^C/arrows/⌨/Paste via a `TerminalView` input handle)
+ visual-viewport keyboard geometry, (4) PWA manifest/icons (= MOB-PWA row),
(5) headless-Chrome mobile-viewport verification + desktop regression.
Phases 1–2 make the app usable on a phone; 1–3 make the terminal genuinely
workable; each ships independently. No daemon changes; interleaves freely
with R4/M4 (disjoint code). i18n rule applies to all new strings. This is the
natural consumer of the R3 milestone ("mobile-ready, zero client tooling").
Smaller follow-ups noted in the doc (attention-pinned home group, font-size
control, tap-and-hold tooltips) ride along or slot in later; **MOB-PUSH**
(Web Push for `approval_needed`/`likely_blocked`) is its own row because it
needs daemon-side push plumbing with the relay as carrier — design that
before building.

### SEC-1 / SEC-2 — the two HIGH security items (P1)

Gate exposing ASM beyond a trusted LAN. SEC-2 (fs-list browses the whole host
filesystem; any client can register any workspace root) is self-contained
daemon work: server-side allowed-roots config enforced for both browsing and
registration. SEC-1 (plaintext HTTP/WS off loopback) splits: the **relay path**
gets TLS via `ASM_RELAY_TLS_CERT/KEY` + deployment guidance (an R5 ops item);
**direct mode** needs the Phase-8 TLS/mTLS deliverable (self-signed +
pinning or ACME). Until then the SSH-tunnel recommendation stays prominent.

### V0–V3 — browser VS Code over the relay (P2)

Design complete in `vscode-over-relay-plan.md`; **V0 gates everything** (flag
matrix for `code serve-web` / `openvscode-server` under
`--server-base-path=/n/<id>/editor`, service-worker/IndexedDB behavior through
the relay, connection-token mechanics — go/no-go on the canonical-base-path
design). V1 is the only relay change (cookie key auth) + the daemon editor
proxy; V2 launcher/tickets; V3 client wiring. This is also the universal
remote-editing path for non-relayed hosts without SSH, so it upgrades the
currently disabled button everywhere. Start after R4 unless the editor is
needed sooner — they don't conflict.

### R5 — relay hardening & productization (P2, itemized — pick per need)

- **Decision gate: splice-point confidentiality** — relay/gateway see stream
  plaintext today. Accept for personal deployment vs build app-layer crypto
  (WebCrypto keyed at enrollment). Update security-followups.md either way.
  The web editor (V-track) raises the value of this exposure — revisit when
  V-track ships.
- Per-owner/per-node relay ACLs, key rotation, rate limiting on `/register`
  and auth failures (overlaps SEC-3/SEC-7).
- Pairing-code enrollment brokered through the relay (replaces token paste;
  also listed as an architecture.md open decision).
- Ops: deployment.md relay section (systemd, TLS, 443), metrics/log surface,
  `--version`/health endpoint.
- Client polish: relay health row, per-node latency hint, reconnect toasts.

### MVP-RICH — rich output pipeline (P2)

Workstream 4 of the MVP plan is essentially unbuilt (no CodeMirror, Marked,
DOMPurify, Shiki, Mermaid, KaTeX in `client/package.json`; diffs render
plain). Scope: read-only file viewer + proper diff viewer, repo markdown
preview, agent transcript preview via plugin parsers, session summary
renderer, sanitization boundary, lazy loading. High user-visible value;
independent of R/M/V tracks; blocked only on DEC-1 (don't pull in a component
stack before deciding whether the plan's baseline still stands).

### MVP-HOOKS — workspace setup hooks (P3)

Copy rules (`.env` and friends), symlink rules (dependency caches), bootstrap
command execution, hook status surfaced in the UI. Makes worktree isolation
practical for real projects whose builds need untracked local files.

### MVP-CKPT — checkpoints + "New segment" (P3)

App-managed checkpoint refs/objects, manual checkpoint update, explicit "New
segment" action, optional plugin boundary-detection hook. No checkpoint code
exists in the daemon today.

### M5 — Windows (P3)

ConPTY via portable_pty, AF_UNIX-or-named-pipe transport (tokio has no AF_UNIX
on Windows), `0600` → owner-only ACL. Note the R-track deliberately added no
new UDS surface, so M5 scope did not grow. Prerequisite for the MVP gate's
"works on Windows" and for MVP-PKG's Windows packaging.

### MVP-PKG — Electron + packaging (P3)

Electron shell (renderer hardening checklist), daemon + client packages for
three OSes, first-run setup, user-scoped service install (systemd user unit /
LaunchAgent / Windows per-user startup — remember the holder must live outside
the daemon's cgroup: `systemd-run --user --scope`), upgrade path, diagnostics
export, quickstart docs. Gated on DEC-1; the Windows leg on M5.

### SEC-3..8 — remaining security follow-ups (P2/P3)

See `security-followups.md` for full guidance; keep that doc and this table in
sync as items land. SEC-5 (CORS) interacts with the V1 cookie design — decide
them together. SEC-9 (danger toggles) and SEC-10 (usage endpoint outbound
call) are accepted-as-designed; revisit only if a host-policy/no-egress mode
is added.

### DEC-1 — client-stack decision (P2, cheap, unblocks two items)

The MVP plan's frontend baseline (shadcn/ui, Tailwind, Dockview, CodeMirror,
Electron) was never adopted — the shipped client is plain React 19 + Vite +
xterm.js and has gone quite far on that footing. Decide: adopt the planned
stack incrementally (starting with MVP-RICH's CodeMirror), or amend
`mvp-execution-plan.md` to match reality. Blocks MVP-RICH and MVP-PKG from
starting with confidence.

### DOC-1 — stale relay-framing line (P3, one-liner)

`architecture.md` → Open Decisions still says the relay stream-multiplexing
default is yamux with dial-out as fallback. R1 locked **dial-out-per-stream**
(recorded in connectivity-execution-plan.md); fix the architecture.md line.

### I18N-2 — additional locales (P4)

Deferred by explicit user choice. Infrastructure is ready (`check-locales.mjs`
parity gate, typed keys); adding a locale is the 3-step recipe in `i18n.md`.

## Suggested order (cross-track)

1. **MOB** phases 1–3 — freshest design, client-only, immediately usable
   value on top of the just-shipped relay path; runs in parallel with any
   daemon work.
2. **R4** — finishes the connectivity story the product is built around.
3. **M4** — durability hardening; the only remaining gap in the headline
   restart promise. Can interleave with R4 (different subsystems).
4. **SEC-2**, then **SEC-1(direct)** — before any exposure beyond trusted
   networks.
5. **DEC-1** (an hour of thought) → **MVP-RICH**.
6. **V0** spike → V1–V3 as a block.
7. **MOB-PWA**, then **MOB-PUSH** (design the push plumbing first).
8. **R5** items as deployment needs them (TLS/ops first, ACLs next,
   E2E-crypto decision when V-track ships).
9. **MVP-HOOKS**, **MVP-CKPT** opportunistically.
10. **M5** → **MVP-PKG** when cross-platform/packaging becomes the goal.

Constraints to respect when reordering: nothing in R-track may assume M4
features exist; V1's relay change must not add daemon-API parsing to the
relay; all new client strings go through `en.json` (verify `client/src/i18n/`
exists on the working branch before starting client work).
