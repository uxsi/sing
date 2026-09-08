import { useCallback, useMemo, useState } from "react";
import {
  validateConfigText,
  type ValidationResult,
} from "@sing/controller/validate";
import {
  DEFAULT_CLASH_API_BASE,
  delayTest,
  getConnections,
  getProxies,
} from "@sing/controller/clashApi";
import { fetchSubscriptionBody } from "@sing/controller/subscribeCore";

type Mode = "office" | "abroad" | "manual";

interface HealthBanner {
  level: "ok" | "warn";
  text: string;
}

interface ProxyRow {
  name: string;
  type?: string;
  delayMs?: number | null;
}

interface ConnRow {
  id: string;
  meta: string;
}

const MODE_HELP: Record<Mode, string> = {
  office:
    "Coexist with company tunnel / 内网. GitHub / ssh / git go direct; strict_route off; final biased to direct.",
  abroad:
    "Prefer clean DNS and exclusive TUN. strict_route on; do not force GitHub direct. Watch for dual TUN.",
  manual: "No mode patch — run baseline config as-is.",
};

function listProxiesFromPayload(data: Record<string, unknown>): ProxyRow[] {
  const proxies = (data.proxies ?? data) as Record<string, unknown>;
  if (!proxies || typeof proxies !== "object") return [];
  return Object.entries(proxies)
    .filter(([name]) => name !== "GLOBAL" || true)
    .map(([name, info]) => {
      const obj = (info ?? {}) as Record<string, unknown>;
      return {
        name,
        type: typeof obj.type === "string" ? obj.type : undefined,
        delayMs: null,
      };
    })
    .filter((p) => p.name !== "Compatible")
    .slice(0, 40);
}

function listConnectionsFromPayload(data: Record<string, unknown>): ConnRow[] {
  const conns = (data.connections as unknown[]) ?? [];
  if (!Array.isArray(conns)) return [];
  return conns.slice(0, 50).map((c, i) => {
    const row = (c ?? {}) as Record<string, unknown>;
    const metaObj = (row.metadata ?? {}) as Record<string, unknown>;
    const host =
      (metaObj.host as string) ||
      (metaObj.destinationIP as string) ||
      (row.id as string) ||
      `#${i}`;
    const chain = Array.isArray(row.chains) ? row.chains.join(" → ") : "";
    return {
      id: String(row.id ?? i),
      meta: chain ? `${host} · ${chain}` : String(host),
    };
  });
}

export function App() {
  const [mode, setMode] = useState<Mode>("office");
  const [connected, setConnected] = useState(false);
  const [logs, setLogs] = useState<string[]>([
    "[ready] sing desktop phase-2 panel",
    "[hint] Subscribe / validate / clash_api wired; core connect remains a stub",
  ]);

  const [subUrl, setSubUrl] = useState("");
  const [subStatus, setSubStatus] = useState<string>("");
  const [subPreview, setSubPreview] = useState("");

  const [validateText, setValidateText] = useState("");
  const [validation, setValidation] = useState<ValidationResult | null>(null);

  const [apiBase] = useState(DEFAULT_CLASH_API_BASE);
  const [proxies, setProxies] = useState<ProxyRow[]>([]);
  const [proxyError, setProxyError] = useState<string>("");
  const [connections, setConnections] = useState<ConnRow[]>([]);
  const [connError, setConnError] = useState<string>("");
  const [busy, setBusy] = useState<string | null>(null);

  const health: HealthBanner = useMemo(() => {
    if (mode === "office") {
      return {
        level: "warn",
        text: "Office mode: if dual TUN or dirty DNS for GitHub is detected, protect with direct rules (公司隧道 / 内网 coexistence).",
      };
    }
    if (mode === "abroad") {
      return {
        level: "warn",
        text: "Abroad mode: disconnect company tunnel if possible; dual TUN is risky.",
      };
    }
    return { level: "ok", text: "Manual mode: baseline only — no automatic health patch." };
  }, [mode]);

  const appendLog = useCallback((line: string) => {
    const ts = new Date().toLocaleTimeString();
    setLogs((prev) => [...prev, `[${ts}] ${line}`]);
  }, []);

  function onToggleConnect() {
    setConnected((c) => {
      const next = !c;
      appendLog(next ? `connect stub (${mode})` : "disconnect stub");
      return next;
    });
  }

  async function onFetchSubscribe() {
    setBusy("subscribe");
    setSubStatus("");
    setSubPreview("");
    try {
      const result = await fetchSubscriptionBody({
        url: subUrl.trim(),
        userAgent: "sing-desktop/0.1",
      });
      if (!result.ok) {
        setSubStatus(result.error);
        appendLog(`subscribe fail: ${result.error}`);
        return;
      }
      setSubStatus(
        `Fetched ${result.body.length} chars` +
          (result.contentType ? ` (${result.contentType})` : "") +
          " — browser cannot save to disk; paste into validate or use CLI `subscribe --out`.",
      );
      setSubPreview(result.body.slice(0, 1200));
      setValidateText(result.body);
      appendLog("subscribe ok (preview loaded into validate box)");
    } catch (err: unknown) {
      const message = err instanceof Error ? err.message : String(err);
      setSubStatus(message);
      appendLog(`subscribe error: ${message}`);
    } finally {
      setBusy(null);
    }
  }

  function onValidate() {
    const result = validateConfigText(validateText || "{}");
    setValidation(result);
    appendLog(
      result.ok
        ? "validate ok"
        : `validate failed (${result.errors.length} error(s))`,
    );
  }

  async function onRefreshProxies() {
    setBusy("proxies");
    setProxyError("");
    try {
      const result = await getProxies({ baseUrl: apiBase, timeoutMs: 2500 });
      if (!result.ok) {
        setProxies([]);
        setProxyError(result.error);
        appendLog(`proxies: ${result.error}`);
        return;
      }
      const rows = listProxiesFromPayload(result.data);
      setProxies(rows);
      appendLog(`proxies: ${rows.length} entr(y/ies)`);
    } finally {
      setBusy(null);
    }
  }

  async function onDelay(name: string) {
    setBusy(`delay:${name}`);
    try {
      const result = await delayTest(name, { baseUrl: apiBase, timeoutMs: 7000 });
      if (!result.ok) {
        appendLog(`delay ${name}: ${result.error}`);
        setProxyError(result.error);
        return;
      }
      const delay = result.data.delay ?? null;
      setProxies((prev) =>
        prev.map((p) => (p.name === name ? { ...p, delayMs: delay } : p)),
      );
      appendLog(`delay ${name}: ${delay ?? "n/a"} ms`);
    } finally {
      setBusy(null);
    }
  }

  async function onRefreshConnections() {
    setBusy("connections");
    setConnError("");
    try {
      const result = await getConnections({ baseUrl: apiBase, timeoutMs: 2500 });
      if (!result.ok) {
        setConnections([]);
        setConnError(result.error);
        appendLog(`connections: ${result.error}`);
        return;
      }
      const rows = listConnectionsFromPayload(result.data);
      setConnections(rows);
      appendLog(`connections: ${rows.length}`);
    } finally {
      setBusy(null);
    }
  }

  return (
    <div className="app">
      <aside className="sidebar">
        <div className="brand">sing</div>
        <div className="section-label">Mode</div>
        {(Object.keys(MODE_HELP) as Mode[]).map((m) => (
          <button
            key={m}
            type="button"
            className={"mode-btn" + (mode === m ? " active" : "")}
            onClick={() => {
              setMode(m);
              appendLog(`mode -> ${m}`);
            }}
          >
            {m}
          </button>
        ))}
        <p className="mode-help">{MODE_HELP[mode]}</p>
      </aside>

      <main className="main">
        <header className="topbar">
          <div>
            <h1>Dashboard</h1>
            <p className="muted">SFM-like shell · Phase 2</p>
          </div>
          <button
            type="button"
            className={"connect" + (connected ? " on" : "")}
            onClick={onToggleConnect}
          >
            {connected ? "Disconnect" : "Connect"}
          </button>
        </header>

        <div className={"banner " + health.level}>{health.text}</div>

        <section className="cards">
          <div className="card">
            <h2>Status</h2>
            <ul>
              <li>
                Mode: <strong>{mode}</strong>
              </li>
              <li>
                Core: <strong>{connected ? "stub running" : "stopped"}</strong>
              </li>
              <li>
                Mixed: <strong>127.0.0.1:1080</strong>
              </li>
              <li>
                clash_api: <strong>{apiBase.replace(/^https?:\/\//, "")}</strong>
              </li>
            </ul>
          </div>

          <div className="card">
            <h2>Subscribe</h2>
            <p className="muted">Fetch subscription URL (CORS may block in browser).</p>
            <div className="field-row">
              <input
                className="input"
                type="url"
                placeholder="https://…"
                value={subUrl}
                onChange={(e) => setSubUrl(e.target.value)}
              />
              <button
                type="button"
                className="ghost primary"
                disabled={!subUrl.trim() || busy === "subscribe"}
                onClick={() => void onFetchSubscribe()}
              >
                Fetch
              </button>
            </div>
            {subStatus ? <p className="status-line">{subStatus}</p> : null}
            {subPreview ? <pre className="preview">{subPreview}</pre> : null}
          </div>

          <div className="card">
            <h2>Validate</h2>
            <p className="muted">Paste sing-box JSON or use subscribe preview.</p>
            <textarea
              className="textarea"
              rows={6}
              placeholder='{"outbounds":[],"route":{}}'
              value={validateText}
              onChange={(e) => setValidateText(e.target.value)}
            />
            <div className="field-row">
              <button type="button" className="ghost primary" onClick={onValidate}>
                Validate
              </button>
              {validation ? (
                <span className={"pill " + (validation.ok ? "ok" : "bad")}>
                  {validation.ok ? "ok" : `${validation.errors.length} error(s)`}
                </span>
              ) : null}
            </div>
            {validation && !validation.ok ? (
              <ul className="error-list">
                {validation.errors.map((e, i) => (
                  <li key={i}>
                    <code>{e.path}</code>: {e.message}
                  </li>
                ))}
              </ul>
            ) : null}
          </div>
        </section>

        <section className="cards">
          <div className="card">
            <div className="card-head">
              <h2>Proxies</h2>
              <button
                type="button"
                className="ghost"
                disabled={busy === "proxies"}
                onClick={() => void onRefreshProxies()}
              >
                Refresh
              </button>
            </div>
            {proxyError ? <p className="status-line warn">{proxyError}</p> : null}
            {!proxyError && proxies.length === 0 ? (
              <p className="muted">Empty — core not running or clash_api unreachable from browser.</p>
            ) : (
              <ul className="list">
                {proxies.map((p) => (
                  <li key={p.name} className="list-row">
                    <div>
                      <strong>{p.name}</strong>
                      <span className="muted">
                        {" "}
                        {p.type ?? ""}
                        {p.delayMs != null ? ` · ${p.delayMs} ms` : ""}
                      </span>
                    </div>
                    <button
                      type="button"
                      className="ghost"
                      disabled={busy === `delay:${p.name}`}
                      onClick={() => void onDelay(p.name)}
                    >
                      Delay
                    </button>
                  </li>
                ))}
              </ul>
            )}
          </div>

          <div className="card">
            <div className="card-head">
              <h2>Connections</h2>
              <button
                type="button"
                className="ghost"
                disabled={busy === "connections"}
                onClick={() => void onRefreshConnections()}
              >
                Refresh
              </button>
            </div>
            {connError ? <p className="status-line warn">{connError}</p> : null}
            {!connError && connections.length === 0 ? (
              <p className="muted">No connections — refresh when core + clash_api are up.</p>
            ) : (
              <ul className="list">
                {connections.map((c) => (
                  <li key={c.id} className="list-row">
                    <span>{c.meta}</span>
                  </li>
                ))}
              </ul>
            )}
          </div>
        </section>

        <section className="log-panel">
          <div className="log-head">
            <h2>Logs</h2>
            <button type="button" className="ghost" onClick={() => setLogs([])}>
              Clear
            </button>
          </div>
          <pre className="log-body">{logs.join("\n")}</pre>
        </section>
      </main>
    </div>
  );
}
