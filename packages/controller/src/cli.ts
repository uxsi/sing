#!/usr/bin/env node
import { readFileSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import { classifyHealth, loadHealthRules } from "./health.js";
import { loadJsonConfig, mergeFromFiles } from "./merge.js";
import {
  detectSingBoxBinary,
  getDefaultSupervisor,
} from "./core.js";
import { fetchSubscription } from "./subscribe.js";
import { validateConfigText } from "./validate.js";
import {
  delayTest,
  getConnections,
  getProxies,
  DEFAULT_CLASH_API_BASE,
} from "./clashApi.js";
import type { HealthInput, Mode } from "./types.js";

function printHelp(): void {
  console.log(`singctl — sing controller CLI

Usage:
  singctl merge --mode <office|abroad|manual> --baseline <path> [--out <path>]
  singctl health --fixture <path> [--rules <path>]
  singctl detect-core
  singctl core status|start --config <path>|stop
  singctl subscribe --url <url> --out <path> [--ua <user-agent>] [--timeout <ms>]
  singctl validate --file <path>
  singctl proxies [--base <url>]
  singctl connections [--base <url>]
  singctl delay --name <proxy> [--base <url>]

Examples:
  singctl merge --mode office --baseline configs/examples/baseline.json
  singctl health --fixture configs/examples/health-dirty-dns.json
  singctl health --fixture configs/examples/health-clean.json --rules configs/examples/health-rules.json
  singctl core status
  singctl core start --config configs/examples/baseline.json
  singctl core stop
  singctl subscribe --url https://example.com/sub --out /tmp/sub.json
  singctl validate --file configs/examples/baseline.json
  singctl proxies
  singctl connections
  singctl delay --name proxy
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

function apiBase(args: string[]): string {
  return argValue(args, "--base") ?? DEFAULT_CLASH_API_BASE;
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
    const rulesPath = argValue(args, "--rules");
    const rules = loadHealthRules(rulesPath ? resolve(rulesPath) : null);
    const input = loadJsonConfig(resolve(fixture)) as HealthInput;
    const result = classifyHealth(input, rules);
    console.log(JSON.stringify(result, null, 2));
    process.exitCode = result.ok ? 0 : 2;
    return;
  }

  if (cmd === "detect-core") {
    const bin = detectSingBoxBinary();
    console.log(JSON.stringify({ binary: bin }, null, 2));
    return;
  }

  if (cmd === "core") {
    const sub = args[1];
    const supervisor = getDefaultSupervisor();
    if (sub === "status") {
      console.log(JSON.stringify(supervisor.status(), null, 2));
      return;
    }
    if (sub === "start") {
      const config = argValue(args, "--config");
      if (!config) throw new Error("core start requires --config <path>");
      const result = supervisor.start(resolve(config));
      console.log(JSON.stringify({ ...result, status: supervisor.status() }, null, 2));
      if (result.softFail) {
        process.exitCode = 0; // soft-fail: binary missing is non-fatal
      } else {
        process.exitCode = result.ok ? 0 : 1;
      }
      return;
    }
    if (sub === "stop") {
      const result = supervisor.stop();
      console.log(JSON.stringify({ ...result, status: supervisor.status() }, null, 2));
      process.exitCode = result.ok ? 0 : 1;
      return;
    }
    throw new Error("core usage: status | start --config <path> | stop");
  }

  if (cmd === "subscribe") {
    const url = argValue(args, "--url");
    const out = argValue(args, "--out");
    if (!url) throw new Error("--url <url> is required");
    if (!out) throw new Error("--out <path> is required");
    const ua = argValue(args, "--ua");
    const timeoutRaw = argValue(args, "--timeout");
    const timeoutMs = timeoutRaw ? Number(timeoutRaw) : undefined;
    if (timeoutRaw && (!Number.isFinite(timeoutMs) || (timeoutMs as number) <= 0)) {
      throw new Error("--timeout must be a positive number (ms)");
    }
    const result = await fetchSubscription({
      url,
      outPath: resolve(out),
      userAgent: ua,
      timeoutMs,
    });
    console.log(JSON.stringify(result, null, 2));
    process.exitCode = result.ok ? 0 : 1;
    return;
  }

  if (cmd === "validate") {
    const file = argValue(args, "--file");
    if (!file) throw new Error("--file <path> is required");
    const abs = resolve(file);
    let text: string;
    try {
      text = readFileSync(abs, "utf8");
    } catch (err: unknown) {
      const message = err instanceof Error ? err.message : String(err);
      throw new Error(`Cannot read file: ${message}`);
    }
    const result = validateConfigText(text);
    console.log(JSON.stringify(result, null, 2));
    process.exitCode = result.ok ? 0 : 1;
    return;
  }

  if (cmd === "proxies") {
    const result = await getProxies({ baseUrl: apiBase(args) });
    console.log(JSON.stringify(result, null, 2));
    process.exitCode = result.ok ? 0 : 1;
    return;
  }

  if (cmd === "connections") {
    const result = await getConnections({ baseUrl: apiBase(args) });
    console.log(JSON.stringify(result, null, 2));
    process.exitCode = result.ok ? 0 : 1;
    return;
  }

  if (cmd === "delay") {
    const name = argValue(args, "--name");
    if (!name) throw new Error("--name <proxy> is required");
    const result = await delayTest(name, { baseUrl: apiBase(args) });
    console.log(JSON.stringify(result, null, 2));
    process.exitCode = result.ok ? 0 : 1;
    return;
  }

  printHelp();
  throw new Error(`Unknown command: ${cmd}`);
}

main(process.argv).catch((err: unknown) => {
  console.error(err instanceof Error ? err.message : err);
  process.exitCode = 1;
});
