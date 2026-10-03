import {
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { DeviceWorkspace } from "./device-workspace";
import { DeviceLabelsProvider } from "./device-labels";
import type { DeviceHistory } from "./device-model";
import { routerSnapshot, jsonResponse } from "../production-fixtures.test-data";

vi.mock("../../components/visualizations/EChart", () => ({
  EChart: ({ label }: { label: string }) => (
    <div role="img" aria-label={label} />
  ),
}));
const mac = "02:00:00:00:00:20";
const now = new Date().toISOString();
const history: DeviceHistory = {
  range: "24h",
  state: "ok",
  source: "trafficd",
  direction: "vendor-rx-tx",
  sampledAt: now,
  resolutionSeconds: 60,
  deviceCount: 1,
  matchedCount: 1,
  truncated: false,
  devices: [
    {
      id: mac,
      name: "test-client",
      addresses: ["192.0.2.20"],
      interface: "wl0",
      associated: true,
      lastSeen: now,
      stale: false,
      rxBytes: 120,
      txBytes: 60,
      coverageSeconds: 60,
      rawRXBytes: 1200,
      rawTXBytes: 600,
      counters: [{ address: "192.0.2.20", rxBytes: 1200, txBytes: 600 }],
      addressConflicts: [],
      links: [
        {
          interface: "wl0",
          protocol: "802.11be",
          mld: true,
          negotiatedRX: "1200M",
          negotiatedTX: "960M",
        },
      ],
      samples: [{ time: now, rxBytes: 120, txBytes: 60, coverageSeconds: 60 }],
    },
  ],
};
const initial = {
  revision: 4,
  devices: { [mac]: { label: "办公电脑", note: "书房", tags: ["办公"] } },
};
const snapshot = { ...routerSnapshot, sampledAt: now };
const mount = (
  props: Partial<React.ComponentProps<typeof DeviceWorkspace>> = {},
) =>
  render(
    <DeviceLabelsProvider initial={initial}>
      <DeviceWorkspace snapshot={snapshot} activity={history} {...props} />
    </DeviceLabelsProvider>,
  );
let fetcher: ReturnType<typeof vi.fn>;
beforeEach(() => {
  fetcher = vi.fn();
  vi.stubGlobal("fetch", fetcher);
});

describe("device workspace", () => {
  it("opens MAC detail and combines lease, links, bytes, notes and explicit shortcuts", async () => {
    const configure = vi.fn();
    const select = vi.fn();
    mount({ onConfigure: configure, onSelectDevice: select });
    await userEvent.click(
      screen.getByRole("button", { name: "查看 办公电脑 详情" }),
    );
    const detail = screen.getByRole("region", { name: "设备详情" });
    expect(select).toHaveBeenCalledWith(mac);
    expect(within(detail).getByText("书房")).toBeVisible();
    expect(within(detail).getAllByText("wl0")).toHaveLength(2);
    expect(within(detail).getByText("802.11be · MLO")).toBeInTheDocument();
    expect(
      within(detail).getByText("未导出信号 / 未导出噪声"),
    ).toBeInTheDocument();
    expect(within(detail).getByText("1200 / 600")).toBeInTheDocument();
    expect(
      within(detail).getByRole("img", { name: /实测速率曲线/ }),
    ).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "静态地址配置" }));
    await userEvent.click(screen.getByRole("button", { name: "端口映射配置" }));
    expect(configure.mock.calls).toEqual([
      ["dhcp", mac],
      ["firewall", mac],
    ]);
    expect(fetcher).not.toHaveBeenCalled();
  });

  it("filters names, IPs, tags and notes while keeping selected MAC visible", async () => {
    mount();
    await userEvent.click(
      screen.getByRole("button", { name: "查看 办公电脑 详情" }),
    );
    await userEvent.type(
      screen.getByRole("textbox", { name: "搜索设备" }),
      "nothing-matches",
    );
    expect(screen.getByText("没有匹配设备")).toBeVisible();
    expect(screen.getByText("当前查看：办公电脑")).toBeVisible();
    expect(
      screen.getByRole("region", { name: "设备详情" }),
    ).toBeInTheDocument();
    for (const query of ["书房", "办公", "192.0.2.20", mac]) {
      fireEvent.change(screen.getByRole("textbox", { name: "搜索设备" }), {
        target: { value: query },
      });
      expect(
        screen.getByRole("button", { name: "查看 办公电脑 详情" }),
      ).toBeInTheDocument();
    }
    expect(fetcher).not.toHaveBeenCalled();
  });

  it("pages no more than 25 rows and limits compare to eight devices", async () => {
    const many = Array.from({ length: 31 }, (_, index) => ({
      ...snapshot.devices[0],
      mac: `02:00:00:00:01:${index.toString(16).padStart(2, "0").toUpperCase()}`,
      hostname: `client-${index}`,
      ip: `192.0.2.${index + 1}`,
    }));
    mount({ snapshot: { ...snapshot, devices: many }, activity: undefined });
    expect(screen.getAllByRole("checkbox")).toHaveLength(25);
    const inputs = screen.getAllByRole("checkbox");
    for (const input of inputs.slice(0, 8)) await userEvent.click(input);
    expect(inputs[8]).toBeDisabled();
    await userEvent.click(screen.getByRole("button", { name: "对比设备" }));
    expect(
      screen.getByRole("region", { name: "设备对比详情" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("heading", { name: "设备对比 · 8 个" }),
    ).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "设备下一页" }));
    expect(screen.getAllByRole("checkbox")).toHaveLength(6);
    expect(
      screen.getByRole("region", { name: "设备对比详情" }),
    ).toBeInTheDocument();
    expect(fetcher).not.toHaveBeenCalled();
  });

  it("compares two actual histories and range changes never write or mix old-range counts", async () => {
    const otherMAC = "02:00:00:00:00:21";
    const other = {
      ...history.devices[0],
      id: otherMAC,
      name: "tv",
      addresses: ["192.0.2.21"],
      rxBytes: 900,
      txBytes: 300,
    };
    const rangeChange = vi.fn();
    mount({
      activity: {
        ...history,
        devices: [...history.devices, other],
        deviceCount: 2,
        matchedCount: 2,
      },
      onRangeChange: rangeChange,
    });
    await userEvent.click(
      screen.getByRole("checkbox", { name: "对比 办公电脑" }),
    );
    await userEvent.click(screen.getByRole("checkbox", { name: "对比 tv" }));
    await userEvent.click(screen.getByRole("button", { name: "对比设备" }));
    expect(
      screen.getByRole("heading", { name: "设备流量对比" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("table", { name: "所选设备 · 同一时间范围数据汇总" }),
    ).toHaveTextContent("办公电脑");
    expect(
      screen.getByRole("table", { name: "所选设备 · 同一时间范围数据汇总" }),
    ).toHaveTextContent("tv");
    await userEvent.click(screen.getByRole("button", { name: "7 天" }));
    expect(rangeChange).toHaveBeenCalledWith("7d");
    expect(screen.getByText(/正在等待所选时间范围记录/)).toBeInTheDocument();
    expect(
      screen.queryByRole("img", { name: /实测速率曲线/ }),
    ).not.toBeInTheDocument();
    expect(fetcher).not.toHaveBeenCalled();
  });

  it("edits only on explicit Save and updates list, detail and charts aliases after actual POST", async () => {
    const user = userEvent.setup();
    fetcher.mockImplementation(async (_url: string, init: RequestInit) => {
      const body = JSON.parse(String(init.body));
      return jsonResponse({
        revision: 5,
        devices: {
          [mac]: { label: body.label, note: body.note, tags: body.tags },
        },
      });
    });
    mount();
    await user.click(
      screen.getByRole("button", { name: "查看 办公电脑 详情" }),
    );
    await user.click(screen.getByRole("button", { name: "编辑设备备注" }));
    const dialog = screen.getByRole("dialog", { name: "编辑设备备注" });
    await user.clear(
      within(dialog).getByRole("textbox", { name: "设备备注名称" }),
    );
    await user.type(
      within(dialog).getByRole("textbox", { name: "设备备注名称" }),
      "我的 MacBook",
    );
    await user.clear(within(dialog).getByRole("textbox", { name: "详细备注" }));
    await user.type(
      within(dialog).getByRole("textbox", { name: "详细备注" }),
      "token=allowed LAN note",
    );
    expect(fetcher).not.toHaveBeenCalled();
    await user.click(within(dialog).getByRole("button", { name: "保存备注" }));
    await waitFor(() =>
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument(),
    );
    expect(fetcher).toHaveBeenCalledTimes(1);
    expect(fetcher.mock.calls[0][0]).toBe("/api/devices/annotations");
    const sent = JSON.parse(String(fetcher.mock.calls[0][1].body));
    expect(sent).toEqual({
      mac,
      label: "我的 MacBook",
      note: "token=allowed LAN note",
      tags: ["办公"],
      expectedRevision: 4,
    });
    expect(
      screen.getByRole("button", { name: "查看 我的 MacBook 详情" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("heading", { name: "我的 MacBook" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("table", { name: "所选设备 · 同一时间范围数据汇总" }),
    ).toHaveTextContent("我的 MacBook");
  });

  it("cancel discards local edit and never POSTs", async () => {
    mount();
    await userEvent.click(
      screen.getByRole("button", { name: "编辑 办公电脑 备注" }),
    );
    const dialog = screen.getByRole("dialog");
    fireEvent.change(
      within(dialog).getByRole("textbox", { name: "设备备注名称" }),
      { target: { value: "unsaved" } },
    );
    await userEvent.click(within(dialog).getByRole("button", { name: "取消" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "查看 办公电脑 详情" }),
    ).toBeInTheDocument();
    expect(fetcher).not.toHaveBeenCalled();
  });

  it("CAS conflict preserves editor and requires deliberate latest reload before new save", async () => {
    let posts = 0;
    fetcher.mockImplementation(async (_url: string, init: RequestInit) =>
      init.method === "POST" && ++posts === 1
        ? jsonResponse(
            { error: { code: "revision_conflict", message: "备注已更新" } },
            409,
          )
        : init.method === "POST"
          ? jsonResponse({ revision: 7, devices: {} })
          : jsonResponse({
              revision: 6,
              devices: {
                [mac]: { label: "另一会话", note: "remote", tags: [] },
              },
            }),
    );
    mount();
    await userEvent.click(
      screen.getByRole("button", { name: "编辑 办公电脑 备注" }),
    );
    const dialog = screen.getByRole("dialog");
    fireEvent.change(
      within(dialog).getByRole("textbox", { name: "设备备注名称" }),
      { target: { value: "my edit" } },
    );
    await userEvent.click(
      within(dialog).getByRole("button", { name: "保存备注" }),
    );
    await screen.findByText("备注已被另一会话更新");
    await waitFor(() =>
      expect(
        screen.getByRole("button", { name: "载入最新备注" }),
      ).toBeEnabled(),
    );
    expect(
      within(dialog).getByRole("textbox", { name: "设备备注名称" }),
    ).toHaveValue("my edit");
    expect(posts).toBe(1);
    await userEvent.click(screen.getByRole("button", { name: "载入最新备注" }));
    expect(
      within(dialog).getByRole("textbox", { name: "设备备注名称" }),
    ).toHaveValue("另一会话");
    await userEvent.click(
      within(dialog).getByRole("button", { name: "保存备注" }),
    );
    await waitFor(() => expect(posts).toBe(2));
    expect(
      JSON.parse(
        String(
          fetcher.mock.calls.filter(([, init]) => init.method === "POST")[1][1]
            .body,
        ),
      ).expectedRevision,
    ).toBe(6);
  });
  it("forwards bounded search to the complete source and distinguishes loaded rows from total", async () => {
    const search = vi.fn();
    mount({
      activity: {
        ...history,
        deviceCount: 128,
        matchedCount: 128,
        truncated: true,
      },
      onSearchChange: search,
    });
    expect(screen.getByText(/已载入 \/ 源统计 128/)).toBeInTheDocument();
    expect(screen.getByText(/完整统计源查找/)).toBeInTheDocument();
    fireEvent.change(screen.getByRole("textbox", { name: "搜索设备" }), {
      target: { value: "device128" },
    });
    expect(search).toHaveBeenLastCalledWith("device128");
    fireEvent.change(screen.getByRole("textbox", { name: "搜索设备" }), {
      target: { value: "x".repeat(80) },
    });
    expect(search).toHaveBeenLastCalledWith("x".repeat(64));
    expect(fetcher).not.toHaveBeenCalled();
  });
});
