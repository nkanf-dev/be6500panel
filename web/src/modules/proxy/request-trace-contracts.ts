import { Schema } from "effect";

const nonnegative = Schema.Number.pipe(Schema.finite(), Schema.nonNegative());
const text = (max: number) => Schema.String.pipe(Schema.maxLength(max));
const timestamp = text(40).pipe(
  Schema.filter(
    (value) =>
      /^\d{4}-\d\d-\d\dT/.test(value) && Number.isFinite(Date.parse(value)),
  ),
);
export const RequestTraceTargetIdSchema = Schema.Literal(
  "google204",
  "cloudflare",
);
export const RequestTraceRouteSchema = Schema.Literal("direct", "proxy");
export const RequestTraceOutcomeSchema = Schema.Literal(
  "success",
  "http_error",
  "failed",
  "cancelled",
  "timeout",
);
export const RequestTracePhaseIdSchema = Schema.Literal(
  "dns",
  "tcp",
  "connect",
  "tls",
  "ttfb",
  "transfer",
);
export const RequestTracePhaseSchema = Schema.Struct({
  id: RequestTracePhaseIdSchema,
  observed: Schema.Boolean,
  startMs: Schema.NullOr(nonnegative),
  endMs: Schema.NullOr(nonnegative),
  durationMs: Schema.NullOr(nonnegative),
  reason: Schema.optional(text(512)),
}).pipe(
  Schema.filter((phase) => {
    if (!phase.observed)
      return (
        phase.startMs === null &&
        phase.endMs === null &&
        phase.durationMs === null
      );
    if (phase.startMs === null)
      return phase.endMs === null && phase.durationMs === null;
    if (phase.endMs === null) return phase.durationMs === null;
    return phase.endMs >= phase.startMs;
  }),
);
export const RequestTraceSchema = Schema.Struct({
  id: text(128),
  targetId: text(64),
  targetLabel: text(128),
  url: text(2048),
  route: RequestTraceRouteSchema,
  startedAt: timestamp,
  finishedAt: timestamp,
  totalMs: nonnegative,
  outcome: RequestTraceOutcomeSchema,
  statusCode: Schema.NullOr(Schema.Int.pipe(Schema.between(100, 599))),
  bytesRead: Schema.Int.pipe(Schema.between(0, 65536)),
  bodyLimitReached: Schema.Boolean,
  peerAddress: Schema.NullOr(text(256)),
  peerScope: Schema.Literal("origin", "proxy"),
  failurePhase: Schema.NullOr(text(64)),
  errorCode: Schema.optional(text(128)),
  phases: Schema.Array(RequestTracePhaseSchema).pipe(Schema.maxItems(6)),
}).pipe(
  Schema.filter(
    (trace) =>
      Date.parse(trace.finishedAt) >= Date.parse(trace.startedAt) &&
      new Set(trace.phases.map((phase) => phase.id)).size ===
        trace.phases.length,
  ),
);
export const RequestTraceTargetSchema = Schema.Struct({
  id: RequestTraceTargetIdSchema,
  label: text(128),
  url: text(2048),
});
export const RequestTraceHistorySchema = Schema.Struct({
  traces: Schema.Array(RequestTraceSchema).pipe(Schema.maxItems(64)),
  targets: Schema.Array(RequestTraceTargetSchema).pipe(Schema.maxItems(2)),
  limits: Schema.Struct({
    timeoutMs: Schema.Literal(10000),
    bodyBytes: Schema.Literal(65536),
    concurrency: Schema.Literal(1),
    capacity: Schema.Literal(64),
  }),
  running: Schema.Boolean,
});
export const RequestTraceRunInputSchema = Schema.Struct({
  targetId: RequestTraceTargetIdSchema,
  route: RequestTraceRouteSchema,
});
export type RequestTrace = typeof RequestTraceSchema.Type;
export type RequestTracePhase = typeof RequestTracePhaseSchema.Type;
export type RequestTraceHistory = typeof RequestTraceHistorySchema.Type;
export type RequestTraceRunInput = typeof RequestTraceRunInputSchema.Type;
export type RequestTraceTargetId = typeof RequestTraceTargetIdSchema.Type;
export type RequestTraceRoute = typeof RequestTraceRouteSchema.Type;
export type RequestTraceOutcome = typeof RequestTraceOutcomeSchema.Type;
export type RequestTracePhaseId = typeof RequestTracePhaseIdSchema.Type;
export const REQUEST_TRACE_PHASES: readonly RequestTracePhaseId[] = [
  "dns",
  "tcp",
  "connect",
  "tls",
  "ttfb",
  "transfer",
];
