# macOS: Tauri desktop ↔ core bridge

macOS-first path so the Vite+React panel can **really** start sing-box and talk to `clash_api` on `127.0.0.1:9090` without browser CORS.

Windows is deferred. TUN entitlements / Network Extension are **later**.

## What you get

| Layer | Role |
|-------|------|
| `apps/desktop` | Vite + React UI (existing) |
| `apps/desktop/src-tauri` | Tauri 2 shell: Rust commands for core + clash_api |
| `packages/bridge` | Thin Node HTTP bridge on `127.0.0.1:8787` (cloud / browser testing) |
| `packages/controller` | Shared logic (CLI + bridge) |

UI picks a backend automatically:

1. **Tauri** — `window.__TAURI__` / invoke
2. **Node bridge** — health check on `http://127.0.0.1:8787/health`
3. **Stub** — clear “need Tauri or bridge” messaging

## Prerequisites (Mac)

- Node.js ≥ 18
- Xcode CLT (`xcode-select --install`)
- Rust via rustup (see https://rustup.rs )
- Recent stable rustc (Tauri 2 deps currently want **≥ 1.88**; run `rustup update`)
- Tauri CLI 2.x: `cargo install tauri-cli --version "^2"` or workspace `@tauri-apps/cli`
- Optional: `sing-box` on `PATH` or under `bin/`. Missing binary → **soft-fail**.

## Install (repo root)

```
npm install
npm test
```

## Option A — Tauri on Mac (preferred)

Terminal 1 — Vite UI:

```
npm run desktop:dev
```

Terminal 2 — Tauri shell (loads `http://127.0.0.1:5173`):

```
cd apps/desktop
cargo tauri dev
```

Or, with `@tauri-apps/cli` installed in the workspace:

```
npm run tauri:dev
```

Use **Connect** in the UI (default config `configs/examples/baseline.json`). Proxies / connections / delay go through Rust → `:9090` (no CORS).

### Rust commands exposed

- `core_status` / `core_start(configPath)` / `core_stop`
- `clash_get_proxies` / `clash_get_connections` / `clash_delay(name)`
- `validate_config` (lite JSON object check)
- `health_classify` (lite dual-TUN / dirty-DNS hints)

`merge_mode` stays on the Node bridge / CLI for now.

## Option B — Node local bridge (cloud / browser)

When Rust/Tauri is unavailable:

```
npm run bridge
```

Default listen: `http://127.0.0.1:8787`

Then start the UI with `desktop:dev`. It should detect **Node bridge :8787**.

Bridge routes: `/health`, `/core/status|start|stop`, `/clash/proxies|connections`, `/clash/delay`, `/validate`, `/merge`, `/health/classify`.

## Clash API note

Browser pages cannot reliably call `http://127.0.0.1:9090` (CORS). Always use Tauri or the Node bridge for panel refresh / delay.

## Later

- macOS TUN entitlements / System Extension guidance
- Packaging signed `.app` / DMG
- Windows skeleton (explicitly deferred)

## Smoke checklist

1. `npm test` green
2. Bridge health: open `http://127.0.0.1:8787/health`
3. Tauri Connect soft-fails cleanly if `sing-box` missing; succeeds when binary + config present and clash_api enabled
