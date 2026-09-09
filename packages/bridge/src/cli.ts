#!/usr/bin/env node
import { listenBridge, DEFAULT_BRIDGE_HOST, DEFAULT_BRIDGE_PORT } from "./server.js";

function argValue(args: string[], name: string): string | undefined {
  const i = args.indexOf(name);
  if (i === -1) return undefined;
  return args[i + 1];
}

async function main(): Promise<void> {
  const args = process.argv.slice(2);
  const host = argValue(args, "--host") ?? process.env.SING_BRIDGE_HOST ?? DEFAULT_BRIDGE_HOST;
  const portRaw = argValue(args, "--port") ?? process.env.SING_BRIDGE_PORT;
  const port = portRaw ? Number(portRaw) : DEFAULT_BRIDGE_PORT;
  if (!Number.isFinite(port) || port <= 0) {
    throw new Error("invalid --port");
  }

  const { url } = await listenBridge({ host, port });
  console.log(`[sing-bridge] listening on ${url}`);
  console.log(`[sing-bridge] GET  /health`);
  console.log(`[sing-bridge] GET  /core/status  POST /core/start  POST /core/stop`);
  console.log(`[sing-bridge] GET  /clash/proxies  /clash/connections  POST /clash/delay`);
  console.log(`[sing-bridge] POST /validate  /merge  /health/classify`);
}

main().catch((err: unknown) => {
  console.error(err instanceof Error ? err.message : err);
  process.exitCode = 1;
});
