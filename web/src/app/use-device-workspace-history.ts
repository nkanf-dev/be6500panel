import { useCallback, useEffect, useMemo, useState } from "react";
import { request, runRequest } from "../lib/api";
import {
  DeviceActivityHistorySchema,
  type DeviceActivityHistory,
  type DeviceActivityRange,
} from "../lib/device-activity-contracts";
import { canonicalMAC, mergeDeviceHistories } from "../modules/devices";

export function workspaceHistoryRequest(
  range: DeviceActivityRange,
  mac?: string,
) {
  const query = new URLSearchParams({
    range,
    maxPoints: "288",
    limit: mac ? "1" : "64",
    search: mac ?? "",
  });
  return request(`/devices/activity?${query}`, DeviceActivityHistorySchema);
}
interface HistoryState {
  key: string;
  base?: DeviceActivityHistory;
  details: readonly DeviceActivityHistory[];
  loading: boolean;
  error?: unknown;
}
/** Fetch the shared collector, not trafficd. MAC selections are bounded and never writes. */
export function useDeviceWorkspaceHistory(
  range: DeviceActivityRange,
  selected: readonly string[],
) {
  const macs = useMemo(
    () =>
      [
        ...new Set(
          selected.map(canonicalMAC).filter((mac): mac is string => !!mac),
        ),
      ]
        .slice(0, 8)
        .sort(),
    [selected],
  );
  const key = JSON.stringify([range, macs]);
  const [state, setState] = useState<HistoryState>({
    key,
    details: [],
    loading: true,
  });
  const [revision, setRevision] = useState(0);
  const refresh = useCallback(() => setRevision((value) => value + 1), []);
  useEffect(() => {
    const controller = new AbortController();
    let active = true;
    let inFlight = false;
    const load = async () => {
      if (!active || inFlight || document.hidden) return;
      inFlight = true;
      setState((previous) => ({
        ...(previous.key === key ? previous : { key, details: [] }),
        loading: true,
      }));
      const read = async (mac?: string) => {
        const data = await runRequest(
          workspaceHistoryRequest(range, mac),
          controller.signal,
        );
        if (data.range !== range)
          throw new Error("设备历史时间范围不一致，请刷新数据");
        if (
          mac &&
          data.devices.some((device) => canonicalMAC(device.id) !== mac)
        )
          throw new Error("设备历史 MAC 不一致，请刷新数据");
        return data;
      };
      try {
        // Every detail query has limit=1. Compare cannot fan out beyond eight MACs.
        const [base, ...details] = await Promise.allSettled([
          read(),
          ...macs.map((mac) => read(mac)),
        ]);
        if (active)
          setState((previous) => ({
            key,
            base:
              base.status === "fulfilled"
                ? base.value
                : previous.key === key
                  ? previous.base
                  : undefined,
            details: details.flatMap((detail) =>
              detail.status === "fulfilled" ? [detail.value] : [],
            ),
            loading: false,
            error: [base, ...details].find(
              (result) => result.status === "rejected",
            )?.reason,
          }));
      } catch (error) {
        if (active)
          setState((previous) => ({
            ...(previous.key === key ? previous : { key, details: [] }),
            error,
            loading: false,
          }));
      } finally {
        inFlight = false;
      }
    };
    void load();
    const timer = window.setInterval(() => void load(), 30_000);
    const visible = () => {
      if (!document.hidden) void load();
    };
    document.addEventListener("visibilitychange", visible);
    return () => {
      active = false;
      controller.abort();
      window.clearInterval(timer);
      document.removeEventListener("visibilitychange", visible);
    };
  }, [key, range, revision]);
  const current =
    state.key === key ? state : { key, details: [], loading: true };
  return {
    data: mergeDeviceHistories(current.base, current.details, range),
    error: current.error,
    loading: current.loading,
    refresh,
  };
}
