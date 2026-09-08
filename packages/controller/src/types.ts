export type Mode = "office" | "abroad" | "manual";

export interface SingBoxConfig {
  log?: Record<string, unknown>;
  dns?: Record<string, unknown>;
  inbounds?: Inbound[];
  outbounds?: Outbound[];
  route?: RouteConfig;
  experimental?: Record<string, unknown>;
  [key: string]: unknown;
}

export interface Inbound {
  type: string;
  tag?: string;
  strict_route?: boolean;
  sniff?: boolean;
  [key: string]: unknown;
}

export interface Outbound {
  type: string;
  tag?: string;
  outbounds?: string[];
  default?: string;
  [key: string]: unknown;
}

export interface RouteRule {
  protocol?: string | string[];
  action?: string;
  outbound?: string;
  domain_suffix?: string[];
  process_name?: string[];
  geosite?: string[];
  inbound?: string;
  [key: string]: unknown;
}

export interface RouteConfig {
  rules?: RouteRule[];
  final?: string;
  auto_detect_interface?: boolean;
  [key: string]: unknown;
}

export interface PatchMeta {
  mode: Mode;
  applied: boolean;
  notes?: string[];
  warnDualTun?: boolean;
  description?: string;
}

export interface ApplyPatchResult {
  config: SingBoxConfig;
  meta: PatchMeta;
}

export interface NetInterface {
  name: string;
  inet?: string[];
}

export interface DnsAnswer {
  name: string;
  addrs: string[];
}

export interface HealthInput {
  interfaces?: NetInterface[];
  dnsAnswers?: DnsAnswer[];
  companyTunHints?: boolean;
}

export type HealthWarningCode =
  | "DUAL_TUN"
  | "DIRTY_DNS_GITHUB"
  | "COMPANY_TUN_PRESENT";

export interface HealthWarning {
  code: HealthWarningCode;
  message: string;
  detail?: unknown;
}

export interface HealthResult {
  ok: boolean;
  warnings: HealthWarning[];
}

export interface DirtyDnsOptions {
  /** CIDR list treated as dirty for GitHub/SSH answers. Default: 100.12.0.0/16 */
  dirtyCidrs?: string[];
  githubNameHints?: string[];
}
