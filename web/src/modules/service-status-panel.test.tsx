import {
  act,
  render,
  renderHook,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ApiError } from "../lib/api";
import type {
  ServiceActionResult,
  ServiceObservation,
  ServiceStatusSnapshot,
} from "../lib/service-status-api";
import { loadServiceStatus, runServiceAction } from "../lib/service-status-api";
import {
  ServiceStatusPanel,
  ServiceStatusView,
  useServiceStatus,
} from "./service-status-panel";

vi.mock("../lib/service-status-api", async (original) => ({
  ...(await original<typeof import("../lib/service-status-api")>()),
  loadServiceStatus: vi.fn(),
  runServiceAction: vi.fn(),
}));
const loader = vi.mocked(loadServiceStatus);
const sampledAt = "2026-10-03T10:00:00Z";
const now = Date.parse(sampledAt);
const row = (value: Partial<ServiceObservation>): ServiceObservation => ({
  name: "dnsmasq",
  instance: "main",
  configured: "present",
  registered: "registered",
  processState: "running",
  procdRunning: true,
  pid: 1820,
  reportedPID: 1820,
  executable: "/usr/sbin/dnsmasq",
  uptimeSeconds: 123,
  rssBytes: 2097152,
  protected: false,
  ...value,
});
const snapshot = (
  services: readonly ServiceObservation[] = [row({})],
): ServiceStatusSnapshot => ({
  source: "procd · /proc",
  sampledAt,
  checkedAt: sampledAt,
  stale: false,
  errors: [],
  services,
});
afterEach(() => {
  vi.useRealTimers();
  vi.clearAllMocks();
});

describe("service status observation view", () => {
  it("shows verified process data, source, times, and diagnostics without health claims", () => {
    render(<ServiceStatusView snapshot={snapshot()} now={now} />);
    expect(screen.getByText("运行中 (PID 1820)")).toHaveClass("badge-success");
    expect(screen.getByText("procd · /proc")).toBeInTheDocument();
    expect(screen.getByText("最后成功采样")).toBeInTheDocument();
    expect(screen.getByText("后端最近尝试")).toBeInTheDocument();
    expect(screen.getByText("已注册")).not.toHaveClass("badge-success");
    expect(screen.getByText("脚本存在")).not.toHaveClass("badge-success");
    expect(screen.getByText("/usr/sbin/dnsmasq")).toBeInTheDocument();
    expect(screen.getByText("0 时 2 分")).toBeInTheDocument();
    expect(screen.getByText("2.0 MiB")).toBeInTheDocument();
    expect(
      screen.getByRole("link", { name: "查看诊断与日志" }),
    ).toHaveAttribute("href", "#/system?tab=diagnostics");
    expect(
      screen.queryByText(/服务健康|网络连通正常|WAN 正常/),
    ).not.toBeInTheDocument();
  });

  it("never treats unknown, present configuration, or an unverified procd PID as running or stopped", () => {
    render(
      <ServiceStatusView
        now={now}
        snapshot={snapshot([
          row({
            name: "missing-observation",
            processState: "unknown",
            pid: undefined,
            reportedPID: 44,
            errorCode: "proc_unavailable",
          }),
          row({
            name: "unverified-report",
            processState: "running",
            pid: undefined,
            reportedPID: 55,
          }),
          row({
            name: "absent-script",
            processState: "not_running",
            pid: undefined,
            configured: "absent",
            registered: "unregistered",
          }),
        ])}
      />,
    );
    const missing = screen.getByRole("article", {
      name: "missing-observation · main",
    });
    expect(within(missing).getByText("状态未知")).toBeInTheDocument();
    expect(within(missing).queryByText("未运行")).not.toBeInTheDocument();
    expect(within(missing).getByText("proc_unavailable")).toBeInTheDocument();
    expect(within(missing).getByText("44")).toBeInTheDocument();
    const unverified = screen.getByRole("article", {
      name: "unverified-report · main",
    });
    expect(within(unverified).getByText("状态未知")).toBeInTheDocument();
    expect(within(unverified).queryByText(/运行中/)).not.toBeInTheDocument();
    expect(
      within(unverified).getByText("缺少经 /proc 核验的 PID"),
    ).toBeInTheDocument();
    const stopped = screen.getByRole("article", {
      name: "absent-script · main",
    });
    expect(within(stopped).getByText("未运行")).toBeInTheDocument();
    expect(within(stopped).getByText("脚本不存在")).toBeInTheDocument();
    expect(within(stopped).getByText("未注册")).toBeInTheDocument();
  });

  it.each([
    { stale: true, now, error: undefined },
    { stale: false, now: now + 15000, error: undefined },
    {
      stale: false,
      now,
      error: new ApiError({ code: "read_failed", message: "连接中断" }),
    },
  ])(
    "overrides running truth for stale, aged, or failed observations: %o",
    (value) => {
      render(
        <ServiceStatusView
          snapshot={{ ...snapshot(), stale: value.stale }}
          now={value.now}
          error={value.error}
        />,
      );
      expect(screen.getByText("状态未知（旧快照）")).not.toHaveClass(
        "badge-success",
      );
      expect(
        screen.getByText("上次记录：运行中 (PID 1820)"),
      ).toBeInTheDocument();
      expect(screen.queryByText("运行中 (PID 1820)")).not.toBeInTheDocument();
      expect(
        screen.getByText("观察已过期，当前进程状态未知。以下仅为上次记录。"),
      ).toBeInTheDocument();
    },
  );

  it("shows null sampling and backend/module errors without fabricating data", () => {
    render(
      <ServiceStatusView
        now={now}
        snapshot={{
          ...snapshot([]),
          sampledAt: null,
          stale: true,
          errorCode: "observation_unavailable",
          errors: [
            { module: "procd", code: "ubus_failed", message: "无法读取 procd" },
          ],
        }}
      />,
    );
    expect(screen.getByText("从未成功采样")).toBeInTheDocument();
    expect(screen.getByText("observation_unavailable")).toBeInTheDocument();
    expect(
      screen.getByText("procd · ubus_failed · 无法读取 procd"),
    ).toBeInTheDocument();
    expect(screen.getByText("暂无服务观察记录")).toBeInTheDocument();
  });

  it("combines search with failure/unknown filters and distinguishes no matches from no observations", async () => {
    const user = userEvent.setup();
    render(
      <ServiceStatusView
        now={now}
        snapshot={snapshot([
          row({}),
          row({
            name: "ddns",
            processState: "failed",
            pid: undefined,
            errorCode: "process_missing",
          }),
          row({
            name: "frpc",
            instance: "remote",
            processState: "unknown",
            pid: undefined,
            registered: "unknown",
          }),
          row({ name: "unverified", processState: "running", pid: undefined }),
        ])}
      />,
    );
    await user.selectOptions(
      screen.getByRole("combobox", { name: "服务状态筛选" }),
      "attention",
    );
    expect(
      screen.queryByRole("article", { name: "dnsmasq · main" }),
    ).not.toBeInTheDocument();
    expect(
      screen.getByRole("article", { name: "ddns · main" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("article", { name: "unverified · main" }),
    ).toBeInTheDocument();
    await user.type(
      screen.getByRole("searchbox", { name: "搜索服务与实例" }),
      "REMOTE",
    );
    expect(screen.getAllByRole("article")).toHaveLength(1);
    expect(
      screen.getByRole("article", { name: "frpc · remote" }),
    ).toBeInTheDocument();
    await user.clear(screen.getByRole("searchbox", { name: "搜索服务与实例" }));
    await user.selectOptions(
      screen.getByRole("combobox", { name: "服务状态筛选" }),
      "failed",
    );
    expect(screen.getByText("启动异常")).toBeInTheDocument();
    expect(screen.getAllByRole("article")).toHaveLength(1);
    await user.type(
      screen.getByRole("searchbox", { name: "搜索服务与实例" }),
      "nothing",
    );
    expect(screen.getByText("没有匹配的服务")).toBeInTheDocument();
    expect(screen.queryByText("暂无服务观察记录")).not.toBeInTheDocument();
  });

  it("keeps independent rescue protected and offers refresh observation only", async () => {
    const onRefresh = vi.fn();
    const user = userEvent.setup();
    render(
      <ServiceStatusView
        now={now}
        onRefresh={onRefresh}
        snapshot={snapshot([
          row({ name: "be6500-rescue", instance: "rescue", protected: true }),
        ])}
      />,
    );
    const rescue = screen.getByRole("article", {
      name: "be6500-rescue · rescue",
    });
    expect(
      within(rescue).getByText("独立救援通道 · 受保护 · 仅观察"),
    ).toBeInTheDocument();
    expect(within(rescue).queryByRole("button")).not.toBeInTheDocument();
    const buttons = screen.getAllByRole("button");
    expect(buttons).toHaveLength(1);
    expect(buttons[0]).toHaveAccessibleName("刷新观察");
    await user.click(buttons[0]);
    expect(onRefresh).toHaveBeenCalledOnce();
    expect(
      screen.queryByRole("button", { name: /停止|重启|重载|启动|应用/ }),
    ).not.toBeInTheDocument();
  });
});

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (cause: unknown) => void;
  const promise = new Promise<T>((onResolve, onReject) => {
    resolve = onResolve;
    reject = onReject;
  });
  return { promise, resolve, reject };
}

describe("service status observation lifecycle", () => {
  it("polls every five seconds, skips overlap, and cancels on unmount", async () => {
    vi.useFakeTimers();
    vi.setSystemTime(now);
    const first = deferred<ServiceStatusSnapshot>();
    const second = deferred<ServiceStatusSnapshot>();
    loader.mockReturnValueOnce(first.promise).mockReturnValue(second.promise);
    const { result, unmount } = renderHook(() => useServiceStatus());
    await act(async () => {
      await Promise.resolve();
    });
    expect(loader).toHaveBeenCalledTimes(1);
    expect(result.current.loading).toBe(true);
    act(() => result.current.refresh());
    await act(async () => {
      await vi.advanceTimersByTimeAsync(10000);
    });
    expect(loader).toHaveBeenCalledTimes(1);
    await act(async () => {
      first.resolve(snapshot());
    });
    expect(result.current.snapshot?.services[0].pid).toBe(1820);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(5000);
    });
    expect(loader).toHaveBeenCalledTimes(2);
    expect(result.current.loading).toBe(false);
    expect(result.current.fetching).toBe(true);
    expect(result.current.snapshot?.source).toBe("procd · /proc");
    expect(result.current.snapshot?.sampledAt).toBe(sampledAt);
    const activeSignal = loader.mock.calls[1][0];
    unmount();
    expect(activeSignal?.aborted).toBe(true);
    await act(async () => {
      second.resolve(snapshot());
      await vi.advanceTimersByTimeAsync(10000);
    });
    expect(loader).toHaveBeenCalledTimes(2);
  });

  it("retains explicit stale rows on failed reads and clears stale only after a fresh success", async () => {
    vi.useFakeTimers();
    vi.setSystemTime(now);
    loader
      .mockResolvedValueOnce(snapshot())
      .mockRejectedValueOnce(
        new ApiError({ code: "observation_unavailable", message: "读回失败" }),
      );
    const { result, unmount } = renderHook(() => useServiceStatus());
    await act(async () => {
      await Promise.resolve();
    });
    expect(result.current.snapshot?.stale).toBe(false);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(5000);
    });
    expect(result.current.snapshot?.stale).toBe(true);
    expect(result.current.snapshot?.sampledAt).toBe(sampledAt);
    expect(result.current.snapshot?.services[0].pid).toBe(1820);
    expect(result.current.error).toMatchObject({
      code: "observation_unavailable",
    });
    const freshTime = new Date(now + 5000).toISOString();
    loader.mockResolvedValueOnce({
      ...snapshot(),
      sampledAt: freshTime,
      checkedAt: freshTime,
    });
    await act(async () => {
      result.current.refresh();
    });
    expect(result.current.snapshot?.stale).toBe(false);
    expect(result.current.error).toBeUndefined();
    expect(loader).toHaveBeenCalledTimes(3);
    unmount();
  });

  it("preserves failed snapshot provenance while a retry is pending", async () => {
    vi.useFakeTimers();
    vi.setSystemTime(now);
    const pending = deferred<ServiceStatusSnapshot>();
    const cause = new ApiError({
      code: "observation_unavailable",
      message: "read failed",
    });
    loader
      .mockResolvedValueOnce(snapshot())
      .mockRejectedValueOnce(cause)
      .mockReturnValueOnce(pending.promise);
    const { result, unmount } = renderHook(() => useServiceStatus());
    await act(async () => {});
    await act(async () => vi.advanceTimersByTimeAsync(5000));
    const failed = result.current.snapshot;
    act(() => result.current.refresh());
    expect(result.current.loading).toBe(false);
    expect(result.current.snapshot).toBe(failed);
    expect(result.current.snapshot?.stale).toBe(true);
    expect(result.current.snapshot?.source).toBe("procd · /proc");
    expect(result.current.error).toBe(cause);
    unmount();
    expect(loader.mock.calls[2][0]?.aborted).toBe(true);
  });
  it("ages retained data at fifteen seconds even when the next read is still pending", async () => {
    vi.useFakeTimers();
    vi.setSystemTime(now);
    const pending = deferred<ServiceStatusSnapshot>();
    loader.mockResolvedValueOnce(snapshot()).mockReturnValue(pending.promise);
    const { result, unmount } = renderHook(() => useServiceStatus());
    await act(async () => {
      await Promise.resolve();
    });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(14000);
    });
    expect(result.current.snapshot?.stale).toBe(false);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(1000);
    });
    expect(result.current.snapshot?.stale).toBe(true);
    expect(result.current.loading).toBe(false);
    expect(result.current.fetching).toBe(true);
    expect(loader).toHaveBeenCalledTimes(2);
    unmount();
  });

  it("mounts its own loader and retries observation without hiding initial failure", async () => {
    const fresh = new Date().toISOString();
    loader
      .mockRejectedValueOnce(
        new ApiError({ code: "ubus_failed", message: "读取失败" }),
      )
      .mockResolvedValueOnce({
        ...snapshot(),
        sampledAt: fresh,
        checkedAt: fresh,
      });
    const user = userEvent.setup();
    const { unmount } = render(<ServiceStatusPanel />);
    await screen.findByText("读取失败 · ubus_failed");
    expect(screen.queryByText("运行中 (PID 1820)")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "刷新观察" }));
    await screen.findByText("运行中 (PID 1820)");
    await waitFor(() =>
      expect(screen.queryByRole("alert")).not.toBeInTheDocument(),
    );
    expect(loader).toHaveBeenCalledTimes(2);
    unmount();
  });
});

describe("controlled installed-service actions", () => {
  it("labels protected management services separately from independent rescue", () => {
    render(
      <ServiceStatusView
        now={now}
        snapshot={snapshot([
          row({ name: "dropbear", protected: true }),
          row({ name: "be6500panel", protected: true }),
        ])}
      />,
    );
    for (const name of ["dropbear", "be6500panel"]) {
      const management = screen.getByRole("article", {
        name: `${name} · main`,
      });
      expect(
        within(management).getByText("管理服务 · 受保护（本区域不操作）"),
      ).toBeInTheDocument();
      expect(
        within(management).queryByText("独立救援通道 · 受保护 · 仅观察"),
      ).not.toBeInTheDocument();
      expect(within(management).queryByRole("button")).not.toBeInTheDocument();
    }
  });

  it("protects both exact-name rescue listener rows even if a legacy flag is missing or false", () => {
    const onAction = vi.fn().mockResolvedValue(undefined);
    render(
      <ServiceStatusView
        now={now}
        onAction={onAction}
        snapshot={snapshot([
          row({
            name: "be6500-rescue",
            instance: "lan-22",
            protected: false,
            actions: ["stop", "restart"],
          }),
          row({
            name: "be6500-rescue",
            instance: "lan-2222",
            protected: true,
            actions: ["stop", "restart"],
          }),
        ])}
      />,
    );
    for (const instance of ["lan-22", "lan-2222"]) {
      const listener = screen.getByRole("article", {
        name: `be6500-rescue · ${instance}`,
      });
      expect(
        within(listener).getByText("独立救援通道 · 受保护 · 仅观察"),
      ).toBeInTheDocument();
      expect(within(listener).queryByRole("button")).not.toBeInTheDocument();
    }
  });

  it("uses only fresh installed allowlisted advertised actions, never rescue or management actions", () => {
    const onAction = vi.fn().mockResolvedValue(undefined);
    render(
      <ServiceStatusView
        now={now}
        onAction={onAction}
        snapshot={snapshot([
          row({
            name: "ddns",
            actions: ["start", "stop", "restart", "reload"],
          }),
          row({
            name: "dnsmasq",
            actions: ["start", "stop", "reload", "restart"],
          }),
          row({
            name: "be6500-rescue",
            protected: true,
            actions: ["stop", "restart"],
          }),
          row({ name: "dropbear", actions: ["stop", "restart"] }),
          row({ name: "network", actions: ["restart"] }),
          row({ name: "wifi", actions: ["reload"] }),
          row({
            name: "not-installed",
            configured: "absent",
            actions: ["start"],
          }),
        ])}
      />,
    );
    const ddns = screen.getByRole("article", { name: "ddns · main" });
    expect(within(ddns).getAllByRole("button")).toHaveLength(4);
    const dns = screen.getByRole("article", { name: "dnsmasq · main" });
    expect(within(dns).getAllByRole("button")).toHaveLength(2);
    expect(
      within(dns).queryByRole("button", { name: /启动|停止/ }),
    ).not.toBeInTheDocument();
    for (const name of [
      "be6500-rescue",
      "dropbear",
      "network",
      "wifi",
      "not-installed",
    ]) {
      expect(
        within(
          screen.getByRole("article", { name: `${name} · main` }),
        ).queryByRole("button"),
      ).not.toBeInTheDocument();
    }
    expect(onAction).not.toHaveBeenCalled();
  });

  it("never offers ddns stop for unknown/unverified processes or any action on stale snapshots", () => {
    const onAction = vi.fn().mockResolvedValue(undefined);
    const { rerender } = render(
      <ServiceStatusView
        now={now}
        onAction={onAction}
        snapshot={snapshot([
          row({
            name: "ddns",
            processState: "unknown",
            pid: undefined,
            actions: ["start", "stop"],
          }),
        ])}
      />,
    );
    expect(
      screen.queryByRole("button", { name: "停止 ddns" }),
    ).not.toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "启动 ddns" }),
    ).toBeInTheDocument();
    rerender(
      <ServiceStatusView
        now={now + 15000}
        onAction={onAction}
        snapshot={snapshot([row({ name: "ddns", actions: ["start", "stop"] })])}
      />,
    );
    expect(screen.queryByRole("button")).not.toBeInTheDocument();
  });

  it("requires a selected-action dialog and explicit one-line DNS/DHCP impact acknowledgement", async () => {
    const user = userEvent.setup();
    const onAction = vi.fn().mockResolvedValue(undefined);
    render(
      <ServiceStatusView
        now={now}
        onAction={onAction}
        snapshot={snapshot([
          row({
            name: "dnsmasq",
            actions: ["reload"],
            actionImpact: "可能短暂中断 DNS/DHCP",
          }),
        ])}
      />,
    );
    await user.click(screen.getByRole("button", { name: "重载 dnsmasq" }));
    const dialog = screen.getByRole("dialog", { name: "确认重载 dnsmasq" });
    expect(
      within(dialog).getByText(/操作作用于整个服务，可能影响全部实例/),
    ).toBeInTheDocument();
    expect(
      within(dialog).getByText(
        /所选实例仅用于显示观察上下文，不是单独操作目标/,
      ),
    ).toBeInTheDocument();
    expect(
      within(dialog).getByText("dnsmasq · main · 重载"),
    ).toBeInTheDocument();
    expect(
      within(dialog).getByText("可能短暂中断 DNS/DHCP"),
    ).toBeInTheDocument();
    expect(
      within(dialog).getByRole("button", { name: "确认重载 dnsmasq" }),
    ).toBeDisabled();
    expect(onAction).not.toHaveBeenCalled();
    await user.click(
      within(dialog).getByRole("checkbox", {
        name: "我确认本次操作可能短暂中断 DNS/DHCP 服务",
      }),
    );
    await user.click(
      within(dialog).getByRole("button", { name: "确认重载 dnsmasq" }),
    );
    expect(onAction).toHaveBeenCalledOnce();
    expect(onAction).toHaveBeenCalledWith({
      service: "dnsmasq",
      action: "reload",
      confirmImpact: true,
    });
    await waitFor(() =>
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument(),
    );
  });

  it("lets a user cancel without sending a command and checks latest advertised permission before confirming", async () => {
    const user = userEvent.setup();
    const onAction = vi.fn().mockResolvedValue(undefined);
    const { rerender } = render(
      <ServiceStatusView
        now={now}
        onAction={onAction}
        snapshot={snapshot([row({ name: "ddns", actions: ["restart"] })])}
      />,
    );
    await user.click(screen.getByRole("button", { name: "重启 ddns" }));
    await user.click(screen.getByRole("button", { name: "取消" }));
    expect(onAction).not.toHaveBeenCalled();
    await user.click(screen.getByRole("button", { name: "重启 ddns" }));
    rerender(
      <ServiceStatusView
        now={now}
        onAction={onAction}
        snapshot={snapshot([row({ name: "ddns", actions: [] })])}
      />,
    );
    expect(
      screen.getByRole("button", { name: "确认重启 ddns" }),
    ).toBeDisabled();
    expect(
      screen.getByText("最新观察已不允许此操作，请关闭确认并刷新观察。"),
    ).toBeInTheDocument();
    expect(onAction).not.toHaveBeenCalled();
  });

  it("treats accepted commands as neutral and still shows unknown observed state", () => {
    const readback = snapshot([
      row({ name: "ddns", processState: "unknown", pid: undefined }),
    ]);
    render(
      <ServiceStatusView
        now={now}
        snapshot={readback}
        actionResult={{
          service: "ddns",
          action: "start",
          commandAccepted: true,
          snapshot: readback,
        }}
      />,
    );
    expect(
      screen.getByText(
        "命令已接受，不代表服务运行或健康；以下以实际观察为准。",
      ),
    ).not.toHaveClass("badge-success");
    expect(screen.getByText("状态未知")).toBeInTheDocument();
    expect(screen.queryByText(/操作成功|启动成功/)).not.toBeInTheDocument();
  });

  it("refreshes after action refusal, retains error detail, and never synthesizes running state", async () => {
    const fresh = new Date().toISOString();
    loader.mockResolvedValue({
      ...snapshot([
        row({
          name: "ddns",
          actions: ["start"],
          processState: "not_running",
          pid: undefined,
        }),
      ]),
      sampledAt: fresh,
      checkedAt: fresh,
    });
    const actionLoader = vi.mocked(runServiceAction);
    actionLoader.mockRejectedValueOnce(
      new ApiError({ code: "service_action_refused", message: "操作被拒绝" }),
    );
    const user = userEvent.setup();
    const { unmount } = render(<ServiceStatusPanel />);
    await screen.findByRole("button", { name: "启动 ddns" });
    await user.click(screen.getByRole("button", { name: "启动 ddns" }));
    await user.click(screen.getByRole("button", { name: "确认启动 ddns" }));
    await screen.findByText("操作被拒绝 · service_action_refused");
    await waitFor(() => expect(loader).toHaveBeenCalledTimes(2));
    expect(actionLoader).toHaveBeenCalledWith(
      { service: "ddns", action: "start", confirmImpact: false },
      expect.any(AbortSignal),
    );
    expect(screen.getByText("未运行")).toBeInTheDocument();
    expect(screen.queryByText(/运行中/)).not.toBeInTheDocument();
    unmount();
  });

  it("serializes action and observation work and cancels a pending command on unmount", async () => {
    vi.useFakeTimers();
    vi.setSystemTime(now);
    loader.mockResolvedValue(
      snapshot([row({ name: "ddns", actions: ["restart"] })]),
    );
    const command = deferred<ServiceActionResult>();
    const actionLoader = vi.mocked(runServiceAction);
    actionLoader.mockReturnValueOnce(command.promise);
    const { result, unmount } = renderHook(() => useServiceStatus());
    await act(async () => {
      await Promise.resolve();
    });
    let task!: Promise<void>;
    act(() => {
      task = result.current.runAction({
        service: "ddns",
        action: "restart",
        confirmImpact: false,
      });
    });
    expect(result.current.actionPending).toBe(true);
    act(() => {
      result.current.refresh();
      void result.current.runAction({
        service: "ddns",
        action: "restart",
        confirmImpact: false,
      });
    });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(10000);
    });
    expect(loader).toHaveBeenCalledTimes(1);
    expect(actionLoader).toHaveBeenCalledTimes(1);
    unmount();
    expect(actionLoader.mock.calls[0][1]?.aborted).toBe(true);
    await act(async () => {
      command.resolve({
        service: "ddns",
        action: "restart",
        commandAccepted: true,
        snapshot: snapshot(),
      });
      await task;
    });
    expect(loader).toHaveBeenCalledTimes(1);
  });
});
