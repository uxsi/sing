import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { validateConfig, validateConfigText } from "../src/validate.js";

const root = join(dirname(fileURLToPath(import.meta.url)), "../../..");
const baselinePath = join(root, "configs/examples/baseline.json");

describe("validateConfig", () => {
  it("accepts baseline example", () => {
    const baseline = JSON.parse(readFileSync(baselinePath, "utf8"));
    const result = validateConfig(baseline);
    expect(result.ok).toBe(true);
    expect(result.errors).toEqual([]);
  });

  it("rejects non-object roots", () => {
    expect(validateConfig(null).ok).toBe(false);
    expect(validateConfig([]).errors[0]?.path).toBe("$");
    expect(validateConfig("x").errors[0]?.message).toMatch(/object/i);
  });

  it("soft-checks broken outbounds / route / dns", () => {
    const result = validateConfig({
      outbounds: [{ tag: "x" }, "bad"],
      route: { rules: "nope", final: 1 },
      dns: { servers: "x", final: false },
      inbounds: [{ listen: "127.0.0.1" }],
    });
    expect(result.ok).toBe(false);
    const paths = result.errors.map((e) => e.path);
    expect(paths).toContain("outbounds[0].type");
    expect(paths).toContain("outbounds[1]");
    expect(paths).toContain("route.rules");
    expect(paths).toContain("route.final");
    expect(paths).toContain("dns.servers");
    expect(paths).toContain("dns.final");
    expect(paths).toContain("inbounds[0].type");
  });

  it("accepts minimal empty object", () => {
    expect(validateConfig({}).ok).toBe(true);
  });
});

describe("validateConfigText", () => {
  it("reports JSON parse errors", () => {
    const result = validateConfigText("{not json");
    expect(result.ok).toBe(false);
    expect(result.errors[0]?.path).toBe("$");
    expect(result.errors[0]?.message).toMatch(/Invalid JSON/);
  });

  it("validates parsed baseline text", () => {
    const text = readFileSync(baselinePath, "utf8");
    expect(validateConfigText(text).ok).toBe(true);
  });
});
