import type {
  ApplyPatchResult,
  Mode,
  Outbound,
  RouteRule,
  SingBoxConfig,
} from "./types.js";

function deepClone<T>(value: T): T {
  return structuredClone(value);
}

function isSniffOrHijackDns(rule: RouteRule): boolean {
  if (rule.action === "sniff" || rule.action === "hijack-dns") return true;
  const proto = rule.protocol;
  if (proto === "dns" || (Array.isArray(proto) && proto.includes("dns"))) {
    if (rule.outbound === "dns-out" || rule.action === "hijack-dns") return true;
  }
  return false;
}

/** Find index after trailing sniff/hijack-dns prefix rules. */
export function findInsertIndex(rules: RouteRule[]): number {
  let i = 0;
  while (i < rules.length && isSniffOrHijackDns(rules[i]!)) i += 1;
  // Also skip leading dns-out protocol rules that sit before sniff
  // Re-scan from start: keep all leading sniff/hijack/dns protocol rules contiguous
  i = 0;
  while (i < rules.length) {
    const r = rules[i]!;
    if (isSniffOrHijackDns(r)) {
      i += 1;
      continue;
    }
    if (r.protocol === "dns" || (Array.isArray(r.protocol) && r.protocol.includes("dns"))) {
      i += 1;
      continue;
    }
    break;
  }
  return i;
}

function officeProtectRules(): RouteRule[] {
  return [
    {
      domain_suffix: ["github.com", "githubusercontent.com"],
      outbound: "direct",
    },
    {
      process_name: ["ssh", "git"],
      outbound: "direct",
    },
  ];
}

function setTunStrictRoute(config: SingBoxConfig, value: boolean): void {
  if (!Array.isArray(config.inbounds)) return;
  for (const inbound of config.inbounds) {
    if (inbound.type === "tun" && "strict_route" in inbound) {
      inbound.strict_route = value;
    } else if (inbound.type === "tun") {
      inbound.strict_route = value;
    }
  }
}

function biasFinalSelector(config: SingBoxConfig, preferred: string): void {
  if (!Array.isArray(config.outbounds)) return;
  const finalSel = config.outbounds.find(
    (o: Outbound) => o.type === "selector" && o.tag === "final",
  );
  if (!finalSel) return;
  if (Array.isArray(finalSel.outbounds) && finalSel.outbounds.includes(preferred)) {
    finalSel.default = preferred;
  }
}

/**
 * Apply a mode patch. Never mutates the input baseline; returns a new object.
 * manual → baseline unchanged (clone).
 */
export function applyModePatch(
  baseline: SingBoxConfig,
  mode: Mode,
): ApplyPatchResult {
  const config = deepClone(baseline);

  if (mode === "manual") {
    return {
      config,
      meta: {
        mode: "manual",
        applied: false,
        description: "Manual mode: baseline unchanged",
      },
    };
  }

  if (mode === "office") {
    if (!config.route) config.route = {};
    if (!Array.isArray(config.route.rules)) config.route.rules = [];
    const rules = config.route.rules;
    const idx = findInsertIndex(rules);
    rules.splice(idx, 0, ...officeProtectRules());

    setTunStrictRoute(config, false);
    biasFinalSelector(config, "direct");
    // Prefer local DNS in office so a dead proxy node does not break direct sites.
    if (config.dns && Array.isArray(config.dns.servers)) {
      const hasLocal = config.dns.servers.some(
        (s: { tag?: string }) => s && s.tag === "dns-local",
      );
      if (hasLocal) {
        config.dns.final = "dns-local";
      }
    }

    return {
      config,
      meta: {
        mode: "office",
        applied: true,
        description:
          "Office: GitHub/ssh/git direct before geosite-github proxy; strict_route false; final bias direct",
        notes: [
          "Protect GitHub and git/ssh process traffic via direct outbound",
          "Prefer coexistence with company tunnel (strict_route false)",
        ],
      },
    };
  }

  // abroad
  setTunStrictRoute(config, true);
  return {
    config,
    meta: {
      mode: "abroad",
      applied: true,
      warnDualTun: true,
      description:
        "Abroad: prefer clean DNS and exclusive TUN; strict_route true; do not force GitHub direct",
      notes: [
        "Do not insert GitHub direct rules — rely on clean DNS and proxy path",
        "Prefer strict_route true when TUN inbound exists",
        "Warn if dual TUN / company tunnel still present before connect",
      ],
    },
  };
}

/** Semantic revert: re-apply from original baseline with manual (or any mode). */
export function revertToBaseline(baseline: SingBoxConfig): ApplyPatchResult {
  return applyModePatch(baseline, "manual");
}
