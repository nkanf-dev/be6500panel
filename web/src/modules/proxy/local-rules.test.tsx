import {
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { Effect } from "effect";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ApiError } from "../../lib/api";
import { LocalRulesEditor } from "./local-rules";
import { ProxyPage } from "../proxy";

vi.mock("../../lib/use-resource", () => ({
  useResource: () => ({
    data: { nodes: [], diagnostics: [], selectedNodeId: "" },
    loading: false,
    reload: vi.fn(),
  }),
}));
vi.mock("../runtime/use-runtime", () => ({
  useRuntime: () => ({
    enabled: true,
    pending: false,
    loading: false,
    refresh: vi.fn(),
  }),
}));
vi.mock("./gateway-panel", () => ({
  GatewayPanel: () => <section aria-label="正常网关状态">网关正常状态</section>,
}));
vi.mock("./import-form", () => ({
  ProxyImportForm: () => <section aria-label="节点默认视图">导入节点</section>,
}));

import {
  localRulesApi,
  type LocalPolicy,
  type LocalRulesState,
  type LocalRule,
  type Rule,
} from "./local-rules-api";

const sourceFingerprint = `${"a".repeat(64)}:1`;
const source: Rule = {
  kind: "domain-suffix",
  value: "example.test",
  target: "proxy",
  index: 27,
};
const local: LocalRule = {
  id: "stable-local",
  enabled: true,
  label: "已有规则",
  note: "保留备注",
  rule: { kind: "domain", value: "existing.test", target: "proxy", index: 3 },
};
function state(
  policy: LocalPolicy = { rules: [], subscriptionEdits: [] },
): LocalRulesState {
  return {
    draft: { policy, revision: "saved-revision" },
    subscriptionRevision: "subscription-one",
    subscriptionRules: [{ fingerprint: sourceFingerprint, rule: source }],
    preview: {
      rules: [source],
      provenance: [
        {
          effectiveIndex: 0,
          layer: "subscription",
          stableId: sourceFingerprint,
          label: "",
          sourceFingerprint,
          sourceIndex: 27,
          sourceOrdinal: 0,
          kind: source.kind,
          value: source.value,
          target: source.target,
        },
      ],
      diagnostics: [],
    },
    applied: { state: "none" },
    runtimeGeneration: 9,
  };
}
const empty = () => state();
const save = () => screen.getByRole("button", { name: "保存草稿" });
const preview = () => screen.getByRole("button", { name: "预览合并规则" });
const apply = () => screen.getByRole("button", { name: "应用已保存规则" });
const row = (id: string) =>
  screen.getByRole("group", { name: `本地规则 ${id}` });
async function loaded() {
  await waitFor(() => expect(preview()).toBeEnabled());
}
beforeEach(() => {
  vi.spyOn(localRulesApi, "state").mockReturnValue(Effect.succeed(empty()));
  vi.spyOn(localRulesApi, "preview").mockReturnValue(
    Effect.succeed(empty().preview),
  );
  vi.spyOn(localRulesApi, "save").mockImplementation((policy) =>
    Effect.succeed({
      ...state(policy),
      draft: { policy, revision: "new-revision" },
    }),
  );
  vi.spyOn(localRulesApi, "apply").mockReturnValue(
    Effect.succeed({
      status: {
        service: "sing-box",
        state: "running",
        generation: 10,
        configured: true,
        artifactAvailable: true,
        rssBytes: 0,
        rssAvailable: false,
        desired: true,
        restarts: 0,
      },
      draftRevision: "saved-revision",
      configSHA256: "b".repeat(64),
      applied: true,
    }),
  );
});

describe("explicit local draft rules", () => {
  it("places the GPT direct shortcut ahead of existing broad local rules", async () => {
    vi.mocked(localRulesApi.state).mockReturnValue(
      Effect.succeed(
        state({
          rules: [
            { ...local, rule: { kind: "match", target: "proxy", index: 0 } },
          ],
          subscriptionEdits: [],
        }),
      ),
    );
    render(<LocalRulesEditor />);
    await loaded();
    fireEvent.click(
      screen.getByRole("button", { name: "加入 gpt.kanglives.top 直连" }),
    );
    fireEvent.click(preview());
    await waitFor(() => expect(localRulesApi.preview).toHaveBeenCalledTimes(1));
    const policy = vi.mocked(localRulesApi.preview).mock.calls[0][0];
    expect(policy.rules[0].rule).toMatchObject({
      kind: "domain",
      value: "gpt.kanglives.top",
      target: "direct",
    });
    expect(policy.rules[1].id).toBe(local.id);
  });

  it("creates stable IDs on local HTTP without randomUUID and clears invalid no-resolve options", async () => {
    const getRandomValues = globalThis.crypto.getRandomValues.bind(
      globalThis.crypto,
    );
    const cryptoSpy = vi
      .spyOn(globalThis, "crypto", "get")
      .mockReturnValue({ getRandomValues } as Crypto);
    vi.mocked(localRulesApi.state).mockReturnValue(
      Effect.succeed(
        state({
          rules: [
            {
              ...local,
              rule: {
                kind: "ip-cidr",
                value: "192.0.2.0/24",
                target: "direct",
                index: 0,
                noResolve: true,
              },
            },
          ],
          subscriptionEdits: [],
        }),
      ),
    );
    render(<LocalRulesEditor />);
    await loaded();
    fireEvent.click(screen.getByRole("button", { name: "添加域名直连规则" }));
    const newRow = screen.getAllByRole("group", { name: /本地规则/ })[1];
    expect(newRow.getAttribute("aria-label")).toMatch(
      /^本地规则 local-[a-f0-9]{32}$/,
    );
    fireEvent.change(within(newRow).getByLabelText("匹配值"), {
      target: { value: "http.test" },
    });
    fireEvent.change(within(row(local.id)).getByLabelText("匹配类型"), {
      target: { value: "rule-set" },
    });
    fireEvent.click(preview());
    await waitFor(() => expect(localRulesApi.preview).toHaveBeenCalledTimes(1));
    expect(
      vi.mocked(localRulesApi.preview).mock.calls[0][0].rules[0].rule,
    ).toMatchObject({ kind: "rule-set", value: "cn-domain", noResolve: false });
    cryptoSpy.mockRestore();
  });

  it("all matcher kinds are explicit draft edits; labels and folded notes survive payloads", async () => {
    vi.mocked(localRulesApi.state).mockReturnValue(
      Effect.succeed(state({ rules: [local], subscriptionEdits: [] })),
    );
    render(<LocalRulesEditor />);
    await loaded();
    const values: { kind: Rule["kind"]; value: string }[] = [
      { kind: "domain", value: "exact.test" },
      { kind: "domain-suffix", value: "suffix.test" },
      { kind: "domain-keyword", value: "keyword" },
      { kind: "ip-cidr", value: "192.0.2.0/24" },
      { kind: "rule-set", value: "cn-ip" },
      { kind: "match", value: "" },
    ];
    for (const [index, item] of values.entries()) {
      await waitFor(() => expect(preview()).toBeEnabled());
      fireEvent.change(within(row(local.id)).getByLabelText("匹配类型"), {
        target: { value: item.kind },
      });
      if (item.kind !== "match")
        fireEvent.change(within(row(local.id)).getByLabelText("匹配值"), {
          target: { value: item.value },
        });
      fireEvent.click(preview());
      await waitFor(() =>
        expect(localRulesApi.preview).toHaveBeenCalledTimes(index + 1),
      );
      expect(
        vi.mocked(localRulesApi.preview).mock.calls[index][0].rules[0],
      ).toMatchObject({
        id: local.id,
        label: "已有规则",
        note: "保留备注",
        rule: { kind: item.kind, value: item.value },
      });
    }
    expect(localRulesApi.save).not.toHaveBeenCalled();
    expect(localRulesApi.apply).not.toHaveBeenCalled();
  });
  it("known mismatched revision and a newer runtime generation never show applied green", async () => {
    const saved = {
      ...state({ rules: [local], subscriptionEdits: [] }),
      applied: {
        state: "known" as const,
        revision: "older-revision",
        generation: 9,
      },
    };
    vi.mocked(localRulesApi.state).mockReturnValue(Effect.succeed(saved));
    const view = render(<LocalRulesEditor />);
    await loaded();
    expect(screen.getByTestId("rules-applied-state")).toHaveTextContent(
      "待应用",
    );
    expect(screen.getByTestId("rules-applied-state")).not.toHaveClass(
      "badge-success",
    );
    vi.mocked(localRulesApi.state).mockReturnValue(
      Effect.succeed({
        ...saved,
        applied: { state: "known", revision: "saved-revision", generation: 9 },
      }),
    );
    fireEvent.click(screen.getByRole("button", { name: "刷新规则状态" }));
    await waitFor(() =>
      expect(screen.getByTestId("rules-applied-state")).toHaveClass(
        "badge-success",
      ),
    );
    const runtimeStatus = {
      service: "sing-box" as const,
      state: "running",
      generation: 10,
      configured: true,
      artifactAvailable: true,
      rssBytes: 0,
      rssAvailable: false,
      desired: true,
      restarts: 0,
    };
    view.rerender(<LocalRulesEditor runtime={{ status: runtimeStatus }} />);
    expect(screen.getByTestId("rules-applied-state")).not.toHaveClass(
      "badge-success",
    );
    expect(screen.getByTestId("rules-applied-state")).toHaveTextContent(
      "未确认",
    );
    expect(apply()).toBeDisabled();
  });
  it("GET refresh errors invalidate old green status without discarding current inputs", async () => {
    const saved = {
      ...state({ rules: [local], subscriptionEdits: [] }),
      applied: {
        state: "known" as const,
        revision: "saved-revision",
        generation: 9,
      },
    };
    vi.mocked(localRulesApi.state)
      .mockReturnValueOnce(Effect.succeed(saved))
      .mockReturnValue(
        Effect.fail(new ApiError({ code: "read_failed", message: "读取失败" })),
      );
    render(<LocalRulesEditor />);
    await loaded();
    expect(screen.getByTestId("rules-applied-state")).toHaveClass(
      "badge-success",
    );
    fireEvent.change(screen.getByLabelText("匹配值"), {
      target: { value: "retained.test" },
    });
    fireEvent.click(screen.getByRole("button", { name: "刷新规则状态" }));
    await waitFor(() =>
      expect(screen.getByRole("alert")).toHaveTextContent("读取失败"),
    );
    expect(screen.getByDisplayValue("retained.test")).toBeVisible();
    expect(screen.getByText("草稿：未保存更改")).toBeVisible();
    expect(screen.getByTestId("rules-applied-state")).not.toHaveClass(
      "badge-success",
    );
    expect(apply()).toBeDisabled();
  });
  it("save readback mismatch retains form and dirty values", async () => {
    vi.mocked(localRulesApi.save).mockReturnValue(
      Effect.succeed(state({ rules: [local], subscriptionEdits: [] })),
    );
    render(<LocalRulesEditor />);
    await loaded();
    fireEvent.change(screen.getByLabelText("匹配值"), {
      target: { value: "my-draft.test" },
    });
    fireEvent.click(save());
    await waitFor(() =>
      expect(screen.getByRole("alert")).toHaveTextContent(
        "save_readback_mismatch",
      ),
    );
    expect(screen.getByDisplayValue("my-draft.test")).toBeVisible();
    expect(screen.getByText("草稿：未保存更改")).toBeVisible();
    expect(apply()).toBeDisabled();
    expect(localRulesApi.apply).not.toHaveBeenCalled();
  });
  it("preview and apply failures retain saved draft and never claim success", async () => {
    const saved = state({ rules: [local], subscriptionEdits: [] });
    vi.mocked(localRulesApi.state).mockReturnValue(Effect.succeed(saved));
    vi.mocked(localRulesApi.preview).mockReturnValue(
      Effect.fail(
        new ApiError({ code: "invalid_policy", message: "预览失败" }),
      ),
    );
    vi.mocked(localRulesApi.apply).mockReturnValue(
      Effect.fail(
        new ApiError({
          code: "generation_conflict",
          message: "运行版本已变化",
        }),
      ),
    );
    render(<LocalRulesEditor />);
    await loaded();
    fireEvent.click(preview());
    await waitFor(() =>
      expect(screen.getByRole("alert")).toHaveTextContent("预览失败"),
    );
    expect(screen.getByDisplayValue("existing.test")).toBeVisible();
    expect(apply()).toBeEnabled();
    fireEvent.click(apply());
    fireEvent.click(screen.getByRole("button", { name: "取消" }));
    expect(localRulesApi.apply).not.toHaveBeenCalled();
    fireEvent.click(apply());
    fireEvent.click(screen.getByRole("button", { name: "确认应用" }));
    await waitFor(() =>
      expect(screen.getByRole("alert")).toHaveTextContent(
        "generation_conflict",
      ),
    );
    expect(screen.getByDisplayValue("existing.test")).toBeVisible();
    expect(screen.getByText("草稿：已保存")).toBeVisible();
    expect(screen.getByTestId("rules-applied-state")).not.toHaveClass(
      "badge-success",
    );
    expect(localRulesApi.state).toHaveBeenCalledTimes(1);
    expect(localRulesApi.apply).toHaveBeenCalledTimes(1);
  });
  it("successful apply followed by failed GET keeps result unconfirmed", async () => {
    vi.mocked(localRulesApi.state)
      .mockReturnValueOnce(
        Effect.succeed(state({ rules: [local], subscriptionEdits: [] })),
      )
      .mockReturnValue(
        Effect.fail(new ApiError({ code: "read_failed", message: "回读失败" })),
      );
    render(<LocalRulesEditor />);
    await loaded();
    fireEvent.click(apply());
    fireEvent.click(screen.getByRole("button", { name: "确认应用" }));
    await waitFor(() =>
      expect(screen.getByRole("alert")).toHaveTextContent("回读失败"),
    );
    expect(screen.getByDisplayValue("existing.test")).toBeVisible();
    expect(screen.getByTestId("rules-applied-state")).not.toHaveClass(
      "badge-success",
    );
    expect(apply()).toBeDisabled();
  });
  it("omission acknowledgment is explicit and revision-specific", async () => {
    const saved = {
      ...state({ rules: [local], subscriptionEdits: [] }),
      policySummary: {
        total: 2,
        supported: 1,
        omitted: 1,
        reasons: [],
        omittedRules: [
          { index: 1, code: "unsupported-rule", message: "ignored" },
        ],
        revision: "review-one",
      },
    };
    vi.mocked(localRulesApi.state).mockReturnValue(Effect.succeed(saved));
    render(<LocalRulesEditor />);
    await loaded();
    expect(apply()).toBeDisabled();
    fireEvent.click(
      screen.getByRole("checkbox", { name: /我已了解.*规则将被忽略/ }),
    );
    expect(localRulesApi.apply).not.toHaveBeenCalled();
    expect(apply()).toBeEnabled();
    fireEvent.click(apply());
    fireEvent.click(screen.getByRole("button", { name: "确认应用" }));
    await waitFor(() => expect(localRulesApi.state).toHaveBeenCalledTimes(2));
    expect(localRulesApi.apply).toHaveBeenCalledExactlyOnceWith({
      revision: "saved-revision",
      generation: 9,
      acknowledgedRevision: "review-one",
    });
    await waitFor(() =>
      expect(
        screen.getByRole("checkbox", { name: /我已了解.*规则将被忽略/ }),
      ).not.toBeChecked(),
    );
    expect(apply()).toBeDisabled();
  });
  it("subscription refresh orphans edits while retaining unsaved local values", async () => {
    const edit = {
      id: "old-edit",
      sourceFingerprint,
      disabled: true,
      label: "旧规则",
      note: "",
    };
    const saved = state({ rules: [local], subscriptionEdits: [edit] });
    vi.mocked(localRulesApi.state)
      .mockReturnValueOnce(Effect.succeed(saved))
      .mockReturnValue(
        Effect.succeed({
          ...saved,
          subscriptionRevision: "subscription-two",
          subscriptionRules: [],
        }),
      );
    render(<LocalRulesEditor />);
    await loaded();
    fireEvent.change(screen.getByLabelText("匹配值"), {
      target: { value: "my-local.test" },
    });
    fireEvent.click(screen.getByRole("button", { name: "刷新规则状态" }));
    await waitFor(() =>
      expect(screen.getByText("引用已失效 · 不参与应用")).toBeVisible(),
    );
    expect(screen.getByDisplayValue("my-local.test")).toBeVisible();
    fireEvent.click(preview());
    await waitFor(() => expect(localRulesApi.preview).toHaveBeenCalledTimes(1));
    expect(
      vi.mocked(localRulesApi.preview).mock.calls[0][0].subscriptionEdits,
    ).toEqual([edit]);
  });

  it("initial GET proposes exact gpt DIRECT as visibly unsaved, without any POST", async () => {
    render(<LocalRulesEditor />);
    await loaded();
    expect(screen.getByDisplayValue("gpt.kanglives.top")).toBeVisible();
    expect(screen.getByDisplayValue("用户要求 · GPT 直连")).toBeVisible();
    expect(screen.getByText("草稿：未保存更改")).toBeVisible();
    expect(screen.getByLabelText("匹配类型")).toHaveValue("domain");
    expect(screen.getByLabelText("动作")).toHaveValue("direct");
    expect(apply()).toBeDisabled();
    expect(localRulesApi.state).toHaveBeenCalledTimes(1);
    expect(localRulesApi.save).not.toHaveBeenCalled();
    expect(localRulesApi.preview).not.toHaveBeenCalled();
    expect(localRulesApi.apply).not.toHaveBeenCalled();
  });
  it("existing rules remain untouched and the GPT shortcut is explicit", async () => {
    vi.mocked(localRulesApi.state).mockReturnValue(
      Effect.succeed(state({ rules: [local], subscriptionEdits: [] })),
    );
    render(<LocalRulesEditor />);
    await loaded();
    expect(screen.queryByDisplayValue("gpt.kanglives.top")).toBeNull();
    expect(screen.getByDisplayValue("existing.test")).toBeVisible();
    expect(save()).toBeDisabled();
    fireEvent.click(
      screen.getByRole("button", { name: "加入 gpt.kanglives.top 直连" }),
    );
    expect(screen.getByDisplayValue("gpt.kanglives.top")).toBeVisible();
    expect(localRulesApi.save).not.toHaveBeenCalled();
  });
  it("CRUD, order, enabled state and actions preserve saved subscription edits", async () => {
    const edit = {
      id: "saved-edit",
      sourceFingerprint,
      disabled: true,
      label: "已有禁用",
      note: "保留",
    };
    vi.mocked(localRulesApi.state).mockReturnValue(
      Effect.succeed(state({ rules: [local], subscriptionEdits: [edit] })),
    );
    render(<LocalRulesEditor />);
    await loaded();
    fireEvent.change(within(row(local.id)).getByLabelText("动作"), {
      target: { value: "direct" },
    });
    fireEvent.click(within(row(local.id)).getByLabelText("启用规则"));
    fireEvent.click(screen.getByRole("button", { name: "添加域名直连规则" }));
    const newRow = screen.getAllByRole("group", { name: /本地规则/ })[1];
    fireEvent.change(within(newRow).getByLabelText("匹配值"), {
      target: { value: "new.test" },
    });
    fireEvent.change(within(newRow).getByLabelText("动作"), {
      target: { value: "block" },
    });
    fireEvent.click(within(newRow).getByRole("button", { name: "上移规则" }));
    fireEvent.click(preview());
    await waitFor(() => expect(localRulesApi.preview).toHaveBeenCalledTimes(1));
    const policy = vi.mocked(localRulesApi.preview).mock.calls[0][0];
    expect(policy.subscriptionEdits).toEqual([edit]);
    expect(policy.rules.map((item) => item.rule.value)).toEqual([
      "new.test",
      "existing.test",
    ]);
    expect(policy.rules[0].rule.target).toBe("block");
    expect(policy.rules[1].enabled).toBe(false);
    fireEvent.click(
      within(row(local.id)).getByRole("button", { name: "删除规则" }),
    );
    expect(screen.queryByDisplayValue("existing.test")).toBeNull();
    expect(localRulesApi.apply).not.toHaveBeenCalled();
  });
  it("preview and save never apply; successful saved readback alone is not green", async () => {
    render(<LocalRulesEditor />);
    await loaded();
    fireEvent.click(preview());
    await waitFor(() => expect(localRulesApi.preview).toHaveBeenCalledTimes(1));
    expect(save()).toBeEnabled();
    fireEvent.click(save());
    await waitFor(() => expect(screen.getByText("草稿：已保存")).toBeVisible());
    expect(localRulesApi.save).toHaveBeenCalledTimes(1);
    expect(localRulesApi.apply).not.toHaveBeenCalled();
    expect(screen.getByTestId("rules-applied-state")).not.toHaveClass(
      "badge-success",
    );
    expect(apply()).toBeEnabled();
  });
  it("explicit apply uses exact saved revision/generation, then fresh GET establishes truth", async () => {
    const saved = state({ rules: [local], subscriptionEdits: [] });
    vi.mocked(localRulesApi.state)
      .mockReturnValueOnce(Effect.succeed(saved))
      .mockReturnValue(
        Effect.succeed({
          ...saved,
          runtimeGeneration: 10,
          applied: {
            state: "known",
            revision: "saved-revision",
            generation: 10,
          },
        }),
      );
    const runtime = { reload: vi.fn() };
    render(<LocalRulesEditor runtime={runtime} />);
    await loaded();
    fireEvent.click(apply());
    expect(localRulesApi.apply).not.toHaveBeenCalled();
    expect(screen.getByRole("dialog", { name: "确认应用规则" })).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "确认应用" }));
    await waitFor(() => expect(localRulesApi.state).toHaveBeenCalledTimes(2));
    expect(localRulesApi.apply).toHaveBeenCalledExactlyOnceWith({
      revision: "saved-revision",
      generation: 9,
    });
    await waitFor(() =>
      expect(screen.getByTestId("rules-applied-state")).toHaveClass(
        "badge-success",
      ),
    );
    expect(runtime.reload).toHaveBeenCalledTimes(1);
  });
  it("does not trust a successful apply response without verified matching GET", async () => {
    const saved = state({ rules: [local], subscriptionEdits: [] });
    vi.mocked(localRulesApi.state)
      .mockReturnValueOnce(Effect.succeed(saved))
      .mockReturnValue(
        Effect.succeed({ ...saved, applied: { state: "unknown" } }),
      );
    render(<LocalRulesEditor />);
    await loaded();
    fireEvent.click(apply());
    fireEvent.click(screen.getByRole("button", { name: "确认应用" }));
    await waitFor(() => expect(localRulesApi.state).toHaveBeenCalledTimes(2));
    expect(screen.getByTestId("rules-applied-state")).not.toHaveClass(
      "badge-success",
    );
    expect(screen.getByTestId("rules-applied-state")).toHaveTextContent(
      "未确认",
    );
  });
  it("failed save retains values and dirty state; failed apply cannot mark rules active", async () => {
    vi.mocked(localRulesApi.save).mockReturnValue(
      Effect.fail(new ApiError({ code: "save_failed", message: "保存失败" })),
    );
    render(<LocalRulesEditor />);
    await loaded();
    fireEvent.change(screen.getByLabelText("匹配值"), {
      target: { value: "edited.test" },
    });
    fireEvent.click(save());
    await waitFor(() =>
      expect(screen.getByRole("alert")).toHaveTextContent("保存失败"),
    );
    expect(screen.getByDisplayValue("edited.test")).toBeVisible();
    expect(screen.getByText("草稿：未保存更改")).toBeVisible();
    expect(apply()).toBeDisabled();
    expect(localRulesApi.apply).not.toHaveBeenCalled();
  });
  it("subscription disable and replacement target exact source fingerprint and can restore", async () => {
    vi.mocked(localRulesApi.state).mockReturnValue(
      Effect.succeed(state({ rules: [local], subscriptionEdits: [] })),
    );
    render(<LocalRulesEditor />);
    await loaded();
    fireEvent.click(screen.getByRole("button", { name: "禁用订阅规则" }));
    fireEvent.click(preview());
    await waitFor(() => expect(localRulesApi.preview).toHaveBeenCalledTimes(1));
    expect(
      vi.mocked(localRulesApi.preview).mock.calls[0][0].subscriptionEdits[0],
    ).toMatchObject({ sourceFingerprint, disabled: true });
    fireEvent.click(screen.getByRole("button", { name: "恢复订阅规则" }));
    fireEvent.click(screen.getByRole("button", { name: "改写订阅规则" }));
    const editor = screen.getByRole("group", { name: "订阅改写" });
    fireEvent.change(within(editor).getByLabelText("匹配类型"), {
      target: { value: "domain" },
    });
    fireEvent.change(within(editor).getByLabelText("匹配值"), {
      target: { value: "replacement.test" },
    });
    fireEvent.change(within(editor).getByLabelText("动作"), {
      target: { value: "direct" },
    });
    fireEvent.click(screen.getByRole("button", { name: "加入改写草稿" }));
    fireEvent.click(preview());
    await waitFor(() => expect(localRulesApi.preview).toHaveBeenCalledTimes(2));
    expect(
      vi.mocked(localRulesApi.preview).mock.calls[1][0].subscriptionEdits[0],
    ).toMatchObject({
      sourceFingerprint,
      disabled: false,
      replacement: {
        kind: "domain",
        value: "replacement.test",
        target: "direct",
        index: 27,
      },
    });
    expect(localRulesApi.apply).not.toHaveBeenCalled();
  });
  it("orphan edits remain visible inactive after refresh; matcher/action/fingerprint search is bounded", async () => {
    const orphan = {
      id: "orphan",
      sourceFingerprint: `${"b".repeat(64)}:1`,
      disabled: true,
      label: "旧订阅编辑",
      note: "",
    };
    const value: LocalRulesState = {
      ...state({ rules: [local], subscriptionEdits: [orphan] }),
      subscriptionRules: Array.from({ length: 42 }, (_, index) => ({
        fingerprint: `${"a".repeat(64)}:${index + 1}`,
        rule: { ...source, value: `domain-${index}.test` },
      })),
    };
    vi.mocked(localRulesApi.state).mockReturnValue(Effect.succeed(value));
    render(<LocalRulesEditor />);
    await loaded();
    expect(screen.getByText("引用已失效 · 不参与应用")).toBeVisible();
    expect(
      within(screen.getByRole("table", { name: "订阅规则" })).getAllByRole(
        "row",
      ),
    ).toHaveLength(21);
    fireEvent.change(screen.getByLabelText("搜索订阅规则"), {
      target: { value: "domain-41.test proxy" },
    });
    expect(
      within(screen.getByRole("table", { name: "订阅规则" })).getAllByRole(
        "row",
      ),
    ).toHaveLength(2);
    expect(screen.getByText("domain-41.test")).toBeVisible();
  });
});

describe("local rules gateway access", () => {
  it("keeps default gateway focus, opens custom rules from direct button, and retains draft across tabs", async () => {
    render(<ProxyPage />);
    expect(screen.getByRole("region", { name: "正常网关状态" })).toBeVisible();
    expect(
      screen.getByText("高级设置与诊断").closest("details"),
    ).not.toHaveAttribute("open");
    expect(
      screen.queryByRole("region", { name: "自定义规则编辑器" }),
    ).toBeNull();
    expect(localRulesApi.state).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "编辑自定义规则" }));
    await loaded();
    expect(screen.getByRole("region", { name: "正常网关状态" })).toBeVisible();
    expect(screen.getByRole("tab", { name: "自定义规则" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    fireEvent.change(screen.getByLabelText("匹配值"), {
      target: { value: "tab-draft.test" },
    });
    fireEvent.click(screen.getByRole("tab", { name: "节点" }));
    expect(screen.getByRole("region", { name: "节点默认视图" })).toBeVisible();
    fireEvent.click(screen.getByRole("tab", { name: "自定义规则" }));
    expect(screen.getByDisplayValue("tab-draft.test")).toBeVisible();
    expect(localRulesApi.state).toHaveBeenCalledTimes(1);
    expect(localRulesApi.save).not.toHaveBeenCalled();
    expect(localRulesApi.apply).not.toHaveBeenCalled();
  });
});
