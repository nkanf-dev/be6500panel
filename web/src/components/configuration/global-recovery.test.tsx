import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { StrictMode, useState } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { Schema } from "effect";
import { GlobalRecoveryBanner } from "./GlobalRecoveryBanner";
import {
  GlobalRecoveryProvider,
  useGlobalRecovery,
} from "./GlobalRecoveryProvider";
import {
  ConfigurationStatusSchema,
  type ConfigurationOperationStatus,
  type ConfigurationStatus,
} from "./contracts";

const response = (body: unknown, status = 200) =>
  new Response(JSON.stringify(body), {
    status,
    headers: { "Content-Type": "application/json" },
  });
const operation = (
  overrides: Partial<ConfigurationOperationStatus> = {},
): ConfigurationOperationStatus => ({
  id: "internal-operation-id",
  state: "pending_confirmation",
  phase: "pending",
  generation: 8,
  deadline: new Date(Date.now() + 120_000).toISOString(),
  changedModules: ["network"],
  canConfirm: true,
  canRollback: true,
  ...overrides,
});
function statusWith(
  op = operation(),
  overrides: Partial<ConfigurationStatus> = {},
): ConfigurationStatus {
  return {
    enabled: true,
    generation: op.generation,
    operation: op,
    ...(op.phase === "pending" && op.deadline
      ? { pendingCommit: { id: op.id, deadline: op.deadline } }
      : {}),
    ...overrides,
  };
}
function ConsoleFixture() {
  const [page, setPage] = useState("概览");
  const [branch, setBranch] = useState("设备页");
  return (
    <GlobalRecoveryProvider>
      <GlobalRecoveryBanner />
      <nav>
        <button onClick={() => setPage("代理")}>代理页面</button>
        <button onClick={() => setBranch("核心未运行")}>核心未运行分支</button>
      </nav>
      <main key={`${page}-${branch}`}>
        <h1>{page}</h1>
        <p>{branch}</p>
      </main>
    </GlobalRecoveryProvider>
  );
}
function Controls() {
  const controller = useGlobalRecovery();
  return (
    <>
      <button
        onClick={() => {
          void controller.refresh();
        }}
      >
        手动读回
      </button>
      <button onClick={controller.clear}>退出清理</button>
      <span data-testid="busy">{controller.busy ?? "idle"}</span>
    </>
  );
}
function renderRecovery(extra?: React.ReactNode) {
  return render(
    <GlobalRecoveryProvider>
      <GlobalRecoveryBanner />
      {extra}
    </GlobalRecoveryProvider>,
  );
}
function mockStatus(initial: ConfigurationStatus) {
  let status = initial;
  const fetchMock = vi.fn(async (url: string, init?: RequestInit) => {
    if (url === "/api/configuration/status") return response(status);
    if (init?.method === "POST") return response(status.operation);
    throw new Error(`Unexpected request: ${url}`);
  });
  vi.stubGlobal("fetch", fetchMock);
  return {
    fetchMock,
    setStatus: (value: ConfigurationStatus) => {
      status = value;
    },
  };
}
const posts = (fetchMock: ReturnType<typeof vi.fn>) =>
  fetchMock.mock.calls.filter(
    ([, init]) => (init as RequestInit | undefined)?.method === "POST",
  );
async function settle() {
  await act(async () => {
    await Promise.resolve();
    await Promise.resolve();
  });
}

afterEach(() => {
  vi.unstubAllGlobals();
  vi.useRealTimers();
});

describe("global configuration recovery", () => {
  it("decodes optional journal metadata while preserving legacy disabled/read-only status", () => {
    expect(
      Schema.decodeUnknownSync(ConfigurationStatusSchema)({
        enabled: false,
        generation: 0,
      }),
    ).toEqual({ enabled: false, generation: 0 });
    const pendingStatus = statusWith();
    expect(
      Schema.decodeUnknownSync(ConfigurationStatusSchema)(pendingStatus),
    ).toEqual(pendingStatus);
    const failed = statusWith(
      operation({
        phase: "rolling_back",
        errorCode: "rollback_failed",
        canConfirm: false,
      }),
      {
        enabled: false,
        pendingCommit: undefined,
        errorCode: "rollback_failed",
      },
    );
    expect(
      Schema.decodeUnknownSync(ConfigurationStatusSchema)(failed).operation
        ?.canRollback,
    ).toBe(true);
  });
  it("keeps one banner and timer across pages and core-not-running branches without loading private documents or POST", async () => {
    const { fetchMock } = mockStatus(statusWith());
    render(<ConsoleFixture />);
    const banner = await screen.findByRole("region", { name: "全局配置恢复" });
    const timer = within(banner).getByRole("timer", {
      name: "自动恢复剩余时间",
    });
    expect(timer).toHaveTextContent(/1[12][0-9]s/);
    expect(within(banner).getByRole("progressbar")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "代理页面" }));
    fireEvent.click(screen.getByRole("button", { name: "核心未运行分支" }));
    expect(screen.getByRole("heading", { name: "代理" })).toBeInTheDocument();
    expect(screen.getByText("核心未运行")).toBeInTheDocument();
    expect(
      screen.getAllByRole("region", { name: "全局配置恢复" }),
    ).toHaveLength(1);
    expect(screen.getByRole("timer")).toBe(timer);
    expect(
      fetchMock.mock.calls.every(
        ([url]) => url === "/api/configuration/status",
      ),
    ).toBe(true);
    expect(posts(fetchMock)).toHaveLength(0);
    expect(
      screen.getByText("internal-operation-id").closest("details"),
    ).not.toHaveAttribute("open");
  });
  it("counts down, blocks late confirmation, and never sends automatic restoration", async () => {
    vi.useFakeTimers();
    mockStatus(
      statusWith(
        operation({ deadline: new Date(Date.now() + 2_000).toISOString() }),
      ),
    );
    renderRecovery();
    await settle();
    expect(screen.getByRole("timer")).toHaveTextContent("2s");
    await act(async () => {
      vi.advanceTimersByTime(3_000);
    });
    expect(screen.getByRole("timer")).toHaveTextContent("0s");
    expect(screen.getByRole("button", { name: "确认生效" })).toBeDisabled();
    expect(
      screen.getByText("确认期限已到，正在核对恢复状态"),
    ).toBeInTheDocument();
    expect(screen.queryByText("已恢复上一配置")).not.toBeInTheDocument();
    expect(posts(vi.mocked(fetch))).toHaveLength(0);
  });
  it("shows recovery failure with Restore available even when normal control is disabled and pending is absent", async () => {
    const op = operation({
      phase: "rolling_back",
      errorCode: "rollback_failed",
      canConfirm: false,
    });
    const { fetchMock } = mockStatus(
      statusWith(op, {
        enabled: false,
        pendingCommit: undefined,
        errorCode: "rollback_failed",
      }),
    );
    renderRecovery();
    await screen.findByText("上一配置恢复尚未完成，请检查后重试恢复");
    expect(screen.getByRole("button", { name: "恢复上一配置" })).toBeEnabled();
    expect(
      screen.queryByRole("button", { name: "确认生效" }),
    ).not.toBeInTheDocument();
    expect(screen.queryByText("已恢复上一配置")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "恢复上一配置" }));
    await waitFor(() => expect(posts(fetchMock)).toHaveLength(1));
    expect(JSON.parse(posts(fetchMock)[0][1].body as string)).toEqual({
      id: op.id,
    });
  });
  it.each(["确认生效", "恢复上一配置"])(
    "performs %s only once and reconciles a lost response by retained terminal ID",
    async (action) => {
      let status = statusWith();
      const fetchMock = vi.fn(async (url: string, init?: RequestInit) => {
        if (url === "/api/configuration/status") return response(status);
        if (init?.method === "POST") {
          const rollback = action === "恢复上一配置";
          status = statusWith(
            operation({
              state: rollback ? "rolled_back" : "committed",
              phase: rollback ? "rolled_back" : "committed",
              deadline: undefined,
              canConfirm: false,
              canRollback: !rollback,
              generation: rollback ? 9 : 8,
            }),
          );
          throw new Error("synthetic lost mutation response");
        }
        throw new Error("unexpected request");
      });
      vi.stubGlobal("fetch", fetchMock);
      renderRecovery();
      const button = await screen.findByRole("button", { name: action });
      fireEvent.click(button);
      fireEvent.click(button);
      await screen.findByText(
        action === "恢复上一配置" ? "已恢复上一配置" : "更改已生效",
      );
      expect(posts(fetchMock)).toHaveLength(1);
      expect(screen.getByRole("alert")).toHaveTextContent("未自动重试配置操作");
      expect(screen.queryByRole("timer")).not.toBeInTheDocument();
    },
  );
  it("does not infer restored when the server no longer returns a pending ID or operation", async () => {
    const api = mockStatus(statusWith());
    renderRecovery();
    await screen.findByRole("timer");
    api.setStatus({ enabled: true, generation: 9 });
    fireEvent.click(screen.getByRole("button", { name: "刷新全局配置状态" }));
    await screen.findByText("未能确认配置操作结果，请重新核对");
    expect(screen.queryByText("已恢复上一配置")).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "恢复上一配置" }),
    ).not.toBeInTheDocument();
    expect(screen.getByText("internal-operation-id")).toBeInTheDocument();
  });
  it("reconnects with GET only and leaves authoritative retry to an explicit click", async () => {
    const api = mockStatus(statusWith());
    renderRecovery();
    await screen.findByRole("timer");
    api.setStatus(
      statusWith(
        operation({
          phase: "rolling_back",
          canConfirm: false,
          errorCode: "rollback_failed",
        }),
        {
          enabled: false,
          pendingCommit: undefined,
          errorCode: "rollback_failed",
        },
      ),
    );
    fireEvent(window, new Event("online"));
    await screen.findByText("上一配置恢复尚未完成，请检查后重试恢复");
    fireEvent(window, new Event("focus"));
    await waitFor(() =>
      expect(api.fetchMock.mock.calls.length).toBeGreaterThanOrEqual(3),
    );
    expect(posts(api.fetchMock)).toHaveLength(0);
  });
  it("uses explicit action flags, not the normal Enabled guard or original operation state", async () => {
    const api = mockStatus(
      statusWith(
        operation({ phase: "pending", canConfirm: false, canRollback: false }),
      ),
    );
    renderRecovery();
    await screen.findByRole("timer");
    expect(screen.getByRole("button", { name: "确认生效" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "恢复上一配置" })).toBeDisabled();
    api.setStatus(
      statusWith(
        operation({
          phase: "rolling_back",
          state: "rolled_back",
          canConfirm: false,
          canRollback: true,
          errorCode: "rollback_failed",
        }),
        { enabled: false, pendingCommit: undefined },
      ),
    );
    fireEvent(window, new Event("online"));
    await screen.findByText("上一配置恢复尚未完成，请检查后重试恢复");
    expect(screen.queryByText("已恢复上一配置")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "恢复上一配置" })).toBeEnabled();
  });
  it("keeps status unknown after a read failure and blocks mutations until a fresh observation", async () => {
    let failed = false;
    const fetchMock = vi.fn(async () =>
      failed
        ? response(
            {
              error: { code: "unavailable", message: "synthetic disconnected" },
            },
            400,
          )
        : response(statusWith()),
    );
    vi.stubGlobal("fetch", fetchMock);
    renderRecovery();
    await screen.findByRole("timer");
    failed = true;
    fireEvent(window, new Event("online"));
    await screen.findByText("连接中断，尚未核对配置恢复状态");
    expect(screen.getByRole("button", { name: "确认生效" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "恢复上一配置" })).toBeDisabled();
    expect(posts(fetchMock)).toHaveLength(0);
  });
  it("rejects an older generation response instead of replacing a newer terminal observation", async () => {
    const api = mockStatus(statusWith());
    renderRecovery();
    await screen.findByRole("timer");
    api.setStatus(
      statusWith(
        operation({
          phase: "rolled_back",
          state: "rolled_back",
          generation: 9,
          deadline: undefined,
          canConfirm: false,
          canRollback: false,
        }),
      ),
    );
    fireEvent(window, new Event("online"));
    await screen.findByText("已恢复上一配置");
    api.setStatus(statusWith());
    fireEvent(window, new Event("online"));
    await screen.findByText("连接中断，尚未核对配置恢复状态");
    expect(screen.queryByRole("timer")).not.toBeInTheDocument();
    expect(screen.getByRole("alert")).toHaveTextContent("stale_response");
  });
  it("supersedes old in-flight reads and reconciles the newest operation ID", async () => {
    let resolveOld: ((response: Response) => void) | undefined;
    let calls = 0;
    const newOp = operation({ id: "newest-operation", generation: 10 });
    const fetchMock = vi.fn(async () =>
      ++calls === 1
        ? new Promise<Response>((resolve) => {
            resolveOld = resolve;
          })
        : response(statusWith(newOp)),
    );
    vi.stubGlobal("fetch", fetchMock);
    renderRecovery(<Controls />);
    await waitFor(() => expect(resolveOld).toBeDefined());
    fireEvent.click(screen.getByRole("button", { name: "手动读回" }));
    await screen.findByText("newest-operation");
    await act(async () => {
      resolveOld!(response(statusWith()));
    });
    expect(screen.queryByText("internal-operation-id")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "恢复上一配置" }));
    await waitFor(() => expect(posts(fetchMock)).toHaveLength(1));
    expect(JSON.parse(posts(fetchMock)[0][1].body as string)).toEqual({
      id: newOp.id,
    });
  });
  it("clears status and cancels in-flight reads on unauthorized cleanup without later GET or POST", async () => {
    let resolveRead: ((response: Response) => void) | undefined;
    const fetchMock = vi.fn(
      async () =>
        new Promise<Response>((resolve) => {
          resolveRead = resolve;
        }),
    );
    vi.stubGlobal("fetch", fetchMock);
    renderRecovery(<Controls />);
    await waitFor(() => expect(resolveRead).toBeDefined());
    fireEvent(window, new Event("be6500panel:unauthorized"));
    await act(async () => {
      resolveRead!(response(statusWith()));
    });
    fireEvent(window, new Event("online"));
    fireEvent.click(screen.getByRole("button", { name: "手动读回" }));
    expect(
      screen.queryByRole("region", { name: "全局配置恢复" }),
    ).not.toBeInTheDocument();
    expect(fetchMock).toHaveBeenCalledTimes(1);
    expect(posts(fetchMock)).toHaveLength(0);
  });
  it("survives StrictMode cleanup and exposes one shared busy action to all consumers", async () => {
    let resolvePost: ((response: Response) => void) | undefined;
    const fetchMock = vi.fn(async (_url: string, init?: RequestInit) =>
      init?.method === "POST"
        ? new Promise<Response>((resolve) => {
            resolvePost = resolve;
          })
        : response(statusWith()),
    );
    vi.stubGlobal("fetch", fetchMock);
    render(
      <StrictMode>
        <GlobalRecoveryProvider>
          <GlobalRecoveryBanner />
          <Controls />
        </GlobalRecoveryProvider>
      </StrictMode>,
    );
    fireEvent.click(await screen.findByRole("button", { name: "确认生效" }));
    await screen.findByText("confirm");
    expect(screen.getByRole("button", { name: "正在确认…" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "恢复上一配置" })).toBeDisabled();
    expect(
      screen.getByRole("button", { name: "刷新全局配置状态" }),
    ).toBeDisabled();
    expect(posts(fetchMock)).toHaveLength(1);
    await act(async () => {
      resolvePost!(response(operation()));
    });
    await waitFor(() =>
      expect(screen.getByTestId("busy")).toHaveTextContent("idle"),
    );
  });
  it("rejects same-ID phase regression even when generation is unchanged", async () => {
    const api = mockStatus(statusWith());
    renderRecovery();
    await screen.findByRole("timer");
    api.setStatus(
      statusWith(
        operation({
          phase: "committed",
          state: "committed",
          deadline: undefined,
          canConfirm: false,
        }),
      ),
    );
    fireEvent(window, new Event("online"));
    await screen.findByText("更改已生效");
    api.setStatus(statusWith());
    fireEvent(window, new Event("online"));
    await screen.findByText("连接中断，尚未核对配置恢复状态");
    expect(screen.queryByRole("timer")).not.toBeInTheDocument();
    expect(screen.getByRole("alert")).toHaveTextContent("stale_response");
  });
  it("keeps legacy pending status readable without treating disappearance as restoration", async () => {
    const pendingCommit = {
      id: "legacy-operation",
      deadline: new Date(Date.now() + 30_000).toISOString(),
    };
    const api = mockStatus({ enabled: true, generation: 8, pendingCommit });
    renderRecovery();
    await screen.findByRole("timer");
    expect(screen.getByRole("button", { name: "确认生效" })).toBeEnabled();
    api.setStatus({ enabled: true, generation: 9 });
    fireEvent(window, new Event("online"));
    await screen.findByText("未能确认配置操作结果，请重新核对");
    expect(screen.queryByText("已恢复上一配置")).not.toBeInTheDocument();
    expect(screen.getByText("legacy-operation")).toBeInTheDocument();
  });
});
