export interface ValidationError {
  path: string;
  message: string;
}

export interface ValidationResult {
  ok: boolean;
  errors: ValidationError[];
}

function isPlainObject(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function push(errors: ValidationError[], path: string, message: string): void {
  errors.push({ path, message });
}

/**
 * Structural validation for sing-box-ish JSON.
 * Hard requirement: root must be a non-null object.
 * Soft checks: route / outbounds / dns shapes when present.
 */
export function validateConfig(input: unknown): ValidationResult {
  const errors: ValidationError[] = [];

  if (input === null || typeof input !== "object" || Array.isArray(input)) {
    push(errors, "$", "Config must be a JSON object");
    return { ok: false, errors };
  }

  const cfg = input as Record<string, unknown>;

  // Soft: outbounds
  if ("outbounds" in cfg) {
    if (!Array.isArray(cfg.outbounds)) {
      push(errors, "outbounds", "outbounds must be an array when present");
    } else {
      cfg.outbounds.forEach((item, i) => {
        if (!isPlainObject(item)) {
          push(errors, `outbounds[${i}]`, "each outbound must be an object");
          return;
        }
        if (typeof item.type !== "string" || !item.type) {
          push(errors, `outbounds[${i}].type`, "outbound.type must be a non-empty string");
        }
        if ("tag" in item && typeof item.tag !== "string") {
          push(errors, `outbounds[${i}].tag`, "outbound.tag must be a string when present");
        }
        if ("outbounds" in item && !Array.isArray(item.outbounds)) {
          push(
            errors,
            `outbounds[${i}].outbounds`,
            "selector/urltest outbounds must be an array of tags",
          );
        }
      });
    }
  }

  // Soft: route
  if ("route" in cfg) {
    if (!isPlainObject(cfg.route)) {
      push(errors, "route", "route must be an object when present");
    } else {
      const route = cfg.route;
      if ("rules" in route) {
        if (!Array.isArray(route.rules)) {
          push(errors, "route.rules", "route.rules must be an array when present");
        } else {
          route.rules.forEach((rule, i) => {
            if (!isPlainObject(rule)) {
              push(errors, `route.rules[${i}]`, "each route rule must be an object");
            }
          });
        }
      }
      if ("final" in route && typeof route.final !== "string") {
        push(errors, "route.final", "route.final must be a string (outbound tag) when present");
      }
    }
  }

  // Soft: dns
  if ("dns" in cfg) {
    if (!isPlainObject(cfg.dns)) {
      push(errors, "dns", "dns must be an object when present");
    } else {
      const dns = cfg.dns;
      if ("servers" in dns && !Array.isArray(dns.servers)) {
        push(errors, "dns.servers", "dns.servers must be an array when present");
      } else if (Array.isArray(dns.servers)) {
        dns.servers.forEach((server, i) => {
          if (!isPlainObject(server)) {
            push(errors, `dns.servers[${i}]`, "each dns server must be an object");
          }
        });
      }
      if ("rules" in dns && !Array.isArray(dns.rules)) {
        push(errors, "dns.rules", "dns.rules must be an array when present");
      }
      if ("final" in dns && typeof dns.final !== "string") {
        push(errors, "dns.final", "dns.final must be a string when present");
      }
    }
  }

  // Soft: inbounds
  if ("inbounds" in cfg) {
    if (!Array.isArray(cfg.inbounds)) {
      push(errors, "inbounds", "inbounds must be an array when present");
    } else {
      cfg.inbounds.forEach((item, i) => {
        if (!isPlainObject(item)) {
          push(errors, `inbounds[${i}]`, "each inbound must be an object");
          return;
        }
        if (typeof item.type !== "string" || !item.type) {
          push(errors, `inbounds[${i}].type`, "inbound.type must be a non-empty string");
        }
      });
    }
  }

  return { ok: errors.length === 0, errors };
}

/**
 * Parse JSON text then validate. Parse errors become path "$".
 */
export function validateConfigText(text: string): ValidationResult {
  let parsed: unknown;
  try {
    parsed = JSON.parse(text);
  } catch (err: unknown) {
    const message = err instanceof Error ? err.message : String(err);
    return { ok: false, errors: [{ path: "$", message: `Invalid JSON: ${message}` }] };
  }
  return validateConfig(parsed);
}
