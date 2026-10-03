import { Effect, Schema } from "effect";
import { ApiError, request } from "../../lib/api";
import {
  RequestTraceHistorySchema,
  RequestTraceRunInputSchema,
  RequestTraceSchema,
  type RequestTraceRunInput,
} from "./request-trace-contracts";

export const requestTraceApi = {
  history: () => request("/proxy/request-traces", RequestTraceHistorySchema),
  run: (input: RequestTraceRunInput) =>
    Schema.decodeUnknown(RequestTraceRunInputSchema)(input).pipe(
      Effect.mapError(
        () =>
          new ApiError({
            code: "invalid_request",
            message: "诊断仅支持固定目标和直连或当前代理链路",
          }),
      ),
      Effect.flatMap(({ targetId, route }) =>
        request("/proxy/request-traces", RequestTraceSchema, {
          method: "POST",
          body: { targetId, route },
          // The backend stops at 10 seconds. Leave time to return the final trace.
          timeoutMs: 12000,
        }),
      ),
    ),
};
