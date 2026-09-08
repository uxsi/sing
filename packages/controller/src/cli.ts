#!/usr/bin/env node
import { writeFileSync } from "node:fs";
import { resolve } from "node:path";
import { classifyHealth } from "./health.js";
import { loadJsonConfig, mergeFromFiles } from "./merge.js";
import { detectSingBoxBinary } from "./core.js";
import type { HealthInput, Mode } from "./types.js";

function printHelp(): void {
  console.log(`singctl — sing controller CLI

Usage:
  singctl merge --mode <office|abroad|manual> --baseline <path> [--out <path>]
  singctl health --fixture <path>
  singctl detect-core

Examples:
  singctl merge --mode office --baseline configs/examples/baseline.json
  singctl health --fixture configs/examples/health-dirty-dns.json
`);
}

function argValue(args: string[], name: string): string | undefined {
  const i = args.indexOf(name);
  if (i === -1) return undefined;
  return args[i + 1];
}

function parseMode(raw: string | undefined): Mode {
  if (raw === "office" || raw === "abroad" || raw === "manual") return raw;
  throw new Error(`Invalid mode: ${raw ?? "(missing)"}. Use office|abroad|manual`);
}

async function main(argv: string[]): Promise<void> {
  const args = argv.slice(2);
  const cmd = args[0];

  if (!cmd || cmd === "-h" || cmd === "--help") {
    printHelp();
    return;
  }

  if (cmd === "merge") {
    const mode = parseMode(argValue(args, "--mode"));
    const baseline = argValue(args, "--baseline");
    if (!baseline) throw new Error("--baseline <path> is required");
    const out = argValue(args, "--out");
    const result = mergeFromFiles(resolve(baseline), mode);
    const payload = {
      meta: result.meta,
      config: result.config,
    };
    const text = JSON.stringify(payload, null, 2);
    if (out) {
      writeFileSync(resolve(out), text + "\n");
      console.log(`Wrote merged config to ${out}`);
    } else {
      console.log(text);
    }
    return;
  }

  if (cmd === "health") {
    const fixture = argValue(args, "--fixture");
    if (!fixture) throw new Error("--fixture <path> is required");
    const input = loadJsonConfig(resolve(fixture)) as HealthInput;
    const result = classifyHealth(input);
    console.log(JSON.stringify(result, null, 2));
    process.exitCode = result.ok ? 0 : 2;
    return;
  }

  if (cmd === "detect-core") {
    const bin = detectSingBoxBinary();
    console.log(JSON.stringify({ binary: bin }, null, 2));
    return;
  }

  printHelp();
  throw new Error(`Unknown command: ${cmd}`);
}

main(process.argv).catch((err: unknown) => {
  console.error(err instanceof Error ? err.message : err);
  process.exitCode = 1;
});
