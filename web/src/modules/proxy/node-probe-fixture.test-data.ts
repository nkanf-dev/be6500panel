import {
  NODE_PROBE_TARGET,
  type NodeProbeSnapshot,
} from "./node-probe-contracts";

export const nodeProbeSnapshotFixture: NodeProbeSnapshot = {
  revision: "nodes-revision-1",
  available: true,
  target: NODE_PROBE_TARGET,
  running: false,
  results: [
    {
      nodeId: "node-1",
      status: "success",
      delayMs: 68,
      measuredAt: "2026-10-03T04:00:02Z",
      target: NODE_PROBE_TARGET,
    },
  ],
  limits: { maxNodes: 256, concurrency: 1, timeoutMs: 3000 },
};
export const activeNodeProbeFixture: NodeProbeSnapshot = {
  ...nodeProbeSnapshotFixture,
  running: true,
  job: {
    id: "job-1",
    status: "running",
    total: 2,
    completed: 1,
    startedAt: "2026-10-03T04:00:00Z",
  },
  results: [
    ...nodeProbeSnapshotFixture.results,
    { nodeId: "node-2", status: "probing", target: NODE_PROBE_TARGET },
  ],
};
