#!/usr/bin/env node
// The phone-on-the-LAN flow, end to end.
//
// The daemon serves the web client itself, so opening `https://<lan-ip>:<port>`
// on a phone IS the app — same-origin, no host to add. But same-origin is not
// loopback: the daemon waives the token only for loopback peers, so that page
// must be able to enroll itself. It could not (connectionStore pinned the local
// entry to token:null), which left the phone stuck at "unauthorized" with no
// affordance, and made a cross-origin saved host the only way in.
//
// Sandboxed: its own port, data dir and TLS-terminating daemon on 0.0.0.0 — it
// never touches the real daemon on 4600. Needs `cargo build -p asm-daemon` and
// `cd client && npm run build` first.
//
// T1  a LAN-origin page is NOT auto-trusted: the daemon 401s it
// T2  the UI surfaces an enroll affordance for the same-origin daemon
// T3  enrolling from that page authorizes it (sessions/workspaces go 200)
// T4  the device token survives a reload (it is persisted for the local entry)
import { execFileSync } from "node:child_process";
import { existsSync } from "node:fs";
import { join } from "node:path";
import { createSandbox, cdpConnect, checker, DAEMON_BIN, ROOT, sleep } from "./lib/testenv.mjs";

// The sandbox daemon serves a self-signed cert; this harness is the "browser
// that already accepted it" (Chrome gets --ignore-certificate-errors below).
process.env.NODE_TLS_REJECT_UNAUTHORIZED = "0";

const CERT = join(ROOT, "cert.pem");
const KEY = join(ROOT, "key.pem");
// The cert's SAN must cover whatever host the page is loaded from.
const LAN_IP = "192.168.0.159";

const c = checker();
const sb = await createSandbox("lan-enroll");

try {
  if (!existsSync(CERT) || !existsSync(KEY)) throw new Error(`need ${CERT} + ${KEY}`);

  // A daemon on 0.0.0.0 with TLS, serving the built client: exactly what the
  // phone talks to. Loopback trust stays ON (the default) — the point is that
  // it does not extend to a LAN peer.
  sb.startProc("daemon", DAEMON_BIN, {
    ASM_BIND: `0.0.0.0:${sb.port}`,
    ASM_STATIC_DIR: join(ROOT, "client", "dist"),
    ASM_TLS_CERT: CERT,
    ASM_TLS_KEY: KEY,
  });

  const origin = `https://${LAN_IP}:${sb.port}`;
  for (let i = 0; i < 100; i++) {
    try {
      if ((await fetch(`${origin}/health`)).ok) break;
    } catch {
      /* not up yet */
    }
    await sleep(150);
  }

  // T1 — the LAN peer is not trusted, even though the page is same-origin.
  const bare = await fetch(`${origin}/api/sessions`);
  c.check("T1 LAN-origin page is not auto-trusted (401)", bare.status === 401, `got ${bare.status}`);

  const loop = await fetch(`https://127.0.0.1:${sb.port}/api/sessions`);
  c.check("T1b loopback peer still needs no token (200)", loop.status === 200, `got ${loop.status}`);

  // The token the user reads off the daemon host.
  const enrollToken = execFileSync(DAEMON_BIN, ["token"], { env: sb.env({}) }).toString().trim();

  // Fresh profile (no stored token), and standing in for a user who accepted the
  // self-signed cert — the interstitial is a browser ceremony, not the thing
  // under test here.
  const chrome = await sb.launchChrome(["--ignore-certificate-errors"]);
  const conn = cdpConnect(chrome.wsUrl);
  await conn.ready;

  const open = async (url) => {
    const { targetId } = await conn.send("Target.createTarget", { url });
    const { sessionId } = await conn.send("Target.attachToTarget", { targetId, flatten: true });
    await conn.send("Runtime.enable", {}, sessionId);
    await conn.send("Page.enable", {}, sessionId);
    const evalJs = async (expression) => {
      const { result, exceptionDetails } = await conn.send(
        "Runtime.evaluate",
        { expression, awaitPromise: true, returnByValue: true },
        sessionId,
      );
      if (exceptionDetails) throw new Error(exceptionDetails.text + " " + (result?.description ?? ""));
      return result?.value;
    };
    return { targetId, sessionId, evalJs };
  };

  const page = await open(origin + "/");
  await sleep(2500);

  // T2 — the connections dialog offers to enroll THIS device.
  const openDialog = `
    (() => {
      const btn = [...document.querySelectorAll("button, a")]
        .find((e) => /manage|connections/i.test(e.textContent || ""));
      if (btn) btn.click();
      return Boolean(btn);
    })()`;
  await page.evalJs(openDialog);
  await sleep(400);

  const enrollUi = await page.evalJs(`
    (() => {
      const txt = document.body.innerText;
      const input = [...document.querySelectorAll("input")]
        .find((i) => /token/i.test(i.placeholder || ""));
      return { hasTitle: /Enroll this device/i.test(txt), hasInput: Boolean(input) };
    })()`);
  c.check(
    "T2 same-origin daemon offers an enroll affordance",
    enrollUi.hasTitle && enrollUi.hasInput,
    JSON.stringify(enrollUi),
  );

  // T3 — enroll from the page itself, the way the user would.
  await page.evalJs(`
    (() => {
      const input = [...document.querySelectorAll("input")]
        .find((i) => /token/i.test(i.placeholder || ""));
      const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value").set;
      setter.call(input, ${JSON.stringify(enrollToken)});
      input.dispatchEvent(new Event("input", { bubbles: true }));
      const btn = [...document.querySelectorAll("button")]
        .find((b) => /^enroll$/i.test((b.textContent || "").trim()));
      btn.click();
      return true;
    })()`);
  await sleep(2500);

  const afterEnroll = await page.evalJs(`
    (() => {
      const stored = JSON.parse(localStorage.getItem("asm.daemons") || "[]");
      const local = stored.find((d) => d.id === "local");
      return { token: Boolean(local && local.token), body: document.body.innerText.slice(0, 200) };
    })()`);
  c.check("T3 enrolling stores a device token for the local daemon", afterEnroll.token, afterEnroll.body);

  const authed = await page.evalJs(`
    (async () => {
      const stored = JSON.parse(localStorage.getItem("asm.daemons") || "[]");
      const local = stored.find((d) => d.id === "local");
      const r = await fetch("/api/sessions", {
        headers: local.token ? { Authorization: "Bearer " + local.token } : {},
      });
      return r.status;
    })()`);
  c.check("T3b the enrolled token authorizes the API (200)", authed === 200, `got ${authed}`);

  // T4 — reload: the token must survive (the store used to wipe local's token).
  const page2 = await open(origin + "/");
  await sleep(2500);
  const reloaded = await page2.evalJs(`
    (() => {
      const txt = document.body.innerText;
      return {
        unauthorized: /unauthorized/i.test(txt),
        unreachable: /UNREACHABLE/.test(txt),
        text: txt.slice(0, 160),
      };
    })()`);
  c.check(
    "T4 after reload the LAN page is authorized (no unauthorized/unreachable)",
    !reloaded.unauthorized && !reloaded.unreachable,
    reloaded.text,
  );
} finally {
  await sb.cleanup();
}

process.exit(c.report("a LAN device can enroll the daemon-served UI it is already looking at") ? 0 : 1);
