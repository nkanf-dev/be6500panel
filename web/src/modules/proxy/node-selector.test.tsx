import { Effect } from "effect";
import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { api } from "../../lib/api";
import type { ProxyNodes } from "../../lib/contracts";
import { runtimeStatus } from "../production-fixtures.test-data";
import type { RuntimeController } from "../runtime/use-runtime";
import { NodeSelector } from "./node-selector";

const nativeConfig = (
  ipv6 = "direct",
  ports = { mixed: 2080, tproxy: 7893, dns: 6450 },
) =>
  JSON.stringify({
    inbounds: [
      {
        type: "mixed",
        tag: "mixed-in",
        listen: "192.168.31.1",
        listen_port: ports.mixed,
      },
      {
        type: "tproxy",
        tag: "tproxy-in",
        listen: ipv6 === "follow" ? "::" : "127.0.0.1",
        listen_port: ports.tproxy,
      },
      {
        type: "direct",
        tag: "dns-in",
        listen: ipv6 === "follow" ? "::" : "192.168.31.1",
        listen_port: ports.dns,
      },
    ],
    route: {
      rules:
        ipv6 === "follow"
          ? []
          : [
              ipv6 === "block"
                ? { ip_version: 6, action: "reject" }
                : { ip_version: 6, outbound: "direct" },
            ],
    },
  });

const nodes: ProxyNodes = {
  selectedNodeId: "node-126",
  diagnostics: [],
  nodes: Array.from({ length: 150 }, (_, index) => ({
    id: `node-${index + 1}`,
    label: `${index === 125 ? "东京" : "Example"} Node ${String(index + 1).padStart(3, "0")}`,
    server: `edge-${index + 1}.example.test`,
    port: index === 42 ? 8443 : 443,
    protocol: index % 2 ? "trojan" : "vless",
    transport: index % 3 ? "tcp" : "ws",
    reality: index % 2 === 0,
    vision: false,
    utls: true,
    udp: true,
  })),
};

function runtime(
  overrides: Partial<RuntimeController> = {},
): RuntimeController {
  return {
    service: "sing-box",
    status: runtimeStatus,
    enabled: true,
    loading: false,
    pending: false,
    error: undefined,
    result: undefined,
    refresh: vi.fn(),
    run: vi.fn(async (load) => {
      await Effect.runPromise(load());
      return true;
    }),
    ...overrides,
  };
}
function setup(overrides: Partial<RuntimeController> = {}) {
  const controller = runtime(overrides);
  const selected = vi.fn();
  const view = render(
    <NodeSelector nodes={nodes} runtime={controller} onSelected={selected} />,
  );
  return { ...view, controller, selected };
}
function visibleRows() {
  return within(
    screen.getByRole("list", { name: "代理节点列表" }),
  ).getAllByRole("button", { name: /^选择节点 / });
}
function saveButton() {
  return screen.getByRole("button", { name: "保存节点配置" });
}

beforeEach(() => {
  window.localStorage.clear();
  window.sessionStorage.clear();
  vi.spyOn(api, "runtimeConfig").mockReturnValue(
    Effect.succeed({
      service: "sing-box",
      config: nativeConfig(),
      generation: 4,
    }),
  );
  vi.spyOn(api, "proxySelect").mockReturnValue(
    Effect.succeed({
      status: runtimeStatus,
      configSHA256: "b".repeat(64),
      diagnostics: [],
    }),
  );
});

describe("bounded node browsing", () => {
  it("renders 20 of 150 nodes and supports native pagination and an active-node jump", async () => {
    const user = userEvent.setup();
    setup();
    expect(visibleRows()).toHaveLength(20);
    expect(screen.getByLabelText("页码")).toHaveValue("1");
    expect(screen.getByRole("button", { name: "上一页" })).toBeDisabled();
    await user.click(screen.getByRole("button", { name: "下一页" }));
    expect(visibleRows()).toHaveLength(20);
    expect(
      screen.getByRole("button", { name: "选择节点 Example Node 021" }),
    ).toBeInTheDocument();
    await user.selectOptions(screen.getByLabelText("页码"), "8");
    expect(visibleRows()).toHaveLength(10);
    expect(screen.getByRole("button", { name: "下一页" })).toBeDisabled();
    await user.click(screen.getByRole("button", { name: "定位当前节点" }));
    const active = screen.getByRole("button", {
      name: "选择节点 东京 Node 126",
    });
    expect(active).toHaveAttribute("aria-pressed", "true");
    expect(active).toHaveFocus();
    expect(screen.getByLabelText("页码")).toHaveValue("7");
    expect(api.proxySelect).not.toHaveBeenCalled();
  });

  it("searches names, label regions, endpoints and protocols without changing selection", async () => {
    const user = userEvent.setup();
    setup();
    const search = screen.getByLabelText("搜索节点");
    await user.type(search, "东京");
    expect(visibleRows()).toHaveLength(1);
    expect(visibleRows()[0]).toHaveAttribute("aria-pressed", "true");
    fireEvent.change(search, {
      target: { value: "EDGE-43.EXAMPLE.TEST:8443" },
    });
    expect(visibleRows()).toHaveLength(1);
    expect(visibleRows()[0]).toHaveAccessibleName("选择节点 Example Node 043");
    fireEvent.change(search, { target: { value: "TROJAN" } });
    expect(visibleRows()).toHaveLength(20);
    expect(screen.getByText(/75 个匹配/)).toBeInTheDocument();
    fireEvent.change(search, { target: { value: "absent node" } });
    expect(
      screen.queryAllByRole("button", { name: /^选择节点 / }),
    ).toHaveLength(0);
    expect(screen.getByText("没有匹配的节点")).toBeInTheDocument();
    expect(saveButton()).toBeEnabled();
    await user.click(screen.getByRole("button", { name: "清除搜索" }));
    expect(visibleRows()).toHaveLength(20);
    expect(search).toHaveValue("");
  });

  it("combines protocol and transport filters, resets filters and retains a hidden draft", async () => {
    const user = userEvent.setup();
    setup();
    await user.click(
      screen.getByRole("button", { name: "选择节点 Example Node 003" }),
    );
    await user.selectOptions(screen.getByLabelText("协议筛选"), "trojan");
    await user.selectOptions(screen.getByLabelText("传输筛选"), "ws");
    expect(visibleRows()).toHaveLength(20);
    expect(screen.getByText(/25 个匹配/)).toBeInTheDocument();
    expect(
      screen.getByText("已选节点不在当前筛选结果中，保存仍使用此节点。"),
    ).toBeInTheDocument();
    expect(api.proxySelect).not.toHaveBeenCalled();
    await user.click(saveButton());
    expect(api.proxySelect).toHaveBeenCalledWith({
      nodeId: "node-3",
      ipv6: "direct",
      failure: "direct",
      ports: { mixed: 2080, tproxy: 7893, dns: 6450 },
    });
    expect(screen.getByText("节点配置已保存")).toBeInTheDocument();
    expect(screen.queryByText(/SHA-256/)).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "重置筛选" }));
    expect(screen.getByLabelText("协议筛选")).toHaveValue("");
    expect(screen.getByLabelText("传输筛选")).toHaveValue("");
    expect(
      screen.getByRole("button", { name: "选择节点 Example Node 003" }),
    ).toHaveAttribute("aria-pressed", "true");
  });

  it("keeps favorites browser-local, supports favorite filtering and removes missing imported IDs", async () => {
    const user = userEvent.setup();
    const view = setup();
    await user.click(
      screen.getByRole("button", { name: "收藏节点 Example Node 004" }),
    );
    await user.click(screen.getByLabelText("仅收藏"));
    expect(visibleRows()).toHaveLength(1);
    expect(api.proxySelect).not.toHaveBeenCalled();
    expect(
      screen.getByRole("button", { name: "取消收藏节点 Example Node 004" }),
    ).toHaveAttribute("aria-pressed", "true");
    const stored = window.localStorage.getItem(
      "be6500panel.proxy.node-favorites",
    );
    expect(JSON.parse(stored!)).toEqual(["node-4"]);
    expect(stored).not.toContain("example.test");
    view.unmount();
    const next = setup();
    expect(screen.getByLabelText("仅收藏")).toBeChecked();
    expect(visibleRows()).toHaveLength(1);
    next.rerender(
      <NodeSelector
        nodes={{
          ...nodes,
          nodes: nodes.nodes.filter((node) => node.id !== "node-4"),
        }}
        runtime={next.controller}
        onSelected={next.selected}
      />,
    );
    expect(
      screen.queryAllByRole("button", { name: /^选择节点 / }),
    ).toHaveLength(0);
    expect(screen.getByText("没有匹配的节点")).toBeInTheDocument();
    await waitFor(() =>
      expect(
        JSON.parse(
          window.localStorage.getItem("be6500panel.proxy.node-favorites")!,
        ),
      ).toEqual([]),
    );
  });

  it("keeps page, filters and chosen ID across re-renders and a tab remount", async () => {
    const user = userEvent.setup();
    const view = setup();
    await user.type(screen.getByLabelText("搜索节点"), "Example");
    await user.selectOptions(screen.getByLabelText("协议筛选"), "vless");
    await user.selectOptions(screen.getByLabelText("传输筛选"), "tcp");
    await user.selectOptions(screen.getByLabelText("页码"), "2");
    await user.click(visibleRows()[3]);
    const chosenName = visibleRows()[3].getAttribute("aria-label");
    view.rerender(
      <NodeSelector
        nodes={{ ...nodes }}
        runtime={view.controller}
        onSelected={view.selected}
      />,
    );
    expect(screen.getByLabelText("页码")).toHaveValue("2");
    view.unmount();
    setup();
    expect(screen.getByLabelText("搜索节点")).toHaveValue("Example");
    expect(screen.getByLabelText("协议筛选")).toHaveValue("vless");
    expect(screen.getByLabelText("传输筛选")).toHaveValue("tcp");
    expect(screen.getByLabelText("页码")).toHaveValue("2");
    expect(screen.getByRole("button", { name: chosenName! })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
  });

  it("uses name-based region shortcuts and lets the whole card select without favorite side effects", async () => {
    const user = userEvent.setup();
    setup();
    await user.click(screen.getByRole("button", { name: "日本" }));
    expect(visibleRows()).toHaveLength(1);
    expect(visibleRows()[0]).toHaveAccessibleName("选择节点 东京 Node 126");
    expect(screen.getByText(/不推断实际地理位置/)).toBeInTheDocument();
    expect(screen.getByText(/延迟未测/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "其他" }));
    const selected = visibleRows()[0];
    await user.click(within(selected).getByText("Example Node 001"));
    expect(selected).toHaveAttribute("aria-pressed", "true");
    await user.click(
      screen.getByRole("button", { name: "收藏节点 Example Node 002" }),
    );
    expect(selected).toHaveAttribute("aria-pressed", "true");
    expect(
      screen.getByRole("button", { name: "选择节点 Example Node 002" }),
    ).toHaveAttribute("aria-pressed", "false");
    expect(api.proxySelect).not.toHaveBeenCalled();
  });

  it("does not silently switch a removed selected ID to the committed node", async () => {
    const user = userEvent.setup();
    const view = setup();
    await user.click(
      screen.getByRole("button", { name: "选择节点 Example Node 001" }),
    );
    view.rerender(
      <NodeSelector
        nodes={{ ...nodes, nodes: nodes.nodes.slice(1) }}
        runtime={view.controller}
        onSelected={view.selected}
      />,
    );
    expect(saveButton()).toBeDisabled();
    expect(
      screen.getByText("已选节点已不在当前订阅中，请重新选择。"),
    ).toBeInTheDocument();
    fireEvent.submit(saveButton().closest("form")!);
    expect(api.proxySelect).not.toHaveBeenCalled();
    await user.click(screen.getByRole("button", { name: "恢复当前节点选择" }));
    expect(saveButton()).toBeEnabled();
    await user.click(saveButton());
    expect(api.proxySelect).toHaveBeenCalledWith(
      expect.objectContaining({ nodeId: "node-126" }),
    );
  });
});

describe("explicit compiler submission", () => {
  it("hydrates accepted follow/custom ports before saving and preserves them on a fresh session", async () => {
    const user = userEvent.setup();
    const custom = { mixed: 3200, tproxy: 3201, dns: 3202 };
    vi.mocked(api.runtimeConfig).mockReturnValue(
      Effect.succeed({
        service: "sing-box",
        config: nativeConfig("follow", custom),
        generation: 4,
      }),
    );
    const view = setup();
    expect(saveButton()).toBeDisabled();
    await waitFor(() => expect(saveButton()).toBeEnabled());
    expect(screen.getByLabelText("节点 IPv6 策略")).toHaveValue("follow");
    expect(screen.getByLabelText("dns 监听端口")).toHaveValue(3202);
    await user.click(
      screen.getByRole("button", { name: "选择节点 Example Node 005" }),
    );
    await user.click(saveButton());
    expect(api.proxySelect).toHaveBeenCalledWith({
      nodeId: "node-5",
      ipv6: "follow",
      failure: "direct",
      ports: custom,
    });
    view.unmount();
    window.sessionStorage.clear();
    setup();
    await waitFor(() => expect(saveButton()).toBeEnabled());
    expect(screen.getByLabelText("节点 IPv6 策略")).toHaveValue("follow");
    expect(screen.getByLabelText("mixed 监听端口")).toHaveValue(3200);
    expect(api.proxySelect).toHaveBeenCalledTimes(1);
    expect(
      window.sessionStorage.getItem("be6500panel.proxy.node-view"),
    ).not.toContain("inbounds");
  });

  it("blocks unsupported accepted listeners instead of silently replacing their settings", async () => {
    const original = JSON.parse(nativeConfig());
    original.inbounds[0].listen = "127.0.0.1";
    vi.mocked(api.runtimeConfig).mockReturnValue(
      Effect.succeed({
        service: "sing-box",
        config: JSON.stringify(original),
        generation: 4,
      }),
    );
    setup();
    expect(
      await screen.findByText(/当前配置的监听或 IPv6 规则不受节点选择器支持/),
    ).toBeInTheDocument();
    expect(saveButton()).toBeDisabled();
    fireEvent.submit(saveButton().closest("form")!);
    expect(api.proxySelect).not.toHaveBeenCalled();
  });

  it("lets keyboard users select and apply, preserves IPv6 and custom ports across remounts", async () => {
    const user = userEvent.setup();
    const view = setup();
    screen.getByRole("button", { name: "选择节点 Example Node 008" }).focus();
    await user.keyboard(" ");
    expect(
      screen.getByRole("button", { name: "选择节点 Example Node 008" }),
    ).toHaveAttribute("aria-pressed", "true");
    await user.selectOptions(screen.getByLabelText("节点 IPv6 策略"), "block");
    expect(screen.getByText(/高级设置/).closest("details")).not.toHaveAttribute(
      "open",
    );
    await user.click(screen.getByText(/高级设置/));
    fireEvent.change(screen.getByLabelText("mixed 监听端口"), {
      target: { value: "3100" },
    });
    fireEvent.change(screen.getByLabelText("tproxy 监听端口"), {
      target: { value: "3101" },
    });
    fireEvent.change(screen.getByLabelText("dns 监听端口"), {
      target: { value: "3102" },
    });
    view.unmount();
    const remount = setup();
    await waitFor(() =>
      expect(screen.getByLabelText("节点 IPv6 策略")).toBeEnabled(),
    );
    expect(screen.getByLabelText("节点 IPv6 策略")).toHaveValue("block");
    expect(screen.getByLabelText("mixed 监听端口")).toHaveValue(3100);
    saveButton().focus();
    await user.keyboard("{Enter}");
    expect(api.proxySelect).toHaveBeenCalledExactlyOnceWith({
      nodeId: "node-8",
      ipv6: "block",
      failure: "direct",
      ports: { mixed: 3100, tproxy: 3101, dns: 3102 },
    });
    expect(remount.selected).toHaveBeenCalledTimes(1);
  });

  it.each([
    ["importing", { importing: true }],
    ["runtime pending", { pending: true }],
    ["disabled runtime", { enabled: false }],
    [
      "missing artifact",
      { status: { ...runtimeStatus, artifactAvailable: false } },
    ],
  ])("blocks submission when %s", (_label, guard) => {
    const controller = runtime("importing" in guard ? {} : guard);
    render(
      <NodeSelector
        nodes={nodes}
        runtime={controller}
        importing={"importing" in guard}
        onSelected={vi.fn()}
      />,
    );
    expect(saveButton()).toBeDisabled();
    fireEvent.submit(saveButton().closest("form")!);
    expect(api.proxySelect).not.toHaveBeenCalled();
    if ("importing" in guard || "pending" in guard) {
      expect(visibleRows()[0]).toBeDisabled();
      expect(screen.getByLabelText("节点 IPv6 策略")).toBeDisabled();
    }
  });

  it("prevents a second apply while an explicit request is in flight", async () => {
    let complete!: (value: boolean) => void;
    const controller = runtime({
      run: vi.fn(
        () =>
          new Promise<boolean>((resolve) => {
            complete = resolve;
          }),
      ),
    });
    render(
      <NodeSelector nodes={nodes} runtime={controller} onSelected={vi.fn()} />,
    );
    await waitFor(() => expect(saveButton()).toBeEnabled());
    const form = saveButton().closest("form")!;
    fireEvent.submit(form);
    expect(screen.getByRole("button", { name: "正在保存…" })).toBeDisabled();
    expect(visibleRows()[0]).toBeDisabled();
    fireEvent.submit(form);
    expect(controller.run).toHaveBeenCalledTimes(1);
    await act(async () => {
      complete(true);
    });
    expect(saveButton()).toBeEnabled();
  });

  it("labels the running-core action without showing technical Commit or hashes", async () => {
    setup({ status: { ...runtimeStatus, state: "running" } });
    await waitFor(() =>
      expect(
        screen.getByRole("button", { name: "保存并应用节点" }),
      ).toBeEnabled(),
    );
    expect(screen.queryByText(/Commit|SHA-256/)).not.toBeInTheDocument();
  });

  it("works with unavailable browser storage and no nodes", () => {
    vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new Error("disabled");
    });
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("disabled");
    });
    render(
      <NodeSelector
        nodes={{ nodes: [], selectedNodeId: "", diagnostics: [] }}
        runtime={runtime()}
        onSelected={vi.fn()}
      />,
    );
    expect(screen.getByText("暂无节点")).toBeInTheDocument();
    expect(saveButton()).toBeDisabled();
    expect(
      screen.queryAllByRole("button", { name: /^选择节点 / }),
    ).toHaveLength(0);
  });
});
