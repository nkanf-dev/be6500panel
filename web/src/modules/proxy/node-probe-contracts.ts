import { Schema } from "effect";

export const NODE_PROBE_TARGET = "https://www.gstatic.com/generate_204";
export const NODE_PROBE_MAX_NODES = 256;
export const NODE_PROBE_POLL_MS = 1000;

const text = (max: number) => Schema.String.pipe(Schema.maxLength(max));
const nodeId = text(128).pipe(Schema.minLength(1));
const count = Schema.Int.pipe(Schema.between(0, NODE_PROBE_MAX_NODES));
const timestamp = text(40).pipe(
  Schema.filter(
    (value) =>
      /^\d{4}-\d\d-\d\dT/.test(value) && Number.isFinite(Date.parse(value)),
  ),
);

export const NodeProbeJobStatusSchema = Schema.Literal(
  "preparing",
  "running",
  "completed",
  "cancelled",
  "invalidated",
  "failed",
);
export const NodeProbeResultStatusSchema = Schema.Literal(
  "queued",
  "probing",
  "success",
  "timeout",
  "unreachable",
  "cancelled",
);
export const NodeProbeJobSchema = Schema.Struct({
  id: text(128).pipe(Schema.minLength(1)),
  status: NodeProbeJobStatusSchema,
  total: count,
  completed: count,
  startedAt: timestamp,
  finishedAt: Schema.optional(timestamp),
  errorCode: Schema.optional(text(128)),
}).pipe(
  Schema.filter(
    (job) =>
      job.completed <= job.total &&
      (job.finishedAt === undefined ||
        Date.parse(job.finishedAt) >= Date.parse(job.startedAt)),
  ),
);
export const NodeProbeResultSchema = Schema.Struct({
  nodeId,
  status: NodeProbeResultStatusSchema,
  delayMs: Schema.optional(
    Schema.Number.pipe(Schema.finite(), Schema.nonNegative()),
  ),
  measuredAt: Schema.optional(timestamp),
  target: Schema.Literal(NODE_PROBE_TARGET),
  errorCode: Schema.optional(text(128)),
}).pipe(
  Schema.filter((result) =>
    result.status === "success"
      ? result.delayMs !== undefined && result.measuredAt !== undefined
      : result.delayMs === undefined,
  ),
);
export const NodeProbeSnapshotSchema = Schema.Struct({
  revision: text(128),
  available: Schema.Boolean,
  unavailableCode: Schema.optional(text(128)),
  target: Schema.Literal(NODE_PROBE_TARGET),
  running: Schema.Boolean,
  job: Schema.optional(NodeProbeJobSchema),
  results: Schema.Array(NodeProbeResultSchema).pipe(
    Schema.maxItems(NODE_PROBE_MAX_NODES),
  ),
  limits: Schema.Struct({
    maxNodes: Schema.Literal(NODE_PROBE_MAX_NODES),
    concurrency: Schema.Literal(1),
    timeoutMs: Schema.Literal(3000),
  }),
}).pipe(
  Schema.filter(
    (snapshot) =>
      new Set(snapshot.results.map((result) => result.nodeId)).size ===
      snapshot.results.length,
  ),
);
export const NodeProbeRunInputSchema = Schema.Struct({
  all: Schema.Boolean,
  nodeIds: Schema.Array(nodeId).pipe(Schema.maxItems(NODE_PROBE_MAX_NODES)),
  revision: text(128).pipe(Schema.minLength(1)),
}).pipe(
  Schema.filter(
    (input) =>
      (input.all ? input.nodeIds.length === 0 : input.nodeIds.length > 0) &&
      new Set(input.nodeIds).size === input.nodeIds.length,
  ),
);

export type NodeProbeJob = typeof NodeProbeJobSchema.Type;
export type NodeProbeResult = typeof NodeProbeResultSchema.Type;
export type NodeProbeSnapshot = typeof NodeProbeSnapshotSchema.Type;
export type NodeProbeRunInput = typeof NodeProbeRunInputSchema.Type;
