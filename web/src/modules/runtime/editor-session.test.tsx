import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { StrictMode } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { NativeConfigEditor } from "./native-config-editor";
import { clearRuntimeEditorSession } from "./editor-session";
import { jsonResponse, runtimeStatus } from "../production-fixtures.test-data";
import type { RuntimeController } from "./use-runtime";
const controller = (
  generation = 4,
  service: "sing-box" | "frpc" = "sing-box",
): RuntimeController => ({
  service,
  status: { ...runtimeStatus, service, generation },
  enabled: true,
  loading: false,
  pending: false,
  error: undefined,
  result: undefined,
  refresh: vi.fn(),
  run: vi.fn().mockResolvedValue(true),
});
const source = "{}";
const edited = '{"log":{"level":"debug"}}';
afterEach(() => {
  clearRuntimeEditorSession();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});
async function editLoaded(runtime = controller()) {
  const view = render(<NativeConfigEditor runtime={runtime} />);
  fireEvent.click(screen.getByRole("button", { name: "载入配置" }));
  await waitFor(() =>
    expect(screen.getByLabelText("原生配置内容")).toHaveValue(source),
  );
  fireEvent.change(screen.getByLabelText("原生配置内容"), {
    target: { value: edited },
  });
  return view;
}
function mockReads(generation = 4) {
  const fetchMock = vi
    .fn()
    .mockImplementation(() =>
      Promise.resolve(
        jsonResponse({ service: "sing-box", config: source, generation }),
      ),
    );
  vi.stubGlobal("fetch", fetchMock);
  return fetchMock;
}
describe("runtime raw editor session memory", () => {
  it("restores dirty config and baseline after tab unmount without reading or writing", async () => {
    const fetchMock = mockReads();
    const first = await editLoaded();
    first.unmount();
    render(<NativeConfigEditor runtime={controller()} />);
    expect(screen.getByLabelText("原生配置内容")).toHaveValue(edited);
    expect(screen.getByRole("button", { name: "审阅配置差异" })).toBeEnabled();
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });
  it("preserves old edit generation on remount and requires explicit discard for newer readback", async () => {
    const fetchMock = mockReads();
    const first = await editLoaded();
    first.unmount();
    render(<NativeConfigEditor runtime={controller(5)} />);
    expect(screen.getByLabelText("原生配置内容")).toHaveValue(edited);
    expect(
      screen.getByText(/配置已被更新。当前编辑内容已保留/),
    ).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "审阅配置差异" }));
    expect(
      screen.getByRole("button", { name: "校验并应用更改" }),
    ).toBeDisabled();
    fireEvent.click(screen.getByRole("button", { name: "重新载入配置" }));
    expect(fetchMock).toHaveBeenCalledTimes(1);
    fireEvent.click(screen.getByRole("button", { name: "保留编辑" }));
    expect(screen.getByLabelText("原生配置内容")).toHaveValue(edited);
    fetchMock.mockImplementation(() =>
      Promise.resolve(
        jsonResponse({
          service: "sing-box",
          config: '{"latest":true}',
          generation: 5,
        }),
      ),
    );
    fireEvent.click(screen.getByRole("button", { name: "重新载入配置" }));
    fireEvent.click(screen.getByRole("button", { name: "丢弃并载入" }));
    await waitFor(() =>
      expect(screen.getByLabelText("原生配置内容")).toHaveValue(
        '{"latest":true}',
      ),
    );
    expect(
      screen.queryByText(/配置已被更新。当前编辑内容已保留/),
    ).not.toBeInTheDocument();
  });
  it("keeps service buffers separate and clears all private edits through logout hook", async () => {
    mockReads();
    const first = await editLoaded();
    first.unmount();
    const frpc = render(<NativeConfigEditor runtime={controller(4, "frpc")} />);
    expect(screen.getByLabelText("原生配置内容")).toHaveValue("");
    frpc.unmount();
    clearRuntimeEditorSession();
    render(<NativeConfigEditor runtime={controller()} />);
    expect(screen.getByLabelText("原生配置内容")).toHaveValue("");
    expect(screen.getByLabelText("原生配置内容")).toBeDisabled();
  });
  it("clears private text on unauthorized while mounted and cannot repopulate from stale component", async () => {
    mockReads();
    const first = await editLoaded();
    window.dispatchEvent(new Event("be6500panel:unauthorized"));
    await waitFor(() =>
      expect(screen.getByLabelText("原生配置内容")).toHaveValue(""),
    );
    first.unmount();
    render(<NativeConfigEditor runtime={controller()} />);
    expect(screen.getByLabelText("原生配置内容")).toHaveValue("");
  });
  it("warns before browser unload for hidden dirty buffers, and stops after clear", async () => {
    mockReads();
    const first = await editLoaded();
    first.unmount();
    const before = new Event("beforeunload", { cancelable: true });
    window.dispatchEvent(before);
    expect(before.defaultPrevented).toBe(true);
    clearRuntimeEditorSession();
    const after = new Event("beforeunload", { cancelable: true });
    window.dispatchEvent(after);
    expect(after.defaultPrevented).toBe(false);
  });
  it("does not retain a saved edit after a successful apply", async () => {
    mockReads();
    const first = await editLoaded();
    fireEvent.click(screen.getByRole("button", { name: "审阅配置差异" }));
    fireEvent.click(screen.getByRole("button", { name: "校验并应用更改" }));
    await screen.findByText(/配置已保存。再次编辑前/);
    first.unmount();
    render(<NativeConfigEditor runtime={controller(5)} />);
    expect(screen.getByLabelText("原生配置内容")).toHaveValue("");
  });
  it("does not erase newer remounted edits when an older apply finishes", async () => {
    mockReads();
    let finishApply: (accepted: boolean) => void = () => {};
    const applying = controller();
    applying.run = vi.fn(
      () =>
        new Promise<boolean>((resolve) => {
          finishApply = resolve;
        }),
    );
    const first = await editLoaded(applying);
    fireEvent.click(screen.getByRole("button", { name: "审阅配置差异" }));
    fireEvent.click(screen.getByRole("button", { name: "校验并应用更改" }));
    first.unmount();
    const remounted = render(<NativeConfigEditor runtime={controller()} />);
    const newer = '{"log":{"level":"warn"}}';
    fireEvent.change(screen.getByLabelText("原生配置内容"), {
      target: { value: newer },
    });
    await act(async () => {
      finishApply(true);
    });
    remounted.unmount();
    render(<NativeConfigEditor runtime={controller(5)} />);
    expect(screen.getByLabelText("原生配置内容")).toHaveValue(newer);
    expect(
      screen.getByText(/配置已被更新。当前编辑内容已保留/),
    ).toBeInTheDocument();
  });
  it("works in StrictMode without storing private config in browser storage", async () => {
    const store = vi.spyOn(Storage.prototype, "setItem");
    mockReads();
    const first = render(
      <StrictMode>
        <NativeConfigEditor runtime={controller()} />
      </StrictMode>,
    );
    fireEvent.click(screen.getByRole("button", { name: "载入配置" }));
    await waitFor(() =>
      expect(screen.getByLabelText("原生配置内容")).toHaveValue(source),
    );
    fireEvent.change(screen.getByLabelText("原生配置内容"), {
      target: { value: edited },
    });
    first.unmount();
    render(
      <StrictMode>
        <NativeConfigEditor runtime={controller()} />
      </StrictMode>,
    );
    expect(screen.getByLabelText("原生配置内容")).toHaveValue(edited);
    expect(store).not.toHaveBeenCalled();
  });
  it("ignores a late private read after clear and remount", async () => {
    let complete: (response: Response) => void = () => {};
    vi.stubGlobal(
      "fetch",
      vi.fn(
        () =>
          new Promise<Response>((resolve) => {
            complete = resolve;
          }),
      ),
    );
    const first = render(<NativeConfigEditor runtime={controller()} />);
    fireEvent.click(screen.getByRole("button", { name: "载入配置" }));
    clearRuntimeEditorSession();
    first.unmount();
    render(<NativeConfigEditor runtime={controller()} />);
    complete(
      jsonResponse({ service: "sing-box", config: source, generation: 4 }),
    );
    await waitFor(() =>
      expect(screen.getByLabelText("原生配置内容")).toHaveValue(""),
    );
  });
});
