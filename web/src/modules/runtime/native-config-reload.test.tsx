import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi, afterEach } from "vitest";
import { NativeConfigEditor } from "./native-config-editor";
import { clearRuntimeEditorSession } from "./editor-session";
import { jsonResponse, runtimeStatus } from "../production-fixtures.test-data";
import type { RuntimeController } from "./use-runtime";
const runtime = {
  service: "sing-box",
  status: runtimeStatus,
  enabled: true,
  loading: false,
  pending: false,
  error: undefined,
  result: undefined,
  refresh: vi.fn(),
  run: vi.fn(),
} satisfies RuntimeController;
afterEach(() => {
  clearRuntimeEditorSession();
  vi.unstubAllGlobals();
});
describe("native runtime config replacement", () => {
  it("keeps dirty text and makes no read until reload is explicitly confirmed", async () => {
    const fetchMock = vi.fn().mockImplementation(() =>
      Promise.resolve(
        jsonResponse({
          service: "sing-box",
          config: "{}",
          generation: runtimeStatus.generation,
        }),
      ),
    );
    vi.stubGlobal("fetch", fetchMock);
    const { container } = render(<NativeConfigEditor runtime={runtime} />);
    expect(
      screen.getByRole("button", { name: "校验并应用更改" }),
    ).toBeDisabled();
    expect(screen.getByText("高级诊断").closest("details")).not.toHaveAttribute(
      "open",
    );
    expect(container.querySelector(".panel-header")).not.toHaveTextContent(
      "generation",
    );
    fireEvent.click(screen.getByRole("button", { name: "载入配置" }));
    await waitFor(() =>
      expect(screen.getByLabelText("原生配置内容")).toHaveValue("{}"),
    );
    fireEvent.change(screen.getByLabelText("原生配置内容"), {
      target: { value: '{"log":{}}' },
    });
    fireEvent.click(screen.getByRole("button", { name: "重新载入配置" }));
    expect(
      screen.getByRole("dialog", { name: "丢弃未保存的原生配置？" }),
    ).toBeInTheDocument();
    expect(fetchMock).toHaveBeenCalledTimes(1);
    fireEvent.click(screen.getByRole("button", { name: "保留编辑" }));
    expect(screen.getByLabelText("原生配置内容")).toHaveValue('{"log":{}}');
    fireEvent.click(screen.getByRole("button", { name: "重新载入配置" }));
    fireEvent.click(screen.getByRole("button", { name: "丢弃并载入" }));
    await waitFor(() =>
      expect(screen.getByLabelText("原生配置内容")).toHaveValue("{}"),
    );
    expect(fetchMock).toHaveBeenCalledTimes(2);
  });
});
