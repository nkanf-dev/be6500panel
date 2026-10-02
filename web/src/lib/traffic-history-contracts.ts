import { Schema } from "effect";

export const TRAFFIC_HISTORY_RANGES = [
  "30m",
  "3h",
  "6h",
  "1d",
  "7d",
  "30d",
  "180d",
  "1y",
] as const;
export type TrafficHistoryRange = (typeof TRAFFIC_HISTORY_RANGES)[number];
export const TRAFFIC_HISTORY_MAX_POINTS = 1500;
export const trafficHistoryRangeLabels: Record<TrafficHistoryRange, string> = {
  "30m": "最近 30 分钟",
  "3h": "最近 3 小时",
  "6h": "最近 6 小时",
  "1d": "最近 1 天",
  "7d": "最近 7 天",
  "30d": "最近 30 天",
  "180d": "最近 180 天",
  "1y": "最近 1 年",
};
export const trafficHistoryRangeSeconds: Record<TrafficHistoryRange, number> = {
  "30m": 1800,
  "3h": 10800,
  "6h": 21600,
  "1d": 86400,
  "7d": 7 * 86400,
  "30d": 30 * 86400,
  "180d": 180 * 86400,
  "1y": 365 * 86400,
};
const nonNegative = Schema.Number.pipe(Schema.finite(), Schema.nonNegative());
const rfc3339 = Schema.String.pipe(
  Schema.filter(
    (value) =>
      /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:\d{2})$/.test(
        value,
      ) && Number.isFinite(Date.parse(value)),
  ),
);
export const TrafficHistorySampleSchema = Schema.Struct({
  time: rfc3339,
  rx: nonNegative,
  tx: nonNegative,
  rxPeak: nonNegative,
  txPeak: nonNegative,
  rxBytes: nonNegative,
  txBytes: nonNegative,
  coverageSeconds: nonNegative,
});
/** Exact server history contract. Rates are bytes/s; totals are counter deltas. */
export const TrafficHistorySchema = Schema.Struct({
  enabled: Schema.Boolean,
  persistent: Schema.Boolean,
  retentionDays: nonNegative,
  source: Schema.String,
  range: Schema.Literal(...TRAFFIC_HISTORY_RANGES),
  resolutionSeconds: nonNegative,
  samples: Schema.Array(TrafficHistorySampleSchema).pipe(
    Schema.maxItems(TRAFFIC_HISTORY_MAX_POINTS),
  ),
  summary: Schema.Struct({
    rxBytes: nonNegative,
    txBytes: nonNegative,
    coverageSeconds: nonNegative,
  }),
  oldestAt: Schema.optional(rfc3339),
  lastFlushAt: Schema.optional(rfc3339),
  maxUnsyncedSeconds: Schema.optional(nonNegative),
  error: Schema.optional(Schema.String),
});
export type TrafficHistorySample = typeof TrafficHistorySampleSchema.Type;
export type TrafficHistory = typeof TrafficHistorySchema.Type;
