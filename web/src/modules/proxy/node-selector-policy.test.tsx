import { Effect } from "effect";
import {
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { api } from "../../lib/api";
import type { ProxyNodes } from "../../lib/contracts";
import { runtimeStatus } from "../production-fixtures.test-data";
import type { RuntimeController } from "../runtime/use-runtime";
import { NodeSelector } from "./node-selector";
import type { ProxyPolicySummary } from "./policy-contracts";

const summary: ProxyPolicySummary = {
  total: 4,
  supported: 2,
  omitted: 2,
  reasons: [
    {
      code: "unsupported-process-rule",
      count: 1,
      message: "process rules cannot classify forwarded LAN clients",
    },
    { code: "unsupported-rule", count: 1, message: "unsupported rule type" },
  ],
  omittedRules: [
    {
      index: 1,
      code: "unsupported-process-rule",
      message: "process rules cannot classify forwarded LAN clients",
    },
    { index: 2, code: "unsupported-rule", message: "unsupported rule type" },
  ],
  revision: "a".repeat(64),
};
const nodes: ProxyNodes & { policySummary?: ProxyPolicySummary } = {
  selectedNodeId: "first",
  diagnostics: [],
  policySummary: summary,
  nodes: ["first", "second"].map((id) => ({
    id,
    label: `Test ${id}`,
    server: `${id}.private.example.test`,
    port: 443,
    protocol: "vless",
    transport: "tcp",
    reality: false,
    vision: false,
    utls: false,
    udp: false,
  })),
};
const acknowledgment = () =>
  screen.getByRole("checkbox", { name: /我已了解.*规则将被忽略/ });
const apply = () => screen.getByRole("button", { name: "保存配置" });
function setup(input = nodes) {
  const runtime: RuntimeController = {
    service: "sing-box",
    status: { ...runtimeStatus, state: "running", configured: false },
    enabled: true,
    pending: false,
    loading: false,
    error: undefined,
    result: undefined,
    refresh: vi.fn(),
    run: vi.fn(async (load) => {
      await Effect.runPromise(load());
      return true;
    }),
  };
  const selected = vi.fn();
  const view = render(
    <NodeSelector nodes={input} runtime={runtime} onSelected={selected} />,
  );
  return {
    ...view,
    runtime,
    selected,
    rerenderNodes: (value: typeof input) =>
      view.rerender(
        <NodeSelector nodes={value} runtime={runtime} onSelected={selected} />,
      ),
  };
}
beforeEach(() => {
  window.localStorage.clear();
  window.sessionStorage.clear();
  vi.spyOn(api, "proxySelect").mockReturnValue(
    Effect.succeed({
      status: { ...runtimeStatus, state: "running" },
      configSHA256: "b".repeat(64),
      diagnostics: [],
    }),
  );
});

describe("explicit current policy acknowledgment", () => {
  it("shows eligible rule counts before Apply and blocks unacknowledged form submission", () => {
    setup();
    const review = screen.getByRole("region", { name: "路由规则审阅" });
    expect(
      within(review).getByText("共 4 条规则 · 2 条可应用路由规则"),
    ).toBeInTheDocument();
    expect(within(review).getByText("2 条规则将忽略")).toBeInTheDocument();
    expect(
      within(review).queryByText(/已生效|命中次数/),
    ).not.toBeInTheDocument();
    expect(
      review.compareDocumentPosition(apply()) &
        Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    expect(acknowledgment()).not.toBeChecked();
    expect(apply()).toBeDisabled();
    fireEvent.submit(apply().closest("form")!);
    expect(api.proxySelect).not.toHaveBeenCalled();
  });

  it("selecting nodes, changing filters and checking acknowledgment never POST; explicit Apply carries the current revision", async () => {
    const user = userEvent.setup();
    const view = setup();
    await user.click(
      screen.getByRole("button", { name: "选择节点 Test second" }),
    );
    await user.type(screen.getByLabelText("搜索节点"), "first");
    await user.selectOptions(screen.getByLabelText("协议筛选"), "vless");
    await user.click(acknowledgment());
    expect(api.proxySelect).not.toHaveBeenCalled();
    expect(view.runtime.run).not.toHaveBeenCalled();
    expect(apply()).toBeEnabled();
    await user.click(apply());
    expect(api.proxySelect).toHaveBeenCalledExactlyOnceWith({
      nodeId: "second",
      ipv6: "direct",
      failure: "direct",
      datapath: "routed-tun",
      routedTUN: { interfaceName: "b6p-tun", address: "172.31.255.253/30" },
      ports: { mixed: 2080, tproxy: 7893, dns: 6450 },
      acknowledgedRevision: summary.revision,
    });
    expect(view.selected).toHaveBeenCalledOnce();
  });

  it("unchecking acknowledgment closes the gate without sending a request", async () => {
    const user = userEvent.setup();
    setup();
    await user.click(acknowledgment());
    expect(apply()).toBeEnabled();
    await user.click(acknowledgment());
    expect(apply()).toBeDisabled();
    fireEvent.submit(apply().closest("form")!);
    expect(api.proxySelect).not.toHaveBeenCalled();
  });

  it("preserves review for unchanged revisions but invalidates it when the policy changes", async () => {
    const user = userEvent.setup();
    const view = setup();
    await user.click(acknowledgment());
    view.rerenderNodes({ ...nodes, policySummary: { ...summary } });
    expect(acknowledgment()).toBeChecked();
    view.rerenderNodes({
      ...nodes,
      policySummary: { ...summary, revision: "c".repeat(64) },
    });
    expect(acknowledgment()).not.toBeChecked();
    expect(apply()).toBeDisabled();
    fireEvent.submit(apply().closest("form")!);
    expect(api.proxySelect).not.toHaveBeenCalled();
    await user.click(acknowledgment());
    await user.click(apply());
    expect(api.proxySelect).toHaveBeenCalledWith(
      expect.objectContaining({ acknowledgedRevision: "c".repeat(64) }),
    );
  });

  it("clears review when the summary disappears, so an old revision cannot revive checked state", async () => {
    const user = userEvent.setup();
    const view = setup();
    await user.click(acknowledgment());
    view.rerenderNodes({ ...nodes, policySummary: undefined });
    expect(screen.getByText("路由规则统计未知")).toBeInTheDocument();
    expect(
      screen.queryByRole("checkbox", { name: /我已了解/ }),
    ).not.toBeInTheDocument();
    await user.click(apply());
    expect(api.proxySelect).toHaveBeenCalledExactlyOnceWith(
      expect.not.objectContaining({ acknowledgedRevision: expect.anything() }),
    );
    view.rerenderNodes(nodes);
    expect(acknowledgment()).not.toBeChecked();
    expect(apply()).toBeDisabled();
  });

  it("does not persist review, rule details, private endpoints or subscription contents in browser storage", async () => {
    const user = userEvent.setup();
    const view = setup();
    await user.click(
      screen.getByRole("button", { name: "选择节点 Test second" }),
    );
    await user.click(
      screen.getByRole("button", { name: "收藏节点 Test second" }),
    );
    await user.click(acknowledgment());
    for (const storage of [window.localStorage, window.sessionStorage]) {
      const values = Array.from({ length: storage.length }, (_, index) =>
        storage.getItem(storage.key(index)!),
      ).join(" ");
      expect(values).not.toMatch(
        /acknowledged|policySummary|omittedRules|unsupported-process-rule|private\.example\.test/,
      );
      expect(values).not.toContain(summary.revision);
    }
    view.unmount();
    setup();
    await waitFor(() => expect(acknowledgment()).not.toBeChecked());
    expect(apply()).toBeDisabled();
    expect(api.proxySelect).not.toHaveBeenCalled();
  });

  it("allows a fully supported policy without a checkbox and does not fabricate acknowledgment", async () => {
    const user = userEvent.setup();
    setup({
      ...nodes,
      policySummary: {
        ...summary,
        total: 2,
        supported: 2,
        omitted: 0,
        reasons: [],
        omittedRules: [],
      },
    });
    expect(
      screen.queryByRole("checkbox", { name: /我已了解/ }),
    ).not.toBeInTheDocument();
    expect(apply()).toBeEnabled();
    await user.click(apply());
    expect(api.proxySelect).toHaveBeenCalledExactlyOnceWith({
      nodeId: "first",
      ipv6: "direct",
      failure: "direct",
      datapath: "routed-tun",
      routedTUN: { interfaceName: "b6p-tun", address: "172.31.255.253/30" },
      ports: { mixed: 2080, tproxy: 7893, dns: 6450 },
    });
  });

  it("keeps the acknowledgment disabled during an import", () => {
    const view = setup();
    view.rerender(
      <NodeSelector
        nodes={nodes}
        runtime={view.runtime}
        onSelected={view.selected}
        importing
      />,
    );
    expect(acknowledgment()).toBeDisabled();
    expect(apply()).toBeDisabled();
  });
});
