/**
 * Unified desktop core bridge:
 * 1) Tauri invoke when window.__TAURI__ / __TAURI_INTERNALS__ is present
 * 2) else Node HTTP bridge at http://127.0.0.1:8787 (packages/bridge)
 * 3) else browser stub with clear messaging
 */

export type BackendKind = "tauri" | "bridge" | "stub";

export const DEFAULT_BRIDGE_URL = "http://127.0.0.1:8787";
export const DEFAULT_CLASH_API_BASE = "http://127.0.0.1:9090";

export interface CoreStatus {
  state: string;
  binary?: string | null;
  configPath?: string | null;
  pid?: number | null;
  lastError?: string | null;
  softFail?: boolean;
  restarts?: number;
  maxRestarts?: number;
}

type Json = Record<string, unknown>;
function tauriInvokeAvailable(): boolean {
  const w = window as unknown as {
    __TAURI_INTERNALS__?: { invoke?: unknown };
    __TAURI__?: { core?: { invoke?: unknown }; invoke?: unknown };
  };
  return Boolean(
    w.__TAURI_INTERNALS__?.invoke ||
      w.__TAURI__?.core?.invoke ||
      w.__TAURI__?.invoke,
  );
}

async function tauriInvoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const w = window as unknown as {
    __TAURI_INTERNALS__?: { invoke: (c: string, a?: Record<string, unknown>) => Promise<T> };
    __TAURI__?: {
      core?: { invoke: (c: string, a?: Record<string, unknown>) => Promise<T> };
      invoke?: (c: string, a?: Record<string, unknown>) => Promise<T>;
    };
  };
  if (w.__TAURI_INTERNALS__?.invoke) return w.__TAURI_INTERNALS__.invoke(cmd, args);
  if (w.__TAURI__?.core?.invoke) return w.__TAURI__.core.invoke(cmd, args);
  if (w.__TAURI__?.invoke) return w.__TAURI__.invoke(cmd, args);
  throw new Error("Tauri invoke not available");
}

function bridgeBase(): string {
  const w = window as unknown as { __SING_BRIDGE_URL__?: string };
  return (w.__SING_BRIDGE_URL__ || DEFAULT_BRIDGE_URL).replace(/\/+$/, "");
}

async function bridgeFetch<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(`${bridgeBase()}${path}`, {
    ...init,
    headers: {
      Accept: "application/json",
      ...(init?.headers ?? {}),
    },
  });
  return (await res.json()) as T;
}

let cachedKind: BackendKind | null = null;

export async function detectBackend(force = false): Promise<BackendKind> {
  if (!force && cachedKind) return cachedKind;
  if (tauriInvokeAvailable()) {
    cachedKind = "tauri";
    return cachedKind;
  }
  try {
    const ctrl = new AbortController();
    const timer = setTimeout(() => ctrl.abort(), 600);
    const res = await fetch(`${bridgeBase()}/health`, { signal: ctrl.signal });
    clearTimeout(timer);
    if (res.ok) {
      cachedKind = "bridge";
      return cachedKind;
    }
  } catch {
    /* fall through */
  }
  cachedKind = "stub";
  return cachedKind;
}

export function backendLabel(kind: BackendKind): string {
  if (kind === "tauri") return "Tauri";
  if (kind === "bridge") return "Node bridge :8787";
  return "browser stub (need Tauri or bridge)";
}

export async function coreStatus(): Promise<{ ok: boolean; status: CoreStatus; error?: string }> {
  const kind = await detectBackend();
  if (kind === "tauri") {
    return tauriInvoke("core_status");
  }
  if (kind === "bridge") {
    return bridgeFetch("/core/status");
  }
  return {
    ok: false,
    error: "Need Tauri or local bridge to query core status",
    status: { state: "stopped", softFail: false },
  };
}

export async function coreStart(
  configPath: string,
): Promise<{ ok: boolean; softFail?: boolean; error?: string; status?: CoreStatus }> {
  const kind = await detectBackend();
  if (kind === "tauri") {
    return tauriInvoke("core_start", { configPath });
  }
  if (kind === "bridge") {
    return bridgeFetch("/core/start", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ configPath }),
    });
  }
  return {
    ok: false,
    softFail: true,
    error: "Need Tauri or local bridge to start core (browser cannot spawn sing-box)",
  };
}

export async function coreStop(): Promise<{ ok: boolean; error?: string; status?: CoreStatus }> {
  const kind = await detectBackend();
  if (kind === "tauri") {
    return tauriInvoke("core_stop");
  }
  if (kind === "bridge") {
    return bridgeFetch("/core/stop", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: "{}",
    });
  }
  return { ok: false, error: "Need Tauri or local bridge to stop core" };
}

export async function clashGetProxies(
  baseUrl = DEFAULT_CLASH_API_BASE,
): Promise<{ ok: boolean; data?: Json; error?: string }> {
  const kind = await detectBackend();
  if (kind === "tauri") {
    return tauriInvoke("clash_get_proxies", { baseUrl });
  }
  if (kind === "bridge") {
    const q = encodeURIComponent(baseUrl);
    return bridgeFetch(`/clash/proxies?base=${q}`);
  }
  return {
    ok: false,
    error:
      "Browser cannot reliably call clash_api (CORS). Start Tauri or local bridge.",
  };
}

export async function clashGetConnections(
  baseUrl = DEFAULT_CLASH_API_BASE,
): Promise<{ ok: boolean; data?: Json; error?: string }> {
  const kind = await detectBackend();
  if (kind === "tauri") {
    return tauriInvoke("clash_get_connections", { baseUrl });
  }
  if (kind === "bridge") {
    const q = encodeURIComponent(baseUrl);
    return bridgeFetch(`/clash/connections?base=${q}`);
  }
  return {
    ok: false,
    error:
      "Browser cannot reliably call clash_api (CORS). Start Tauri or local bridge.",
  };
}

export async function clashDelay(
  name: string,
  baseUrl = DEFAULT_CLASH_API_BASE,
): Promise<{ ok: boolean; data?: { name: string; delay?: number | null }; error?: string }> {
  const kind = await detectBackend();
  if (kind === "tauri") {
    return tauriInvoke("clash_delay", { name, baseUrl });
  }
  if (kind === "bridge") {
    return bridgeFetch("/clash/delay", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ name, baseUrl }),
    });
  }
  return {
    ok: false,
    error: "Need Tauri or bridge for delay tests (avoids CORS)",
  };
}

export async function bridgeValidate(
  text: string,
): Promise<{ ok: boolean; errors?: { path: string; message: string }[] }> {
  const kind = await detectBackend();
  if (kind === "tauri") {
    return tauriInvoke("validate_config", { text });
  }
  if (kind === "bridge") {
    return bridgeFetch("/validate", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ text }),
    });
  }
  return { ok: false, errors: [{ path: "$", message: "use local validateConfigText" }] };
}

export async function systemProxyStatus(): Promise<{
  ok: boolean;
  enabled?: boolean;
  host?: string;
  port?: number;
  error?: string;
}> {
  const kind = await detectBackend();
  if (kind === "tauri") {
    return tauriInvoke("system_proxy_status");
  }
  return { ok: false, error: "System proxy requires Tauri on macOS", enabled: false };
}

export async function systemProxySet(enabled: boolean): Promise<{
  ok: boolean;
  enabled?: boolean;
  host?: string;
  port?: number;
  services?: string[];
  warnings?: string[];
  error?: string;
}> {
  const kind = await detectBackend();
  if (kind === "tauri") {
    return tauriInvoke("system_proxy_set", { enabled });
  }
  return { ok: false, error: "System proxy requires Tauri on macOS", enabled: false };
}
