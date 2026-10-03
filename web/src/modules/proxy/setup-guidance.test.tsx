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
const emptyProbes = {revision:"empty",available:false,unavailableCode:"empty_subscription",target:"https://www.gstatic.com/generate_204",running:false,results:[],limits:{maxNodes:256,concurrency:1,timeoutMs:3000}};
describe("proxy setup and recovery", () => {
  it("explains the acquisition prerequisite and links to runtime without issuing writes", async () => {
    const fetchMock = vi.fn((url: string) =>
      Promise.resolve(
        jsonResponse(
          url === "/api/runtime"
            ? {
                enabled: true,
                services: [
                  {
                    ...runtimeStatus,
                    configured: false,
                    artifactAvailable: false,
                  },
                ],
              }
            : url === "/api/proxy/node-probes" ? emptyProbes : emptyNodes,
        ),
      ),
    );
    vi.stubGlobal("fetch", fetchMock);
    render(<ProxyPage />);
    await screen.findByText("先获取校验过的 sing-box 运行文件");
    fireEvent.click(screen.getByRole("button", { name: "获取运行文件" }));
    expect(screen.getByRole("tab", { name: "运行管理" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    expect(
      fetchMock.mock.calls.every(
        ([url]) =>
          url === "/api/runtime" ||
          url === "/api/proxy/nodes" ||
          url === "/api/proxy/node-probes" ||
          url === "/api/runtime/config?service=sing-box",
      ),
    ).toBe(true);
  });
  it("provides a read-only retry after runtime read failure", async () => {
    let failed = true;
    const fetchMock = vi.fn((url: string) =>
      Promise.resolve(
        url === "/api/runtime"
          ? failed
            ? jsonResponse({ enabled: true, services: [{}] })
            : jsonResponse({ enabled: true, services: [runtimeStatus] })
          : jsonResponse(url === "/api/proxy/node-probes" ? emptyProbes : emptyNodes),
      ),
    );
    vi.stubGlobal("fetch", fetchMock);
    render(<ProxyPage />);
    await screen.findByRole("button", { name: "重新读取运行状态" });
    failed = false;
    fireEvent.click(screen.getByRole("button", { name: "重新读取运行状态" }));
    await waitFor(() =>
      expect(screen.queryByText(/invalid_response/)).not.toBeInTheDocument(),
    );
    expect(
      fetchMock.mock.calls.filter(([url]) => url === "/api/runtime").length,
    ).toBeGreaterThan(1);
    expect(
      fetchMock.mock.calls.every(
        ([url]) =>
          url === "/api/runtime" ||
          url === "/api/proxy/nodes" ||
          url === "/api/proxy/node-probes" ||
          url === "/api/runtime/config?service=sing-box",
      ),
    ).toBe(true);
  });
  it("keeps setup shortcuts disabled while a subscription import is pending", async () => {
    let completeImport: (response: Response) => void = () => {};
    vi.stubGlobal(
      "fetch",
      vi.fn((url: string) => {
        if (url === "/api/proxy/import")
          return new Promise<Response>((resolve) => {
            completeImport = resolve;
          });
        return Promise.resolve(
          jsonResponse(
            url === "/api/runtime"
              ? { enabled: true, services: [runtimeStatus] }
              : url === "/api/proxy/node-probes" ? emptyProbes : emptyNodes,
          ),
        );
      }),
    );
    render(<ProxyPage />);
    await waitFor(() =>
      expect(
        screen.getByRole("button", { name: "解析并导入节点" }),
      ).toBeEnabled(),
    );
    fireEvent.change(screen.getByLabelText("私密订阅 URL"), {
      target: { value: "https://subscription.example.test/private" },
    });
    fireEvent.click(screen.getByRole("button", { name: "解析并导入节点" }));
    expect(screen.getByRole("button", { name: "打开运行管理" })).toBeDisabled();
    completeImport(jsonResponse(emptyNodes));
    await screen.findByText("已导入 0 个节点");
    expect(screen.getByRole("button", { name: "打开运行管理" })).toBeEnabled();
  });
  it("explains server-disabled control rather than leaving a silent disabled form", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn((url: string) =>
        Promise.resolve(
          jsonResponse(
            url === "/api/runtime"
              ? { enabled: false, services: [] }
              : url === "/api/proxy/node-probes" ? emptyProbes : emptyNodes,
          ),
        ),
      ),
    );
    render(<ProxyPage />);
    await screen.findByText(/服务端未启用运行管理/);
    expect(
      screen.getByRole("button", { name: "解析并导入节点" }),
    ).toBeDisabled();
  });
});
