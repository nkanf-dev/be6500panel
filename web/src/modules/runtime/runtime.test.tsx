import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { useRuntime } from "./use-runtime";
import { RuntimeControls } from "./controls";
import { NativeConfigEditor } from "./native-config-editor";
import { jsonResponse, runtimeStatus } from "../production-fixtures.test-data";

const consoleState = vi.hoisted(() => ({ health: { mode: "host" } }));
vi.mock("../../app/console-context", () => ({
  useConsole: () => consoleState,
}));
function RuntimeView() {
  const runtime = useRuntime("sing-box");
  return (
    <>
      <RuntimeControls runtime={runtime} />
      <NativeConfigEditor runtime={runtime} />
    </>
  );
}
afterEach(() => {
  vi.unstubAllGlobals();
  consoleState.health.mode = "host";
});
const setup = (
  extra?: (url: string, init: RequestInit) => Response | undefined,
) => {
  const fetch = vi.fn((url: string, init: RequestInit) =>
    Promise.resolve(
      extra?.(url, init) ??
        (url === "/api/runtime"
          ? jsonResponse({ enabled: true, services: [runtimeStatus] })
          : jsonResponse(runtimeStatus)),
    ),
  );
  vi.stubGlobal("fetch", fetch);
  return fetch;
};
describe("managed runtime controls", () => {
  it("acquires explicitly selected URL, checksum, version and compression then starts/stops", async () => {
    const fetch = setup();
    const user = userEvent.setup();
    render(<RuntimeView />);
    await screen.findByText("已停止");
    await user.click(screen.getByText("运行文件 · HTTPS / SHA-256"));
    await user.type(
      screen.getByLabelText("运行文件 URL"),
      "https://artifacts.example.test/core.gz",
    );
    await user.type(screen.getByLabelText("SHA-256"), "a".repeat(64));
    await user.type(screen.getByLabelText("文件版本"), "test-2");
    await user.click(
      screen.getByRole("button", { name: "校验并获取运行文件" }),
    );
    await screen.findByText("校验下载完成，运行文件已激活");
    expect(fetch).toHaveBeenCalledWith(
      "/api/runtime/acquire",
      expect.objectContaining({
        method: "POST",
        body: JSON.stringify({
          service: "sing-box",
          artifact: {
            url: "https://artifacts.example.test/core.gz",
            sha256: "a".repeat(64),
            version: "test-2",
            compression: "gzip",
          },
        }),
      }),
    );
    await user.click(screen.getByRole("button", { name: "启动 sing-box" }));
    await screen.findByText("启动请求已接受");
    await user.click(screen.getByRole("button", { name: "停止 sing-box" }));
    await screen.findByText("进程已停止，所属接管规则已清理");
    expect(fetch).toHaveBeenCalledWith(
      "/api/runtime/stop",
      expect.objectContaining({
        method: "POST",
        body: JSON.stringify({ service: "sing-box" }),
      }),
    );
  });
  it("does not enable runtime mutations in demo, even if backend advertises enabled", async () => {
    consoleState.health.mode = "demo";
    setup();
    render(<RuntimeView />);
    await screen.findByText("运行管理未启用");
    expect(
      screen.getByRole("button", { name: "启动 sing-box" }),
    ).toBeDisabled();
    expect(screen.getByRole("button", { name: "载入配置" })).toBeDisabled();
  });
  it("loads native generation, reviews before Commit, preserves edits and backend conflict code", async () => {
    const fetch = setup((url) =>
      url.startsWith("/api/runtime/config?")
        ? jsonResponse({ service: "sing-box", config: "{}", generation: 4 })
        : url === "/api/runtime/configure"
          ? jsonResponse(
              { error: { code: "generation_conflict", message: "配置已变化" } },
              409,
            )
          : undefined,
    );
    const user = userEvent.setup();
    render(<RuntimeView />);
    await screen.findByText("已停止");
    await user.click(screen.getByRole("button", { name: "载入配置" }));
    await waitFor(() =>
      expect(screen.getByLabelText("原生配置内容")).toHaveValue("{}"),
    );
    await user.clear(screen.getByLabelText("原生配置内容"));
    fireEvent.change(screen.getByLabelText("原生配置内容"), {
      target: { value: '{"log":{}}' },
    });
    expect(
      fetch.mock.calls.filter(([url]) => url === "/api/runtime/configure"),
    ).toHaveLength(0);
    expect(
      screen.getByRole("button", { name: "校验并应用更改" }),
    ).toBeDisabled();
    await user.click(screen.getByRole("button", { name: "审阅配置差异" }));
    await user.click(screen.getByRole("button", { name: "校验并应用更改" }));
    await screen.findByText("配置已变化 · generation_conflict");
    expect(screen.getByLabelText("原生配置内容")).toHaveValue('{"log":{}}');
    expect(fetch).toHaveBeenCalledWith(
      "/api/runtime/configure",
      expect.objectContaining({
        body: JSON.stringify({
          service: "sing-box",
          config: '{"log":{}}',
          generation: 4,
        }),
      }),
    );
    expect(
      fetch.mock.calls.filter(([url]) => url === "/api/runtime/configure"),
    ).toHaveLength(1);
  });
  it("shows invalid_response instead of crashing on malformed runtime payload", async () => {
    setup(() => jsonResponse({ enabled: true, services: [{}] }));
    render(<RuntimeView />);
    await screen.findByText(/invalid_response/);
    expect(
      screen.getByRole("button", { name: "启动 sing-box" }),
    ).toBeDisabled();
  });
  it("creates a first raw config locally with the observed generation, without fetching missing config", async () => {
    const fetch = setup((url) =>
      url === "/api/runtime"
        ? jsonResponse({
            enabled: true,
            services: [{ ...runtimeStatus, configured: false, generation: 0 }],
          })
        : undefined,
    );
    const user = userEvent.setup();
    render(<RuntimeView />);
    await screen.findByRole("button", { name: "新建原生配置" });
    await user.click(screen.getByRole("button", { name: "新建原生配置" }));
    fireEvent.change(screen.getByLabelText("原生配置内容"), {
      target: { value: "{}" },
    });
    expect(
      fetch.mock.calls.filter(([url]) =>
        url.startsWith("/api/runtime/config?"),
      ),
    ).toHaveLength(0);
    await user.click(screen.getByRole("button", { name: "审阅配置差异" }));
    await user.click(screen.getByRole("button", { name: "校验并应用更改" }));
    await screen.findByText("原生配置已校验并保存");
    expect(fetch).toHaveBeenCalledWith(
      "/api/runtime/configure",
      expect.objectContaining({
        body: JSON.stringify({
          service: "sing-box",
          config: "{}",
          generation: 0,
        }),
      }),
    );
  });
});
