import { Buffer } from "node:buffer";
import type { IncomingMessage, ServerResponse } from "node:http";
import type { Socket } from "node:net";
import { createProxyServer, type ServerOptions } from "http-proxy-3";
import type { Plugin } from "vite";

export const UI_GATEWAY_PREFIX = "/__asm/gateway/";

export interface GatewayRequest {
  /** Scheme + authority used for the proxy connection. */
  target: string;
  /** Target base-path plus the API path/query from the browser request. */
  path: string;
}

/**
 * Resolve one managed-UI gateway URL into an upstream request.
 *
 * The daemon URL occupies one base64url path segment. This survives reverse
 * proxies that normalize percent-encoded slashes, while keeping the target out
 * of a query parameter so the terminal WebSocket can retain its own
 * access-token query unchanged. Only HTTP(S) targets without credentials or a
 * base query are accepted; the gateway is an API transport, not a general URL
 * fetcher.
 */
export function parseGatewayRequest(rawUrl: string | undefined): GatewayRequest | null {
  if (!rawUrl?.startsWith(UI_GATEWAY_PREFIX)) return null;

  const queryAt = rawUrl.indexOf("?");
  const pathname = queryAt === -1 ? rawUrl : rawUrl.slice(0, queryAt);
  const query = queryAt === -1 ? "" : rawUrl.slice(queryAt);
  const remainder = pathname.slice(UI_GATEWAY_PREFIX.length);
  const slashAt = remainder.indexOf("/");
  const encodedTarget = slashAt === -1 ? remainder : remainder.slice(0, slashAt);
  const requestPath = slashAt === -1 ? "/" : remainder.slice(slashAt);
  if (!encodedTarget) throw new Error("missing daemon URL");
  if (!/^[A-Za-z0-9_-]+$/.test(encodedTarget)) throw new Error("invalid daemon URL");

  let upstream: URL;
  try {
    const decoded = Buffer.from(encodedTarget, "base64url").toString("utf8");
    // Reject non-canonical/truncated base64 instead of letting Node's tolerant
    // decoder silently turn it into some other target.
    if (Buffer.from(decoded, "utf8").toString("base64url") !== encodedTarget) {
      throw new Error();
    }
    upstream = new URL(decoded);
  } catch {
    throw new Error("invalid daemon URL");
  }
  if (!["http:", "https:"].includes(upstream.protocol)) {
    throw new Error("daemon URL must use http or https");
  }
  if (upstream.username || upstream.password) {
    throw new Error("daemon URL must not contain credentials");
  }
  if (upstream.search || upstream.hash) {
    throw new Error("daemon URL must not contain a query or fragment");
  }

  const basePath = upstream.pathname.replace(/\/$/, "");
  // Direct daemon URLs have no path. The only supported prefix is the relay's
  // own `/n/<node_id>` route, which connectionStore constructs internally.
  // Together with the endpoint check below, this keeps the feature scoped to
  // ASM transport instead of exposing a general-purpose forward proxy.
  if (basePath && !/^\/n\/[^/]+$/.test(basePath)) {
    throw new Error("unsupported daemon URL path");
  }
  if (
    requestPath !== "/health" &&
    requestPath !== "/nodes" &&
    !requestPath.startsWith("/api/")
  ) {
    throw new Error("unsupported daemon endpoint");
  }
  return {
    target: upstream.origin,
    path: `${basePath}${requestPath}${query}`,
  };
}

/**
 * Proxy options that carry one gateway request upstream.
 *
 * The upstream path rides in `target` (with `ignorePath`) instead of being
 * written back onto `req.url`. Vite's own `/api` proxy keeps an `upgrade`
 * listener on this same HTTP server and every listener is handed the same
 * request object, so rewriting `req.url` to `/api/...` here made that listener
 * match as well and proxy one browser WebSocket to the daemon a second time.
 * The duplicate connection's `101` handshake then lands mid-stream on the
 * browser's socket, which drops it as a protocol error before the attach
 * snapshot arrives — leaving the terminal reconnecting forever behind
 * "Loading terminal...".
 */
function upstreamOptions(route: GatewayRequest): ServerOptions {
  return { target: `${route.target}${route.path}`, ignorePath: true, changeOrigin: true };
}

function writeGatewayError(res: ServerResponse | Socket, message: string) {
  if ("writeHead" in res) {
    if (res.headersSent || res.writableEnded) return;
    res
      .writeHead(502, { "content-type": "application/json" })
      .end(JSON.stringify({ error: message }));
  } else {
    res.destroy();
  }
}

/**
 * Let a managed Vite UI act as the network hop for daemons the UI host can
 * reach but the browser cannot (the common case is a phone reaching Vite over
 * Tailscale while the daemon lives at a private LAN address).
 */
export function uiGatewayPlugin(): Plugin {
  return {
    name: "asm-ui-gateway",
    configureServer(server) {
      const proxy = createProxyServer({ ws: true });
      proxy.on("error", (_err, _req, res) => {
        if (res) writeGatewayError(res, "connected node cannot reach that daemon");
      });

      server.middlewares.use((req, res, next) => {
        let route: GatewayRequest | null;
        try {
          route = parseGatewayRequest(req.url);
        } catch (err) {
          res
            .writeHead(400, { "content-type": "application/json" })
            .end(JSON.stringify({ error: (err as Error).message }));
          return;
        }
        if (!route) {
          next();
          return;
        }
        proxy.web(req, res, upstreamOptions(route));
      });

      const onUpgrade = (req: IncomingMessage, socket: Socket, head: Buffer) => {
        let route: GatewayRequest | null;
        try {
          route = parseGatewayRequest(req.url);
        } catch {
          socket.destroy();
          return;
        }
        if (!route) return;
        proxy.ws(req, socket, head, upstreamOptions(route));
      };
      server.httpServer?.prependListener("upgrade", onUpgrade);
      server.httpServer?.once("close", () => {
        server.httpServer?.removeListener("upgrade", onUpgrade);
        proxy.close();
      });
    },
  };
}
