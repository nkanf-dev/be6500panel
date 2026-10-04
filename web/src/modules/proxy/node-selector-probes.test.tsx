import { Effect } from "effect";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { api } from "../../lib/api";
import type { ProxyNodes } from "../../lib/contracts";
import { runtimeStatus } from "../production-fixtures.test-data";
import type { RuntimeController } from "../runtime/use-runtime";
import { nodeProbeApi } from "./node-probe-api";
import {
  NODE_PROBE_TARGET,
  type NodeProbeSnapshot,
} from "./node-probe-contracts";
import { NodeSelector } from "./node-selector";

const nodes: ProxyNodes & { revision: string } = {
  revision: "import-one",
  selectedNodeId: "node-1",
  diagnostics: [],
  nodes: Array.from({ length: 220 }, (_, index) => ({
    id: `node-${index + 1}`,
    label: `Probe Node ${index + 1}`,
    server: `edge-${index + 1}.example.test`,
    port: 443,
    protocol: "vless",
    transport: "tcp",
    reality: true,
    vision: true,
    utls: true,
    udp: true,
  })),
};
const idle: NodeProbeSnapshot = {
  revision: "import-one",
  available: true,
  target: NODE_PROBE_TARGET,
  running: false,
  results: [],
  limits: { maxNodes: 256, concurrency: 1, timeoutMs: 3000 },
};
const active: NodeProbeSnapshot = {
  ...idle,
  running: true,
  job: {
    id: "job",
    status: "running",
    total: 220,
    completed: 45,
    startedAt: "2026-10-03T05:00:00Z",
  },
  results: [
    {
      nodeId: "node-2",
      status: "success",
      delayMs: 185,
      measuredAt: "2026-10-03T05:00:01Z",
      target: NODE_PROBE_TARGET,
    },
  ],
};
function setup() {
  const controller: RuntimeController = {
    service: "sing-box",
    status: { ...runtimeStatus, configured: false },
    enabled: true,
    loading: false,
    pending: false,
    error: undefined,
    result: undefined,
    refresh: vi.fn(),
    run: vi.fn(),
  };
  const onSelected = vi.fn();
  return {
    ...render(
      <NodeSelector
        nodes={nodes}
        runtime={controller}
        onSelected={onSelected}
      />,
    ),
    controller,
    onSelected,
  };
}
beforeEach(() => {
  window.localStorage.clear();
  window.sessionStorage.clear();
  let current = idle;
  vi.spyOn(nodeProbeApi, "snapshot").mockImplementation(() =>
    Effect.succeed(current),
  );
  vi.spyOn(nodeProbeApi, "start").mockImplementation((input) => {
    current = {
      ...active,
      job: {
        ...active.job!,
        total: input.all ? 220 : input.nodeIds.length,
        completed: 0,
      },
    };
    return Effect.succeed(current);
  });
  vi.spyOn(nodeProbeApi, "stop").mockImplementation(() => {
    current = {
      ...active,
      running: false,
      job: {
        ...active.job!,
        status: "cancelled",
        completed: 220,
        finishedAt: "2026-10-03T05:00:02Z",
      },
    };
    return Effect.succeed(current);
  });
  vi.spyOn(api, "proxySelect").mockReturnValue(
    Effect.succeed({
      status: runtimeStatus,
      configSHA256: "a".repeat(64),
      diagnostics: [],
    }),
  );
});
describe("one-click node probes are independent of selection", () => {
  it("mount reads only; probes only20 current-page IDs and does not save or change selected node", async () => {
    const user = userEvent.setup();
    const view = setup();
    const button = screen.getByRole("button", { name: "测速当前页 (20)" });
    await waitFor(() => expect(button).toBeEnabled());
    expect(nodeProbeApi.start).not.toHaveBeenCalled();
    await user.click(screen.getByRole("button", { name: "下一页" }));
    await user.click(button);
    expect(nodeProbeApi.start).toHaveBeenCalledWith({
      all: false,
      nodeIds: Array.from({ length: 20 }, (_, index) => `node-${index + 21}`),
      revision: "import-one",
    });
    expect(screen.getByText("与当前配置一致")).toBeInTheDocument();
    expect(api.proxySelect).not.toHaveBeenCalled();
    expect(view.controller.run).not.toHaveBeenCalled();
    expect(view.onSelected).not.toHaveBeenCalled();
  });
  it("single node badge never selects/applies, and all220/Stop use explicit job API", async () => {
    const user = userEvent.setup();
    const view = setup();
    const single = screen.getByRole("button", {
      name: "测速节点 Probe Node 2：-- ms",
    });
    await waitFor(() => expect(single).toBeEnabled());
    await user.click(single);
    expect(nodeProbeApi.start).toHaveBeenLastCalledWith({
      all: false,
      nodeIds: ["node-2"],
      revision: "import-one",
    });
    expect(
      screen.getByRole("button", { name: "选择节点 Probe Node 1" }),
    ).toHaveAttribute("aria-pressed", "true");
    expect(
      screen.getByRole("button", { name: "选择节点 Probe Node 2" }),
    ).toHaveAttribute("aria-pressed", "false");
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "停止测速" })).toBeEnabled(),
    );
    await user.click(screen.getByRole("button", { name: "停止测速" }));
    expect(nodeProbeApi.stop).toHaveBeenCalledOnce();
    const all = screen.getByRole("button", { name: "测速全部 (220)" });
    await waitFor(() => expect(all).toBeEnabled());
    await user.click(all);
    expect(nodeProbeApi.start).toHaveBeenLastCalledWith({
      all: true,
      nodeIds: [],
      revision: "import-one",
    });
    expect(api.proxySelect).not.toHaveBeenCalled();
    expect(view.controller.run).not.toHaveBeenCalled();
  });
  it("renders stored185ms with fixed target and timestamp; a changed import hides it and never auto-probes", async () => {
    vi.mocked(nodeProbeApi.snapshot).mockReturnValue(
      Effect.succeed({
        ...active,
        running: false,
        job: { ...active.job!, status: "completed", completed: 220 },
      }),
    );
    const view = setup();
    const badge = await screen.findByRole("button", {
      name: "测速节点 Probe Node 2：185 ms",
    });
    expect(badge).toHaveAttribute(
      "title",
      expect.stringContaining(NODE_PROBE_TARGET),
    );
    expect(badge).toHaveAttribute(
      "title",
      expect.stringContaining("2026-10-03T05:00:01Z"),
    );
    view.rerender(
      <NodeSelector
        nodes={{ ...nodes, revision: "import-two" }}
        runtime={view.controller}
        onSelected={view.onSelected}
      />,
    );
    expect(
      screen.getByRole("button", { name: "测速节点 Probe Node 2：-- ms" }),
    ).toBeDisabled();
    expect(nodeProbeApi.start).not.toHaveBeenCalled();
    expect(nodeProbeApi.stop).not.toHaveBeenCalled();
  });
});

it("shows the real current-node delay and starts only that node without changing selection or saving", async () => {
  vi.mocked(nodeProbeApi.snapshot).mockReturnValue(
    Effect.succeed({
      ...idle,
      results: [
        {
          nodeId: "node-1",
          status: "success",
          delayMs: 185,
          measuredAt: "2026-10-03T05:00:01Z",
          target: NODE_PROBE_TARGET,
        },
      ],
    }),
  );
  const view = setup();
  const button = screen.getByRole("button", { name: "测试当前延迟" });
  await waitFor(() => expect(button).toBeEnabled());
  expect(screen.getByText(/当前延迟：185 ms/)).toBeInTheDocument();
  expect(nodeProbeApi.start).not.toHaveBeenCalled();
  await userEvent.setup().click(button);
  expect(nodeProbeApi.start).toHaveBeenCalledExactlyOnceWith({
    all: false,
    nodeIds: ["node-1"],
    revision: "import-one",
  });
  expect(api.proxySelect).not.toHaveBeenCalled();
  expect(view.controller.run).not.toHaveBeenCalled();
  expect(view.onSelected).not.toHaveBeenCalled();
  expect(
    screen.getByRole("button", { name: "选择节点 Probe Node 1" }),
  ).toHaveAttribute("aria-pressed", "true");
});
