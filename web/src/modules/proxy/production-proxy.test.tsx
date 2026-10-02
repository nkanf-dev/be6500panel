import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ProxyPage } from "../proxy";
import {
  jsonResponse,
  proxyNodes,
  routerSnapshot,
  runtimeStatus,
} from "../production-fixtures.test-data";
vi.mock("../../app/console-context", () => ({
  useConsole: () => ({ health: { mode: "host" } }),
}));
afterEach(() => vi.unstubAllGlobals());
function setup() {
  let imported = false;
  let selected = false;
  const fetch = vi.fn((url: string, init: RequestInit) => {
    if (url === "/api/runtime")
      return Promise.resolve(
        jsonResponse({ enabled: true, services: [runtimeStatus] }),
      );
    if (url === "/api/proxy/nodes")
      return Promise.resolve(
        jsonResponse(
          imported
            ? {
                ...proxyNodes,
                selectedNodeId: selected ? "node-synthetic" : "",
              }
            : { nodes: [], selectedNodeId: "", diagnostics: [] },
        ),
      );
    if (url === "/api/proxy/import") {
      imported = true;
      return Promise.resolve(jsonResponse(proxyNodes));
    }
    if (url === "/api/proxy/select") {
      selected = true;
      return Promise.resolve(
        jsonResponse({
          status: runtimeStatus,
          configSHA256: "b".repeat(64),
          diagnostics: [],
        }),
      );
    }
    if (url === "/api/proxy/capture" && init.method === "POST")
      return Promise.resolve(
        jsonResponse({ active: true, clientIPv4: "192.0.2.20", commands: 8 }),
      );
    if (url === "/api/proxy/capture")
      return Promise.resolve(jsonResponse({ active: false, commands: 0 }));
    throw new Error(`Unexpected request: ${url}`);
  });
  vi.stubGlobal("fetch", fetch);
  return fetch;
}
describe("real proxy operations", () => {
  it("imports one private HTTPS URL, selects a named node and Commits exact native compiler inputs", async () => {
    const fetch = setup();
    const user = userEvent.setup();
    render(<ProxyPage />);
    await waitFor(() =>
      expect(
        screen.getByRole("button", { name: "解析并导入节点" }),
      ).toBeEnabled(),
    );
    await user.type(
      screen.getByLabelText("私密订阅 URL"),
      "https://subscriptions.example.test/private-test",
    );
    await user.click(screen.getByRole("button", { name: "解析并导入节点" }));
    await screen.findByText("Synthetic Node");
    expect(screen.getByLabelText("私密订阅 URL")).toHaveValue("");
    expect(fetch).toHaveBeenCalledWith(
      "/api/proxy/import",
      expect.objectContaining({
        method: "POST",
        body: JSON.stringify({
          url: "https://subscriptions.example.test/private-test",
        }),
      }),
    );
    await user.click(
      screen.getByRole("radio", { name: "选择节点 Synthetic Node" }),
    );
    expect(
      fetch.mock.calls.filter(([url]) => url === "/api/proxy/select"),
    ).toHaveLength(0);
    await user.click(
      screen.getByRole("button", { name: "生成并 Commit 节点配置" }),
    );
    await screen.findByText(/配置已校验并保存 · SHA-256/);
    expect(fetch).toHaveBeenCalledWith(
      "/api/proxy/select",
      expect.objectContaining({
        body: JSON.stringify({
          nodeId: "node-synthetic",
          ipv6: "direct",
          failure: "direct",
          ports: { mixed: 2080, tproxy: 7893, dns: 6450 },
        }),
      }),
    );
    expect(
      fetch.mock.calls.filter(([url]) => url === "/api/proxy/capture"),
    ).toHaveLength(0);
  });
  it("imports YAML content without URL or client-side conversion", async () => {
    const fetch = setup();
    const user = userEvent.setup();
    render(<ProxyPage />);
    await waitFor(() =>
      expect(
        screen.getByRole("button", { name: "解析并导入节点" }),
      ).toBeEnabled(),
    );
    await user.click(screen.getByRole("button", { name: "YAML 内容" }));
    const content = "proxies: []\n";
    fireEvent.change(screen.getByLabelText("订阅 YAML"), {
      target: { value: content },
    });
    await user.click(screen.getByRole("button", { name: "解析并导入节点" }));
    await screen.findByText("已导入 1 个节点");
    expect(fetch).toHaveBeenCalledWith(
      "/api/proxy/import",
      expect.objectContaining({ body: JSON.stringify({ content }) }),
    );
    expect(screen.getByLabelText("订阅 YAML")).toHaveValue("");
  });
  it("captures only checked observed devices after a second confirmation", async () => {
    const fetch = setup();
    let active = false;
    fetch.mockImplementation((url: string, init: RequestInit) => {
      if (url === "/api/runtime")
        return Promise.resolve(
          jsonResponse({
            enabled: true,
            services: [{ ...runtimeStatus, state: "running", desired: true }],
          }),
        );
      if (url === "/api/router")
        return Promise.resolve(
          jsonResponse({
            ...routerSnapshot,
            currentClientIP: "192.0.2.20",
            devices: [{ ...routerSnapshot.devices[0], eligible: true }],
          }),
        );
      if (url === "/api/proxy/nodes")
        return Promise.resolve(jsonResponse(proxyNodes));
      if (url === "/api/proxy/capture" && init.method === "POST") active = true;
      return Promise.resolve(
        jsonResponse({
          active,
          desired: active,
          clients: active
            ? [
                {
                  mac: "02:00:00:00:00:20",
                  ip: "192.0.2.20",
                  hostname: "test-client",
                },
              ]
            : [],
          ipv6: "direct",
          commands: active ? 8 : 0,
        }),
      );
    });
    const user = userEvent.setup();
    render(<ProxyPage />);
    await user.click(screen.getByRole("tab", { name: "客户端接管" }));
    await waitFor(() =>
      expect(
        screen.getByRole("button", { name: "审阅客户端接管" }),
      ).toBeEnabled(),
    );
    expect(
      screen.getByRole("checkbox", { name: /选择设备 test-client/ }),
    ).toBeChecked();
    await user.click(screen.getByRole("button", { name: "审阅客户端接管" }));
    expect(
      screen.getByRole("alertdialog", { name: "确认客户端接管" }),
    ).toBeInTheDocument();
    expect(
      fetch.mock.calls.filter(
        ([url, init]) => url === "/api/proxy/capture" && init.method === "POST",
      ),
    ).toHaveLength(0);
    await user.click(screen.getByRole("button", { name: "确认接管客户端" }));
    await screen.findByText("接管中");
    expect(fetch).toHaveBeenCalledWith(
      "/api/proxy/capture",
      expect.objectContaining({
        method: "POST",
        body: JSON.stringify({
          devices: [{ mac: "02:00:00:00:00:20" }],
          ipv6: "direct",
        }),
      }),
    );
  });
  it("blocks node Commit and runtime tab switching while a subscription import is pending", async () => {
    const fetch = setup();
    let complete: (value: Response) => void = () => {};
    fetch.mockImplementation((url: string) =>
      url === "/api/runtime"
        ? Promise.resolve(
            jsonResponse({ enabled: true, services: [runtimeStatus] }),
          )
        : url === "/api/proxy/nodes"
          ? Promise.resolve(
              jsonResponse({ ...proxyNodes, selectedNodeId: "node-synthetic" }),
            )
          : new Promise<Response>((resolve) => {
              complete = resolve;
            }),
    );
    const user = userEvent.setup();
    render(<ProxyPage />);
    await waitFor(() =>
      expect(
        screen.getByRole("button", { name: "生成并 Commit 节点配置" }),
      ).toBeEnabled(),
    );
    await user.type(
      screen.getByLabelText("私密订阅 URL"),
      "https://subscriptions.example.test/new",
    );
    await user.click(screen.getByRole("button", { name: "解析并导入节点" }));
    expect(
      screen.getByRole("button", { name: "生成并 Commit 节点配置" }),
    ).toBeDisabled();
    expect(screen.getByRole("tab", { name: "运行管理" })).toBeDisabled();
    complete(jsonResponse(proxyNodes));
    await screen.findByText("已导入 1 个节点");
    expect(screen.getByRole("tab", { name: "运行管理" })).toBeEnabled();
  });
});
