import { Data, Effect, Either, Schedule, Schema } from "effect";
import {
  RouterSchema,
  RuntimeSchema,
  RuntimeStatusSchema,
  RuntimeConfigSchema,
  ProxyNodesSchema,
  ProxySelectSchema,
  ProxyCaptureSchema,
  type RuntimeService,
  type RuntimeAcquireInput,
  type RuntimeConfigureInput,
  type ProxyImportInput,
  type ProxySelectInput,
  type ProxyCaptureInput,
  DevicesSchema,
  FrpcSchema,
  HealthSchema,
  LogsSchema,
  ModulesSchema,
  NetworkSchema,
  PlanSchema,
  SessionSchema,
  SystemSchema,
  type FrpcPlanInput,
  type ProxyPlanInput,
} from "./contracts";

export class ApiError extends Data.TaggedError("ApiError")<{
  readonly code: string;
  readonly message: string;
  readonly status?: number;
}> {}
const failure = (cause: unknown) =>
  cause instanceof ApiError
    ? cause
    : new ApiError({
        code: "network_error",
        message: cause instanceof Error ? cause.message : "无法连接控制服务",
      });

/** All HTTP decoding, cancellation, failures and bounded read retry live at this boundary. */
export function request<A, I>(
  path: string,
  schema: Schema.Schema<A, I>,
  options: {
    method?: "GET" | "POST" | "DELETE";
    body?: unknown;
    timeoutMs?: number;
  } = {},
): Effect.Effect<A, ApiError> {
  const method = options.method ?? "GET";
  const response = Effect.tryPromise({
    try: async (signal) => {
      const timeout = AbortSignal.timeout(options.timeoutMs ?? 10_000);
      const res = await fetch(`/api${path}`, {
        method,
        credentials: "same-origin",
        signal: AbortSignal.any([signal, timeout]),
        headers: {
          Accept: "application/json",
          ...(options.body !== undefined
            ? { "Content-Type": "application/json" }
            : {}),
        },
        ...(options.body !== undefined
          ? { body: JSON.stringify(options.body) }
          : {}),
      });
      let json: unknown;
      try {
        json = await res.json();
      } catch {
        throw new ApiError({
          code: "invalid_response",
          status: res.status,
          message: "服务返回了非 JSON 响应",
        });
      }
      if (!res.ok) {
        if (res.status === 401)
          window.dispatchEvent(new Event("be6500panel:unauthorized"));
        const envelope = (
          typeof json === "object" && json !== null ? json : {}
        ) as {
          error?: { code?: unknown; message?: unknown };
        };
        throw new ApiError({
          status: res.status,
          code:
            typeof envelope.error?.code === "string"
              ? envelope.error.code
              : "http_error",
          message:
            typeof envelope.error?.message === "string"
              ? envelope.error.message
              : `请求失败（HTTP ${res.status}）`,
        });
      }
      return json;
    },
    catch: failure,
  });
  const decoded = response.pipe(
    Effect.flatMap((json) =>
      Schema.decodeUnknown(schema)(json).pipe(
        Effect.mapError(
          () =>
            new ApiError({
              code: "invalid_response",
              message: "响应与 API 合同不符，请检查服务版本与诊断日志",
            }),
        ),
      ),
    ),
  );
  return method === "GET"
    ? decoded.pipe(
        Effect.retry({
          schedule: Schedule.exponential("250 millis").pipe(
            Schedule.intersect(Schedule.recurs(2)),
          ),
          while: (error) =>
            error.code === "network_error" ||
            (error.status !== undefined &&
              error.status >= 500 &&
              error.code !== "observation_unavailable"),
        }),
      )
    : decoded;
}

export async function runRequest<A>(
  effect: Effect.Effect<A, ApiError>,
  signal?: AbortSignal,
): Promise<A> {
  const result = await Effect.runPromise(Effect.either(effect), { signal });
  if (Either.isLeft(result)) throw result.left;
  return result.right;
}
export const api = {
  session: () => request("/session", SessionSchema),
  login: (password: string) =>
    request("/session/login", SessionSchema, {
      method: "POST",
      body: { password },
    }),
  logout: () => request("/session/logout", SessionSchema, { method: "POST" }),
  health: () => request("/health", HealthSchema),
  modules: () => request("/modules", ModulesSchema),
  system: () => request("/system", SystemSchema),
  router: () => request("/router", RouterSchema),
  runtime: () => request("/runtime", RuntimeSchema),
  runtimeAcquire: (body: RuntimeAcquireInput) =>
    request("/runtime/acquire", RuntimeStatusSchema, {
      method: "POST",
      body,
      timeoutMs: 420_000,
    }),
  runtimeConfigure: (body: RuntimeConfigureInput) =>
    request("/runtime/configure", RuntimeStatusSchema, {
      method: "POST",
      body,
      timeoutMs: 90_000,
    }),
  runtimeConfig: (service: RuntimeService) =>
    request(
      `/runtime/config?service=${encodeURIComponent(service)}`,
      RuntimeConfigSchema,
    ),
  runtimeStart: (service: RuntimeService) =>
    request("/runtime/start", RuntimeStatusSchema, {
      method: "POST",
      timeoutMs: 90_000,
      body: { service },
    }),
  runtimeStop: (service: RuntimeService) =>
    request("/runtime/stop", RuntimeStatusSchema, {
      method: "POST",
      timeoutMs: 30_000,
      body: { service },
    }),
  proxyNodes: () => request("/proxy/nodes", ProxyNodesSchema),
  proxyImport: (body: ProxyImportInput) =>
    request("/proxy/import", ProxyNodesSchema, {
      method: "POST",
      body,
      timeoutMs: 60_000,
    }),
  proxySelect: (body: ProxySelectInput) =>
    request("/proxy/select", ProxySelectSchema, {
      method: "POST",
      body,
      timeoutMs: 90_000,
    }),
  proxyCapture: () => request("/proxy/capture", ProxyCaptureSchema),
  proxyCaptureApply: (body: ProxyCaptureInput) =>
    request("/proxy/capture", ProxyCaptureSchema, {
      method: "POST",
      body,
      timeoutMs: 90_000,
    }),
  proxyCaptureDisable: () =>
    request("/proxy/capture", ProxyCaptureSchema, {
      method: "DELETE",
      timeoutMs: 30_000,
    }),
  network: () => request("/network", NetworkSchema),
  devices: () => request("/devices", DevicesSchema),
  logs: () => request("/logs?limit=100", LogsSchema),
  frpc: () => request("/frpc", FrpcSchema),
  proxyPlan: (body: ProxyPlanInput) =>
    request("/proxy/plan", PlanSchema, { method: "POST", body }),
  frpcPlan: (body: FrpcPlanInput) =>
    request("/frpc/plan", PlanSchema, { method: "POST", body }),
};
export const errorMessage = (error: unknown) =>
  error instanceof ApiError
    ? `${error.message} · ${error.code}`
    : error instanceof Error
      ? error.message
      : "未知错误";
