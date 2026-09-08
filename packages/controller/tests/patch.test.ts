import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { applyModePatch, findInsertIndex, revertToBaseline } from "../src/patch.js";
import type { RouteRule, SingBoxConfig } from "../src/types.js";

const root = join(dirname(fileURLToPath(import.meta.url)), "../../..");
const baselinePath = join(root, "configs/examples/baseline.json");

function loadBaseline(): SingBoxConfig {
  return JSON.parse(readFileSync(baselinePath, "utf8")) as SingBoxConfig;
}

describe("applyModePatch", () => {
  it("manual returns unchanged clone and does not mutate baseline", () => {
    const baseline = loadBaseline();
    const snapshot = JSON.stringify(baseline);
    const { config, meta } = applyModePatch(baseline, "manual");
    expect(meta.mode).toBe("manual");
    expect(meta.applied).toBe(false);
    expect(JSON.stringify(config)).toBe(snapshot);
    expect(JSON.stringify(baseline)).toBe(snapshot);
    config.log = { level: "debug" };
    expect(baseline.log).toEqual({ level: "info" });
  });

  it("office inserts github/ssh/git direct rules before geosite-github proxy", () => {
    const baseline = loadBaseline();
    const { config, meta } = applyModePatch(baseline, "office");
    expect(meta.applied).toBe(true);
    expect(meta.mode).toBe("office");

    const rules = config.route?.rules ?? [];
    const githubDirect = rules.findIndex(
      (r) =>
        Array.isArray(r.domain_suffix) &&
        r.domain_suffix.includes("github.com") &&
        r.outbound === "direct",
    );
    const processDirect = rules.findIndex(
      (r) =>
        Array.isArray(r.process_name) &&
        r.process_name.includes("ssh") &&
        r.process_name.includes("git") &&
        r.outbound === "direct",
    );
    const geositeGithub = rules.findIndex(
      (r) => Array.isArray(r.geosite) && r.geosite.includes("github") && r.outbound === "proxy",
    );

    expect(githubDirect).toBeGreaterThanOrEqual(0);
    expect(processDirect).toBeGreaterThanOrEqual(0);
    expect(geositeGithub).toBeGreaterThan(githubDirect);
    expect(geositeGithub).toBeGreaterThan(processDirect);

    // inserted after sniff / hijack-dns / dns protocol prefix
    const insertAt = findInsertIndex(baseline.route?.rules ?? []);
    expect(githubDirect).toBe(insertAt);
  });

  it("office sets tun strict_route false and biases final selector to direct", () => {
    const baseline = loadBaseline();
    const { config } = applyModePatch(baseline, "office");
    const tun = config.inbounds?.find((i) => i.type === "tun");
    expect(tun?.strict_route).toBe(false);

    const finalSel = config.outbounds?.find((o) => o.tag === "final");
    expect(finalSel?.default).toBe("direct");

    // baseline file semantics: in-memory original untouched
    const origTun = baseline.inbounds?.find((i) => i.type === "tun");
    expect(origTun?.strict_route).toBe(true);
    const origFinal = baseline.outbounds?.find((o) => o.tag === "final");
    expect(origFinal?.default).toBe("proxy");
  });

  it("abroad does not force github direct; sets strict_route true; dual TUN warning flag", () => {
    const baseline = loadBaseline();
    const { config, meta } = applyModePatch(baseline, "abroad");

    expect(meta.mode).toBe("abroad");
    expect(meta.warnDualTun).toBe(true);
    expect(meta.notes?.some((n) => /clean DNS/i.test(n))).toBe(true);

    const rules = config.route?.rules ?? [];
    const forcedGithubDirect = rules.some(
      (r: RouteRule) =>
        Array.isArray(r.domain_suffix) &&
        r.domain_suffix.includes("github.com") &&
        r.outbound === "direct",
    );
    expect(forcedGithubDirect).toBe(false);

    const tun = config.inbounds?.find((i) => i.type === "tun");
    expect(tun?.strict_route).toBe(true);

    // geosite-github to proxy remains as in baseline
    const geositeGithub = rules.find(
      (r) => Array.isArray(r.geosite) && r.geosite.includes("github") && r.outbound === "proxy",
    );
    expect(geositeGithub).toBeTruthy();
  });

  it("revertToBaseline equals manual clone", () => {
    const baseline = loadBaseline();
    const reverted = revertToBaseline(baseline);
    const manual = applyModePatch(baseline, "manual");
    expect(reverted.config).toEqual(manual.config);
    expect(reverted.meta.applied).toBe(false);
  });
});

describe("findInsertIndex", () => {
  it("returns 0 when no sniff/hijack prefix", () => {
    expect(findInsertIndex([{ geosite: ["cn"], outbound: "direct" }])).toBe(0);
  });

  it("skips sniff and hijack-dns prefix", () => {
    const rules: RouteRule[] = [
      { protocol: "dns", outbound: "dns-out" },
      { action: "sniff" },
      { action: "hijack-dns", protocol: "dns" },
      { geosite: ["github"], outbound: "proxy" },
    ];
    expect(findInsertIndex(rules)).toBe(3);
  });
});
