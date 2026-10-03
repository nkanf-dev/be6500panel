import { Schema } from "effect";
import { request, runRequest } from "./api";

const nonNegative = Schema.Number.pipe(Schema.finite(), Schema.nonNegative());
const positiveInteger = Schema.Number.pipe(
  Schema.finite(),
  Schema.int(),
  Schema.positive(),
);
const timestamp = Schema.String.pipe(
  Schema.filter((value) => Number.isFinite(Date.parse(value))),
);

/** Configuration means init-script presence, never a running process or health check. */
export const ServiceObservationSchema = Schema.Struct({
  name: Schema.String,
  instance: Schema.String,
  configured: Schema.Literal("present", "absent", "unknown"),
  registered: Schema.Literal("registered", "unregistered", "unknown"),
  processState: Schema.Literal("running", "not_running", "failed", "unknown"),
  procdRunning: Schema.optional(Schema.Boolean),
  reportedPID: Schema.optional(positiveInteger),
  pid: Schema.optional(positiveInteger),
  executable: Schema.optional(Schema.String),
  uptimeSeconds: Schema.optional(nonNegative),
  rssBytes: Schema.optional(nonNegative),
  startTicks: Schema.optional(nonNegative.pipe(Schema.int())),
  errorCode: Schema.optional(Schema.String),
  protected: Schema.Boolean,
  actions: Schema.optional(Schema.Array(Schema.String)),
  actionImpact: Schema.optional(Schema.String),
});

/** sampledAt is the last successful observation; checkedAt is the last backend attempt. */
export const ServiceStatusSchema = Schema.Struct({
  source: Schema.String,
  sampledAt: Schema.NullOr(timestamp),
  checkedAt: timestamp,
  stale: Schema.Boolean,
  errorCode: Schema.optional(Schema.String),
  errors: Schema.Array(
    Schema.Struct({
      module: Schema.String,
      code: Schema.String,
      message: Schema.String,
    }),
  ),
  services: Schema.Array(ServiceObservationSchema),
});
export type ServiceObservation = typeof ServiceObservationSchema.Type;
export type ServiceStatusSnapshot = typeof ServiceStatusSchema.Type;

export function requestServiceStatus() {
  return request("/system/services", ServiceStatusSchema);
}
export function loadServiceStatus(signal?: AbortSignal) {
  return runRequest(requestServiceStatus(), signal);
}

export const ServiceActionSchema = Schema.Literal(
  "start",
  "stop",
  "restart",
  "reload",
);
export type ServiceAction = typeof ServiceActionSchema.Type;
export type ServiceActionInput = {
  service: string;
  action: ServiceAction;
  confirmImpact: boolean;
};
export const ServiceActionResultSchema = Schema.Struct({
  service: Schema.String,
  action: ServiceActionSchema,
  commandAccepted: Schema.Boolean,
  errorCode: Schema.optional(Schema.String),
  snapshot: ServiceStatusSchema,
});
export type ServiceActionResult = typeof ServiceActionResultSchema.Type;

/** A command acceptance is not process health. The result contains observed state. */
export function requestServiceAction(body: ServiceActionInput) {
  return request("/system/services/action", ServiceActionResultSchema, {
    method: "POST",
    timeoutMs: 20_000,
    body,
  });
}
export function runServiceAction(
  body: ServiceActionInput,
  signal?: AbortSignal,
) {
  return runRequest(requestServiceAction(body), signal);
}
