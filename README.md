# sing

Cross-platform desktop client for **sing-box**: Clash-like capabilities, SFM-like UI, with **office / abroad / manual** modes for corporate VPN coexistence.

Kernel = official sing-box. This repo does not fork the core.

**Priority**: macOS end-to-end first; Windows deferred.

## Modes

| Mode | Behavior |
|------|----------|
| **office** | Coexist with 公司隧道 / 内网. Insert GitHub / ssh / git to direct before geosite-github to proxy; strict_route false; bias final selector toward direct. |
| **abroad** | Prefer clean DNS / exclusive TUN. Do not force GitHub direct; set strict_route true when TUN exists; warn about dual TUN. |
| **manual** | No patch - run baseline JSON as-is. |

## Layout

apps/desktop - Vite + React panel (+ src-tauri Tauri 2 macOS shell)
packages/controller - Mode patches, health probe, core supervisor, subscribe, validate, clash_api, CLI, merge
packages/bridge - Local HTTP bridge (127.0.0.1:8787) for UI to controller without CORS
configs/examples - Baseline + mode patches + health fixtures / rules
docs/ - Product and architecture docs

## Quick start

npm install
npm test

Desktop UI (browser stub or with bridge):

npm run bridge          # terminal A — optional, needed for core/clash without Tauri
npm run desktop:dev     # terminal B — Vite on 127.0.0.1:5173

macOS Tauri (preferred for real core start): see docs/05-macos-tauri.md

npm run desktop:dev
cd apps/desktop && cargo tauri dev

## CLI

Use the workspace cli script with a command name.

Phase 1: merge, health, detect-core
Phase 2: subscribe, validate, proxies, connections, delay
Phase 3: core status | start --config PATH | stop; health --rules PATH

Examples (prefix with workspace cli runner):
  merge --mode office --baseline configs/examples/baseline.json
  health --fixture configs/examples/health-dirty-dns.json
  health --fixture configs/examples/health-clean.json --rules configs/examples/health-rules.json
  detect-core
  core status
  core start --config configs/examples/baseline.json
  core stop
  subscribe --url URL --out PATH
  validate --file configs/examples/baseline.json
  proxies
  connections
  delay --name PROXY

`core start` soft-fails (exit 0, softFail: true) when the sing-box binary is missing.

## Docs

See docs/. macOS Tauri / bridge: docs/05-macos-tauri.md.
