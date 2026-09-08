export interface SubscribeFailure {
  ok: false;
  error: string;
  status?: number;
  contentType?: string;
}

const DEFAULT_TIMEOUT_MS = 15_000;

function isHttpUrl(raw: string): boolean {
  try {
    const u = new URL(raw);
    return u.protocol === "http:" || u.protocol === "https:";
  } catch {
    return false;
  }
}

function contentTypeLooksText(contentType: string | null): boolean {
  if (!contentType) return true;
  const ct = contentType.toLowerCase().split(";")[0]?.trim() ?? "";
  if (!ct) return true;
  return (
    ct.startsWith("text/") ||
    ct.includes("json") ||
    ct.includes("yaml") ||
    ct.includes("yml") ||
    ct === "application/octet-stream"
  );
}

/**
 * Browser-friendly fetch that returns the body without writing to disk.
 */
export async function fetchSubscriptionBody(options: {
  url: string;
  userAgent?: string;
  timeoutMs?: number;
  fetchImpl?: typeof fetch;
}): Promise<{ ok: true; body: string; contentType?: string } | SubscribeFailure> {
  const { url, userAgent, timeoutMs = DEFAULT_TIMEOUT_MS } = options;
  const fetchImpl = options.fetchImpl ?? globalThis.fetch;

  if (!url || typeof url !== "string") {
    return { ok: false, error: "URL is required" };
  }
  if (!isHttpUrl(url)) {
    return { ok: false, error: `Invalid URL (http/https required): ${url}` };
  }
  if (typeof fetchImpl !== "function") {
    return { ok: false, error: "fetch is not available in this runtime" };
  }

  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), timeoutMs);

  try {
    const headers: Record<string, string> = {
      Accept: "text/plain, application/json, application/octet-stream, */*",
    };
    if (userAgent) headers["User-Agent"] = userAgent;

    const res = await fetchImpl(url, {
      method: "GET",
      headers,
      signal: controller.signal,
      redirect: "follow",
    });

    const contentType = res.headers.get("content-type") ?? undefined;

    if (!res.ok) {
      return {
        ok: false,
        error: `HTTP ${res.status} ${res.statusText || "error"} fetching subscription`,
        status: res.status,
        contentType,
      };
    }

    if (!contentTypeLooksText(contentType ?? null)) {
      return {
        ok: false,
        error: `Unsupported Content-Type for subscription: ${contentType}`,
        status: res.status,
        contentType,
      };
    }

    const body = await res.text();
    if (!body.trim()) {
      return {
        ok: false,
        error: "Subscription response body is empty",
        status: res.status,
        contentType,
      };
    }

    return { ok: true, body, contentType };
  } catch (err: unknown) {
    if (err instanceof Error && err.name === "AbortError") {
      return { ok: false, error: `Subscription fetch timed out after ${timeoutMs}ms` };
    }
    const message = err instanceof Error ? err.message : String(err);
    return { ok: false, error: `Subscription fetch failed: ${message}` };
  } finally {
    clearTimeout(timer);
  }
}
