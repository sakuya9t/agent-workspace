# Security Follow-Ups (Hardening Backlog)

Next-step guidance for security gaps that are **known and accepted for the
current MVP** but must be addressed before this is exposed on untrusted
networks or multi-user hosts. Ordered roughly by priority. Keep this list in
sync as items land.

The MVP already discloses (see `architecture.md` → Security Model): terminal
logs can contain secrets, logs live on the daemon host, retention is
conservative, and encryption-at-rest + production redaction are deferred. The
items below are additional, tracked here so we don't forget.

## 1. Transport encryption — DONE (relay + daemon) — LOW (residual)

**Status (2026-07-12): the relay path — the product path — is encrypted end to
end.** What landed:

- **The agent can dial `wss://`.** `tokio-tungstenite` carries the
  `rustls-tls-webpki-roots` feature and the agent connects through an explicit
  rustls connector (`asm-relay/src/tls.rs`). This was the blocker: the daemon
  dials the relay *outbound* from behind NAT, so it is itself a TLS client, and
  a TLS-terminating proxy in front of the relay would have locked the daemon out
  rather than secured it.
- **The relay serves TLS.** `ASM_RELAY_TLS_CERT` / `ASM_RELAY_TLS_KEY` (rustls,
  ALPN pinned to `http/1.1` so the WebSocket upgrade survives). With TLS on there
  is **no cleartext port to fall back to** — a plaintext client fails the
  handshake. Setting only one of the two is a hard error, so a typo cannot
  silently downgrade the relay. `ASM_RELAY_HSTS=1` covers the
  proxy-terminated deployment.
- **Enforced where it must be, surfaced where it shouldn't be.** The daemon
  *refuses* a plaintext `ASM_RELAY_URL` to a remote host
  (`ASM_ALLOW_INSECURE_RELAY=1` overrides) — a plaintext relay is never
  deliberate and `wss://` is free. An off-loopback `ASM_BIND` *without* a
  certificate is only *warned* about: it is a deliberate choice, refusing it
  broke every LAN and container deployment that had legitimately made it, and a
  certificate is now available for anyone who wants the encrypted version. The
  web client likewise flags a plaintext daemon/relay URL as unencrypted without
  blocking it. Loopback is exempt throughout: `http://localhost` is the local
  daemon and the far end of an SSH forward, and browsers already treat it as a
  secure context.
- **Covered by `crates/asm-relay/tests/tls.rs`:** the agent registers over
  `wss://`, HTTPS *and* WSS proxy through to the node, a plaintext client on the
  TLS port is refused, and an untrusted certificate is rejected rather than
  waved through.

Use a **real ACME cert** on the relay. A self-signed one works — point
`ASM_RELAY_CA` at it so the daemon trusts it — but the *browser* has no such
escape hatch and will show a certificate warning.

### The daemon's own HTTPS does NOT secure the LAN journey (2026-07-12, corrected same day)

`ASM_TLS_CERT` / `ASM_TLS_KEY` make the daemon serve `https://host:4600`. That
was claimed here as "the direct LAN path is encrypted like any other". **It is
not, and it cannot be, for the journey the product actually has.** The correction
matters more than the feature:

- A LAN daemon is reached **by IP**. No public CA will certify `192.168.x.x`, so
  its certificate is necessarily **self-signed** (or privately signed, which is
  the same thing to a browser that lacks the CA).
- The client's connection to a daemon is a **cross-origin `fetch`** — the user
  adds `https://192.168.0.159:4600` in the Connections dialog from whatever page
  they are on (a client served by another host, a dev server, another daemon).
- A browser refuses an untrusted certificate on a cross-origin fetch **with no
  interstitial and no API to accept it**. It surfaces as an opaque `TypeError`,
  indistinguishable from a dead host. There is nothing any client code can do.

So daemon-terminated TLS is **unreachable by construction** in the one journey it
was added to protect. The certificate interstitial that `tls.rs` reasons about
("the user could never click through") only exists for a *top-level navigation* —
i.e. only if the daemon is also the origin serving the page, which is not the
supported journey. Turning it on silently breaks the LAN client; that is how it
was found.

**Where daemon HTTPS is still right:** a daemon behind a reverse proxy or on a
host with a **real name and a publicly-trusted cert** (then the browser trusts it
and nothing changes for the user), and the daemon→relay hop, where the daemon is
a TLS *client* and can be told to trust a private CA (`ASM_RELAY_CA`). Keep the
feature; stop presenting it as the LAN answer.

**The corrected approach — encryption must terminate where a browser already
trusts, which on a LAN means a NAME, not an IP:**

1. **LAN direct = plaintext, deliberately.** The user chose their network's trust
   boundary by binding off-loopback. Warn, never break. This is the current
   default again.
2. **Anything beyond the LAN = the relay**, which has a real hostname and a real
   ACME cert. The browser sees only that cert, so there is *zero* ceremony and
   the journey is unchanged (add relay URL + key in the UI). This is the product
   path and where encryption genuinely belongs.
3. **Encrypted LAN direct, if wanted:** only via a **publicly-trusted cert for a
   name that resolves to the LAN IP** (ACME DNS-01 against a domain you own; the
   `*.plex.direct` pattern). Then `ASM_TLS_CERT` is exactly right and the CUJ is
   untouched. A self-signed cert is not a cheaper version of this — it is a
   different, broken thing.

Certificate parsing is shared with the relay (`asm_relay::tls`). Two
daemon-specific choices:

- **No HSTS from the daemon.** A daemon is usually reached by IP or a LAN name
  with a self-signed certificate, and HSTS makes the browser interstitial
  *non-bypassable* — the user could never click through, and turning TLS back off
  would lock them out of the host entirely. The relay, which has a real name and
  a real cert, does send it.
- **`ASM_TRUST_LOOPBACK=0`** disables loopback trust. This is mandatory behind a
  same-host reverse proxy: the proxy connects from `127.0.0.1`, so without it
  every request it forwards would be loopback-trusted and the daemon's auth would
  be silently off. (Partially addresses item 6.)

**What remains:**

- **The LAN hop is unencrypted again, and that is the honest state.** The daemon
  runs plaintext off-loopback (warned, not refused). A self-signed cert does not
  fix it — it only breaks the client. Closing this for real needs path 2 (relay
  with an ACME cert, no client tooling, journey unchanged) or path 3 (a named,
  publicly-trusted cert on the daemon). Both need a domain; neither needs the
  user to install anything on their phone.
- **mTLS** is still open: the device token remains the only credential on the
  wire. Worth revisiting if the daemon is ever exposed beyond a LAN.
- **Guardrail worth having:** the daemon should say at startup, when it is given
  a self-signed cert on an off-loopback bind, that browsers will refuse
  cross-origin calls from a client served elsewhere — the failure is otherwise
  silent and reads as "daemon not started".

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
