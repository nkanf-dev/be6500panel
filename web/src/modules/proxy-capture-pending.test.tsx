import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ProxyPage } from "./proxy";
vi.mock("./runtime/use-runtime", () => ({
  useRuntime: () => ({
    pending: false,
    enabled: true,
    loading: false,
    status: { state: "running" },
    refresh: vi.fn(),
  }),
}));
vi.mock("../lib/use-resource", () => ({
  useResource: () => ({ loading: false, data: { nodes: [] }, reload: vi.fn() }),
}));
vi.mock("./proxy/capture-panel", () => ({
  CapturePanel: ({ onPending }: { onPending?: (value: boolean) => void }) => (
    <>
      <button onClick={() => onPending?.(true)}>begin-capture-mock</button>
      <button onClick={() => onPending?.(false)}>end-capture-mock</button>
    </>
  ),
}));
vi.mock("./proxy/gateway-panel", () => ({ GatewayPanel: () => null }));
vi.mock("./proxy/node-selector", () => ({ NodeSelector: () => null }));
vi.mock("./proxy/import-form", () => ({ ProxyImportForm: () => null }));
describe("proxy page capture mutation ownership", () => {
  it("holds tabs and refresh until the capture child reports completion", () => {
    render(<ProxyPage />);
    fireEvent.click(screen.getByText("高级设置与诊断"));
    fireEvent.click(screen.getByRole("tab", { name: "设备诊断" }));
    fireEvent.click(screen.getByRole("button", { name: "begin-capture-mock" }));
    expect(screen.getByRole("tab", { name: "节点" })).toBeDisabled();
    expect(screen.getByRole("tab", { name: "运行管理" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "刷新代理状态" })).toBeDisabled();
    fireEvent.click(screen.getByRole("button", { name: "end-capture-mock" }));
    expect(screen.getByRole("tab", { name: "节点" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "刷新代理状态" })).toBeEnabled();
  });
});
