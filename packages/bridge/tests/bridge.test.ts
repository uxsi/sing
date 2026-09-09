import { afterAll, beforeAll, describe, expect, it } from "vitest";
import type { Server } from "node:http";
import { listenBridge } from "../src/server.js";
import { resetDefaultSupervisor } from "@sing/controller";

describe("sing bridge", () => {
  let server: Server;
  let base: string;

  beforeAll(async () => {
    resetDefaultSupervisor();
    const listened = await listenBridge({ host: "127.0.0.1", port: 0 });
    server = listened.server;
    const addr = server.address();
    if (!addr || typeof addr === "string") throw new Error("expected TCP address");
    base = `http://127.0.0.1:${addr.port}`;
  });

  afterAll(async () => {
    resetDefaultSupervisor();
    await new Promise<void>((resolve, reject) => {
      server.close((err) => (err ? reject(err) : resolve()));
    });
  });

  it("GET /health", async () => {
    const res = await fetch(`${base}/health`);
    expect(res.status).toBe(200);
    const body = (await res.json()) as { ok: boolean; service: string };
    expect(body.ok).toBe(true);
    expect(body.service).toBe("sing-bridge");
  });

  it("GET /core/status soft-fails path when stopped", async () => {
    const res = await fetch(`${base}/core/status`);
    expect(res.status).toBe(200);
    const body = (await res.json()) as { ok: boolean; status: { state: string } };
    expect(body.ok).toBe(true);
    expect(body.status.state).toBe("stopped");
  });

  it("POST /core/start soft-fails when binary missing", async () => {
    const res = await fetch(`${base}/core/start", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ configPath: "configs/examples/baseline.json" }),
    });
    expect(res.status).toBe(200);
    const body = (await res.json()) as {
      ok: boolean;
      softFail?: boolean;
      status: { state: string };
    };
    // In CI/cloud sing-box is usually missing → softFail; if present, ok may be true.
    if (!body.ok) {
      expect(body.softFail).toBe(true);
      expect(body.status.state).toBe("missing_binary");
    }
  });

  it("POST /validate", async () => {
    const res = await fetch(`${base}/validate", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ text: '{"outbounds":[{"type":"direct","tag":"d"}]}' }),
    });
    expect(res.status).toBe(200);
    const body = (await res.json()) as { ok: boolean };
    expect(body.ok).toBe(true);
  });
});
