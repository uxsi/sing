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

apps/desktop - Vite + React panel
packages/controller - Mode patches, health probe, core supervisor, subscribe, validate, clash_api, CLI, merge
configs/examples - Baseline + mode patches + health fixtures / rules
docs/ - Product and architecture docs

## Quick start

npm install
npm test

then build and start the desktop with the root scripts

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

See docs/.
