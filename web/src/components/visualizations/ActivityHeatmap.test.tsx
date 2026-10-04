import { fireEvent, render, screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { EChartsOption } from "echarts";
import { ActivityHeatmap, measuredDeviceBytes } from "./ActivityHeatmap";
import type { ChartPalette } from "./chart-theme";
import { activityFixture } from "../device-activity/activity-fixture.test-data";
let option: (palette: ChartPalette) => EChartsOption;
let onDataClick:
  | ((event: {
      value: unknown;
      componentType?: string;
      seriesType?: string;
    }) => void)
  | undefined;
vi.mock("./EChart", () => ({
  EChart: ({
    option: value,
    label,
    onDataClick: select,
  }: {
    option: typeof option;
    label: string;
    onDataClick?: typeof onDataClick;
  }) => {
    option = value;
    onDataClick = select;
    return <div role="img" aria-label={label} />;
  },
}));
const palette = {
  text: "#aaa",
  foreground: "#fff",
  grid: "#444",
  surface: "#000",
  tooltip: "#222",
  heatLow: "#eee",
  heatHigh: "#00f",
  font: "sans-serif",
} as ChartPalette;
const options = () =>
  option(palette) as {
    xAxis: { data: string[] };
    yAxis: { data: string[] };
    visualMap: {
      min: number;
      max: number;
      text: string[];
      formatter: (value: number) => string;
    };
    series: { name: string; data: number[][] }[];
    tooltip: { formatter: (parameter: unknown) => string };
  };

describe("truthful device byte heatmap", () => {
  it("plots actual byte deltas and UTC times, leaving null gaps and preserving measured zero", () => {
    render(
      <ActivityHeatmap
        devices={activityFixture().devices}
        resolutionSeconds={3600}
      />,
    );
    const chart = options();
    expect(chart.xAxis.data).toEqual([
      "2026-10-03T00:00:00.000Z",
      "2026-10-03T01:00:00.000Z",
      "2026-10-03T02:00:00.000Z",
    ]);
    expect(chart.series[0]).toMatchObject({
      name: "RX + TX / KB",
      data: [
        [0, 0, 3.072],
        [2, 0, 0],
      ],
    });
    expect(chart.tooltip.formatter({ value: [0, 0, 3.072] })).toContain(
      "3.07 KB",
    );
    expect(chart.tooltip.formatter({ value: [2, 0, 0] })).toContain("部分采样");
    expect(screen.queryByText("演示数据")).toBeNull();
    expect(screen.getByText("数据源：系统流量统计 (trafficd)")).toBeTruthy();
    fireEvent.click(screen.getByText(/查看数据表/));
    const table = screen.getByRole("table");
    expect(within(table).getByText("未采样")).toBeTruthy();
    expect(within(table).getByText("部分采样")).toBeTruthy();
    expect(within(table).getAllByText("0 B")).toHaveLength(2);
    expect(within(table).getByText("0")).toBeTruthy();
    expect(within(table).getAllByText("—")).toHaveLength(2);
  });
  it("shares a decimal scale across heat cells and color labels without changing raw byte counters", () => {
    const base = activityFixture().devices[0];
    const samples = Object.freeze([
      Object.freeze({
        time: "2026-10-03T00:00:00Z",
        rxBytes: 2e9,
        txBytes: 0,
        coverageSeconds: 30,
      }),
      Object.freeze({
        time: "2026-10-03T01:00:00Z",
        rxBytes: 1,
        txBytes: 0,
        coverageSeconds: 30,
      }),
      Object.freeze({
        time: "2026-10-03T02:00:00Z",
        rxBytes: 0,
        txBytes: 0,
        coverageSeconds: 30,
      }),
      Object.freeze({
        time: "2026-10-03T03:00:00Z",
        rxBytes: null,
        txBytes: 10,
        coverageSeconds: 30,
      }),
    ]);
    render(<ActivityHeatmap devices={[{ ...base, samples }]} />);
    const chart = options();
    expect(chart.series[0].data).toEqual([
      [0, 0, 2],
      [1, 0, 1e-9],
      [2, 0, 0],
    ]);
    expect(chart.visualMap.text).toEqual(["2 GB", "0 GB"]);
    expect(chart.visualMap.max).toBe(2);
    expect(chart.visualMap.formatter(1.234567)).toBe("1.23 GB");
    expect(chart.tooltip.formatter({ value: [1, 0, 1e-9] })).toContain(
      "<0.01 GB",
    );
    expect(samples[0].rxBytes).toBe(2e9);
    expect(screen.getByText(/已测量 2 GB/)).toBeTruthy();
  });
  it("uses B for a measured fractional byte cell and rejects invalid values instead of plotting zero", () => {
    const base = activityFixture().devices[0];
    render(
      <ActivityHeatmap
        devices={[
          {
            ...base,
            samples: [
              {
                time: "2026-10-03T00:00:00Z",
                rxBytes: 0.25,
                txBytes: 0,
                coverageSeconds: 30,
              },
              {
                time: "2026-10-03T01:00:00Z",
                rxBytes: NaN,
                txBytes: 1,
                coverageSeconds: 30,
              },
              {
                time: "2026-10-03T02:00:00Z",
                rxBytes: -1,
                txBytes: 1,
                coverageSeconds: 30,
              },
              {
                time: "2026-10-03T03:00:00Z",
                rxBytes: Infinity,
                txBytes: 1,
                coverageSeconds: 30,
              },
            ],
          },
        ]}
      />,
    );
    expect(options().series[0].data).toEqual([[0, 0, 0.25]]);
    expect(options().visualMap.text).toEqual(["0.25 B", "0 B"]);
    expect(options().visualMap.max).toBe(0.25);
  });
  it("uses vendor RX and TX without calling them download or upload", () => {
    render(<ActivityHeatmap devices={activityFixture().devices} />);
    fireEvent.change(screen.getByLabelText("字节方向"), {
      target: { value: "rx" },
    });
    expect(options().series[0].data).toEqual([
      [0, 0, 1.024],
      [2, 0, 0],
    ]);
    fireEvent.change(screen.getByLabelText("字节方向"), {
      target: { value: "tx" },
    });
    expect(options().series[0].data).toEqual([
      [0, 0, 2.048],
      [2, 0, 0],
    ]);
    expect(screen.queryByText(/请求|下载|上传/)).toBeNull();
    expect(
      screen.queryByRole("option", { name: /Connections|连接数/ }),
    ).toBeNull();
  });
  it("does not invent combined bytes when only one direction was measured", () => {
    const point = {
      time: "2026-10-03T00:00:00Z",
      rxBytes: 5,
      txBytes: null,
      coverageSeconds: 30,
    };
    expect(measuredDeviceBytes(point, "total")).toBeNull();
    expect(measuredDeviceBytes(point, "rx")).toBe(5);
    expect(
      measuredDeviceBytes({ ...point, rxBytes: 0, coverageSeconds: 0 }, "rx"),
    ).toBeNull();
  });
  it("keeps filters and detail rows available when a direction has no data, so users can recover", () => {
    const base = activityFixture().devices[0];
    const device = {
      ...base,
      samples: [
        {
          time: "2026-10-03T00:00:00Z",
          rxBytes: 5,
          txBytes: null,
          coverageSeconds: 30,
        },
      ],
    };
    render(<ActivityHeatmap devices={[device]} />);
    expect(screen.queryByRole("img")).toBeNull();
    expect(screen.getByRole("combobox", { name: "字节方向" })).toBeTruthy();
    expect(screen.getByText(/尚无已测量字节（不是零流量）/)).toBeTruthy();
    fireEvent.change(screen.getByLabelText("字节方向"), {
      target: { value: "rx" },
    });
    expect(screen.getByRole("img")).toBeTruthy();
    expect(options().series[0].data).toEqual([[0, 0, 5]]);
    fireEvent.click(screen.getByText(/查看数据表/));
    expect(within(screen.getByRole("table")).getByText("—")).toBeTruthy();
  });
  it("caps chart rows at 16 and exposes other returned devices through a keyboard-native filter", () => {
    const device = activityFixture().devices[0];
    const devices = Array.from({ length: 32 }, (_, index) => ({
      ...device,
      id: `02:00:00:00:00:${index.toString(16).padStart(2, "0")}`,
      name: `中立设备 ${index}`,
    }));
    render(<ActivityHeatmap devices={devices} />);
    expect(options().yAxis.data).toHaveLength(16);
    expect(screen.getByText(/图表仅显示前 16 个/)).toBeTruthy();
    fireEvent.change(screen.getByRole("combobox", { name: "终端" }), {
      target: { value: devices[31].id },
    });
    expect(options().yAxis.data).toEqual(["中立设备 31"]);
    expect(options().series[0].data).toHaveLength(2);
  });
  it("drills down clicked measured cells by canonical MAC, including measured zero and filtered rows", () => {
    const base = activityFixture().devices[0];
    const second = { ...base, id: "02:00:00:00:00:02", name: "同名设备" };
    const onSelectDevice = vi.fn();
    render(
      <ActivityHeatmap
        devices={[base, second]}
        onSelectDevice={onSelectDevice}
      />,
    );
    const click = (value: unknown) =>
      onDataClick?.({ value, componentType: "series", seriesType: "heatmap" });
    click([0, 1, 3072]);
    expect(onSelectDevice).toHaveBeenLastCalledWith(second.id);
    click([2, 0, 0]);
    expect(onSelectDevice).toHaveBeenLastCalledWith(base.id);
    click([1, 0, 0]);
    click([0, 99, 3072]);
    click([0.5, 0, 3072]);
    click(null);
    onDataClick?.({
      value: [0, 0, 3072],
      componentType: "axis",
      seriesType: "heatmap",
    });
    onDataClick?.({
      value: [0, 0, 3072],
      componentType: "series",
      seriesType: "line",
    });
    expect(onSelectDevice).toHaveBeenCalledTimes(2);
    fireEvent.change(screen.getByLabelText("终端"), {
      target: { value: second.id },
    });
    click([0, 0, 3072]);
    expect(onSelectDevice).toHaveBeenLastCalledWith(second.id);
  });
  it("never turns demo cell selection into a device drilldown", () => {
    const onSelectDevice = vi.fn();
    render(<ActivityHeatmap demo onSelectDevice={onSelectDevice} />);
    expect(onDataClick).toBeUndefined();
    expect(onSelectDevice).not.toHaveBeenCalled();
  });
  it("keeps stale measurements visible but marks their device and tooltip as stale", () => {
    const device = { ...activityFixture().devices[0], stale: true };
    render(<ActivityHeatmap devices={[device]} />);
    expect(options().yAxis.data[0]).toContain("陈旧");
    expect(options().tooltip.formatter({ value: [0, 0, 3072] })).toContain(
      "设备记录陈旧",
    );
  });
  it("always shows trafficd source even empty and never falls back to demo when real data is empty", () => {
    render(<ActivityHeatmap devices={[]} demo />);
    expect(screen.getByText("数据源：系统流量统计 (trafficd)")).toBeTruthy();
    expect(screen.getByRole("status")).toHaveTextContent("暂无时段活跃数据");
    expect(screen.queryByRole("img")).toBeNull();
    expect(screen.queryByText("演示数据")).toBeNull();
  });
  it("labels its own neutral demo byte shape and readable table, never requests", () => {
    render(<ActivityHeatmap demo />);
    expect(screen.getByText("演示数据")).toBeTruthy();
    expect(options().series[0].name).toBe("RX + TX / KB");
    expect(screen.queryByText(/请求/)).toBeNull();
    fireEvent.change(screen.getByLabelText("终端"), {
      target: { value: "sample-2" },
    });
    expect(screen.getByText("24 条")).toBeTruthy();
  });
});
