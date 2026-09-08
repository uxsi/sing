import { accessSync, constants } from "node:fs";
import { spawn, type ChildProcess } from "node:child_process";
import { delimiter, join, resolve } from "node:path";

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

export type CoreRunState =
  | "stopped"
  | "running"
  | "missing_binary"
  | "crashed"
  | "starting";

export interface CoreStatus {
  state: CoreRunState;
  binary: string | null;
  configPath: string | null;
  pid: number | null;
  restarts: number;
  maxRestarts: number;
  lastError: string | null;
  softFail: boolean;
}

export interface SupervisorOptions {
  /** Override binary path; null forces missing-binary soft-fail. */
  binary?: string | null;
  configPath?: string;
  maxRestarts?: number;
  /** Base backoff in ms; doubles each restart (capped). */
  backoffMs?: number;
  /** Injected spawn for tests / stubs. */
  spawnFn?: (binary: string, args: string[], configPath: string) => ChildProcess;
  /** Injected detect for tests. */
  detectFn?: () => string | null;
}

type ExitListener = (code: number | null, signal: NodeJS.Signals | null) => void;

/**
 * In-process sing-box supervisor: detect, start, stop/kill, crash restart
 * with max restarts + exponential backoff. Soft-fails when binary missing.
 * macOS-first; works anywhere Node can spawn the binary.
 */
export class CoreSupervisor {
  private binary: string | null | undefined;
  private configPath: string | null;
  private child: ChildProcess | null = null;
  private state: CoreRunState = "stopped";
  private restarts = 0;
  private lastError: string | null = null;
  private softFail = false;
  private stopping = false;
  private restartTimer: ReturnType<typeof setTimeout> | null = null;
  private readonly maxRestarts: number;
  private readonly backoffMs: number;
  private readonly spawnFn?: SupervisorOptions["spawnFn"];
  private readonly detectFn: () => string | null;
  private exitHandler: ExitListener | null = null;

  constructor(options: SupervisorOptions = {}) {
    this.binary = options.binary;
    this.configPath = options.configPath ?? null;
    this.maxRestarts = options.maxRestarts ?? 3;
    this.backoffMs = options.backoffMs ?? 500;
    this.spawnFn = options.spawnFn;
    this.detectFn = options.detectFn ?? (() => detectSingBoxBinary());
  }

  status(): CoreStatus {
    return {
      state: this.state,
      binary: this.resolveBinary(),
      configPath: this.configPath,
      pid: this.child?.pid ?? null,
      restarts: this.restarts,
      maxRestarts: this.maxRestarts,
      lastError: this.lastError,
      softFail: this.softFail,
    };
  }

  private resolveBinary(): string | null {
    if (this.binary === null) return null;
    if (typeof this.binary === "string") return this.binary;
    return this.detectFn();
  }

  /**
   * Start core with config. Soft-fails (ok:false, softFail:true) if binary missing.
   */
  start(configPath?: string): {
    ok: boolean;
    softFail?: boolean;
    error?: string;
  } {
    if (configPath) this.configPath = resolve(configPath);
    if (!this.configPath) {
      this.lastError = "config path required";
      return { ok: false, error: this.lastError };
    }

    this.clearRestartTimer();
    this.stopping = false;
    this.softFail = false;

    const binary = this.resolveBinary();
    if (!binary) {
      this.state = "missing_binary";
      this.softFail = true;
      this.lastError = "sing-box binary not found";
      this.binary = null;
      return { ok: false, softFail: true, error: this.lastError };
    }
    this.binary = binary;

    if (this.child && !this.child.killed) {
      this.detachChild();
    }

    return this.spawnOnce(binary, this.configPath);
  }

  stop(): { ok: boolean; error?: string } {
    this.stopping = true;
    this.clearRestartTimer();
    if (!this.child) {
      this.state = "stopped";
      return { ok: true };
    }
    const child = this.child;
    this.detachChild();
    try {
      child.kill("SIGTERM");
    } catch (err: unknown) {
      this.lastError = err instanceof Error ? err.message : String(err);
      this.state = "stopped";
      return { ok: false, error: this.lastError };
    }
    try {
      if (!child.killed && child.exitCode === null) {
        child.kill("SIGKILL");
      }
    } catch {
      /* ignore */
    }
    this.state = "stopped";
    this.restarts = 0;
    return { ok: true };
  }

  /** Test helper: simulate unexpected child exit. */
  _simulateCrash(code: number | null = 1): void {
    if (this.exitHandler) this.exitHandler(code, null);
  }

  private spawnOnce(
    binary: string,
    configPath: string,
  ): { ok: boolean; softFail?: boolean; error?: string } {
    this.state = "starting";
    const args = ["run", "-c", configPath];
    let child: ChildProcess;
    try {
      child = this.spawnFn
        ? this.spawnFn(binary, args, configPath)
        : spawn(binary, args, { stdio: ["ignore", "pipe", "pipe"] });
    } catch (err: unknown) {
      this.lastError = err instanceof Error ? err.message : String(err);
      this.state = "crashed";
      return { ok: false, error: this.lastError };
    }

    this.child = child;
    this.state = "running";
    this.lastError = null;

    const onExit: ExitListener = (code, signal) => {
      if (this.child !== child) return;
      this.child = null;
      if (this.stopping) {
        this.state = "stopped";
        return;
      }
      this.handleCrash(code, signal);
    };
    this.exitHandler = onExit;
    child.once("exit", onExit);
    child.once("error", (err) => {
      this.lastError = err.message;
      if (this.child === child) {
        this.child = null;
        if (!this.stopping) this.handleCrash(null, null);
      }
    });

    return { ok: true };
  }

  private handleCrash(
    code: number | null,
    signal: NodeJS.Signals | null,
  ): void {
    this.lastError = `core exited (code=${code ?? "null"}, signal=${signal ?? "null"})`;
    if (this.restarts >= this.maxRestarts) {
      this.state = "crashed";
      return;
    }
    this.restarts += 1;
    const delay = Math.min(
      this.backoffMs * 2 ** (this.restarts - 1),
      30_000,
    );
    this.state = "starting";
    this.restartTimer = setTimeout(() => {
      this.restartTimer = null;
      if (this.stopping || !this.configPath) {
        this.state = "stopped";
        return;
      }
      const binary = this.resolveBinary();
      if (!binary) {
        this.state = "missing_binary";
        this.softFail = true;
        this.lastError = "sing-box binary not found";
        return;
      }
      this.spawnOnce(binary, this.configPath);
    }, delay);
    if (typeof this.restartTimer.unref === "function") {
      this.restartTimer.unref();
    }
  }

  private detachChild(): void {
    if (this.child && this.exitHandler) {
      this.child.removeListener("exit", this.exitHandler);
    }
    this.exitHandler = null;
    this.child = null;
  }

  private clearRestartTimer(): void {
    if (this.restartTimer) {
      clearTimeout(this.restartTimer);
      this.restartTimer = null;
    }
  }
}

/** Process-local supervisor used by CLI `core` commands. */
let defaultSupervisor: CoreSupervisor | null = null;

export function getDefaultSupervisor(): CoreSupervisor {
  if (!defaultSupervisor) defaultSupervisor = new CoreSupervisor();
  return defaultSupervisor;
}

export function resetDefaultSupervisor(): void {
  if (defaultSupervisor) {
    defaultSupervisor.stop();
  }
  defaultSupervisor = null;
}
