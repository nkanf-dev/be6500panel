import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ThemeProvider } from "../theme";
import { ProxyPage } from "./proxy";
import { OverviewPage } from "./overview";
import { ConnectionAnalysis } from "./proxy/connection-analysis";
import { jsonResponse, routerSnapshot } from "./production-fixtures.test-data";
import { historyFixture } from "../components/traffic-history/history-fixture.test-data";

const state = vi.hoisted(() => ({ mode: "host" }));
vi.mock("../app/console-context", () => ({
  useConsole: () => ({
    health: { mode: state.mode, readOnly: state.mode === "demo" },
    capabilities: [],
    connection: "live",
    router: routerSnapshot,
    routerLoading: false,
    refreshRouter: vi.fn(),
    refreshing: false,
    trafficSamples:
      state.mode === "host"
        ? [{ time: "2026-01-01T00:00:00Z", rx: 0, tx: 0 }]
        : [],
    trafficSource: "WAN · synthetic-wan",
  }),
}));
vi.mock("./proxy/use-proxy-telemetry", () => ({
  useProxyTelemetry: () => ({
    data: undefined,
    error: undefined,
    loading: false,
    refresh: vi.fn(),
    probe: vi.fn(async () => {}),
    probing: false,
  }),
}));
vi.mock("../components/visualizations/EChart", () => ({
  EChart: ({ label }: { label: string }) => (
    <div role="img" aria-label={label} />
  ),
}));
beforeEach(() => window.localStorage.clear());
afterEach(() => {
  vi.unstubAllGlobals();
  state.mode = "host";
});
const renderPanel = (element: React.ReactNode) =>
  render(<ThemeProvider>{element}</ThemeProvider>);

describe("production visualization entry points", () => {
  it("exposes connection analysis as a primary proxy tab, separate from preview and logs", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn((url: string) =>
        Promise.resolve(
          jsonResponse(
            url === "/api/runtime"
              ? { enabled: false, services: [] }
              : { nodes: [], diagnostics: [], selectedNodeId: "" },
          ),
        ),
      ),
    );
    const user = userEvent.setup();
    renderPanel(<ProxyPage />);
    await user.click(screen.getByRole("tab", { name: "连接分析" }));
    expect(screen.getByRole("tab", { name: "连接分析" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    expect(
      screen.getByRole("region", { name: "活动连接时间线" }),
    ).toBeInTheDocument();
    expect(screen.queryByText("变更计划")).not.toBeInTheDocument();
    expect(screen.queryByText("运行日志")).not.toBeInTheDocument();
  });
  it("keeps all four actual analysis charts accessible, with missing samples and no demo statistics", async () => {
    const user = userEvent.setup();
    renderPanel(<ConnectionAnalysis />);
    for (const [tab, title] of [
      ["活动连接", "活动连接时间线"],
      ["代理流量", "代理流量"],
      ["探测延迟", "探测延迟分布"],
      ["活动连接分流", "活动连接分流"],
    ]) {
      await user.click(screen.getByRole("tab", { name: tab }));
      const chart = screen.getByRole("region", { name: title });
      expect(within(chart).getByRole("status")).toHaveTextContent(
        "尚未取得代理采样",
      );
      expect(within(chart).queryByRole("img")).not.toBeInTheDocument();
      expect(within(chart).queryByRole("table")).not.toBeInTheDocument();
      expect(within(chart).queryByText("演示数据")).not.toBeInTheDocument();
    }
  });
  it("labels each demo analysis chart and keeps its readable data-table alternative", async () => {
    state.mode = "demo";
    const user = userEvent.setup();
    renderPanel(<ConnectionAnalysis />);
    for (const [tab, title] of [
      ["请求阶段", "请求瀑布"],
      ["延迟分布", "延迟分布"],
      ["规则命中", "规则命中"],
    ]) {
      await user.click(screen.getByRole("tab", { name: tab }));
      const chart = screen.getByRole("region", { name: title });
      expect(within(chart).getByText("演示数据")).toBeInTheDocument();
      expect(within(chart).getByText("来源：固定样本")).toBeInTheDocument();
      expect(within(chart).getByRole("img")).toBeInTheDocument();
      expect(
        within(chart).getByRole("table", { hidden: true }),
      ).toBeInTheDocument();
    }
  });
  it("renders the custom homepage with observed devices and durable WAN history, not browser samples or a heatmap", async () => {
    const fetch = vi.fn().mockResolvedValue(jsonResponse(historyFixture()));
    vi.stubGlobal("fetch", fetch);
    renderPanel(<OverviewPage navigate={vi.fn()} />);
    await screen.findByText("来源：服务器 WAN 聚合历史（当前接口：WAN · test-wan）");
    expect(
      screen.getByRole("button", { name: "编辑布局" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("list", { name: "仪表盘组件" }),
    ).toBeInTheDocument();
    const devices = screen.getByRole("listitem", { name: "设备观察" });
    expect(within(devices).getByText("test-client")).toBeInTheDocument();
    expect(within(devices).getByText("192.0.2.20")).toBeInTheDocument();
    expect(within(devices).getByText("ARP 已观测")).toBeInTheDocument();
    expect(
      screen.queryByRole("region", { name: "终端活跃度" }),
    ).not.toBeInTheDocument();
    const traffic = screen.getByRole("region", { name: "WAN 流量历史" });
    expect(
      within(traffic).getByText(
        "服务器 WAN 聚合历史（当前接口：WAN · test-wan）",
        { selector: ".viz-source" },
      ),
    ).toBeInTheDocument();
    expect(within(traffic).getByRole("img")).toBeInTheDocument();
    expect(
      within(traffic).getByRole("table", { hidden: true }),
    ).toBeInTheDocument();
    expect(screen.queryByText("演示数据")).not.toBeInTheDocument();
    expect(fetch).toHaveBeenCalledWith(
      "/api/traffic/history?range=30m&maxPoints=1500",
      expect.any(Object),
    );
    expect(screen.queryByText("WAN · synthetic-wan")).not.toBeInTheDocument();
  });
  it("labels demo history and actual supplied demo device observations without inventing a device heatmap", () => {
    state.mode = "demo";
    const fetch = vi.fn();
    vi.stubGlobal("fetch", fetch);
    renderPanel(<OverviewPage navigate={vi.fn()} />);
    const devices = screen.getByRole("listitem", { name: "设备观察" });
    expect(within(devices).getByText("演示数据")).toBeInTheDocument();
    expect(within(devices).getByText("test-client")).toBeInTheDocument();
    const traffic = screen.getByRole("region", { name: "WAN 流量历史" });
    expect(within(traffic).getByText("演示数据")).toBeInTheDocument();
    expect(within(traffic).getByRole("img")).toBeInTheDocument();
    expect(
      within(traffic).getByRole("table", { hidden: true }),
    ).toBeInTheDocument();
    expect(
      screen.queryByRole("region", { name: "终端活跃度" }),
    ).not.toBeInTheDocument();
    expect(fetch).not.toHaveBeenCalled();
  });
});
