import {
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { EChartsOption } from "echarts";
import { DeviceHistoryCharts } from "./device-history-charts";
import type { DeviceActivitySample, WorkspaceDevice } from "./device-model";
import type { ChartPalette } from "../../components/visualizations/chart-theme";
import { activityFixture } from "../../components/device-activity/activity-fixture.test-data";

const captured = vi.hoisted(
  () => new Map<string, (palette: ChartPalette) => EChartsOption>(),
);
vi.mock("../../components/visualizations/EChart", () => ({
  EChart: ({
    option,
    label,
  }: {
    option: (palette: ChartPalette) => EChartsOption;
    label: string;
  }) => {
    captured.set(label, option);
    return <div role="img" aria-label={label} />;
  },
}));
beforeEach(() => captured.clear());
interface OptionView {
  yAxis: { name: string; axisLabel: { formatter: (value: number) => string } };
  visualMap: {
    min: number;
    max: number;
    text: string[];
    formatter: (value: number) => string;
  };
  series: {
    name: string;
    id: string;
    data: (number[] | [string, number | null])[];
    connectNulls: boolean;
  }[];
  tooltip: { formatter: (parameters: unknown) => string };
}
const current = (heat = false) =>
  [...captured.values()][heat ? 1 : 0](
    {} as ChartPalette,
  ) as unknown as OptionView;
function workspace(
  samples?: readonly DeviceActivitySample[],
  mac = "02:00:00:00:00:01",
): WorkspaceDevice {
  const activity = activityFixture().devices[0];
  return {
    mac,
    hostname: "同名设备",
    addresses: [],
    currentAddresses: [],
    leases: [],
    activity: { ...activity, id: mac, samples: samples ?? activity.samples },
  };
}
const renderHistory = (devices: readonly WorkspaceDevice[]) =>
  render(
    <DeviceHistoryCharts
      devices={devices}
      resolutionSeconds={3600}
      source="trafficd"
    />,
  );

describe("device history adaptive byte units", () => {
  it("keeps measured fractions and zeros in B/s, omits missing cells, and does not change coverage seconds", async () => {
    const device = workspace();
    renderHistory([device]);
    const line = current();
    expect(line.yAxis.name).toBe("B/s");
    expect(line.yAxis.axisLabel.formatter(1.234567)).toBe("1.23");
    expect(line.series[0].data).toEqual([
      [device.activity!.samples[0].time, 1024 / 3600],
      [device.activity!.samples[1].time, null],
      [device.activity!.samples[2].time, 0],
    ]);
    expect(line.series[1].data[0]).toEqual([
      device.activity!.samples[0].time,
      2048 / 3600,
    ]);
    expect(line.series.every((series) => !series.connectNulls)).toBe(true);
    expect(current(true).series[0].data).toEqual([
      [0, 0, 3072 / 3600],
      [2, 0, 0],
    ]);
    expect(current(true).visualMap.text).toEqual(["0.85 B/s", "0 B/s"]);
    const details = screen.getAllByText(/查看数据表/)[0].closest("details")!;
    details.open = true;
    fireEvent(details, new Event("toggle"));
    await waitFor(() =>
      expect(within(details).getByRole("table")).toBeTruthy(),
    );
    const table = within(details).getByRole("table");
    expect(within(table).getByText("1.02 KB")).toBeTruthy();
    expect(within(table).getByText("2.05 KB")).toBeTruthy();
    expect(within(table).getByText("0.28 B/s")).toBeTruthy();
    expect(within(table).getByText("0.57 B/s")).toBeTruthy();
    expect(within(table).getAllByText("—")).toHaveLength(4);
    expect(within(table).getByText("3600")).toBeTruthy();
    expect(within(table).getByText("30")).toBeTruthy();
    expect(device.activity!.samples[0].rxBytes).toBe(1024);
  });
  it.each([
    [0, "B/s", 0],
    [0.25, "B/s", 0.25],
    [999, "B/s", 999],
    [1000, "KB/s", 1],
    [1023, "KB/s", 1.023],
    [1e6, "MB/s", 1],
    [1e9, "GB/s", 1],
    [1e12, "TB/s", 1],
  ])(
    "scales actual %s rates with decimal boundaries",
    (rate, unit, plotted) => {
      renderHistory([
        workspace([
          {
            time: "2026-10-03T00:00:00Z",
            rxBytes: rate as number,
            txBytes: 0,
            coverageSeconds: 1,
          },
        ]),
      ]);
      expect(current().yAxis.name).toBe(unit);
      expect(current().series[0].data).toEqual([
        ["2026-10-03T00:00:00Z", plotted],
      ]);
      expect(current(true).series[0].name).toBe(`RX + TX ${unit}`);
      expect(current(true).series[0].data).toEqual([[0, 0, plotted]]);
    },
  );
  it("uses one unit across multiple devices/directions, keeps tiny measurements, and retains canonical series IDs", () => {
    const time = "2026-10-03T00:00:00Z";
    const large = workspace([
      { time, rxBytes: 2e9, txBytes: 0, coverageSeconds: 1 },
    ]);
    const small = workspace(
      [{ time, rxBytes: 1, txBytes: 0.25, coverageSeconds: 1 }],
      "02:00:00:00:00:02",
    );
    renderHistory([large, small]);
    expect(current().yAxis.name).toBe("GB/s");
    expect(current().series.map((series) => series.id)).toEqual([
      `${large.mac}-RX`,
      `${large.mac}-TX`,
      `${small.mac}-RX`,
      `${small.mac}-TX`,
    ]);
    expect(current().series[2].data).toEqual([[time, 1e-9]]);
    expect(current().series[3].data).toEqual([[time, 2.5e-10]]);
    expect(current(true).series[0].data).toEqual([
      [0, 0, 2],
      [0, 1, 1.25e-9],
    ]);
    expect(current(true).visualMap.text).toEqual(["2 GB/s", "0 GB/s"]);
    expect(current(true).visualMap.formatter(1.23456)).toBe("1.23 GB/s");
    expect(
      current().tooltip.formatter([
        { seriesName: "同名设备 · RX", value: [time, 2] },
        { seriesName: "同名设备 · TX", value: [time, 2.5e-10] },
        { seriesName: "缺测", value: [time, null] },
      ]),
    ).toContain("同名设备 · RX  2 GB/s");
    expect(
      current().tooltip.formatter([
        { seriesName: "同名设备 · TX", value: [time, 2.5e-10] },
      ]),
    ).toContain("<0.01 GB/s");
    expect(
      current(true).tooltip.formatter({ value: [0, 1, 1.25e-9] }),
    ).toContain(`${small.mac}`);
    expect(
      current(true).tooltip.formatter({ value: [0, 1, 1.25e-9] }),
    ).toContain("RX + TX <0.01 GB/s");
    expect(large.activity!.samples[0].rxBytes).toBe(2e9);
  });
  it("does not invent combined flow when only one direction exists and ignores unmeasured huge counters", () => {
    renderHistory([
      workspace([
        {
          time: "2026-10-03T00:00:00Z",
          rxBytes: 2000,
          txBytes: null,
          coverageSeconds: 2,
        },
        {
          time: "2026-10-03T01:00:00Z",
          rxBytes: 1e12,
          txBytes: 1e12,
          coverageSeconds: 0,
        },
        {
          time: "2026-10-03T02:00:00Z",
          rxBytes: NaN,
          txBytes: -1,
          coverageSeconds: 1,
        },
        {
          time: "2026-10-03T03:00:00Z",
          rxBytes: Infinity,
          txBytes: null,
          coverageSeconds: 1,
        },
      ]),
    ]);
    expect(captured.size).toBe(1);
    expect(current().yAxis.name).toBe("KB/s");
    expect(current().series[0].data.map((point) => point[1])).toEqual([
      1,
      null,
      null,
      null,
    ]);
    expect(current().series[1].data.map((point) => point[1])).toEqual([
      null,
      null,
      null,
      null,
    ]);
    expect(screen.getByText("等待所选设备的有效流量采样。")).toBeTruthy();
  });
  it("formats totals separately from rates and shows missing/stale latest rates as unavailable", () => {
    const base = workspace();
    const device = {
      ...base,
      activity: {
        ...base.activity!,
        rxBytes: 2e9,
        txBytes: 1e12,
        rxBytesPerSecond: 2e6,
        txBytesPerSecond: undefined,
      },
    };
    const view = renderHistory([device]);
    const table = screen.getByRole("table", {
      name: "所选设备 · 同一时间范围数据汇总",
    });
    expect(within(table).getByText("2 GB / 1 TB")).toBeTruthy();
    expect(within(table).getByText("2 MB/s / —")).toBeTruthy();
    expect(within(table).getByText("3600 秒")).toBeTruthy();
    view.rerender(
      <DeviceHistoryCharts
        devices={[{ ...device, activity: { ...device.activity, stale: true } }]}
        resolutionSeconds={3600}
        source="trafficd"
      />,
    );
    expect(within(table).getByText("等待有效速率")).toBeTruthy();
    expect(within(table).getByText("2 GB / 1 TB")).toBeTruthy();
  });
  it("keeps chart DOM and source values stable when a new range changes units", () => {
    const time = "2026-10-03T00:00:00Z";
    const view = renderHistory([
      workspace([{ time, rxBytes: 1000, txBytes: 1, coverageSeconds: 1 }]),
    ]);
    const charts = screen.getAllByRole("img");
    view.rerender(
      <DeviceHistoryCharts
        devices={[
          workspace([{ time, rxBytes: 1e9, txBytes: 1, coverageSeconds: 1 }]),
        ]}
        resolutionSeconds={3600}
        source="trafficd"
      />,
    );
    expect(screen.getAllByRole("img")[0]).toBe(charts[0]);
    expect(screen.getAllByRole("img")[1]).toBe(charts[1]);
    expect(current().yAxis.name).toBe("GB/s");
    expect(current().series[1].data[0]).toEqual([time, 1e-9]);
  });
});
