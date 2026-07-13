# Deployment (Docker)

Status: **design-forward.** The durable, session-surviving deployment described
here presupposes the `asmux` sidecar (milestones **M1–M3** in
[`durable-sessions.md`](durable-sessions.md)). It is **not built yet** — today
the daemon uses the in-process `native` backend, so *any* restart loses live
sessions. Use the [single-container / today](#today-single-container-native-backend)
recipe now; adopt the [two-container](#target-two-containers) split once the
sidecar lands.

Companion docs: [`durable-sessions.md`](durable-sessions.md) (why the split
exists), [`asmux-protocol.md`](asmux-protocol.md) (the wire contract),
[`security-followups.md`](security-followups.md) (the accepted gaps this doc
must not regress).

## Principle: put the durability split at the container boundary

The design's durability rule is **"the daemon restarts freely; asmux never
restarts."** A Docker image is immutable, so an "update" is *replace the
container*, which kills every process in it — including the entrypoint. Putting
both processes in one container therefore does **not** survive an image update:
replacing the container replaces asmux, and its RAM (ring buffers + PTYs) dies
with it.

The fix is to realise the split at the **container** boundary instead of the
process boundary — churny half and durable half as *separate images*:

| | **asmux container** (substrate) | **daemon container** (control plane) |
| --- | --- | --- |
| holds | PTY masters, child agents, cursor rings, live metadata — in *its* namespace/cgroup | HTTP/WS API, `vt100`, SQLite, all product logic |
| image weight | heavy — ships `git` + `node` + the agent CLIs (`claude`/`codex`); agents `exec` here | lean — daemon binary + built client + `git` |
| update cadence | **rare** — wire protocol is frozen, holder logic is tiny and never-crash | **constant** — UI, classifiers, summaries, plugins all live here |
| network-facing? | **never** (no ports; UDS only) | yes (HTTP/WS) |
| replaced on update? | almost never | freely |

Two containers on the **same host** share the sidecar's Unix socket by
bind-mounting the socket directory (the same pattern as sharing `docker.sock`) —
UDS needs a shared *filesystem* mount, not a shared network namespace, so
"No TCP, ever" still holds. Update the daemon container and asmux is untouched:
the daemon reconnects over the socket, runs `session.list` + `attach
FromCursor`, and re-adopts every session with the terminal intact. The
`instance_id` in `hello` is how it confirms it reattached to the *same* asmux
instance and not a fresh one (`binary_sha256` only detects a binary *change* — a
recreated container running the same image has the same hash, so it can't prove
instance identity on its own).

## What survives what

| Event | Sessions survive? | Why |
| --- | --- | --- |
| **daemon image update** (recreate daemon container) | ✅ yes | asmux container lives; daemon re-adopts on reconnect |
| daemon crash / restart | ✅ yes | same path as above |
| **asmux image update** (recreate asmux container) | ❌ no | its RAM (rings + PTYs) is destroyed |
| host reboot / migrate hosts | ❌ no | records persist on the `data` volume, reconcile to `failed` |

The honest promise is **"daemon image updates don't break sessions."** Since
essentially all change lands in the daemon, that covers the vast majority of
updates. asmux only moves when the frozen wire protocol or the PTY-holder logic
changes — which the never-crash / no-`vt100` discipline is designed to make rare.

RAM loss (asmux update, host reboot) is the floor: making an *asmux* update
non-disruptive would require passing live PTY master fds across processes
(`SCM_RIGHTS`, roughly what tmux's server does). That fights the "keep asmux
dumb and never-crash" ethos and is explicitly **out of scope**; SQLite records +
`failed` reconciliation is the graceful landing.

## Target: two containers

### `docker-compose.yml`

```yaml
services:
  asmux:                      # durable substrate — rarely replaced
    image: asm-asmux:latest
    restart: unless-stopped
    user: "10001:10001"       # MUST match the daemon's UID (shared 0600 socket)
    environment:
      - ASM_RUNTIME_DIR=/run/asm      # creates /run/asm/asmux.sock (0600, dir 0700)
    volumes:
      - run:/run/asm          # UDS shared with the daemon
      - data:/data            # worktrees the agents operate in (same path both sides)
    # NO ports — asmux is never network-facing

  daemon:                     # control plane — replaced on every update
    image: asm-daemon:latest
    restart: unless-stopped
    user: "10001:10001"
    depends_on: [asmux]
    environment:
      - ASM_BIND=0.0.0.0:4600         # 0.0.0.0 so Docker's port-publish can reach it
      - ASM_DATA_DIR=/data            # asm.sqlite3 + worktrees/
      - ASM_RUNTIME_DIR=/run/asm      # finds asmux.sock here
      - ASM_STATIC_DIR=/app/web       # serve the built client
      - ASM_BACKEND=sidecar           # (M2) connect to asmux instead of native
      - ASM_ASMUX_AUTOSPAWN=0         # (M2) do NOT fork a child; asmux is a peer container
    ports:
      - "127.0.0.1:4600:4600"         # host loopback ONLY — SSH-tunnel in (see Security)
    volumes:
      - run:/run/asm          # same socket dir
      - data:/data            # same worktrees path as asmux

volumes:
  run:                        # ephemeral socket dir (could be tmpfs)
  data:                       # durable: SQLite + worktrees survive container replacement
```

### Update recipes

```bash
# Daemon-only update — sessions survive. --no-deps keeps asmux from being recreated.
docker compose pull daemon
docker compose up -d --no-deps daemon

# asmux update — sessions are lost (records reconcile to failed). Do it deliberately.
docker compose pull asmux
docker compose up -d asmux
```

### Constraints that make the split work

- **Same UID both containers.** The socket is `0600`; both sides must run as the
  same user to share it, and `/data/worktrees` must be owned consistently.
- **`/data` mounted at the same path in both.** The daemon creates worktrees
  under `ASM_DATA_DIR/worktrees`; asmux `exec`s the agent with that path as its
  `cwd`. They must resolve to the same bytes.
- **Registered workspace roots must live on a shared volume** visible to both at
  the same path. A client can register any root (see
  [`security-followups.md`](security-followups.md) item 2), but in containers a
  root only works if it's inside a mounted volume both containers see. Constrain
  registration to an allowlist under `/data`.
- **Agent CLIs live in the asmux image**, not the daemon image — agents are
  `exec`'d by asmux. The daemon needs only `git` (for `source_control`/worktrees).
- **Daemon connects, doesn't spawn.** In this topology asmux is a peer, not a
  child, so disable auto-spawn (`ASM_ASMUX_AUTOSPAWN=0`) and rely on the M4
  heartbeat/backoff reconnect to ride out asmux (re)starts.
- **Volume ownership must be initialised.** Fresh named volumes mount
  **root-owned**, so a container running as `10001:10001` can't write `/data` or
  create the socket in `/run/asm`. Fix ownership once — an entrypoint/init step
  that `chown`s `/data` and `/run/asm` to `10001:10001` before dropping
  privileges, a one-shot init container, or a pre-created host dir with the right
  owner. Without it the daemon can't create `asm.sqlite3`/`worktrees` and asmux
  can't bind the socket.
- **Credentials must reach the container that runs the tool.** Agents `exec` in
  **asmux**, so per-agent auth/home/cache (`~/.claude`, `~/.codex`, `~/.config`,
  `~/.cache`) mount into the **asmux** container, not the daemon. **Git identity
  and credentials/SSH are needed in both** — the daemon runs `git` for
  worktrees/status/log and the agent runs `git` inside its session — so mount
  `~/.gitconfig` and credential/SSH material into each (read-only where possible).

### Dockerfiles (sketch)

`asmux` does not build yet; its stage is aspirational until M1. PTYs work with
Docker's default `devpts` — **no `--privileged`** and no extra capabilities are
needed for `openpty` + `setsid`.

```dockerfile
# ---- shared build ----
FROM rust:1-bookworm AS build
WORKDIR /src
COPY . .
RUN cargo build --release -p asm-daemon      # add: -p asmux   (once M1 lands)

FROM node:20-bookworm AS web
WORKDIR /w
COPY client/ .
RUN npm ci && npm run build                   # -> /w/dist

# ---- daemon runtime (lean) ----
FROM debian:bookworm-slim AS daemon
RUN apt-get update && apt-get install -y --no-install-recommends git ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=build /src/target/release/asm-daemon /usr/local/bin/
COPY --from=web   /w/dist                       /app/web
USER 10001:10001
ENTRYPOINT ["asm-daemon"]

# ---- asmux runtime (heavy: agent CLIs) — M1+ ----
FROM debian:bookworm-slim AS asmux
RUN apt-get update && apt-get install -y --no-install-recommends git ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*
# + node + the agent CLIs (claude/codex) your users launch
COPY --from=build /src/target/release/asmux /usr/local/bin/
USER 10001:10001
ENTRYPOINT ["asmux"]
```

## Security posture in containers

Containerising nudges you off the loopback-trust model, so the accepted gaps in
[`security-followups.md`](security-followups.md) matter more, not less:

- **Publishing forces `ASM_BIND=0.0.0.0` inside the container** (Docker's
  port-publish can't reach a container-loopback bind). That flips the daemon out
  of `loopback_only`, so **token auth is enforced** — good. Make sure the
  enrollment token is set (item 3) and treat it as the real credential now that
  loopback-trust no longer implies "one human on the box" (item 6).
- **Give the daemon a certificate** (`ASM_TLS_CERT` / `ASM_TLS_KEY`) if you
  publish its port anywhere but host loopback — it then serves https/wss and the
  direct path is encrypted. Otherwise keep the port on the host's loopback
  (`127.0.0.1:4600:4600`) and reach it by **SSH local port-forward**, or use the
  **relay** (below), which needs no inbound port at all.
- **A same-host reverse proxy in front of the daemon DISABLES AUTH.** The proxy
  connects from `127.0.0.1`, and loopback peers are trusted without a token — so
  every request it forwards, from anywhere, arrives pre-trusted. If you front the
  daemon with a proxy you **must** set `ASM_TRUST_LOOPBACK=0`
  (`scripts/start.sh --no-loopback-trust`), which makes a device token mandatory
  on every request. Giving the daemon its own certificate avoids the whole
  question.
- **CORS is permissive** (item 5); if a proxy serves the client from a different
  origin, restrict allowed origins there.
- asmux exposes **no port** — it must stay that way. Its only surface is the
  `0600` UDS on the shared `run` volume.

## Today: single container, native backend

Until the sidecar lands you can still deploy the daemon by itself. Live sessions
do **not** survive a restart (in-process PTYs), but records and worktrees persist
on the `data` volume.

```yaml
services:
  daemon:
    image: asm-daemon:latest
    restart: unless-stopped
    user: "10001:10001"
    environment:
      - ASM_BIND=0.0.0.0:4600
      - ASM_DATA_DIR=/data
      - ASM_STATIC_DIR=/app/web
    ports:
      - "127.0.0.1:4600:4600"
    volumes:
      - data:/data
volumes:
  data:
```

This image must carry the agent CLIs itself (the native backend `exec`s agents
in-process). When you migrate to the two-container split, those CLIs move to the
`asmux` image and the daemon image slims down to just `git` + the binary.

## The relay: TLS on a public host

The relay is the only component that takes inbound connections from the open
internet, and the only one that holds a certificate. It carries device tokens and
whole terminal streams for every node registered with it, so **it must be TLS**.
Both hops are encrypted by the same certificate: the browser dials `https://`,
and the daemon dials `wss://` *outbound* from behind its NAT.

Two shapes, both supported:

```bash
# A. The relay terminates TLS itself.
ASM_RELAY_BIND=0.0.0.0:443 \
ASM_RELAY_KEYS=<access-key> \
ASM_RELAY_TLS_CERT=/etc/asm/fullchain.pem \
ASM_RELAY_TLS_KEY=/etc/asm/privkey.pem \
  asm-relay

# B. A reverse proxy (Caddy, nginx) terminates TLS and forwards to the relay.
ASM_RELAY_BIND=127.0.0.1:4700 ASM_RELAY_KEYS=<access-key> ASM_RELAY_HSTS=1 asm-relay
```

Notes that matter:

- **Use a real ACME certificate.** A self-signed cert works for the *daemon* —
  point `ASM_RELAY_CA` at it — but the *browser* has no equivalent escape hatch
  and will throw a certificate warning at every user.
- **A self-signed cert must be a leaf (`CA:FALSE`).** `openssl req -x509` marks
  its output `CA:TRUE` by default, and rustls refuses a CA certificate presented
  as a server's own leaf — the daemon fails to register with `CaUsedAsEndEntity`
  while browsers accept the same cert happily, so it looks like the node is
  broken rather than the cert. Mint it explicitly as a leaf:

  ```bash
  openssl req -x509 -newkey rsa:2048 -keyout key.pem -out cert.pem -days 365 -nodes \
    -subj "/CN=relay.example.com" \
    -addext "subjectAltName=DNS:relay.example.com" \
    -addext "basicConstraints=critical,CA:FALSE"
  ```

  Then copy `cert.pem` to every node and pass `--relay-ca cert.pem`.
- **With TLS on, the relay speaks only TLS.** There is no cleartext port to fall
  back to; a plaintext client fails the handshake. Setting only one of
  `ASM_RELAY_TLS_CERT` / `ASM_RELAY_TLS_KEY` is a startup error, so a typo can't
  quietly downgrade a production relay to plaintext.
- **In shape B, the proxy must pass WebSocket upgrades through** — the control
  stream, every data stream, and every terminal stream are upgrades. Set
  `ASM_RELAY_HSTS=1` so the header still reaches the browser (the relay itself
  only sees plain HTTP in that shape).
- **Nodes then register with `ASM_RELAY_URL=wss://relay.example.com`.** A
  plaintext `ws://` URL to a remote host is refused at daemon boot.

### Worked example: browser on A, daemon+relay on B, NAT'd daemon on C

```bash
# B — the reachable box: a daemon the LAN can reach, plus the relay C dials into.
#     One --relay-key serves both roles: the relay accepts it, nodes present it.
scripts/start.sh --bind 0.0.0.0:4600 \
  --relay --relay-key s3cret \
  --relay-tls-cert /etc/asm/fullchain.pem --relay-tls-key /etc/asm/privkey.pem

# C — behind NAT: dials out to B's relay, accepts nothing inbound.
scripts/start.sh --register wss://relay.example.com:4700 --relay-key s3cret

# A — the machine you browse from: a local daemon, which also serves the web UI.
scripts/start.sh                      # → http://127.0.0.1:4600
```

In the client on A:

- **B, directly** — manage → Add → Daemon: `http://<B>:4600` + B's enrollment
  token (`scripts/token.sh` on B). The client flags the URL as unencrypted, which
  it is: B's daemon has no certificate, so on the LAN the token and the terminal
  stream are readable. That's the trade you're making for a direct connection;
  it connects regardless.
- **C, through the relay** — manage → Add → Relay: `https://relay.example.com:4700`
  + key `s3cret`. C appears underneath it; enroll it with C's own token. This hop
  *is* encrypted end to end.

**If you want B encrypted too**, give its daemon no LAN bind at all and have it
register to its own relay instead — then every client hop, to B and to C alike,
runs over the TLS relay and the client needs only the one relay entry:

```bash
# B, all traffic through its own relay: drop --bind, add --register.
scripts/start.sh \
  --relay --relay-key s3cret \
  --relay-tls-cert /etc/asm/fullchain.pem --relay-tls-key /etc/asm/privkey.pem \
  --register wss://relay.example.com:4700
```

B must then be able to resolve and reach `relay.example.com` itself, since its
daemon dials the relay by name — the certificate is verified against the name,
not the address. If B cannot reach its own public address, a
`127.0.0.1 relay.example.com` line in B's `/etc/hosts` makes that hop pure
loopback and the certificate still validates.
