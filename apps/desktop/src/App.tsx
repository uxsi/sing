import { useMemo, useState } from "react";

type Mode = "office" | "abroad" | "manual";

interface HealthBanner {
  level: "ok" | "warn";
  text: string;
}

const MODE_HELP: Record<Mode, string> = {
  office:
    "Coexist with company tunnel. GitHub / ssh / git go direct; strict_route off; final biased to direct.",
  abroad:
    "Prefer clean DNS and exclusive TUN. strict_route on; do not force GitHub direct. Watch for dual TUN.",
  manual: "No mode patch — run baseline config as-is.",
};

export function App() {
  const [mode, setMode] = useState<Mode>("office");
  const [connected, setConnected] = useState(false);
  const [logs, setLogs] = useState<string[]>([
    "[ready] sing desktop phase-1 UI stub",
    "[hint] Connect is a stub — controller merge/health are in @sing/controller",
  ]);

  const health: HealthBanner = useMemo(() => {
    if (mode === "office") {
      return {
        level: "warn",
        text: "Office mode: if dual TUN or dirty DNS for GitHub is detected, protect with direct rules.",
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

  function appendLog(line: string) {
    const ts = new Date().toLocaleTimeString();
    setLogs((prev) => [...prev, `[${ts}] ${line}`]);
  }

  function onToggleConnect() {
    setConnected((c) => {
      const next = !c;
      appendLog(next ? `connect stub (${mode})` : "disconnect stub");
      return next;
    });
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
            <p className="muted">SFM-like shell · Phase 1</p>
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
              <li>Mode: <strong>{mode}</strong></li>
              <li>Core: <strong>{connected ? "stub running" : "stopped"}</strong></li>
              <li>Mixed: <strong>127.0.0.1:1080</strong></li>
              <li>clash_api: <strong>127.0.0.1:9090</strong></li>
            </ul>
          </div>
          <div className="card">
            <h2>Outbounds</h2>
            <p className="muted">Selector/node switching hooks into controller later.</p>
            <div className="pill-row">
              <span className="pill">final</span>
              <span className="pill">proxy</span>
              <span className="pill">direct</span>
            </div>
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
