import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { RuntimeControls } from "./controls";
import { useRuntime } from "./use-runtime";
import { api, runRequest } from "../../lib/api";
import type { RuntimeService, RuntimeStatus } from "../../lib/contracts";
import { jsonResponse, runtimeStatus } from "../production-fixtures.test-data";
vi.mock("../../app/console-context", () => ({
  useConsole: () => ({ health: { mode: "host" } }),
}));
vi.mock("./artifact-inputs", () => ({ ArtifactInputs: () => null }));
afterEach(() => vi.unstubAllGlobals());
function View({ service = "sing-box" }: { service?: RuntimeService }) {
  const runtime = useRuntime(service);
  return <RuntimeControls runtime={runtime} />;
}
function install(service: RuntimeService = "sing-box", overrides = {}) {
  let status: RuntimeStatus = {
    ...runtimeStatus,
    service,
    state: "running",
    desired: true,
    ...overrides,
  };
  const fetch = vi.fn((url: string, init?: RequestInit) => {
    if (url === "/api/runtime")
      return Promise.resolve(
        jsonResponse({ enabled: true, services: [status] }),
      );
    if (url === "/api/runtime/restart") {
      status = { ...status, pid: 4000, restarts: status.restarts + 1 };
      return Promise.resolve(jsonResponse(status));
    }
    throw new Error(`Unexpected request ${url} ${init?.method}`);
  });
  vi.stubGlobal("fetch", fetch);
  return fetch;
}
describe("explicit managed runtime restart", () => {
  it.each(["sing-box", "frpc"] as const)(
    "restarts %s only after confirmation with one owner POST, never Stop + Start",
    async (service) => {
      const fetch = install(service);
      const user = userEvent.setup();
      render(<View service={service} />);
      await waitFor(() =>
        expect(screen.getByRole("button", { name: "重启服务" })).toBeEnabled(),
      );
      await user.click(screen.getByRole("button", { name: "重启服务" }));
      const dialog = screen.getByRole("alertdialog", { name: "确认重启服务" });
      expect(dialog).toHaveTextContent(
        service === "sing-box"
          ? "现有 sing-box 连接将短暂中断"
          : "frpc 映射将短暂中断",
      );
      expect(dialog).toHaveTextContent("独立 SSH 救援不受影响");
      expect(fetch.mock.calls.some(([, init]) => init?.method === "POST")).toBe(
        false,
      );
      await user.click(
        within(dialog).getByRole("button", { name: "确认重启服务" }),
      );
      await screen.findByText("重启操作已完成，请核对当前进程状态。");
      expect(screen.getByText("4000 / —")).toBeInTheDocument();
      const posts = fetch.mock.calls.filter(
        ([, init]) => init?.method === "POST",
      );
      expect(posts).toHaveLength(1);
      expect(posts[0]).toEqual([
        "/api/runtime/restart",
        expect.objectContaining({
          method: "POST",
          body: JSON.stringify({ service }),
        }),
      ]);
      expect(
        fetch.mock.calls.some(
          ([url]) =>
            url === "/api/runtime/stop" || url === "/api/runtime/start",
        ),
      ).toBe(false);
      expect(
        fetch.mock.calls.filter(([url]) => url === "/api/runtime").length,
      ).toBeGreaterThan(1);
    },
  );
  it("cancel makes no mutation", async () => {
    const fetch = install();
    const user = userEvent.setup();
    render(<View />);
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "重启服务" })).toBeEnabled(),
    );
    await user.click(screen.getByRole("button", { name: "重启服务" }));
    await user.click(screen.getByRole("button", { name: "取消" }));
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
    expect(fetch.mock.calls.some(([, init]) => init?.method === "POST")).toBe(
      false,
    );
  });
  it.each([{ configured: false }, { artifactAvailable: false }])(
    "requires accepted configuration and runtime artifact %j",
    async (overrides) => {
      install("sing-box", overrides);
      render(<View />);
      await screen.findByText("运行中");
      expect(screen.getByRole("button", { name: "重启服务" })).toBeDisabled();
    },
  );
  it("preserves the real failed status and backend error, with no mutation retry", async () => {
    const fetch = install();
    fetch.mockImplementation((url: string) =>
      Promise.resolve(
        url === "/api/runtime/restart"
          ? jsonResponse(
              {
                error: {
                  code: "runtime_restart_failed",
                  message: "进程重启失败",
                },
              },
              500,
            )
          : jsonResponse({
              enabled: true,
              services: [
                {
                  ...runtimeStatus,
                  state: "failed",
                  configured: true,
                  artifactAvailable: true,
                  errorCode: "process_exit",
                },
              ],
            }),
      ),
    );
    const user = userEvent.setup();
    render(<View />);
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "重启服务" })).toBeEnabled(),
    );
    await user.click(screen.getByRole("button", { name: "重启服务" }));
    await user.click(screen.getByRole("button", { name: "确认重启服务" }));
    await screen.findByText("进程重启失败 · runtime_restart_failed");
    expect(screen.getByText("故障")).toBeInTheDocument();
    expect(
      fetch.mock.calls.filter(([url]) => url === "/api/runtime/restart"),
    ).toHaveLength(1);
    expect(
      screen.queryByText("重启操作已完成，请核对当前进程状态。"),
    ).not.toBeInTheDocument();
  });
  it("API decodes returned status and uses the bounded restart operation timeout", async () => {
    const fetch = install();
    const timeout = vi.spyOn(AbortSignal, "timeout");
    await expect(
      runRequest(api.runtimeRestart("sing-box")),
    ).resolves.toMatchObject({ service: "sing-box", pid: 4000 });
    expect(timeout).toHaveBeenLastCalledWith(90_000);
    expect(fetch).toHaveBeenCalledOnce();
    timeout.mockRestore();
  });
});
