import { afterEach, describe, expect, it, vi } from "vitest";
import { StrictMode } from "react";
import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { ConfigurationEditor, ConfigurationWorkspace } from "./index";
import type { ConfigurationDraft } from "./contracts";

const source = "config interface 'lan'\n\toption ipaddr '192.0.2.1'\n";
const edited = "config interface 'lan'\n\toption ipaddr '192.0.2.2'\n";
const draft: ConfigurationDraft = {
  id: "draft-synthetic",
  module: "network",
  generation: 7,
  diff: "--- network\n+++ network\n@@ -1,2 +1,2 @@\n config interface 'lan'\n- option ipaddr '192.0.2.1'\n+ option ipaddr '192.0.2.2'",
  risks: [],
  valid: true,
  errors: [],
  createdAt: "2026-01-01T00:00:00Z",
};
const respond = (body: unknown, status = 200) =>
  new Response(JSON.stringify(body), {
    status,
    headers: { "Content-Type": "application/json" },
  });
function mockApi(
  options: {
    drafts?: readonly ConfigurationDraft[];
    risky?: boolean;
    enabled?: boolean;
    stageConflict?: boolean;
    pending?: { id: string; deadline: string };
  } = {},
) {
  let generation = 7;
  let drafts = [...(options.drafts ?? [])];
  let pending = options.pending;
  const fetchMock = vi.fn(async (url: string, _init?: RequestInit) => {
    if (url === "/api/configuration/status")
      return respond({
        enabled: options.enabled ?? true,
        generation,
        ...(pending ? { pendingCommit: pending } : {}),
      });
    if (url === "/api/configuration")
      return respond({
        generation,
        documents: [
          {
            module: "network",
            content:
              options.stageConflict && generation === 8
                ? source.replace("192.0.2.1", "192.0.2.9")
                : source,
          },
          {
            module: "wireless",
            content:
              "config wifi-iface 'synthetic'\n option key 'synthetic-test-key'\n",
          },
        ],
        ...(pending ? { pendingCommit: pending } : {}),
      });
    if (url === "/api/configuration/drafts") return respond({ drafts });
    if (url === "/api/configuration/stage") {
      if (options.stageConflict) {
        generation = 8;
        return respond(
          { error: { code: "generation_conflict", message: "配置版本已改变" } },
          409,
        );
      }
      const staged = {
        ...draft,
        risks: options.risky
          ? [{ code: "management_address", message: "管理地址发生变化" }]
          : [],
      };
      drafts.push(staged);
      return respond(staged);
    }
    if (url.startsWith("/api/configuration/drafts?id=")) {
      drafts = [];
      return respond({ deleted: true });
    }
    if (url === "/api/configuration/commit") {
      generation = 8;
      drafts = [];
      if (options.risky)
        pending = {
          id: "commit-synthetic",
          deadline: new Date(Date.now() + 120_000).toISOString(),
        };
      return respond({
        id: "commit-synthetic",
        generation,
        state: pending ? "pending_confirmation" : "committed",
        ...(pending ? { deadline: pending.deadline } : {}),
        changedModules: ["network"],
      });
    }
    if (
      url === "/api/configuration/confirm" ||
      url === "/api/configuration/rollback"
    ) {
      pending = undefined;
      return respond({
        id: "commit-synthetic",
        generation,
        state: url.endsWith("rollback") ? "rolled_back" : "committed",
        changedModules: ["network"],
      });
    }
    throw new Error(`Unexpected request: ${url}`);
  });
  vi.stubGlobal("fetch", fetchMock);
  return fetchMock;
}
function posts(fetchMock: ReturnType<typeof mockApi>, endpoint: string) {
  return fetchMock.mock.calls.filter(
    ([url, init]) =>
      url === `/api/configuration/${endpoint}` && init?.method === "POST",
  );
}
afterEach(() => {
  vi.unstubAllGlobals();
  vi.useRealTimers();
});

describe("FullControl configuration", () => {
  it("edits native fields and Ctrl+Enter stages a diff without committing or changing saved generation", async () => {
    const fetchMock = mockApi();
    render(<ConfigurationEditor module="network" />);
    const input = await screen.findByRole("textbox", {
      name: "network 原生配置",
    });
    fireEvent.change(input, { target: { value: edited } });
    expect(screen.getByText("未暂存")).toBeInTheDocument();
    fireEvent.keyDown(input, { key: "Enter", ctrlKey: true });
    await screen.findByText("已暂存");
    expect(posts(fetchMock, "stage")).toHaveLength(1);
    expect(JSON.parse(posts(fetchMock, "stage")[0][1]!.body as string)).toEqual(
      { module: "network", content: edited, generation: 7 },
    );
    expect(posts(fetchMock, "commit")).toHaveLength(0);
    expect(screen.getByText("已保存版本 g7")).toBeInTheDocument();
    expect(screen.getByTestId("configuration-diff")).toHaveTextContent(
      "192.0.2.2",
    );
  });
  it("shows generation conflicts without losing unsaved text or silently retrying Stage", async () => {
    const fetchMock = mockApi({ stageConflict: true });
    render(<ConfigurationEditor module="network" />);
    const input = await screen.findByRole("textbox", {
      name: "network 原生配置",
    });
    fireEvent.change(input, { target: { value: edited } });
    fireEvent.click(screen.getByRole("button", { name: "暂存并校验" }));
    await screen.findByText(/generation_conflict/);
    expect(input).toHaveValue(edited);
    expect(posts(fetchMock, "stage")).toHaveLength(1);
    await screen.findByText("编辑基线 g7 · 当前 g8");
    expect(screen.getByRole("button", { name: "暂存并校验" })).toBeDisabled();
  });
  it("commits a low-risk selected draft directly with no confirmation dialog", async () => {
    const fetchMock = mockApi({ drafts: [draft] });
    const user = userEvent.setup();
    render(<ConfigurationWorkspace />);
    await screen.findByRole("checkbox", { name: "选择草稿 draft-synthetic" });
    await user.click(
      screen.getByRole("button", { name: "Commit 已选草稿 (1)" }),
    );
    await screen.findByText(/已提交.*network/);
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(
      JSON.parse(posts(fetchMock, "commit")[0][1]!.body as string),
    ).toEqual({ draftIds: [draft.id], generation: 7, acknowledgeRisks: false });
  });
  it("acknowledges risks once and shows pending confirmation with explicit Confirm and Rollback", async () => {
    const risky = {
      ...draft,
      risks: [{ code: "management_address", message: "管理地址发生变化" }],
    };
    const fetchMock = mockApi({ drafts: [risky], risky: true });
    const user = userEvent.setup();
    render(<ConfigurationWorkspace />);
    await screen.findByRole("checkbox", { name: "选择草稿 draft-synthetic" });
    await user.click(
      screen.getByRole("button", { name: "Commit 已选草稿 (1)" }),
    );
    const dialog = screen.getByRole("dialog", { name: "确认高风险 Commit" });
    expect(
      within(dialog).getByText("network / option ipaddr"),
    ).toBeInTheDocument();
    expect(posts(fetchMock, "commit")).toHaveLength(0);
    await user.click(
      within(dialog).getByRole("button", { name: "确认风险并 Commit" }),
    );
    await screen.findByText("等待连接确认");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(
      JSON.parse(posts(fetchMock, "commit")[0][1]!.body as string)
        .acknowledgeRisks,
    ).toBe(true);
    expect(screen.getByRole("button", { name: "确认当前连接" })).toBeEnabled();
    await user.click(screen.getByRole("button", { name: "恢复先前配置" }));
    await screen.findByText(/已回滚/);
    expect(posts(fetchMock, "rollback")).toHaveLength(1);
  });
  it("blocks confirmation after deadline and never assumes the rollback is already complete", async () => {
    mockApi({
      pending: {
        id: "commit-synthetic",
        deadline: new Date(Date.now() - 1000).toISOString(),
      },
    });
    render(<ConfigurationWorkspace />);
    await screen.findByText("确认期限已到，正在核对回滚状态");
    expect(screen.getByRole("button", { name: "确认当前连接" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "刷新提交状态" })).toBeEnabled();
    expect(screen.queryByText(/已回滚/)).not.toBeInTheDocument();
  });
  it("does not request private documents when write support is disabled", async () => {
    const fetchMock = mockApi({ enabled: false });
    render(<ConfigurationEditor module="wireless" />);
    await screen.findByText("配置服务未启用");
    expect(
      fetchMock.mock.calls.some(([url]) => url === "/api/configuration"),
    ).toBe(false);
  });
  it("expert editing exposes native fields without masking admin-visible values", async () => {
    mockApi();
    render(<ConfigurationEditor module="wireless" />);
    await screen.findByRole("textbox", { name: "wireless 原生配置" });
    expect(
      (
        screen.getByRole("textbox", {
          name: "wireless 原生配置",
        }) as HTMLTextAreaElement
      ).value,
    ).toContain("synthetic-test-key");
    fireEvent.click(screen.getByRole("tab", { name: "字段视图" }));
    expect(screen.getByText("synthetic-test-key")).toBeInTheDocument();
  });
  it("retains local edits when switching documents and refreshing", async () => {
    mockApi();
    render(<ConfigurationWorkspace />);
    const input = await screen.findByRole("textbox", {
      name: "network 原生配置",
    });
    fireEvent.change(input, { target: { value: edited } });
    fireEvent.click(screen.getByRole("button", { name: "编辑 wireless" }));
    await screen.findByRole("textbox", { name: "wireless 原生配置" });
    fireEvent.click(screen.getByRole("button", { name: "刷新配置" }));
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "刷新配置" })).toBeEnabled(),
    );
    fireEvent.click(screen.getByRole("button", { name: "编辑 network" }));
    expect(
      screen.getByRole("textbox", { name: "network 原生配置" }),
    ).toHaveValue(edited);
  });
  it("does not select invalid or stale drafts and shows their diagnostics and diff", async () => {
    const invalid = {
      ...draft,
      id: "invalid-synthetic",
      valid: false,
      errors: [{ code: "invalid_port", message: "端口超出范围" }],
    };
    const stale = { ...draft, id: "stale-synthetic", generation: 6 };
    mockApi({ drafts: [invalid, stale] });
    render(<ConfigurationWorkspace />);
    await screen.findByRole("checkbox", { name: "选择草稿 invalid-synthetic" });
    expect(
      screen.getByRole("checkbox", { name: "选择草稿 invalid-synthetic" }),
    ).toBeDisabled();
    expect(
      screen.getByRole("checkbox", { name: "选择草稿 stale-synthetic" }),
    ).toBeDisabled();
    expect(
      screen.getByRole("button", { name: "Commit 已选草稿 (0)" }),
    ).toBeDisabled();
    fireEvent.click(
      screen.getByRole("button", { name: "查看差异 invalid-synthetic" }),
    );
    expect(screen.getByText("invalid_port")).toBeInTheDocument();
    expect(screen.getByText("端口超出范围")).toBeInTheDocument();
  });
  it("selects only one draft per native document and deletes through the typed DELETE boundary", async () => {
    const older = { ...draft, id: "older-synthetic" };
    const fetchMock = mockApi({ drafts: [older, draft] });
    render(<ConfigurationWorkspace />);
    const checkbox = await screen.findByRole("checkbox", {
      name: "选择草稿 draft-synthetic",
    });
    expect(checkbox).toBeChecked();
    expect(
      screen.getByRole("checkbox", { name: "选择草稿 older-synthetic" }),
    ).not.toBeChecked();
    fireEvent.click(
      screen.getByRole("checkbox", { name: "选择草稿 older-synthetic" }),
    );
    expect(checkbox).not.toBeChecked();
    fireEvent.click(
      screen.getByRole("button", { name: "删除草稿 older-synthetic" }),
    );
    await waitFor(() =>
      expect(
        screen.queryByRole("checkbox", { name: "选择草稿 older-synthetic" }),
      ).not.toBeInTheDocument(),
    );
    expect(fetchMock).toHaveBeenCalledWith(
      "/api/configuration/drafts?id=older-synthetic",
      expect.objectContaining({ method: "DELETE", credentials: "same-origin" }),
    );
    expect(posts(fetchMock, "commit")).toHaveLength(0);
  });
  it("confirms a pending operation explicitly", async () => {
    const fetchMock = mockApi({
      pending: {
        id: "commit-synthetic",
        deadline: new Date(Date.now() + 120_000).toISOString(),
      },
    });
    render(<ConfigurationWorkspace />);
    const confirm = await screen.findByRole("button", { name: "确认当前连接" });
    fireEvent.click(confirm);
    await screen.findByText(/已提交.*network/);
    expect(posts(fetchMock, "confirm")).toHaveLength(1);
    expect(posts(fetchMock, "commit")).toHaveLength(0);
  });
  it("reconciles only pending status every 10s and cancels timers on unmount", async () => {
    vi.useFakeTimers({ toFake: ["setInterval", "clearInterval", "Date"] });
    const fetchMock = mockApi({
      pending: {
        id: "commit-synthetic",
        deadline: new Date(Date.now() + 120_000).toISOString(),
      },
    });
    const view = render(<ConfigurationWorkspace />);
    await screen.findByRole("button", { name: "确认当前连接" });
    await waitFor(() =>
      expect(
        screen.getByRole("button", { name: "刷新提交状态" }),
      ).toBeEnabled(),
    );
    const documentReads = fetchMock.mock.calls.filter(
      ([url]) => url === "/api/configuration",
    ).length;
    const draftReads = fetchMock.mock.calls.filter(
      ([url]) => url === "/api/configuration/drafts",
    ).length;
    const statusReads = fetchMock.mock.calls.filter(
      ([url]) => url === "/api/configuration/status",
    ).length;
    await act(async () => {
      await vi.advanceTimersByTimeAsync(10_001);
    });
    expect(
      fetchMock.mock.calls.filter(
        ([url]) => url === "/api/configuration/status",
      ),
    ).toHaveLength(statusReads + 1);
    expect(
      fetchMock.mock.calls.filter(([url]) => url === "/api/configuration"),
    ).toHaveLength(documentReads);
    expect(
      fetchMock.mock.calls.filter(
        ([url]) => url === "/api/configuration/drafts",
      ),
    ).toHaveLength(draftReads);
    const total = fetchMock.mock.calls.length;
    view.unmount();
    await act(async () => {
      await vi.advanceTimersByTimeAsync(20_000);
    });
    expect(fetchMock.mock.calls).toHaveLength(total);
  });
  it("initializes safely in React StrictMode after its first lifetime is aborted", async () => {
    mockApi();
    render(
      <StrictMode>
        <ConfigurationEditor module="network" />
      </StrictMode>,
    );
    await screen.findByRole("textbox", { name: "network 原生配置" });
    expect(screen.getByRole("button", { name: "刷新配置" })).toBeEnabled();
  });
  it("aborts in-flight authenticated reads on unmount", async () => {
    const signals: AbortSignal[] = [];
    vi.stubGlobal(
      "fetch",
      vi.fn((_url: string, init: RequestInit) => {
        signals.push(init.signal as AbortSignal);
        return new Promise<Response>((_resolve, reject) =>
          init.signal?.addEventListener("abort", () =>
            reject(new DOMException("Aborted", "AbortError")),
          ),
        );
      }),
    );
    const view = render(<ConfigurationWorkspace />);
    await waitFor(() => expect(signals.length).toBeGreaterThan(0));
    view.unmount();
    await waitFor(() =>
      expect(signals.every((signal) => signal.aborted)).toBe(true),
    );
  });
});
