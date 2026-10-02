import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { ApiError } from "../lib/api";
import type { RouterSnapshot } from "../lib/contracts";
import { NetworkPage } from "./network";
import { UnavailablePage } from "./unavailable";

const state = vi.hoisted(() => ({ current: {} as Record<string, unknown> }));
vi.mock("../app/console-context", () => ({
  useConsole: () => state.current,
}));
vi.mock("../components/configuration", () => ({
  ConfigurationEditor: ({ module }: { module: string }) => (
    <section aria-label={`Configuration editor: ${module}`}>
      Config: {module}
    </section>
  ),
}));

const sample: RouterSnapshot = {
  platform: {
    model: "RN02-test",
    firmware: "test-firmware",
    kernel: "test-kernel",
    architecture: "arm-test",
  },
  devices: [
    {
      ip: "192.0.2.20",
      mac: "02:00:00:00:00:20",
      hostname: "sample-client",
      expiresAt: "2026-10-02T18:00:00Z",
      online: true,
    },
    {
      ip: "192.0.2.21",
      mac: "02:00:00:00:00:21",
      hostname: "",
      expiresAt: null,
      online: false,
    },
  ],
  wifi: [
    {
      name: "test-radio",
      ssid: "sample-network",
      band: "5g",
      channel: 36,
      bandwidth: "HE80",
      disabled: false,
      encryption: "sae-mixed",
    },
    {
      name: "test-disabled-radio",
      ssid: "",
      band: "2g",
      channel: 0,
      bandwidth: "HT20",
      disabled: true,
      encryption: "none",
    },
  ],
  dns: { resolvers: ["198.51.100.53", "2001:db8::53"], leaseCount: 7 },
  firewall: {
    ipv4: { input: "ACCEPT", forward: "DROP", output: "ACCEPT", rules: 17 },
    ipv6: { input: "DROP", forward: "REJECT", output: "ACCEPT", rules: 8 },
  },
  traffic: [
    {
      interface: "sample-wan",
      rxBytes: 1234567,
      txBytes: 2345678,
      rxBytesPerSecond: 2048.5,
      txBytesPerSecond: 512.25,
    },
  ],
  routes: [
    {
      family: "ipv4",
      destination: "0.0.0.0/0",
      gateway: "192.0.2.1",
      interface: "sample-wan",
      metric: 12,
    },
    {
      family: "ipv6",
      destination: "2001:db8::/64",
      gateway: "fe80::1",
      interface: "sample-lan",
      metric: 256,
    },
  ],
  sampledAt: "2026-10-02T16:20:30Z",
  errors: [],
};
const respond = (body: unknown) =>
  new Response(JSON.stringify(body), {
    status: 200,
    headers: { "Content-Type": "application/json" },
  });

beforeEach(() => {
  state.current = {
    router: sample,
    routerLoading: false,
    refreshRouter: vi.fn(),
  };
  vi.stubGlobal(
    "fetch",
    vi.fn().mockResolvedValue(
      respond({
        interfaces: [
          {
            name: "host-interface",
            addresses: ["192.0.2.2/24"],
            up: true,
            mtu: 1500,
          },
        ],
        routes: [],
        routeObservationSupported: false,
      }),
    ),
  );
});
afterEach(() => vi.unstubAllGlobals());

describe("router snapshot pages", () => {
  it.each([
    ["wifi", "wireless"],
    ["dns", "dhcp"],
    ["firewall", "firewall"],
  ] as const)(
    "opens the supported %s configuration editor only from the config tab",
    async (id, module) => {
      const user = userEvent.setup();
      render(<UnavailablePage id={id} />);
      expect(screen.getByRole("tab", { name: "观察" })).toHaveAttribute(
        "aria-selected",
        "true",
      );
      expect(
        screen.queryByRole("region", {
          name: `Configuration editor: ${module}`,
        }),
      ).not.toBeInTheDocument();
      await user.click(screen.getByRole("tab", { name: "配置" }));
      expect(
        screen.getByRole("region", { name: `Configuration editor: ${module}` }),
      ).toBeInTheDocument();
      expect(
        screen.queryByRole("button", { name: "刷新路由器观察" }),
      ).not.toBeInTheDocument();
      await user.click(screen.getByRole("tab", { name: "观察" }));
      expect(
        screen.getByRole("button", { name: "刷新路由器观察" }),
      ).toBeInTheDocument();
    },
  );

  it("does not invent a device configuration editor", () => {
    render(<UnavailablePage id="devices" />);
    expect(screen.queryByRole("tab", { name: "配置" })).not.toBeInTheDocument();
    expect(screen.getByText("sample-client")).toBeInTheDocument();
  });

  it("labels a retained snapshot as the previous sample during refresh", () => {
    state.current.routerLoading = true;
    render(<UnavailablePage id="devices" />);
    expect(screen.getByText("更新中 · 上次采样")).toBeInTheDocument();
    expect(screen.getByText("sample-client")).toBeInTheDocument();
    expect(screen.queryByText("采样快照")).not.toBeInTheDocument();
  });

  it("shows observed identities, lease expiry, ARP status, and the sample time", () => {
    render(<UnavailablePage id="devices" />);
    expect(screen.getByText("sample-client")).toBeInTheDocument();
    expect(screen.getByText("192.0.2.20")).toBeInTheDocument();
    expect(screen.getByText("02:00:00:00:00:20")).toBeInTheDocument();
    expect(screen.getByText("ARP 已观测")).toBeInTheDocument();
    expect(screen.getByText("未见 ARP")).toBeInTheDocument();
    expect(
      document.querySelector('time[datetime="2026-10-02T18:00:00Z"]'),
    ).not.toBeNull();
    expect(
      document.querySelector('time[datetime="2026-10-02T16:20:30Z"]'),
    ).not.toBeNull();
    expect(
      screen.queryByRole("button", { name: "添加策略" }),
    ).not.toBeInTheDocument();
  });

  it("shows safe wireless configuration fields without credential or configuration controls", () => {
    render(<UnavailablePage id="wifi" />);
    for (const value of [
      "test-radio",
      "sample-network",
      "5g",
      "36",
      "HE80",
      "sae-mixed",
      "启用",
      "禁用",
      "自动 / 未知",
    ]) {
      expect(screen.getByText(value)).toBeInTheDocument();
    }
    expect(
      screen.queryByRole("button", { name: "新建配置" }),
    ).not.toBeInTheDocument();
    expect(screen.queryByText(/密码|口令/)).not.toBeInTheDocument();
  });

  it("shows resolver addresses and the actual DHCP lease count", () => {
    render(<UnavailablePage id="dns" />);
    expect(screen.getByText("198.51.100.53")).toBeInTheDocument();
    expect(screen.getByText("2001:db8::53")).toBeInTheDocument();
    expect(screen.getByText("DHCP 租约数：7")).toBeInTheDocument();
  });

  it("shows both firewall families, policies, and observed rule counts", () => {
    render(<UnavailablePage id="firewall" />);
    const ipv4 = screen.getByText("ipv4").closest("tr")!;
    const ipv6 = screen.getByText("ipv6").closest("tr")!;
    expect(within(ipv4).getByText("DROP")).toBeInTheDocument();
    expect(within(ipv4).getByText("17")).toBeInTheDocument();
    expect(within(ipv6).getByText("REJECT")).toBeInTheDocument();
    expect(within(ipv6).getByText("8")).toBeInTheDocument();
  });

  it("keeps partial observations and shows only relevant module/code/message errors", () => {
    state.current.router = {
      ...sample,
      errors: [
        {
          module: "devices.arp",
          code: "unavailable",
          message: "ARP source missing",
        },
        { module: "dns", code: "invalid", message: "Resolver row invalid" },
      ],
    };
    render(<UnavailablePage id="devices" />);
    expect(screen.getByText("sample-client")).toBeInTheDocument();
    expect(screen.getByText("devices.arp")).toBeInTheDocument();
    expect(screen.getByText("unavailable")).toBeInTheDocument();
    expect(screen.getByText("ARP source missing")).toBeInTheDocument();
    expect(screen.getByText("部分观察")).toBeInTheDocument();
    expect(screen.queryByText("Resolver row invalid")).not.toBeInTheDocument();
  });

  it("does not present unavailable firewall counters as a successful zero observation", () => {
    state.current.router = {
      ...sample,
      firewall: {
        ...sample.firewall,
        ipv6: { input: "", forward: "", output: "", rules: 0 },
      },
      errors: [
        {
          module: "firewall.ipv6",
          code: "unavailable",
          message: "IPv6 firewall source missing",
        },
      ],
    };
    render(<UnavailablePage id="firewall" />);
    const ipv6 = screen.getByText("ipv6").closest("tr")!;
    expect(within(ipv6).queryByText("0")).not.toBeInTheDocument();
    expect(screen.getByText("firewall.ipv6")).toBeInTheDocument();
    expect(
      screen.getByText("IPv6 firewall source missing"),
    ).toBeInTheDocument();
    expect(screen.getByText("17")).toBeInTheDocument();
  });

  it("does not claim a device is absent from ARP when the ARP source is missing", () => {
    state.current.router = {
      ...sample,
      errors: [
        {
          module: "devices.arp",
          code: "unavailable",
          message: "ARP source missing",
        },
      ],
    };
    render(<UnavailablePage id="devices" />);
    expect(screen.getByText("ARP 观察不完整")).toBeInTheDocument();
    expect(screen.queryByText("未见 ARP")).not.toBeInTheDocument();
    expect(screen.getByText("ARP 已观测")).toBeInTheDocument();
  });

  it("shows the lease source error on DNS instead of a false successful count", () => {
    state.current.router = {
      ...sample,
      dns: { ...sample.dns, leaseCount: 0 },
      errors: [
        {
          module: "devices.leases",
          code: "unavailable",
          message: "DHCP source missing",
        },
      ],
    };
    render(<UnavailablePage id="dns" />);
    expect(screen.getByText("DHCP 租约数：—")).toBeInTheDocument();
    expect(screen.getByText("devices.leases")).toBeInTheDocument();
    expect(screen.getByText("DHCP source missing")).toBeInTheDocument();
    expect(screen.getByText("198.51.100.53")).toBeInTheDocument();
  });

  it.each([
    ["devices", "sample-client"],
    ["wifi", "sample-network"],
    ["dns", "198.51.100.53"],
    ["firewall", "17"],
  ] as const)(
    "hides stale %s data after the router request fails",
    (id, staleValue) => {
      state.current.routerError = new ApiError({
        code: "observation_unavailable",
        message: "Router sample failed",
      });
      render(<UnavailablePage id={id} />);
      expect(screen.queryByText(staleValue)).not.toBeInTheDocument();
      expect(screen.getByRole("alert")).toHaveTextContent(
        "observation_unavailable",
      );
      expect(screen.getByRole("alert")).toHaveTextContent(
        "Router sample failed",
      );
      expect(screen.queryByText("采样快照")).not.toBeInTheDocument();
      expect(document.querySelector("time")).toBeNull();
    },
  );

  it("refreshes the authenticated router observation", async () => {
    const user = userEvent.setup();
    render(<UnavailablePage id="wifi" />);
    await user.click(screen.getByRole("button", { name: "刷新路由器观察" }));
    expect(state.current.refreshRouter).toHaveBeenCalledOnce();
  });

  it("shows initial loading without invented observation rows", () => {
    state.current = { routerLoading: true, refreshRouter: vi.fn() };
    render(<UnavailablePage id="devices" />);
    expect(screen.getByRole("status")).toHaveTextContent("正在读取路由器观察");
    expect(screen.queryByText("sample-client")).not.toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "刷新路由器观察" }),
    ).toBeDisabled();
  });
});

describe("network router observations", () => {
  it("keeps all interface, route, and counter observations on the observation tab", async () => {
    const user = userEvent.setup();
    render(<NetworkPage />);
    await screen.findByText("host-interface");
    await user.click(screen.getByRole("tab", { name: "配置" }));
    expect(
      screen.getByRole("region", { name: "Configuration editor: network" }),
    ).toBeInTheDocument();
    expect(screen.queryByText("host-interface")).not.toBeInTheDocument();
    expect(screen.queryByText("0.0.0.0/0")).not.toBeInTheDocument();
    expect(screen.queryByText("1,234,567")).not.toBeInTheDocument();
    await user.click(screen.getByRole("tab", { name: "观察" }));
    expect(screen.getByText("host-interface")).toBeInTheDocument();
    expect(screen.getByText("0.0.0.0/0")).toBeInTheDocument();
    expect(screen.getByText("1,234,567")).toBeInTheDocument();
  });

  it("keeps the interface API and shows real routes and per-interface counters from the snapshot", async () => {
    render(<NetworkPage />);
    await screen.findByText("host-interface");
    const routes = within(screen.getByRole("table", { name: "路由表" }));
    for (const value of [
      "0.0.0.0/0",
      "192.0.2.1",
      "sample-wan",
      "12",
      "2001:db8::/64",
      "fe80::1",
      "sample-lan",
      "256",
    ]) {
      expect(routes.getByText(value)).toBeInTheDocument();
    }
    const traffic = within(screen.getByRole("table", { name: "接口流量计数" }));
    for (const value of [
      "sample-wan",
      "1,234,567",
      "2,345,678",
      "2,048.5",
      "512.25",
    ]) {
      expect(traffic.getByText(value)).toBeInTheDocument();
    }
    expect(screen.queryByText("路由观察未接入")).not.toBeInTheDocument();
  });

  it("shows route and traffic source errors while preserving successful partial rows", async () => {
    state.current.router = {
      ...sample,
      errors: [
        {
          module: "routes.ipv6",
          code: "unavailable",
          message: "IPv6 route source missing",
        },
        { module: "traffic", code: "invalid", message: "Counter row invalid" },
      ],
    };
    render(<NetworkPage />);
    await screen.findByText("host-interface");
    expect(screen.getByText("routes.ipv6")).toBeInTheDocument();
    expect(screen.getByText("IPv6 route source missing")).toBeInTheDocument();
    expect(screen.getByText("traffic")).toBeInTheDocument();
    expect(screen.getByText("Counter row invalid")).toBeInTheDocument();
    expect(screen.getByText("0.0.0.0/0")).toBeInTheDocument();
    expect(screen.getByText("2,048.5")).toBeInTheDocument();
  });

  it("hides stale router routes and counters after a request error but keeps host interfaces", async () => {
    state.current.routerError = new ApiError({
      code: "network_error",
      message: "Router connection failed",
    });
    render(<NetworkPage />);
    await screen.findByText("host-interface");
    expect(screen.queryByText("0.0.0.0/0")).not.toBeInTheDocument();
    expect(screen.queryByText("1,234,567")).not.toBeInTheDocument();
    expect(
      screen
        .getAllByRole("alert")
        .some((alert) => alert.textContent?.includes("network_error")),
    ).toBe(true);
  });

  it("refreshes both interface and router observations", async () => {
    const user = userEvent.setup();
    render(<NetworkPage />);
    await screen.findByText("host-interface");
    await user.click(screen.getByRole("button", { name: "刷新" }));
    expect(state.current.refreshRouter).toHaveBeenCalledOnce();
    expect(fetch).toHaveBeenCalledTimes(2);
  });

  it("accepts legacy context mocks without router fields as empty observations", async () => {
    state.current = { health: { mode: "host" }, capabilities: [] };
    render(<NetworkPage />);
    await screen.findByText("host-interface");
    expect(screen.getByText("路由观察未接入")).toBeInTheDocument();
    expect(screen.queryByText("0.0.0.0/0")).not.toBeInTheDocument();
  });
});
