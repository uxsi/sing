import { describe, expect, it, vi } from "vitest";
import {
  delayTest,
  getConfigs,
  getConnections,
  getProxies,
} from "../src/clashApi.js";

function jsonOk(data: unknown, status = 200): Response {
  return new Response(JSON.stringify(data), {
    status,
    headers: { "content-type": "application/json" },
  });
}

describe("clashApi", () => {
  it("getProxies returns data on success", async () => {
    const fetchImpl = vi.fn(async (url: RequestInfo | URL) => {
      expect(String(url)).toBe("http://127.0.0.1:9090/proxies");
      return jsonOk({ proxies: { DIRECT: { type: "Direct" } } });
    });
    const result = await getProxies({ fetchImpl: fetchImpl as unknown as typeof fetch });
    expect(result.ok).toBe(true);
    if (!result.ok) return;
    expect((result.data as { proxies: unknown }).proxies).toBeTruthy();
  });

  it("getConnections soft-fails when core is down", async () => {
    const fetchImpl = vi.fn(async () => {
      throw new TypeError("fetch failed");
    });
    const result = await getConnections({
      fetchImpl: fetchImpl as unknown as typeof fetch,
    });
    expect(result.ok).toBe(false);
    if (result.ok) return;
    expect(result.code).toBe("CORE_UNAVAILABLE");
    expect(result.error).toMatch(/unavailable|not running/i);
  });

  it("getConfigs maps HTTP errors", async () => {
    const fetchImpl = vi.fn(async () => jsonOk({ message: "nope" }, 401));
    const result = await getConfigs({ fetchImpl: fetchImpl as unknown as typeof fetch });
    expect(result.ok).toBe(false);
    if (result.ok) return;
    expect(result.code).toBe("HTTP_ERROR");
    expect(result.status).toBe(401);
  });

  it("delayTest hits /proxies/:name/delay", async () => {
    const fetchImpl = vi.fn(async (url: RequestInfo | URL) => {
      const s = String(url);
      expect(s).toContain("/proxies/node-1/delay");
      expect(s).toContain("timeout=");
      return jsonOk({ delay: 42 });
    });
    const result = await delayTest("node-1", {
      fetchImpl: fetchImpl as unknown as typeof fetch,
    });
    expect(result.ok).toBe(true);
    if (!result.ok) return;
    expect(result.data.name).toBe("node-1");
    expect(result.data.delay).toBe(42);
  });

  it("delayTest requires name", async () => {
    const result = await delayTest("", {});
    expect(result.ok).toBe(false);
    if (result.ok) return;
    expect(result.code).toBe("INVALID");
  });

  it("timeout returns TIMEOUT code", async () => {
    const fetchImpl = vi.fn(async (_u: RequestInfo | URL, init?: RequestInit) => {
      return new Promise<Response>((_resolve, reject) => {
        init?.signal?.addEventListener("abort", () => {
          const err = new Error("Aborted");
          err.name = "AbortError";
          reject(err);
        });
      });
    });
    const result = await getProxies({
      timeoutMs: 15,
      fetchImpl: fetchImpl as unknown as typeof fetch,
    });
    expect(result.ok).toBe(false);
    if (result.ok) return;
    expect(result.code).toBe("TIMEOUT");
  });
});
