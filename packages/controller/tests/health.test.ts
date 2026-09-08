import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { classifyHealth, ipInCidr } from "../src/health.js";
import type { HealthInput } from "../src/types.js";

const root = join(dirname(fileURLToPath(import.meta.url)), "../../..");

function loadFixture(name: string): HealthInput {
  return JSON.parse(
    readFileSync(join(root, "configs/examples", name), "utf8"),
  ) as HealthInput;
}

describe("ipInCidr", () => {
  it("matches 100.12.0.0/16", () => {
    expect(ipInCidr("100.12.0.30", "100.12.0.0/16")).toBe(true);
    expect(ipInCidr("100.12.255.1", "100.12.0.0/16")).toBe(true);
    expect(ipInCidr("100.13.0.1", "100.12.0.0/16")).toBe(false);
    expect(ipInCidr("140.82.112.3", "100.12.0.0/16")).toBe(false);
  });
});

describe("classifyHealth", () => {
  it("flags dirty DNS and dual TUN from dirty fixture", () => {
    const input = loadFixture("health-dirty-dns.json");
    const result = classifyHealth(input);
    expect(result.ok).toBe(false);
    const codes = result.warnings.map((w) => w.code);
    expect(codes).toContain("DIRTY_DNS_GITHUB");
    expect(codes).toContain("DUAL_TUN");
    expect(codes).toContain("COMPANY_TUN_PRESENT");
  });

  it("clean fixture has no warnings", () => {
    const input = loadFixture("health-clean.json");
    const result = classifyHealth(input);
    expect(result.ok).toBe(true);
    expect(result.warnings).toEqual([]);
  });

  it("respects configurable dirty CIDR list", () => {
    const input: HealthInput = {
      dnsAnswers: [{ name: "github.com", addrs: ["10.99.1.2"] }],
    };
    const none = classifyHealth(input);
    expect(none.warnings.some((w) => w.code === "DIRTY_DNS_GITHUB")).toBe(false);

    const custom = classifyHealth(input, { dirtyCidrs: ["10.99.0.0/16"] });
    expect(custom.warnings.some((w) => w.code === "DIRTY_DNS_GITHUB")).toBe(true);
  });

  it("detects ssh.github.com dirty answers", () => {
    const result = classifyHealth({
      dnsAnswers: [{ name: "ssh.github.com", addrs: ["100.12.1.1"] }],
    });
    expect(result.warnings.some((w) => w.code === "DIRTY_DNS_GITHUB")).toBe(true);
  });
});
