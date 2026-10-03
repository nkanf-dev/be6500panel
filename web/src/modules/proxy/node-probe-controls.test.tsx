import { fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ApiError } from "../../lib/api";
import {
  NODE_PROBE_TARGET,
  type NodeProbeResult,
} from "./node-probe-contracts";
import { NodeProbeBadge, NodeProbeControls } from "./node-probe-controls";
import {
  activeNodeProbeFixture,
  nodeProbeSnapshotFixture,
} from "./node-probe-fixture.test-data";
import type { NodeProbeController } from "./use-node-probes";

function controller(
  overrides: Partial<NodeProbeController> = {},
): NodeProbeController {
  return {
    snapshot: nodeProbeSnapshotFixture,
    results: nodeProbeSnapshotFixture.results,
    stale: false,
    error: undefined,
    loading: false,
    pending: undefined,
    starting: undefined,
    running: false,
    canStart: true,
    progress: { completed: 0, total: 0 },
    refresh: vi.fn(),
    start: vi.fn().mockResolvedValue(undefined),
    stop: vi.fn().mockResolvedValue(undefined),
    ...overrides,
  };
}
afterEach(() => vi.clearAllMocks());

describe("explicit batch node probe controls", () => {
  it("starts only on click, uses page IDs or all flag, and never submits or selects a node", () => {
    const probes = controller();
    const select = vi.fn();
    const submit = vi.fn();
    render(
      <form onSubmit={submit}>
        <div onClick={select}>
          <NodeProbeControls
            controller={probes}
            pageNodeIds={["node-1", "node-2", "node-1"]}
            allCount={220}
          />
        </div>
      </form>,
    );
    expect(probes.start).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "测速当前页 (2)" }));
    expect(probes.start).toHaveBeenLastCalledWith({
      all: false,
      nodeIds: ["node-1", "node-2"],
    });
    fireEvent.click(screen.getByRole("button", { name: "测速全部 (220)" }));
    expect(probes.start).toHaveBeenLastCalledWith({ all: true, nodeIds: [] });
    expect(select).not.toHaveBeenCalled();
    expect(submit).not.toHaveBeenCalled();
  });
  it("refreshes passive status only, without bubbling or starting", () => {
    const probes = controller();
    const select = vi.fn();
    render(
      <div onClick={select}>
        <NodeProbeControls
          controller={probes}
          pageNodeIds={["node-1"]}
          allCount={2}
        />
      </div>,
    );
    fireEvent.click(screen.getByRole("button", { name: "刷新测速状态" }));
    expect(probes.refresh).toHaveBeenCalledTimes(1);
    expect(probes.start).not.toHaveBeenCalled();
    expect(probes.stop).not.toHaveBeenCalled();
    expect(select).not.toHaveBeenCalled();
  });
  it("shows progress and explicit stop while running even when node selection is disabled or stale", () => {
    const probes = controller({
      snapshot: activeNodeProbeFixture,
      running: true,
      canStart: false,
      stale: true,
      progress: { completed: 45, total: 220 },
    });
    const select = vi.fn();
    render(
      <div onClick={select}>
        <NodeProbeControls
          controller={probes}
          pageNodeIds={["node-1"]}
          allCount={220}
          disabled
        />
      </div>,
    );
    expect(screen.getByText("正在测速 45/220…")).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: /测速当前页/ }),
    ).not.toBeInTheDocument();
    const stop = screen.getByRole("button", { name: "停止测速" });
    expect(stop).toBeEnabled();
    expect(stop).toHaveClass("node-probe-stop");
    fireEvent.click(stop);
    expect(probes.stop).toHaveBeenCalledTimes(1);
    expect(probes.start).not.toHaveBeenCalled();
    expect(select).not.toHaveBeenCalled();
  });
  it("indicates a pending all-node start without showing the prior job progress and blocks double stop", () => {
    const probes = controller({
      running: true,
      canStart: false,
      pending: "start",
      starting: { all: true, nodeIds: [] },
      progress: { completed: 0, total: 0 },
    });
    const view = render(
      <NodeProbeControls
        controller={probes}
        pageNodeIds={["node-1"]}
        allCount={220}
      />,
    );
    expect(screen.getByText("正在测速 0/220…")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "停止测速" })).toBeDisabled();
    view.rerender(
      <NodeProbeControls
        controller={{ ...probes, pending: "stop" }}
        pageNodeIds={["node-1"]}
        allCount={220}
      />,
    );
    expect(screen.getByText("正在停止测速…")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "停止测速" }));
    expect(probes.stop).not.toHaveBeenCalled();
  });
  it("explains fixed target, isolation and real serial resource limits", () => {
    render(
      <NodeProbeControls
        controller={controller()}
        pageNodeIds={["node-1"]}
        allCount={220}
      />,
    );
    expect(
      screen.getByText(/独立临时核心，不切换当前节点，也不修改已保存配置/),
    ).toHaveTextContent(NODE_PROBE_TARGET);
    expect(screen.getByText(/并发 1，每节点最长 3000 ms/)).toBeInTheDocument();
  });
  it("disables all beyond 256 and directs users to the current page", () => {
    render(
      <NodeProbeControls
        controller={controller()}
        pageNodeIds={["node-1"]}
        allCount={300}
      />,
    );
    expect(
      screen.getByRole("button", { name: "测速全部 (300)" }),
    ).toBeDisabled();
    expect(
      screen.getByRole("button", { name: "测速当前页 (1)" }),
    ).toBeEnabled();
    expect(screen.getByText(/每次最多测速 256 个节点/)).toHaveTextContent(
      "测速当前页",
    );
  });
  it("disables empty current-page actions while preserving all-node probing", () => {
    render(
      <NodeProbeControls
        controller={controller()}
        pageNodeIds={[]}
        allCount={220}
      />,
    );
    expect(
      screen.getByRole("button", { name: "测速当前页 (0)" }),
    ).toBeDisabled();
    expect(
      screen.getByRole("button", { name: "测速全部 (220)" }),
    ).toBeEnabled();
    expect(screen.getByText(/当前页没有节点/)).toBeInTheDocument();
  });
  it.each([
    ["empty_subscription", "暂无节点。请先导入订阅，再手动测速。", 0],
    [
      "artifact_unavailable",
      "测速需要 sing-box 运行文件。请先在运行管理中获取运行文件。",
      2,
    ],
    ["probes_unavailable", "节点测速暂不可用。请检查控制服务与诊断日志。", 2],
  ])(
    "explains unavailable %s without offering sample results",
    (code, text, count) => {
      const probes = controller({
        snapshot: {
          ...nodeProbeSnapshotFixture,
          available: false,
          unavailableCode: code as string,
          results: [],
        },
        canStart: false,
        results: [],
      });
      render(
        <NodeProbeControls
          controller={probes}
          pageNodeIds={[]}
          allCount={count as number}
        />,
      );
      expect(screen.getByText(text as string)).toBeInTheDocument();
      expect(screen.getByRole("button", { name: /测速全部/ })).toBeDisabled();
    },
  );
  it("shows revision-change guidance and errors with a passive refresh action", () => {
    const probes = controller({
      stale: true,
      results: [],
      canStart: false,
      error: new ApiError({ code: "revision_mismatch", message: "stale" }),
    });
    render(
      <NodeProbeControls
        controller={probes}
        pageNodeIds={["node-1"]}
        allCount={2}
      />,
    );
    expect(screen.getByText(/旧测速结果已隐藏/)).toBeInTheDocument();
    expect(screen.getByRole("alert")).toHaveTextContent("revision_mismatch");
    expect(
      screen.getByRole("button", { name: "测速当前页 (1)" }),
    ).toBeDisabled();
    fireEvent.click(screen.getByRole("button", { name: "刷新测速状态" }));
    expect(probes.refresh).toHaveBeenCalledTimes(1);
  });
  it.each([
    ["failed", "core_unavailable", "临时测速核心未能启动"],
    ["cancelled", "", "测速已停止"],
    ["invalidated", "", "订阅已变化，本次测速已失效"],
  ] as const)("explains terminal %s jobs", (status, errorCode, text) => {
    const probes = controller({
      snapshot: {
        ...nodeProbeSnapshotFixture,
        job: {
          ...activeNodeProbeFixture.job!,
          status,
          errorCode,
        },
      },
    });
    render(
      <NodeProbeControls
        controller={probes}
        pageNodeIds={["node-1"]}
        allCount={2}
      />,
    );
    expect(screen.getByText(new RegExp(text))).toBeInTheDocument();
  });
});

describe("isolated single-node latency badges", () => {
  it("uses -- ms for unknown and explicitly probes only this node without selecting or submitting", () => {
    const probes = controller({ results: [] });
    const select = vi.fn();
    const submit = vi.fn();
    const keySelect = vi.fn();
    render(
      <form onSubmit={submit}>
        <div onClick={select} onKeyDown={keySelect} onKeyUp={keySelect}>
          <NodeProbeBadge
            controller={probes}
            nodeId="node-2"
            nodeLabel="香港 2"
          />
        </div>
      </form>,
    );
    const badge = screen.getByRole("button", {
      name: "测速节点 香港 2：-- ms",
    });
    expect(badge).toHaveTextContent("-- ms");
    expect(badge).toHaveAttribute("type", "button");
    expect(badge.title).toContain("单独测速此节点");
    expect(probes.start).not.toHaveBeenCalled();
    fireEvent.keyDown(badge, { key: "Enter" });
    fireEvent.keyUp(badge, { key: "Enter" });
    fireEvent.click(badge);
    expect(probes.start).toHaveBeenCalledWith({
      all: false,
      nodeIds: ["node-2"],
    });
    expect(select).not.toHaveBeenCalled();
    expect(keySelect).not.toHaveBeenCalled();
    expect(submit).not.toHaveBeenCalled();
  });
  it.each([
    [0, "fast"],
    [68, "fast"],
    [119.9, "fast"],
    [120, "medium"],
    [185, "medium"],
    [300, "medium"],
    [300.1, "slow"],
    [450, "slow"],
  ])("colors successful %s ms by exact thresholds", (delayMs, tone) => {
    const probes = controller({
      results: [{ ...nodeProbeSnapshotFixture.results[0], delayMs }],
    });
    render(
      <NodeProbeBadge controller={probes} nodeId="node-1" nodeLabel="节点 1" />,
    );
    const badge = screen.getByRole("button", {
      name: new RegExp(`测速节点 节点 1：${Math.round(delayMs)} ms`),
    });
    expect(badge).toHaveClass(`node-probe-badge-${tone}`);
    expect(badge.title).toContain(NODE_PROBE_TARGET);
    expect(badge.title).toContain("2026-10-03T04:00:02Z");
    expect(badge.title).toContain("测试时间：");
  });
  it.each([
    ["queued", "等待测速…", "pending"],
    ["probing", "测速中…", "pending"],
    ["timeout", "超时", "failure"],
    ["unreachable", "不可达", "failure"],
    ["cancelled", "-- ms", "unknown"],
    ["success", "-- ms", "unknown"],
  ] as const)(
    "shows %s honestly without fabricated zero delay",
    (status, text, tone) => {
      const result: NodeProbeResult = {
        nodeId: "node-1",
        status,
        target: NODE_PROBE_TARGET,
      };
      render(
        <NodeProbeBadge
          controller={controller({ results: [result] })}
          nodeId="node-1"
          nodeLabel="节点 1"
        />,
      );
      const badge = screen.getByRole("button", {
        name: `测速节点 节点 1：${text}`,
      });
      expect(badge).toHaveClass(`node-probe-badge-${tone}`);
      expect(badge).not.toHaveTextContent("0 ms");
      expect(badge).toHaveAttribute(
        "aria-busy",
        String(status === "queued" || status === "probing"),
      );
    },
  );
  it("immediately marks only targeted rows busy during an explicit POST", () => {
    const probes = controller({
      pending: "start",
      starting: { all: false, nodeIds: ["node-2"] },
      running: true,
      canStart: false,
      results: [],
    });
    render(
      <>
        <NodeProbeBadge
          controller={probes}
          nodeId="node-1"
          nodeLabel="节点 1"
        />
        <NodeProbeBadge
          controller={probes}
          nodeId="node-2"
          nodeLabel="节点 2"
        />
      </>,
    );
    expect(
      screen.getByRole("button", { name: "测速节点 节点 1：-- ms" }),
    ).toHaveAttribute("aria-busy", "false");
    expect(
      screen.getByRole("button", { name: "测速节点 节点 2：测速中…" }),
    ).toHaveAttribute("aria-busy", "true");
    expect(
      screen.getByRole("button", { name: "测速节点 节点 2：测速中…" }),
    ).toBeDisabled();
  });
  it("hides stale results and respects disabled actions", () => {
    const probes = controller({ results: [], stale: true, canStart: false });
    render(
      <NodeProbeBadge
        controller={probes}
        nodeId="node-1"
        nodeLabel="节点 1"
        disabled
      />,
    );
    const badge = screen.getByRole("button", {
      name: "测速节点 节点 1：-- ms",
    });
    expect(badge).toBeDisabled();
    fireEvent.click(badge);
    expect(probes.start).not.toHaveBeenCalled();
  });
});
