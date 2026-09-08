export type * from "./types.js";
export { applyModePatch, revertToBaseline, findInsertIndex } from "./patch.js";
export {
  classifyHealth,
  ipInCidr,
  DEFAULT_DIRTY_CIDRS,
  DEFAULT_GITHUB_HINTS,
} from "./health.js";
export {
  loadJsonConfig,
  mergeBaselineWithMode,
  mergeFromFiles,
} from "./merge.js";
export { detectSingBoxBinary, spawnSingBox } from "./core.js";
export {
  fetchSubscription,
  fetchSubscriptionBody,
  type SubscribeOptions,
  type SubscribeResult,
  type SubscribeSuccess,
  type SubscribeFailure,
} from "./subscribe.js";
export {
  validateConfig,
  validateConfigText,
  type ValidationError,
  type ValidationResult,
} from "./validate.js";
export {
  DEFAULT_CLASH_API_BASE,
  getConfigs,
  getProxies,
  getConnections,
  delayTest,
  type ClashApiOptions,
  type ClashApiResult,
  type ClashApiError,
  type ClashApiSuccess,
  type DelayTestData,
} from "./clashApi.js";
