import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { ProxyCapture, RouterSnapshot } from "../../lib/contracts";
import { runRequest } from "../../lib/api";
import {
  jsonResponse,
  routerSnapshot,
  runtimeStatus,
} from "../production-fixtures.test-data";
import type { RuntimeController } from "../runtime/use-runtime";
import { CapturePanel } from "./capture-panel";

const first = {
  ...routerSnapshot.devices[0],
  hostname: "Mac test",
  eligible: true,
};
const second = {
  ...first,
  ip: "192.0.2.21",
  mac: "02:00:00:00:00:21",
  hostname: "Phone test",
};
const offline = {
  ...first,
  ip: "192.0.2.22",
  mac: "02:00:00:00:00:22",
  hostname: "Offline test",
  online: false,
};
const snapshot: RouterSnapshot = {
  ...routerSnapshot,
  currentClientIP: first.ip,
  devices: [first, second, offline],
};
const running = { ...runtimeStatus, state: "running", desired: true };
const initialCapture: ProxyCapture = {
  active: false,
  desired: false,
  clients: [],
  ipv6: "direct",
  state: "idle",
  cleanupPending: false,
  commands: 0,
};

function setup(
  options: {
    capture?: ProxyCapture;
    router?: RouterSnapshot;
    state?: string;
    enabled?: boolean;
    routerError?: string;
    captureError?: string;
  } = {},
) {
  const state = {
    capture: options.capture ?? initialCapture,
    router: options.router ?? snapshot,
    routerError: options.routerError,
    captureError: options.captureError,
    applyError: undefined as string | undefined,
    disableError: undefined as string | undefined,
  };
  const fetch = vi.fn((url: string, init: RequestInit) => {
    if (url === "/api/router")
      return Promise.resolve(
        state.routerError
          ? jsonResponse(
              {
                error: {
                  code: "observation_unavailable",
                  message: state.routerError,
                },
              },
              503,
            )
          : jsonResponse(state.router),
      );
    if (url === "/api/proxy/capture") {
      if (init.method === "GET" && state.captureError)
        return Promise.resolve(
          jsonResponse(
            {
              error: {
                code: "capture_unavailable",
                message: state.captureError,
              },
            },
            409,
          ),
        );
      if (init.method === "POST") {
        if (state.applyError)
          return Promise.resolve(
            jsonResponse(
              { error: { code: "capture_failed", message: state.applyError } },
              409,
            ),
          );
        const input = JSON.parse(init.body as string) as {
          devices: { mac: string }[];
          ipv6: ProxyCapture["ipv6"];
          clientIPv6?: string;
        };
        state.capture = {
          ...initialCapture,
          active: true,
          desired: true,
          state: "active",
          commands: 8,
          ipv6: input.ipv6,
          clientIPv6: input.clientIPv6,
          clients: input.devices.map(({ mac }) => {
            const device = state.router.devices.find(
              (item) => item.mac === mac,
            );
            return {
              mac,
              ip: device?.ip ?? "",
              hostname: device?.hostname ?? "",
            };
          }),
        };
      }
      if (init.method === "DELETE") {
        if (state.disableError)
          return Promise.resolve(
            jsonResponse(
              {
                error: { code: "cleanup_failed", message: state.disableError },
              },
              500,
            ),
          );
        state.capture = initialCapture;
      }
      return Promise.resolve(jsonResponse(state.capture));
    }
    if (url === "/api/runtime/stop") {
      state.capture = {
        ...state.capture,
        active: false,
        state: "suspended",
        commands: 0,
      };
      return Promise.resolve(
        jsonResponse({ ...runtimeStatus, state: "stopped" }),
      );
    }
    throw new Error(`Unexpected request: ${url}`);
  });
  vi.stubGlobal("fetch", fetch);
  const run = vi.fn<RuntimeController["run"]>(async (load) => {
    await runRequest(load());
    return true;
  });
  const runtime: RuntimeController = {
    service: "sing-box",
    status: { ...running, state: options.state ?? "running" },
    enabled: options.enabled ?? true,
    pending: false,
    loading: false,
    error: undefined,
    result: undefined,
    refresh: vi.fn(),
    run,
  };
  const view = render(<CapturePanel runtime={runtime} />);
  const mutations = (method: "POST" | "DELETE") =>
    fetch.mock.calls.filter(
      ([url, init]) => url === "/api/proxy/capture" && init.method === method,
    );
  return { state, fetch, runtime, view, mutations };
}
const choice = (name: string) =>
  screen.getByRole("checkbox", { name: new RegExp(`选择设备 ${name}`) });
async function ready() {
  await waitFor(() => expect(choice("Mac test")).toBeEnabled());
}
afterEach(() => vi.unstubAllGlobals());

describe("observed device capture", () => {
  it("shows LAN device name/IP/MAC/online state and defaults only the current terminal without POST", async () => {
    const { mutations } = setup();
    await ready();
    const table = within(screen.getByRole("table", { name: "接管设备选择" }));
    for (const text of [
      first.hostname,
      first.ip,
      first.mac,
      "当前终端",
      "离线 / 未见 ARP",
    ])
      expect(table.getByText(text)).toBeInTheDocument();
    expect(table.getAllByText("在线")).toHaveLength(2);
    expect(choice("Mac test")).toBeChecked();
    expect(choice("Phone test")).not.toBeChecked();
    expect(choice("Offline test")).not.toBeChecked();
    expect(screen.queryByLabelText("客户端 IPv4")).not.toBeInTheDocument();
    expect(screen.queryByLabelText("客户端 IPv6")).not.toBeInTheDocument();
    expect(screen.getByLabelText("接管 IPv6 策略")).toHaveValue("direct");
    expect(screen.getByText(/IPv6 直连不会被透明代理接管/)).toBeInTheDocument();
    expect(mutations("POST")).toHaveLength(0);
  });
  it.each([undefined, "192.0.2.99"])(
    "never selects the whole LAN when current terminal is %s",
    async (currentClientIP) => {
      const { mutations } = setup({ router: { ...snapshot, currentClientIP } });
      await ready();
      expect(
        screen
          .getAllByRole("checkbox")
          .every((input) => !(input as HTMLInputElement).checked),
      ).toBe(true);
      expect(screen.queryByText("当前终端")).not.toBeInTheDocument();
      expect(
        screen.getByRole("button", { name: "审阅客户端接管" }),
      ).toBeDisabled();
      expect(mutations("POST")).toHaveLength(0);
    },
  );
  it("submits exactly checked MACs only after a second confirmation", async () => {
    const { fetch, mutations } = setup();
    const user = userEvent.setup();
    await ready();
    await user.click(choice("Phone test"));
    await user.click(screen.getByRole("button", { name: "审阅客户端接管" }));
    const dialog = screen.getByRole("alertdialog", {
      name: "开启客户端透明接管",
    });
    expect(
      within(dialog).getByText(
        /即将为设备.*开启透明代理转发，请确认是否继续。/,
      ),
    ).toBeInTheDocument();
    expect(within(dialog).getByText(first.mac)).toBeInTheDocument();
    expect(within(dialog).getByText(second.mac)).toBeInTheDocument();
    expect(within(dialog).queryByText(offline.mac)).not.toBeInTheDocument();
    expect(mutations("POST")).toHaveLength(0);
    await user.click(screen.getByRole("button", { name: "确认接管" }));
    await screen.findByText("已生效");
    expect(fetch).toHaveBeenCalledWith(
      "/api/proxy/capture",
      expect.objectContaining({
        method: "POST",
        body: JSON.stringify({
          devices: [{ mac: first.mac }, { mac: second.mac }],
          ipv6: "direct",
        }),
      }),
    );
    expect(mutations("POST")).toHaveLength(1);
    expect(
      screen.getByText(/接管规则已生效（不代表互联网连通性已验证）/),
    ).toBeInTheDocument();
  });
  it("invalidates confirmation when selection, policy, restore or refresh changes", async () => {
    const { mutations } = setup();
    const user = userEvent.setup();
    await ready();
    await user.click(screen.getByRole("button", { name: "审阅客户端接管" }));
    await user.click(choice("Phone test"));
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "审阅客户端接管" }));
    await user.selectOptions(screen.getByLabelText("接管 IPv6 策略"), "block");
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
    await user.selectOptions(screen.getByLabelText("接管 IPv6 策略"), "direct");
    await user.click(screen.getByRole("button", { name: "审阅客户端接管" }));
    await user.click(
      screen.getByRole("button", { name: "刷新设备与接管状态" }),
    );
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
    await ready();
    expect(choice("Phone test")).toBeChecked();
    await user.click(screen.getByRole("button", { name: "审阅客户端接管" }));
    await user.click(screen.getByRole("button", { name: "还原已保存选择" }));
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
    expect(choice("Mac test")).not.toBeChecked();
    expect(choice("Phone test")).not.toBeChecked();
    expect(mutations("POST")).toHaveLength(0);
  });
  it("saves a three-device group, restores it on reopening, and applies a two-device subset", async () => {
    const { state, view, runtime, mutations } = setup();
    const user = userEvent.setup();
    await ready();
    await user.click(choice("Phone test"));
    await user.click(choice("Offline test"));
    await user.click(screen.getByRole("button", { name: "审阅客户端接管" }));
    expect(mutations("POST")).toHaveLength(0);
    await user.click(screen.getByRole("button", { name: "确认接管" }));
    await screen.findByText("已生效");
    expect(JSON.parse(mutations("POST")[0][1].body as string)).toEqual({
      devices: [{ mac: first.mac }, { mac: second.mac }, { mac: offline.mac }],
      ipv6: "direct",
    });
    state.capture = {
      ...state.capture,
      active: false,
      state: "suspended",
      commands: 0,
    };
    view.unmount();
    const reopened = render(
      <CapturePanel
        runtime={{ ...runtime, status: { ...running, state: "stopped" } }}
      />,
    );
    await ready();
    expect(screen.getByText("已暂停")).toBeInTheDocument();
    for (const name of ["Mac test", "Phone test", "Offline test"])
      expect(choice(name)).toBeChecked();
    expect(mutations("POST")).toHaveLength(1);
    reopened.rerender(<CapturePanel runtime={runtime} />);
    await waitFor(() =>
      expect(
        screen.getByRole("button", { name: "审阅客户端接管" }),
      ).toBeEnabled(),
    );
    await user.click(choice("Phone test"));
    const saved = within(
      screen.getByRole("region", { name: "已保存的接管选择" }),
    );
    expect(saved.getByText(second.mac)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "审阅客户端接管" }));
    expect(mutations("POST")).toHaveLength(1);
    await user.click(screen.getByRole("button", { name: "确认接管" }));
    await screen.findByText("已生效");
    expect(JSON.parse(mutations("POST")[1][1].body as string)).toEqual({
      devices: [{ mac: first.mac }, { mac: offline.mac }],
      ipv6: "direct",
    });
    expect(
      within(
        screen.getByRole("region", { name: "已保存的接管选择" }),
      ).queryByText(second.mac),
    ).not.toBeInTheDocument();
  });
  it("cancels review when the core generation changes, without applying saved or draft scope", async () => {
    const { runtime, view, mutations } = setup();
    const user = userEvent.setup();
    await ready();
    await user.click(screen.getByRole("button", { name: "审阅客户端接管" }));
    expect(screen.getByRole("alertdialog")).toBeInTheDocument();
    view.rerender(
      <CapturePanel
        runtime={{
          ...runtime,
          status: { ...running, generation: running.generation + 1 },
        }}
      />,
    );
    await waitFor(() =>
      expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument(),
    );
    expect(choice("Mac test")).toBeChecked();
    expect(mutations("POST")).toHaveLength(0);
  });
  it("does not default a conflicting current IP to two different MAC identities", async () => {
    const { mutations } = setup({
      router: { ...snapshot, devices: [first, { ...second, ip: first.ip }] },
    });
    await ready();
    expect(choice("Mac test")).not.toBeChecked();
    expect(choice("Phone test")).not.toBeChecked();
    expect(
      screen.getByRole("button", { name: "审阅客户端接管" }),
    ).toBeDisabled();
    expect(mutations("POST")).toHaveLength(0);
  });
  it("keeps a selected draft MAC visibly unresolved and removable after device refresh", async () => {
    const { state, mutations } = setup();
    const user = userEvent.setup();
    await ready();
    state.router = { ...snapshot, devices: [second] };
    await user.click(
      screen.getByRole("button", { name: "刷新设备与接管状态" }),
    );
    await waitFor(() =>
      expect(
        screen.getByRole("checkbox", { name: new RegExp(first.mac) }),
      ).toBeEnabled(),
    );
    const unresolved = screen.getByRole("checkbox", {
      name: new RegExp(first.mac),
    });
    expect(unresolved).toBeChecked();
    expect(screen.getByText("未在当前 LAN 观察到")).toBeInTheDocument();
    expect(screen.queryByText(first.ip)).not.toBeInTheDocument();
    await user.click(unresolved);
    expect(
      screen.queryByRole("checkbox", { name: new RegExp(first.mac) }),
    ).not.toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "审阅客户端接管" }),
    ).toBeDisabled();
    expect(mutations("POST")).toHaveLength(0);
  });
  it("requires a single selected device and an explicit IPv6 address for follow/block", async () => {
    const { fetch, mutations } = setup();
    const user = userEvent.setup();
    await ready();
    await user.selectOptions(screen.getByLabelText("接管 IPv6 策略"), "follow");
    expect(
      screen.getByRole("button", { name: "审阅客户端接管" }),
    ).toBeDisabled();
    await user.type(screen.getByLabelText("客户端 IPv6"), "2001:db8::20");
    await user.click(choice("Phone test"));
    expect(screen.getByRole("alert")).toHaveTextContent("需要只选择一台设备");
    expect(
      screen.getByRole("button", { name: "审阅客户端接管" }),
    ).toBeDisabled();
    await user.click(choice("Phone test"));
    await user.click(screen.getByRole("button", { name: "审阅客户端接管" }));
    expect(mutations("POST")).toHaveLength(0);
    await user.click(screen.getByRole("button", { name: "确认接管" }));
    await screen.findByText("已生效");
    expect(fetch).toHaveBeenCalledWith(
      "/api/proxy/capture",
      expect.objectContaining({
        body: JSON.stringify({
          devices: [{ mac: first.mac }],
          ipv6: "follow",
          clientIPv6: "2001:db8::20",
        }),
      }),
    );
    expect(
      screen.getByText(/不会自动覆盖设备的其他 IPv6 地址/),
    ).toBeInTheDocument();
  });
});

describe("saved versus live capture", () => {
  const saved: ProxyCapture = {
    ...initialCapture,
    desired: true,
    state: "suspended",
    ipv6: "block",
    clientIPv6: "2001:db8::21",
    clients: [
      { mac: second.mac, ip: second.ip, hostname: second.hostname },
      { mac: "02:00:00:00:00:99", ip: "", hostname: "Missing test" },
    ],
  };
  it("restores saved MACs rather than the current terminal, including offline/unresolved devices", async () => {
    const { mutations } = setup({ capture: saved });
    await ready();
    expect(choice("Mac test")).not.toBeChecked();
    expect(choice("Phone test")).toBeChecked();
    expect(choice("Missing test")).toBeChecked();
    expect(screen.getByText("未在当前 LAN 观察到")).toBeInTheDocument();
    expect(screen.getByText("已暂停")).toBeInTheDocument();
    expect(screen.getByText(/实时接管：未确认生效/)).toBeInTheDocument();
    const diagnostics = screen.getByText("高级诊断").closest("details");
    expect(diagnostics).not.toHaveAttribute("open");
    expect(within(diagnostics!).getByText("suspended")).toBeInTheDocument();
    expect(screen.getByLabelText("接管 IPv6 策略")).toHaveValue("block");
    expect(screen.getByLabelText("客户端 IPv6")).toHaveValue("2001:db8::21");
    const region = within(
      screen.getByRole("region", { name: "已保存的接管选择" }),
    );
    expect(region.getByText(second.mac)).toBeInTheDocument();
    expect(region.getByText("等待设备当前地址")).toBeInTheDocument();
    expect(mutations("POST")).toHaveLength(0);
  });
  it("restores retained MAC choices with desired false without promising automatic restoration", async () => {
    const { mutations } = setup({
      capture: { ...saved, desired: false, ipv6: "direct" },
    });
    await ready();
    expect(choice("Mac test")).not.toBeChecked();
    expect(choice("Phone test")).toBeChecked();
    expect(choice("Missing test")).toBeChecked();
    expect(screen.getByText("已停用（选择保留）")).toBeInTheDocument();
    expect(
      screen.getByText(/自动恢复已停用；只有明确应用后才启用接管/),
    ).toBeInTheDocument();
    expect(
      within(
        screen.getByRole("region", { name: "已保存的接管选择" }),
      ).getByText(second.mac),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "禁用接管并清除选择" }),
    ).toBeEnabled();
    expect(mutations("POST")).toHaveLength(0);
  });
  it("keeps saved scope separate from draft changes and offers local restore without applying", async () => {
    const { mutations } = setup({
      capture: { ...saved, ipv6: "direct", clientIPv6: undefined },
    });
    const user = userEvent.setup();
    await ready();
    await user.click(choice("Phone test"));
    await user.click(choice("Mac test"));
    expect(
      within(
        screen.getByRole("region", { name: "已保存的接管选择" }),
      ).getByText(second.mac),
    ).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "还原已保存选择" }));
    expect(choice("Mac test")).not.toBeChecked();
    expect(choice("Phone test")).toBeChecked();
    expect(choice("Missing test")).toBeChecked();
    expect(mutations("POST")).toHaveLength(0);
  });
  it("retains scope while the core is stopped and does not claim active capture", async () => {
    const { mutations } = setup({ capture: saved, state: "stopped" });
    await ready();
    expect(choice("Phone test")).toBeChecked();
    expect(screen.getByText("已暂停")).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "审阅客户端接管" }),
    ).toBeDisabled();
    expect(
      screen.getByRole("button", { name: "禁用接管并清除选择" }),
    ).toBeEnabled();
    expect(screen.getByText(/启动后可恢复/)).toBeInTheDocument();
    expect(mutations("POST")).toHaveLength(0);
  });
  it("reads legacy single-address live status without restoring by an unrelated MAC", async () => {
    setup({ capture: { active: true, clientIPv4: first.ip, commands: 8 } });
    await ready();
    expect(screen.getByText(/兼容旧版地址选择/)).toHaveTextContent(first.ip);
    expect(choice("Mac test")).toBeChecked();
    expect(choice("Phone test")).not.toBeChecked();
    expect(screen.getByText("已生效")).toBeInTheDocument();
  });
  it("Stop calls only runtime stop, suspends live capture and preserves checked devices", async () => {
    const { fetch, runtime, mutations } = setup({
      capture: {
        ...saved,
        active: true,
        ipv6: "direct",
        state: "active",
        commands: 8,
      },
    });
    const user = userEvent.setup();
    await ready();
    await user.click(
      screen.getByRole("button", { name: "停止核心（保留选择）" }),
    );
    await screen.findByText("已暂停");
    expect(fetch).toHaveBeenCalledWith(
      "/api/runtime/stop",
      expect.objectContaining({
        method: "POST",
        body: JSON.stringify({ service: "sing-box" }),
      }),
    );
    expect(runtime.run).toHaveBeenCalledOnce();
    expect(choice("Phone test")).toBeChecked();
    expect(choice("Missing test")).toBeChecked();
    expect(mutations("DELETE")).toHaveLength(0);
    expect(mutations("POST")).toHaveLength(0);
  });
  it("explicit disable confirms then DELETEs saved selection and clears the draft", async () => {
    const { mutations } = setup({
      capture: { ...saved, ipv6: "direct" },
      state: "stopped",
    });
    const user = userEvent.setup();
    await ready();
    await user.click(
      screen.getByRole("button", { name: "禁用接管并清除选择" }),
    );
    expect(
      screen.getByRole("alertdialog", { name: "确认禁用客户端接管" }),
    ).toHaveTextContent("之后核心重启也不会恢复接管");
    expect(mutations("DELETE")).toHaveLength(0);
    await user.click(screen.getByRole("button", { name: "确认禁用接管" }));
    await screen.findByText("未接管");
    expect(mutations("DELETE")).toHaveLength(1);
    expect(choice("Mac test")).not.toBeChecked();
    expect(choice("Phone test")).not.toBeChecked();
    expect(
      screen.queryByRole("checkbox", { name: /Missing test/ }),
    ).not.toBeInTheDocument();
    expect(mutations("POST")).toHaveLength(0);
  });
  it("reports only resolved devices active in a partially restored group", async () => {
    setup({
      capture: {
        ...saved,
        active: true,
        state: "partial",
        error: "capture_devices_pending",
        ipv6: "direct",
        commands: 8,
      },
    });
    await ready();
    expect(screen.getByText("部分已生效")).toBeInTheDocument();
    expect(
      screen.getByText(/仅已解析设备的接管规则已生效；其余设备等待当前地址/),
    ).toBeInTheDocument();
    expect(
      screen.getByText(/未解析设备不会接管，也不会使用过期 IP/),
    ).toBeInTheDocument();
    expect(choice("Phone test")).toBeChecked();
    expect(choice("Missing test")).toBeChecked();
    const region = within(
      screen.getByRole("region", { name: "已保存的接管选择" }),
    );
    expect(region.getByText(second.mac).closest("li")).toHaveTextContent(
      `${second.ip} · 实时规则已生效`,
    );
    expect(
      region.getByText("02:00:00:00:00:99").closest("li"),
    ).toHaveTextContent("等待当前地址");
    expect(screen.queryByText("已生效")).not.toBeInTheDocument();
  });
  it("shows capture errors and pending cleanup instead of claiming a working path", async () => {
    setup({
      capture: {
        ...saved,
        active: true,
        cleanupPending: true,
        error: "address unresolved",
      },
    });
    await ready();
    expect(screen.getByText("清理待完成")).toBeInTheDocument();
    expect(screen.getByText("address unresolved")).toBeInTheDocument();
    expect(screen.getByText(/不能视为已撤回/)).toBeInTheDocument();
    expect(screen.queryByText(/接管规则已生效/)).not.toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "禁用接管并清除选择" }),
    ).toBeEnabled();
    expect(
      screen.getByRole("button", { name: "审阅客户端接管" }),
    ).toBeDisabled();
  });
  it.each(["apply", "disable"] as const)(
    "reports %s mutation busy to the page and releases it after failure",
    async (operation) => {
      const { state, view, runtime, mutations } = setup({
        capture: { ...saved, ipv6: "direct" },
      });
      const onPending = vi.fn();
      view.rerender(<CapturePanel runtime={runtime} onPending={onPending} />);
      const user = userEvent.setup();
      await ready();
      if (operation === "apply") {
        state.applyError = "apply rejected";
        await user.click(
          screen.getByRole("button", { name: "审阅客户端接管" }),
        );
        await user.click(screen.getByRole("button", { name: "确认接管" }));
        await screen.findByText(/apply rejected/);
      } else {
        state.disableError = "disable rejected";
        await user.click(
          screen.getByRole("button", { name: "禁用接管并清除选择" }),
        );
        await user.click(screen.getByRole("button", { name: "确认禁用接管" }));
        await screen.findByText(/disable rejected/);
      }
      expect(onPending.mock.calls).toEqual([[true], [false]]);
      expect(mutations(operation === "apply" ? "POST" : "DELETE")).toHaveLength(
        1,
      );
    },
  );
  it("keeps desired scope visible when DELETE cleanup fails without retrying", async () => {
    const { state, mutations } = setup({ capture: saved });
    state.disableError = "withdraw failed";
    const user = userEvent.setup();
    await ready();
    await user.click(
      screen.getByRole("button", { name: "禁用接管并清除选择" }),
    );
    await user.click(screen.getByRole("button", { name: "确认禁用接管" }));
    await screen.findByText(/withdraw failed · cleanup_failed/);
    expect(choice("Phone test")).toBeChecked();
    expect(mutations("DELETE")).toHaveLength(1);
  });
});

describe("device observation failures", () => {
  it("does not guess saved selection or active status when capture observation fails, but allows explicit DELETE", async () => {
    const { mutations } = setup({ captureError: "capture status unavailable" });
    const user = userEvent.setup();
    await screen.findByText(/capture status unavailable · capture_unavailable/);
    expect(
      screen.getByText("状态未知", { selector: ".badge" }),
    ).toBeInTheDocument();
    expect(choice("Mac test")).toBeDisabled();
    expect(choice("Mac test")).not.toBeChecked();
    expect(
      screen.getByRole("button", { name: "审阅客户端接管" }),
    ).toBeDisabled();
    expect(
      screen.getByRole("button", { name: "禁用接管并清除选择" }),
    ).toBeEnabled();
    await user.click(
      screen.getByRole("button", { name: "禁用接管并清除选择" }),
    );
    expect(mutations("DELETE")).toHaveLength(0);
    await user.click(screen.getByRole("button", { name: "确认禁用接管" }));
    await waitFor(() => expect(mutations("DELETE")).toHaveLength(1));
    expect(mutations("POST")).toHaveLength(0);
  });
  it("allows offline MAC selection but still requires explicit Apply", async () => {
    const { mutations } = setup();
    const user = userEvent.setup();
    await ready();
    await user.click(choice("Mac test"));
    await user.click(choice("Offline test"));
    expect(choice("Offline test")).toBeChecked();
    await user.click(screen.getByRole("button", { name: "审阅客户端接管" }));
    expect(screen.getByRole("alertdialog")).toHaveTextContent(
      "离线设备等待当前地址",
    );
    expect(mutations("POST")).toHaveLength(0);
    await user.click(screen.getByRole("button", { name: "确认接管" }));
    await waitFor(() => expect(mutations("POST")).toHaveLength(1));
    expect(JSON.parse(mutations("POST")[0][1].body as string)).toEqual({
      devices: [{ mac: offline.mac }],
      ipv6: "direct",
    });
  });
  it("leaves draft device choices usable but blocks Apply when runtime control is unavailable", async () => {
    const { mutations } = setup({ enabled: false });
    await ready();
    expect(choice("Mac test")).toBeChecked();
    expect(
      screen.getByRole("button", { name: "审阅客户端接管" }),
    ).toBeDisabled();
    expect(
      screen.getByRole("button", { name: "停止核心（保留选择）" }),
    ).toBeDisabled();
    expect(mutations("POST")).toHaveLength(0);
  });
  it("shows an empty list and never allows apply to zero devices", async () => {
    const { mutations } = setup({
      router: {
        ...snapshot,
        devices: [],
        routes: [
          {
            family: "ipv4",
            destination: "192.0.2.0/24",
            gateway: "",
            interface: "br-lan",
            metric: 0,
          },
        ],
      },
    });
    await screen.findByText("未观察到可接管的 LAN 设备");
    expect(screen.queryByRole("checkbox")).not.toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "审阅客户端接管" }),
    ).toBeDisabled();
    expect(mutations("POST")).toHaveLength(0);
  });
  it("does not expose upstream or ineligible rows as checkbox choices", async () => {
    setup({
      router: {
        ...snapshot,
        devices: [
          first,
          {
            ...second,
            eligible: false,
            ip: "192.168.1.3",
            hostname: "WAN test",
          },
        ],
      },
    });
    await ready();
    expect(screen.queryByText("WAN test")).not.toBeInTheDocument();
    expect(screen.getAllByRole("checkbox")).toHaveLength(1);
  });
  it("disables new selection without a LAN eligibility source rather than exposing all devices", async () => {
    setup({ router: routerSnapshot });
    await screen.findByText(/无法确认 LAN 设备范围/);
    expect(screen.queryByRole("checkbox")).not.toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "审阅客户端接管" }),
    ).toBeDisabled();
  });
  it("reports offline/error, retries router observation, and never posts automatically", async () => {
    const { state, mutations } = setup({ routerError: "router offline" });
    const user = userEvent.setup();
    await screen.findByText(/router offline · observation_unavailable/);
    expect(screen.queryByRole("checkbox")).not.toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "审阅客户端接管" }),
    ).toBeDisabled();
    state.routerError = undefined;
    await user.click(screen.getByRole("button", { name: "重试" }));
    await ready();
    expect(choice("Mac test")).toBeChecked();
    expect(mutations("POST")).toHaveLength(0);
  });
  it("hides stale discovered devices after a refresh error while retaining desired status", async () => {
    const { state } = setup({
      capture: {
        ...initialCapture,
        desired: true,
        clients: [
          { mac: second.mac, ip: second.ip, hostname: second.hostname },
        ],
        state: "suspended",
      },
    });
    const user = userEvent.setup();
    await ready();
    state.routerError = "router unavailable";
    await user.click(
      screen.getByRole("button", { name: "刷新设备与接管状态" }),
    );
    await screen.findByText(/router unavailable/);
    expect(
      screen.queryByRole("checkbox", { name: /Mac test/ }),
    ).not.toBeInTheDocument();
    expect(choice("Phone test")).toBeDisabled();
    expect(
      screen.getByRole("button", { name: "审阅客户端接管" }),
    ).toBeDisabled();
    expect(screen.getByText("已暂停")).toBeInTheDocument();
  });
  it("shows incomplete ARP observations instead of marking every unseen device offline", async () => {
    setup({
      router: {
        ...snapshot,
        errors: [
          {
            module: "devices.arp",
            code: "unavailable",
            message: "ARP source missing",
          },
        ],
      },
    });
    await ready();
    expect(screen.getByText("在线观察不完整")).toBeInTheDocument();
    expect(
      screen.getByText(/devices.arp · unavailable · ARP source missing/),
    ).toBeInTheDocument();
    expect(screen.queryByText("离线 / 未见 ARP")).not.toBeInTheDocument();
  });
  it("does not report a failed application as active or retry POST automatically", async () => {
    const { state, mutations } = setup();
    state.applyError = "current device unresolved";
    const user = userEvent.setup();
    await ready();
    await user.click(screen.getByRole("button", { name: "审阅客户端接管" }));
    await user.click(screen.getByRole("button", { name: "确认接管" }));
    await screen.findByText(/current device unresolved · capture_failed/);
    expect(screen.queryByText("已生效")).not.toBeInTheDocument();
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
    expect(mutations("POST")).toHaveLength(1);
  });
});
