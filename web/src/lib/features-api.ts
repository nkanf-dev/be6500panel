import { Effect, Schema } from "effect";
import { ApiError, request, runRequest } from "./api";

export const FeatureFieldKindSchema = Schema.Literal(
  "text",
  "secret",
  "boolean",
  "integer",
  "ipv4",
  "ipv6",
  "mac",
  "select",
  "json",
);

export const FeatureImpactSchema = Schema.Literal(
  "local",
  "network",
  "wireless",
  "maintenance",
);

export const FeatureFieldSchema = Schema.Struct({
  key: Schema.String,
  label: Schema.String,
  kind: FeatureFieldKindSchema,
  required: Schema.Boolean,
  options: Schema.optional(Schema.Array(Schema.String)),
  min: Schema.optional(Schema.NullOr(Schema.Number)),
  max: Schema.optional(Schema.NullOr(Schema.Number)),
});

export const FeatureReadSchema = Schema.Struct({
  id: Schema.String,
  title: Schema.String,
  fields: Schema.Array(FeatureFieldSchema),
});

export const FeatureActionSchema = Schema.Struct({
  id: Schema.String,
  title: Schema.String,
  fields: Schema.Array(FeatureFieldSchema),
  impact: FeatureImpactSchema,
  configs: Schema.optional(Schema.Array(Schema.String)),
  readback: Schema.optional(Schema.String),
});

export const FeatureOperationStateSchema = Schema.Literal(
  "pending",
  "completed",
  "failed",
);

export const FeatureOperationSchema = Schema.Struct({
  id: Schema.String,
  state: FeatureOperationStateSchema,
  actionId: Schema.String,
  domain: Schema.String,
  generation: Schema.Number,
  canConfirm: Schema.optional(Schema.Boolean),
  reconnectAddress: Schema.optional(Schema.String),
  waitingFor: Schema.optional(Schema.String),
  progress: Schema.optional(Schema.Number),
  error: Schema.optional(Schema.String),
  updatedAt: Schema.optional(Schema.String),
});

export const FeatureDomainSchema = Schema.Struct({
  id: Schema.String,
  title: Schema.String,
  reads: Schema.Array(FeatureReadSchema),
  actions: Schema.Array(FeatureActionSchema),
});

export const FeatureCatalogSchema = Schema.Struct({
  domains: Schema.Array(FeatureDomainSchema),
  generation: Schema.optional(Schema.Number),
  pendingOperation: Schema.optional(FeatureOperationSchema),
});

export const FeatureStateSchema = Schema.Struct({
  available: Schema.Boolean,
  readId: Schema.String,
  sampledAt: Schema.optional(Schema.String),
  generation: Schema.optional(Schema.Number),
  data: Schema.Record({ key: Schema.String, value: Schema.Unknown }),
  errors: Schema.optional(
    Schema.Array(
      Schema.Struct({
        code: Schema.String,
        message: Schema.String,
      }),
    ),
  ),
});

export const FeatureOperationEnvelopeSchema = Schema.Struct({
  operation: FeatureOperationSchema,
});

export const FeatureApplyResponseSchema = Schema.Struct({
  operation: FeatureOperationSchema,
  data: Schema.optional(
    Schema.Record({ key: Schema.String, value: Schema.Unknown }),
  ),
});

export type FeatureFieldKind = typeof FeatureFieldKindSchema.Type;
export type FeatureImpact = typeof FeatureImpactSchema.Type;
export type FeatureField = typeof FeatureFieldSchema.Type;
export type FeatureRead = typeof FeatureReadSchema.Type;
export type FeatureAction = typeof FeatureActionSchema.Type;
export type FeatureDomain = typeof FeatureDomainSchema.Type;
export type FeatureCatalog = typeof FeatureCatalogSchema.Type;
export type FeatureState = typeof FeatureStateSchema.Type;
export type FeatureOperationState = typeof FeatureOperationStateSchema.Type;
export type FeatureOperation = typeof FeatureOperationSchema.Type;
export type FeatureApplyResponse = typeof FeatureApplyResponseSchema.Type;

export interface FeatureApplyInput {
  actionId: string;
  input: Record<string, unknown>;
  generation?: number;
  acknowledgeImpact?: boolean;
}

export const featuresApi = {
  catalog: () => request("/features/catalog", FeatureCatalogSchema),
  state: (domain: string, readId: string, params: Record<string, string | number | boolean> = {}) => {
    const searchParams = new URLSearchParams();
    searchParams.set("read", readId);
    for (const [key, value] of Object.entries(params)) {
      if (value !== undefined && value !== "") {
        searchParams.set(key, String(value));
      }
    }
    return request(`/features/${domain}/state?${searchParams.toString()}`, FeatureStateSchema);
  },
  apply: (domain: string, payload: FeatureApplyInput) =>
    request(`/features/${domain}/apply`, FeatureApplyResponseSchema, {
      method: "POST",
      body: payload,
    }),
  operation: (id: string) =>
    request(`/features/operations?id=${encodeURIComponent(id)}`, FeatureOperationEnvelopeSchema).pipe(
      Effect.map((res) => res.operation),
    ),
  confirm: (id: string) =>
    request("/features/confirm", FeatureOperationEnvelopeSchema, {
      method: "POST",
      body: { id },
    }).pipe(Effect.map((res) => res.operation)),
};

/** Poll an operation until completed or failed. Safe bounded polling at ~1s interval. */
export function formatReconnectUrl(rawAddress?: string): string {
  if (!rawAddress) return "";
  if (rawAddress.startsWith("http://") || rawAddress.startsWith("https://")) {
    return rawAddress;
  }
  if (typeof window === "undefined") {
    return `http://${rawAddress}`;
  }
  const protocol = window.location.protocol || "http:";
  const port = window.location.port ? `:${window.location.port}` : "";
  return `${protocol}//${rawAddress}${port}`;
}

export async function pollOperation(
  id: string,
  options: {
    timeoutMs?: number;
    intervalMs?: number;
    initialOperation?: FeatureOperation;
    signal?: AbortSignal;
    onProgress?: (op: FeatureOperation) => void;
  } = {},
): Promise<FeatureOperation> {
  const timeoutMs = options.timeoutMs ?? 30_000;
  const intervalMs = options.intervalMs ?? 1_000;
  const start = Date.now();

  let latestOp: FeatureOperation | undefined = options.initialOperation;

  while (Date.now() - start < timeoutMs) {
    if (options.signal?.aborted) {
      throw new ApiError({ code: "aborted", message: "操作已取消" });
    }
    try {
      const op = await runRequest(featuresApi.operation(id), options.signal);
      latestOp = op;
      options.onProgress?.(op);
      if (op.state === "completed" || op.canConfirm) return op;
      if (op.state === "failed") {
        throw new ApiError({
          code: "operation_failed",
          message: op.error || "配置应用未成功",
        });
      }
    } catch (cause) {
      if (options.signal?.aborted) throw cause;
      // Re-throw if error is not an ApiError, or if status is set (401, 403, 500, etc.), or if code != network_error
      if (
        !(cause instanceof ApiError) ||
        cause.code !== "network_error" ||
        cause.status !== undefined
      ) {
        throw cause;
      }
      // Only transient network_error without HTTP status (connection drop during IP switch / reload) is tolerated
      if (latestOp) {
        options.onProgress?.(latestOp);
      }
    }
    await new Promise((resolve) => setTimeout(resolve, intervalMs));
  }
  if (latestOp) return latestOp;
  throw new ApiError({ code: "operation_timeout", message: "配置正在后台处理，请稍后刷新状态核对" });
}
