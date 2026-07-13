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
- **Keep the port on the host's loopback** (`127.0.0.1:4600:4600`) and reach it
  by **SSH local port-forward**, or terminate **TLS at a reverse proxy** in
  front. Do not publish `4600` on a public interface — the channel is still
  plaintext off-loopback (item 1).
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
