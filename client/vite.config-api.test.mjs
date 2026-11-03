import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, rmSync, statSync, writeFileSync, mkdirSync } from "node:fs";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createConfigHandler, CONFIG_API_PATH } from "./vite.config-api.ts";
import {
  CONFIG_KEYS,
  configRevision,
  parseConfigFile,
  pendingKeys,
  renderConfigFile,
  validateConfig,
  validateValue,
} from "./src/startupConfig.ts";

let failures = 0;
async function check(name, fn) {
  try {
    await fn();
    console.log(`  ok  - ${name}`);
  } catch (e) {
    failures++;
    console.log(`  FAIL- ${name}\n        ${e.message}`);
  }
}

// ── the shared schema (mirrors crates/daemon/src/startup_config.rs) ─────────

await check("revision is FNV-1a 64, identical to the daemon's", () => {
  assert.equal(configRevision(""), "cbf29ce484222325");
  assert.equal(configRevision("a"), "af63dc4c8601ec8c");
});

await check("parse skips noise, trims, tolerates CRLF; render round-trips in key order", () => {
  const parsed = parseConfigFile(
    "# c\n\nASM_BIND = 0.0.0.0:4600 \r\nASM_NODE_LABEL=a = b\nNOT_OURS=1\nnope\nASM_RELAY_URL=\n",
  );
  assert.deepEqual(parsed, { ASM_BIND: "0.0.0.0:4600", ASM_NODE_LABEL: "a = b", ASM_RELAY_URL: "" });
  const text = renderConfigFile({ ASM_UI_PORT: "8080" });
  const keys = text.split("\n").filter((l) => l && !l.startsWith("#")).map((l) => l.split("=")[0]);
  assert.deepEqual(keys, [...CONFIG_KEYS]);
  assert.equal(parseConfigFile(text).ASM_UI_PORT, "8080");
});

await check("value rules match the daemon's", () => {
  const ok = [
    ["ASM_BIND", "0.0.0.0:4600"],
    ["ASM_BIND", "[::1]:4600"],
    ["ASM_RELAY_URL", "wss://relay.example.com"],
    ["ASM_RELAY_KEYS", "a,b , c"],
    ["ASM_RELAY_DOWNSTREAMS", "10.0.0.5:4600,box.lan:4601,[fd00::1]:4600"],
    ["ASM_UI_HOST", "[::]"],
    ["ASM_UI_ALLOWED_HOSTS", "pi.tail.ts.net,box"],
    ["ASM_UI_DAEMON", "http://10.0.0.2:4600"],
    ["ASM_NODE_LABEL", "my box ✓"],
    ["ASM_ENROLLMENT_TOKEN", "0123456789abcdef"],
    ["ASM_BIND", ""],
  ];
  for (const [k, v] of ok) assert.equal(validateValue(k, v), null, `${k}=${v}`);
  const bad = [
    ["ASM_PROFILE", "fast", "profile"],
    ["ASM_BIND", "localhost:4600", "socketAddr"],
    ["ASM_BIND", "0.0.0.0:0", "socketAddr"],
    ["ASM_BIND", "256.0.0.1:4600", "socketAddr"],
    ["ASM_RELAY_URL", "http://relay:4700", "wsUrl"],
    ["ASM_RELAY_KEY", "a,b", "token"],
    ["ASM_RELAY_KEYS", "a,,b", "keyList"],
    ["ASM_RELAY_DOWNSTREAMS", "box", "downstreams"],
    ["ASM_UI_PORT", "70000", "port"],
    ["ASM_UI_ALLOWED_HOSTS", "*.ts.net", "hostList"],
    ["ASM_RUN_UI", "true", "bool"],
    ["ASM_NODE_LABEL", "line\nbreak", "multiline"],
    ["ASM_NODE_LABEL", "a|b", "pipe"],
    ["ASM_ENROLLMENT_TOKEN", "short", "enrollmentToken"],
    ["ASM_RUN_DAEMON", "yes", "bool"],
  ];
  for (const [k, v, code] of bad) assert.equal(validateValue(k, v), code, `${k}=${JSON.stringify(v)}`);
});

await check("any non-empty set of components is valid; settings checked only when their component runs", () => {
  const codes = (v, secretSet) => validateConfig(v, secretSet).map((e) => e.code);
  for (const d of ["0", "1"]) {
    for (const r of ["0", "1"]) {
      for (const u of ["0", "1"]) {
        const v = { ASM_RUN_DAEMON: d, ASM_RUN_RELAY: r, ASM_RUN_UI: u, ASM_RELAY_KEYS: "k" };
        const none = d === "0" && r === "0" && u === "0";
        assert.deepEqual(codes(v), none ? ["noComponent"] : [], JSON.stringify(v));
      }
    }
  }
  assert.deepEqual(codes({}), [], "empty switches are the defaults (daemon + UI)");
  assert.deepEqual(codes({ ASM_RUN_RELAY: "1" }), ["relayNeedsKeys"]);
  assert.deepEqual(codes({ ASM_RUN_RELAY: "1" }, (k) => k === "ASM_RELAY_KEYS"), []);
  assert.deepEqual(codes({ ASM_RELAY_URL: "ws://r:4700" }), ["registerNeedsKey"]);
  assert.deepEqual(codes({ ASM_RELAY_URL: "ws://r:4700", ASM_RUN_DAEMON: "0" }), [], "registration unused without a daemon");
});

await check("pending compares saved vs applied, absent ≡ empty", () => {
  assert.deepEqual(pendingKeys({ ASM_BIND: "a", ASM_NODE_LABEL: "" }, { ASM_BIND: "b" }), ["ASM_BIND"]);
});

// ── the /__asm/config endpoint ──────────────────────────────────────────────

const tmp = mkdtempSync(join(tmpdir(), "asm-config-api-"));
const env = {
  ASM_DATA_DIR: join(tmp, "data"),
  ASM_RUNTIME_DIR: join(tmp, "run"),
  ASM_UI_MANAGED: "1",
  ASM_UI_PORT: "6000", // an env seed, used only while there is no file
  ASM_UI_DAEMON_TOKEN: "seeded-token",
};
const handler = createConfigHandler(env);
const server = createServer((req, res) =>
  handler(req, res, () => {
    res.statusCode = 404;
    res.end("fallthrough");
  }),
);
await new Promise((r) => server.listen(0, "127.0.0.1", r));
const base = `http://127.0.0.1:${server.address().port}`;
const url = base + CONFIG_API_PATH;
const put = (body, headers = {}) =>
  fetch(url, {
    method: "PUT",
    headers: { "content-type": "application/json", ...headers },
    body: typeof body === "string" ? body : JSON.stringify(body),
  });
const confPath = join(env.ASM_DATA_DIR, "startup.conf");

try {
  let first;
  await check("GET with no file: seeded from env, secrets reported as set only", async () => {
    const res = await fetch(url);
    assert.equal(res.status, 200);
    first = await res.json();
    assert.equal(first.source, "ui");
    assert.equal(first.exists, false);
    assert.equal(first.revision, null);
    assert.equal(first.managed, true);
    assert.equal(first.path, confPath);
    assert.equal(first.values.ASM_UI_PORT, "6000");
    assert.equal(first.values.ASM_UI_DAEMON_TOKEN, undefined);
    assert.equal(first.secrets.ASM_UI_DAEMON_TOKEN, true);
    assert.equal(first.pending, null);
  });

  await check("other paths fall through to Vite", async () => {
    assert.equal((await fetch(base + "/__asm/configx")).status, 404);
  });

  await check("cross-origin and non-JSON writes are refused before touching disk", async () => {
    assert.equal((await put({ revision: null, values: {} }, { origin: "http://evil.example" })).status, 403);
    assert.equal((await put({ revision: null, values: {} }, { "sec-fetch-site": "cross-site" })).status, 403);
    assert.equal((await put(JSON.stringify({ revision: null, values: {} }), { "content-type": "text/plain" })).status, 415);
    assert.throws(() => statSync(confPath));
  });

  let saved;
  await check("PUT writes an owner-only file, keeps unsent secrets, never echoes them", async () => {
    const res = await put(
      { revision: null, values: { ASM_UI_HOST: "0.0.0.0", ASM_UI_ALLOWED_HOSTS: "pi.example.ts.net" } },
      { origin: base },
    );
    assert.equal(res.status, 200, await res.clone().text());
    saved = await res.json();
    assert.equal(saved.exists, true);
    assert.equal(saved.values.ASM_UI_HOST, "0.0.0.0");
    assert.equal(saved.values.ASM_UI_DAEMON_TOKEN, undefined);
    // Windows has no mode bits (Node reports 0o666); the profile dir's ACL
    // is what keeps the file owner-only there.
    if (process.platform !== "win32") assert.equal(statSync(confPath).mode & 0o777, 0o600);
    const onDisk = parseConfigFile(readFileSync(confPath, "utf8"));
    assert.equal(onDisk.ASM_UI_DAEMON_TOKEN, "seeded-token");
    assert.equal(onDisk.ASM_UI_PORT, "6000");
    assert.equal(saved.revision, configRevision(readFileSync(confPath, "utf8")));
  });

  await check("stale revision → 409; unknown key / invalid value → 400; file untouched", async () => {
    const before = readFileSync(confPath, "utf8");
    assert.equal((await put({ revision: null, values: { ASM_UI_PORT: "7000" } })).status, 409);
    assert.equal((await put({ revision: saved.revision, values: { ASM_EVIL: "1" } })).status, 400);
    const bad = await put({ revision: saved.revision, values: { ASM_UI_PORT: "99999" } });
    assert.equal(bad.status, 400);
    assert.match((await bad.json()).error, /ASM_UI_PORT/);
    assert.equal(readFileSync(confPath, "utf8"), before);
  });

  await check("clearing a secret with an empty string", async () => {
    const res = await put({ revision: saved.revision, values: { ASM_UI_DAEMON_TOKEN: "" } });
    assert.equal(res.status, 200);
    saved = await res.json();
    assert.equal(saved.secrets.ASM_UI_DAEMON_TOKEN, false);
  });

  await check("pending lists what the scripts have not applied yet", async () => {
    mkdirSync(env.ASM_RUNTIME_DIR, { recursive: true });
    const applied = parseConfigFile(readFileSync(confPath, "utf8"));
    writeFileSync(join(env.ASM_RUNTIME_DIR, "startup.applied"), renderConfigFile({ ...applied, ASM_UI_HOST: "127.0.0.1" }));
    const got = await (await fetch(url)).json();
    assert.deepEqual(got.pending, ["ASM_UI_HOST"]);
  });
} finally {
  server.close();
  rmSync(tmp, { recursive: true, force: true });
}

if (failures) {
  console.log(`\n${failures} failure(s)`);
  process.exit(1);
}
console.log("\nall startup-config checks passed");
