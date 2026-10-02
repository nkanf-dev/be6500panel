import {
  cleanup,
  fireEvent,
  render,
  screen,
  within,
} from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { EChartsOption } from "echarts";
import { ThemeProvider } from "../../theme";
import { TrafficTrend, type TrafficSample } from "./index";
import type { ChartPalette } from "./chart-theme";
import { historyFixture } from "../traffic-history/history-fixture.test-data";

const captured = vi.hoisted(() => ({
  option: undefined as ((palette: ChartPalette) => EChartsOption) | undefined,
}));
vi.mock("./EChart", () => ({
  EChart: ({
    option,
    label,
  }: {
    option: (palette: ChartPalette) => EChartsOption;
    label: string;
  }) => {
    captured.option = option;
    return <div role="img" aria-label={label} />;
  },
}));
afterEach(() => {
  cleanup();
  captured.option = undefined;
});
const points: readonly TrafficSample[] = [
  { time: "14:00:00", rx: 1_250_000, tx: 500_000 },
  { time: "14:00:05", rx: 2_000_000, tx: 750_000 },
];
interface OptionView {
  series: {
    name: string;
    data: (number | null | [number, number | null])[];
    connectNulls?: boolean;
  }[];
  yAxis: { name: string }[];
  xAxis: {
    type: string;
    data: string[];
    min?: number;
    max?: number;
    axisLabel?: { formatter: (time: number) => string };
  };
  dataZoom: { type: string }[];
  tooltip: {
    formatter: (
      parameters: {
        name: string;
        seriesName: string;
        value: number | null | [number, number | null];
      }[],
    ) => string;
  };
}
function currentOption() {
  return captured.option!({} as ChartPalette) as unknown as OptionView;
}
function renderTraffic(
  samples?: readonly TrafficSample[],
  demo = false,
  source?: string,
) {
  return render(
    <ThemeProvider defaultMode="light">
      <TrafficTrend samples={samples} demo={demo} source={source} />
    </ThemeProvider>,
  );
}

describe("TrafficTrend actual interface samples", () => {
  it("renders real bytes/s in MB/s without demo samples or latency", () => {
    renderTraffic(points);
    expect(
      screen.getByText("接口采样", { selector: ".viz-source" }),
    ).toBeTruthy();
    expect(screen.getByText("来源：接口采样")).toBeTruthy();
    expect(screen.queryByText("演示数据")).toBeNull();
    expect(screen.queryByText("来源：固定样本")).toBeNull();
    expect(screen.queryByLabelText("指标")).toBeNull();
    expect(screen.getByText("最新 RX 2.000 MB/s · TX 0.750 MB/s")).toBeTruthy();
    const option = currentOption();
    expect(option.series.map((series) => series.name)).toEqual(["RX", "TX"]);
    expect(option.series[0].data).toEqual([1.25, 2]);
    expect(option.series[1].data).toEqual([0.5, 0.75]);
    expect(option.yAxis.map((axis) => axis.name)).toEqual(["MB/s"]);
    expect(option.xAxis.data).toEqual(points.map((point) => point.time));
    fireEvent.click(screen.getByText(/查看数据表/));
    const table = screen.getByRole("table", { hidden: true });
    expect(within(table).getAllByRole("row", { hidden: true })).toHaveLength(3);
    expect(
      within(table)
        .getAllByRole("columnheader", { hidden: true })
        .map((cell) => cell.textContent),
    ).toEqual(["时间", "RX / MB/s", "TX / MB/s"]);
    expect(within(table).getByText("1.250")).toBeTruthy();
  });
  it("actual samples take priority even if demo is true", () => {
    renderTraffic(points, true, "WAN · eth0.1");
    expect(
      screen.getByText("WAN · eth0.1", { selector: ".viz-source" }),
    ).toBeTruthy();
    expect(screen.getByText("来源：WAN · eth0.1")).toBeTruthy();
    expect(screen.queryByText("演示数据")).toBeNull();
    expect(screen.getByText("2 条")).toBeTruthy();
    expect(currentOption().xAxis.data).toEqual(["14:00:00", "14:00:05"]);
  });
  it("keeps the entire server-bounded range beyond 300 points and keeps measured zero traffic", () => {
    const large = Array.from({ length: 1500 }, (_, i) => ({
      time: `T${i}`,
      rx: i * 1_000_000,
      tx: 0,
    }));
    renderTraffic(large);
    expect(screen.getByText("1500 条")).toBeTruthy();
    const option = currentOption();
    expect(option.series[0].data).toHaveLength(1500);
    expect(option.series[0].data[0]).toBe(0);
    expect(option.series[0].data.at(-1)).toBe(1499);
    expect(option.series[1].data.every((value) => value === 0)).toBe(true);
    expect(large).toHaveLength(1500);
    expect(option.dataZoom.map((zoom) => zoom.type)).toEqual([
      "inside",
      "slider",
    ]);
    expect(screen.queryByLabelText("样本范围")).toBeNull();
    expect(
      within(screen.getByRole("table", { hidden: true })).getAllByRole("row", {
        hidden: true,
      }),
    ).toHaveLength(1501);
  });
  it("uses an empty state instead of a demo fallback when no samples exist", () => {
    renderTraffic([]);
    expect(screen.getByRole("status").textContent).toContain("未采样");
    expect(screen.queryByRole("img")).toBeNull();
    expect(screen.queryByText("演示数据")).toBeNull();
    expect(screen.queryByText(/最新 RX/)).toBeNull();
  });
  it("preserves the explicit demo interface and converts original Mbps fixtures", () => {
    renderTraffic([], true);
    expect(screen.getByText("演示数据")).toBeTruthy();
    expect(screen.getByText("61 条")).toBeTruthy();
    expect(currentOption().yAxis.map((axis) => axis.name)).toEqual([
      "MB/s",
      "ms",
    ]);
    expect(currentOption().series[0].data[0]).toBe(17 / 8);
    fireEvent.change(screen.getByLabelText("时间范围"), {
      target: { value: "15" },
    });
    expect(screen.getByText("16 条")).toBeTruthy();
  });
  it("leaves missing latency as gaps and never fabricates the latest value", () => {
    const partial = [{ ...points[0], latency: 0 }, points[1]];
    renderTraffic(partial);
    expect(screen.getByLabelText("指标")).toBeTruthy();
    const latency = currentOption().series.find(
      (series) => series.name === "延迟",
    );
    expect(latency?.data).toEqual([0, null]);
    expect(latency?.connectNulls).toBe(false);
    expect(screen.getByText(/最新 RX/).textContent).not.toContain("延迟");
    const table = screen.getByRole("table", { hidden: true });
    expect(within(table).getByText("—")).toBeTruthy();
    fireEvent.change(screen.getByLabelText("指标"), {
      target: { value: "latency" },
    });
    expect(currentOption().series.map((series) => series.name)).toEqual([
      "延迟",
    ]);
  });
  it("drops stale latency selection when the new interface has no latency", () => {
    const view = renderTraffic(
      points.map((point) => ({ ...point, latency: 12 })),
    );
    fireEvent.change(screen.getByLabelText("指标"), {
      target: { value: "latency" },
    });
    view.rerender(
      <ThemeProvider defaultMode="light">
        <TrafficTrend samples={points} />
      </ThemeProvider>,
    );
    expect(screen.queryByLabelText("指标")).toBeNull();
    expect(currentOption().series.map((series) => series.name)).toEqual([
      "RX",
      "TX",
    ]);
    expect(screen.queryByText(/延迟/)).toBeNull();
  });
  it("uses a UTC time axis and leaves uncovered and partially covered buckets blank, with accurate table totals", () => {
    const history = historyFixture();
    const sample = history.samples[0];
    const samples = [
      sample,
      history.samples[1],
      { ...history.samples[2], coverageSeconds: 15 },
      { ...sample, time: "2026-10-02T18:01:30Z", rx: 0, tx: 0 },
    ];
    render(
      <ThemeProvider>
        <TrafficTrend
          samples={samples}
          range="30m"
          resolutionSeconds={30}
          rangeEnd={Date.parse("2026-10-02T18:02:00Z")}
        />
      </ThemeProvider>,
    );
    const option = currentOption();
    expect(option.xAxis.type).toBe("time");
    expect(option.xAxis.max).toBe(Date.parse("2026-10-02T18:02:00Z"));
    expect(option.xAxis.min).toBe(Date.parse("2026-10-02T17:32:00Z"));
    expect(option.series[0].data).toEqual([
      [Date.parse(sample.time), 1],
      [Date.parse(history.samples[1].time), null],
      [Date.parse(history.samples[2].time), null],
      [Date.parse(samples[3].time), 0],
    ]);
    expect(option.series.every((series) => series.connectNulls === false)).toBe(
      true,
    );
    const table = screen.getByRole("table", { hidden: true });
    expect(within(table).getByText("2026-10-02T18:00:00.000Z")).toBeTruthy();
    expect(
      within(table).getByRole("columnheader", {
        name: "RX 总量 / bytes",
        hidden: true,
      }),
    ).toBeTruthy();
    expect(within(table).getAllByText("30000000")).toHaveLength(2);
    expect(
      within(table).getByText("15000000", { selector: "td:nth-child(6)" }),
    ).toBeTruthy();
    expect(within(table).getByText("15", { selector: "td" })).toBeTruthy();
    expect(
      option.tooltip.formatter([
        {
          name: "local-time",
          seriesName: "RX",
          value: [Date.parse(sample.time), 1],
        },
      ]),
    ).toBe("2026-10-02T18:00:00.000Z\nRX  1.000 MB/s");
  });
  it("does not connect complete buckets across omitted intervals or turn a latest uncovered bucket into zero traffic", () => {
    const history = historyFixture();
    const samples = [
      history.samples[0],
      history.samples[2],
      { ...history.samples[1], time: "2026-10-02T18:01:30Z" },
    ];
    render(
      <ThemeProvider>
        <TrafficTrend samples={samples} resolutionSeconds={30} />
      </ThemeProvider>,
    );
    expect(currentOption().series[0].data).toEqual([
      [Date.parse(samples[0].time), 1],
      [Date.parse("2026-10-02T18:00:30Z"), null],
      [Date.parse(samples[1].time), 0.5],
      [Date.parse(samples[2].time), null],
    ]);
    expect(screen.getByText("最新时间桶未采样")).toBeTruthy();
    expect(screen.getByText("3 条")).toBeTruthy();
  });
  it("uses the correct tooltip units and skips absent latency values", () => {
    renderTraffic(points);
    expect(
      currentOption().tooltip.formatter([
        { name: "14:00:05", seriesName: "RX", value: 2 },
        { name: "14:00:05", seriesName: "TX", value: 0.75 },
        { name: "14:00:05", seriesName: "延迟", value: null },
      ]),
    ).toBe("14:00:05\nRX  2.000 MB/s\nTX  0.750 MB/s");
    expect(
      currentOption().tooltip.formatter([
        { name: "14:00:05", seriesName: "延迟", value: 12 },
      ]),
    ).toBe("14:00:05\n延迟  12 ms");
  });
});
