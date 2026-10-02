import { Effect } from "effect";
import { ApiError, request, runRequest } from "./api";
import {
  TrafficHistorySchema,
  TRAFFIC_HISTORY_MAX_POINTS,
  type TrafficHistoryRange,
} from "./traffic-history-contracts";

/** The server chooses/downsamples resolution; the browser never slices the selected range. */
export function requestTrafficHistory(range: TrafficHistoryRange) {
  return request(
    `/traffic/history?range=${encodeURIComponent(range)}&maxPoints=${TRAFFIC_HISTORY_MAX_POINTS}`,
    TrafficHistorySchema,
  ).pipe(
    Effect.flatMap((history) =>
      history.range === range
        ? Effect.succeed(history)
        : Effect.fail(
            new ApiError({
              code: "invalid_response",
              message: "流量历史返回了不同的时间范围",
            }),
          ),
    ),
  );
}
export function loadTrafficHistory(
  range: TrafficHistoryRange,
  signal?: AbortSignal,
) {
  return runRequest(requestTrafficHistory(range), signal);
}
