# "Continue in VS Code" over the relay — design note

Status: **design only, no code.** The immediate correctness fix (disable the
button for relayed hosts) has shipped; this document scopes the feature that
makes editing a relayed node possible. It follows the R-track cadence in
[`connectivity-execution-plan.md`](connectivity-execution-plan.md): chosen
approach, hard problems with decided solutions, milestones with acceptance
criteria. A V0 spike gates the rest — several claims below are marked
*(verify in V0)* because IDE server behavior varies by version.

## Problem

Today "Continue in VS Code" is a `vscode://` deep link fired from the browser
(`client/src/vscode.ts`). A same-origin daemon opens the folder locally
(`vscode://file<path>`); a remote daemon opens Remote-SSH
(`vscode://vscode-remote/ssh-remote+<user@host><path>`). Both need the
*client's* machine to reach the workspace host **directly** — a local path or
an SSH destination.

A relayed node has neither. By design it is reachable **only** through the
relay (`/n/<node_id>/…`); the client is NAT-separated from it and speaks
nothing but HTTPS/WSS to the relay. The old code derived the SSH host from the
profile's `baseUrl`, which for a relay is `https://relay/n/<node_id>` — so it
emitted `ssh-remote+user@<RELAY_HOST>`, telling the user's VS Code to SSH into
the **relay machine** (the wrong host) at a path that only exists on the
**node**. Misdirected, not just non-functional.

**Shipped now (correctness fix):** `vscodeReachable(t)` returns `false` when
`t.relayKey` is set; `buildVscodeLaunch` returns `{ kind: "unavailable" }` and
`RightPanel` renders the button disabled with an honest hint. This document is
the plan for turning that disabled state into a working editor.

## Scope note: this is the universal remote-editing path

Remote-SSH already fails for many *direct* hosts too — it assumes the client
has SSH access (keys, reachability) that plain device-token enrollment never
required. The browser editor below works for **any** remote node, relayed or
direct, with zero client-side setup. Ship it as the default "Continue in VS
Code" for every remote target; keep the Remote-SSH deep link as the
power-user path when SSH happens to exist (offer both, or prefer web and
demote SSH to the fallback panel — decide at V3). Local same-origin daemons
keep the `vscode://file` deep link.

## Approach (chosen): browser VS Code on the node, proxied through the relay

Run a **web-based VS Code server on the node itself**, bound to a loopback
port, reverse-proxied by the daemon so it rides the existing relay path. The
client opens a **browser tab** — no deep link, no local VS Code, no client
tooling, works on mobile. The IDE process runs on the node with the daemon's
filesystem/git access, pointed at the session's `working_directory` (often an
isolated per-session worktree with the agent's uncommitted changes): it edits
the real worktree in place — no copy, no push round-trip.

This satisfies the core connection principle (zero client-side tooling,
outbound HTTPS/WSS to a single address) and is the only VS Code path that
survives NAT — which is the relay's whole reason to exist.

**Honest cost accounting** (revised after review): this needs **one small
relay change** (cookie-based key auth, below) — the original "zero new relay
capability" claim was wrong. The relay still never parses the daemon API; the
change is confined to its auth layer. The daemon grows a reverse proxy and a
process supervisor. The UX is VS Code *web* (extensions install on the node;
some desktop-only extensions unavailable), not the user's local VS Code
profile — a real product difference worth stating in the UI copy.

### Rejected alternatives

- **Edit on the relay host, then push to the node.** (1) The relay is a
  locked-design opaque byte proxy that never touches files or credentials — a
  git waypoint breaks that model and its security posture; (2) the relay may
  be a bare public VPS with none of the repo, toolchain, or the live dirty
  worktree; (3) editing a divergent checkout and pushing is not live editing
  and invites conflicts with the running agent.
- **Remote-SSH through the relay via `ProxyCommand`.** Needs sshd + keys on
  the node, a new raw-TCP tunnel class to `:22` (today the tunnel dials only
  the daemon's loopback HTTP port), *and* a client-side stdio↔relay helper —
  exactly the client tooling the relay exists to eliminate; impossible in a
  browser or on mobile.
- **`code tunnel` (VS Code Remote Tunnels).** Routes through Microsoft's
  global tunnel relay and requires a GitHub/Microsoft account — third-party
  dependency that defeats the self-hosted relay (same 2026-07-04 decision:
  ASM owns its connectivity path).

## Web IDE choice

Prefer, in order, whichever is present on the node — mirroring the existing
pattern where the new-session dialog only offers agents whose CLI is
installed:

1. **`code serve-web`** — official VS Code CLI subcommand; serves the full web
   workbench locally with no Microsoft account and no external relay
   (unlike `code tunnel`). Supports `--host`, `--port`,
   `--connection-token(-file)`, `--server-base-path` *(verify flag set in
   V0)*. Caveat: it **downloads the server build from Microsoft's CDN on
   first run** — fine for relay-registered nodes (they have outbound HTTPS by
   construction), but a real constraint for airgapped LANs; document it.
2. **`openvscode-server`** — Gitpod's MIT single-binary build of the same
   upstream server; fully self-contained (no CDN fetch), same token/base-path
   surface *(verify in V0)*. Fallback when `code` is absent.

Do **not** bundle `code-server` (Coder) by default — AGPL, heavier — unless a
deployment explicitly opts in. If neither binary is available, keep the button
disabled but change the copy to an actionable "install the VS Code CLI on this
host to enable browser editing," matching the agent-not-installed UX.

## Architecture

```
Browser tab: https://relay/n/<node_id>/editor/?folder=<ws>&ticket=<t>
     │
     ▼  (relay: auth via key cookie, strips /n/<node_id>, splices bytes)
RELAY ──────────────────────────────────────────► NODE (daemon tunnel listener)
                                                  ┌──────────────────────────────┐
   direct client, same canonical URL:             │ editor proxy                 │
   https://host:4600/n/<node_id>/editor/… ──────► │  /editor/*  (relay-stripped) │
                                                  │  /n/<self>/editor/* (direct) │
                                                  │  ticket ⇄ cookie, WS upgrade │
                                                  │        │ re-adds /n/<self>   │
                                                  │        ▼ loopback            │
                                                  │ code serve-web /             │
                                                  │ openvscode-server            │
                                                  │  127.0.0.1:<ephemeral>       │
                                                  │  --server-base-path=         │
                                                  │    /n/<node_id>/editor       │
                                                  │  --connection-token=<CSPRNG> │
                                                  │        │                     │
                                                  │        ▼                     │
                                                  │ session.working_directory    │
                                                  │ (real worktree on the host)  │
                                                  └──────────────────────────────┘
```

- **One shared IDE server per daemon** (not per session), lazy-started;
  `?folder=` selects the workspace. Simplest and lowest-memory; revisit
  per-session processes only if isolation demands it.
- **Daemon editor proxy** — new routes *outside* `/api` (so outside the bearer
  middleware; auth is the ticket/cookie flow below): `/editor/*rest` and
  `/n/:node_id/editor/*rest` (validated `:node_id == server_id`), both
  forwarding to the loopback IDE, WS upgrades included. Served on the same
  router the tunnel listener serves; **never loopback-trusted** (reuse
  `ListenerKind::Tunnel` / `LoopbackTrust`). The HTTP-over-duplex + upgrade
  splice machinery already exists in `asm-relay` (`transport.rs`, `forward`) —
  reuse or mirror it rather than reinventing.
- **Relay** — one auth-layer addition (below); routing untouched.
- **Client** — `POST /api/sessions/:id/editor-ticket` (device-token authed)
  returns `{ ticket, folder }`; the client composes the canonical URL
  (relayed: `baseUrl` already ends in `/n/<id>`, append `/editor/…`; direct:
  insert `/n/<node_id>` using the `node_id` that `/health` reports since R2)
  and opens it in a new tab. `buildVscodeLaunch` grows an `editor-web` kind
  alongside `local` / `remote-ssh` / `unavailable`.

## The three hard problems (found in review) and their solutions

### 1. A web app under a rewritten path prefix

The relay **strips `/n/<node_id>`** before forwarding. The browser sits at
`https://relay/n/<id>/editor/…` while the IDE would see `/editor/…` — so every
absolute URL the workbench emits (static assets, WS endpoints, service-worker
scope) would miss the prefix and 404 at the relay. Web apps are not
transparently proxyable the way the daemon's own API is (the ASM client builds
its URLs explicitly; the IDE's client code does not).

**Solution — canonical base path.** The daemon knows its own `node_id`
(= `server_id`). Run the IDE with `--server-base-path=/n/<node_id>/editor` and
have the proxy **re-add the prefix upstream** (relay-stripped `/editor/foo` →
IDE `/n/<id>/editor/foo`). Every absolute URL the IDE generates is then
correct *as seen by the browser through the relay*. Direct access uses the
same canonical URL via the `/n/<self>/editor/*` alias. No HTML rewriting, no
relay change for routing. Side benefit: IDE cookies and the service-worker
scope become path-scoped under `/n/<id>/` — **distinct per node**, so two
nodes behind one relay origin don't clobber each other's cookies/SW.
Residual: VS Code web keeps workbench state in origin-keyed IndexedDB, so
nodes sharing a relay origin share that state; likely tolerable (much of it is
workspace-keyed — *verify in V0*); the escape hatch is subdomain-per-node
routing, a deliberately deferred relay feature.

### 2. Relay auth on IDE-generated requests

The relay requires its key on **every** request, accepted only as the
`x-asm-relay-key` header or `relay_key` query param
(`asm-relay/src/server.rs::key_ok`). The ASM client attaches it explicitly —
but the IDE's hundreds of asset/WS requests are generated by VS Code's own
client code, which will carry neither. Everything after the first page load
would 401 at the relay. **This is why "zero relay changes" was wrong.**

**Solution — relay key via cookie.** Small, generic relay addition: when a
request authenticates via header/param, set an `HttpOnly; SameSite=Lax;
Secure(when TLS); Path=/` cookie carrying the key (or a relay-minted session
id); `key_ok` additionally accepts it. Browser-attached cookies then cover
every subresource and WS handshake (same-origin WS sends cookies). The relay
still parses nothing of the daemon protocol. Security delta: a lure page
cannot ride the cookie cross-site (`SameSite=Lax` withholds it on cross-site
subresources/WS), and even a relayed request that passes the relay still faces
the daemon's device-token / editor-ticket layer end-to-end.

### 3. Credentials in a tab URL

The original sketch put the device token in the editor URL (`access_token=`
style, as `streamUrl` does for its WS). A top-level tab URL is worse than a WS
URL: it persists in browser history, sync, and same-origin referrers.

**Solution — one-time editor ticket.** The client mints a short-TTL (~60 s),
single-use ticket via the authenticated `POST /api/sessions/:id/editor-ticket`;
only the ticket appears in the opened URL. The daemon's editor proxy consumes
it and sets its own `HttpOnly; SameSite=Lax` session cookie (path-scoped to
`/n/<id>/editor`); all subsequent requests ride the cookie. The device token
never appears in any URL. The **IDE connection token** stays a third, inner
credential: minted per IDE process, injected by the daemon on every upstream
request *(query vs cookie mechanics: verify in V0)*, never sent to the client
— it exists so other local users on the node can't drive the loopback IDE
port directly.

## Security notes

- **The editor is remote code execution on the node** (integrated terminal,
  tasks) as the daemon user — the same power the session terminal WS already
  grants, so no new privilege class, but the editor route must be gated
  exactly as strongly (ticket derived from device token; never
  loopback-trust through the tunnel).
- **Cross-site WS hijacking** on the cookie-authed editor WS is blocked by
  `SameSite=Lax`; add an Origin allowlist check at the daemon proxy as
  belt-and-braces.
- **Plaintext at the relay**: the relay sees editor traffic plaintext, same as
  terminal/SCM streams today (accepted for the personal deployment; R5
  app-layer-encryption decision gate in `connectivity-execution-plan.md` /
  `security-followups.md`). Editing traffic raises the value of that
  exposure; note it when R5 is decided.

## Open decisions (resolve by end of V0)

- **IDE flag surface**: exact `--server-base-path` / connection-token behavior
  per binary and minimum versions — the V0 spike's whole job.
- **Lifecycle numbers**: idle-shutdown timeout (proposal: 30 min with no
  proxied connections), kill on daemon shutdown, restart-on-crash policy.
- **Bundling**: detect-only for MVP (document install in `deployment.md`) vs
  shipping a pinned `openvscode-server`. Leaning detect-only.
- **Worktree cleanup vs open editor**: cleaning a session's worktree while an
  editor tab has it open — block, warn, or let the IDE show the folder
  vanishing. Leaning warn (same as the dirty-worktree cleanup guard).
- **Remote-SSH placement in the UI** once web editing exists (see scope note).

## Milestones

- **V0 — de-risking spike (scratchpad only, no product code).** Run
  `code serve-web` and `openvscode-server` with
  `--server-base-path=/n/FAKE/editor` behind (a) a minimal prefix-stripping
  proxy and (b) the real `asm-relay` + a stub agent. Headless-Chrome CDP
  loads the workbench through the relay path. *Answers:* does the workbench
  fully load and open `?folder=` under a rewritten prefix (assets, WS,
  service worker, cookies); connection-token injection mechanics; IndexedDB
  behavior with two fake nodes on one origin; CDN-download behavior offline.
  *Output:* IDE choice + verified flag matrix recorded in this doc; go/no-go
  on the canonical-base-path design (fallback if no-go: subdomain routing
  study before any daemon work).
- **V1 — relay cookie auth + daemon editor proxy.** Relay: accept the key
  from the cookie, set it on successful header/param auth (contract addition
  documented in `connectivity-execution-plan.md`'s wire contract; still no
  daemon-API parsing). Daemon: `/editor/*` + `/n/<self>/editor/*` proxy
  routes (HTTP + WS upgrade, prefix re-add) to a configurable loopback
  target, stub ticket store, never loopback-trusted. *Acceptance:* a
  `relay-test.mjs`-style script proves a loopback echo app (HTTP + WS) works
  through both the direct alias and the relay **with cookies only after the
  first request**; requests without ticket/cookie are rejected; tunnel
  requests never gain loopback trust.
- **V2 — IDE launcher, detection, tickets.** Detect installed IDE binaries;
  lazy-start with CSPRNG connection token on an ephemeral loopback port;
  `POST /api/sessions/:id/editor-ticket`; ticket ⇄ cookie exchange in the
  proxy. *Acceptance:* CDP harness opens the workbench through the relay,
  workspace folder listed and a file editable; no device token in any URL at
  any point; a second tab without a fresh ticket is rejected; the raw
  loopback port rejects requests lacking the connection token.
- **V3 — client wiring, lifecycle, polish.** Button logic per target kind
  (local deep link / web editor / optional Remote-SSH), idle shutdown +
  daemon-shutdown kill, actionable no-CLI copy, i18n via `en.json`,
  `npm run build` green. *Acceptance:* end-to-end from the real client bundle
  against a relayed node; idle server exits and a later open cold-starts
  cleanly; relay-unavailable hint replaced by the working flow.

## What ships without this feature (already done)

`client/src/vscode.ts` `vscodeReachable()` + the `unavailable` launch kind,
and the disabled button + hint in `client/src/components/RightPanel.tsx`
(`rightPanel.vscode.relayUnavailable`). That stops the misdirected SSH deep
link today; everything above turns the disabled state into a working browser
editor.
