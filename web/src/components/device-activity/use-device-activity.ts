import { useCallback, useEffect, useState } from "react";
import {
  loadDeviceActivity,
  normalizeDeviceActivitySearch,
} from "../../lib/device-activity-api";
import type {
  DeviceActivityHistory,
  DeviceActivityRange,
} from "../../lib/device-activity-contracts";

export const DEVICE_ACTIVITY_REFRESH_MS = 30_000;
interface State {
  key: string;
  data?: DeviceActivityHistory;
  error?: unknown;
  loading: boolean;
}
export function useDeviceActivity(
  range: DeviceActivityRange,
  search = "",
  active = true,
) {
  const normalizedSearch = normalizeDeviceActivitySearch(search);
  const key = JSON.stringify([range, normalizedSearch]);
  const [state, setState] = useState<State>({
    key,
    loading: active && !document.hidden,
  });
  const [revision, setRevision] = useState(0);
  const reload = useCallback(() => setRevision((value) => value + 1), []);
  useEffect(() => {
    if (!active) return;
    let disposed = false;
    let inFlight = false;
    let requestId = 0;
    let controller: AbortController | undefined;
    const load = async () => {
      if (disposed || inFlight || document.hidden) return;
      inFlight = true;
      const id = ++requestId;
      const currentController = new AbortController();
      controller = currentController;
      setState((previous) => ({
        ...(previous.key === key ? previous : { key }),
        loading:
          previous.key !== key ||
          (previous.data === undefined && previous.error === undefined),
      }));
      try {
        const data = await loadDeviceActivity(
          range,
          normalizedSearch,
          currentController.signal,
        );
        if (!disposed && requestId === id)
          setState({ key, data, loading: false });
      } catch (error) {
        if (!disposed && requestId === id)
          setState((previous) => ({
            key,
            data: previous.key === key ? previous.data : undefined,
            error,
            loading: false,
          }));
      } finally {
        if (requestId === id) inFlight = false;
      }
    };
    const visible = () => {
      if (document.hidden) {
        ++requestId;
        controller?.abort();
        inFlight = false;
        setState((previous) => ({ ...previous, loading: false }));
      } else void load();
    };
    if (document.hidden)
      setState((previous) => ({
        ...(previous.key === key ? previous : { key }),
        loading: false,
      }));
    void load();
    const timer = window.setInterval(() => {
      void load();
    }, DEVICE_ACTIVITY_REFRESH_MS);
    document.addEventListener("visibilitychange", visible);
    return () => {
      disposed = true;
      controller?.abort();
      window.clearInterval(timer);
      document.removeEventListener("visibilitychange", visible);
    };
  }, [range, normalizedSearch, key, active, revision]);
  return {
    ...(active && state.key === key
      ? state
      : { key, loading: active && !document.hidden }),
    reload,
  };
}
