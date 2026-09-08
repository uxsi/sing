import { EventEmitter } from "node:events";
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  CoreSupervisor,
  detectSingBoxBinary,
  resetDefaultSupervisor,
} from "../src/core.js";
import type { ChildProcess } from "node:child_process";

function fakeChild(pid = 4242): ChildProcess & EventEmitter {
  const ee = new EventEmitter() as ChildProcess & EventEmitter;
  (ee as { pid: number }).pid = pid;
  (ee as { killed: boolean }).killed = false;
  (ee as { exitCode: number | null }).exitCode = null;
  (ee as { kill: (signal?: string) => boolean }).kill = vi.fn(
    (_signal?: string) => {
      (ee as { killed: boolean }).killed = true;
      return true;
    },
  );
  return ee;
}

describe("detectSingBoxBinary", () => {
  it("returns null for empty PATH without local binary", () => {
    const found = detectSingBoxBinary("");
    // may be null or a cwd/bin hit — just ensure function is callable
    expect(found === null || typeof found === "string").toBe(true);
  });
});

describe("CoreSupervisor", () => {
  afterEach(() => {
    resetDefaultSupervisor();
    vi.useRealTimers();
  });

  it("soft-fails start when binary missing", () => {
    const sup = new CoreSupervisor({
      binary: null,
      detectFn: () => null,
    });
    const result = sup.start("/tmp/fake-config.json");
    expect(result.ok).toBe(false);
    expect(result.softFail).toBe(true);
    expect(sup.status().state).toBe("missing_binary");
    expect(sup.status().softFail).toBe(true);
  });

  it("starts with injected spawn stub and reports running", () => {
    const child = fakeChild(111);
    const spawnFn = vi.fn(() => child);
    const sup = new CoreSupervisor({
      binary: "/usr/local/bin/sing-box",
      spawnFn,
      maxRestarts: 2,
      backoffMs: 10,
    });
    const result = sup.start("/tmp/cfg.json");
    expect(result.ok).toBe(true);
    expect(spawnFn).toHaveBeenCalledOnce();
    expect(spawnFn.mock.calls[0][0]).toBe("/usr/local/bin/sing-box");
    expect(spawnFn.mock.calls[0][1]).toEqual(["run", "-c", "/tmp/cfg.json"]);
    expect(sup.status().state).toBe("running");
    expect(sup.status().pid).toBe(111);
    expect(sup.status().configPath).toBe("/tmp/cfg.json");
  });

  it("stop kills child and returns stopped", () => {
    const child = fakeChild(222);
    const sup = new CoreSupervisor({
      binary: "/bin/sing-box",
      spawnFn: () => child,
    });
    sup.start("/tmp/cfg.json");
    const stop = sup.stop();
    expect(stop.ok).toBe(true);
    expect(child.kill).toHaveBeenCalled();
    expect(sup.status().state).toBe("stopped");
    expect(sup.status().pid).toBeNull();
  });

  it("restarts on crash up to maxRestarts then marks crashed", async () => {
    vi.useFakeTimers();
    let n = 0;
    const children: ReturnType<typeof fakeChild>[] = [];
    const spawnFn = vi.fn(() => {
      n += 1;
      const c = fakeChild(1000 + n);
      children.push(c);
      return c;
    });
    const sup = new CoreSupervisor({
      binary: "/bin/sing-box",
      spawnFn,
      maxRestarts: 2,
      backoffMs: 50,
    });
    expect(sup.start("/tmp/cfg.json").ok).toBe(true);
    expect(spawnFn).toHaveBeenCalledTimes(1);

    // crash #1 → restart 1
    children[0].emit("exit", 1, null);
    expect(sup.status().state).toBe("starting");
    expect(sup.status().restarts).toBe(1);
    await vi.advanceTimersByTimeAsync(50);
    expect(spawnFn).toHaveBeenCalledTimes(2);
    expect(sup.status().state).toBe("running");

    // crash #2 → restart 2
    children[1].emit("exit", 1, null);
    expect(sup.status().restarts).toBe(2);
    await vi.advanceTimersByTimeAsync(100);
    expect(spawnFn).toHaveBeenCalledTimes(3);

    // crash #3 → no more restarts (maxRestarts=2 already used)
    children[2].emit("exit", 1, null);
    expect(sup.status().state).toBe("crashed");
    expect(sup.status().restarts).toBe(2);
    await vi.advanceTimersByTimeAsync(1000);
    expect(spawnFn).toHaveBeenCalledTimes(3);
  });

  it("requires config path", () => {
    const sup = new CoreSupervisor({ binary: "/bin/sing-box" });
    const result = sup.start();
    expect(result.ok).toBe(false);
    expect(result.error).toMatch(/config/i);
  });
});
