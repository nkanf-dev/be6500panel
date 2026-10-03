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
  search = "",
) {
  const query = new URLSearchParams({
    range,
    maxPoints: "288",
    limit: mac ? "1" : "64",
    search: mac ?? search,
  });
  return request(`/devices/activity?${query}`, DeviceActivityHistorySchema);
}
interface HistoryState {
  key: string;
  baseKey: string;
  base?: DeviceActivityHistory;
  details: readonly (DeviceActivityHistory | undefined)[];
  loading: boolean;
  error?: unknown;
}
/** Fetch the shared collector, not trafficd. MAC selections are bounded and never writes. */
export function useDeviceWorkspaceHistory(
  range: DeviceActivityRange,
  selected: readonly string[],
  search = "",
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
  const boundedSearch = Array.from(search.trim()).slice(0, 64).join("");
  const baseKey = JSON.stringify([range, boundedSearch]);
  const key = JSON.stringify([baseKey, macs]);
  const [state, setState] = useState<HistoryState>({
    key,
    baseKey,
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
        ...(previous.baseKey === baseKey
          ? previous
          : { key, baseKey, details: [] }),
        key,
        details:
          previous.key === key
            ? previous.details
            : previous.baseKey === baseKey
              ? previous.details.filter((detail) =>
                  detail?.devices.some((device) =>
                    macs.includes(canonicalMAC(device.id) ?? ""),
                  ),
                )
              : [],
        loading:
          previous.baseKey !== baseKey ||
          (!previous.base &&
            !previous.details.some(Boolean) &&
            previous.error === undefined),
      }));
      const read = async (mac?: string) => {
        const data = await runRequest(
          workspaceHistoryRequest(range, mac, boundedSearch),
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
            baseKey,
            base:
              base.status === "fulfilled"
                ? base.value
                : previous.baseKey === baseKey
                  ? previous.base
                  : undefined,
            details: details.map((detail, index) =>
              detail.status === "fulfilled"
                ? detail.value
                : previous.baseKey === baseKey
                  ? previous.details.find((old) =>
                      old?.devices.some(
                        (device) => canonicalMAC(device.id) === macs[index],
                      ),
                    )
                  : undefined,
            ),
            loading: false,
            error: [base, ...details].find(
              (result) => result.status === "rejected",
            )?.reason,
          }));
      } catch (error) {
        if (active)
          setState((previous) => ({
            ...(previous.baseKey === baseKey
              ? previous
              : { key, baseKey, details: [] }),
            key,
            details:
              previous.baseKey === baseKey
                ? previous.details.filter((detail) =>
                    detail?.devices.some((device) =>
                      macs.includes(canonicalMAC(device.id) ?? ""),
                    ),
                  )
                : [],
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
  const current: HistoryState =
    state.key === key
      ? state
      : state.baseKey === baseKey
        ? {
            ...state,
            key,
            details: state.details.filter((detail) =>
              detail?.devices.some((device) =>
                macs.includes(canonicalMAC(device.id) ?? ""),
              ),
            ),
          }
        : { key, baseKey, details: [], loading: true };
  const data = useMemo(() => {
    const merged = mergeDeviceHistories(current.base, current.details, range);
    // A failed source cannot make retained selected-device records look current.
    return merged && current.error !== undefined
      ? {
          ...merged,
          state: "stale" as const,
          devices: merged.devices.map((device) => ({ ...device, stale: true })),
        }
      : merged;
  }, [current.base, current.details, current.error, range]);
  return {
    data,
    error: current.error,
    loading: current.loading,
    refresh,
  };
}
