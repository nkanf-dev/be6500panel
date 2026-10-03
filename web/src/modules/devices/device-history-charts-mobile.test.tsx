import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { afterEach, describe, it, expect, vi } from "vitest";
import { DeviceHistoryCharts } from "./device-history-charts";
import { DeviceLabelsProvider } from "./device-labels";
import type { WorkspaceDevice } from "./device-model";
vi.mock("../../components/visualizations/EChart", () => ({
  EChart: ({ label }: { label: string }) => (
    <div role="img" aria-label={label} />
  ),
}));
afterEach(() => vi.unstubAllGlobals());
const device: WorkspaceDevice = {
  mac: "02:00:00:00:00:31",
  hostname: "fixture-device",
  addresses: [],
  currentAddresses: [],
  leases: [],
  activity: {
    id: "02:00:00:00:00:31",
    name: "fixture-device",
    addresses: [],
    interface: "fixture",
    associated: true,
    lastSeen: "2026-10-03T00:00:00Z",
    stale: false,
    rxBytes: 600,
    txBytes: 300,
    coverageSeconds: 60,
    samples: [
      {
        time: "2026-10-03T00:00:00Z",
        rxBytes: 600,
        txBytes: 300,
        coverageSeconds: 60,
      },
    ],
  },
};
const mount = () =>
  render(
    <DeviceLabelsProvider initial={{ revision: 0, devices: {} }}>
      <DeviceHistoryCharts
        devices={[device]}
        resolutionSeconds={60}
        source="trafficd"
      />
    </DeviceLabelsProvider>,
  );
function viewport(compact: boolean) {
  vi.stubGlobal(
    "matchMedia",
    vi.fn(() => ({
      matches: compact,
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
    })),
  );
}
describe("device mobile chart disclosure", () => {
  it("leaves heavy charts collapsed on compact screens but shows real summary data", async () => {
    viewport(true);
    mount();
    expect(screen.queryByRole("img")).not.toBeInTheDocument();
    expect(
      screen.getByText("所选设备 · 同一时间范围数据汇总"),
    ).toBeInTheDocument();
    const summary = screen.getByText("设备流量趋势").closest("summary")!;
    const details = summary.closest("details")!;
    fireEvent.click(summary);
    await waitFor(() => expect(details.open).toBe(true));
    await waitFor(() =>
      expect(
        screen.getByRole("img", { name: /实测速率曲线/ }),
      ).toBeInTheDocument(),
    );
    expect(
      screen.queryByRole("img", { name: /实际流量速率热力图/ }),
    ).not.toBeInTheDocument();
  });
  it("keeps desktop charts expanded and accessible", () => {
    viewport(false);
    mount();
    expect(screen.getAllByRole("img")).toHaveLength(2);
    const disclosures = document.querySelectorAll<HTMLDetailsElement>(
      ".device-chart-disclosure",
    );
    expect([...disclosures].every((d) => d.open)).toBe(true);
  });
});
