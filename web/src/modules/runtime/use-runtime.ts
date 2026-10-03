import { useCallback, useEffect, useRef, useState } from "react";
import type { Effect } from "effect";
import { useConsole } from "../../app/console-context";
import { api, runRequest, type ApiError } from "../../lib/api";
import type { RuntimeService, RuntimeStatus } from "../../lib/contracts";

interface Observation {
  key: string;
  status?: RuntimeStatus;
  enabled: boolean;
  loading: boolean;
  error?: unknown;
}
export function useRuntime(service: RuntimeService) {
  const { health } = useConsole();
  const key = JSON.stringify([service, health?.mode]);
  const [observation, setObservation] = useState<Observation>({
    key,
    enabled: false,
    loading: true,
  });
  const mutation = useRef(false);
  const [pending, setPending] = useState(false);
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
      setObservation((previous) =>
        previous.key === key
          ? previous
          : { key, enabled: false, loading: true },
      );
      try {
        const response = await runRequest(api.runtime(), controller.signal);
        if (active && !mutation.current) {
          setObservation({
            key,
            status: response.services.find((item) => item.service === service),
            enabled: response.enabled && health?.mode === "host",
            loading: false,
          });
        }
      } catch (cause) {
        if (active)
          setObservation((previous) => ({
            ...previous,
            enabled: false,
            error: cause,
          }));
      } finally {
        inFlight = false;
        if (active)
          setObservation((previous) => ({ ...previous, loading: false }));
      }
    };
    void load();
    const timer = window.setInterval(() => {
      void load();
    }, 3000);
    return () => {
      active = false;
      controller.abort();
      window.clearInterval(timer);
    };
  }, [service, health?.mode, key, revision]);
  const current =
    observation.key === key
      ? observation
      : {
          key,
          enabled: false,
          loading: true,
          status: undefined,
          error: undefined,
        };
  const run = async (
    load: () => Effect.Effect<RuntimeStatus, ApiError>,
    success: string,
  ) => {
    if (!current.enabled || mutation.current) return false;
    mutation.current = true;
    setPending(true);
    setError(undefined);
    setResult(undefined);
    try {
      const status = await runRequest(load());
      setObservation((previous) =>
        previous.key === key && status.service === service
          ? { ...previous, status }
          : previous,
      );
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
    status: current.status,
    enabled: current.enabled,
    loading: current.loading,
    pending,
    error: error ?? current.error,
    result,
    refresh,
    run,
  };
}
export type RuntimeController = ReturnType<typeof useRuntime>;
