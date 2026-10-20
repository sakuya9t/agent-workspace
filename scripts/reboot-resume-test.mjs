// Host-reboot resume test — sessions lost with the host come back in place.
//
//   node scripts/reboot-resume-test.mjs
//
// A real reboot can't be run in a test, so it is staged: the daemon stops, the
// holder is killed (taking every session's process with it, as a reboot does),
// and the sessions' `boot_id` is rewritten to an earlier boot's — the one thing
// a reboot changes that the daemon reads. Then, in a sandbox
// (scripts/lib/testenv.mjs):
//
//   1. a shell, a Claude session (a fake `claude` that prints its argv) and a
//      custom command run under holder #1 / daemon #1
//   2. "reboot": daemon #1 stops, holder #1 is killed, boot ids are rewound
//   3. holder #2 + daemon #2 come up and must relaunch each session in place:
//      - the shell: same id, `running`, old output + banner, accepts input
//      - Claude: resumed with `--resume <its conversation>` and its
//        skip-permissions flag, never `--fork-session`
//      - the custom command: NOT re-run, still `indeterminate`, says why
//   4. daemon #3 (holder #2 still alive) must ADOPT the relaunched shell with
//      its whole history exactly once — proving the relaunch reset the holder
//      cursor rather than leaving the dead holder's behind
//
// Requires the built binaries (`cargo build`) and Node 22+ (`node:sqlite`).

import { chmodSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { DatabaseSync } from "node:sqlite";
import { createSandbox, checker, sleep } from "./lib/testenv.mjs";

const { check, report } = checker();
const sb = await createSandbox("asm-reboot");

// A stand-in `claude`: prints its argv, then echoes input like a live TUI would.
const fakeBin = join(sb.tmp, "bin");
mkdirSync(fakeBin, { recursive: true });
writeFileSync(
  join(fakeBin, "claude"),
  `#!/bin/sh\nprintf 'FAKE-CLAUDE-ARGV:'\nfor a in "$@"; do printf ' [%s]' "$a"; done\nprintf '\\n'\nexec cat\n`,
);
chmodSync(join(fakeBin, "claude"), 0o755);

const DAEMON = {
  ASM_BACKEND: "sidecar",
  ASM_ASMUX_AUTOSPAWN: "0",
  PATH: `${fakeBin}:${process.env.PATH}`,
};
const NATIVE_ID = "conv-e2e-" + Math.random().toString(36).slice(2, 10);
const BOOT_ID = readFileSync("/proc/sys/kernel/random/boot_id", "utf8").trim();

/** Attach, let the snapshot arrive, optionally type, and return everything seen. */
async function attach(id, input, waitMs = 900) {
  const c = sb.ws(id);
  await new Promise((res, rej) => {
    c.ws.onopen = res;
    c.ws.onerror = rej;
  });
  await sleep(500);
  if (input) {
    c.ws.send(JSON.stringify({ t: "i", d: input }));
    await sleep(waitMs);
  }
  c.ws.close();
  return c.buf;
}

const count = (hay, needle) => hay.split(needle).length - 1;
const session = async (id) => (await sb.api(`/api/sessions/${id}`)).session;

async function main() {
  // 1. sessions under holder #1 / daemon #1 -----------------------------------
  await sb.startAsmux();
  await sb.startDaemon("daemon1", DAEMON);

  const create = (body) =>
    sb.api("/api/sessions", { method: "POST", body: JSON.stringify({ cwd: sb.cwd, ...body }) });
  const shell = (await create({ agent_plugin_id: "shell" })).session;
  const claude = (await create({ agent_plugin_id: "claude", options: { skip_permissions: true } }))
    .session;
  const custom = (
    await create({
      agent_plugin_id: "custom_command",
      command: "sleep",
      args: ["600"],
      approve_custom: true,
    })
  ).session;
  check(
    "three sessions running before the reboot",
    [shell, claude, custom].every((s) => s.status === "running"),
    [shell, claude, custom].map((s) => s.status).join(","),
  );

  // `$((…))` so the output differs from the typed command: each marker below
  // appears in the stream exactly once per time it really ran.
  const before = await attach(shell.id, "echo BEFORE-$((40+2))\r");
  check("shell output before the reboot", before.includes("BEFORE-42"));

  // 2. the "reboot" ------------------------------------------------------------
  sb.stop("daemon1");
  await sb.waitExit("daemon1", 5000);
  sb.stop("asmux"); // the holder goes, and every session's process with it
  await sb.waitExit("asmux", 5000);

  {
    const db = new DatabaseSync(join(sb.dataDir, "asm.sqlite3"));
    const stamped = db.prepare("SELECT COUNT(*) AS n FROM sessions WHERE boot_id = ?").get(BOOT_ID);
    check("every session was stamped with this boot", stamped.n === 3, `n=${stamped.n}`);
    db.prepare("UPDATE sessions SET boot_id = 'boot-before-the-reboot'").run();
    // What the monitor captures from a real Claude transcript.
    db.prepare("UPDATE sessions SET agent_session_id = ? WHERE id = ?").run(NATIVE_ID, claude.id);
    db.close();
  }

  // 3. holder #2 + daemon #2 relaunch them --------------------------------------
  await sb.startAsmux();
  await sb.startDaemon("daemon2", DAEMON);

  const shell2 = await session(shell.id);
  check("shell is running again under its own id", shell2.status === "running", shell2.status);
  const view = await attach(shell.id, "echo AFTER-$((40+3))\r");
  check("its old output is still there", view.includes("BEFORE-42"));
  check("a banner says the host restarted", view.includes("the host restarted"));
  check("and the new shell takes input", view.includes("AFTER-43"));

  const claude2 = await session(claude.id);
  check("claude is running again under its own id", claude2.status === "running", claude2.status);
  const cview = await attach(claude.id);
  const argv = cview.split("FAKE-CLAUDE-ARGV:").pop().split("\n")[0];
  check(
    "claude resumed its own conversation",
    argv.includes(`[--resume] [${NATIVE_ID}]`),
    argv.trim().slice(0, 160),
  );
  check("…with its skip-permissions flag", argv.includes("[--dangerously-skip-permissions]"));
  check("…and not as a fork", !argv.includes("--fork-session"));
  check(
    "…with no opening prompt",
    argv.trim().endsWith(`[${NATIVE_ID}]`),
    argv.trim().slice(-60),
  );

  const custom2 = await session(custom.id);
  check(
    "the custom command was not re-run",
    custom2.status === "indeterminate",
    custom2.status,
  );
  check(
    "…and says why",
    (custom2.attention_reason ?? "").includes("never re-run unattended"),
    custom2.attention_reason,
  );

  {
    const db = new DatabaseSync(join(sb.dataDir, "asm.sqlite3"));
    const row = db.prepare("SELECT boot_id FROM sessions WHERE id = ?").get(shell.id);
    check("the relaunched shell is stamped with this boot", row.boot_id === BOOT_ID, row.boot_id);
    db.close();
  }

  // 4. daemon #3 adopts the relaunched sessions with their history intact -----
  sb.stop("daemon2");
  await sb.waitExit("daemon2", 5000);
  await sb.startDaemon("daemon3", DAEMON);

  const shell3 = await session(shell.id);
  check("daemon restart adopts the relaunched shell", shell3.status === "running", shell3.status);
  const replay = await attach(shell.id, "echo AGAIN-$((40+4))\r");
  check("history from before the reboot, once", count(replay, "BEFORE-42") === 1, count(replay, "BEFORE-42"));
  check("history from after it, once", count(replay, "AFTER-43") === 1, count(replay, "AFTER-43"));
  check("no restart-gap marker", !replay.includes("were not recorded"));
  check("the adopted shell still takes input", replay.includes("AGAIN-44"));
  const claude3 = await session(claude.id);
  check("daemon restart adopts the resumed claude", claude3.status === "running", claude3.status);

  return report("sessions lost to a host reboot come back in place");
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
