// End-to-end proof of the saved startup config through the REAL service
// scripts: start.sh / restart-daemon.sh / stop.sh / status.sh against a
// sandboxed install (private --data-dir / --runtime-dir, a kernel-assigned
// port, no inherited ASM_* env). Covers: first start saves the resolved
// settings; a change saved through the daemon's /api/config is reported as
// pending and applied by the next FLAGLESS start (daemon restarted, holder and
// sessions kept); flags patch single keys; stop → start restores the saved
// setup; the bundled relay follows the saved switch; restart-daemon.sh leaves
// relay changes pending; the saved file beats inherited env; a broken
// hand-edit is refused while stop/status keep working; and an install started
// by the older scripts is migrated from its runtime records. Plus the three
// components (daemon / relay / web UI) switched independently — including a
// relay with no daemon, which stops the daemon but keeps the holder and its
// sessions — the enrollment token, and that none of the startup secrets leak
// into an agent session's environment.
//
//   cargo build                          # asm-daemon, asmux, asm-relay
//   node scripts/startup-config-test.mjs

import { execFileSync, spawnSync } from "node:child_process";
import { existsSync, readFileSync, writeFileSync, rmSync, statSync } from "node:fs";
import { homedir } from "node:os";
import { delimiter, join } from "node:path";
import { createSandbox, checker, cleanEnv, freePort, sleep, ROOT, BASH, IS_WINDOWS } from "./lib/testenv.mjs";

const { check, report } = checker();
const sb = await createSandbox("asm-startcfg");
const conf = join(sb.dataDir, "startup.conf");
const applied = join(sb.runDir, "startup.applied");
const base = sb.http;

// The scripts shell out to cargo; make sure it is reachable without a login shell.
const cargoBin = join(homedir(), ".cargo", "bin");
// Windows spells it `Path`; reuse whatever key the environment has, so the
// child does not end up with two differently-cased copies.
const pathKey = Object.keys(process.env).find((k) => k.toUpperCase() === "PATH") ?? "PATH";
const scriptEnv = (extra = {}) => ({
  ...cleanEnv(),
  [pathKey]: existsSync(cargoBin) ? `${cargoBin}${delimiter}${process.env[pathKey]}` : process.env[pathKey],
  XDG_RUNTIME_DIR: join(sb.tmp, "xdg"),
  XDG_DATA_HOME: join(sb.tmp, "xdg", "data"),
  XDG_CONFIG_HOME: join(sb.tmp, "xdg", "config"),
  // No packaged UI and no managed Vite: this test is about the settings.
  ASM_STATIC_DIR: "",
  ...extra,
});
const DIRS = ["--data-dir", sb.dataDir, "--runtime-dir", sb.runDir];

function run(script, args = [], extraEnv = {}) {
  const r = spawnSync(BASH, [join(ROOT, "scripts", script), ...DIRS, ...args], {
    env: scriptEnv(extraEnv),
    encoding: "utf8",
    timeout: 240_000,
  });
  return { code: r.status, out: (r.stdout ?? "") + (r.stderr ?? "") };
}

const parse = (path) =>
  Object.fromEntries(
    readFileSync(path, "utf8")
      .split("\n")
      .filter((l) => l && !l.startsWith("#") && l.includes("="))
      .map((l) => [l.slice(0, l.indexOf("=")).trim(), l.slice(l.indexOf("=") + 1).trim()]),
  );
const pid = (name) => {
  try {
    return readFileSync(join(sb.runDir, `${name}.pid`), "utf8").trim();
  } catch {
    return null;
  }
};
// The scripts restart the daemon under us, and undici would reuse a pooled
// keep-alive socket to the old process — retry that one failure mode.
async function retrying(fn) {
  for (let i = 0; ; i++) {
    try {
      return await fn();
    } catch (e) {
      if (i >= 4 || !/fetch failed|ECONNRESET|socket/.test(String(e.message))) throw e;
      await sleep(250);
    }
  }
}
const api = (path, init) => retrying(() => sb.api(path, init));
const health = () => retrying(async () => (await fetch(`${base}/health`)).json());
async function putConfig(values) {
  const { revision } = await api("/api/config");
  return api("/api/config", { method: "PUT", body: JSON.stringify({ revision, values }) });
}

async function main() {
  for (const bin of ["asm-daemon", "asmux", "asm-relay"]) {
    const exe = IS_WINDOWS ? `${bin}.exe` : bin;
    if (!existsSync(join(ROOT, "target", "debug", exe))) throw new Error(`missing target/debug/${exe} — run cargo build`);
  }

  // ── first start: flags are resolved and saved ────────────────────────────
  const TOKEN1 = "first-enroll-token-01";
  let r = run("start.sh", ["--bind", sb.base, "--no-ui", "--label", "first", "--enrollment-token", TOKEN1]);
  check("first start succeeds", r.code === 0, r.code === 0 ? "" : r.out.slice(-800));
  check("…says the settings are new", /settings — defaults \+ flags/.test(r.out));
  // Windows has no mode bits to check: the file inherits the profile dir's owner-only ACL.
  check("startup.conf saved, owner-only", existsSync(conf) && (IS_WINDOWS || (statSync(conf).mode & 0o777) === 0o600));
  let saved = parse(conf);
  check(
    "…with the resolved settings",
    saved.ASM_BIND === sb.base && saved.ASM_NODE_LABEL === "first" && saved.ASM_RUN_UI === "0" &&
      saved.ASM_RUN_DAEMON === "1" && saved.ASM_RUN_RELAY === "0" && saved.ASM_ENROLLMENT_TOKEN === TOKEN1,
  );
  check("…and the applied snapshot matches", existsSync(applied) && readFileSync(applied, "utf8") === readFileSync(conf, "utf8"));
  check("daemon runs with them", (await health()).label === "first");
  const holderPid = pid("asmux");
  const enroll = (token) =>
    retrying(() =>
      fetch(`${base}/api/auth/enroll`, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ enrollment_token: token, device_name: "t" }),
      }),
    );
  check("the configured enrollment token enrolls a device", (await enroll(TOKEN1)).ok);
  check("token.sh prints it", run("token.sh").out.trim().split("\n").pop() === TOKEN1);

  // A live session must survive every settings change below.
  const { session } = await api("/api/sessions", {
    method: "POST",
    body: JSON.stringify({ agent_plugin_id: "shell", cwd: sb.cwd }),
  });

  // The scripts export the startup secrets to what they launch; the holder must
  // not hand them to an agent's shell (non-secret settings still come through).
  const term = new WebSocket(`ws://${sb.base}/api/sessions/${session.id}/stream`);
  term.binaryType = "arraybuffer";
  let screen = "";
  term.onmessage = (ev) => (screen += typeof ev.data === "string" ? ev.data : Buffer.from(ev.data).toString());
  await new Promise((res) => (term.onopen = res));
  // The Shell agent is PowerShell on Windows, where env vars read `$env:NAME`.
  const envRef = (name) => (IS_WINDOWS ? `$env:${name}` : `$${name}`);
  term.send(JSON.stringify({ t: "i", d: `echo "bind=[${envRef("ASM_BIND")}] enroll=[${envRef("ASM_ENROLLMENT_TOKEN")}]"\r` }));
  for (let i = 0; i < 150 && !/bind=\[127/.test(screen); i++) await sleep(100);
  term.close();
  check("session env keeps plain settings", screen.includes(`bind=[${sb.base}]`));
  check("…but not the enrollment token", screen.includes("enroll=[]") && !screen.includes(TOKEN1.slice(0, 12) + "]"));

  // ── a change saved from the web UI is pending until the next flagless start
  const view = await putConfig({ ASM_NODE_LABEL: "second" });
  check("API save reports the change as pending", JSON.stringify(view.pending) === '["ASM_NODE_LABEL"]');
  r = run("status.sh");
  check("status.sh lists the pending setting", /not applied yet: ASM_NODE_LABEL/.test(r.out));
  const daemonPid = pid("asm-daemon");
  r = run("start.sh");
  check("flagless start uses the saved file", /settings — .*startup\.conf$/m.test(r.out));
  check("…restarts the daemon to apply it", /config changed, restarting/.test(r.out) && pid("asm-daemon") !== daemonPid);
  check("…new label live", (await health()).label === "second");
  check("…nothing pending any more", JSON.stringify((await api("/api/config")).pending) === "[]");
  check("…holder untouched", pid("asmux") === holderPid);
  const s1 = await api(`/api/sessions/${session.id}`);
  check("…live session survived", s1.session?.status === "running", s1.session?.status);

  // A flagless start with nothing changed restarts nothing.
  const steady = pid("asm-daemon");
  r = run("start.sh");
  check("unchanged flagless start leaves the daemon alone", /daemon already running/.test(r.out) && pid("asm-daemon") === steady);

  // ── a new enrollment token saved from the web UI applies on the next start ─
  const TOKEN2 = "second-enroll-token-2";
  const tooShort = await retrying(async () => {
    const { revision } = await api("/api/config");
    return fetch(`${base}/api/config`, {
      method: "PUT",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ revision, values: { ASM_ENROLLMENT_TOKEN: "short" } }),
    });
  });
  check("a weak enrollment token is refused by the API", tooShort.status === 400);
  const tokView = await putConfig({ ASM_ENROLLMENT_TOKEN: TOKEN2 });
  check("…a good one is saved write-only", tokView.secrets.ASM_ENROLLMENT_TOKEN === true && tokView.values.ASM_ENROLLMENT_TOKEN === undefined);
  r = run("start.sh");
  check("flagless start restarts the daemon for the new token", /config changed, restarting/.test(r.out));
  check("…new token enrolls", (await enroll(TOKEN2)).ok);
  check("…old one no longer does", (await enroll(TOKEN1)).status === 401);

  // ── flags patch single settings ───────────────────────────────────────────
  r = run("start.sh", ["--label", "third"]);
  saved = parse(conf);
  check("--label changes just the label", saved.ASM_NODE_LABEL === "third" && saved.ASM_BIND === sb.base && saved.ASM_RUN_UI === "0");
  check("…saved back", /\+ flags \(saved back\)/.test(r.out));

  // ── the saved file beats inherited env ────────────────────────────────────
  r = run("start.sh", [], { ASM_BIND: "127.0.0.1:1", ASM_NODE_LABEL: "leaked" });
  check("inherited ASM_* env is ignored once saved", /daemon already running/.test(r.out) && (await health()).label === "third");

  // ── bundled relay follows the saved switch ────────────────────────────────
  const relayPort = await freePort();
  r = run("start.sh", ["--relay", "--relay-key", "k1", "--relay-bind", `127.0.0.1:${relayPort}`]);
  check("--relay starts a relay and saves it", /relay up/.test(r.out) && parse(conf).ASM_RUN_RELAY === "1" && parse(conf).ASM_RELAY_KEYS === "k1");
  const relayUp = await retrying(() => fetch(`http://127.0.0.1:${relayPort}/nodes`, { headers: { "X-ASM-Relay-Key": "k1" } }));
  check("…and it answers", relayUp.ok);
  await putConfig({ ASM_RUN_RELAY: "0" });
  r = run("restart-daemon.sh");
  check("restart-daemon.sh leaves the relay, reports it pending", r.code === 0 && /not applied by a daemon restart: ASM_RUN_RELAY/.test(r.out) && !!pid("asm-relay"));
  r = run("start.sh");
  check("flagless start stops the relay the settings turned off", /no longer enabled in the settings — stopping it/.test(r.out) && !pid("asm-relay"));
  check("…key kept for next time", parse(conf).ASM_RELAY_KEYS === "k1");

  // ── components are independent: a relay with no daemon, then back ────────
  const relayPort2 = await freePort();
  r = run("start.sh", ["--no-daemon", "--relay", "--relay-key", "k2", "--relay-bind", `127.0.0.1:${relayPort2}`]);
  check("daemon off + relay on starts", r.code === 0 && /components — relay$/m.test(r.out));
  check("…stops the daemon", /daemon is off in the settings — stopping it/.test(r.out) && !pid("asm-daemon"));
  check("…keeps the holder (and its sessions)", pid("asmux") === holderPid && /session holder is still up/.test(r.out));
  const relay2 = await retrying(() => fetch(`http://127.0.0.1:${relayPort2}/nodes`, { headers: { "X-ASM-Relay-Key": "k2" } }));
  check("…runs the relay", relay2.ok);
  check("…saved as such", parse(conf).ASM_RUN_DAEMON === "0" && parse(conf).ASM_RUN_RELAY === "1");
  r = run("restart-daemon.sh");
  check("restart-daemon.sh refuses while the daemon is off", r.code === 2 && /daemon is off/.test(r.out));
  r = run("start.sh", ["--no-relay"]);
  check("switching everything off is refused", r.code === 2 && /nothing to start/.test(r.out));
  r = run("start.sh", ["--daemon", "--no-relay"]);
  check("daemon back on", r.code === 0 && !!pid("asm-daemon") && !pid("asm-relay"));
  const back = await api(`/api/sessions/${session.id}`);
  check("…re-adopts the session the holder kept", back.session?.status === "running", back.session?.status);

  // ── stop keeps the settings; start restores them ──────────────────────────
  r = run("stop.sh");
  check("stop.sh clears the runtime state but not the settings", !existsSync(applied) && existsSync(conf) && !pid("asm-daemon"));
  r = run("start.sh");
  check("flagless start after stop restores the saved setup", r.code === 0 && (await health()).label === "third");
  check("…at the saved bind", parse(conf).ASM_BIND === sb.base);

  // ── a broken hand edit is refused; stop/status still work ────────────────
  const good = readFileSync(conf, "utf8");
  writeFileSync(conf, good.replace(/^ASM_BIND=.*$/m, "ASM_BIND=nowhere"));
  r = run("start.sh");
  check("invalid saved value refuses to start, naming the key", r.code === 2 && /invalid startup setting ASM_BIND=nowhere/.test(r.out));
  r = run("status.sh");
  check("status.sh still works with the broken file", r.code === 0 && /daemon  RUNNING/.test(r.out));
  writeFileSync(conf, good + "ASM_BOGUS=1\r\nASM_NODE_LABEL=crlf\r\n");
  r = run("start.sh");
  check("unknown keys are warned about, CRLF tolerated", r.code === 0 && /ignoring unknown setting ASM_BOGUS/.test(r.out) && (await health()).label === "crlf");
  check("…and the rewrite drops them", !/ASM_BOGUS/.test(readFileSync(conf, "utf8")));

  // ── migration from the older scripts' runtime records ─────────────────────
  // Those records never carried an enrollment token (the daemon's own record
  // only holds a checksum of one), so model that stack: no configured token.
  // Clearing it keeps the daemon's current token in its database.
  await putConfig({ ASM_ENROLLMENT_TOKEN: "" });
  run("start.sh");
  check("clearing the configured token keeps the current one", (await enroll(TOKEN2)).ok);
  rmSync(conf);
  const before = pid("asm-daemon");
  r = run("start.sh");
  check("no saved file: settings recovered from the running stack", /recovered from the running stack/.test(r.out));
  check("…without restarting it", /daemon already running/.test(r.out) && pid("asm-daemon") === before);
  check("…and saved", existsSync(conf) && parse(conf).ASM_NODE_LABEL === "crlf" && parse(conf).ASM_BIND === sb.base);

  // ── argument errors ───────────────────────────────────────────────────────
  r = run("start.sh", ["--relay-key", "x"]);
  check("a --relay-key with nothing to apply it to is refused", r.code === 2 && /--relay-key needs/.test(r.out));
  r = run("start.sh", ["--ui-only", "--bind", "0.0.0.0:4600"]);
  check("--ui-only with daemon flags is refused", r.code === 2 && /conflict/.test(r.out));
  r = run("start.sh", ["--relay-only", "--ui-port", "6000"]);
  check("--relay-only with UI flags is refused", r.code === 2 && /conflict/.test(r.out));

  run("stop.sh");
}

try {
  await main();
} catch (e) {
  check(`unexpected error: ${e.message}`, false);
} finally {
  // Belt and braces: the scripts detach their children; stop.sh ran above, but
  // make sure nothing outlives a failed run.
  for (const name of ["asm-daemon", "asmux", "asm-relay"]) {
    const p = pid(name);
    if (p) {
      try {
        execFileSync("kill", ["-9", p]);
      } catch {
        /* gone */
      }
    }
  }
  if (process.env.ASM_TEST_KEEP) console.log("kept", sb.tmp); else sb.cleanup();
}
process.exit(report("saved startup config through start/restart/stop/status") ? 0 : 1);
