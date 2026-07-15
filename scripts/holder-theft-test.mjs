// Regression test for the 2026-07-12 holder-theft incident.
//
//   node scripts/holder-theft-test.mjs
//
// What happened: an e2e test inherited the dev host's ambient ASMUX_SOCK, so its
// asmux `remove_file`d and rebound the REAL holder's socket path, then removed it
// again on exit. The real holder kept running — its listener fd was still open, so
// it logged nothing — but nobody could dial it by path any more. The next daemon
// restart could not find the holder, refused to boot (ASM_ASMUX_AUTOSPAWN=0), and
// the orphan had to be killed to recover. SIX LIVE SESSIONS WERE LOST.
//
// This reproduces both halves of that, in a sandbox, and asserts the defences:
//
//   A. THEFT   — a second asmux aimed at a live socket must REFUSE to bind, and
//                the victim must still be serving afterwards.
//   B. HEALING — if the socket path is unlinked anyway (a stray `rm`, a /run
//                sweep), the holder must notice and REBIND, so a subsequent
//                daemon restart still ADOPTS the session instead of reconciling
//                it to `indeterminate`. Sessions must survive.
//
// B is the one that matters: it is the difference between losing six sessions and
// losing none.

import { existsSync, unlinkSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { createSandbox, checker, sleep, ASMUX_BIN } from "./lib/testenv.mjs";

const { check, report } = checker();
const sb = await createSandbox("asm-theft");

const SIDECAR = { ASM_BACKEND: "sidecar", ASM_ASMUX_AUTOSPAWN: "0" };

async function main() {
  await sb.startAsmux();
  await sb.startDaemon("daemon1", SIDECAR);

  // A live session with a marker we can look for after the dust settles.
  const { session } = await sb.api("/api/sessions", {
    method: "POST",
    body: JSON.stringify({ agent_plugin_id: "shell", cwd: sb.cwd }),
  });
  const id = session.id;
  const marker = "SURVIVE-" + Math.random().toString(36).slice(2, 8).toUpperCase();
  const c1 = sb.ws(id);
  await new Promise((r) => (c1.ws.onopen = r));
  await sleep(500);
  c1.ws.send(JSON.stringify({ t: "i", d: `echo ${marker}\r` }));
  await sleep(800);
  c1.ws.close();
  check("session live before the incident", c1.buf.includes(marker), id.slice(0, 8));

  // ---- A. THEFT: a second asmux must not be able to take the live socket ----
  const thief = spawnSync(ASMUX_BIN, [], {
    env: { ...sb.env(), ASM_LOG: "info" },
    encoding: "utf8",
    timeout: 10000,
  });
  check("a 2nd asmux REFUSES to steal the live socket", thief.status !== 0, `exit=${thief.status}`);
  check(
    "...and says why",
    /refusing to displace/.test(thief.stderr || ""),
    (thief.stderr || "").split("\n")[0]?.slice(0, 60),
  );
  check("victim's socket still on disk after the theft attempt", existsSync(sb.socket));

  // The victim must still be *serving*, not merely present.
  const stillServing = await sb.api(`/api/sessions/${id}`);
  check(
    "victim holder still serving its session after the theft attempt",
    stillServing.session.status === "running",
    stillServing.session.status,
  );

  // ---- B. HEALING: unlink the path anyway; the holder must rebind ----
  unlinkSync(sb.socket);
  check("socket path unlinked (the damage that lost 6 sessions)", !existsSync(sb.socket));

  // The watchdog ticks every 5s.
  for (let i = 0; i < 30 && !existsSync(sb.socket); i++) await sleep(500);
  check("holder REBOUND its socket instead of silently orphaning", existsSync(sb.socket));

  // The real proof: restart the daemon. Before the fix this is where the sessions
  // died — the daemon could not reach the holder, and reconciled them away.
  sb.stop("daemon1");
  await sb.waitExit("daemon1", 5000);
  await sb.startDaemon("daemon2", SIDECAR);

  const after = await sb.api(`/api/sessions/${id}`);
  check(
    "session ADOPTED after restart — not lost to `indeterminate`",
    after.session.status === "running",
    `status=${after.session.status}`,
  );

  // ...and it is genuinely alive, not a replayed corpse.
  const c2 = sb.ws(id);
  await new Promise((r) => (c2.ws.onopen = r));
  await sleep(1200);
  check("scrollback survived the incident", c2.buf.includes(marker), marker);
  const marker2 = "ALIVE-" + Math.random().toString(36).slice(2, 8).toUpperCase();
  c2.ws.send(JSON.stringify({ t: "i", d: `echo ${marker2}\r` }));
  await sleep(900);
  c2.ws.close();
  check("session still accepts input after the incident", c2.buf.includes(marker2), marker2);

  await sb.api(`/api/sessions/${id}/stop`, { method: "POST" }).catch(() => {});
  return report("the holder survives theft and heals an unlinked socket — no sessions lost");
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
