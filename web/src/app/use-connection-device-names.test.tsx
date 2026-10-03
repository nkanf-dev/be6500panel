import { renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { ReactNode } from "react";
import { useConnectionDeviceNames } from "./use-connection-device-names";
import { DeviceLabelsProvider } from "../modules/devices";
import { routerSnapshot } from "../modules/production-fixtures.test-data";
import type { ProxyMetrics } from "../modules/proxy/telemetry-api";
const state = vi.hoisted(() => ({
  router: undefined as unknown,
  routerError: undefined as unknown,
}));
vi.mock("./console-context", () => ({ useOptionalConsole: () => state }));
afterEach(() => {
  state.router = undefined;
  state.routerError = undefined;
});
const capability = { available: true, reason: "measured" };
function metrics(): ProxyMetrics {
  return {
    state: "ready",
    reason: "measured",
    source: "actual controller",
    sampledAt: new Date().toISOString(),
    capabilities: {
      connections: capability,
      traffic: capability,
      routing: capability,
      latency: capability,
      requestPhases: capability,
    },
    totals: { uploadBytes: 0, downloadBytes: 0 },
    activeConnections: 1,
    truncated: false,
    traffic: [],
    probes: [],
    connections: [
      {
        id: "measured-connection",
        startedAt: new Date().toISOString(),
        ageMs: 1,
        network: "tcp",
        sourceIP: "192.0.2.20",
        sourcePort: 1234,
        destinationIP: "203.0.113.1",
        destinationPort: 443,
        host: "example.test",
        uploadBytes: 1,
        downloadBytes: 1,
        outbound: "proxy",
        ruleId: "",
        rule: "",
      },
    ],
  };
}
const wrapper = ({ children }: { children: ReactNode }) => (
  <DeviceLabelsProvider
    initial={{
      revision: 1,
      devices: {
        "02:00:00:00:00:20": { label: "客厅电视", note: "", tags: [] },
      },
    }}
  >
    {children}
  </DeviceLabelsProvider>
);
describe("shared proxy device aliases", () => {
  it("uses saved annotation before hostname only for fresh current MAC-owned addresses", () => {
    state.router = {
      ...routerSnapshot,
      sampledAt: new Date().toISOString(),
      devices: [
        {
          ...routerSnapshot.devices[0],
          mac: "02:00:00:00:00:20",
          ip: "192.0.2.20",
          hostname: "system-name",
          online: true,
        },
      ],
    };
    const { result } = renderHook(() => useConnectionDeviceNames(metrics()), {
      wrapper,
    });
    expect(result.current.get("measured-connection")).toBe("客厅电视");
  });
  it("does not attribute stale, conflicting, failed or absent current addresses", () => {
    const current = metrics();
    state.router = {
      ...routerSnapshot,
      sampledAt: new Date(Date.now() - 60_000).toISOString(),
      devices: [
        {
          ...routerSnapshot.devices[0],
          mac: "02:00:00:00:00:20",
          ip: "192.0.2.20",
          online: true,
        },
      ],
    };
    const { result, rerender } = renderHook(
      () => useConnectionDeviceNames(current),
      { wrapper },
    );
    expect(result.current.size).toBe(0);
    state.router = {
      ...routerSnapshot,
      sampledAt: new Date().toISOString(),
      devices: [
        {
          ...routerSnapshot.devices[0],
          mac: "02:00:00:00:00:20",
          ip: "192.0.2.20",
          online: true,
        },
        {
          ...routerSnapshot.devices[0],
          mac: "02:00:00:00:00:21",
          ip: "192.0.2.20",
          online: true,
        },
      ],
    };
    rerender();
    expect(result.current.size).toBe(0);
    state.routerError = new Error("failed current observation");
    rerender();
    expect(result.current.size).toBe(0);
    state.router = undefined;
    state.routerError = undefined;
    rerender();
    expect(result.current.size).toBe(0);
  });
});
