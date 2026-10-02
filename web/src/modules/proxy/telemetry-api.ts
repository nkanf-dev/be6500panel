import { Schema } from "effect";
import { request } from "../../lib/api";

const nonnegative = Schema.Number.pipe(Schema.finite(), Schema.nonNegative());
const port = Schema.Int.pipe(Schema.between(0, 65535));
const activeCount = Schema.Int.pipe(Schema.between(0, 4096));
const label = (max: number) => Schema.String.pipe(Schema.maxLength(max));
const timestamp = Schema.String.pipe(
  Schema.maxLength(40),
  Schema.filter(
    (value) =>
      /^\d{4}-\d\d-\d\dT/.test(value) && Number.isFinite(Date.parse(value)),
  ),
);
const bounded = <A, I>(item: Schema.Schema<A, I>, max: number) =>
  Schema.Array(item).pipe(Schema.maxItems(max));
const CapabilitySchema = Schema.Struct({
  available: Schema.Boolean,
  reason: label(256),
});
export const ProxyMetricsSchema = Schema.Struct({
  state: Schema.Literal("ready", "stale", "unavailable"),
  reason: label(256),
  source: label(100),
  sampledAt: Schema.optional(timestamp),
  capabilities: Schema.Struct({
    connections: CapabilitySchema,
    traffic: CapabilitySchema,
    routing: CapabilitySchema,
    latency: CapabilitySchema,
    requestPhases: CapabilitySchema,
  }),
  totals: Schema.Struct({
    uploadBytes: nonnegative,
    downloadBytes: nonnegative,
  }),
  activeConnections: activeCount,
  truncated: Schema.Boolean,
  connections: bounded(
    Schema.Struct({
      id: label(24),
      startedAt: timestamp,
      ageMs: nonnegative,
      network: Schema.Literal("tcp", "udp", "unknown"),
      sourceIP: label(45),
      sourcePort: port,
      destinationIP: label(45),
      destinationPort: port,
      host: label(253),
      uploadBytes: nonnegative,
      downloadBytes: nonnegative,
      outbound: Schema.Literal("proxy", "direct", "unavailable"),
      ruleId: label(24),
      rule: label(160),
    }),
    128,
  ),
  traffic: bounded(
    Schema.Struct({
      time: timestamp,
      uploadRate: nonnegative,
      downloadRate: nonnegative,
      reset: Schema.Boolean,
    }),
    900,
  ),
  probes: bounded(
    Schema.Struct({
      time: timestamp,
      delayMs: nonnegative,
      status: Schema.Literal("ok", "failed"),
    }),
    32,
  ),
});
export type ProxyMetrics = typeof ProxyMetricsSchema.Type;
export const proxyTelemetry = {
  metrics: () => request("/proxy/metrics", ProxyMetricsSchema),
  probe: () =>
    request("/proxy/probe", ProxyMetricsSchema, {
      method: "POST",
      body: {},
      timeoutMs: 8000,
    }),
};
