import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { EChartsOption } from "echarts";
import type { ProxyMetrics } from "./telemetry-api";
import type { ChartPalette } from "../../components/visualizations/chart-theme";
import { ConnectionAnalysis } from "./connection-analysis";
import { ProxyTelemetryOverview } from "./telemetry-overview";

const state = vi.hoisted(() => ({
  data: undefined as unknown,
  error: undefined as unknown,
  loading: false,
  probing: false,
  refresh: vi.fn(),
  probe: vi.fn(async () => {}),
  charts: vi.fn(),
}));
vi.mock("../../app/console-context", () => ({
  useOptionalConsole: () => undefined,
  useConsole: () => ({ health: { mode: "host" } }),
}));
vi.mock("./use-proxy-telemetry", () => ({
  useProxyTelemetry: () => state,
}));
vi.mock("../../components/visualizations/EChart", () => ({
  EChart: ({
    label,
    option,
  }: {
    label: string;
    option: (p: ChartPalette) => EChartsOption;
  }) => {
    state.charts(label, option);
    return <div role="img" aria-label={label} />;
  },
}));

const available = { available: true, reason: "采样已启用" };
const sample = (): ProxyMetrics => ({
  state: "ready",
  reason: "控制器采样成功",
  source: "sing-box controller",
  sampledAt: "2026-01-01T00:00:02Z",
  capabilities: {
    connections: available,
    traffic: available,
    routing: available,
    latency: available,
    requestPhases: { available: false, reason: "HTTPS 流量不提供请求阶段" },
  },
  totals: { uploadBytes: 8192, downloadBytes: 65536 },
  activeConnections: 3,
  truncated: false,
  connections: [
    {
      id: "observed-a",
      sourceIP: "192.0.2.20",
      sourcePort: 44001,
      destinationIP: "203.0.113.10",
      destinationPort: 443,
      host: "www.example.test",
      startedAt: "2026-01-01T00:00:00Z",
      ageMs: 2000,
      network: "tcp",
      uploadBytes: 4096,
      downloadBytes: 32768,
      outbound: "proxy",
      ruleId: "rule-observed",
      rule: "domain-suffix=example.test",
    },
    {
      id: "observed-b",
      sourceIP: "192.0.2.21",
      sourcePort: 44002,
      destinationIP: "203.0.113.11",
      destinationPort: 53,
      host: "dns.example.test",
      startedAt: "2026-01-01T00:00:01Z",
      ageMs: 1000,
      network: "udp",
      uploadBytes: 2048,
      downloadBytes: 16384,
      outbound: "proxy",
      ruleId: "rule-observed",
      rule: "domain-suffix=example.test",
    },
    {
      id: "observed-c",
      sourceIP: "192.0.2.20",
      sourcePort: 44003,
      destinationIP: "192.0.2.1",
      destinationPort: 80,
      host: "router.example.test",
      startedAt: "2026-01-01T00:00:01Z",
      ageMs: 1000,
      network: "tcp",
      uploadBytes: 2048,
      downloadBytes: 16384,
      outbound: "direct",
      ruleId: "rule-lan",
      rule: "ip-is-private",
    },
  ],
  traffic: [
    {
      time: "2026-01-01T00:00:00Z",
      uploadRate: 1024,
      downloadRate: 2048,
      reset: false,
    },
    {
      time: "2026-01-01T00:00:01Z",
      uploadRate: 0,
      downloadRate: 0,
      reset: true,
    },
    {
      time: "2026-01-01T00:00:02Z",
      uploadRate: 512,
      downloadRate: 4096,
      reset: false,
    },
  ],
  probes: [
    { time: "2026-01-01T00:00:00Z", delayMs: 45, status: "ok" },
    { time: "2026-01-01T00:00:01Z", delayMs: 75, status: "ok" },
    { time: "2026-01-01T00:00:02Z", delayMs: 0, status: "failed" },
  ],
});
const palette = new Proxy({} as ChartPalette, { get: () => "#123456" });
const chartOption = (label: RegExp) => {
  const call = state.charts.mock.calls.find(([name]) => label.test(name));
  expect(call).toBeDefined();
  return (call![1] as (p: ChartPalette) => EChartsOption)(palette);
};
const tableIn = (title: string) =>
  within(screen.getByRole("region", { name: title })).getByRole("table", {
    hidden: true,
  });

beforeEach(() => {
  state.data = sample();
  state.error = undefined;
  state.loading = false;
  state.probing = false;
  state.refresh.mockReset();
  state.probe.mockReset().mockResolvedValue(undefined);
  state.charts.mockClear();
});
afterEach(() => vi.unstubAllGlobals());

describe("actual proxy telemetry analysis", () => {
  it("shows observed active connections and measured ages without invented DNS or TLS phases", () => {
    render(<ConnectionAnalysis />);
    const table = tableIn("活动连接时间线");
    expect(within(table).getByText("observed-a")).toBeInTheDocument();
    expect(within(table).getByText("2026-01-01T00:00:00Z")).toBeInTheDocument();
    expect(within(table).getByText("2000")).toBeInTheDocument();
    expect(within(table).getByText("www.example.test")).toBeInTheDocument();
    expect(within(table).getAllByText("192.0.2.20")).toHaveLength(2);
    expect(within(table).getByText("203.0.113.10")).toBeInTheDocument();
    expect(within(table).getByText("443")).toBeInTheDocument();
    expect(within(table).getByText("32768")).toBeInTheDocument();
    expect(screen.getAllByText(/sing-box controller/).length).toBeGreaterThan(
      0,
    );
    expect(screen.queryByText("演示数据")).not.toBeInTheDocument();
    expect(
      screen.queryByRole("tab", { name: "请求阶段" }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("columnheader", {
        name: /DNS|TLS|TTFB/,
        hidden: true,
      }),
    ).not.toBeInTheDocument();
    expect(state.probe).not.toHaveBeenCalled();
  });

  it("shows real proxy traffic rates and breaks the line when counters reset", async () => {
    const user = userEvent.setup();
    render(<ConnectionAnalysis />);
    await user.click(screen.getByRole("tab", { name: "代理流量" }));
    const table = tableIn("代理流量");
    expect(within(table).getByText("2026-01-01T00:00:01Z")).toBeInTheDocument();
    expect(within(table).getByText("4096")).toBeInTheDocument();
    expect(within(table).getByText("计数器重置")).toBeInTheDocument();
    const option = chartOption(/代理上传和下载/);
    const series = option.series as {
      data: unknown[];
      connectNulls: boolean;
    }[];
    expect(series[0].data).toEqual([1024, null, 512]);
    expect(series[1].data).toEqual([2048, null, 4096]);
    expect(series[0].connectNulls).toBe(false);
    expect(state.probe).not.toHaveBeenCalled();
  });

  it("groups only observed active routing and never calls the counts historical hits", async () => {
    const user = userEvent.setup();
    render(<ConnectionAnalysis />);
    await user.click(screen.getByRole("tab", { name: "活动连接分流" }));
    const table = tableIn("活动连接分流");
    const observed = within(table).getByText("rule-observed").closest("tr")!;
    expect(
      within(observed).getByText("domain-suffix=example.test"),
    ).toBeInTheDocument();
    expect(within(observed).getByText("proxy")).toBeInTheDocument();
    expect(within(observed).getByText("2")).toBeInTheDocument();
    expect(
      screen.queryByText(/累计命中|所选命中|GeoIP/),
    ).not.toBeInTheDocument();
    expect(screen.getByText(/不是累计规则命中/)).toBeInTheDocument();
  });

  it("counts only successful explicit selected-node probes and reports failed requests separately", async () => {
    const user = userEvent.setup();
    render(<ConnectionAnalysis />);
    await user.click(screen.getByRole("tab", { name: "探测延迟" }));
    expect(state.probe).not.toHaveBeenCalled();
    expect(screen.getByText(/2 次成功.*1 次失败/)).toBeInTheDocument();
    const table = tableIn("探测延迟分布");
    expect(within(table).getByText("45")).toBeInTheDocument();
    const failure = within(table).getByText("失败").closest("tr")!;
    expect(within(failure).getByText("—")).toBeInTheDocument();
    const option = chartOption(/成功请求探测延迟/);
    const series = option.series as { data: number[] }[];
    expect(series[0].data.reduce((sum, count) => sum + count, 0)).toBe(2);
    await user.click(screen.getByRole("button", { name: "探测当前选中节点" }));
    expect(state.probe).toHaveBeenCalledTimes(1);
    await user.click(screen.getByRole("button", { name: "刷新代理采样" }));
    expect(state.refresh).toHaveBeenCalledTimes(1);
    expect(state.probe).toHaveBeenCalledTimes(1);
  });

  it("keeps stale rows visibly labeled and reports sampling errors without demo fallback", () => {
    state.data = {
      ...sample(),
      state: "stale",
      reason: "控制器暂时断开",
      truncated: true,
    };
    state.error = new Error("controller refused");
    render(<ConnectionAnalysis />);
    expect(screen.getByRole("alert")).toHaveTextContent("controller refused");
    expect(screen.getAllByText(/上次采样/).length).toBeGreaterThan(0);
    expect(screen.getByText(/连接列表已截断/)).toBeInTheDocument();
    expect(
      within(tableIn("活动连接时间线")).getByText("observed-a"),
    ).toBeInTheDocument();
    expect(screen.queryByText("演示数据")).not.toBeInTheDocument();
  });

  it("shows the last backend snapshot when current capability reads fail, but disables active probes", async () => {
    const previous = sample();
    const capability = { available: false, reason: "控制器读取中断" };
    state.data = {
      ...previous,
      state: "stale",
      reason: "最后一次观测",
      capabilities: {
        connections: capability,
        traffic: capability,
        routing: capability,
        latency: capability,
        requestPhases: previous.capabilities.requestPhases,
      },
    };
    const user = userEvent.setup();
    render(<ConnectionAnalysis />);
    expect(
      within(tableIn("活动连接时间线")).getByText("observed-a"),
    ).toBeInTheDocument();
    expect(screen.getAllByText(/上次采样/).length).toBeGreaterThan(0);
    expect(screen.getByText(/控制器读取中断/)).toBeInTheDocument();
    await user.click(screen.getByRole("tab", { name: "代理流量" }));
    expect(within(tableIn("代理流量")).getByText("4096")).toBeInTheDocument();
    await user.click(screen.getByRole("tab", { name: "活动连接分流" }));
    expect(
      within(tableIn("活动连接分流")).getByText("rule-observed"),
    ).toBeInTheDocument();
    await user.click(screen.getByRole("tab", { name: "探测延迟" }));
    expect(within(tableIn("探测延迟分布")).getByText("45")).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "探测当前选中节点" }),
    ).toBeDisabled();
    expect(state.probe).not.toHaveBeenCalled();
  });

  it("shows failed-only probe history without a fabricated latency histogram", async () => {
    state.data = {
      ...sample(),
      probes: [{ time: "2026-01-01T00:00:02Z", delayMs: 0, status: "failed" }],
    };
    const user = userEvent.setup();
    render(<ConnectionAnalysis />);
    await user.click(screen.getByRole("tab", { name: "探测延迟" }));
    const chart = screen.getByRole("region", { name: "探测延迟分布" });
    expect(within(chart).queryByRole("img")).not.toBeInTheDocument();
    expect(within(chart).getByRole("status")).toHaveTextContent("没有成功探测");
    expect(
      within(tableIn("探测延迟分布")).getByText("失败"),
    ).toBeInTheDocument();
    expect(screen.getByText("0 次成功 · 1 次失败")).toBeInTheDocument();
  });

  it("labels unavailable capabilities and never displays contradictory fixture rows", async () => {
    state.data = {
      ...sample(),
      state: "unavailable",
      reason: "运行时未启动",
      capabilities: {
        ...sample().capabilities,
        connections: { available: false, reason: "连接控制器未启用" },
      },
    };
    const user = userEvent.setup();
    render(<ConnectionAnalysis />);
    const chart = screen.getByRole("region", { name: "活动连接时间线" });
    expect(within(chart).getByRole("status")).toHaveTextContent("运行时未启动");
    expect(
      within(chart).queryByRole("table", { hidden: true }),
    ).not.toBeInTheDocument();
    expect(within(chart).queryByRole("img")).not.toBeInTheDocument();
    await user.click(screen.getByRole("tab", { name: "探测延迟" }));
    expect(
      screen.getByRole("button", { name: "探测当前选中节点" }),
    ).toBeDisabled();
    expect(state.probe).not.toHaveBeenCalled();
  });

  it("shows request failure and empty measured connections as different states", () => {
    state.data = undefined;
    state.error = new Error("metrics request failed");
    const { rerender } = render(<ConnectionAnalysis />);
    expect(screen.getByRole("alert")).toHaveTextContent(
      "metrics request failed",
    );
    expect(screen.queryByRole("img")).not.toBeInTheDocument();
    state.error = undefined;
    state.data = { ...sample(), activeConnections: 0, connections: [] };
    rerender(<ConnectionAnalysis />);
    expect(
      within(screen.getByRole("region", { name: "活动连接时间线" })).getByRole(
        "status",
      ),
    ).toHaveTextContent("当前没有活动连接");
    expect(screen.queryByText("演示数据")).not.toBeInTheDocument();
  });

  it("shows loading and disables the explicit action while a probe is already running", async () => {
    state.data = undefined;
    state.loading = true;
    const user = userEvent.setup();
    const { rerender } = render(<ConnectionAnalysis />);
    expect(screen.getAllByText(/正在读取代理采样/).length).toBeGreaterThan(0);
    state.data = sample();
    state.loading = false;
    state.probing = true;
    rerender(<ConnectionAnalysis />);
    await user.click(screen.getByRole("tab", { name: "探测延迟" }));
    expect(
      screen.getByRole("button", { name: "正在探测选中节点" }),
    ).toBeDisabled();
    expect(state.probe).not.toHaveBeenCalled();
  });
});

describe("compact proxy telemetry overview", () => {
  it("shows measured stats, source and capability limits without running or offering probes", async () => {
    const user = userEvent.setup();
    render(<ProxyTelemetryOverview />);
    expect(
      screen.getByRole("region", { name: "代理遥测" }),
    ).toBeInTheDocument();
    expect(screen.getByText("3")).toBeInTheDocument();
    expect(screen.getByText("8.0 KiB")).toBeInTheDocument();
    expect(screen.getByText("64.0 KiB")).toBeInTheDocument();
    expect(screen.getByText(/sing-box controller/)).toBeInTheDocument();
    expect(screen.getByText(/HTTPS 流量不提供请求阶段/)).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: /探测/ }),
    ).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "刷新代理采样" }));
    expect(state.refresh).toHaveBeenCalledTimes(1);
    expect(state.probe).not.toHaveBeenCalled();
  });

  it("does not turn missing traffic capability into zero counters", () => {
    state.data = {
      ...sample(),
      capabilities: {
        ...sample().capabilities,
        traffic: { available: false, reason: "流量控制器未启用" },
      },
    };
    render(<ProxyTelemetryOverview />);
    expect(screen.getByText(/流量控制器未启用/)).toBeInTheDocument();
    expect(screen.queryByText("8.0 KiB")).not.toBeInTheDocument();
    expect(screen.queryByText("64.0 KiB")).not.toBeInTheDocument();
    expect(state.probe).not.toHaveBeenCalled();
  });

  it("retains last observed stats during a backend outage and shows current capability failures", () => {
    const previous = sample();
    const capability = { available: false, reason: "核心采样中断" };
    state.data = {
      ...previous,
      state: "stale",
      reason: "最后一次观测",
      capabilities: {
        connections: capability,
        traffic: capability,
        routing: capability,
        latency: capability,
        requestPhases: previous.capabilities.requestPhases,
      },
    };
    render(<ProxyTelemetryOverview />);
    expect(screen.getByText(/上次采样/)).toBeInTheDocument();
    expect(screen.getByText("3")).toBeInTheDocument();
    expect(screen.getByText("8.0 KiB")).toBeInTheDocument();
    expect(screen.getAllByText(/核心采样中断/).length).toBeGreaterThan(0);
    expect(state.probe).not.toHaveBeenCalled();
  });

  it("keeps prior stats labeled as stale on request failure", () => {
    state.error = new Error("metrics request failed");
    render(<ProxyTelemetryOverview />);
    expect(screen.getByRole("alert")).toHaveTextContent(
      "metrics request failed",
    );
    expect(screen.getByText(/上次采样/)).toBeInTheDocument();
    expect(screen.getByText("3")).toBeInTheDocument();
  });
});
