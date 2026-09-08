import { readFileSync } from "node:fs";
import { applyModePatch } from "./patch.js";
import type { ApplyPatchResult, Mode, SingBoxConfig } from "./types.js";

export function loadJsonConfig(filePath: string): SingBoxConfig {
  const raw = readFileSync(filePath, "utf8");
  return JSON.parse(raw) as SingBoxConfig;
}

/**
 * Load baseline JSON and apply mode patch. Returns a new merged object;
 * never writes back to the baseline file.
 */
export function mergeBaselineWithMode(
  baseline: SingBoxConfig,
  mode: Mode,
): ApplyPatchResult {
  return applyModePatch(baseline, mode);
}

export function mergeFromFiles(
  baselinePath: string,
  mode: Mode,
): ApplyPatchResult {
  const baseline = loadJsonConfig(baselinePath);
  return mergeBaselineWithMode(baseline, mode);
}
