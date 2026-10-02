import { beforeEach, afterEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { FrpcPage } from "./frpc";
import {
  commitButton,
  frpcStatus,
  mockRuntime,
  setServer,
} from "./frpc-test-support";

const consoleState = vi.hoisted(() => ({ mode: "host" }));
vi.mock("../app/console-context", () => ({
  useConsole: () => ({ health: { mode: consoleState.mode } }),
}));
beforeEach(() => {
  consoleState.mode = "host";
});
afterEach(() => vi.unstubAllGlobals());

describe("FrpcPage runtime configuration", () => {
  it("previews redacted TOML locally and sends nothing until explicit Commit", async () => {
    const fetch = mockRuntime();
    const user = userEvent.setup();
    render(<FrpcPage />);
    await setServer(user);
    await user.type(
      screen.getByLabelText("认证令牌", { exact: true }),
      "synthetic-private-token",
    );
    const preview = screen.getByLabelText("frpc TOML 预览");
    expect(preview).toHaveTextContent('serverAddr = "frps.example.test"');
    expect(preview).toHaveTextContent('auth.token = "[令牌已隐藏]"');
    expect(preview).not.toHaveTextContent("synthetic-private-token");
    expect(
      fetch.mock.calls.some(([url]) => url === "/api/runtime/configure"),
    ).toBe(false);
  });

  it("starts with no invented server or token, pending configuration and runtime controls", async () => {
    const fetch = mockRuntime();
    render(<FrpcPage />);
    await screen.findByText("generation 7 · 版本未登记");
    expect(screen.getByRole("textbox", { name: "服务器地址" })).toHaveValue("");
    expect(screen.getByLabelText("认证令牌", { exact: true })).toHaveValue("");
    expect(screen.getByLabelText("认证令牌", { exact: true })).toHaveAttribute(
      "type",
      "password",
    );
    expect(screen.getByRole("tab", { name: "连接与映射" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    expect(screen.getByText("尚未填写 frps 服务器")).toBeInTheDocument();
    expect(commitButton()).toBeDisabled();
    expect(fetch.mock.calls.every(([url]) => url === "/api/runtime")).toBe(
      true,
    );
  });
  it("saves generated native TOML with current generation and clears token only after success", async () => {
    const fetch = mockRuntime();
    const user = userEvent.setup();
    render(<FrpcPage />);
    await setServer(user);
    await user.type(
      screen.getByLabelText("认证令牌", { exact: true }),
      'synthetic"\\token',
    );
    await waitFor(() => expect(commitButton()).toBeEnabled());
    await user.click(commitButton());
    await screen.findByText(
      "frpc 配置 Commit 完成，已校验并保存。可在运行管理中启动。",
    );
    const call = fetch.mock.calls.find(
      ([url]) => url === "/api/runtime/configure",
    )!;
    const body = JSON.parse(call[1]?.body as string);
    expect(body).toMatchObject({ service: "frpc", generation: 7 });
    expect(body.config).toContain('serverAddr = "frps.example.test"');
    expect(body.config).toContain(
      String.raw`auth.token = "synthetic\"\\token"`,
    );
    expect(body.config).toContain("transport.tls.enable = true");
    expect(body.config).toContain("[[proxies]]");
    expect(body.config).toContain("localPort = 8080");
    expect(body.config).toContain("remotePort = 18080");
    expect(screen.getByLabelText("认证令牌", { exact: true })).toHaveValue("");
    expect(screen.getByText(/再次 Commit 需显式填写令牌/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "启动 frpc" })).toBeEnabled();
    expect(fetch.mock.calls.some(([url]) => url === "/api/runtime/start")).toBe(
      false,
    );
    expect(fetch.mock.calls.some(([url]) => url === "/api/frpc/plan")).toBe(
      false,
    );
  });
  it("keeps user token and shows the backend code when configuration is rejected", async () => {
    mockRuntime({ failure: true });
    const user = userEvent.setup();
    render(<FrpcPage />);
    await setServer(user);
    await user.type(
      screen.getByLabelText("认证令牌", { exact: true }),
      "synthetic-token",
    );
    await waitFor(() => expect(commitButton()).toBeEnabled());
    await user.click(commitButton());
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "generation_conflict",
    );
    expect(screen.getByLabelText("认证令牌", { exact: true })).toHaveValue(
      "synthetic-token",
    );
  });
  it.each([
    { missingStatus: true },
    { enabled: false },
    { artifactAvailable: false },
  ])("does not configure when runtime is unavailable: %s", async (options) => {
    const fetch = mockRuntime(options);
    const user = userEvent.setup();
    render(<FrpcPage />);
    await setServer(user);
    expect(commitButton()).toBeDisabled();
    fireEvent.submit(screen.getByRole("tabpanel"));
    expect(
      fetch.mock.calls.some(([url]) => url === "/api/runtime/configure"),
    ).toBe(false);
  });
  it("disables mutation in demo mode even if API runtime reports enabled", async () => {
    consoleState.mode = "demo";
    const fetch = mockRuntime();
    const user = userEvent.setup();
    render(<FrpcPage />);
    await setServer(user);
    expect(commitButton()).toBeDisabled();
    expect(screen.getByRole("button", { name: "启动 frpc" })).toBeDisabled();
    expect(
      fetch.mock.calls.some(([url]) => url === "/api/runtime/configure"),
    ).toBe(false);
  });
  it("validates local errors without sending native configuration", async () => {
    const fetch = mockRuntime();
    const user = userEvent.setup();
    render(<FrpcPage />);
    await setServer(user);
    await waitFor(() => expect(commitButton()).toBeEnabled());
    fireEvent.change(screen.getByRole("spinbutton", { name: "本地端口" }), {
      target: { value: "0" },
    });
    fireEvent.submit(screen.getByRole("tabpanel"));
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "映射 1本地端口必须为 1–65535 的整数",
    );
    expect(
      fetch.mock.calls.some(([url]) => url === "/api/runtime/configure"),
    ).toBe(false);
  });
  it("does not silently rebase edited input when another configuration generation appears", async () => {
    const fetch = mockRuntime();
    const user = userEvent.setup();
    render(<FrpcPage />);
    await screen.findByText("generation 7 · 版本未登记");
    await setServer(user);
    expect(commitButton()).toBeEnabled();
    fetch.mockImplementation(() =>
      Promise.resolve(
        new Response(
          JSON.stringify({
            enabled: true,
            services: [{ ...frpcStatus, generation: 9 }],
          }),
          { headers: { "Content-Type": "application/json" } },
        ),
      ),
    );
    await user.click(screen.getByRole("button", { name: "刷新状态" }));
    await screen.findByText(/当前输入已保留/);
    expect(commitButton()).toBeDisabled();
    expect(screen.getByRole("textbox", { name: "服务器地址" })).toHaveValue(
      "frps.example.test",
    );
    expect(
      fetch.mock.calls.filter(([url]) => url === "/api/runtime/configure"),
    ).toHaveLength(0);
    await user.click(
      screen.getByRole("button", { name: "使用最新 generation 审阅" }),
    );
    expect(commitButton()).toBeEnabled();
  });
});
