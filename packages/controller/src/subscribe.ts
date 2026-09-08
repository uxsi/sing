import { writeFileSync } from "node:fs";
import { resolve } from "node:path";
import {
  fetchSubscriptionBody,
  type SubscribeFailure,
} from "./subscribeCore.js";

export type { SubscribeFailure } from "./subscribeCore.js";
export { fetchSubscriptionBody } from "./subscribeCore.js";

export interface SubscribeOptions {
  /** Subscription URL (http or https). */
  url: string;
  /** Filesystem path to write the response body. */
  outPath: string;
  /** Optional User-Agent header. */
  userAgent?: string;
  /** Request timeout in milliseconds (default 15000). */
  timeoutMs?: number;
  /** Injected fetch for tests (defaults to global fetch). */
  fetchImpl?: typeof fetch;
}

export interface SubscribeSuccess {
  ok: true;
  bytesWritten: number;
  contentType?: string;
  path: string;
}

export type SubscribeResult = SubscribeSuccess | SubscribeFailure;

/**
 * Fetch a subscription URL and save the response body to `outPath`.
 * Soft-fails with a structured result (does not throw for HTTP/network errors).
 */
export async function fetchSubscription(
  options: SubscribeOptions,
): Promise<SubscribeResult> {
  const { outPath } = options;
  if (!outPath) {
    return { ok: false, error: "outPath is required" };
  }

  const fetched = await fetchSubscriptionBody(options);
  if (!fetched.ok) return fetched;

  try {
    const path = resolve(outPath);
    writeFileSync(path, fetched.body, "utf8");
    return {
      ok: true,
      bytesWritten: Buffer.byteLength(fetched.body, "utf8"),
      contentType: fetched.contentType,
      path,
    };
  } catch (err: unknown) {
    const message = err instanceof Error ? err.message : String(err);
    return { ok: false, error: `Failed to write subscription file: ${message}` };
  }
}
