import { Schema } from "effect";
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
import { runRequest } from "../../lib/api";
import {
  ProxyCaptureSchema,
  type ProxyCapture,
  type ProxyNodes,
} from "../../lib/contracts";
import { ProxyPage } from "../proxy";
import {
  jsonResponse,
  proxyNodes,
  runtimeStatus,
} from "../production-fixtures.test-data";
import type { RuntimeController } from "../runtime/use-runtime";
import { GatewayPanel } from "./gateway-panel";

vi.mock("../../app/console-context", () => ({
  useOptionalConsole: () => undefined,
  useConsole: () => ({ health: { mode: "host" } }),
}));

const nodes: ProxyNodes = {
  ...proxyNodes,
  selectedNodeId: "node-synthetic",
  revision: "test-revision",
};
const off: ProxyCapture = {
  active: false,
  desired: false,
  scope: "gateway",
  state: "inactive",
  lanIPv4Prefixes: ["192.168.31.0/24"],
  clients: [],
  ipv6: "direct",
  commands: 0,
};
const active: ProxyCapture = {
  ...off,
  active: true,
  desired: true,
  state: "active",
  scopeState: "current",
  installedLanIPv4Prefixes: ["192.168.31.0/24"],
  commands: 8,
};
const emptyProbes = {
  revision: "test-revision",
  available: true,
  target: "https://www.gstatic.com/generate_204",
  running: false,
  results: [],
  limits: { maxNodes: 256, concurrency: 1, timeoutMs: 3000 },
};
function runtime(
  overrides: Partial<RuntimeController> = {},
): RuntimeController {
  return {
    service: "sing-box",
    status: { ...runtimeStatus, state: "running" },
    enabled: true,
    loading: false,
    pending: false,
    error: undefined,
    result: undefined,
    refresh: vi.fn(),
    run: vi.fn(async (load) => {
      await runRequest(load());
      return true;
    }),
    ...overrides,
  };
}
function fakeAPI(initial: ProxyCapture = off) {
  let capture = initial;
  const fetch = vi.fn(async (url: string, init?: RequestInit) => {
    if (url === "/api/proxy/node-probes") return jsonResponse(emptyProbes);
    if (url === "/api/proxy/capture") {
      if (init?.method === "POST") capture = active;
      if (init?.method === "DELETE") capture = off;
      return jsonResponse(capture);
    }
    if (url === "/api/runtime/start")
      return jsonResponse({ ...runtimeStatus, state: "running" });
    throw new Error(`Unexpected fake API request: ${url}`);
  });
  vi.stubGlobal("fetch", fetch);
  return fetch;
}
function mount(controller = runtime(), pending = false) {
  const onPending = vi.fn();
  const onSetup = vi.fn();
  const view = render(
    <GatewayPanel
      runtime={controller}
      nodes={nodes}
      pending={pending}
      onPending={onPending}
      onSetup={onSetup}
    />,
  );
  return { ...view, controller, onPending, onSetup };
}
function writes(fetch: ReturnType<typeof fakeAPI>) {
  return fetch.mock.calls.filter(([, init]) => init?.method !== "GET");
}

beforeEach(() => {
  window.sessionStorage.clear();
  window.localStorage.clear();
});

describe("explicit gateway control", () => {
  it("mounts and reloads with reads only; a running core is not active capture", async () => {
    const fetch = fakeAPI();
    const view = mount();
    await screen.findByText("已停用");
    expect(screen.getByText("未接管")).toBeInTheDocument();
    expect(screen.getByText("Synthetic Node")).toBeInTheDocument();
    expect(screen.getByText("规则分流")).toBeInTheDocument();
    expect(screen.getByText("未测")).toBeInTheDocument();
    expect(screen.getByText("192.168.31.0/24")).toBeInTheDocument();
    expect(screen.queryByText("运行中")).not.toBeInTheDocument();
    expect(document.querySelector(".badge-success")).toBeNull();
    view.rerender(
      <GatewayPanel
        runtime={view.controller}
        nodes={nodes}
        refreshVersion={1}
        onPending={view.onPending}
        onSetup={view.onSetup}
      />,
    );
    await waitFor(() =>
      expect(
        fetch.mock.calls.filter(([url]) => url === "/api/proxy/capture").length,
      ).toBeGreaterThan(1),
    );
    expect(writes(fetch)).toEqual([]);
    expect(fetch.mock.calls.every(([url]) => url.startsWith("/api/"))).toBe(
      true,
    );
  });

  it("enables gateway with only scope and IPv6 policy and no browser ranges or device inventory", async () => {
    const fetch = fakeAPI();
    const view = mount();
    const button = await screen.findByRole("button", { name: "开启网关代理" });
    await waitFor(() => expect(button).toBeEnabled());
    fireEvent.click(button);
    await screen.findByText("运行中");
    expect(writes(fetch)).toEqual([
      [
        "/api/proxy/capture",
        expect.objectContaining({
          method: "POST",
          body: JSON.stringify({ scope: "gateway", ipv6: "direct" }),
        }),
      ],
    ]);
    expect(view.controller.run).not.toHaveBeenCalled();
    expect(view.onPending.mock.calls).toEqual([[true], [false]]);
  });

  it("starts a configured stopped core before enabling gateway", async () => {
    const fetch = fakeAPI();
    const view = mount(runtime({ status: runtimeStatus }));
    await waitFor(() =>
      expect(
        screen.getByRole("button", { name: "开启网关代理" }),
      ).toBeEnabled(),
    );
    fireEvent.click(screen.getByRole("button", { name: "开启网关代理" }));
    await waitFor(() => expect(writes(fetch)).toHaveLength(2));
    expect(writes(fetch).map(([url]) => url)).toEqual([
      "/api/runtime/start",
      "/api/proxy/capture",
    ]);
    expect(view.controller.run).toHaveBeenCalledTimes(1);
    expect(writes(fetch)[1][1]?.body).toBe(
      JSON.stringify({ scope: "gateway", ipv6: "direct" }),
    );
  });

  it.each(["failed request", "not running"])(
    "never posts capture when core start is %s",
    async (failure) => {
      const fetch = fakeAPI();
      const normal = fetch.getMockImplementation()!;
      fetch.mockImplementation(async (url, init) =>
        url === "/api/runtime/start"
          ? failure === "failed request"
            ? jsonResponse(
                { error: { code: "start_failed", message: "cannot start" } },
                409,
              )
            : jsonResponse({ ...runtimeStatus, state: "starting" })
          : normal(url, init),
      );
      const controller = runtime({ status: runtimeStatus });
      controller.run = vi.fn(async (load) => {
        try {
          await runRequest(load());
          return true;
        } catch {
          return false;
        }
      });
      mount(controller);
      await waitFor(() =>
        expect(
          screen.getByRole("button", { name: "开启网关代理" }),
        ).toBeEnabled(),
      );
      fireEvent.click(screen.getByRole("button", { name: "开启网关代理" }));
      await screen.findByText("核心未能运行，请检查运行管理后重试。");
      expect(writes(fetch).map(([url]) => url)).toEqual(["/api/runtime/start"]);
    },
  );

  it("offers enable for a suspended gateway declaration with no installed rules", async () => {
    const fetch = fakeAPI({ ...off, desired: true, state: "suspended" });
    mount(runtime({ status: runtimeStatus }));
    await screen.findByText("已停用");
    expect(screen.getByText("未接管")).toBeInTheDocument();
    expect(screen.getByText("192.168.31.0/24")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "开启网关代理" }));
    await waitFor(() => expect(writes(fetch)).toHaveLength(2));
    expect(writes(fetch).map(([url]) => url)).toEqual([
      "/api/runtime/start",
      "/api/proxy/capture",
    ]);
  });

  it("disables with DELETE only and leaves the core running", async () => {
    const fetch = fakeAPI(active);
    const view = mount();
    await screen.findByText("运行中");
    fireEvent.click(screen.getByRole("button", { name: "关闭" }));
    await screen.findByText("已停用");
    expect(writes(fetch).map(([url, init]) => [url, init?.method])).toEqual([
      ["/api/proxy/capture", "DELETE"],
    ]);
    expect(view.controller.run).not.toHaveBeenCalled();
  });

  it("shows a scope mismatch for review and retries reads without withdrawing", async () => {
    const fetch = fakeAPI({
      ...active,
      lanIPv4Prefixes: ["192.168.31.0/24", "10.42.0.0/24"],
      installedLanIPv4Prefixes: ["192.168.31.20/32"],
    });
    mount();
    await screen.findByText("接管异常");
    expect(
      screen.getByText("192.168.31.0/24、10.42.0.0/24"),
    ).toBeInTheDocument();
    expect(screen.getByText("192.168.31.20/32")).toBeInTheDocument();
    expect(document.querySelector(".badge-success")).toBeNull();
    expect(screen.queryByText(/全网段/)).not.toBeInTheDocument();
    expect(screen.getByRole("alert")).toHaveTextContent(
      "当前声明与已安装范围不一致",
    );
    expect(screen.queryByRole("button", { name: "重试撤回" })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "重新读取" }));
    await waitFor(() =>
      expect(
        fetch.mock.calls.filter(([url]) => url === "/api/proxy/capture").length,
      ).toBeGreaterThan(1),
    );
    expect(writes(fetch)).toEqual([]);
    expect(screen.getByRole("button", { name: "关闭" })).toHaveClass(
      "button-secondary",
    );
    expect(document.querySelector(".badge-success")).toBeNull();
  });

  it.each([
    { ...active, desired: false, cleanupPending: true },
    { ...active, cleanupPending: true, error: "capture_cleanup_failed" },
    {
      ...active,
      desired: false,
      cleanupPending: true,
      error: "capture_cleanup_failed: resource remains\ncontext canceled",
    },
    {
      ...off,
      error: "capture_disable_not_persisted: write failed\ncontext canceled",
    },
    { ...active, error: "capture_withdraw_failed" },
  ])(
    "keeps genuine cleanup failures actionable only on user click",
    async (capture) => {
      const fetch = fakeAPI(capture);
      mount();
      await screen.findByRole("alert");
      expect(document.querySelector(".badge-success")).toBeNull();
      expect(screen.getByRole("button", { name: "重试撤回" })).toBeEnabled();
      expect(screen.queryByText(/context canceled/)).toBeNull();
      expect(writes(fetch)).toEqual([]);
      fireEvent.click(screen.getByRole("button", { name: "重试" }));
      await screen.findByText("已停用");
      expect(writes(fetch).map(([, init]) => init?.method)).toEqual(["DELETE"]);
    },
  );

  it("keeps failed or unknown capture observations non-green and does not enable", async () => {
    const fetch = fakeAPI();
    const normal = fetch.getMockImplementation()!;
    fetch.mockImplementation(async (url, init) =>
      url === "/api/proxy/capture"
        ? jsonResponse(
            { error: { code: "observation_unavailable", message: "读取失败" } },
            503,
          )
        : normal(url, init),
    );
    mount();
    await screen.findByText("状态暂未确认");
    expect(screen.getByRole("button", { name: "开启网关代理" })).toBeDisabled();
    expect(screen.getByRole("alert")).toHaveTextContent("请重新读取");
    expect(screen.getByRole("button", { name: "重新读取" })).toBeEnabled();
    expect(document.querySelector(".badge-success")).toBeNull();
    expect(writes(fetch)).toHaveLength(0);
  });

  it.each([
    {
      active: false,
      cleanupPending: true,
      state: "cleanup-pending",
      error: "capture_observation_busy",
      commands: 0,
    },
    {
      ...active,
      active: false,
      cleanupPending: true,
      state: "cleanup-pending",
      error: "capture_observation_failed: missing resource",
    },
    {
      ...active,
      active: false,
      scopeState: "unresolved" as const,
      state: "scope-changed",
      error: "capture_backend_not_ready\ncontext canceled",
    },
    { ...active, error: "context canceled" },
    { ...active, error: "context deadline exceeded" },
    { ...active, cleanupPending: true },
    { ...off, cleanupPending: true, error: "capture_observation_busy" },
    { ...active, error: "capture_cleanup_failed_other" },
  ])("retries uncertain observations with GET only", async (capture) => {
    const fetch = fakeAPI(capture);
    const view = mount();
    await screen.findByRole("alert");
    expect(document.querySelector(".badge-success")).toBeNull();
    expect(screen.queryByText(/规则尚未撤回/)).toBeNull();
    expect(
      screen.queryByText(/context canceled|context deadline exceeded/),
    ).toBeNull();
    expect(screen.queryByRole("button", { name: "重试撤回" })).toBeNull();
    expect(view.controller.run).not.toHaveBeenCalled();
    const readsBefore = fetch.mock.calls.filter(
      ([url]) => url === "/api/proxy/capture",
    ).length;
    fireEvent.click(screen.getByRole("button", { name: "重新读取" }));
    await waitFor(() =>
      expect(
        fetch.mock.calls.filter(([url]) => url === "/api/proxy/capture").length,
      ).toBeGreaterThan(readsBefore),
    );
    expect(writes(fetch)).toEqual([]);
    const enable = screen.queryByRole("button", { name: "开启网关代理" });
    if (enable) expect(enable).toBeDisabled();
  });

  it.each([
    "busy response",
    "canceled response",
    "HTTP read failure",
    "network read failure",
  ])(
    "retains the last confirmed active scope after a %s without fresh success or destructive retry",
    async (failure) => {
      const fetch = fakeAPI(active);
      const normal = fetch.getMockImplementation()!;
      const view = mount();
      await screen.findByText("运行中");
      let uncertain = true;
      fetch.mockImplementation(async (url, init) => {
        if (url !== "/api/proxy/capture" || !uncertain)
          return normal(url, init);
        if (failure === "HTTP read failure")
          return jsonResponse(
            {
              error: {
                code: "observation_unavailable",
                message: "context canceled",
              },
            },
            503,
          );
        if (failure === "network read failure")
          throw new TypeError("Failed to fetch");
        return jsonResponse(
          failure === "busy response"
            ? {
                active: false,
                cleanupPending: true,
                state: "cleanup-pending",
                error: "capture_observation_busy",
                commands: 0,
              }
            : {
                ...active,
                active: false,
                state: "scope-changed",
                scopeState: "unresolved",
                error: "capture_backend_not_ready\ncontext canceled",
              },
        );
      });
      view.rerender(
        <GatewayPanel
          runtime={view.controller}
          nodes={nodes}
          refreshVersion={1}
          onPending={view.onPending}
          onSetup={view.onSetup}
        />,
      );
      await screen.findByText("状态暂未确认");
      expect(screen.getByText("上次确认已安装：")).toHaveTextContent(
        "192.168.31.0/24",
      );
      expect(screen.queryByText("范围未知")).toBeNull();
      expect(screen.queryByText("运行中")).toBeNull();
      expect(document.querySelector(".badge-success")).toBeNull();
      expect(screen.queryByText(/规则尚未撤回|context canceled/)).toBeNull();
      expect(screen.queryByRole("button", { name: "重试撤回" })).toBeNull();
      expect(screen.queryByRole("button", { name: "开启网关代理" })).toBeNull();
      expect(screen.getByRole("button", { name: "关闭" })).toHaveClass(
        "button-secondary",
      );
      expect(screen.getByRole("button", { name: "重新读取" })).toHaveClass(
        "button-primary",
      );
      expect(writes(fetch)).toEqual([]);
      uncertain = false;
      fireEvent.click(screen.getByRole("button", { name: "重新读取" }));
      await screen.findByText("运行中");
      expect(screen.queryByText("上次确认已安装：")).toBeNull();
      expect(screen.queryByRole("button", { name: "重新读取" })).toBeNull();
      expect(writes(fetch)).toEqual([]);
    },
  );

  it("clears the prior active sample after confirmed withdrawal", async () => {
    const fetch = fakeAPI(active);
    const normal = fetch.getMockImplementation()!;
    const view = mount();
    await screen.findByText("运行中");
    fireEvent.click(screen.getByRole("button", { name: "关闭" }));
    await screen.findByText("已停用");
    fetch.mockImplementation(async (url, init) =>
      url === "/api/proxy/capture"
        ? jsonResponse({
            active: false,
            cleanupPending: true,
            state: "cleanup-pending",
            error: "capture_observation_busy",
            commands: 0,
          })
        : normal(url, init),
    );
    view.rerender(
      <GatewayPanel
        runtime={view.controller}
        nodes={nodes}
        refreshVersion={1}
        onPending={view.onPending}
        onSetup={view.onSetup}
      />,
    );
    await screen.findByText("状态暂未确认");
    expect(screen.queryByText("上次确认已安装：")).toBeNull();
    expect(screen.getByText("范围未知")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "关闭" })).toBeNull();
    expect(screen.getByRole("button", { name: "开启网关代理" })).toBeDisabled();
    fireEvent.click(screen.getByRole("button", { name: "重新读取" }));
    expect(writes(fetch).map(([, init]) => init?.method)).toEqual(["DELETE"]);
  });

  it("labels uncertain ranges as recorded rather than confirmed without a prior sample", async () => {
    const fetch = fakeAPI({
      ...active,
      active: false,
      cleanupPending: true,
      state: "cleanup-pending",
      error: "capture_observation_failed",
    });
    mount();
    await screen.findByText("状态暂未确认");
    expect(screen.getByText("已记录范围：")).toHaveTextContent(
      "192.168.31.0/24",
    );
    expect(screen.queryByText("上次确认已安装：")).toBeNull();
    expect(document.querySelector(".badge-success")).toBeNull();
    expect(screen.getByRole("button", { name: "关闭" })).toHaveClass(
      "button-secondary",
    );
    fireEvent.click(screen.getByRole("button", { name: "重新读取" }));
    expect(writes(fetch)).toEqual([]);
  });

  it("does not enable from a retained inactive sample after a failed read", async () => {
    const fetch = fakeAPI(off);
    const normal = fetch.getMockImplementation()!;
    const view = mount();
    await screen.findByText("已停用");
    fetch.mockImplementation(async (url, init) =>
      url === "/api/proxy/capture"
        ? jsonResponse(
            {
              error: {
                code: "observation_unavailable",
                message: "unavailable",
              },
            },
            503,
          )
        : normal(url, init),
    );
    view.rerender(
      <GatewayPanel
        runtime={view.controller}
        nodes={nodes}
        refreshVersion={1}
        onPending={view.onPending}
        onSetup={view.onSetup}
      />,
    );
    await screen.findByText("状态暂未确认");
    expect(screen.getByRole("button", { name: "开启网关代理" })).toBeDisabled();
    fireEvent.click(screen.getByRole("button", { name: "开启网关代理" }));
    fireEvent.click(screen.getByRole("button", { name: "重新读取" }));
    expect(writes(fetch)).toEqual([]);
    expect(view.controller.run).not.toHaveBeenCalled();
  });

  it("does not recommend withdrawal when an explicit enable fails", async () => {
    const fetch = fakeAPI(off);
    const normal = fetch.getMockImplementation()!;
    fetch.mockImplementation(async (url, init) =>
      url === "/api/proxy/capture" && init?.method === "POST"
        ? jsonResponse(
            {
              error: {
                code: "capture_backend_not_ready",
                message: "context canceled",
              },
            },
            409,
          )
        : normal(url, init),
    );
    mount();
    await screen.findByText("已停用");
    fireEvent.click(screen.getByRole("button", { name: "开启网关代理" }));
    await screen.findByRole("alert");
    expect(screen.queryByText(/context canceled/)).toBeNull();
    expect(screen.queryByRole("button", { name: "重试撤回" })).toBeNull();
    expect(screen.queryByRole("button", { name: "重试" })).toBeNull();
    expect(writes(fetch).map(([, init]) => init?.method)).toEqual(["POST"]);
  });

  it.each([
    { ...active, installedLanIPv4Prefixes: undefined },
    { ...active, state: "unknown" },
    { ...active, scope: "devices" as const },
  ])(
    "does not turn incomplete or device-only observations green",
    async (capture) => {
      fakeAPI(capture);
      mount();
      await waitFor(() =>
        expect(screen.getByRole("button", { name: "关闭" })).toBeEnabled(),
      );
      expect(document.querySelector(".badge-success")).toBeNull();
      expect(screen.queryByText("运行中")).not.toBeInTheDocument();
    },
  );

  it.each([
    [
      "artifact",
      { ...runtimeStatus, artifactAvailable: false },
      nodes,
      "获取运行文件",
      "runtime",
    ],
    [
      "subscription",
      runtimeStatus,
      { ...nodes, nodes: [], selectedNodeId: "" },
      "导入订阅",
      "subscription",
    ],
    [
      "node",
      runtimeStatus,
      { ...nodes, selectedNodeId: "" },
      "选择并保存节点",
      "node",
    ],
    [
      "config",
      { ...runtimeStatus, configured: false },
      nodes,
      "选择并保存节点",
      "node",
    ],
  ] as const)(
    "offers concrete setup when %s is missing",
    async (_label, status, availableNodes, action, setup) => {
      const fetch = fakeAPI();
      const onSetup = vi.fn();
      render(
        <GatewayPanel
          runtime={runtime({ status })}
          nodes={availableNodes}
          onPending={vi.fn()}
          onSetup={onSetup}
        />,
      );
      await screen.findByText("已停用");
      expect(
        screen.getByRole("button", { name: "开启网关代理" }),
      ).toBeDisabled();
      fireEvent.click(screen.getByRole("button", { name: action }));
      expect(onSetup).toHaveBeenCalledWith(setup);
      expect(writes(fetch)).toEqual([]);
    },
  );

  it("uses actual successful probe data without starting a probe job", async () => {
    const fetch = fakeAPI();
    const normal = fetch.getMockImplementation()!;
    fetch.mockImplementation(async (url, init) =>
      url === "/api/proxy/node-probes"
        ? jsonResponse({
            ...emptyProbes,
            results: [
              {
                nodeId: "node-synthetic",
                status: "success",
                delayMs: 37.4,
                measuredAt: "2026-10-04T00:00:00Z",
                target: emptyProbes.target,
              },
            ],
          })
        : normal(url, init),
    );
    mount();
    await screen.findByText("37 ms");
    expect(writes(fetch)).toHaveLength(0);
  });

  it("blocks duplicate or conflicting controls while pending", async () => {
    const fetch = fakeAPI();
    const normal = fetch.getMockImplementation()!;
    let complete!: (response: Response) => void;
    fetch.mockImplementation(async (url, init) =>
      url === "/api/proxy/capture" && init?.method === "POST"
        ? new Promise<Response>((resolve) => {
            complete = resolve;
          })
        : normal(url, init),
    );
    mount();
    await waitFor(() =>
      expect(
        screen.getByRole("button", { name: "开启网关代理" }),
      ).toBeEnabled(),
    );
    const button = screen.getByRole("button", { name: "开启网关代理" });
    fireEvent.click(button);
    expect(screen.getByRole("button", { name: "处理中…" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "更换节点" })).toBeDisabled();
    fireEvent.click(button);
    expect(writes(fetch)).toHaveLength(1);
    await act(async () => complete(jsonResponse(active)));
  });

  it("keeps a failed withdrawal actionable and never reports success", async () => {
    const fetch = fakeAPI(active);
    const normal = fetch.getMockImplementation()!;
    let fail = true;
    fetch.mockImplementation(async (url, init) =>
      url === "/api/proxy/capture" && init?.method === "DELETE" && fail
        ? jsonResponse(
            { error: { code: "withdraw_failed", message: "撤回失败" } },
            409,
          )
        : normal(url, init),
    );
    mount();
    await screen.findByText("运行中");
    fireEvent.click(screen.getByRole("button", { name: "关闭" }));
    await screen.findByText(/撤回失败/);
    expect(document.querySelector(".badge-success")).toBeNull();
    fail = false;
    fireEvent.click(screen.getByRole("button", { name: "重试" }));
    await screen.findByText("已停用");
    expect(writes(fetch).map(([, init]) => init?.method)).toEqual([
      "DELETE",
      "DELETE",
    ]);
  });

  it("decodes the gateway scope and both optional range lists", () => {
    expect(Schema.decodeUnknownSync(ProxyCaptureSchema)(active)).toEqual(
      active,
    );
    expect(
      Schema.decodeUnknownSync(ProxyCaptureSchema)({
        active: false,
        commands: 0,
      }),
    ).toEqual({ active: false, commands: 0 });
  });
});

const savedConfig = {
  service: "sing-box",
  generation: 4,
  config: JSON.stringify({
    inbounds: [
      {
        type: "mixed",
        tag: "mixed-in",
        listen: "192.168.31.1",
        listen_port: 2080,
      },
      {
        type: "tproxy",
        tag: "tproxy-in",
        listen: "127.0.0.1",
        listen_port: 7893,
      },
      {
        type: "direct",
        tag: "dns-in",
        listen: "192.168.31.1",
        listen_port: 6450,
      },
    ],
    route: { rules: [{ ip_version: 6, outbound: "direct" }] },
  }),
};
describe("compact proxy first screen", () => {
  it("keeps the current node visible and picker folded; opening, refreshing, saving never applies capture", async () => {
    const fetch = fakeAPI();
    const normal = fetch.getMockImplementation()!;
    fetch.mockImplementation(async (url, init) => {
      if (url === "/api/runtime")
        return jsonResponse({ enabled: true, services: [runtimeStatus] });
      if (url === "/api/proxy/nodes") return jsonResponse(nodes);
      if (url === "/api/runtime/config?service=sing-box")
        return jsonResponse(savedConfig);
      if (url === "/api/proxy/select")
        return jsonResponse({
          status: runtimeStatus,
          configSHA256: "b".repeat(64),
          diagnostics: [],
        });
      return normal(url, init);
    });
    const user = userEvent.setup();
    render(<ProxyPage />);
    await screen.findByText("Synthetic Node");
    const card = screen.getByRole("region", { name: "网关路由代理" });
    expect(within(card).getByText("Synthetic Node")).toBeInTheDocument();
    expect(
      screen.queryByRole("list", { name: "代理节点列表" }),
    ).not.toBeInTheDocument();
    expect(
      screen.getByText("更换节点", { selector: "summary" }).closest("details"),
    ).not.toHaveAttribute("open");
    expect(
      screen.getByText("高级设置与诊断").closest("details"),
    ).not.toHaveAttribute("open");
    await user.click(within(card).getByRole("button", { name: "更换节点" }));
    expect(
      screen.getByRole("list", { name: "代理节点列表" }),
    ).toBeInTheDocument();
    expect(
      card.compareDocumentPosition(
        screen.getByRole("list", { name: "代理节点列表" }),
      ) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "保存配置" })).toBeEnabled(),
    );
    await user.click(screen.getByRole("button", { name: "保存配置" }));
    await screen.findByText("节点配置已保存");
    await user.click(screen.getByText("高级设置与诊断"));
    await user.click(screen.getByRole("button", { name: "刷新代理状态" }));
    await waitFor(() =>
      expect(
        fetch.mock.calls.filter(([url]) => url === "/api/proxy/nodes").length,
      ).toBeGreaterThan(1),
    );
    expect(writes(fetch).map(([url]) => url)).toEqual(["/api/proxy/select"]);
    expect(fetch.mock.calls.every(([url]) => url.startsWith("/api/"))).toBe(
      true,
    );
    expect(screen.queryByLabelText("接管后端")).not.toBeInTheDocument();
  });

  it("blocks existing node, subscription, runtime and diagnostic actions during gateway enable", async () => {
    const fetch = fakeAPI();
    const normal = fetch.getMockImplementation()!;
    let complete!: (response: Response) => void;
    fetch.mockImplementation(async (url, init) => {
      if (url === "/api/runtime")
        return jsonResponse({
          enabled: true,
          services: [{ ...runtimeStatus, state: "running" }],
        });
      if (url === "/api/proxy/nodes") return jsonResponse(nodes);
      if (url === "/api/runtime/config?service=sing-box")
        return jsonResponse(savedConfig);
      if (url === "/api/proxy/capture" && init?.method === "POST")
        return new Promise<Response>((resolve) => {
          complete = resolve;
        });
      return normal(url, init);
    });
    const user = userEvent.setup();
    render(<ProxyPage />);
    await waitFor(() =>
      expect(
        screen.getByRole("button", { name: "开启网关代理" }),
      ).toBeEnabled(),
    );
    await user.click(screen.getByRole("button", { name: "更换节点" }));
    await user.click(screen.getByText("高级设置与诊断"));
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "保存配置" })).toBeEnabled(),
    );
    await user.click(screen.getByRole("button", { name: "开启网关代理" }));
    expect(screen.getByRole("button", { name: "保存配置" })).toBeDisabled();
    expect(
      screen.getByRole("button", { name: "解析并导入节点" }),
    ).toBeDisabled();
    expect(screen.getByRole("button", { name: "刷新代理状态" })).toBeDisabled();
    expect(screen.getByRole("tab", { name: "设备诊断" })).toBeDisabled();
    expect(screen.getByRole("tab", { name: "运行管理" })).toBeDisabled();
    await act(async () => complete(jsonResponse(active)));
  });
});
