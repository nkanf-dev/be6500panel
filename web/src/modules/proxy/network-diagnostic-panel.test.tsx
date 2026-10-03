import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { Effect } from "effect";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ApiError } from "../../lib/api";
import { NetworkDiagnosticPanel } from "./network-diagnostic-panel";
import {
  requestTraceFixture,
  requestTraceHistoryFixture,
} from "./request-trace-fixture.test-data";

const loaders = vi.hoisted(() => ({ history: vi.fn(), run: vi.fn() }));
vi.mock("./request-trace-api", () => ({ requestTraceApi: loaders }));
vi.mock("../../components/visualizations/EChart", () => ({
  EChart: ({ label }: { label: string }) => (
    <div role="img" aria-label={label} />
  ),
}));
beforeEach(() => {
  loaders.history.mockReturnValue(
    Effect.succeed({ ...requestTraceHistoryFixture, traces: [] }),
  );
});
afterEach(() => vi.clearAllMocks());
describe("network diagnostic panel", () => {
  it("shows honest empty state and fixed controls with GET-only loading", async () => {
    render(<NetworkDiagnosticPanel />);
    await waitFor(() =>
      expect(
        screen.getByRole("button", { name: "发起诊断测试" }),
      ).toBeEnabled(),
    );
    expect(loaders.history).toHaveBeenCalledTimes(1);
    expect(loaders.run).not.toHaveBeenCalled();
    expect(screen.getByText("暂未执行网络诊断")).toBeVisible();
    expect(screen.getByText(/不保证经过远端节点/)).toBeVisible();
    expect(
      screen.getByLabelText("诊断目标").querySelectorAll("option"),
    ).toHaveLength(2);
    expect(screen.queryByRole("textbox")).toBeNull();
    fireEvent.change(screen.getByLabelText("诊断链路"), {
      target: { value: "proxy" },
    });
    fireEvent.change(screen.getByLabelText("诊断目标"), {
      target: { value: "cloudflare" },
    });
    expect(loaders.run).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "刷新诊断记录" }));
    await waitFor(() => expect(loaders.history).toHaveBeenCalledTimes(2));
    expect(loaders.run).not.toHaveBeenCalled();
  });
  it("sends one selected fixed target/route only after click and disables concurrent runs", async () => {
    loaders.run.mockReturnValue(Effect.async(() => {}));
    render(<NetworkDiagnosticPanel />);
    await waitFor(() =>
      expect(
        screen.getByRole("button", { name: "发起诊断测试" }),
      ).toBeEnabled(),
    );
    fireEvent.change(screen.getByLabelText("诊断目标"), {
      target: { value: "cloudflare" },
    });
    fireEvent.change(screen.getByLabelText("诊断链路"), {
      target: { value: "proxy" },
    });
    fireEvent.click(screen.getByRole("button", { name: "发起诊断测试" }));
    await waitFor(() =>
      expect(loaders.run).toHaveBeenCalledWith({
        targetId: "cloudflare",
        route: "proxy",
      }),
    );
    expect(loaders.run).toHaveBeenCalledTimes(1);
    const run = screen.getByRole("button", { name: "正在测试各阶段时延…" });
    expect(run).toBeDisabled();
    fireEvent.click(run);
    expect(loaders.run).toHaveBeenCalledTimes(1);
    expect(screen.getByLabelText("诊断链路")).toBeDisabled();
    expect(screen.getByLabelText("诊断目标")).toBeDisabled();
    expect(screen.queryByRole("button", { name: /取消|Cancel/ })).toBeNull();
  });
  it("shows an explicitly returned failed trace instead of treating failure as empty data", async () => {
    loaders.run.mockReturnValue(Effect.succeed(requestTraceFixture));
    loaders.history
      .mockReturnValueOnce(
        Effect.succeed({ ...requestTraceHistoryFixture, traces: [] }),
      )
      .mockReturnValue(Effect.succeed(requestTraceHistoryFixture));
    render(<NetworkDiagnosticPanel />);
    await waitFor(() =>
      expect(
        screen.getByRole("button", { name: "发起诊断测试" }),
      ).toBeEnabled(),
    );
    fireEvent.click(screen.getByRole("button", { name: "发起诊断测试" }));
    await waitFor(() => expect(screen.getByText("1 条诊断记录")).toBeVisible());
    expect(screen.queryByText("暂未执行网络诊断")).toBeNull();
    fireEvent.click(screen.getByText("查看详细阶段耗时"));
    expect(screen.getAllByText("失败").length).toBeGreaterThan(1);
  });
  it("keeps history readable when an explicit proxy run is rejected", async () => {
    loaders.history.mockReturnValue(Effect.succeed(requestTraceHistoryFixture));
    loaders.run.mockReturnValue(
      Effect.fail(
        new ApiError({
          code: "proxy_unavailable",
          message: "current proxy unavailable",
        }),
      ),
    );
    render(<NetworkDiagnosticPanel />);
    await waitFor(() =>
      expect(
        screen.getByRole("button", { name: "发起诊断测试" }),
      ).toBeEnabled(),
    );
    fireEvent.click(screen.getByRole("button", { name: "发起诊断测试" }));
    await waitFor(() =>
      expect(screen.getByRole("alert")).toHaveTextContent("proxy_unavailable"),
    );
    expect(screen.getByText("1 条诊断记录")).toBeVisible();
  });
});
