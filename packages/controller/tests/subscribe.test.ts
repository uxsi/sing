import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, describe, expect, it, vi } from "vitest";
import { fetchSubscription, fetchSubscriptionBody } from "../src/subscribe.js";

function jsonResponse(body: string, init: { status?: number; contentType?: string } = {}) {
  const { status = 200, contentType = "application/json" } = init;
  return new Response(body, {
    status,
    headers: contentType ? { "content-type": contentType } : undefined,
  });
}

describe("fetchSubscription", () => {
  const dirs: string[] = [];

  afterEach(() => {
    for (const d of dirs.splice(0)) {
      rmSync(d, { recursive: true, force: true });
    }
    vi.unstubAllGlobals();
  });

  function tmpOut(): string {
    const dir = mkdtempSync(join(tmpdir(), "sing-sub-"));
    dirs.push(dir);
    return join(dir, "sub.json");
  }

  it("saves body on success and respects User-Agent", async () => {
    const outPath = tmpOut();
    const fetchImpl = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      expect(String(input)).toBe("https://example.com/sub");
      expect((init?.headers as Record<string, string>)["User-Agent"]).toBe("sing-test/1");
      return jsonResponse('{"outbounds":[]}');
    });

    const result = await fetchSubscription({
      url: "https://example.com/sub",
      outPath,
      userAgent: "sing-test/1",
      fetchImpl: fetchImpl as unknown as typeof fetch,
    });

    expect(result.ok).toBe(true);
    if (!result.ok) return;
    expect(result.bytesWritten).toBeGreaterThan(0);
    expect(readFileSync(outPath, "utf8")).toBe('{"outbounds":[]}');
    expect(result.contentType).toContain("json");
  });

  it("rejects non-http URLs", async () => {
    const result = await fetchSubscription({
      url: "file:///etc/passwd",
      outPath: tmpOut(),
      fetchImpl: vi.fn() as unknown as typeof fetch,
    });
    expect(result.ok).toBe(false);
    if (result.ok) return;
    expect(result.error).toMatch(/http\/https/i);
  });

  it("returns structured error on HTTP failure", async () => {
    const fetchImpl = vi.fn(async () => jsonResponse("nope", { status: 404 }));
    const result = await fetchSubscription({
      url: "https://example.com/missing",
      outPath: tmpOut(),
      fetchImpl: fetchImpl as unknown as typeof fetch,
    });
    expect(result.ok).toBe(false);
    if (result.ok) return;
    expect(result.status).toBe(404);
    expect(result.error).toMatch(/HTTP 404/);
  });

  it("rejects unsupported content-type", async () => {
    const fetchImpl = vi.fn(
      async () => jsonResponse("\x00\x01", { contentType: "image/png" }),
    );
    const result = await fetchSubscription({
      url: "https://example.com/bin",
      outPath: tmpOut(),
      fetchImpl: fetchImpl as unknown as typeof fetch,
    });
    expect(result.ok).toBe(false);
    if (result.ok) return;
    expect(result.error).toMatch(/Content-Type/i);
  });

  it("times out via AbortError", async () => {
    const fetchImpl = vi.fn(async (_u: RequestInfo | URL, init?: RequestInit) => {
      return new Promise<Response>((_resolve, reject) => {
        init?.signal?.addEventListener("abort", () => {
          const err = new Error("Aborted");
          err.name = "AbortError";
          reject(err);
        });
      });
    });
    const result = await fetchSubscription({
      url: "https://example.com/slow",
      outPath: tmpOut(),
      timeoutMs: 20,
      fetchImpl: fetchImpl as unknown as typeof fetch,
    });
    expect(result.ok).toBe(false);
    if (result.ok) return;
    expect(result.error).toMatch(/timed out/i);
  });
});

describe("fetchSubscriptionBody", () => {
  it("returns body without writing", async () => {
    const fetchImpl = vi.fn(async () => jsonResponse("hello"));
    const result = await fetchSubscriptionBody({
      url: "https://example.com/x",
      fetchImpl: fetchImpl as unknown as typeof fetch,
    });
    expect(result.ok).toBe(true);
    if (!result.ok) return;
    expect(result.body).toBe("hello");
  });
});
