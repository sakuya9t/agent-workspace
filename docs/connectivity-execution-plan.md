# Connectivity Execution Plan: Relay + Gateway ("R-track")

Status: **design decided, no code yet.** The connectivity model — contract,
topology cases, phases, auth, client flow — is specified in
[`architecture.md`](architecture.md) → *Connectivity* (decision recorded
2026-07-04: ASM owns the relay/gateway path; no third-party overlay). This
document is the implementation plan: concrete wire contracts, crate layout,
env vars, milestones **R1–R5** with acceptance criteria, mirroring how
[`durable-sessions.md`](durable-sessions.md) M1–M5 drove the asmux work.

## Relation to other tracks (read first)

- **Durable-sessions M-track status: M1–M3 done, M4–M5 pending.** M4
  (holder hardening: idle watchdog, reconnect-with-backoff to asmux,
  soft-reboot, exact cold-stitch adopt) and M5 (Windows) have **not** landed.
  Nothing in R1–R5 may assume an M4 feature exists. The R-track's
  relay-reconnect logic (R2) is **its own code path** (daemon ↔ relay over
  WSS), unrelated to the daemon ↔ asmux UDS watchdog deferred to M4.
- **The tracks are orthogonal and can interleave.** The R-track touches the
  daemon only at its network edge (one outbound registration task + one extra
  listener); it never touches `SessionBackend`, asmux, or the adopt path.
  Landing R-track milestones does not close, and is not blocked by, M4/M5.
- **MVP gates:** `mvp-execution-plan.md` lists "production relay/gateway" as
  post-MVP. This plan pulls that forward as a parallel track by explicit
  decision; the MVP gates themselves are unchanged.
- **Windows note:** R-track designs below deliberately avoid new UDS surfaces
  (the tunnel listener is loopback TCP) so M5 does not grow.

## Locked decisions

- ASM **owns** relay + gateway (`architecture.md` decision, 2026-07-04).
- New crate **`crates/asm-relay`** — a lib + bin: the bin is the standalone
  relay server; the lib exports the shared tunnel protocol and the node-side
  **agent task** that the daemon reuses in R2 (mirrors the asmux/asmux-wire
  split: one protocol, two processes).
- Routing is **opaque path-prefix routing**: `/n/<node_id>/...` (HTTP and WS
  upgrade alike) is spliced to the target node as bytes; the relay never
  parses the daemon API.
- **Two independent credentials**: relay access key (relay layer) and device
  token (daemon layer, end-to-end). They never mix; header/param names below.
- **Relayed traffic never inherits loopback trust** — enforced by serving
  tunnel streams on a listener with loopback trust disabled (see R2).
- **`node_id` = the daemon's existing persistent `server_id`** (already
  generated and persisted; already returned by enrollment). No second
  identity. `/health` gains `node_id` + `label` fields (R2) so gateways can
  probe downstreams (R4).
- Stream mechanism: **dial-out-per-stream** (chosen 2026-07-04 during R1). The
  control WSS stays open; for each inbound client connection the relay sends an
  `Open{stream_id,target}` down the control channel and the node dials a fresh
  outbound **data WSS** (`/data?stream_id=…`) which the relay pairs with the
  waiting client and splices. *Why not yamux (the original default):* `yamux
  0.13` is poll-only (no `Control` handle), so multiplexing one WSS is subtle
  single-point-of-failure code; dial-out gives **per-stream isolation for free**
  (each stream is its own WS/TCP, so one stalled stream cannot block another —
  an acceptance criterion becomes true by construction) using only WebSocket
  primitives. Cost — a WS handshake per stream — is negligible for this
  workload (few long-lived terminal streams + occasional polls). The target
  travels in the `Open` message, so there is **no in-stream preamble**.

## Wire contract (frozen once R2 ships end-to-end)

### Registration (node → relay)

```text
GET wss://<relay>/register            (WebSocket upgrade)
  ?relay_key=<key>                    auth (browser-independent path, but nodes
                                      are native — header X-ASM-Relay-Key also
                                      accepted; query wins if both)
```

After upgrade the socket carries **binary yamux frames** (WS binary messages
adapted to a byte stream). Both sides may open streams:

- **Control stream** — the node opens the *first* stream immediately and it is
  control for the life of the connection. JSON Lines, one object per line:
  - node → relay `{"t":"hello","proto":1,"node_id":"…","label":"…",
    "downstreams":[{"node_id":"…","label":"…","reachable":true}]}`
    (first line; relay closes with an error line + WS close on proto or key
    mismatch, or on `node_id` collision with a *different* live connection —
    same `node_id` reconnecting supersedes the old registration, takeover
    semantics like asmux's single-attacher)
  - node → relay `{"t":"downstreams","downstreams":[…]}` (replace-set update,
    sent whenever a downstream probe result changes)
  - node → relay `{"t":"ping","seq":n}` every **15 s**; relay replies
    `{"t":"pong","seq":n}`. Relay marks the node offline after **45 s**
    without any control traffic; node treats a missing pong the same way and
    reconnects. (JSON ping is authoritative liveness; WS ping frames are not
    relied on.)
- **Proxy streams** — opened by the *relay*, one per inbound client
  connection. **Frozen framing rule: every proxy stream — direct and gateway
  alike — begins with exactly one relay-written JSON line**
  `{"target":"<node_id>"}` (extensible object; nodes ignore unknown fields).
  After that line the payload is the raw client bytes (HTTP/1.1 request or
  upgraded WS byte stream) with the `/n/<node_id>` prefix already stripped
  by the relay. The node reads the preamble, dials the matching target — its
  own tunnel listener when `target` is its own `node_id`, else the advertised
  downstream — then goes byte-blind and copies until either side closes
  (`copy_bidirectional`). One rule, no per-kind variants: R1 tests the
  preamble on direct streams, R4 on downstream streams.

Reconnect (node side): exponential backoff 1 s → 60 s cap, ±20 % jitter,
reset after 60 s of stable connection. This is R-track code; do not reuse or
wait for the M4 asmux watchdog.

### Client-facing surface (relay)

```text
GET /nodes                            list registered nodes
  auth: X-ASM-Relay-Key header (HTTP) or ?relay_key= (accepted on any route;
        required because browsers cannot set WS headers)
  200: {"nodes":[{"node_id":"…","label":"…","kind":"leaf"|"gateway",
                  "via":"<gateway node_id>"|null,
                  "online":true,"last_seen":"<iso8601>"}]}

ANY /n/<node_id>/<rest>               opaque proxy (HTTP + WS upgrade)
  auth: relay key as above; Authorization header passes through UNTOUCHED
        (it carries the daemon device token, end-to-end)
  routing: direct node -> its registration connection;
           advertised downstream -> the owning gateway's connection.
           Either way the proxy stream starts with the uniform one-line
           {"target":"<node_id>"} preamble (see Registration) telling the
           node which target to dial.
```

Error bodies (client maps these to distinct UI states):

```text
401 {"error":"relay_unauthorized"}     bad/missing relay key
404 {"error":"unknown_node"}           node_id never registered
502 {"error":"node_offline"}           registered, connection currently down
502 {"error":"downstream_unreachable"} gateway up, its probe of D failing
```

Proxying details (relay side): strip `/n/<node_id>` prefix; forward via
`hyper` http1 client handshake **over the yamux stream**; stream request and
response bodies (no buffering — terminal WS and SSE-like flows must not
stall). For `Upgrade: websocket` requests use `hyper::upgrade::on` on both
legs and then `copy_bidirectional`. Host header is left as the relay's host —
the daemon ignores Host. CORS: the relay answers permissive CORS (mirroring
the daemon's stance) and must allow the `X-ASM-Relay-Key` and `Authorization`
headers, because the client is served from a daemon origin, not the relay.

TLS: the relay binary optionally terminates TLS natively
(`ASM_RELAY_TLS_CERT` / `ASM_RELAY_TLS_KEY`, rustls); otherwise it binds
plain HTTP for dev or behind a TLS-terminating reverse proxy. Production
guidance (deployment.md addition, R5): real certificate, port 443.

### Daemon-side auth interaction (existing conventions, unchanged)

- HTTP: `Authorization: Bearer <device_token>` (`crates/daemon/src/auth.rs`).
- WS: `?access_token=<device_token>` (browsers cannot set WS headers) —
  already implemented; passes through the relay untouched.
- **Verified, no daemon change needed:** `/api/auth/enroll` and
  `/api/auth/status` are public at the daemon layer (`is_public` in
  `auth.rs`), and `/health` is outside `/api` — so relayed enrollment (R2
  acceptance) and the gateway's downstream `/health` probe (R4) work through
  the tunnel as-is, gated only by the relay key at the outer layer. Brute
  force against the public enroll endpoint through the relay is the same
  exposure as direct LAN today (32-char CSPRNG enrollment token) plus the
  relay key; R5's rate limiting on auth failures covers it.
- Loopback trust is consulted in **two places today, not one**: the auth
  middleware (`crates/daemon/src/auth.rs` → `peer_is_loopback`) **and a
  handler-level check inside `/api/auth/enrollment-token`**
  (`crates/daemon/src/api/auth.rs`, its own `ConnectInfo…is_loopback()`).
  The tunnel listener must defeat **both**: per-listener trust becomes a
  single shared attribute (e.g. an Extension/state flag stamped where each
  listener is served) and **every** loopback check consults that attribute,
  never the raw peer address — the tunnel agent dials the tunnel listener
  over *real* loopback, so any `ConnectInfo`-direct check silently trusts
  relayed traffic. Before closing R2, `grep -rn is_loopback crates/daemon/`
  and route every caller through the shared helper; a handler reading
  `ConnectInfo` directly is a security bug.

## Environment / config surface

```text
asm-relay (bin):
  ASM_RELAY_BIND        default 127.0.0.1:4700 (0.0.0.0:443 in production;
                        note: binding 0.0.0.0 is permission-denied for agents
                        in this dev environment — tests always bind loopback)
  ASM_RELAY_KEYS        comma-separated accepted access keys (MVP: one)
  ASM_RELAY_TLS_CERT / ASM_RELAY_TLS_KEY   optional rustls

asm-daemon additions:
  ASM_RELAY_URL         e.g. wss://relay.example.com — presence enables the
                        registration task (R2)
  ASM_RELAY_KEY         relay access key
  ASM_NODE_LABEL        default: hostname
  ASM_RELAY_DOWNSTREAMS comma-separated host:port targets on the private net
                        (R4; labels/node_ids discovered by probing /health)
```

## Client contract (R3)

- `connectionStore.ts`: new `RelayConn { id, url, accessKey, label }`
  persisted under `asm.relays`; `DaemonConn` gains `via?: string` (relay id).
  `Target` gains `relayKey?: string`, resolved by `targetOf` when `via` is
  set.
- `api.ts`: `req()` adds `X-ASM-Relay-Key` when `target.relayKey` — **and so
  must the raw-`fetch` helpers that bypass `req()`**: `enrollDevice` and
  `probeHealth` (thread a `relayKey` param or refactor them onto `req()`).
  Without this, enrollment and the connect-time health probe through
  `/n/<id>` fail relay auth and the R3 connect flow cannot work at all.
- `streamUrl` today rebuilds the WS URL from `baseUrl`'s **host only**,
  silently dropping any path — a relayed `baseUrl` like
  `https://relay/n/<id>` would attach to `wss://relay/api/...` and bypass
  the node route. It must preserve the base URL's path prefix and append
  `relay_key=` alongside the existing `access_token=`.
- Discovery: `GET <relay>/nodes` with the key.
- Establishment flow (from `architecture.md` → Client Connection
  Establishment): add relay (URL + key) → list discovered nodes grouped under
  the relay with liveness → per node, paste that node's enrollment token →
  `enrollDevice("https://relay/n/<id>", …)` → store
  `{ baseUrl: "https://relay/n/<id>", token, via }`. Steady state: ordinary
  `DaemonConn`, aggregation unchanged (`useDaemons.ts` untouched except
  `enabled` gating on the relay's reachability).
- Failure states: relay unreachable ⇒ the whole relay group dims (cached data
  retained, existing `keepPreviousData` behavior); `node_offline` /
  `downstream_unreachable` ⇒ only that node dims, with distinct copy.
- **i18n rule applies — with a base-branch check.** The client i18n
  infrastructure (en.json, eslint-plugin-i18next, check-locales wired into
  `npm run build`) lives on `release/next` (commits through `9e4ab58`,
  2026-07-03) and is **not** present on every session branch. Before starting
  R3, verify `client/src/i18n/` exists; if it does not, branch from (or merge
  in) `release/next` first — do **not** hand-roll plain strings on an old
  base and do not treat the missing gate as license to skip it. All new R3
  strings then go through `client/src/i18n/locales/en.json` with typed keys.

## Milestones

Work each milestone to completion (code + tests green + script proof) before
starting the next, matching the M-track cadence. Rust work needs
`source ~/.cargo/env` first.

- **R1 — `asm-relay` core (standalone; no daemon changes).**
  New crate `crates/asm-relay` (workspace member): axum server with
  `/register` (WSS + yamux + control stream, hello/ping/downstreams,
  same-`node_id` takeover), `/nodes`, and the `/n/<node_id>/*` proxy (HTTP
  forward over mux stream + WS upgrade splice + the one-line `{"target"}`
  preamble), relay-key auth on every route, permissive CORS, liveness
  bookkeeping, error bodies exactly as specified. The **lib half** exports
  the protocol types/consts and a reusable node-side `agent` task (register,
  control stream, backoff/reconnect, serve proxy streams by dialing a
  configurable local TCP target).
  *Acceptance:* `cargo test -p asm-relay` includes an in-process e2e: start
  relay on an ephemeral loopback port; start a fake node (the lib agent
  pointed at a local hello-world HTTP server); assert (1) `/nodes` shows it
  online, (2) `GET /n/<id>/…` round-trips through the tunnel — including the
  one-line `{"target"}` preamble on this *direct* stream (the frozen
  always-preamble rule), (3) a WS
  echo upgrade works through `/n/<id>/…`, (4) wrong/missing relay key ⇒ 401,
  unknown node ⇒ 404, killed agent ⇒ offline in `/nodes` and 502
  `node_offline`, (5) a deliberately stalled proxy stream does not block a
  concurrent stream (the mux flow-control guarantee). `cargo build` stays
  warning-free.

- **R2 — daemon register-out mode + tunnel listener.**
  Daemon consumes the R1 lib agent behind `ASM_RELAY_URL`/`ASM_RELAY_KEY`/
  `ASM_NODE_LABEL`. Add the **tunnel listener**: a second axum serve of the
  same router on an ephemeral loopback TCP port whose connections carry a
  per-listener `loopback_trusted = false` attribute. **All** loopback checks
  consult that attribute — both the auth middleware and the handler-level
  check in `/api/auth/enrollment-token` (see the auth-interaction section
  above; grep `is_loopback` before closing) — so relayed requests always
  require a device token and loopback-only endpoints refuse relayed callers
  outright, while the primary listener keeps today's behavior. `/health`
  gains `node_id` (= `server_id`) and `label`. `node_id` sent in hello is
  `server_id`.
  *Acceptance:* new `scripts/relay-test.mjs` (self-contained like
  `durable-restart-test.mjs`): starts asm-relay + a daemon with registration
  enabled on loopback ports; asserts (1) node appears online in `/nodes`,
  (2) enrollment **through the relay** yields a device token, (3) full API
  loop through `/n/<id>` — create session, attach WS, see output, stop —
  against the existing smoke-test flow, (4) a relayed request **without** a
  token is rejected 401 even though the spliced hop is loopback (the
  loopback-trust regression test — security-critical assert), (5) `GET
  /api/auth/enrollment-token` through `/n/<id>` **with a valid device token**
  is refused 403 — enrollment tokens are never retrievable through the relay
  (this pins the handler-level check to the per-listener attribute),
  (6) kill the relay, restart it, daemon re-registers within backoff and the
  loop works again. `cargo test` for both crates green.

- **R3 — client relay support.**
  `RelayConn` store + persistence/migration, ConnectionDialog relay section
  (add relay, discovered-node list with liveness, per-node connect via token
  paste, remove), `via` plumbing in `Target`/`api.ts`/WS URLs, relay grouping
  + the two failure states in the session tree, all strings via en.json.
  *Acceptance:* `npm run build` green (tsc + eslint + check-locales); a CDP
  script (scratchpad, per the established headless-Chrome technique with
  localStorage seeding) drives: add relay → discover → connect node with
  token → sessions appear in the tree → create + attach a session through
  the relay → kill relay → group shows unreachable while cached data
  remains → restart relay → recovers. Loopback-only ports (127.0.0.x)
  since 0.0.0.0 binds are denied in this environment.

- **R4 — gateway mode (egress-less downstreams).**
  Daemon parses `ASM_RELAY_DOWNSTREAMS`; probes each target's `/health` for
  `node_id`/`label` (re-probe on interval; on change or failure, send a
  `downstreams` control update); agent handles proxy streams whose
  `{"target"}` preamble names a downstream by dialing that `host:port`
  instead of the tunnel listener. Relay routes downstream node_ids via the
  owning gateway connection and reports `via` in `/nodes`;
  `downstream_unreachable` surfaced from probe state. Client renders the
  `via` attribution (e.g. "D · via C").
  *Acceptance:* extend `scripts/relay-test.mjs` (or add
  `scripts/gateway-test.mjs`): relay + gateway daemon C + downstream daemon D
  on distinct loopback addresses (127.0.0.1/2/3 — the established technique
  for emulating separate hosts), where the client hits **only** the relay:
  (1) `/nodes` lists D with `via: C`, (2) the full session loop works against
  D through `/n/<D_id>`, (3) D receives C's address as peer (not loopback)
  and enforces its token, (4) stopping D flips it to
  `downstream_unreachable` while C stays online, (5) depth is invisible: the
  client config for D differs from a direct node only in URL.

- **R5 — hardening & productization (scoped items, pick per need).**
  - **Splice-point confidentiality**: relay/gateway processes currently see
    stream plaintext. True end-to-end requires app-layer encryption
    (browser clients cannot pin certs for a nested TLS layer), e.g. a
    WebCrypto-based scheme keyed at enrollment. Decision gate: accept
    relay-sees-plaintext for the personal deployment vs. build app-layer
    crypto for the company/gateway story. Ties to the "no TLS off loopback"
    gap (docs/security-followups.md) — update that doc either way.
  - Per-owner/per-node relay ACLs (which key may route to which node),
    key rotation, basic rate limiting on `/register` and auth failures.
  - Pairing-code enrollment brokered through the relay (replaces token
    paste; architecture.md open decision).
  - Ops: `deployment.md` section for the relay (systemd unit, TLS, 443),
    relay metrics/log surface, `asm-relay --version`/health endpoint.
  - Client polish: relay health row, per-node latency hint, reconnect toasts.

## Testing conventions (all milestones)

- Loopback only: bind 127.0.0.x (0.0.0.0 is permission-denied for agents);
  distinct loopback addresses emulate distinct hosts.
- Self-contained scripts in `scripts/` (Node 24, global WebSocket) that spawn
  every process they need, like `durable-restart-test.mjs`.
- `cargo test` + the daemon smoke scripts must stay green before each commit;
  client changes additionally gate on `npm run build`.
- UI verification: headless Chrome via CDP with localStorage seeding
  (scratchpad `cdp*.mjs` technique).

## Deliberately deferred (do not build in R1–R4)

- App-layer end-to-end encryption (R5 decision gate).
- Multi-owner relay tenancy; account systems.
- Relay-side caching, aggregation, or any parsing of the daemon API.
- Hole-punching / P2P direct connections (relay covers the decided scope).
- Serving the web client from the relay itself (idea only; revisit in R5).
- NAT'd-client cases where A cannot reach the relay (out of scope by
  definition — the relay is the mutually reachable point).
