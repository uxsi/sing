# sing

Cross-platform desktop client for **sing-box**: Clash-like capabilities, SFM-like UI, with **office / abroad / manual** modes for corporate VPN coexistence.

Kernel = official sing-box. This repo does not fork the core.

## Modes

| Mode | Behavior |
|------|----------|
| **office** | Coexist with company tunnel. Insert GitHub / ssh / git to direct before geosite-github to proxy; strict_route false; bias final selector toward direct. |
| **abroad** | Prefer clean DNS / exclusive TUN. Do not force GitHub direct; set strict_route true when TUN exists; warn about dual TUN. |
| **manual** | No patch — run baseline JSON as-is. |

## Layout

apps/desktop — Vite + React panel
packages/controller — Mode patches, health probe, CLI, merge
configs/examples — Baseline + mode patches + health fixtures
docs/ — Product and architecture docs

## Quick start

cd /workspace/sing
npm install
npm test
npm run build
npm run dev
npm run cli -- merge --mode office --baseline configs/examples/baseline.json
npm run cli -- health --fixture configs/examples/health-dirty-dns.json

## Docs

See docs/.
