import type { Page } from "@playwright/test";
import { Schema } from "effect";
import { ProxyNodesSchema, RuntimeConfigSchema } from "../src/lib/contracts";
import { nodeConfigInputs } from "../src/modules/proxy/node-selector-config";
import {
  NODE_PROBE_TARGET,
  NodeProbeRunInputSchema,
  NodeProbeSnapshotSchema,
  type NodeProbeResult,
  type NodeProbeRunInput,
} from "../src/modules/proxy/node-probe-contracts";
import { createMaturityFixture } from "./maturity-fixtures";

/** Synthetic UI evidence only. No core, network measurement, router or credentials. */
export const PROBE_NODE_COUNT = 220;
export const PROBE_PATH = "/api/proxy/node-probes";
export const PROBE_REVISION = "fixture-node-probes-revision-1";
export const PROBE_MEASURED_AT = "2026-10-03T04:00:01.000Z";
export const probeNodeID = (number: number) =>
  `probe-node-${String(number).padStart(3, "0")}`;
export const probeNodeLabel = (number: number) =>
  `fixture Probe Node ${String(number).padStart(3, "0")}`;
const now = Date.parse("2026-10-03T04:00:00.000Z");
const iso = (time: number) => new Date(time).toISOString();
const checked = <A, I>(schema: Schema.Schema<A, I>, value: unknown): A =>
  Schema.decodeUnknownSync(schema, { onExcessProperty: "error" })(value);
const assert = (condition: boolean, message: string) => {
  if (!condition) throw new Error(message);
};
const nativeConfig = JSON.stringify({
  inbounds: [
    {
      tag: "mixed-in",
      type: "mixed",
      listen: "192.168.31.1",
      listen_port: 2080,
    },
    {
      tag: "tproxy-in",
      type: "tproxy",
      listen: "127.0.0.1",
      listen_port: 7893,
    },
    {
      tag: "dns-in",
      type: "direct",
      listen: "192.168.31.1",
      listen_port: 6450,
    },
  ],
  route: { rules: [{ ip_version: 6, outbound: "direct" }] },
});
export type ProbeWrite = {
  method: string;
  path: string;
  raw: string | null;
  body: unknown;
};
export const expectedProbeStart = (input: NodeProbeRunInput): ProbeWrite => ({
  method: "POST",
  path: PROBE_PATH,
  raw: JSON.stringify(input),
  body: input,
});
export const expectedProbeStop: ProbeWrite = {
  method: "DELETE",
  path: PROBE_PATH,
  raw: null,
  body: null,
};

/** GET is pure. Only explicit start/stop or step advance the few controlled job states. */
export function createNodeProbeBrowserFixture(stored = false) {
  const ordinary = createMaturityFixture(now);
  let nodes = checked(ProxyNodesSchema, {
    revision: PROBE_REVISION,
    selectedNodeId: probeNodeID(1),
    diagnostics: [],
    nodes: Array.from({ length: PROBE_NODE_COUNT }, (_, index) => ({
      id: probeNodeID(index + 1),
      label: probeNodeLabel(index + 1),
      server: `probe-${index + 1}.example.test`,
      port: 443,
      protocol: "vless",
      transport: "tcp",
      reality: false,
      vision: false,
      utls: false,
      udp: true,
    })),
  });
  const result = (nodeId: string, index: number): NodeProbeResult => ({
    nodeId,
    target: NODE_PROBE_TARGET,
    measuredAt: iso(now + (index + 1) * 1000),
    ...(index === 1
      ? { status: "timeout", errorCode: "node_timeout" }
      : index === 2
        ? { status: "unreachable", errorCode: "node_unreachable" }
        : { status: "success", delayMs: 185 + index }),
  });
  let snapshot = checked(NodeProbeSnapshotSchema, {
    revision: PROBE_REVISION,
    available: true,
    target: NODE_PROBE_TARGET,
    running: false,
    limits: { maxNodes: 256, concurrency: 1, timeoutMs: 3000 },
    results: stored
      ? [2, 3, 4].map((number, index) => result(probeNodeID(number), index))
      : [],
    ...(stored
      ? {
          job: {
            id: "fixture-stored-job",
            status: "completed",
            total: 3,
            completed: 3,
            startedAt: iso(now),
            finishedAt: iso(now + 3000),
          },
        }
      : {}),
  });
  let jobNodeIds: readonly string[] = [];
  let cached: readonly NodeProbeResult[] = [];
  const admissions: { input: NodeProbeRunInput; nodeIds: readonly string[] }[] =
    [];
  const get = (url: URL): unknown => {
    if (url.pathname === PROBE_PATH) {
      assert(!url.search, "Node-probe GET must not have a query");
      return checked(NodeProbeSnapshotSchema, snapshot);
    }
    if (url.pathname === "/api/proxy/nodes")
      return checked(ProxyNodesSchema, nodes);
    if (
      url.pathname === "/api/runtime/config" &&
      url.searchParams.get("service") === "sing-box"
    )
      return checked(RuntimeConfigSchema, {
        service: "sing-box",
        config: nativeConfig,
        generation: 7,
      });
    // Existing REST responses keep their existing Effect decoders; unknown GETs throw.
    return ordinary.get(url);
  };
  const start = (raw: unknown) => {
    const input = checked(NodeProbeRunInputSchema, raw);
    assert(!snapshot.running, "A synthetic job is already running");
    assert(
      input.revision === nodes.revision && snapshot.revision === nodes.revision,
      "Synthetic start requires the current subscription revision",
    );
    const available = new Set(nodes.nodes.map((node) => node.id));
    jobNodeIds = input.all ? nodes.nodes.map((node) => node.id) : input.nodeIds;
    assert(
      jobNodeIds.every((id) => available.has(id)),
      "Synthetic start contains unknown nodes",
    );
    // Match same-revision server readback: keep unrelated stored observations.
    const selected = new Set(jobNodeIds);
    cached = snapshot.results.filter(
      (item) => !selected.has(item.nodeId) && item.measuredAt !== undefined,
    );
    admissions.push({ input, nodeIds: [...jobNodeIds] });
    snapshot = checked(NodeProbeSnapshotSchema, {
      ...snapshot,
      running: true,
      job: {
        id: `fixture-job-${admissions.length}`,
        status: "preparing",
        total: jobNodeIds.length,
        completed: 0,
        startedAt: iso(now),
      },
      results: [
        ...jobNodeIds.map((nodeId) => ({
          nodeId,
          status: "queued",
          target: NODE_PROBE_TARGET,
        })),
        ...cached,
      ],
    });
    return snapshot;
  };
  const step = (completed: number) => {
    const job = snapshot.job;
    assert(
      snapshot.running &&
        job !== undefined &&
        Number.isInteger(completed) &&
        completed >= job.completed &&
        completed <= job.total,
      "Synthetic progress must advance a running job within its exact total",
    );
    if (!job) throw new Error("Synthetic progress requires a job");
    const terminal = completed === job.total;
    snapshot = checked(NodeProbeSnapshotSchema, {
      ...snapshot,
      running: !terminal,
      job: {
        ...job,
        completed,
        status: terminal ? "completed" : "running",
        ...(terminal ? { finishedAt: iso(now + job.total * 1000) } : {}),
      },
      results: [
        ...jobNodeIds.map((nodeId, index) =>
          index < completed
            ? result(nodeId, index)
            : {
                nodeId,
                status: index === completed ? "probing" : "queued",
                target: NODE_PROBE_TARGET,
              },
        ),
        ...cached,
      ],
    });
    return snapshot;
  };
  const stop = () => {
    assert(snapshot.running && snapshot.job !== undefined, "No job to stop");
    const job = snapshot.job!;
    snapshot = checked(NodeProbeSnapshotSchema, {
      ...snapshot,
      running: false,
      job: {
        ...job,
        status: "cancelled",
        completed: job.total,
        finishedAt: iso(now + job.total * 1000),
        errorCode: "node_cancelled",
      },
      results: snapshot.results.map((item) =>
        item.status === "queued" || item.status === "probing"
          ? {
              nodeId: item.nodeId,
              status: "cancelled",
              errorCode: "node_cancelled",
              target: NODE_PROBE_TARGET,
            }
          : item,
      ),
    });
    return snapshot;
  };
  return {
    get,
    start,
    stop,
    step,
    admissions,
    snapshot: () => checked(NodeProbeSnapshotSchema, snapshot),
    nodes: () => checked(ProxyNodesSchema, nodes),
    // Intentionally leave an old response in flight to exercise the browser revision guard.
    setNodesRevision: (revision: string) => {
      assert(
        revision.length > 0 && revision !== nodes.revision,
        "Use a new revision",
      );
      nodes = checked(ProxyNodesSchema, { ...nodes, revision });
    },
  };
}

export async function installNodeProbeBrowserFixture(
  page: Page,
  baseURL: string,
  stored = false,
) {
  const origin = new URL(baseURL).origin; // Includes the configured candidate port.
  const source = createNodeProbeBrowserFixture(stored);
  const reads: string[] = [];
  const writes: ProbeWrite[] = [];
  const unexpected: string[] = [];
  let armed: ProbeWrite | undefined;
  const arm = (entry: ProbeWrite) => {
    assert(!armed, "Consume the explicitly armed probe request first");
    armed = entry;
  };
  // Intercept the full browser boundary. Only same-origin static GET assets may pass.
  await page.route("**/*", async (route) => {
    const request = route.request();
    const url = new URL(request.url());
    const method = request.method();
    try {
      assert(
        url.origin === origin,
        `External browser request forbidden: ${request.url()}`,
      );
      assert(!/\bubus\b/i.test(url.pathname), "Browser ubus is forbidden");
      if (
        method === "GET" &&
        !url.pathname.startsWith("/api/") &&
        ["document", "script", "stylesheet", "image", "font"].includes(
          request.resourceType(),
        )
      ) {
        await route.continue();
        return;
      }
      if (method === "GET") {
        reads.push(`${url.pathname}${url.search}`);
        const value = source.get(url);
        await route.fulfill({
          status: 200,
          contentType:
            url.pathname === "/api/events"
              ? "text/event-stream"
              : "application/json",
          body:
            url.pathname === "/api/events"
              ? `event: snapshot\ndata: ${JSON.stringify(value)}\n\n`
              : JSON.stringify(value),
        });
        return;
      }
      const raw = request.postData();
      const entry = {
        method,
        path: url.pathname,
        raw,
        body: raw ? JSON.parse(raw) : null,
      };
      writes.push(entry);
      assert(
        url.pathname === PROBE_PATH &&
          !url.search &&
          (method === "POST" || method === "DELETE"),
        `Only explicit node-probe mutations are allowed: ${method} ${url.pathname}`,
      );
      assert(
        armed !== undefined && JSON.stringify(entry) === JSON.stringify(armed),
        `Unarmed or non-exact probe mutation: ${method} ${url.pathname}`,
      );
      armed = undefined;
      const value =
        method === "POST" ? source.start(entry.body) : source.stop();
      await route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify(value),
      });
    } catch (error) {
      unexpected.push(error instanceof Error ? error.message : String(error));
      await route.fulfill({
        status: 409,
        contentType: "application/json",
        body: JSON.stringify({
          error: {
            code: "node_probe_fixture_unexpected",
            message: "Synthetic node-probe fixture refused this request",
          },
        }),
      });
    }
  });
  return {
    ...source,
    reads,
    writes,
    unexpected,
    armStart: (input: NodeProbeRunInput) =>
      arm(expectedProbeStart(checked(NodeProbeRunInputSchema, input))),
    armStop: () => arm(expectedProbeStop),
    unconsumed: () => armed,
  };
}

/** Run with native Bun. This does not import the spec or launch Playwright/core/network. */
export function validateNodeProbeBrowserFixtures() {
  const source = createNodeProbeBrowserFixture();
  const url = (path: string) =>
    new URL(path, "http://fixture.example.test:5573");
  const idle = JSON.stringify(source.get(url(PROBE_PATH)));
  assert(
    JSON.stringify(source.get(url(PROBE_PATH))) === idle,
    "GET must be pure",
  );
  assert(source.admissions.length === 0, "Mount GET must not admit a job");
  const nodes = checked(ProxyNodesSchema, source.get(url("/api/proxy/nodes")));
  assert(
    nodes.nodes.length === 220 &&
      nodes.revision === PROBE_REVISION &&
      nodes.selectedNodeId === probeNodeID(1),
    "Exactly220 revision-bound public nodes required",
  );
  const native = checked(
    RuntimeConfigSchema,
    source.get(url("/api/runtime/config?service=sing-box")),
  );
  nodeConfigInputs(native.config);
  for (const path of [
    "/api/session",
    "/api/health",
    "/api/runtime",
    "/api/configuration/status",
  ])
    source.get(url(path));
  const pageIds = Array.from({ length: 20 }, (_, index) =>
    probeNodeID(index + 21),
  );
  source.start({
    all: false,
    nodeIds: [probeNodeID(2)],
    revision: PROBE_REVISION,
  });
  source.step(1);
  source.start({ all: false, nodeIds: pageIds, revision: PROBE_REVISION });
  source.step(2);
  assert(
    source.snapshot().job?.total === 20 &&
      source.snapshot().job?.completed === 2,
    "Current-page progress must be exact20/2",
  );
  source.step(20);
  assert(
    source
      .snapshot()
      .results.some(
        (item) => item.nodeId === probeNodeID(2) && item.status === "success",
      ),
    "Unrelated same-revision single result must survive page probe",
  );
  source.start({ all: true, nodeIds: [], revision: PROBE_REVISION });
  const accepted = JSON.stringify(source.snapshot());
  assert(
    source.admissions.at(-1)?.nodeIds.length === 220 &&
      source.snapshot().job?.total === 220,
    "All flag must resolve all220 fixture nodes",
  );
  assert(
    JSON.stringify(source.get(url(PROBE_PATH))) === accepted,
    "An active GET must not advance job progress",
  );
  source.step(45);
  const stopped = source.stop();
  assert(
    !stopped.running &&
      stopped.job?.status === "cancelled" &&
      stopped.results.filter((item) => item.status === "cancelled").length ===
        175 &&
      stopped.results
        .filter((item) => item.status === "cancelled")
        .every(
          (item) => item.delayMs === undefined && item.measuredAt === undefined,
        ),
    "Cancel must retain measured45 and leave175 unmeasured, never fake zero delays",
  );
  const stored = createNodeProbeBrowserFixture(true);
  const storedSnapshot = stored.snapshot();
  assert(
    storedSnapshot.results[0]?.delayMs === 185 &&
      storedSnapshot.results[0]?.measuredAt === PROBE_MEASURED_AT &&
      storedSnapshot.results[1]?.status === "timeout" &&
      storedSnapshot.results[2]?.status === "unreachable",
    "Stored badge source must remain exact",
  );
  stored.setNodesRevision("fixture-node-probes-revision-2");
  assert(
    stored.nodes().revision !== stored.snapshot().revision &&
      stored.admissions.length === 0,
    "Revision guard source must remain stale without jobs",
  );
  const rejected = (run: () => unknown) => {
    try {
      run();
      return false;
    } catch {
      return true;
    }
  };
  for (const input of [
    { all: "true", nodeIds: [], revision: PROBE_REVISION },
    { all: true, nodeIds: [probeNodeID(1)], revision: PROBE_REVISION },
    { all: false, nodeIds: [], revision: PROBE_REVISION },
    {
      all: false,
      nodeIds: [probeNodeID(1), probeNodeID(1)],
      revision: PROBE_REVISION,
    },
    { all: false, nodeIds: [123], revision: PROBE_REVISION },
    { all: false, nodeIds: ["unknown-node"], revision: PROBE_REVISION },
    { all: true, nodeIds: [], revision: 123 },
    { all: true, nodeIds: [], revision: "old-revision" },
    {
      all: true,
      nodeIds: [],
      revision: PROBE_REVISION,
      target: NODE_PROBE_TARGET,
    },
  ])
    assert(
      rejected(() => createNodeProbeBrowserFixture().start(input)),
      "Start must reject invalid types, selection, revision and extra fields",
    );
  assert(
    rejected(() => source.get(url("/api/unknown-node-probe-read"))) &&
      rejected(() => source.get(url(`${PROBE_PATH}?target=unsafe`))) &&
      rejected(() => source.step(221)),
    "Unknown reads and unbounded steps must fail closed",
  );
  for (const invalid of [
    {
      ...storedSnapshot,
      results: [...storedSnapshot.results, storedSnapshot.results[0]],
    },
    {
      ...storedSnapshot,
      results: [{ ...storedSnapshot.results[1], delayMs: 0 }],
    },
    {
      ...storedSnapshot,
      target: "https://fixture.example.test/not-the-fixed-target",
    },
  ])
    assert(
      rejected(() => checked(NodeProbeSnapshotSchema, invalid)),
      "Effect must reject duplicate nodes, fabricated failure delays and wrong targets",
    );
}
