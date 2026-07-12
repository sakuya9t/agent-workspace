// Durable-session restart test — the headline proof that sessions survive a
// daemon restart via the out-of-process asmux holder (durable-sessions.md M2+M3).
//
//   node scripts/durable-restart-test.mjs
//
// It orchestrates everything itself, inside a sandbox (scripts/lib/testenv.mjs):
//   1. start asmux (the holder) on a PRIVATE socket
//   2. start daemon #1 with ASM_BACKEND=sidecar
//   3. create a shell session, run a marker command, confirm it is running
//   4. SIGTERM daemon #1 (the holder keeps the PTY alive)
//   5. start daemon #2 on the same data dir + holder socket
//   6. assert the session was ADOPTED: still `running`, screen reconstructed
//      (marker present), and still accepting input (a second marker echoes back)
//
// History: this test once inherited the dev host's ambient ASMUX_SOCK, so its
// asmux unlinked and rebound the REAL holder's socket — orphaning six live prod
// sessions. It now takes its socket from the sandbox, which cannot resolve to
// the real one, and asmux itself refuses to displace a live holder.
//
// Requires the built binaries (`cargo build`) and Node 18+ (global WebSocket).

import { existsSync } from "node:fs";
import { createSandbox, checker, sleep } from "./lib/testenv.mjs";

const { check, report } = checker();
const sb = await createSandbox("asm-dur");

// Both daemons drive the holder rather than in-process PTYs.
const SIDECAR = { ASM_BACKEND: "sidecar", ASM_ASMUX_AUTOSPAWN: "0" };

async function main() {
  // 1. the holder, on the sandbox's private socket
  await sb.startAsmux();
  check("holder (asmux) socket up", existsSync(sb.socket), sb.socket);

  // 2. daemon #1, driving that holder
  const health = await sb.startDaemon("daemon1", SIDECAR);
  check("daemon1 up on sidecar backend", health.backend === "asmux-sidecar", health.backend);

  // 3. create a shell session + run a marker
  const { session } = await sb.api("/api/sessions", {
    method: "POST",
    body: JSON.stringify({ agent_plugin_id: "shell", cwd: sb.cwd }),
  });
  check("session created running", session.status === "running", session.id.slice(0, 8));
  const id = session.id;

  const marker = "DURABLE-" + Math.random().toString(36).slice(2, 8).toUpperCase();
  const c1 = sb.ws(id);
  await new Promise((res) => (c1.ws.onopen = res));
  await sleep(500);
  c1.ws.send(JSON.stringify({ t: "i", d: `echo ${marker}\r` }));
  await sleep(800);
  c1.ws.close();
  check("live output shows marker (pre-restart)", c1.buf.includes(marker));

  // 3b. Cold-stitch discriminator: emit a marker, then MORE than the 2 MiB
  // holder ring of filler, so that marker is evicted from the ring. Only the
  // exact cold-stitch adopt (seed vt100 + history from the daemon's SQLite cold
  // history) can reconstruct it after restart — plain ring-replay cannot.
  const coldMarker = "COLD-" + Math.random().toString(36).slice(2, 8).toUpperCase();
  const cf = sb.ws(id);
  await new Promise((res) => (cf.ws.onopen = res));
  await sleep(300);
  cf.ws.send(JSON.stringify({ t: "i", d: `echo ${coldMarker}\r` }));
  await sleep(500);
  // ~2.65 MiB (52000 lines * 51 bytes) — well past the 2 MiB default ring.
  const filler = "".padEnd(50, ".");
  cf.ws.send(JSON.stringify({ t: "i", d: `yes ${filler} | head -n 52000\r` }));
  await sleep(4500); // let it all emit AND flush to cold history
  cf.ws.close();

  // 4. kill daemon #1 — the holder must keep the PTY alive
  sb.stop("daemon1");
  await sb.waitExit("daemon1", 5000);
  check("daemon1 exited", true);
  check("holder still alive after daemon exit", existsSync(sb.socket));

  // 5. daemon #2 on the same data dir + holder socket
  await sb.startDaemon("daemon2", SIDECAR);

  // 6. the durability assertions
  const after = await sb.api(`/api/sessions/${id}`);
  check(
    "session ADOPTED as running after restart",
    after.session.status === "running",
    `status=${after.session.status}`,
  );

  const c2 = sb.ws(id);
  await new Promise((res) => (c2.ws.onopen = res));
  await sleep(1200);
  check("screen reconstructed after restart (marker present)", c2.buf.includes(marker));
  check(
    "cold-stitch preserved history beyond the 2 MiB ring",
    c2.buf.includes(coldMarker),
    coldMarker,
  );

  // prove it is genuinely alive, not just a replayed corpse
  const marker2 = "ALIVE-" + Math.random().toString(36).slice(2, 8).toUpperCase();
  c2.ws.send(JSON.stringify({ t: "i", d: `echo ${marker2}\r` }));
  await sleep(900);
  c2.ws.close();
  check("session still accepts input after restart", c2.buf.includes(marker2), marker2);

  // clean stop
  await sb.api(`/api/sessions/${id}/stop`, { method: "POST" }).catch(() => {});

  return report("sessions survive daemon restart");
}

main()
  .then((pass) => {
    sb.cleanup();
    process.exit(pass ? 0 : 1);
  })
  .catch((e) => {
    console.error(e);
    sb.cleanup();
    process.exit(2);
  });
