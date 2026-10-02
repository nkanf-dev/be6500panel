import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ThemeProvider } from "../theme";
import { ProxyPage } from "./proxy";
import { OverviewPage } from "./overview";
import { ConnectionAnalysis } from "./proxy/connection-analysis";
import { jsonResponse } from "./production-fixtures.test-data";
import { historyFixture } from "../components/traffic-history/history-fixture.test-data";

const state = vi.hoisted(() => ({ mode: "host" }));
vi.mock("../app/console-context", () => ({
  useConsole: () => ({
    health: { mode: state.mode, readOnly: state.mode === "demo" },
    capabilities: [],
    connection: "live",
    trafficSamples:
      state.mode === "host"
        ? [{ time: "2026-01-01T00:00:00Z", rx: 0, tx: 0 }]
        : [],
    trafficSource: "WAN · synthetic-wan",
  }),
}));
vi.mock("../components/visualizations/EChart", () => ({
  EChart: ({ label }: { label: string }) => (
    <div role="img" aria-label={label} />
  ),
}));
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
      screen.getByRole("region", { name: "请求瀑布" }),
    ).toBeInTheDocument();
    expect(screen.queryByText("变更计划")).not.toBeInTheDocument();
    expect(screen.queryByText("运行日志")).not.toBeInTheDocument();
  });
  it("keeps all three analysis charts accessible, with empty host collectors and no demo statistics", async () => {
    const user = userEvent.setup();
    renderPanel(<ConnectionAnalysis />);
    for (const [tab, title] of [
      ["请求阶段", "请求瀑布"],
      ["延迟分布", "延迟分布"],
      ["规则命中", "规则命中"],
    ]) {
      await user.click(screen.getByRole("tab", { name: tab }));
      const chart = screen.getByRole("region", { name: title });
      expect(within(chart).getByRole("status")).toHaveTextContent("未接入");
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
  it("retains the overview device heatmap while server history replaces browser WAN samples without a demo fallback", async () => {
    const fetch = vi.fn().mockResolvedValue(jsonResponse(historyFixture()));
    vi.stubGlobal("fetch", fetch);
    renderPanel(<OverviewPage navigate={vi.fn()} />);
    await screen.findByText("来源：WAN · test-wan");
    const heatmap = screen.getByRole("region", { name: "终端活跃度" });
    expect(within(heatmap).getByRole("status")).toHaveTextContent(
      "未接入终端活跃度",
    );
    expect(within(heatmap).queryByRole("img")).not.toBeInTheDocument();
    const traffic = screen.getByRole("region", { name: "WAN 流量历史" });
    expect(
      within(traffic).getByText("WAN · test-wan", { selector: ".viz-source" }),
    ).toBeInTheDocument();
    expect(screen.queryByText("演示数据")).not.toBeInTheDocument();
    expect(fetch).toHaveBeenCalledWith(
      "/api/traffic/history?range=30m&maxPoints=1500",
      expect.any(Object),
    );
    expect(screen.queryByText("WAN · synthetic-wan")).not.toBeInTheDocument();
  });
  it("keeps the overview heatmap explicitly marked as demo in demo mode", () => {
    state.mode = "demo";
    renderPanel(<OverviewPage navigate={vi.fn()} />);
    const heatmap = screen.getByRole("region", { name: "终端活跃度" });
    expect(within(heatmap).getByText("演示数据")).toBeInTheDocument();
    expect(within(heatmap).getByRole("img")).toBeInTheDocument();
  });
});
