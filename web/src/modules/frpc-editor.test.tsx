import { beforeEach, afterEach, describe, expect, it, vi } from "vitest";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { FrpcPage } from "./frpc";
import { commitButton, mockRuntime } from "./frpc-test-support";

vi.mock("../app/console-context", () => ({
  useConsole: () => ({ health: { mode: "host" } }),
}));
// JSDOM has no scrolling API; Radix scrolls the highlighted option.
const scrollDescriptor = Object.getOwnPropertyDescriptor(
  Element.prototype,
  "scrollIntoView",
);
beforeEach(() =>
  Object.defineProperty(Element.prototype, "scrollIntoView", {
    value: vi.fn(),
    configurable: true,
  }),
);
afterEach(() => {
  vi.unstubAllGlobals();
  if (scrollDescriptor)
    Object.defineProperty(
      Element.prototype,
      "scrollIntoView",
      scrollDescriptor,
    );
  else Reflect.deleteProperty(Element.prototype, "scrollIntoView");
});
describe("frpc mapping and native editors", () => {
  it("switches HTTP mappings to validated domains and commits native customDomains", async () => {
    const fetch = mockRuntime();
    const user = userEvent.setup();
    render(<FrpcPage />);
    await user.type(
      screen.getByRole("textbox", { name: "服务器地址" }),
      "frps.example.test",
    );
    screen.getByRole("combobox", { name: "映射 1 类型" }).focus();
    await user.keyboard("{Enter}");
    await user.click(screen.getByRole("option", { name: /^HTTP$/ }));
    expect(
      screen.queryByRole("spinbutton", { name: "远程端口" }),
    ).not.toBeInTheDocument();
    expect(commitButton()).toBeDisabled();
    await user.type(
      screen.getByRole("textbox", { name: /^域名/ }),
      "app.example.test, api.example.test",
    );
    await waitFor(() => expect(commitButton()).toBeEnabled());
    await user.click(commitButton());
    await screen.findByText(
      "frpc 配置已校验并保存。可在运行管理中启动；运行状态不代表远端连通。",
    );
    const call = fetch.mock.calls.find(
      ([url]) => url === "/api/runtime/configure",
    )!;
    const config = JSON.parse(call[1]?.body as string).config;
    expect(config).toContain('type = "http"');
    expect(config).toContain(
      'customDomains = ["app.example.test", "api.example.test"]',
    );
    expect(config).not.toContain("remotePort");
    expect(config).not.toContain("auth.");
  });
  it("retains mapping add/remove labels and empty state", async () => {
    mockRuntime();
    const user = userEvent.setup();
    render(<FrpcPage />);
    await user.click(screen.getByRole("button", { name: "删除映射 1" }));
    expect(screen.getByText("暂无服务映射")).toBeInTheDocument();
    expect(commitButton()).toBeDisabled();
    await user.click(screen.getByRole("button", { name: "添加" }));
    expect(
      screen.getByRole("region", { name: "服务映射 1" }),
    ).toBeInTheDocument();
    expect(screen.getByText("映射 1 / 64")).toBeInTheDocument();
  });
  it("keeps runtime controls above the secondary editor and loads private config only on request", async () => {
    const fetch = mockRuntime();
    const user = userEvent.setup();
    render(<FrpcPage />);
    await user.click(screen.getByRole("tab", { name: "原生配置" }));
    expect(
      screen.getByRole("heading", { name: "frpc 运行管理" }),
    ).toBeInTheDocument();
    expect(
      fetch.mock.calls.some(
        ([url]) => url === "/api/runtime/config?service=frpc",
      ),
    ).toBe(false);
    const panel = screen.getByRole("tabpanel", { name: "原生配置" });
    await waitFor(() =>
      expect(
        within(panel).getByRole("button", { name: "载入配置" }),
      ).toBeEnabled(),
    );
    await user.click(within(panel).getByRole("button", { name: "载入配置" }));
    await waitFor(() =>
      expect(
        within(panel).getByRole("textbox", { name: "原生配置内容" }),
      ).toHaveValue(
        'serverAddr = "private.example.test"\nauth.token = "synthetic-stored-token"\n',
      ),
    );
  });
});
