import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { createServer as createHttpServer } from "node:http";
import { createServer as createViteServer } from "vite";
import {
  parseGatewayRequest,
  UI_GATEWAY_PREFIX,
  uiGatewayPlugin,
} from "./vite.gateway.ts";

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

const route = (base, path) =>
  `${UI_GATEWAY_PREFIX}${Buffer.from(base).toString("base64url")}${path}`;

check("routes a LAN daemon through the UI host", () => {
  assert.deepEqual(parseGatewayRequest(route("http://192.168.1.23:4600", "/health")), {
    target: "http://192.168.1.23:4600",
    path: "/health",
  });
});

check("preserves relay prefixes and terminal WebSocket queries", () => {
  assert.deepEqual(
    parseGatewayRequest(
      route(
        "https://relay.example/n/node-7",
        "/api/sessions/s1/stream?access_token=device&relay_key=relay",
      ),
    ),
    {
      target: "https://relay.example",
      path: "/n/node-7/api/sessions/s1/stream?access_token=device&relay_key=relay",
    },
  );
});

check("ignores requests outside the gateway namespace", () => {
  assert.equal(parseGatewayRequest("/@vite/client"), null);
});

check("rejects non-HTTP targets and embedded credentials", () => {
  assert.throws(() => parseGatewayRequest(route("file:///etc/passwd", "/health")), /http or https/);
  assert.throws(
    () => parseGatewayRequest(route("http://user:secret@host:4600", "/health")),
    /credentials/,
  );
});

check("is limited to ASM daemon and relay routes", () => {
  assert.throws(
    () => parseGatewayRequest(route("http://host:4600/private", "/health")),
    /unsupported daemon URL path/,
  );
  assert.throws(
    () => parseGatewayRequest(route("http://host:4600", "/admin")),
    /unsupported daemon endpoint/,
  );
});

async function integration() {
  let seenHttp;
  let seenWs;
  const upstreamSockets = new Set();
  const upstream = createHttpServer((req, res) => {
    const chunks = [];
    req.on("data", (chunk) => chunks.push(chunk));
    req.on("end", () => {
      seenHttp = {
        method: req.method,
        url: req.url,
        authorization: req.headers.authorization,
        body: Buffer.concat(chunks).toString(),
      };
      res.writeHead(200, { "content-type": "application/json" });
      res.end(JSON.stringify({ node_id: "lan-node" }));
    });
  });
  upstream.on("upgrade", (req, socket) => {
    upstreamSockets.add(socket);
    socket.on("close", () => upstreamSockets.delete(socket));
    seenWs = req.url;
    const accept = createHash("sha1")
      .update(`${req.headers["sec-websocket-key"]}258EAFA5-E914-47DA-95CA-C5AB0DC85B11`)
      .digest("base64");
    socket.write(
      "HTTP/1.1 101 Switching Protocols\r\n" +
        "Upgrade: websocket\r\n" +
        "Connection: Upgrade\r\n" +
        `Sec-WebSocket-Accept: ${accept}\r\n\r\n`,
    );
    const message = Buffer.from("gateway-ws-ok");
    socket.write(Buffer.concat([Buffer.from([0x81, message.length]), message]));
  });
  await new Promise((resolve) => upstream.listen(0, "127.0.0.1", resolve));
  const upstreamPort = upstream.address().port;

  const vite = await createViteServer({
    configFile: false,
    logLevel: "silent",
    plugins: [uiGatewayPlugin()],
    server: { host: "127.0.0.1", port: 0 },
  });
  await vite.listen();
  const vitePort = vite.httpServer.address().port;
  const daemon = `http://127.0.0.1:${upstreamPort}/n/lan-node`;

  try {
    const response = await fetch(
      `http://127.0.0.1:${vitePort}${route(daemon, "/api/sessions?fresh=1")}`,
      {
        method: "POST",
        headers: { authorization: "Bearer device-token" },
        body: "request-body",
      },
    );
    assert.equal(response.status, 200);
    assert.deepEqual(await response.json(), { node_id: "lan-node" });
    assert.deepEqual(seenHttp, {
      method: "POST",
      url: "/n/lan-node/api/sessions?fresh=1",
      authorization: "Bearer device-token",
      body: "request-body",
    });

    const wsMessage = await new Promise((resolve, reject) => {
      const ws = new WebSocket(
        `ws://127.0.0.1:${vitePort}${route(daemon, "/api/sessions/s1/stream?access_token=tok")}`,
      );
      const timer = setTimeout(() => reject(new Error("gateway WebSocket timed out")), 3000);
      ws.onmessage = (event) => {
        clearTimeout(timer);
        resolve(event.data);
        ws.close();
      };
      ws.onerror = () => {
        clearTimeout(timer);
        reject(new Error("gateway WebSocket failed"));
      };
    });
    assert.equal(wsMessage, "gateway-ws-ok");
    assert.equal(seenWs, "/n/lan-node/api/sessions/s1/stream?access_token=tok");
  } finally {
    for (const socket of upstreamSockets) socket.destroy();
    await vite.close();
    await new Promise((resolve) => upstream.close(resolve));
  }
}

try {
  await integration();
  console.log("  ok  - proxies HTTP and terminal WebSockets from the UI host");
} catch (e) {
  failures++;
  console.log(`  FAIL- proxies HTTP and terminal WebSockets from the UI host\n        ${e.message}`);
}

if (failures) {
  console.log(`\n${failures} test(s) failed`);
  process.exit(1);
}
