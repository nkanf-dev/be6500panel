import { Effect } from "effect";
import { ApiError, request, runRequest } from "./api";
import {
  DeviceActivityHistorySchema,
  DEVICE_ACTIVITY_MAX_POINTS,
  DEVICE_ACTIVITY_MAX_DEVICES,
  DEVICE_ACTIVITY_SEARCH_LIMIT,
  type DeviceActivityRange,
} from "./device-activity-contracts";

export function normalizeDeviceActivitySearch(search: string): string {
  return Array.from(search.trim())
    .slice(0, DEVICE_ACTIVITY_SEARCH_LIMIT)
    .join("");
}
/** Read the collector's retained memory. Refresh never starts a network probe or a write. */
export function requestDeviceActivity(range: DeviceActivityRange, search = "") {
  const query = new URLSearchParams({
    range,
    maxPoints: String(DEVICE_ACTIVITY_MAX_POINTS),
    limit: String(DEVICE_ACTIVITY_MAX_DEVICES),
    search: normalizeDeviceActivitySearch(search),
  });
  return request(
    `/devices/activity?${query}`,
    DeviceActivityHistorySchema,
  ).pipe(
    Effect.flatMap((history) =>
      history.range === range
        ? Effect.succeed(history)
        : Effect.fail(
            new ApiError({
              code: "invalid_response",
              message: "设备活跃数据返回了不同的时间范围",
            }),
          ),
    ),
  );
}
export function loadDeviceActivity(
  range: DeviceActivityRange,
  search = "",
  signal?: AbortSignal,
) {
  return runRequest(requestDeviceActivity(range, search), signal);
}
