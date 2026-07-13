# Security Follow-Ups (Hardening Backlog)

Next-step guidance for security gaps that are **known and accepted for the
current MVP** but must be addressed before this is exposed on untrusted
networks or multi-user hosts. Ordered roughly by priority. Keep this list in
sync as items land.

The MVP already discloses (see `architecture.md` → Security Model): terminal
logs can contain secrets, logs live on the daemon host, retention is
conservative, and encryption-at-rest + production redaction are deferred. The
items below are additional, tracked here so we don't forget.

## 1. No transport encryption off loopback — direct AND relay paths — HIGH

- **What:** off-loopback HTTP + WebSocket is plaintext. Device bearer tokens,
  terminal input/output, and diffs travel unencrypted over the network. This
  applies to **both** exposure paths: a direct off-loopback bind, and the relay
  path (the product path) — the browser↔relay and daemon↔relay hops both carry
  the device token and full terminal I/O.
- **Current mitigation:** loopback-trust means the recommended remote path is an
  **SSH local port-forward**, which encrypts the channel. Direct off-loopback
  bind is opt-in and documented as untrusted-network-unsafe.
- **Relay path is plaintext today, with an unbuilt code prerequisite.** The
  design (`connectivity-execution-plan.md`) has the relay listen on rustls TLS
  via `ASM_RELAY_TLS_CERT` / `ASM_RELAY_TLS_KEY`, but the relay binary does not
  read those yet — `RelayConfig` is `bind` + `keys` only and `run()` binds a
  plain `TcpListener`. Worse, the daemon's relay agent depends on
  `tokio-tungstenite` with **no TLS feature enabled**, so it can only dial
  `ws://`; a `wss://` URL fails at runtime. Relay TLS is therefore not merely an
  ops/cert task: **a TLS-terminating reverse proxy in front of the relay is
  useless until the agent can speak `wss://`**, because the daemon dials the
  relay outbound from behind NAT and must itself make the TLS connection.
- **Guidance:**
  - Enable a TLS feature (e.g. `rustls-tls-webpki-roots`) on
    `tokio-tungstenite` and wire the connector so the daemon agent can dial
    `wss://`. This is the blocker — do it first.
  - Implement relay-side TLS (`ASM_RELAY_TLS_CERT/KEY`, rustls) or terminate TLS
    at a reverse proxy in front of the relay.
  - Use a **real ACME cert** on the relay, not self-signed, so there is **no
    client-facing UX change** (no browser certificate warning). Because the
    daemon stays loopback-bound, it needs no cert of its own on the relay path.
  - For **direct mode**, implement the Phase 8 "TLS/mTLS or equivalent"
    deliverable — self-signed cert + client pinning, or an ACME path; consider
    mTLS so the device token isn't the only credential on the wire.
  - Until all of this lands, keep the SSH-tunnel recommendation prominent.

## 2. `/api/fs/list` exposes the whole host filesystem — HIGH

- **What:** the directory-picker endpoint lets any loopback client or enrolled
  device browse the daemon host's directory tree (directories only, but
  arbitrary paths). Combined with workspace registration (any client can
  register any root) and `custom_command`, this is broad host access.
- **Guidance:** constrain browsing + workspace registration to a server-side
  configured set of allowed roots that a client cannot expand without host-side
  approval; treat "browse anywhere" as an explicit, host-granted capability.
  Enforce the workspace allowlist for the picker, not just for raw-cwd sessions.
- **Related (image preview):** `GET /api/sessions/:id/scm/file` serves a changed
  file's raw bytes so the diff panel can show image previews. It is deliberately
  narrow: `guard_path` blocks `..`/absolute paths, the working-tree read is
  canonicalized and confined to the session `cwd` (a repo symlink pointing
  outside is refused), and only bytes that magic-sniff as PNG/JPEG/GIF/WebP are
  returned. The residual exposure is the same as the picker's: `cwd` itself is
  host-chosen, so the allowed-roots work above also bounds what this endpoint can
  ever reach.

## 3. Enrollment token is a static, non-expiring shared secret — MEDIUM

- **What:** one long-lived enrollment token mints device tokens for anyone who
  can reach `POST /api/auth/enroll` with it. No expiry, rotation, or use limit.
- **Guidance:** one-time / short-TTL enrollment codes, an owner-approval step
  for new devices, token rotation, and rate-limiting on the enroll endpoint.

## 4. Tokens stored in plaintext at rest — MEDIUM

- **What:** device tokens and the enrollment token are stored as plaintext in
  SQLite. DB read = full account takeover.
- **Guidance:** store only a hash of device tokens (compare hash on auth);
  fold into the broader encryption-at-rest work; add secret redaction for
  terminal logs and summaries.

## 5. Permissive CORS — MEDIUM

- **What:** the daemon uses `CorsLayer::permissive()` (any origin). Risk is
  limited because auth uses bearer tokens (not auto-sent cookies), but any web
  page can call the API and would succeed if it obtained a token.
- **Guidance:** restrict allowed origins to the configured client origin(s);
  never enable credentialed CORS; keep tokens out of URLs where avoidable
  (the WS `?access_token=` is a pragmatic exception — scope/short-TTL it later).

## 6. Loopback is fully trusted — MEDIUM (context-dependent)

- **What:** any process able to originate a loopback connection gets full,
  tokenless access. Fine for a single-user personal host; broad on shared or
  multi-user machines.
- **Guidance:** offer an optional "always require a token" mode that disables
  loopback trust; document the multi-user caveat.

## 7. No auth rate-limiting / lifecycle audit log — LOW

- **What:** enroll and token checks aren't rate-limited; lifecycle audit events
  (create/attach/input/stop/delete) listed in the docs aren't emitted yet.
- **Guidance:** add rate-limiting on `/api/auth/*`; emit and persist the
  lifecycle audit events; surface them in diagnostics.

## 8. Terminal-escape policy is client-side only — LOW

- **What:** OSC 52 / OSC 8 / title-sequence policy is planned at the xterm.js
  layer; the daemon currently stores and replays raw bytes without a
  capture-side escape filter, and the parser hasn't been fuzzed.
- **Guidance:** enforce dangerous-sequence policy at capture/replay too, and
  add the hostile-escape fuzz tests called for in the plan.

## 9. Agent permission-skipping toggles — LOW (by design, user-gated)

- **What:** the new-session dialog exposes per-agent "danger" toggles that
  inject guardrail-disabling flags (`claude --dangerously-skip-permissions`,
  `codex --dangerously-bypass-approvals-and-sandbox`). When enabled, the agent
  edits files and runs commands with no approval prompt and (for codex) no
  sandbox, inside the session's worktree/cwd on the daemon host.
- **Current mitigation:** off by default; each toggle is opt-in per session,
  rendered with a "dangerous" affordance, and the exact flag is persisted in the
  session's `args` (auditable). Sessions started this way carry a persisted
  `risky` flag (schema v4) and are surfaced with an **⚠ UNSAFE badge** in the
  session list plus a warning banner in the details panel. Isolation still comes
  from the per-session Git worktree.
- **Guidance:** consider a host-level policy to disable these toggles (env/config
  allowlist) and fold their use into the lifecycle audit log (item 7).
  Re-evaluate once the worktree is the only isolation boundary (a bypassed
  sandbox can still reach anything the daemon user can).

## 10. Usage endpoint reads the Claude OAuth token and calls out — LOW

- **What:** `GET /api/sessions/:id/usage` for Claude sessions reads the CLI's
  own `~/.claude/.credentials.json` and makes an outbound HTTPS call to
  `api.anthropic.com/api/oauth/usage` (the same call Claude Code's `/usage`
  makes) to report account-wide rate-limit windows. The token never leaves the
  daemon or appears in responses/logs; results are cached ~30s.
- **Current mitigation:** best-effort and read-only — no token refresh, no
  writes to the credentials file; a missing/expired token just omits the
  rate-limit rows.
- **Guidance:** if a "no outbound network" deployment mode is added, gate this
  fetch behind it.

## 11. Gateway→downstream trust relies on a non-loopback hop — LOW (topology-dependent)

- **What:** a downstream reached through a gateway (R4) is dialed by the gateway
  at its ordinary listener, so the downstream makes its own loopback-trust
  decision from the **gateway's** source address. In the intended deployment the
  gateway reaches the downstream over the private network — a non-loopback hop —
  so the downstream enforces its device token on relayed traffic and the R2
  invariant ("relayed traffic never inherits loopback trust") holds by topology.
  A downstream **co-located on the gateway host** (a loopback hop) would instead
  grant loopback trust to gatewayed traffic, i.e. tokenless access for any client
  that can reach the gateway through the relay. (This is also why
  `scripts/gateway-test.mjs`, which emulates the gateway and downstream on
  127.0.0.x, cannot assert downstream token enforcement across the hop — it
  proves the relay-key gate instead, and documents the caveat inline.)
- **Current mitigation:** the gateway model exists for **egress-less**
  downstreams (the downstream cannot reach the relay while the gateway can),
  which implies a network boundary between them; co-location is a degenerate
  configuration.
- **Guidance:** ship the item-6 "always require a token" mode and recommend it on
  any downstream that shares a host (or loopback range) with its gateway; fold
  into item 1's off-loopback story.
