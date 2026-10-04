import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ProxyPage } from "../proxy";
import { jsonResponse, runtimeStatus } from "../production-fixtures.test-data";
vi.mock("../../app/console-context", () => ({
  useOptionalConsole: () => undefined,
  useConsole: () => ({ health: { mode: "host" } }),
}));
afterEach(() => vi.unstubAllGlobals());
const emptyNodes = { nodes: [], diagnostics: [], selectedNodeId: "" };
const emptyProbes = {
  revision: "empty",
  available: false,
  unavailableCode: "empty_subscription",
  target: "https://www.gstatic.com/generate_204",
  running: false,
  results: [],
  limits: { maxNodes: 256, concurrency: 1, timeoutMs: 3000 },
};
const off = { active: false, desired: false, commands: 0 };
function readResponse(url: string) {
  return jsonResponse(
    url === "/api/proxy/node-probes"
      ? emptyProbes
      : url === "/api/proxy/capture"
        ? off
        : emptyNodes,
  );
}
describe("proxy setup and recovery", () => {
  it("offers the acquisition prerequisite and links to advanced runtime without issuing writes", async () => {
    const fetchMock = vi.fn((url: string) =>
      Promise.resolve(
        url === "/api/runtime"
          ? jsonResponse({
              enabled: true,
              services: [
                {
                  ...runtimeStatus,
                  configured: false,
                  artifactAvailable: false,
                },
              ],
            })
          : readResponse(url),
      ),
    );
    vi.stubGlobal("fetch", fetchMock);
    render(<ProxyPage />);
    await screen.findByText("先获取校验过的 sing-box 运行文件");
    fireEvent.click(screen.getByRole("button", { name: "获取运行文件" }));
    expect(
      screen.getByText("高级设置与诊断").closest("details"),
    ).toHaveAttribute("open");
    expect(screen.getByRole("tab", { name: "运行管理" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    expect(fetchMock.mock.calls.every(([url]) => url.startsWith("/api/"))).toBe(
      true,
    );
    expect(
      fetchMock.mock.calls.every(
        (call) =>
          (call as unknown as [string, RequestInit])[1]?.method === "GET",
      ),
    ).toBe(true);
  });

  it("provides a read-only retry after runtime read failure", async () => {
    let failed = true;
    const fetchMock = vi.fn((url: string) =>
      Promise.resolve(
        url === "/api/runtime"
          ? jsonResponse(
              failed
                ? { enabled: true, services: [{}] }
                : { enabled: true, services: [runtimeStatus] },
            )
          : readResponse(url),
      ),
    );
    vi.stubGlobal("fetch", fetchMock);
    render(<ProxyPage />);
    await screen.findByRole("alert");
    failed = false;
    fireEvent.click(screen.getByRole("button", { name: "重试" }));
    await waitFor(() =>
      expect(screen.queryByText(/invalid_response/)).not.toBeInTheDocument(),
    );
    expect(
      fetchMock.mock.calls.filter(([url]) => url === "/api/runtime").length,
    ).toBeGreaterThan(1);
    expect(
      fetchMock.mock.calls.every(
        (call) =>
          (call as unknown as [string, RequestInit])[1]?.method === "GET",
      ),
    ).toBe(true);
  });

  it("keeps gateway and setup controls disabled while a subscription import is pending", async () => {
    let completeImport: (response: Response) => void = () => {};
    vi.stubGlobal(
      "fetch",
      vi.fn((url: string) => {
        if (url === "/api/proxy/import")
          return new Promise<Response>((resolve) => {
            completeImport = resolve;
          });
        return Promise.resolve(
          url === "/api/runtime"
            ? jsonResponse({ enabled: true, services: [runtimeStatus] })
            : readResponse(url),
        );
      }),
    );
    render(<ProxyPage />);
    fireEvent.click(screen.getByText("高级设置与诊断"));
    await waitFor(() =>
      expect(
        screen.getByRole("button", { name: "解析并导入节点" }),
      ).toBeEnabled(),
    );
    fireEvent.change(screen.getByLabelText("私密订阅 URL"), {
      target: { value: "https://subscription.example.test/private" },
    });
    fireEvent.click(screen.getByRole("button", { name: "解析并导入节点" }));
    expect(screen.getByRole("button", { name: "更换节点" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "导入订阅" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "开启网关代理" })).toBeDisabled();
    completeImport(jsonResponse(emptyNodes));
    await screen.findByText("已导入 0 个节点");
    expect(screen.getByRole("button", { name: "导入订阅" })).toBeEnabled();
  });

  it("explains server-disabled control rather than leaving a silent disabled form", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn((url: string) =>
        Promise.resolve(
          url === "/api/runtime"
            ? jsonResponse({ enabled: false, services: [] })
            : readResponse(url),
        ),
      ),
    );
    render(<ProxyPage />);
    await screen.findByText(/服务端未启用运行管理/);
    fireEvent.click(screen.getByText("高级设置与诊断"));
    expect(
      screen.getByRole("button", { name: "解析并导入节点" }),
    ).toBeDisabled();
    expect(screen.getByRole("button", { name: "开启网关代理" })).toBeDisabled();
  });
});
