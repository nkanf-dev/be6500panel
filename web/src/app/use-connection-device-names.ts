import { useMemo } from "react";
import { useOptionalConsole } from "./console-context";
import {
  correlateDeviceProxy,
  mergeDeviceInventory,
  useDeviceLabels,
} from "../modules/devices";
import type { ProxyMetrics } from "../modules/proxy/telemetry-api";

/** Attribute only fresh current-IP ownership. Missing/conflicting/stale sources stay unlabelled. */
export function useConnectionDeviceNames(metrics?: ProxyMetrics) {
  const console = useOptionalConsole();
  const router = console?.router;
  const routerError = console?.routerError;
  const labels = useDeviceLabels();
  return useMemo(() => {
    const names = new Map<string, string>();
    const devices = mergeDeviceInventory(
      routerError === undefined ? router : undefined,
    );
    for (const device of devices) {
      const observation = correlateDeviceProxy(device.mac, devices, metrics);
      if (!observation.stale)
        for (const connection of observation.connections)
          names.set(
            connection.id,
            labels.displayName(device.mac, device.hostname),
          );
    }
    return names;
  }, [router, routerError, metrics, labels.displayName]);
}
