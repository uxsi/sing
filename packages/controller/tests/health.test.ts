import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import {
  classifyHealth,
  defaultHealthRules,
  ipInCidr,
  loadHealthRules,
} from "../src/health.js";
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

describe("loadHealthRules", () => {
  it("returns defaults when path omitted or missing", () => {
    const d = defaultHealthRules();
    expect(loadHealthRules()).toEqual(d);
    expect(loadHealthRules("/no/such/health-rules.json")).toEqual(d);
    expect(d.dirtyCidrs).toContain("100.12.0.0/16");
    expect(d.companyTunNamePatterns).toEqual([]);
  });

  it("loads custom rules from JSON example", () => {
    const path = join(root, "configs/examples/health-rules.json");
    const rules = loadHealthRules(path);
    expect(rules.dirtyCidrs).toContain("10.99.0.0/16");
    expect(rules.companyTunNamePatterns).toContain("corp-tun");
    expect(rules.githubNameHints).toContain("ssh.github.com");
  });

  it("merges partial JSON over defaults", () => {
    const dir = mkdtempSync(join(tmpdir(), "sing-health-"));
    const path = join(dir, "rules.json");
    writeFileSync(path, JSON.stringify({ dirtyCidrs: ["192.168.50.0/24"] }));
    const rules = loadHealthRules(path);
    expect(rules.dirtyCidrs).toEqual(["192.168.50.0/24"]);
    expect(rules.githubNameHints).toEqual(defaultHealthRules().githubNameHints);
    expect(rules.companyTunNamePatterns).toEqual([]);
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

  it("uses custom github name hints", () => {
    const input: HealthInput = {
      dnsAnswers: [{ name: "git.example.internal", addrs: ["100.12.0.5"] }],
    };
    expect(
      classifyHealth(input).warnings.some((w) => w.code === "DIRTY_DNS_GITHUB"),
    ).toBe(false);
    expect(
      classifyHealth(input, {
        githubNameHints: ["git.example.internal"],
      }).warnings.some((w) => w.code === "DIRTY_DNS_GITHUB"),
    ).toBe(true);
  });

  it("flags 公司隧道 via configurable iface name patterns", () => {
    const input: HealthInput = {
      interfaces: [{ name: "utun9-corp-tun", inet: ["10.8.0.2/24"] }],
    };
    expect(
      classifyHealth(input).warnings.some((w) => w.code === "COMPANY_TUN_PRESENT"),
    ).toBe(false);
    const result = classifyHealth(input, {
      companyTunNamePatterns: ["corp-tun"],
    });
    expect(result.warnings.some((w) => w.code === "COMPANY_TUN_PRESENT")).toBe(
      true,
    );
  });

  it("applies loaded rules file to classifyHealth", () => {
    const rules = loadHealthRules(
      join(root, "configs/examples/health-rules.json"),
    );
    const result = classifyHealth(
      {
        interfaces: [{ name: "en0-company-tun" }],
        dnsAnswers: [{ name: "github.com", addrs: ["10.99.1.9"] }],
      },
      rules,
    );
    const codes = result.warnings.map((w) => w.code);
    expect(codes).toContain("COMPANY_TUN_PRESENT");
    expect(codes).toContain("DIRTY_DNS_GITHUB");
  });
});
