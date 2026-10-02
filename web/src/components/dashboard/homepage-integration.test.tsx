import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { ThemeProvider } from "../../theme";
import { CustomDashboard } from "./CustomDashboard";
import { DASHBOARD_STORAGE_KEY } from "./layout";
import { historyFixture } from "../traffic-history/history-fixture.test-data";
import {
  jsonResponse,
  routerSnapshot,
} from "../../modules/production-fixtures.test-data";
import type { ProxyMetrics } from "../../modules/proxy/telemetry-api";

const state = vi.hoisted(() => ({
  router: undefined as unknown,
  routerError: undefined as unknown,
  mode: "host",
}));
vi.mock("../../app/console-context", () => ({
  useConsole: () => ({
    health: { mode: state.mode, readOnly: true },
    system: {
      mode: state.mode,
      hostname: "observed-home-router",
      os: "linux",
      arch: "arm",
      kernel: "observed-kernel",
      uptimeSeconds: 3600,
      cpuCount: 4,
      memory: { totalBytes: 1024, availableBytes: 256 },
      load: [0.25, 0.5, 0.75],
      sampledAt: "2026-10-02T18:01:00Z",
    },
    router: state.router,
    routerError: state.routerError,
    routerLoading: false,
    refreshRouter: vi.fn(),
    capabilities: [
      {
        id: "proxy",
        title: "Proxy",
        description: "",
        state: "ready",
        capabilities: [
          { id: "observe", title: "实际连接观测", supported: true },
        ],
      },
    ],
    connection: "live",
    refreshing: false,
    refresh: vi.fn(),
    trafficSamples: [{ time: "2026-10-02T18:00:00Z", rx: 999999, tx: 999999 }],
    trafficSource: "browser-only-sample",
  }),
}));
vi.mock("../visualizations/EChart", () => ({
  EChart: ({ label }: { label: string }) => (
    <div role="img" aria-label={label} />
  ),
}));

const capability = { available: true, reason: "真实采样" };
function metricsFixture(): ProxyMetrics {
  return {
    state: "ready",
    reason: "控制器采样成功",
    source: "observed-local-controller",
    sampledAt: "2026-10-02T18:01:00Z",
    capabilities: {
      connections: capability,
      traffic: capability,
      routing: capability,
      latency: { available: false, reason: "尚未主动探测" },
      requestPhases: { available: false, reason: "HTTPS 请求阶段不可见" },
    },
    totals: { uploadBytes: 8192, downloadBytes: 65536 },
    activeConnections: 3,
    truncated: false,
    connections: [],
    traffic: [
      {
        time: "2026-10-02T18:01:00Z",
        uploadRate: 512,
        downloadRate: 4096,
        reset: false,
      },
    ],
    probes: [],
  };
}
function installFetch(telemetry = metricsFixture()) {
  const fetch = vi.fn((url: string) => {
    if (url.startsWith("/api/traffic/history"))
      return Promise.resolve(jsonResponse(historyFixture()));
    if (url === "/api/proxy/metrics")
      return Promise.resolve(jsonResponse(telemetry));
    return Promise.resolve(
      jsonResponse(
        { error: { code: "unexpected_test_request", message: url } },
        400,
      ),
    );
  });
  vi.stubGlobal("fetch", fetch);
  return fetch;
}
beforeEach(() => {
  window.localStorage.clear();
  state.router = routerSnapshot;
  state.routerError = undefined;
  state.mode = "host";
});
afterEach(() => vi.unstubAllGlobals());

describe("custom homepage with real widget integration", () => {
  it("renders observed system, devices, persistent history and proxy metrics; edit/save retains real widgets", async () => {
    const fetch = installFetch();
    const user = userEvent.setup();
    const navigate = vi.fn();
    render(
      <ThemeProvider>
        <CustomDashboard navigate={navigate} />
      </ThemeProvider>,
    );
    await screen.findByText("来源：WAN · test-wan");
    await screen.findByText(/来源：observed-local-controller/);
    const widgets = screen.getByRole("list", { name: "仪表盘组件" });
    expect(
      within(widgets)
        .getAllByRole("listitem")
        .filter((item) => item.hasAttribute("data-widget-id")),
    ).toHaveLength(6);
    expect(screen.getByText("observed-home-router")).toBeInTheDocument();
    expect(screen.getByText("75.0")).toBeInTheDocument();
    const devices = screen.getByRole("listitem", { name: "设备观察" });
    expect(within(devices).getByText("test-client")).toBeInTheDocument();
    expect(within(devices).getByText("ARP 已观测")).toBeInTheDocument();
    const traffic = screen.getByRole("region", { name: "WAN 流量历史" });
    expect(within(traffic).getByRole("img")).toBeInTheDocument();
    expect(
      within(traffic).getByRole("table", { hidden: true }),
    ).toHaveTextContent("2026-10-02T18:00:00.000Z");
    expect(screen.getByRole("button", { name: "导出 CSV" })).toBeEnabled();
    const proxy = screen.getByRole("region", { name: "代理遥测" });
    expect(within(proxy).getByText("3")).toBeInTheDocument();
    expect(within(proxy).getByText("8.0 KiB")).toBeInTheDocument();
    expect(within(proxy).getByText("64.0 KiB")).toBeInTheDocument();
    expect(within(proxy).getByText("512 / 4096 B/s")).toBeInTheDocument();
    expect(screen.queryByText("演示数据")).not.toBeInTheDocument();
    expect(screen.queryByText("browser-only-sample")).not.toBeInTheDocument();
    expect(
      screen.queryByRole("region", { name: "终端活跃度" }),
    ).not.toBeInTheDocument();
    expect(fetch.mock.calls.map(([url]) => url)).toEqual(
      expect.arrayContaining([
        "/api/traffic/history?range=30m&maxPoints=1500",
        "/api/proxy/metrics",
      ]),
    );
    expect(fetch.mock.calls.some(([url]) => url === "/api/proxy/probe")).toBe(
      false,
    );
    await user.click(within(devices).getByRole("button", { name: "设备详情" }));
    expect(navigate).toHaveBeenCalledWith("devices");
    await user.click(screen.getByRole("button", { name: "编辑布局" }));
    await user.click(screen.getByRole("checkbox", { name: "显示运行环境" }));
    await user.click(screen.getByRole("button", { name: "保存布局" }));
    expect(
      screen.queryByRole("listitem", { name: "运行环境" }),
    ).not.toBeInTheDocument();
    expect(
      screen.getByRole("region", { name: "WAN 流量历史" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("region", { name: "代理遥测" }),
    ).toBeInTheDocument();
    const stored = window.localStorage.getItem(DASHBOARD_STORAGE_KEY)!;
    expect(stored).not.toMatch(
      /test-client|192\.0\.2|observed-local-controller|sampledAt/,
    );
  });
  it("shows empty device observations and unavailable proxy telemetry without demo fallback", async () => {
    state.router = { ...routerSnapshot, devices: [] };
    const sample = metricsFixture();
    const unavailable = {
      ...sample,
      state: "unavailable" as const,
      reason: "演示模式未连接实际核心",
      activeConnections: 0,
      totals: { uploadBytes: 0, downloadBytes: 0 },
      traffic: [],
      capabilities: Object.fromEntries(
        Object.keys(sample.capabilities).map((key) => [
          key,
          { available: false, reason: "核心未连接" },
        ]),
      ) as ProxyMetrics["capabilities"],
    };
    installFetch(unavailable);
    render(
      <ThemeProvider>
        <CustomDashboard navigate={vi.fn()} />
      </ThemeProvider>,
    );
    await screen.findByText(/演示模式未连接实际核心/);
    expect(screen.getByText("未观察到设备")).toBeInTheDocument();
    const proxy = screen.getByRole("region", { name: "代理遥测" });
    expect(within(proxy).getByRole("status")).toHaveTextContent("不可用");
    expect(within(proxy).queryByText("0")).not.toBeInTheDocument();
    expect(within(proxy).queryByText("0 B")).not.toBeInTheDocument();
    expect(screen.queryByText("演示数据")).not.toBeInTheDocument();
    expect(
      screen.queryByRole("region", { name: "终端活跃度" }),
    ).not.toBeInTheDocument();
  });
});
