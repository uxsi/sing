import { accessSync, constants } from "node:fs";
import { spawn, type ChildProcess } from "node:child_process";
import { delimiter, join } from "node:path";

const CANDIDATE_NAMES = ["sing-box", "singbox"];

function isExecutable(filePath: string): boolean {
  try {
    accessSync(filePath, constants.X_OK);
    return true;
  } catch {
    return false;
  }
}

/**
 * Locate a sing-box binary on PATH or common install locations.
 * Returns absolute path or null if not found.
 */
export function detectSingBoxBinary(
  envPath: string = process.env.PATH ?? "",
): string | null {
  const dirs = envPath.split(delimiter).filter(Boolean);
  const extras = [
    "/usr/local/bin",
    "/opt/homebrew/bin",
    join(process.cwd(), "bin"),
  ];
  const search = [...dirs, ...extras];

  for (const dir of search) {
    for (const name of CANDIDATE_NAMES) {
      const full = join(dir, name);
      if (isExecutable(full)) return full;
    }
  }

  // bare name fallback — let OS resolve later
  for (const name of CANDIDATE_NAMES) {
    try {
      accessSync(name, constants.F_OK);
      return name;
    } catch {
      /* continue */
    }
  }
  return null;
}

export interface SpawnCoreOptions {
  binary?: string;
  configPath: string;
  args?: string[];
}

/** Optionally spawn sing-box run -c <config>. Returns null if binary missing. */
export function spawnSingBox(
  options: SpawnCoreOptions,
): ChildProcess | null {
  const binary = options.binary ?? detectSingBoxBinary();
  if (!binary) return null;
  const args = options.args ?? ["run", "-c", options.configPath];
  return spawn(binary, args, { stdio: ["ignore", "pipe", "pipe"] });
}
