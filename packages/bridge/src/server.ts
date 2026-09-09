import { createServer, type IncomingMessage, type Server, type ServerResponse } from "node:http";
import { resolve } from "node:path";
import {
  classifyHealth,
  delayTest,
  getConnections,
  getDefaultSupervisor,
  getProxies,
  loadHealthRules,
  loadJsonConfig,
  mergeFromFiles,
  validateConfigText,
  type Mode,
} from "@sing/controller";
import type { HealthInput } from "@sing/controller";

export const DEFAULT_BRIDGE_PORT = 8787;
export const DEFAULT_BRIDGE_HOST = "127.0.0.1";

type Json = Record<string, unknown> | unknown[] | string | number | boolean | null;

export interface BridgeServerOptions {
  host?: string;
  port?: number;
}

function sendJson(res: ServerResponse, status: number, body: Json): void {
  const text = JSON.stringify(body);
  res.writeHead(status, {
    "Content-Type": "application/json; charset=utf-8",
    "Access-Control-Allow-Origin": "*",
    "Access-Control-Allow-Methods": "GET,POST,OPTIONS",
    "Access-Control-Allow-Headers": "Content-Type",
    "Content-Length": Buffer.byteLength(text),
  });
  res.end(text);
}

function readBody(req: IncomingMessage): Promise<string> {
  return new Promise((resolvePromise, reject) => {
    const chunks: Buffer[] = [];
    req.on("data", (c) => chunks.push(Buffer.isBuffer(c) ? c : Buffer.from(c)));
    req.on("end", () => resolvePromise(Buffer.concat(chunks).toString("utf8")));
    req.on("error", reject);
  });
}

async function parseJsonBody(req: IncomingMessage): Promise<Record<string, unknown>> {
  const raw = await readBody(req);
  if (!raw.trim()) return {};
  const parsed = JSON.parse(raw) as unknown;
  if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
    throw new Error("JSON body must be an object");
  }
  return parsed as Record<string, unknown>;
}

function parseMode(raw: unknown): Mode {
  if (raw === "office" || raw === "abroad" || raw === "manual") return raw;
  throw new Error("mode must be office|abroad|manual");
}

export function createBridgeServer(options: BridgeServerOptions = {}): Server {
  const supervisor = getDefaultSupervisor();

  return createServer(async (req, res) => {
    try {
      const method = req.method ?? "GET";
      const url = new URL(req.url ?? "/", `http://${req.headers.host ?? "127.0.0.1"}`);
      const path = url.pathname;

      if (method === "OPTIONS") {
        res.writeHead(204, {
          "Access-Control-Allow-Origin": "*",
          "Access-Control-Allow-Methods": "GET,POST,OPTIONS",
          "Access-Control-Allow-Headers": "Content-Type",
        });
        res.end();
        return;
      }

      if (method === "GET" && path === "/health") {
        sendJson(res, 200, { ok: true, service: "sing-bridge", version: "0.1.0" });
        return;
      }

      if (method === "GET" && path === "/core/status") {
        sendJson(res, 200, { ok: true, status: supervisor.status() });
        return;
      }

      if (method === "POST" && path === "/core/start") {
        const body = await parseJsonBody(req);
        const configPath = body.configPath;
        if (typeof configPath !== "string" || !configPath.trim()) {
          sendJson(res, 400, { ok: false, error: "configPath is required" });
          return;
        }
        const result = supervisor.start(resolve(configPath));
        sendJson(res, 200, { ...result, status: supervisor.status() });
        return;
      }

      if (method === "POST" && path === "/core/stop") {
        const result = supervisor.stop();
        sendJson(res, 200, { ...result, status: supervisor.status() });
        return;
      }

      if (method === "GET" && path === "/clash/proxies") {
        const baseUrl = url.searchParams.get("base") ?? undefined;
        const result = await getProxies({ baseUrl: baseUrl || undefined });
        sendJson(res, result.ok ? 200 : 502, result);
        return;
      }

      if (method === "GET" && path === "/clash/connections") {
        const baseUrl = url.searchParams.get("base") ?? undefined;
        const result = await getConnections({ baseUrl: baseUrl || undefined });
        sendJson(res, result.ok ? 200 : 502, result);
        return;
      }

      if (method === "POST" && path === "/clash/delay") {
        const body = await parseJsonBody(req);
        const name = body.name;
        if (typeof name !== "string" || !name.trim()) {
          sendJson(res, 400, { ok: false, error: "name is required", code: "INVALID" });
          return;
        }
        const baseUrl = typeof body.baseUrl === "string" ? body.baseUrl : undefined;
        const result = await delayTest(name, { baseUrl });
        sendJson(res, result.ok ? 200 : 502, result);
        return;
      }

      if (method === "POST" && path === "/validate") {
        const body = await parseJsonBody(req);
        const text =
          typeof body.text === "string"
            ? body.text
            : body.config != null
              ? JSON.stringify(body.config)
              : "";
        if (!text.trim()) {
          sendJson(res, 400, { ok: false, error: "text or config required" });
          return;
        }
        sendJson(res, 200, validateConfigText(text));
        return;
      }

      if (method === "POST" && path === "/merge") {
        const body = await parseJsonBody(req);
        const mode = parseMode(body.mode);
        const baselinePath = body.baselinePath;
        if (typeof baselinePath !== "string" || !baselinePath.trim()) {
          sendJson(res, 400, { ok: false, error: "baselinePath is required" });
          return;
        }
        const result = mergeFromFiles(resolve(baselinePath), mode);
        sendJson(res, 200, { ok: true, meta: result.meta, config: result.config });
        return;
      }

      if (method === "POST" && path === "/health/classify") {
        const body = await parseJsonBody(req);
        const rulesPath = typeof body.rulesPath === "string" ? body.rulesPath : null;
        const rules = loadHealthRules(rulesPath ? resolve(rulesPath) : null);
        let input: HealthInput;
        if (typeof body.fixturePath === "string" && body.fixturePath.trim()) {
          input = loadJsonConfig(resolve(body.fixturePath)) as HealthInput;
        } else if (body.input && typeof body.input === "object") {
          input = body.input as HealthInput;
        } else {
          sendJson(res, 400, { ok: false, error: "fixturePath or input required" });
          return;
        }
        const result = classifyHealth(input, rules);
        sendJson(res, 200, result);
        return;
      }

      sendJson(res, 404, { ok: false, error: `not found: ${method} ${path}` });
    } catch (err: unknown) {
      const message = err instanceof Error ? err.message : String(err);
      sendJson(res, 500, { ok: false, error: message });
    }
  });
}

export function listenBridge(
  options: BridgeServerOptions = {},
): Promise<{ server: Server; host: string; port: number; url: string }> {
  const host = options.host ?? DEFAULT_BRIDGE_HOST;
  const port = options.port ?? DEFAULT_BRIDGE_PORT;
  const server = createBridgeServer(options);
  return new Promise((resolvePromise, reject) => {
    server.once("error", reject);
    server.listen(port, host, () => {
      resolvePromise({
        server,
        host,
        port,
        url: `http://${host}:${port}`,
      });
    });
  });
}
