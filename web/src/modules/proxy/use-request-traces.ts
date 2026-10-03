import { useCallback, useEffect, useRef, useState } from "react";
import { runRequest } from "../../lib/api";
import { requestTraceApi } from "./request-trace-api";
import type {
  RequestTraceHistory,
  RequestTraceRunInput,
} from "./request-trace-contracts";

/** Read stored history. Active diagnostics require a separate explicit run action. */
export function useRequestTraces() {
  const [data, setData] = useState<RequestTraceHistory>();
  const [readError, setReadError] = useState<unknown>();
  const [runError, setRunError] = useState<unknown>();
  const [loading, setLoading] = useState(true);
  const [ownRun, setOwnRun] = useState(false);
  const [revision, setRevision] = useState(0);
  const mounted = useRef(false);
  const runInFlight = useRef(false);
  const history = useRef<RequestTraceHistory | undefined>(undefined);
  const refresh = useCallback(() => setRevision((value) => value + 1), []);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);
  useEffect(() => {
    const controller = new AbortController();
    let current = true;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const load = async () => {
      setLoading(true);
      try {
        const value = await runRequest(
          requestTraceApi.history(),
          controller.signal,
        );
        if (current) {
          history.current = value;
          setData(value);
          setReadError(undefined);
          // A GET may observe a run accepted by another mounted panel. Only
          // history reads are repeated; this never schedules a diagnostic POST.
          if (value.running) timer = setTimeout(() => void load(), 1500);
        }
      } catch (cause) {
        if (current) setReadError(cause);
      } finally {
        if (current) setLoading(false);
      }
    };
    void load();
    return () => {
      current = false;
      controller.abort();
      clearTimeout(timer);
    };
  }, [revision]);
  const run = useCallback(
    async (input: RequestTraceRunInput) => {
      if (runInFlight.current || !history.current || history.current.running)
        return;
      runInFlight.current = true;
      setOwnRun(true);
      setRunError(undefined);
      try {
        // No consumer-owned AbortSignal: leaving this view must not cancel a run
        // accepted by the service. The backend retains its final trace in its ring.
        const trace = await runRequest(requestTraceApi.run(input));
        if (mounted.current) {
          const previous = history.current;
          if (previous) {
            const value = {
              ...previous,
              running: false,
              traces: [
                trace,
                ...previous.traces.filter((item) => item.id !== trace.id),
              ].slice(0, previous.limits.capacity),
            };
            history.current = value;
            setData(value);
          }
        }
      } catch (cause) {
        if (mounted.current) setRunError(cause);
      } finally {
        runInFlight.current = false;
        if (mounted.current) {
          setOwnRun(false);
          refresh();
        }
      }
    },
    [refresh],
  );
  return {
    data,
    error: runError ?? readError,
    loading,
    running: ownRun || data?.running === true,
    refresh,
    run,
  };
}
