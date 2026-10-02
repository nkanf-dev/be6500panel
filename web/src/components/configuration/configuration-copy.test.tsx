import { afterEach, describe, expect, it, vi } from "vitest";
import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { CommitControls } from "./CommitControls";
import { ConfigurationSurface } from "./ConfigurationSurface";
import { DraftQueue } from "./DraftQueue";
import type { ConfigurationCommit, ConfigurationDraft } from "./contracts";
import type { ConfigurationController } from "./use-configuration";

const draft: ConfigurationDraft = {
  id: "internal-draft-hash",
  module: "network",
  generation: 7,
  diff: "--- network\n+++ network\n@@ -1,2 +1,2 @@\n config interface 'lan'\n- option ipaddr '192.0.2.1'\n+ option ipaddr '192.0.2.2'",
  risks: [],
  valid: true,
  errors: [],
  createdAt: "2026-01-01T00:00:00Z",
};

function makeController(
  overrides: Partial<ConfigurationController> = {},
): ConfigurationController {
  return {
    status: { enabled: true, generation: 7 },
    snapshot: { generation: 7, documents: [] },
    drafts: [draft],
    buffers: {},
    selectedIds: [draft.id],
    loading: false,
    edit: vi.fn(),
    reset: vi.fn(),
    select: vi.fn(),
    stage: vi.fn(),
    remove: vi.fn(),
    commit: vi.fn(),
    confirm: vi.fn(),
    rollback: vi.fn(),
    refresh: vi.fn(async () => {}),
    reconcile: vi.fn(async () => {}),
    ...overrides,
  };
}

function operation(state: ConfigurationCommit["state"]): ConfigurationCommit {
  return {
    id: "internal-operation-hash",
    state,
    generation: 8,
    changedModules: ["network"],
  };
}

afterEach(() => vi.useRealTimers());

describe("configuration transaction copy", () => {
  it("uses a readable apply action and keeps low-risk application direct", () => {
    const controller = makeController();
    render(<CommitControls controller={controller} />);
    const apply = screen.getByRole("button", { name: "应用已选更改 (1)" });
    expect(apply).toHaveTextContent("应用更改 (1)");
    fireEvent.click(apply);
    expect(controller.commit).toHaveBeenCalledWith(false);
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(screen.queryByText(/Commit|提交/)).not.toBeInTheDocument();
  });

  it("explains high-risk changes in plain language and gates technical metadata", () => {
    const risky = {
      ...draft,
      risks: [{ code: "management_address", message: "管理地址发生变化" }],
    };
    const controller = makeController({ drafts: [risky] });
    render(<CommitControls controller={controller} />);
    fireEvent.click(screen.getByRole("button", { name: "应用已选更改 (1)" }));
    const dialog = screen.getByRole("dialog", { name: "确认应用高风险更改" });
    expect(within(dialog).getByText("管理地址发生变化")).toBeVisible();
    expect(within(dialog).getByText("网络 · IPv4 地址")).toBeVisible();
    expect(within(dialog).getByText("management_address")).not.toBeVisible();
    expect(within(dialog).getByText(draft.id)).not.toBeVisible();
    expect(
      within(dialog).getByText("network / option ipaddr"),
    ).not.toBeVisible();
    expect(within(dialog).getByText("7")).not.toBeVisible();
    expect(controller.commit).not.toHaveBeenCalled();
    fireEvent.click(within(dialog).getByText("高级详情"));
    expect(within(dialog).getByText("management_address")).toBeVisible();
    expect(within(dialog).getByText("network / option ipaddr")).toBeVisible();
    fireEvent.click(within(dialog).getByRole("button", { name: "确认并应用" }));
    expect(controller.commit).toHaveBeenCalledWith(true, [draft.id]);
  });

  it("describes pending changes as temporary and does not claim expiry restored them", async () => {
    vi.useFakeTimers({ toFake: ["Date", "setInterval", "clearInterval"] });
    vi.setSystemTime(new Date("2026-01-01T00:00:00Z"));
    const controller = makeController({
      status: {
        enabled: true,
        generation: 8,
        pendingCommit: {
          id: "internal-pending-hash",
          deadline: new Date(Date.now() + 2_000).toISOString(),
        },
      },
      operation: operation("pending_confirmation"),
    });
    render(<CommitControls controller={controller} />);
    expect(screen.getByText("待确认生效")).toBeVisible();
    expect(screen.getByText("临时应用")).toBeVisible();
    expect(
      screen.getByText(
        "配置已应用，正在等待连接确认（剩余2s）。超时未确认将自动恢复上一配置。",
      ),
    ).toBeVisible();
    expect(
      screen.getByRole("timer", { name: "连接确认剩余时间" }),
    ).toHaveTextContent("2s");
    expect(screen.getByRole("button", { name: "确认生效" })).toBeEnabled();
    expect(screen.getByText("internal-pending-hash")).not.toBeVisible();
    await act(async () => vi.advanceTimersByTimeAsync(2_001));
    expect(screen.getByText("确认期限已到，正在核对恢复状态")).toBeVisible();
    expect(screen.getByRole("button", { name: "确认生效" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "刷新应用状态" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "恢复上一配置" })).toBeEnabled();
    expect(
      screen.queryByText(/已恢复上一配置|更改已生效/),
    ).not.toBeInTheDocument();
    expect(controller.rollback).not.toHaveBeenCalled();
    expect(controller.confirm).not.toHaveBeenCalled();
  });

  it("names pending actions without exposing transaction IDs", () => {
    const controller = makeController({
      status: {
        enabled: true,
        generation: 8,
        pendingCommit: {
          id: "internal-pending-hash",
          deadline: new Date(Date.now() + 120_000).toISOString(),
        },
      },
    });
    render(<CommitControls controller={controller} />);
    fireEvent.click(screen.getByRole("button", { name: "刷新应用状态" }));
    fireEvent.click(screen.getByRole("button", { name: "确认生效" }));
    fireEvent.click(screen.getByRole("button", { name: "恢复上一配置" }));
    expect(controller.reconcile).toHaveBeenCalledOnce();
    expect(controller.confirm).toHaveBeenCalledOnce();
    expect(controller.rollback).toHaveBeenCalledOnce();
  });

  it.each([
    ["committed", "更改已生效 · 网络"],
    ["rolled_back", "已恢复上一配置 · 网络"],
  ] as const)(
    "describes a server-reported %s outcome without visible IDs or generations",
    (state, label) => {
      render(
        <CommitControls
          controller={makeController({ operation: operation(state) })}
        />,
      );
      expect(screen.getByText(label)).toBeVisible();
      expect(screen.getByText("internal-operation-hash")).not.toBeVisible();
      expect(screen.getByText("8")).not.toBeVisible();
      fireEvent.click(screen.getByText("高级详情"));
      expect(screen.getByText("internal-operation-hash")).toBeVisible();
    },
  );

  it("does not invent a confirmed outcome when pending status disappears", () => {
    render(
      <CommitControls
        controller={makeController({
          operation: operation("pending_confirmation"),
        })}
      />,
    );
    expect(
      screen.getByText("已核对当前配置，请检查更改结果 · 网络"),
    ).toBeVisible();
    expect(
      screen.queryByText(/更改已生效|已恢复上一配置/),
    ).not.toBeInTheDocument();
  });

  it("teaches the empty queue workflow without suggesting checking applies changes", () => {
    render(
      <DraftQueue
        controller={makeController({ drafts: [], selectedIds: [] })}
      />,
    );
    expect(screen.getByRole("heading", { name: "检查结果" })).toBeVisible();
    expect(
      screen.getByRole("heading", { name: "暂无待应用更改" }),
    ).toBeVisible();
    expect(
      screen.getByText(
        "先编辑配置，再点击“检查更改”。检查只保存草稿；选择检查通过的草稿后，点击“应用更改”才会生效。",
      ),
    ).toBeVisible();
  });

  it("keeps invalid and stale draft semantics readable with raw diagnostics optional", () => {
    const invalid = {
      ...draft,
      id: "internal-invalid-hash",
      valid: false,
      errors: [{ code: "invalid_port", message: "端口超出范围" }],
      risks: [{ code: "management_address", message: "管理地址发生变化" }],
    };
    const stale = { ...draft, id: "internal-stale-hash", generation: 6 };
    const controller = makeController({
      drafts: [invalid, stale],
      selectedIds: [],
    });
    render(<DraftQueue controller={controller} />);
    expect(
      screen.getByRole("checkbox", { name: "选择网络草稿 1" }),
    ).toBeDisabled();
    expect(
      screen.getByRole("checkbox", { name: "选择网络草稿 2" }),
    ).toBeDisabled();
    expect(screen.getByText("需要重新检查")).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "查看网络草稿 1详情" }));
    expect(screen.getByText("端口超出范围")).toBeVisible();
    expect(screen.getByText("管理地址发生变化")).toBeVisible();
    expect(screen.getByText("invalid_port")).not.toBeVisible();
    expect(screen.getByText("management_address")).not.toBeVisible();
    expect(screen.getByText(invalid.id)).not.toBeVisible();
    expect(screen.getByTestId("configuration-diff")).not.toBeVisible();
    fireEvent.click(screen.getByText("高级详情"));
    expect(screen.getByText("invalid_port")).toBeVisible();
    expect(screen.getByTestId("configuration-diff")).toBeVisible();
  });

  it("numbers alternative drafts per document and preserves intentional selection", () => {
    const older = { ...draft, id: "internal-older-hash" };
    const controller = makeController({ drafts: [older, draft] });
    render(<DraftQueue controller={controller} />);
    expect(
      screen.getByRole("checkbox", { name: "选择网络草稿 1" }),
    ).not.toBeChecked();
    expect(
      screen.getByRole("checkbox", { name: "选择网络草稿 2" }),
    ).toBeChecked();
    fireEvent.click(screen.getByRole("checkbox", { name: "选择网络草稿 1" }));
    expect(controller.select).toHaveBeenCalledWith(older, true);
    fireEvent.click(screen.getByRole("button", { name: "删除网络草稿 1" }));
    expect(controller.remove).toHaveBeenCalledWith(older.id);
    expect(
      screen.getByText(
        "检查只保存草稿，不会应用更改。每个配置文档只能选择一个草稿，再统一应用更改。",
      ),
    ).toBeVisible();
  });

  it("keeps configuration versions optional in the surface header", () => {
    render(
      <ConfigurationSurface
        title="配置工作区"
        controller={makeController({
          drafts: [],
          selectedIds: [],
          buffers: {
            network: {
              content: "edited",
              savedContent: "saved",
              generation: 7,
            },
          },
        })}
      >
        <div>编辑区域</div>
      </ConfigurationSurface>,
    );
    expect(screen.getByText("编辑配置 → 检查更改 → 应用更改")).toBeVisible();
    expect(screen.getByText("1 个文档有未检查更改")).toBeVisible();
    expect(screen.getByText("7")).not.toBeVisible();
    fireEvent.click(screen.getByText("高级详情"));
    expect(screen.getByText("7")).toBeVisible();
    expect(screen.queryByText(/g7|Commit/)).not.toBeInTheDocument();
  });
});
