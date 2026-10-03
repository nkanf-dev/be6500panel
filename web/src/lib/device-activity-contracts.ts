import { Schema } from "effect";

export const DEVICE_ACTIVITY_RANGES = ["24h", "7d", "30m"] as const;
export type DeviceActivityRange = (typeof DEVICE_ACTIVITY_RANGES)[number];
export const DEVICE_ACTIVITY_MAX_POINTS = 288;
/** Default panel query/table bound. The API also admits bounded workspace queries. */
export const DEVICE_ACTIVITY_MAX_DEVICES = 32;
export const DEVICE_ACTIVITY_MAX_QUERY_DEVICES = 64;
export const DEVICE_ACTIVITY_CHART_ROWS = 16;
export const DEVICE_ACTIVITY_SEARCH_LIMIT = 64;
export const deviceActivityRangeLabels: Record<DeviceActivityRange, string> = {
  "24h": "最近 24 小时",
  "7d": "最近 7 天",
  "30m": "最近 30 分钟",
};
export const deviceActivityRangeSeconds: Record<DeviceActivityRange, number> = {
  "24h": 86400,
  "7d": 7 * 86400,
  "30m": 1800,
};
const nonNegative = Schema.Number.pipe(Schema.finite(), Schema.nonNegative());
const count = nonNegative.pipe(Schema.int());
const rfc3339 = Schema.String.pipe(
  Schema.filter(
    (value) =>
      /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:\d{2})$/.test(
        value,
      ) && Number.isFinite(Date.parse(value)),
  ),
);
const mac = Schema.String.pipe(
  Schema.pattern(/^(?:[0-9a-fA-F]{2}:){5}[0-9a-fA-F]{2}$/),
);
export const DeviceActivityPointSchema = Schema.Struct({
  time: rfc3339,
  rxBytes: Schema.NullOr(nonNegative),
  txBytes: Schema.NullOr(nonNegative),
  coverageSeconds: nonNegative,
});
export const DeviceActivityCounterSchema = Schema.Struct({
  address: Schema.String,
  rxBytes: nonNegative,
  txBytes: nonNegative,
});
export const DeviceActivityLinkSchema = Schema.Struct({
  interface: Schema.String,
  protocol: Schema.optional(Schema.String),
  mld: Schema.optional(Schema.Boolean),
  signalDBM: Schema.optional(Schema.Number.pipe(Schema.finite())),
  noiseDBM: Schema.optional(Schema.Number.pipe(Schema.finite())),
  negotiatedRX: Schema.optional(Schema.String),
  negotiatedTX: Schema.optional(Schema.String),
  ageingSeconds: Schema.optional(nonNegative),
});
export const DeviceActivityDeviceSchema = Schema.Struct({
  id: mac,
  name: Schema.String,
  addresses: Schema.Array(Schema.String),
  interface: Schema.String,
  associated: Schema.Boolean,
  lastSeen: rfc3339,
  stale: Schema.Boolean,
  rxBytes: nonNegative,
  txBytes: nonNegative,
  coverageSeconds: nonNegative,
  rawRXBytes: Schema.optional(nonNegative),
  rawTXBytes: Schema.optional(nonNegative),
  onlineSeconds: Schema.optional(nonNegative),
  ageingSeconds: Schema.optional(nonNegative),
  counters: Schema.optional(Schema.Array(DeviceActivityCounterSchema)),
  links: Schema.optional(Schema.Array(DeviceActivityLinkSchema)),
  addressConflicts: Schema.optional(Schema.Array(Schema.String)),
  rxBytesPerSecond: Schema.optional(nonNegative),
  txBytesPerSecond: Schema.optional(nonNegative),
  samples: Schema.Array(DeviceActivityPointSchema).pipe(
    Schema.maxItems(DEVICE_ACTIVITY_MAX_POINTS),
  ),
});
export const DeviceActivityGroupSchema = Schema.Struct({
  name: Schema.String,
  deviceCount: count,
  rxBytes: nonNegative,
  txBytes: nonNegative,
  coverageSeconds: Schema.optional(nonNegative),
  rxBytesPerSecond: Schema.optional(nonNegative),
  txBytesPerSecond: Schema.optional(nonNegative),
});
/** Vendor RX/TX directions are unverified. Bytes are counter deltas, not request counts. */
export const DeviceActivityHistorySchema = Schema.Struct({
  enabled: Schema.Boolean,
  persistent: Schema.Literal(false),
  retentionDays: Schema.Literal(7),
  source: Schema.Literal("trafficd"),
  direction: Schema.Literal("vendor-rx-tx"),
  range: Schema.Literal(...DEVICE_ACTIVITY_RANGES),
  resolutionSeconds: nonNegative,
  state: Schema.Literal("waiting", "ok", "stale", "unavailable"),
  sampledAt: Schema.optional(rfc3339),
  oldestAt: Schema.optional(rfc3339),
  error: Schema.optional(Schema.String),
  deviceCount: count,
  matchedCount: count,
  truncated: Schema.Boolean,
  devices: Schema.Array(DeviceActivityDeviceSchema).pipe(
    Schema.maxItems(DEVICE_ACTIVITY_MAX_QUERY_DEVICES),
  ),
  groups: Schema.Array(DeviceActivityGroupSchema),
});
export type DeviceActivityCounter = typeof DeviceActivityCounterSchema.Type;
export type DeviceActivityLink = typeof DeviceActivityLinkSchema.Type;
export type DeviceActivityPoint = typeof DeviceActivityPointSchema.Type;
export type DeviceActivityDevice = typeof DeviceActivityDeviceSchema.Type;
export type DeviceActivityGroup = typeof DeviceActivityGroupSchema.Type;
export type DeviceActivityHistory = typeof DeviceActivityHistorySchema.Type;
