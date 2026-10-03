import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { SystemPage } from "../modules/system";
import { DevicePage } from "./device-page";
const state = vi.hoisted(() => ({
  selected: "02:00:00:00:00:AA",
  select: vi.fn(),
  refresh: vi.fn(),
  workspace: vi.fn(),
}));
vi.mock("./console-context", () => ({
  useConsole: () => ({
    selectedDeviceMAC: state.selected,
    selectDevice: state.select,
    refreshRouter: state.refresh,
    refresh: state.refresh,
    capabilities: [],
  }),
}));
vi.mock("../modules/devices", () => ({
  useDeviceLabels: () => ({ displayName: (mac: string) => mac }),
  DeviceWorkspace: (props: Record<string, unknown>) => {
    state.workspace(props);
    return <p>真实设备工作区</p>;
  },
}));
vi.mock("./use-device-workspace-history", () => ({
  useDeviceWorkspaceHistory: () => ({
    data: { range: "24h" },
    loading: false,
    refresh: state.refresh,
  }),
}));
vi.mock("../components/device-activity", () => ({
  DeviceActivityPanel: ({
    onSelectDevice,
  }: {
    onSelectDevice: (mac: string) => void;
  }) => (
    <button onClick={() => onSelectDevice("02:00:00:00:00:BB")}>
      选择热力图设备
    </button>
  ),
}));
vi.mock("../modules/proxy/use-proxy-telemetry", () => ({
  useProxyTelemetry: () => ({ data: undefined, refresh: state.refresh }),
}));
vi.mock("../modules/service-status-panel", () => ({
  ServiceStatusPanel: () => <p>真实服务管理面板</p>,
}));
vi.mock("../components/maintenance", () => ({
  MaintenanceBackupPanel: ({
    onOpenConfiguration,
  }: {
    onOpenConfiguration: () => void;
  }) => <button onClick={onOpenConfiguration}>打开暂存队列</button>,
}));
vi.mock("../components/configuration", () => ({
  ConfigurationEditor: ({ module }: { module: string }) => <p>编辑 {module}</p>,
  ConfigurationWorkspace: () => <p>真实配置暂存队列</p>,
}));
vi.mock("../modules/logs", () => ({ LogsPanel: () => null }));
describe("maturity page workflow wiring", () => {
  it("mounts real service and backup flows; staged import opens existing risk-aware queue", async () => {
    const user = userEvent.setup();
    render(<SystemPage />);
    await user.click(screen.getByRole("tab", { name: "服务管理" }));
    expect(screen.getByText("真实服务管理面板")).toBeInTheDocument();
    await user.click(screen.getByRole("tab", { name: "备份与导入" }));
    await user.click(screen.getByRole("button", { name: "打开暂存队列" }));
    expect(screen.getByRole("tab", { name: "配置变更" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    expect(screen.getByText("真实配置暂存队列")).toBeInTheDocument();
  });
  it("passes the root MAC and real source props to workspace; heatmap uses the same root selection", async () => {
    const user = userEvent.setup();
    render(<DevicePage />);
    const props = state.workspace.mock.calls.at(-1)![0];
    expect(props.selectedMAC).toBe(state.selected);
    expect(props.activity.range).toBe("24h");
    expect(props.proxy).toBeUndefined();
    await user.click(screen.getByRole("button", { name: "选择热力图设备" }));
    expect(state.select).toHaveBeenCalledWith("02:00:00:00:00:BB");
  });
});
