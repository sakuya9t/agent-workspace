# Connectivity Execution Plan: Relay + Gateway ("R-track")

Status: **R1–R4 implemented (2026-07-05 → 07-07); R5 (hardening) pending.** A
NAT'd leaf and an egress-less downstream behind a gateway are both fully
controllable from the browser with zero client tooling. The connectivity model —
contract, topology cases, phases, auth, client flow — is specified in
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

The `/register` socket is the **control stream** (text JSON frames). Data
streams are separate outbound WS connections (see below) — no in-connection
multiplexing.

- **Control stream** (`/register`) — JSON Lines, one object per line:
  - node → relay `{"t":"hello","proto":1,"node_id":"…","label":"…",
    "downstreams":[{"node_id":"…","label":"…","reachable":true}]}`
    (first frame; relay closes with an error line + WS close on proto or key
    mismatch — same `node_id` reconnecting supersedes the old registration,
    takeover semantics like asmux's single-attacher, guarded by a generation
    counter so the stale loop can't clobber the new entry)
  - node → relay `{"t":"downstreams","downstreams":[…]}` (replace-set update,
    sent whenever a downstream probe result changes)
  - node → relay `{"t":"ping","seq":n}` every **15 s**; relay replies
    `{"t":"pong","seq":n}`. Relay marks the node offline after **45 s**
    without any control traffic; node treats a missing pong the same way and
    reconnects. (JSON ping is authoritative liveness; WS ping frames are not
    relied on.)
  - relay → node `{"t":"open","stream_id":"<uuid>","target":"<node_id>"}` —
    asks the node to dial back a data stream for one waiting client. `target`
    is the node itself or one of its advertised downstreams.
- **Data streams** (`/data?relay_key=…&stream_id=<uuid>`) — the node dials one
  fresh outbound WSS per `open`. The relay pairs it back to the waiting client
  by `stream_id` and splices; the node resolves the target and
  `copy_bidirectional`s the WS (as a byte duplex) to the target's TCP socket.
  Streams are independent connections — a stalled one cannot block another.
  There is no in-stream preamble; the target came in the `open` message.

Reconnect (node side): exponential backoff 1 s → 60 s cap, ±20 % jitter,
reset after 60 s of stable connection. This is R-track code; it does not reuse
or wait for the M4 asmux watchdog.

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
  routing: mint a stream_id, send {"open",stream_id,target} to the owning
           node's control stream (direct node = itself; advertised downstream
           = its gateway), await the matching /data dial-back (10s timeout ->
           502), then forward over it. relay_key is stripped from the query
           before forwarding; Authorization is preserved.
```

Error bodies (client maps these to distinct UI states):

```text
401 {"error":"relay_unauthorized"}     bad/missing relay key
404 {"error":"unknown_node"}           node_id never registered
502 {"error":"node_offline"}           registered, connection currently down
502 {"error":"downstream_unreachable"} gateway up, its probe of D failing
```

Proxying details (relay side): strip `/n/<node_id>` prefix; forward via
`hyper` http1 client handshake **over the dial-back data WS** (adapted to a
byte duplex); stream request and response bodies (no buffering — terminal WS
and SSE-like flows must not stall). For `Upgrade: websocket` requests use
`hyper::upgrade::on` on both legs and then `copy_bidirectional`. Host header is
left as the relay's host — the daemon ignores Host. CORS: `CorsLayer::permissive`
(mirroring the daemon's stance), which admits the `X-ASM-Relay-Key` and
`Authorization` headers.

TLS: the relay binary optionally terminates TLS natively
(`ASM_RELAY_TLS_CERT` / `ASM_RELAY_TLS_KEY`, rustls); otherwise it binds
plain HTTP for dev or behind a TLS-terminating reverse proxy. Production
guidance (deployment.md addition, R5): real certificate, port 443.

> **Status (not yet implemented — SEC-1).** As of this writing the relay TLS
> path above is *design only*. The relay binary does not read
> `ASM_RELAY_TLS_CERT/KEY` — `RelayConfig` is `bind` + `keys` only and `run()`
> binds a plain `TcpListener`. And the daemon's relay agent pulls in
> `tokio-tungstenite` with **no TLS feature**, so `connect_async` can only dial
> `ws://`; the `wss://relay.example.com` form below fails at runtime. This is a
> **code prerequisite, not just ops**: a TLS-terminating reverse proxy in front
> of the relay is useless until the agent can speak `wss://`, because the daemon
> dials outbound from behind NAT and must make the TLS connection itself. Order
> of work: enable `rustls-tls-webpki-roots` on `tokio-tungstenite` + wire the
> connector, then relay-side rustls or a proxy, with a real ACME cert (no client
> UX change). Tracked in `security-followups.md` → 1 and `backlog.md` → SEC-1.
>
> **Update (2026-07-12): this was implemented in full and then reverted.**
> `1dcb15e` delivered exactly the order of work above (agent `wss://` connector,
> relay rustls with ALPN pinned to `http/1.1`, plaintext-to-remote-relay refusal,
> tests in `crates/asm-relay/tests/tls.rs`), riding along with daemon-terminated
> HTTPS — which turned out to make a LAN daemon *unreachable* from the web
> client (browsers refuse a self-signed cert on a cross-origin `fetch`) and was
> reverted wholesale to keep the LAN journey plaintext by design (`a36fdfa`;
> see `security-followups.md` → 1). The relay half was sound: when R5 needs it,
> **resurrect it from `1dcb15e`** instead of rebuilding.

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
  ASM_RELAY_TLS_CERT / ASM_RELAY_TLS_KEY   optional rustls (NOT IMPLEMENTED — SEC-1)

asm-daemon additions:
  ASM_RELAY_URL         e.g. ws://relay.example.com — presence enables the
                        registration task (R2). NOTE: wss:// not supported yet
                        (agent tokio-tungstenite has no TLS feature — SEC-1)
  ASM_RELAY_KEY         relay access key
  ASM_NODE_LABEL        default: hostname
  ASM_RELAY_DOWNSTREAMS comma-separated host:port targets on the private net
                        (R4; labels/node_ids discovered by probing /health)
  ASM_RELAY_PROBE_INTERVAL_MS  downstream /health re-probe cadence (R4; default 5000)
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

- **R1 — `asm-relay` core (standalone; no daemon changes). _Done
  2026-07-05._** New crate `crates/asm-relay` (workspace member): axum server
  with `/register` (control WSS: hello/ping/downstreams, same-`node_id`
  takeover via a generation guard), `/data` (dial-back handoff by `stream_id`),
  `/nodes`, and the `/n/<node_id>/*` proxy (hyper http1 over the data WS + WS
  upgrade splice), relay-key auth on every route, permissive CORS, liveness
  bookkeeping, frozen error bodies. `transport.rs` is the WS↔byte-duplex
  adapter (serves both axum and tungstenite `Message`). The **lib half**
  exports the protocol types/consts and the reusable node-side `agent` task
  (register, backoff/reconnect, serve dial-back streams to a configurable local
  TCP target). *Landed:* dial-out-per-stream replaced yamux (see Stream
  mechanism above). *Acceptance (met):* `cargo test -p asm-relay tests/e2e.rs`
  — relay on an ephemeral loopback port + a fake node (the lib agent → an axum
  hello/echo server); asserts (1) `/nodes` online, (2) `GET /n/<id>/…`
  round-trips, (3) WS echo through `/n/<id>/…`, (4) wrong/missing key ⇒ 401,
  unknown node ⇒ 404, killed agent ⇒ offline + 502, (5) a stalled stream does
  not block a concurrent one. Clippy clean; workspace warning-free.

- **R2 — daemon register-out mode + tunnel listener. _Done 2026-07-05._**
  Daemon consumes the R1 lib agent behind `ASM_RELAY_URL`/`ASM_RELAY_KEY`/
  `ASM_NODE_LABEL` (`start_relay_if_configured` in `main.rs`). The **tunnel
  listener** is a second axum serve of the same router on an ephemeral loopback
  TCP port, stamped `ListenerKind::Tunnel` via an `Extension` layer. The one
  trust decision lives in `require_auth` (`trusted = peer_is_loopback && kind
  != Tunnel`), which stamps `LoopbackTrust(bool)`; the `/api/auth/enrollment-
  token` handler now reads `LoopbackTrust` instead of `ConnectInfo` — closing
  the review finding where a relayed caller with a token could read the
  enrollment token. `/health` gained `node_id` (= `server_id`) and `label`;
  hello sends `server_id`. *Acceptance (met):* `scripts/relay-test.mjs`
  (self-contained): (1) node online in `/nodes`, (2) enrollment **through the
  relay** yields a device token, (3) full loop through `/n/<id>` — create, WS
  attach + marker echo, stop, (4) a relayed request **without** a token ⇒ 401
  though the hop is loopback (the loopback-trust regression), (5) `GET
  /api/auth/enrollment-token` through the relay with a valid token ⇒ 403, (6)
  relay restart → daemon re-registers → API works again. All 11 checks pass;
  50 daemon tests + relay suite green.

  **Milestone reached:** a private daemon reachable only by dialing out is
  fully controllable — HTTP + live terminal WS — through the relay.

- **R3 — client relay support. _Done 2026-07-05._**
  `RelayConn` store (persisted `asm.relays`, cascade-remove) + `relayKey`/`via`
  on `DaemonConn`/`Target`; `req`/`enrollDevice`/`probeHealth` send
  `X-ASM-Relay-Key`; `streamUrl` now preserves the `/n/<id>` path prefix and
  adds `relay_key` (fixing the drop-path bug); `listRelayNodes` discovery;
  ConnectionDialog relay section (add relay, discovered-node list with
  liveness, per-node connect via token paste, remove); all strings via en.json;
  relayed traffic never gets loopback trust. Also added an OPTIONS bypass to the
  relay's key middleware so a cross-origin browser's CORS preflight is not
  blocked. *Acceptance (met):* `npm run build` green (tsc + eslint +
  check-locales + vite). A headless-Chrome CDP harness (scratchpad
  `r3-browser.mjs`) loads the built client from a daemon origin and drives a
  node ONLY through the relay, cross-origin: discovery (CORS preflight for the
  custom header), enroll-through-relay, session list (`req` path), create +
  terminal-WS marker echo (`streamUrl` path); then seeds a relayed `DaemonConn`
  and confirms the **real client bundle** renders it in the tree. All 7 checks
  pass. **Milestone reached: the browser client reaches a relayed (NAT'd) node
  with zero client-side tooling — works on any client including mobile.**

- **R4 — gateway mode (egress-less downstreams). _Done 2026-07-07._**
  Daemon parses `ASM_RELAY_DOWNSTREAMS` (comma-separated `host:port`) and runs a
  probe loop that GETs each target's `/health` for `node_id`/`label` and tracks
  reachability. **Design choice:** the probe lives in the daemon (blocking `ureq`
  on a `spawn_blocking`, cadence `ASM_RELAY_PROBE_INTERVAL_MS`, default 5 s) and
  publishes the resolved, identity-annotated set to the relay agent over a
  `tokio::sync::watch` channel — so the reusable `asm-relay` lib gains no HTTP
  client and `/health`-shape knowledge stays a daemon concern. The agent
  advertises the current set in its `hello` and re-sends `NodeMsg::Downstreams`
  whenever the watch changes, and `resolve()`s an `Open{target}` naming a
  downstream by dialing that `host:port` instead of the tunnel listener. A
  downstream that has answered once stays advertised as `reachable:false` when a
  later probe fails (transient outage ⇒ offline, not vanished). Relay side was
  already R4-ready from R1: `route()` finds the owning gateway, `snapshot()`
  emits each downstream as a `via`-attributed leaf, and the proxy maps failure to
  `downstream_unreachable`; R4 added a **fast-fail** so a target the gateway last
  probed as unreachable 502s immediately instead of on the 10 s open timeout. The
  client already renders `via` (R3); R4 upgraded it to show the gateway's **label**
  ("D · via C") rather than its id.
  *Acceptance (met):* `scripts/gateway-test.mjs` (self-contained) — relay +
  gateway daemon C (127.0.0.2) + downstream daemon D (127.0.0.3), client hits
  **only** the relay: (1) C online as `kind:gateway`; D discovered by C's probe
  and listed as a leaf with `via:C` and its own label; (2) full session loop
  against D through `/n/<D_id>` — enroll, create, WS marker echo, list; (3)
  isolation — D's session lists on D but **not** on C (distinct daemons, both
  driven through the one relay); (4) the relay key still gates downstream routing
  (no key ⇒ 401); (5) stopping D flips it to offline / `downstream_unreachable`
  while C stays online. All 15 checks pass. **Loopback caveat:** because C and D
  are emulated on 127.0.0.x, D sees a loopback peer and grants loopback trust, so
  cross-gateway *token* enforcement (which holds in production, where C→D is a
  real network hop) cannot be reproduced and is not asserted — recorded as
  `security-followups.md` → 11.

  **Milestone reached:** an egress-less downstream (a host that cannot reach the
  relay at all) is fully controllable through a gateway that bridges it.

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
