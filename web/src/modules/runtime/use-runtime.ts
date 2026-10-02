import { useCallback, useEffect, useRef, useState } from "react";
import type { Effect } from "effect";
import { useConsole } from "../../app/console-context";
import { api, runRequest, type ApiError } from "../../lib/api";
import type { RuntimeService, RuntimeStatus } from "../../lib/contracts";

export function useRuntime(service: RuntimeService) {
  const { health } = useConsole();
  const [status, setStatus] = useState<RuntimeStatus>();
  const [enabled, setEnabled] = useState(false);
  const [loading, setLoading] = useState(true);
  const mutation = useRef(false);
  const [pending, setPending] = useState(false);
  const [observationError, setObservationError] = useState<unknown>();
  const [error, setError] = useState<unknown>();
  const [result, setResult] = useState<string>();
  const [revision, setRevision] = useState(0);
  const refresh = useCallback(() => setRevision((value) => value + 1), []);
  useEffect(() => {
    const controller = new AbortController();
    let active = true;
    let inFlight = false;
    const load = async () => {
      if (inFlight || mutation.current || document.hidden) return;
      inFlight = true;
      try {
        const response = await runRequest(api.runtime(), controller.signal);
        if (active && !mutation.current) {
          setStatus(response.services.find((item) => item.service === service));
          setEnabled(response.enabled && health?.mode === "host");
          setObservationError(undefined);
        }
      } catch (cause) {
        if (active) {
          setEnabled(false);
          setObservationError(cause);
        }
      } finally {
        inFlight = false;
        if (active) setLoading(false);
      }
    };
    setLoading(true);
    void load();
    const timer = window.setInterval(() => {
      void load();
    }, 3000);
    return () => {
      active = false;
      controller.abort();
      window.clearInterval(timer);
    };
  }, [service, health?.mode, revision]);
  const run = async (
    load: () => Effect.Effect<RuntimeStatus, ApiError>,
    success: string,
  ) => {
    if (!enabled || mutation.current) return false;
    mutation.current = true;
    setPending(true);
    setError(undefined);
    setResult(undefined);
    try {
      setStatus(await runRequest(load()));
      setResult(success);
      return true;
    } catch (cause) {
      setError(cause);
      return false;
    } finally {
      mutation.current = false;
      refresh();
      setPending(false);
    }
  };
  return {
    service,
    status,
    enabled,
    loading,
    pending,
    error: error ?? observationError,
    result,
    refresh,
    run,
  };
}
export type RuntimeController = ReturnType<typeof useRuntime>;
