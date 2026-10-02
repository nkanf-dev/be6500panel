import { useCallback, useEffect, useState } from "react";
import { loadTrafficHistory } from "../../lib/traffic-history-api";
import type {
  TrafficHistory,
  TrafficHistoryRange,
} from "../../lib/traffic-history-contracts";

export const TRAFFIC_HISTORY_REFRESH_MS = 30_000;
interface State {
  range: TrafficHistoryRange;
  data?: TrafficHistory;
  error?: unknown;
  loading: boolean;
}
export function useTrafficHistory(range: TrafficHistoryRange, active = true) {
  const [state, setState] = useState<State>({ range, loading: active });
  const [revision, setRevision] = useState(0);
  const reload = useCallback(() => setRevision((value) => value + 1), []);
  useEffect(() => {
    if (!active) return;
    let disposed = false;
    let inFlight = false;
    let controller: AbortController | undefined;
    const load = async () => {
      if (disposed || inFlight || document.hidden) return;
      inFlight = true;
      controller = new AbortController();
      setState((previous) => ({
        range,
        data: previous.range === range ? previous.data : undefined,
        loading: true,
      }));
      try {
        const data = await loadTrafficHistory(range, controller.signal);
        if (!disposed) setState({ range, data, loading: false });
      } catch (error) {
        if (!disposed)
          setState((previous) => ({
            ...previous,
            range,
            error,
            loading: false,
          }));
      } finally {
        inFlight = false;
      }
    };
    void load();
    const timer = window.setInterval(() => {
      void load();
    }, TRAFFIC_HISTORY_REFRESH_MS);
    const visible = () => {
      if (!document.hidden) void load();
    };
    document.addEventListener("visibilitychange", visible);
    return () => {
      disposed = true;
      controller?.abort();
      window.clearInterval(timer);
      document.removeEventListener("visibilitychange", visible);
    };
  }, [range, active, revision]);
  // Never flash the previous range (even before effect cleanup), or treat it as exportable current data.
  return {
    ...(active && state.range === range ? state : { range, loading: active }),
    reload,
  };
}
