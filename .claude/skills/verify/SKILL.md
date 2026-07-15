---
name: verify
description: Build, launch, and drive the ASM daemon + web client to verify a change end-to-end (headless Chrome + raw CDP, sandboxed throwaway daemon).
---

# Verifying ASM changes

## Build
```bash
source ~/.cargo/env
cargo build -p asm-daemon          # binary at target/debug/asm-daemon
(cd client && npm run build)       # bundle at client/dist
```

## Launch a throwaway daemon (NEVER port 4600 / ~/.local/share/asm — that's the user's real install)
This host exports `ASM_*`/`ASMUX_*` globally and an inherited `ASMUX_SOCK` can
destroy the live holder's socket — always strip the env with `env -i`:
```bash
D=<scratch>; mkdir -p $D/{data,cfg,rt,chrome}
env -i PATH="$PATH" HOME="$HOME" \
  ASM_BIND=127.0.0.1:<freeport> ASM_DATA_DIR=$D/data ASM_CONFIG_DIR=$D/cfg \
  ASM_RUNTIME_DIR=$D/rt ASM_STATIC_DIR=$PWD/client/dist ASM_BACKEND=native \
  ASM_ASMUX_AUTOSPAWN=0 ./target/debug/asm-daemon &
curl -sf http://127.0.0.1:<freeport>/health
```
Same-origin loopback needs no token. Create sessions with
`POST /api/sessions {"agent_plugin_id":"shell","cwd":...}`; SCM endpoints are
`/api/sessions/<id>/scm/status` and `/scm/log?limit=N`.

## Drive the UI
No puppeteer/playwright — use headless Chrome + raw CDP over Node's global WebSocket:
```bash
google-chrome --headless=new --disable-gpu --no-sandbox --disable-dev-shm-usage \
  --remote-debugging-port=<port> --user-data-dir=$D/chrome about:blank &
```
Copy the CDP client pattern from `scripts/attach-button-test.mjs`
(Target.createTarget → attachToTarget flatten → Runtime.evaluate / Page.captureScreenshot).
Gotchas: `Network.setCacheDisabled` or the tab loads a stale bundle; auto-accept
`Page.javascriptDialogOpening` or one `confirm()` freezes every evaluate in the
renderer. Click a `.session-row` to select a session; the right panel's SCM
status + History (`.commit-row`, `.commit-graph`) render for any git-repo cwd.

## Cleanup
Kill daemon/Chrome **by PID** — a broad `pkill -f` can match your own shell or
the user's live processes.
