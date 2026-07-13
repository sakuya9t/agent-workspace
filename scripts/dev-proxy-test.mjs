#!/usr/bin/env node
// The "client on another box" topology: a Vite dev server on host A proxying to
// a TLS daemon on host B, with the browser (phone) on A's plain-http origin.
//
// Two things break there and both are covered here:
//
//  1. The proxy target defaulted to `http://127.0.0.1:4600`. A daemon holding
//     ASM_TLS_CERT serves TLS on its ONLY listener — loopback included — so the
//     proxy spoke plaintext at a TLS socket and Vite reported the daemon "down"
//     (502) while it was up and healthy. ASM_DAEMON must carry the scheme, and
//     an https target needs `secure: false` (self-signed is the norm for a
//     daemon reached by IP, and a server-side proxy has no interstitial).
//
//  2. The daemon then judges the PROXY's address, not the browser's. A remote
//     dev server dials from its LAN address, which is not loopback, so the page
//     is same-origin yet unauthorized — and must be able to enroll itself.
//
// Sandboxed daemon (own port + data dir); the real one on 4600 is never touched.
// Needs `cargo build -p asm-daemon` and `cd client && npm install`.
import { execFileSync, spawn } from "node:child_process";
import { existsSync, openSync } from "node:fs";
import { join } from "node:path";
import {
  cdpConnect,
  checker,
  createSandbox,
  DAEMON_BIN,
  freePort,
  ROOT,
  sleep,
} from "./lib/testenv.mjs";

process.env.NODE_TLS_REJECT_UNAUTHORIZED = "0"; // this harness = a browser that accepted the cert

const CERT = join(ROOT, "cert.pem");
const KEY = join(ROOT, "key.pem");
const LAN_IP = "192.168.0.159"; // must be in the cert's SAN, and not loopback

const c = checker();
const sb = await createSandbox("dev-proxy");
let vite;

try {
  if (!existsSync(CERT) || !existsSync(KEY)) throw new Error(`need ${CERT} + ${KEY}`);

  sb.startProc("daemon", DAEMON_BIN, {
    ASM_BIND: `0.0.0.0:${sb.port}`,
    ASM_TLS_CERT: CERT,
    ASM_TLS_KEY: KEY,
  });
  const daemonUrl = `https://${LAN_IP}:${sb.port}`;
  for (let i = 0; i < 100; i++) {
    try {
      if ((await fetch(`${daemonUrl}/health`)).ok) break;
    } catch {
      /* not up yet */
    }
    await sleep(150);
  }

  // The dev server, pointed at the TLS daemon the way a remote client host must.
  const vitePort = await freePort();
  const log = openSync(sb.logPath("vite"), "a");
  vite = spawn(
    "npx",
    ["vite", "--port", String(vitePort), "--host", "0.0.0.0", "--strictPort"],
    {
      cwd: join(ROOT, "client"),
      // sb.env() (not a hand-rolled {...process.env}) — it strips inherited
      // ASM_*/ASMUX_* and repoints the runtime dir into the sandbox, so this
      // child can never reach the prod holder socket.
      env: sb.env({ ASM_DAEMON: daemonUrl }),
      stdio: ["ignore", log, log],
    },
  );
  const origin = `http://${LAN_IP}:${vitePort}`;
  for (let i = 0; i < 120; i++) {
    try {
      if ((await fetch(`${origin}/`)).ok) break;
    } catch {
      /* not up yet */
    }
    await sleep(250);
  }

  // T1 — the regression: the proxy must actually reach a TLS daemon.
  const health = await fetch(`${origin}/health`);
  const healthBody = health.ok ? await health.json() : null;
  c.check(
    "T1 dev proxy reaches the TLS daemon (was 502 'daemon down' on a live daemon)",
    health.status === 200 && healthBody?.status === "ok",
    `got ${health.status}`,
  );

  // T2 — the proxy is an off-loopback peer, so the daemon still demands a token.
  const unauth = await fetch(`${origin}/api/sessions`);
  c.check("T2 proxied API is unauthorized without a token (401)", unauth.status === 401, `got ${unauth.status}`);

  const enrollToken = execFileSync(DAEMON_BIN, ["token"], { env: sb.env({}) }).toString().trim();

  // The phone: plain http to the dev server, never TLS — so no certificate
  // interstitial is involved anywhere in this flow.
  const chrome = await sb.launchChrome();
  const conn = cdpConnect(chrome.wsUrl);
  await conn.ready;
  const { targetId } = await conn.send("Target.createTarget", { url: origin + "/" });
  const { sessionId } = await conn.send("Target.attachToTarget", { targetId, flatten: true });
  await conn.send("Runtime.enable", {}, sessionId);
  // cdpConnect's send() already unwraps the CDP envelope to `result`.
  const evalJs = async (expression) => {
    const { result, exceptionDetails } = await conn.send(
      "Runtime.evaluate",
      { expression, awaitPromise: true, returnByValue: true },
      sessionId,
    );
    if (exceptionDetails) throw new Error(exceptionDetails.text ?? "evaluate failed");
    return result?.value;
  };
  await sleep(3500);

  await evalJs(
    `(() => { const b = [...document.querySelectorAll("button,a")].find(e => /manage|connections/i.test(e.textContent||"")); if (b) b.click(); return !!b; })()`,
  );
  await sleep(600);

  const ui = await evalJs(`(() => {
    const input = [...document.querySelectorAll("input")].find(i => /token/i.test(i.placeholder||""));
    return { title: /Enroll this device/i.test(document.body.innerText), input: Boolean(input) };
  })()`);
  c.check(
    "T3 the proxied same-origin daemon offers an enroll affordance",
    ui.title && ui.input,
    JSON.stringify(ui),
  );

  await evalJs(`(() => {
    const input = [...document.querySelectorAll("input")].find(i => /token/i.test(i.placeholder||""));
    const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value").set;
    setter.call(input, ${JSON.stringify(enrollToken)});
    input.dispatchEvent(new Event("input", { bubbles: true }));
    [...document.querySelectorAll("button")].find(b => /^enroll$/i.test((b.textContent||"").trim())).click();
    return true;
  })()`);
  await sleep(2500);

  const status = await evalJs(`(async () => {
    const local = JSON.parse(localStorage.getItem("asm.daemons") || "[]").find(d => d.id === "local");
    if (!local || !local.token) return "no token stored";
    const r = await fetch("/api/sessions", { headers: { Authorization: "Bearer " + local.token } });
    return r.status;
  })()`);
  c.check("T4 after enrolling, the proxied API authorizes (200)", status === 200, `got ${status}`);

  const clean = await evalJs(
    `(() => ({ unauth: /unauthorized/i.test(document.body.innerText), unreach: /UNREACHABLE/.test(document.body.innerText) }))()`,
  );
  c.check(
    "T5 no unauthorized/unreachable left in the UI",
    !clean.unauth && !clean.unreach,
    JSON.stringify(clean),
  );
} finally {
  if (vite) vite.kill("SIGTERM");
  await sb.cleanup();
}

process.exit(c.report("a dev server on another host can proxy to a TLS daemon and enroll") ? 0 : 1);
