import { useCallback, useEffect, useRef, useState } from "react";
import { runRequest } from "../../lib/api";
import { proxyTelemetry, type ProxyMetrics } from "./telemetry-api";

/** Poll the panel's cache, not the core. Never run active probes on a timer. */
export function useProxyTelemetry() {
  const [data, setData] = useState<ProxyMetrics>();
  const [error, setError] = useState<unknown>();
  const [probeError, setProbeError] = useState<unknown>();
  const [loading, setLoading] = useState(true);
  const [probing, setProbing] = useState(false);
  const [revision, setRevision] = useState(0);
  const active = useRef(false);
  const probeController = useRef<AbortController | undefined>(undefined);
  const refresh = useCallback(() => setRevision((value) => value + 1), []);
  useEffect(() => {
    active.current = true;
    return () => {
      active.current = false;
      probeController.current?.abort();
    };
  }, []);
  useEffect(() => {
    const controller = new AbortController();
    let timer: ReturnType<typeof setTimeout>;
    let current = true;
    const load = async () => {
      try {
        const value = await runRequest(
          proxyTelemetry.metrics(),
          controller.signal,
        );
        if (current) {
          setData(value);
          setError(undefined);
        }
      } catch (cause) {
        if (current) setError(cause);
      } finally {
        if (current) {
          setLoading(false);
          timer = setTimeout(() => void load(), 4000);
        }
      }
    };
    // Refreshing compatible observations is not an initial page load.
    void load();
    return () => {
      current = false;
      controller.abort();
      clearTimeout(timer);
    };
  }, [revision]);
  const probe = useCallback(async () => {
    if (probeController.current) return;
    const controller = new AbortController();
    probeController.current = controller;
    setProbing(true);
    try {
      const value = await runRequest(proxyTelemetry.probe(), controller.signal);
      if (active.current) {
        setData(value);
        setProbeError(undefined);
        refresh();
      }
    } catch (cause) {
      if (active.current) {
        setProbeError(cause);
        refresh();
      }
    } finally {
      if (probeController.current === controller)
        probeController.current = undefined;
      if (active.current) setProbing(false);
    }
  }, [refresh]);
  return { data, error: probeError ?? error, loading, refresh, probe, probing };
}
