export const DEFAULT_CLASH_API_BASE = "http://127.0.0.1:9090";

export interface ClashApiOptions {
  /** Base URL for experimental.clash_api (default http://127.0.0.1:9090). */
  baseUrl?: string;
  /** Request timeout in ms (default 5000). */
  timeoutMs?: number;
  /** Optional secret for Authorization / ?token= */
  secret?: string;
  /** Injected fetch for tests. */
  fetchImpl?: typeof fetch;
}

export interface ClashApiError {
  ok: false;
  error: string;
  code: "CORE_UNAVAILABLE" | "HTTP_ERROR" | "TIMEOUT" | "INVALID" | "NETWORK";
  status?: number;
}

export interface ClashApiSuccess<T> {
  ok: true;
  data: T;
}

export type ClashApiResult<T> = ClashApiSuccess<T> | ClashApiError;

export type ClashConfigs = Record<string, unknown>;
export type ClashProxies = Record<string, unknown>;
export type ClashConnections = Record<string, unknown>;

export interface DelayTestData {
  name: string;
  delay?: number;
  raw: unknown;
}

function normalizeBase(baseUrl: string): string {
  return baseUrl.replace(/\/+$/, "");
}

function buildUrl(baseUrl: string, path: string): string {
  return normalizeBase(baseUrl) + path;
}

async function clashFetch<T>(
  path: string,
  options: ClashApiOptions = {},
  init: RequestInit = {},
): Promise<ClashApiResult<T>> {
  const baseUrl = options.baseUrl ?? DEFAULT_CLASH_API_BASE;
  const timeoutMs = options.timeoutMs ?? 5_000;
  const fetchImpl = options.fetchImpl ?? globalThis.fetch;

  if (typeof fetchImpl !== "function") {
    return { ok: false, error: "fetch is not available", code: "INVALID" };
  }

  const headers: Record<string, string> = {
    Accept: "application/json",
    ...(init.headers as Record<string, string> | undefined),
  };
  if (options.secret) {
    headers.Authorization = `Bearer ${options.secret}`;
  }

  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), timeoutMs);

  try {
    const url = buildUrl(baseUrl, path);
    const res = await fetchImpl(url, {
      ...init,
      headers,
      signal: controller.signal,
    });

    if (!res.ok) {
      const code = res.status >= 500 || res.status === 0 ? "CORE_UNAVAILABLE" : "HTTP_ERROR";
      return {
        ok: false,
        error: `clash_api HTTP ${res.status} ${res.statusText || ""}`.trim(),
        code,
        status: res.status,
      };
    }

    const data = (await res.json()) as T;
    return { ok: true, data };
  } catch (err: unknown) {
    if (err instanceof Error && err.name === "AbortError") {
      return {
        ok: false,
        error: `clash_api timed out after ${timeoutMs}ms (is the core running?)`,
        code: "TIMEOUT",
      };
    }
    const message = err instanceof Error ? err.message : String(err);
    const unavailable =
      /ECONNREFUSED|Failed to fetch|NetworkError|fetch failed|ENOTFOUND/i.test(message);
    return {
      ok: false,
      error: unavailable
        ? `clash_api unavailable at ${normalizeBase(baseUrl)} (core not running?): ${message}`
        : `clash_api request failed: ${message}`,
      code: unavailable ? "CORE_UNAVAILABLE" : "NETWORK",
    };
  } finally {
    clearTimeout(timer);
  }
}

export async function getConfigs(
  options: ClashApiOptions = {},
): Promise<ClashApiResult<ClashConfigs>> {
  return clashFetch<ClashConfigs>("/configs", options);
}

export async function getProxies(
  options: ClashApiOptions = {},
): Promise<ClashApiResult<ClashProxies>> {
  return clashFetch<ClashProxies>("/proxies", options);
}

export async function getConnections(
  options: ClashApiOptions = {},
): Promise<ClashApiResult<ClashConnections>> {
  return clashFetch<ClashConnections>("/connections", options);
}

/**
 * Delay-test a named proxy via GET /proxies/:name/delay?timeout=5000&url=...
 */
export async function delayTest(
  proxyName: string,
  options: ClashApiOptions & { testUrl?: string; delayTimeoutMs?: number } = {},
): Promise<ClashApiResult<DelayTestData>> {
  if (!proxyName || typeof proxyName !== "string") {
    return { ok: false, error: "proxy name is required", code: "INVALID" };
  }

  const delayTimeoutMs = options.delayTimeoutMs ?? 5_000;
  const testUrl = options.testUrl ?? "http://www.gstatic.com/generate_204";
  const encoded = encodeURIComponent(proxyName);
  const path = `/proxies/${encoded}/delay?timeout=${delayTimeoutMs}&url=${encodeURIComponent(testUrl)}`;

  const result = await clashFetch<{ delay?: number }>(path, {
    ...options,
    timeoutMs: options.timeoutMs ?? delayTimeoutMs + 2_000,
  });

  if (!result.ok) return result;
  return {
    ok: true,
    data: {
      name: proxyName,
      delay: typeof result.data.delay === "number" ? result.data.delay : undefined,
      raw: result.data,
    },
  };
}
