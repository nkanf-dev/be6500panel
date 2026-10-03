import { fireEvent, render, screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { EChartsOption } from "echarts";
import { requestTraceFixture } from "../../modules/proxy/request-trace-fixture.test-data";
import type { RequestTrace } from "../../modules/proxy/request-trace-contracts";
import { RequestWaterfall, requestWaterfallOption } from "./RequestWaterfall";
import type { ChartPalette } from "./chart-theme";

vi.mock("./EChart", () => ({
  EChart: ({ label }: { label: string }) => (
    <div role="img" aria-label={label} />
  ),
}));
const palette = {
  text: "text",
  foreground: "fg",
  grid: "grid",
  surface: "surface",
  tooltip: "tooltip",
  rx: "rx",
  tx: "tx",
  latency: "latency",
  direct: "direct",
  proxy: "proxy",
  blocked: "blocked",
  dns: "dns",
  connect: "tcp",
  tls: "tls",
  wait: "ttfb",
  transfer: "transfer",
  heatLow: "low",
  heatHigh: "high",
  font: "sans",
} satisfies ChartPalette;
describe("actual request waterfall", () => {
  it("does not turn missing production traces into fictional demo samples", () => {
    render(<RequestWaterfall />);
    expect(screen.getByText("暂未执行网络诊断")).toBeVisible();
    expect(screen.queryByRole("img")).toBeNull();
    expect(screen.queryByRole("table", { hidden: true })).toBeNull();
    expect(screen.queryByText(/R01/)).toBeNull();
    expect(screen.queryByText("演示数据")).toBeNull();
  });
  it("draws each phase at its actual start/end offsets, preserves overlap and marks incomplete/failure positions", () => {
    const option: EChartsOption = requestWaterfallOption(
      [requestTraceFixture],
      palette,
    );
    const series = option.series as {
      name: string;
      data: [number, number][];
      symbol: string;
      stack?: string;
    }[];
    const tcp = series.find((item) => item.name === "代理连接 (TCP)")!;
    const connect = series.find((item) => item.name === "CONNECT 隧道")!;
    const tls = series.find((item) => item.name === "目标服务 TLS 握手")!;
    const fail = series.find((item) => item.name === "失败或终止")!;
    expect(tcp.data.map((point) => point[0])).toEqual([10, 35]);
    expect(connect.data.map((point) => point[0])).toEqual([30, 50]);
    expect(tls.data.map((point) => point[0])).toEqual([50]);
    expect(tls.symbol).toBe("emptyCircle");
    expect(fail.data.map((point) => point[0])).toEqual([90]);
    expect(fail.symbol).toBe("diamond");
    expect(series.some((item) => item.name === "DNS 解析")).toBe(false);
    expect(series.every((item) => item.stack === undefined)).toBe(true);
    expect(option.yAxis).toMatchObject({ min: -1, max: 1, interval: 1 });
    const yAxis = option.yAxis as {
      axisLabel: { formatter: (value: number) => string };
    };
    expect(yAxis.axisLabel.formatter(0)).toContain("trace-actual");
    expect(yAxis.axisLabel.formatter(-1)).toBe("");
    expect((option.tooltip as { renderMode: string }).renderMode).toBe(
      "richText",
    );
  });
  it("offers an accessible detail table with unknowns, incomplete phases, true IP scope and failure metadata", () => {
    render(<RequestWaterfall traces={[requestTraceFixture]} />);
    fireEvent.click(screen.getByText("查看详细阶段耗时"));
    const table = screen.getByRole("table");
    expect(
      within(table).getByRole("columnheader", { name: "开始偏移 / ms" }),
    ).toBeVisible();
    expect(
      within(table).getByRole("columnheader", { name: "HTTP 状态码" }),
    ).toBeVisible();
    const dns = within(table).getByRole("row", { name: /DNS 解析/ });
    expect(within(dns).getAllByText("未知")).toHaveLength(4);
    expect(within(dns).queryByText("0")).toBeNull();
    const tls = within(table).getByRole("row", { name: /TLS 握手/ });
    expect(within(tls).getByText("已开始，未完成")).toBeVisible();
    expect(within(tls).getByText("50")).toBeVisible();
    expect(within(tls).getByText(/tls_failed/)).toBeVisible();
    expect(
      within(tls).getByText(/127.0.0.1:7890 · 代理监听器地址/),
    ).toBeVisible();
    expect(screen.getByText(/DNS 服务器未提供/)).toBeVisible();
  });
  it("filters stored traces by route and outcome, including failed HTTP outcomes", () => {
    const success: RequestTrace = {
      ...requestTraceFixture,
      id: "successful",
      route: "direct",
      outcome: "success",
      statusCode: 204,
      failurePhase: null,
      errorCode: undefined,
    };
    const http: RequestTrace = {
      ...success,
      id: "http-error",
      outcome: "http_error",
      statusCode: 503,
    };
    render(<RequestWaterfall traces={[requestTraceFixture, success, http]} />);
    fireEvent.change(screen.getByLabelText("筛选链路"), {
      target: { value: "direct" },
    });
    expect(screen.getByText("2 条诊断记录")).toBeVisible();
    fireEvent.change(screen.getByLabelText("筛选结果"), {
      target: { value: "http_error" },
    });
    expect(screen.getByText("1 条诊断记录")).toBeVisible();
    fireEvent.click(screen.getByText("查看详细阶段耗时"));
    expect(
      within(screen.getByRole("table")).queryByText(/trace-actual/),
    ).toBeNull();
    expect(within(screen.getByRole("table")).getAllByText("503")).toHaveLength(
      6,
    );
    fireEvent.change(screen.getByLabelText("筛选结果"), {
      target: { value: "timeout" },
    });
    expect(screen.getByText("没有符合筛选条件的诊断记录")).toBeVisible();
  });
  it("keeps rejected CONNECT timing unknown and locates the failure at totalMs", () => {
    const trace: RequestTrace = {
      ...requestTraceFixture,
      failurePhase: "connect",
      errorCode: "proxy_connect_failed",
      phases: [
        {
          id: "connect",
          observed: false,
          startMs: null,
          endMs: null,
          durationMs: null,
          reason: "connect_timing_not_exposed_by_httptrace",
        },
      ],
    };
    const option = requestWaterfallOption([trace], palette);
    const series = option.series as {
      name: string;
      data: [number, number][];
    }[];
    expect(series).toHaveLength(1);
    expect(series[0].data[0][0]).toBe(trace.totalMs);
    render(<RequestWaterfall traces={[trace]} />);
    fireEvent.click(screen.getByText("查看详细阶段耗时"));
    const connect = within(screen.getByRole("table")).getByRole("row", {
      name: /CONNECT 隧道/,
    });
    expect(
      within(connect).getByText(/HTTP 追踪未提供 CONNECT 阶段时间/),
    ).toBeVisible();
    expect(within(connect).getByText(/proxy_connect_failed/)).toBeVisible();
    expect(within(connect).getAllByText("未知")).toHaveLength(4);
  });
  it("labels multi-attempt TCP spans honestly instead of isolated handshakes", () => {
    const trace: RequestTrace = {
      ...requestTraceFixture,
      phases: [
        {
          ...requestTraceFixture.phases[1],
          reason: "multiple_connection_attempt_span",
        },
      ],
    };
    const option = requestWaterfallOption([trace], palette);
    const series = option.series as { name: string }[];
    expect(series[0].name).toBe("代理连接跨度 (TCP，多次尝试)");
    render(<RequestWaterfall traces={[trace]} />);
    fireEvent.click(screen.getByText("查看详细阶段耗时"));
    expect(screen.getByText("代理连接跨度 (TCP，多次尝试)")).toBeVisible();
    expect(screen.getByText(/此跨度包含多次连接尝试/)).toBeVisible();
  });
  it("labels backwards-compatible demo samples as fictional", () => {
    render(<RequestWaterfall demo />);
    expect(screen.getByText("演示数据")).toBeVisible();
    expect(screen.getByText(/虚构演示样本/)).toBeVisible();
    fireEvent.change(screen.getByLabelText("协议"), {
      target: { value: "DNS" },
    });
    expect(screen.getByText("1 个样本请求 · 完成时间 +208 ms")).toBeVisible();
    fireEvent.click(screen.getByText(/查看数据表/));
    expect(screen.getByRole("table")).toBeVisible();
  });
});
