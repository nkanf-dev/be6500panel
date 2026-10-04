import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import {
  DevicesWidget,
  EnvironmentWidget,
  ModuleStatusWidget,
  SystemSummaryWidget,
} from "./observed-widgets";
import { routerSnapshot } from "../../modules/production-fixtures.test-data";
import type {
  Health,
  ModuleInfo,
  RouterSnapshot,
  SystemInfo,
} from "../../lib/contracts";

const state = vi.hoisted(() => ({
  health: { mode: "host", readOnly: true, status: "ok" } as Health | undefined,
  system: undefined as SystemInfo | undefined,
  systemError: undefined as unknown,
  router: undefined as RouterSnapshot | undefined,
  routerError: undefined as unknown,
  routerLoading: false,
  refreshRouter: vi.fn(),
  capabilities: [] as readonly ModuleInfo[],
  error: undefined as unknown,
  refresh: vi.fn(),
  refreshing: false,
  connection: "live",
}));
vi.mock("../../app/console-context", () => ({ useConsole: () => state }));
const systemSample: SystemInfo = {
  mode: "host",
  hostname: "observed-router",
  os: "linux",
  arch: "arm",
  kernel: "observed-kernel",
  uptimeSeconds: 3600,
  cpuCount: 4,
  memory: { totalBytes: 1024, availableBytes: 256 },
  load: [0.25, 0.5, 0.75],
  sampledAt: "2026-10-01T00:00:00Z",
};
beforeEach(() => {
  state.health = { mode: "host", readOnly: true, status: "ok" };
  state.system = undefined;
  state.systemError = undefined;
  state.router = undefined;
  state.routerError = undefined;
  state.routerLoading = false;
  state.refreshRouter.mockReset();
  state.refresh.mockReset();
  state.capabilities = [];
  state.error = undefined;
  state.refreshing = false;
  state.connection = "live";
});

describe("observed homepage widgets", () => {
  it("shows system placeholders without manufacturing metrics or demo", () => {
    render(<SystemSummaryWidget />);
    expect(screen.getByRole("status")).toHaveTextContent("不显示虚构指标");
    expect(screen.queryByText("演示数据")).not.toBeInTheDocument();
    expect(screen.queryByText("75.0")).not.toBeInTheDocument();
    expect(screen.getByText("等待采样")).toBeInTheDocument();
  });
  it("renders measured system values and labels stale data on an observation error", () => {
    state.system = systemSample;
    state.systemError = new Error("sampling interrupted");
    render(<SystemSummaryWidget />);
    expect(screen.getByText("0.25")).toBeInTheDocument();
    expect(screen.getByText("75.0")).toBeInTheDocument();
    expect(screen.getByText("768 B / 1.02 KB")).toBeInTheDocument();
    expect(screen.getByText("上次采样")).toBeInTheDocument();
    expect(screen.getByRole("alert")).toHaveTextContent("sampling interrupted");
    expect(screen.queryByText("实时采样")).not.toBeInTheDocument();
  });
  it("marks actual demo system and environment observations explicitly", () => {
    state.health = { mode: "demo", readOnly: true, status: "ok" };
    state.system = { ...systemSample, mode: "demo" };
    render(
      <>
        <SystemSummaryWidget />
        <EnvironmentWidget navigate={vi.fn()} />
      </>,
    );
    expect(screen.getAllByText("演示数据")).toHaveLength(2);
    expect(screen.getByText("observed-router")).toBeInTheDocument();
    expect(screen.getByText("linux / arm")).toBeInTheDocument();
    expect(screen.getByText("本地预览模式")).toBeInTheDocument();
  });
  it("uses reviewed write-enabled language without internal Commit terminology", () => {
    state.health = { mode: "host", readOnly: false, status: "ok" };
    state.system = systemSample;
    render(<EnvironmentWidget navigate={vi.fn()} />);
    expect(screen.getByText("配置与运行管理")).toBeInTheDocument();
    expect(screen.queryByText(/Commit/)).not.toBeInTheDocument();
  });
  it("shows explicit loading, empty and failed device observations, never sample clients", async () => {
    const user = userEvent.setup();
    const first = render(<DevicesWidget navigate={vi.fn()} />);
    expect(screen.getByText("未取得设备观察")).toBeInTheDocument();
    expect(screen.queryByText("test-client")).not.toBeInTheDocument();
    state.routerLoading = true;
    first.rerender(<DevicesWidget navigate={vi.fn()} />);
    expect(screen.getByRole("status")).toHaveTextContent("正在读取设备观察");
    state.routerError = new Error("router unavailable");
    first.rerender(<DevicesWidget navigate={vi.fn()} />);
    expect(screen.getByRole("alert")).toHaveTextContent("router unavailable");
    expect(screen.queryByText("正在读取设备观察")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "重试" }));
    expect(state.refreshRouter).toHaveBeenCalledOnce();
  });
  it("renders observed devices, bounds the list and keeps navigation", async () => {
    const user = userEvent.setup();
    const navigate = vi.fn();
    state.router = {
      ...routerSnapshot,
      devices: Array.from({ length: 8 }, (_, index) => ({
        ...routerSnapshot.devices[0],
        ip: `192.0.2.${index + 20}`,
        hostname: `observed-${index}`,
        online: index < 2,
      })),
    };
    render(<DevicesWidget navigate={navigate} />);
    expect(screen.getByText(/租约 8 · ARP 已观测 2/)).toBeInTheDocument();
    expect(screen.getByRole("list")).toBeInTheDocument();
    expect(screen.getAllByRole("listitem")).toHaveLength(5);
    expect(screen.getByText("observed-0")).toBeInTheDocument();
    expect(screen.queryByText("observed-5")).not.toBeInTheDocument();
    expect(screen.getAllByText("已发现设备")).toHaveLength(2);
    expect(screen.getAllByText("未见 ARP")).toHaveLength(3);
    expect(screen.queryByText("离线")).not.toBeInTheDocument();
    expect(screen.queryByText("演示数据")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "设备详情" }));
    expect(navigate).toHaveBeenCalledWith("devices");
  });
  it("distinguishes an empty device snapshot, incomplete ARP and stale failures", () => {
    state.router = { ...routerSnapshot, devices: [] };
    const result = render(<DevicesWidget navigate={vi.fn()} />);
    expect(screen.getByText("未观察到设备")).toBeInTheDocument();
    state.router = {
      ...routerSnapshot,
      devices: [{ ...routerSnapshot.devices[0], online: false }],
      errors: [
        {
          module: "devices.arp",
          code: "arp_failed",
          message: "ARP unavailable",
        },
      ],
    };
    state.routerError = new Error("new snapshot failed");
    result.rerender(<DevicesWidget navigate={vi.fn()} />);
    expect(screen.getAllByRole("alert")).toHaveLength(2);
    expect(screen.getByText(/上次采样 · 租约 1/)).toBeInTheDocument();
    expect(screen.getByText("ARP 观察不完整")).toBeInTheDocument();
    expect(screen.getByText("test-client")).toBeInTheDocument();
  });
  it("marks demo device observations without filling missing records", () => {
    state.health = { mode: "demo", readOnly: true, status: "ok" };
    state.router = { ...routerSnapshot, devices: [] };
    render(<DevicesWidget navigate={vi.fn()} />);
    expect(screen.getByText("演示数据")).toBeInTheDocument();
    expect(screen.getByText("未观察到设备")).toBeInTheDocument();
    expect(screen.queryByText("test-client")).not.toBeInTheDocument();
  });
  it("reports unknown capability status and exposes module shortcuts", async () => {
    const user = userEvent.setup();
    const navigate = vi.fn();
    render(<ModuleStatusWidget navigate={navigate} />);
    expect(screen.getByRole("status")).toHaveTextContent("暂无能力清单");
    expect(screen.queryByText("就绪")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "打开代理" }));
    expect(navigate).toHaveBeenCalledWith("proxy");
  });
  it("renders real module capabilities and keeps failures separate from availability", () => {
    state.capabilities = [
      {
        id: "proxy",
        title: "Proxy",
        description: "",
        state: "ready",
        capabilities: [
          { id: "telemetry", title: "真实连接观测", supported: true },
        ],
      },
    ];
    state.error = new Error("health lookup failed");
    render(<ModuleStatusWidget navigate={vi.fn()} />);
    expect(screen.getByRole("alert")).toHaveTextContent("health lookup failed");
    const proxyRow = screen.getByRole("row", { name: /代理.*就绪/ });
    expect(within(proxyRow).getByText("真实连接观测")).toBeInTheDocument();
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
    expect(screen.queryByText("演示数据")).not.toBeInTheDocument();
  });
});
