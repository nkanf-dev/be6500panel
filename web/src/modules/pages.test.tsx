import { describe, expect, it, vi, afterEach } from "vitest";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { NetworkPage } from "./network";
import { ProxyPage } from "./proxy";
import { FrpcPage } from "./frpc";
import { LogsPanel } from "./logs";
vi.mock("../app/console-context", () => ({
  useConsole: () => ({ health: { mode: "host" }, capabilities: [] }),
}));
vi.mock("../components/visualizations", () => ({
  RequestWaterfall: () => null,
  RuleHitChart: () => null,
  LatencyDistribution: () => null,
}));
const respond = (body: unknown) =>
  new Response(JSON.stringify(body), {
    status: 200,
    headers: { "Content-Type": "application/json" },
  });
const plan = {
  id: "plan-sample",
  generation: 1,
  readOnly: true,
  summary: "联合校验通过",
  steps: [{ module: "network", action: "validate", detail: "管理接口已排除" }],
  warnings: ["外部暴露需要访问控制"],
  canApply: false,
};
afterEach(() => vi.unstubAllGlobals());
describe("domain pages", () => {
  it("renders host network observations, filters and exposes keyboard-accessible details", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue(
        respond({
          interfaces: [
            {
              name: "eth-test",
              addresses: ["192.0.2.2/24"],
              up: true,
              mtu: 1500,
            },
            { name: "lo-test", addresses: [], up: false, mtu: 65536 },
          ],
          routes: [],
          routeObservationSupported: false,
        }),
      ),
    );
    const user = userEvent.setup();
    render(<NetworkPage />);
    await screen.findByText("eth-test");
    await user.type(
      screen.getByRole("textbox", { name: "筛选接口" }),
      "192.0.2",
    );
    expect(screen.queryByText("lo-test")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "查看 eth-test" }));
    expect(screen.getByText("接口详情")).toBeInTheDocument();
    expect(screen.getByText("路由观察未接入")).toBeInTheDocument();
  });
  it("submits proxy plans via POST and never enables unsupported apply", async () => {
    const fetch = vi.fn().mockResolvedValue(respond(plan));
    vi.stubGlobal("fetch", fetch);
    const user = userEvent.setup();
    render(<ProxyPage />);
    await user.click(screen.getByRole("button", { name: "校验并生成计划" }));
    await screen.findByText("联合校验通过");
    expect(fetch).toHaveBeenCalledWith(
      "/api/proxy/plan",
      expect.objectContaining({
        method: "POST",
        body: JSON.stringify({
          mode: "split",
          dnsStrategy: "split",
          ipv6Policy: "follow",
          failurePolicy: "block-proxy",
          nodeCount: 1,
        }),
      }),
    );
    expect(screen.getByRole("button", { name: "应用计划" })).toBeDisabled();
  });
  it("builds credential-free frpc mappings through the actual contract", async () => {
    const fetch = vi.fn((url: string) =>
      Promise.resolve(
        respond(
          url === "/api/frpc"
            ? {
                supported: false,
                running: false,
                reason: "未接入",
                proxies: [],
              }
            : plan,
        ),
      ),
    );
    vi.stubGlobal("fetch", fetch);
    const user = userEvent.setup();
    render(<FrpcPage />);
    await user.type(
      screen.getByRole("textbox", { name: "服务器地址" }),
      "frps.example.com",
    );
    await user.click(screen.getByRole("button", { name: "校验并生成计划" }));
    await screen.findByText("联合校验通过");
    const call = fetch.mock.calls.find(
      ([url]) => url === "/api/frpc/plan",
    ) as unknown as [string, RequestInit];
    expect(call).toBeDefined();
    const body = JSON.parse(call[1].body as string);
    expect(body.serverAddress).toBe("frps.example.com");
    expect(body.tls).toBe(true);
    expect(body.proxies[0]).toMatchObject({
      type: "tcp",
      localPort: 8080,
      remotePort: 18080,
    });
    expect(body).not.toHaveProperty("token");
    expect(screen.getByRole("button", { name: "应用计划" })).toBeDisabled();
  });
  it("loads structured logs on demand and filters codes", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue(
        respond({
          capacity: 500,
          entries: [
            {
              sequence: 1,
              time: "2026-01-01T00:00:00Z",
              level: "INFO",
              code: "startup",
              module: "core",
              message: "就绪",
            },
            {
              sequence: 2,
              time: "2026-01-01T00:00:01Z",
              level: "WARN",
              code: "validation_rejected",
              module: "frpc",
              message: "端口无效",
            },
          ],
        }),
      ),
    );
    const user = userEvent.setup();
    render(<LogsPanel />);
    await screen.findByText("validation_rejected");
    await user.type(
      screen.getByRole("textbox", { name: "筛选日志" }),
      "validation",
    );
    expect(screen.queryByText("startup")).not.toBeInTheDocument();
    const rows = within(screen.getByRole("table")).getAllByRole("row");
    expect(rows).toHaveLength(2);
  });
  it("supports removing and adding mapping rows", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue(
        respond({
          supported: false,
          running: false,
          reason: "未接入",
          proxies: [],
        }),
      ),
    );
    const user = userEvent.setup();
    render(<FrpcPage />);
    await user.click(screen.getByRole("button", { name: "删除映射 1" }));
    expect(screen.getByText("暂无服务映射")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "添加" }));
    expect(
      screen.getByRole("region", { name: "服务映射 1" }),
    ).toBeInTheDocument();
    await waitFor(() =>
      expect(screen.getByText("映射 1 / 64")).toBeInTheDocument(),
    );
  });
});
