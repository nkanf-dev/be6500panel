import { fireEvent, render, screen, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { DeviceActivityHistory } from "../../lib/device-activity-contracts";
import { DeviceActivityPanel } from "./DeviceActivityPanel";
import { useDeviceActivity } from "./use-device-activity";
import { activityFixture } from "./activity-fixture.test-data";
vi.mock("./use-device-activity", () => ({ useDeviceActivity: vi.fn() }));
vi.mock("../visualizations/EChart", () => ({
  EChart: ({
    label,
    onDataClick,
  }: {
    label: string;
    onDataClick?: (event: {
      value: unknown;
      componentType?: string;
      seriesType?: string;
    }) => void;
  }) => (
    <div
      role="img"
      aria-label={label}
      onClick={() =>
        onDataClick?.({
          value: [0, 0, 3072],
          componentType: "series",
          seriesType: "heatmap",
        })
      }
    />
  ),
}));
const hook = vi.mocked(useDeviceActivity);
const reload = vi.fn();
function result(
  data?: DeviceActivityHistory,
  extra: Partial<ReturnType<typeof useDeviceActivity>> = {},
) {
  return { key: "test", data, loading: false, reload, ...extra };
}
beforeEach(() => {
  hook.mockReset();
  reload.mockReset();
  hook.mockReturnValue(result(activityFixture()));
});

describe("passive device activity panel", () => {
  it("shows source, true byte units, recent memory policy and latest-derived rate semantics", () => {
    render(<DeviceActivityPanel />);
    expect(
      screen.getAllByText("数据源：系统流量统计 (trafficd)").length,
    ).toBeGreaterThan(0);
    expect(
      screen.getByText(/最近最多 7 天的内存记录，服务重启后清空/),
    ).toBeTruthy();
    expect(
      screen.getByText(
        /最新 B\/s 由最近有效的连续采样间隔派生，不是范围平均值/,
      ),
    ).toBeTruthy();
    const table = screen.getByRole("table", { name: /设备流量 ·/ });
    expect(within(table).getByText("1.02 KB")).toHaveAttribute(
      "title",
      "1024 bytes",
    );
    expect(within(table).getByText("2.05 KB")).toHaveAttribute(
      "title",
      "2048 bytes",
    );
    expect(within(table).getByText("10 B/s")).toBeTruthy();
    expect(within(table).getByText("20 B/s")).toBeTruthy();
    expect(within(table).getByText("3600 秒 · 4.2%")).toBeTruthy();
    expect(
      screen.queryByRole("option", { name: /请求|Connections|活跃连接/ }),
    ).toBeNull();
    expect(screen.queryByRole("button", { name: /诊断|探测/ })).toBeNull();
  });
  it("formats large totals and latest rates adaptively with raw titles and unchanged counts/coverage", () => {
    const fixture = activityFixture();
    const device = {
      ...fixture.devices[0],
      rxBytes: 2e9,
      txBytes: 1e12,
      rxBytesPerSecond: 1.234567e9,
      txBytesPerSecond: 2e6,
    };
    const group = {
      ...fixture.groups[0],
      rxBytes: 2e9,
      txBytes: 1e12,
      rxBytesPerSecond: 1000,
      txBytesPerSecond: 2e9,
    };
    hook.mockReturnValue(
      result({ ...fixture, devices: [device], groups: [group] }),
    );
    render(<DeviceActivityPanel />);
    const devices = screen.getByRole("table", { name: /设备流量 ·/ });
    expect(within(devices).getByText("2 GB")).toHaveAttribute(
      "title",
      "2000000000 bytes",
    );
    expect(within(devices).getByText("1 TB")).toHaveAttribute(
      "title",
      "1000000000000 bytes",
    );
    expect(within(devices).getByText("1.23 GB/s")).toBeTruthy();
    expect(within(devices).getByText("2 MB/s")).toBeTruthy();
    expect(within(devices).getByText("3600 秒 · 4.2%")).toBeTruthy();
    const groups = screen.getByRole("table", { name: /系统接口分组/ });
    expect(within(groups).getByText("1 KB/s")).toBeTruthy();
    expect(within(groups).getByText("2 GB/s")).toBeTruthy();
    expect(within(groups).getByText("1")).toBeTruthy();
    expect(device.rxBytes).toBe(2e9);
  });
  it("leaves malformed/missing latest rates unavailable instead of reporting zero", () => {
    const fixture = activityFixture();
    const device = {
      ...fixture.devices[0],
      rxBytesPerSecond: NaN,
      txBytesPerSecond: -1,
    };
    const group = {
      ...fixture.groups[0],
      rxBytesPerSecond: Infinity,
      txBytesPerSecond: undefined,
    };
    hook.mockReturnValue(
      result({ ...fixture, devices: [device], groups: [group] }),
    );
    render(<DeviceActivityPanel />);
    const devices = screen.getByRole("table", { name: /设备流量 ·/ });
    const groups = screen.getByRole("table", { name: /系统接口分组/ });
    expect(within(devices).getAllByText("—")).toHaveLength(2);
    expect(within(groups).getAllByText("—")).toHaveLength(2);
    expect(within(devices).queryByText("0 B/s")).toBeNull();
  });
  it("changes real range and submits a bounded search with native Enter/form controls", () => {
    render(<DeviceActivityPanel />);
    fireEvent.change(screen.getByRole("combobox", { name: "时间范围" }), {
      target: { value: "7d" },
    });
    expect(hook).toHaveBeenLastCalledWith("7d", "", true);
    const search = screen.getByRole("searchbox", { name: "搜索设备" });
    expect(search).toHaveAttribute("maxlength", "64");
    fireEvent.change(search, {
      target: { value: "  " + "a".repeat(70) + "  " },
    });
    expect(hook).toHaveBeenLastCalledWith("7d", "", true);
    fireEvent.submit(search.closest("form")!);
    expect(hook).toHaveBeenLastCalledWith("7d", "a".repeat(64), true);
    fireEvent.click(screen.getByRole("button", { name: "清除搜索" }));
    expect(hook).toHaveBeenLastCalledWith("7d", "", true);
    expect(search).toHaveValue("");
  });
  it("refreshes only retained API reads and labels old data when a refresh fails", () => {
    hook.mockReturnValue(
      result(activityFixture(), { error: new Error("read failed") }),
    );
    render(<DeviceActivityPanel />);
    expect(screen.getByRole("alert")).toHaveTextContent(
      "保留此范围和筛选的上次成功记录，并非新采样",
    );
    expect(screen.getByRole("table", { name: /设备流量 ·/ })).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "刷新数据" }));
    expect(reload).toHaveBeenCalledOnce();
  });
  it.each([
    ["waiting", "等待积累"],
    ["unavailable", "统计不可用"],
  ] as const)(
    "keeps source visible in %s with no generated rows or zero rates",
    (state, label) => {
      const fixture = {
        ...activityFixture(),
        state,
        devices: [],
        groups: [],
        deviceCount: 0,
        matchedCount: 0,
      };
      hook.mockReturnValue(result(fixture));
      render(<DeviceActivityPanel />);
      expect(screen.getByText(label)).toBeTruthy();
      expect(
        screen.getAllByText("数据源：系统流量统计 (trafficd)").length,
      ).toBeGreaterThan(0);
      expect(screen.queryByRole("img")).toBeNull();
      expect(screen.queryByRole("table")).toBeNull();
      expect(screen.queryByText("0 B/s")).toBeNull();
      expect(screen.getByText("暂无时段活跃数据")).toBeTruthy();
    },
  );
  it("shows useful loading/error/disabled source states without replacing them with demos", () => {
    hook.mockReturnValue(result(undefined, { loading: true }));
    const { rerender } = render(<DeviceActivityPanel />);
    expect(screen.getByText("正在读取时段活跃数据")).toBeTruthy();
    expect(screen.getByRole("button", { name: "刷新数据" })).toBeDisabled();
    hook.mockReturnValue(result(undefined, { error: new Error("offline") }));
    rerender(<DeviceActivityPanel />);
    expect(screen.getByRole("alert")).toHaveTextContent("offline");
    hook.mockReturnValue(
      result({ ...activityFixture(), enabled: false, devices: [], groups: [] }),
    );
    rerender(<DeviceActivityPanel />);
    expect(
      screen.getAllByText("服务器未启用设备时段记录。").length,
    ).toBeGreaterThan(0);
    expect(screen.queryByText("演示数据")).toBeNull();
  });
  it("hides stale latest rates but retains historical byte totals with a clear warning", () => {
    hook.mockReturnValue(result({ ...activityFixture(), state: "stale" }));
    render(<DeviceActivityPanel />);
    expect(screen.getByText("采样陈旧")).toBeTruthy();
    expect(screen.getByText(/源采样或部分设备记录陈旧/)).toBeTruthy();
    const table = screen.getByRole("table", { name: /设备流量 ·/ });
    expect(within(table).getByText("1.02 KB")).toBeTruthy();
    expect(within(table).queryByText("10 B/s")).toBeNull();
    expect(within(table).getAllByText("—")).toHaveLength(2);
  });
  it("does not show baseline zero bytes for one group just because another group has measurements", () => {
    const fixture = activityFixture();
    hook.mockReturnValue(
      result({
        ...fixture,
        groups: [
          ...fixture.groups,
          {
            name: "baseline-interface",
            deviceCount: 1,
            rxBytes: 0,
            txBytes: 0,
            coverageSeconds: 0,
          },
        ],
      }),
    );
    render(<DeviceActivityPanel />);
    const table = screen.getByRole("table", { name: /系统接口分组/ });
    const baselineRow = within(table)
      .getByText("baseline-interface")
      .closest("tr")!;
    expect(within(baselineRow).queryByText("0 B")).toBeNull();
    expect(within(baselineRow).getAllByText("—")).toHaveLength(4);
    expect(within(table).getByText("1.02 KB")).toBeTruthy();
  });
  it("suppresses latest rates while source unavailable even if historical totals remain", () => {
    hook.mockReturnValue(
      result({ ...activityFixture(), state: "unavailable" }),
    );
    render(<DeviceActivityPanel />);
    expect(screen.queryAllByText("10 B/s")).toHaveLength(0);
    expect(screen.queryAllByText("20 B/s")).toHaveLength(0);
    expect(screen.getAllByText("1.02 KB").length).toBeGreaterThan(0);
  });
  it("does not report baseline counters as measured zero flow or rate", () => {
    const fixture = activityFixture();
    hook.mockReturnValue(
      result({
        ...fixture,
        devices: [
          {
            ...fixture.devices[0],
            samples: [],
            rxBytes: 0,
            txBytes: 0,
            coverageSeconds: 0,
            rxBytesPerSecond: undefined,
            txBytesPerSecond: undefined,
          },
        ],
        groups: [],
      }),
    );
    render(<DeviceActivityPanel />);
    expect(
      screen.getAllByText(/首个计数器读数只建立基线/).length,
    ).toBeGreaterThan(0);
    const table = screen.getByRole("table", { name: /设备流量 ·/ });
    expect(within(table).queryByText("0 B")).toBeNull();
    expect(within(table).queryByText("0 B/s")).toBeNull();
    expect(within(table).getAllByText("—")).toHaveLength(4);
  });
  it("shows truncation and group scope, caps returned table rows and keeps scroll regions keyboard-focusable", () => {
    const fixture = activityFixture();
    const devices = Array.from({ length: 32 }, (_, index) => ({
      ...fixture.devices[0],
      id: `02:00:00:00:00:${index.toString(16).padStart(2, "0")}`,
      name: `终端 ${index}`,
    }));
    hook.mockReturnValue(
      result({
        ...fixture,
        devices,
        deviceCount: 64,
        matchedCount: 64,
        truncated: true,
      }),
    );
    render(<DeviceActivityPanel />);
    expect(
      screen.getByText(/匹配 64 个设备，表格仅返回按流量排序的前 32 个/),
    ).toBeTruthy();
    expect(screen.getByText(/接口分组覆盖全部匹配设备/)).toBeTruthy();
    const table = screen.getByRole("table", { name: /设备流量 ·/ });
    expect(within(table).getAllByRole("row")).toHaveLength(33);
    expect(
      screen.getByRole("region", { name: "设备流量明细表，可横向滚动" }),
    ).toHaveAttribute("tabindex", "0");
    expect(
      screen.getByRole("region", { name: "接口分组表，可横向滚动" }),
    ).toHaveAttribute("tabindex", "0");
  });
  it("labels no matches and supports local device names with read-only detail callbacks", () => {
    const onSelectDevice = vi.fn();
    const getDeviceName = vi.fn(() => "书房终端");
    const { rerender } = render(
      <DeviceActivityPanel
        onSelectDevice={onSelectDevice}
        getDeviceName={getDeviceName}
      />,
    );
    expect(getDeviceName).toHaveBeenCalledWith("02:00:00:00:00:01", "测试终端");
    fireEvent.click(screen.getByRole("button", { name: "查看设备 书房终端" }));
    expect(onSelectDevice).toHaveBeenCalledWith("02:00:00:00:00:01");
    fireEvent.click(screen.getByRole("img"));
    expect(onSelectDevice).toHaveBeenCalledTimes(2);
    expect(onSelectDevice).toHaveBeenLastCalledWith("02:00:00:00:00:01");
    hook.mockReturnValue(
      result({
        ...activityFixture(),
        devices: [],
        groups: [],
        matchedCount: 0,
      }),
    );
    fireEvent.change(screen.getByRole("searchbox"), {
      target: { value: "none" },
    });
    fireEvent.click(screen.getByRole("button", { name: "应用搜索" }));
    rerender(<DeviceActivityPanel />);
    expect(screen.getAllByText(/没有匹配“none”的设备/).length).toBeGreaterThan(
      0,
    );
  });
  it("disables reads in demo and inactive views", () => {
    hook.mockReturnValue(result());
    const { rerender } = render(<DeviceActivityPanel active={false} />);
    expect(hook).toHaveBeenLastCalledWith("24h", "", false);
    expect(screen.getByRole("button", { name: "刷新数据" })).toBeDisabled();
    rerender(<DeviceActivityPanel demo />);
    expect(hook).toHaveBeenLastCalledWith("24h", "", false);
    expect(screen.getAllByText("演示数据").length).toBeGreaterThan(0);
    expect(screen.queryByRole("button", { name: "刷新数据" })).toBeNull();
    expect(screen.queryByRole("table", { name: /设备流量 ·/ })).toBeNull();
  });
});
