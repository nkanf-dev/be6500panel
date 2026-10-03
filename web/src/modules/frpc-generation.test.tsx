import { afterEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { FrpcPage } from "./frpc";
import {
  commitButton,
  frpcStatus,
  mockRuntime,
  setServer,
} from "./frpc-test-support";
import type { RuntimeStatus } from "../lib/contracts";

vi.mock("../app/console-context", () => ({
  useConsole: () => ({ health: { mode: "host" } }),
}));
afterEach(() => vi.unstubAllGlobals());
async function refresh(
  user: ReturnType<typeof userEvent.setup>,
  generation: number,
) {
  await user.click(screen.getByRole("button", { name: "刷新状态" }));
  await screen.findByText(`generation ${generation} · 版本未登记`);
}
const tokenInput = () => screen.getByLabelText("认证令牌", { exact: true });

describe("frpc sticky edit generation", () => {
  it("blocks stale edits and sends only after deliberate rebase without losing input", async () => {
    let observed = { ...frpcStatus };
    const fetch = mockRuntime({ observedStatus: () => observed });
    const user = userEvent.setup();
    render(<FrpcPage />);
    await screen.findByText("generation 7 · 版本未登记");
    await setServer(user);
    await user.type(tokenInput(), "synthetic-rebase-token");
    observed = { ...observed, generation: 9 };
    await refresh(user, 9);
    expect(screen.getByRole("alert")).toHaveTextContent("配置已被更新");
    expect(commitButton()).toBeDisabled();
    fireEvent.submit(screen.getByRole("tabpanel"));
    expect(
      fetch.mock.calls.some(([url]) => url === "/api/runtime/configure"),
    ).toBe(false);
    expect(tokenInput()).toHaveValue("synthetic-rebase-token");
    await user.type(screen.getByRole("textbox", { name: "名称" }), "-edited");
    expect(commitButton()).toBeDisabled();
    await user.click(
      screen.getByRole("button", { name: "读取最新配置并合并更改" }),
    );
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    expect(tokenInput()).toHaveValue("synthetic-rebase-token");
    await waitFor(() => expect(commitButton()).toBeEnabled());
    await user.click(commitButton());
    await screen.findByText(
      "frpc 配置已校验并保存。可在运行管理中启动；运行状态不代表远端连通。",
    );
    const call = fetch.mock.calls.find(
      ([url]) => url === "/api/runtime/configure",
    )!;
    const body = JSON.parse(call[1]?.body as string);
    expect(body.generation).toBe(9);
    expect(body.config).toContain('name = "service-1-edited"');
    expect(body.config).toContain('auth.token = "synthetic-rebase-token"');
  });
  it("does not freeze generation until the first edit", async () => {
    let observed = { ...frpcStatus };
    const fetch = mockRuntime({ observedStatus: () => observed });
    const user = userEvent.setup();
    render(<FrpcPage />);
    await screen.findByText("generation 7 · 版本未登记");
    observed = { ...observed, generation: 10 };
    await refresh(user, 10);
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    await setServer(user);
    await waitFor(() => expect(commitButton()).toBeEnabled());
    await user.click(commitButton());
    await screen.findByText(
      "frpc 配置已校验并保存。可在运行管理中启动；运行状态不代表远端连通。",
    );
    const call = fetch.mock.calls.find(
      ([url]) => url === "/api/runtime/configure",
    )!;
    expect(JSON.parse(call[1]?.body as string).generation).toBe(10);
  });
  it("captures bootstrap edits when the first generation becomes available", async () => {
    let observed: RuntimeStatus | undefined;
    mockRuntime({ observedStatus: () => observed });
    const user = userEvent.setup();
    render(<FrpcPage />);
    await setServer(user);
    expect(commitButton()).toBeDisabled();
    observed = { ...frpcStatus, generation: 3 };
    await refresh(user, 3);
    await waitFor(() => expect(commitButton()).toBeEnabled());
    observed = { ...observed, generation: 4 };
    await refresh(user, 4);
    expect(screen.getByRole("alert")).toHaveTextContent("配置已被更新");
    expect(commitButton()).toBeDisabled();
  });
  it("resets edit generation after successful Commit and freezes again on a new edit", async () => {
    let observed = { ...frpcStatus };
    mockRuntime({ observedStatus: () => observed });
    const user = userEvent.setup();
    render(<FrpcPage />);
    await screen.findByText("generation 7 · 版本未登记");
    await setServer(user);
    observed = { ...observed, generation: 8, configured: true };
    await user.click(commitButton());
    await screen.findByText(
      "frpc 配置已校验并保存。可在运行管理中启动；运行状态不代表远端连通。",
    );
    await screen.findByText("generation 8 · 版本未登记");
    observed = { ...observed, generation: 11 };
    await refresh(user, 11);
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    await user.type(tokenInput(), "synthetic-next-token");
    observed = { ...observed, generation: 12 };
    await refresh(user, 12);
    expect(screen.getByRole("alert")).toHaveTextContent("配置已被更新");
    expect(commitButton()).toBeDisabled();
  });
});
