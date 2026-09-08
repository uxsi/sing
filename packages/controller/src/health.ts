import type {
  DirtyDnsOptions,
  HealthInput,
  HealthResult,
  HealthWarning,
} from "./types.js";

const DEFAULT_DIRTY_CIDRS = ["100.12.0.0/16"];
const DEFAULT_GITHUB_HINTS = ["github.com", "githubusercontent.com", "ssh.github.com"];

function ipv4ToInt(ip: string): number | null {
  const parts = ip.split(".");
  if (parts.length !== 4) return null;
  let n = 0;
  for (const p of parts) {
    const v = Number(p);
    if (!Number.isInteger(v) || v < 0 || v > 255) return null;
    n = (n << 8) + v;
  }
  return n >>> 0;
}

function parseCidr(cidr: string): { base: number; mask: number } | null {
  const [ip, bitsStr] = cidr.split("/");
  if (!ip || bitsStr === undefined) return null;
  const base = ipv4ToInt(ip);
  const bits = Number(bitsStr);
  if (base === null || !Number.isInteger(bits) || bits < 0 || bits > 32) return null;
  const mask = bits === 0 ? 0 : ((0xffffffff << (32 - bits)) >>> 0);
  return { base: (base & mask) >>> 0, mask };
}

export function ipInCidr(ip: string, cidr: string): boolean {
  const parsed = parseCidr(cidr);
  const addr = ipv4ToInt(ip);
  if (!parsed || addr === null) return false;
  return ((addr & parsed.mask) >>> 0) === parsed.base;
}

function looksLikeTun(name: string): boolean {
  const n = name.toLowerCase();
  return (
    n.startsWith("utun") ||
    n.startsWith("tun") ||
    n.startsWith("wintun") ||
    n.includes("sing-tun")
  );
}

function isGithubRelatedName(name: string, hints: string[]): boolean {
  const lower = name.toLowerCase().replace(/\.$/, "");
  return hints.some(
    (h) => lower === h || lower.endsWith("." + h) || lower.includes(h),
  );
}

/**
 * Classify health signals into warning codes:
 * DUAL_TUN, DIRTY_DNS_GITHUB, COMPANY_TUN_PRESENT
 */
export function classifyHealth(
  input: HealthInput,
  options: DirtyDnsOptions = {},
): HealthResult {
  const dirtyCidrs = options.dirtyCidrs ?? DEFAULT_DIRTY_CIDRS;
  const githubHints = options.githubNameHints ?? DEFAULT_GITHUB_HINTS;
  const warnings: HealthWarning[] = [];

  const ifaces = input.interfaces ?? [];
  const tunIfaces = ifaces.filter((i) => looksLikeTun(i.name));

  if (tunIfaces.length >= 2) {
    warnings.push({
      code: "DUAL_TUN",
      message: "Multiple TUN interfaces detected — company tunnel may overlap with sing-box TUN",
      detail: { interfaces: tunIfaces.map((i) => i.name) },
    });
  }

  if (input.companyTunHints) {
    warnings.push({
      code: "COMPANY_TUN_PRESENT",
      message: "Company tunnel / corporate VPN hints present",
    });
  }

  const dirtyHits: { name: string; addr: string; cidr: string }[] = [];
  for (const ans of input.dnsAnswers ?? []) {
    if (!isGithubRelatedName(ans.name, githubHints)) continue;
    for (const addr of ans.addrs) {
      for (const cidr of dirtyCidrs) {
        if (ipInCidr(addr, cidr)) {
          dirtyHits.push({ name: ans.name, addr, cidr });
        }
      }
    }
  }

  if (dirtyHits.length > 0) {
    warnings.push({
      code: "DIRTY_DNS_GITHUB",
      message: "GitHub-related DNS answers fall in dirty/corporate CIDR range",
      detail: { hits: dirtyHits, dirtyCidrs },
    });
  }

  return {
    ok: warnings.length === 0,
    warnings,
  };
}

export { DEFAULT_DIRTY_CIDRS, DEFAULT_GITHUB_HINTS };
