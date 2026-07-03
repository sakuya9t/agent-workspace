import assert from "node:assert/strict";
import {
  createDaemonAwareLogger,
  isDaemonDownProxyError,
  respondDaemonDown,
} from "./vite.proxy-log.ts";

// A minimal stand-in for Vite's Logger that records calls.
function fakeLogger() {
  const warns = [];
  const errors = [];
  return {
    hasWarned: false,
    info() {},
    warn(msg) { warns.push(msg); },
    warnOnce(msg) { warns.push(msg); },
    error(msg) { errors.push(msg); },
    clearScreen() {},
    hasErrorLogged() { return false; },
    _warns: warns,
    _errors: errors,
  };
}

// The exact string shape Vite passes to logger.error for a down daemon.
const PROXY_ERR =
  "http proxy error: /health\nError: connect ECONNREFUSED 127.0.0.1:4600\n    at TCPConnectWrap.afterConnect [as oncomplete] (node:net:1705:16)";
const WS_ERR =
  "ws proxy error:\nError: connect ECONNREFUSED 127.0.0.1:4600\n    at TCPConnectWrap.afterConnect";
const REAL_ERR = "Internal server error: something legitimately broke";

let failures = 0;
function check(name, fn) {
  try {
    fn();
    console.log(`  ok  - ${name}`);
  } catch (e) {
    failures++;
    console.log(`  FAIL- ${name}\n        ${e.message}`);
  }
}

check("classifier matches http + ws proxy ECONNREFUSED", () => {
  assert.equal(isDaemonDownProxyError(PROXY_ERR), true);
  assert.equal(isDaemonDownProxyError(WS_ERR), true);
});

check("classifier ignores unrelated errors and non-strings", () => {
  assert.equal(isDaemonDownProxyError(REAL_ERR), false);
  assert.equal(isDaemonDownProxyError("proxy error: 502 bad gateway"), false); // no down-code
  assert.equal(isDaemonDownProxyError(new Error("x")), false);
  assert.equal(isDaemonDownProxyError(undefined), false);
});

check("daemon-down proxy errors are suppressed (not logged as error)", () => {
  const base = fakeLogger();
  const log = createDaemonAwareLogger(base, "http://127.0.0.1:4600", () => 0);
  log.error(PROXY_ERR);
  assert.equal(base._errors.length, 0, "should not forward to base.error");
});

check("emits exactly one throttled hint within a 3s window", () => {
  let t = 1000;
  const base = fakeLogger();
  const log = createDaemonAwareLogger(base, "http://127.0.0.1:4600", () => t);
  log.error(PROXY_ERR); // t=1000 -> warns (first ever)
  t = 2000; log.error(PROXY_ERR); // within 3s of 1000 -> throttled
  t = 3500; log.error(WS_ERR);   // within 3s of 1000 -> throttled
  t = 4001; log.error(PROXY_ERR); // >3s after 1000 -> warns again
  assert.equal(base._warns.length, 2, `expected 2 hints, got ${base._warns.length}`);
  assert.match(base._warns[0], /daemon not reachable at http:\/\/127\.0\.0\.1:4600/);
  assert.match(base._warns[0], /cargo run -p asm-daemon/);
});

check("real errors still pass through to base.error", () => {
  const base = fakeLogger();
  const log = createDaemonAwareLogger(base, "http://127.0.0.1:4600", () => 0);
  log.error(REAL_ERR);
  assert.deepEqual(base._errors, [REAL_ERR]);
  assert.equal(base._warns.length, 0);
});

// --- respondDaemonDown: the proxy `configure` hook ---

// Minimal stand-ins for node-http-proxy's Server and http.ServerResponse.
function fakeProxy() {
  const listeners = {};
  return {
    on(event, cb) { listeners[event] = cb; },
    _fire(event, ...args) { listeners[event]?.(...args); },
  };
}
function fakeRes() {
  return {
    headersSent: false,
    writableEnded: false,
    _status: null,
    _headers: null,
    _body: null,
    writeHead(status, headers) {
      this.headersSent = true;
      this._status = status;
      this._headers = headers;
      return this;
    },
    end(body) {
      this.writableEnded = true;
      this._body = body;
    },
  };
}

check("connection error answers 502 with a JSON 'cannot connect' body", () => {
  const proxy = fakeProxy();
  respondDaemonDown("http://127.0.0.1:4600")(proxy, {});
  const res = fakeRes();
  proxy._fire("error", new Error("connect ECONNREFUSED 127.0.0.1:4600"), {}, res);
  assert.equal(res._status, 502);
  assert.equal(res._headers["content-type"], "application/json");
  const body = JSON.parse(res._body);
  assert.match(body.error, /cannot connect/);
  assert.match(body.error, /127\.0\.0\.1:4600/);
});

check("leaves already-answered responses and ws sockets alone", () => {
  const proxy = fakeProxy();
  respondDaemonDown("http://127.0.0.1:4600")(proxy, {});

  const sent = fakeRes();
  sent.headersSent = true;
  proxy._fire("error", new Error("x"), {}, sent);
  assert.equal(sent._status, null, "must not write over an in-flight response");

  const socket = { destroyed: false, end() {} }; // ws upgrade: no writeHead
  proxy._fire("error", new Error("x"), {}, socket); // must not throw
});

if (failures) {
  console.log(`\n${failures} test(s) failed`);
  process.exit(1);
}
console.log("\nall proxy-log tests passed");
