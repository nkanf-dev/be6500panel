import { Schema } from "effect";
import { afterEach, describe, expect, it, vi } from "vitest";
import { runRequest } from "../../lib/api";
import { nodeProbeApi } from "./node-probe-api";
import {
  NODE_PROBE_TARGET,
  NodeProbeRunInputSchema,
  NodeProbeSnapshotSchema,
  type NodeProbeRunInput,
} from "./node-probe-contracts";
import {
  activeNodeProbeFixture,
  nodeProbeSnapshotFixture,
} from "./node-probe-fixture.test-data";

const response = (body: unknown, status = 200) =>
  new Response(JSON.stringify(body), {
    status,
    headers: { "Content-Type": "application/json" },
  });
afterEach(() => vi.unstubAllGlobals());

describe("explicit fixed-target node probe API", () => {
  it("GET reads bounded snapshots without starting or selecting nodes", async () => {
    const fetch = vi
      .fn()
      .mockImplementation(async () => response(activeNodeProbeFixture));
    vi.stubGlobal("fetch", fetch);
    const snapshot = await runRequest(nodeProbeApi.snapshot());
    expect(snapshot).toEqual(activeNodeProbeFixture);
    expect(fetch).toHaveBeenCalledTimes(1);
    expect(fetch).toHaveBeenCalledWith(
      "/api/proxy/node-probes",
      expect.objectContaining({ method: "GET", credentials: "same-origin" }),
    );
  });
  it("POST sends only explicit selection and revision; all uses an empty node list", async () => {
    const fetch = vi
      .fn()
      .mockImplementation(async () => response(activeNodeProbeFixture));
    vi.stubGlobal("fetch", fetch);
    for (const input of [
      {
        all: false,
        nodeIds: ["node-1", "node-2"],
        revision: "nodes-revision-1",
      },
      { all: true, nodeIds: [], revision: "nodes-revision-1" },
    ]) {
      await runRequest(nodeProbeApi.start(input));
      expect(fetch).toHaveBeenLastCalledWith(
        "/api/proxy/node-probes",
        expect.objectContaining({
          method: "POST",
          body: JSON.stringify(input),
          credentials: "same-origin",
        }),
      );
    }
    expect(fetch).toHaveBeenCalledTimes(2);
  });
  it("DELETE stops the current job without a body or node-selection request", async () => {
    const fetch = vi
      .fn()
      .mockImplementation(async () => response(nodeProbeSnapshotFixture));
    vi.stubGlobal("fetch", fetch);
    await runRequest(nodeProbeApi.stop());
    expect(fetch).toHaveBeenCalledTimes(1);
    expect(fetch).toHaveBeenCalledWith(
      "/api/proxy/node-probes",
      expect.objectContaining({ method: "DELETE" }),
    );
    expect(fetch.mock.calls[0][1]).not.toHaveProperty("body");
  });
  it("does not retry explicit starts or stops and retains backend error codes", async () => {
    const fetch = vi
      .fn()
      .mockImplementation(async () =>
        response({ error: { code: "probe_busy", message: "busy" } }, 503),
      );
    vi.stubGlobal("fetch", fetch);
    await expect(
      runRequest(
        nodeProbeApi.start({
          all: false,
          nodeIds: ["node-1"],
          revision: "nodes-revision-1",
        }),
      ),
    ).rejects.toMatchObject({ code: "probe_busy" });
    expect(fetch).toHaveBeenCalledTimes(1);
    await expect(runRequest(nodeProbeApi.stop())).rejects.toMatchObject({
      code: "probe_busy",
    });
    expect(fetch).toHaveBeenCalledTimes(2);
  });
  it("rejects empty, duplicate, oversized or ambiguous selections before fetching", async () => {
    const fetch = vi.fn();
    vi.stubGlobal("fetch", fetch);
    const invalid = [
      { all: false, nodeIds: [], revision: "r1" },
      { all: true, nodeIds: ["node-1"], revision: "r1" },
      { all: false, nodeIds: ["node-1", "node-1"], revision: "r1" },
      { all: false, nodeIds: [""], revision: "r1" },
      { all: false, nodeIds: ["node-1"], revision: "" },
      {
        all: false,
        nodeIds: Array.from({ length: 257 }, (_, i) => `node-${i}`),
        revision: "r1",
      },
    ];
    for (const input of invalid) {
      expect(() =>
        Schema.decodeUnknownSync(NodeProbeRunInputSchema)(input),
      ).toThrow();
      await expect(
        runRequest(nodeProbeApi.start(input as NodeProbeRunInput)),
      ).rejects.toMatchObject({ code: "invalid_request" });
    }
    expect(fetch).not.toHaveBeenCalled();
  });
  it("accepts every job/result state and unavailable snapshots without fabricated delay", () => {
    for (const status of [
      "queued",
      "probing",
      "timeout",
      "unreachable",
      "cancelled",
    ] as const) {
      const decoded = Schema.decodeUnknownSync(NodeProbeSnapshotSchema)({
        ...nodeProbeSnapshotFixture,
        results: [{ nodeId: "node-1", status, target: NODE_PROBE_TARGET }],
      });
      expect(decoded.results[0].delayMs).toBeUndefined();
    }
    for (const status of [
      "preparing",
      "running",
      "completed",
      "cancelled",
      "invalidated",
      "failed",
    ] as const) {
      expect(
        Schema.decodeUnknownSync(NodeProbeSnapshotSchema)({
          ...activeNodeProbeFixture,
          job: { ...activeNodeProbeFixture.job, status },
        }).job?.status,
      ).toBe(status);
    }
    const unavailable = Schema.decodeUnknownSync(NodeProbeSnapshotSchema)({
      ...nodeProbeSnapshotFixture,
      available: false,
      unavailableCode: "artifact_unavailable",
      results: [],
    });
    expect(unavailable.results).toEqual([]);
  });
  it("rejects arbitrary targets, invalid timestamps, delays, progress, limits and unbounded results", () => {
    const result = nodeProbeSnapshotFixture.results[0];
    const invalid = [
      { ...nodeProbeSnapshotFixture, target: "https://example.test" },
      {
        ...nodeProbeSnapshotFixture,
        results: [{ ...result, target: "https://example.test" }],
      },
      { ...nodeProbeSnapshotFixture, results: [{ ...result, delayMs: -1 }] },
      {
        ...nodeProbeSnapshotFixture,
        results: [{ ...result, delayMs: undefined }],
      },
      {
        ...nodeProbeSnapshotFixture,
        results: [{ ...result, measuredAt: undefined }],
      },
      {
        ...nodeProbeSnapshotFixture,
        results: [{ ...result, status: "timeout" }],
      },
      {
        ...nodeProbeSnapshotFixture,
        results: [{ ...result, delayMs: Infinity }],
      },
      {
        ...nodeProbeSnapshotFixture,
        results: [{ ...result, measuredAt: "yesterday" }],
      },
      {
        ...nodeProbeSnapshotFixture,
        results: [{ ...result, status: "invented" }],
      },
      { ...nodeProbeSnapshotFixture, results: [result, result] },
      {
        ...nodeProbeSnapshotFixture,
        results: Array.from({ length: 257 }, (_, i) => ({
          ...result,
          nodeId: `node-${i}`,
        })),
      },
      {
        ...activeNodeProbeFixture,
        job: { ...activeNodeProbeFixture.job, completed: 3 },
      },
      {
        ...activeNodeProbeFixture,
        job: { ...activeNodeProbeFixture.job, total: 257 },
      },
      {
        ...activeNodeProbeFixture,
        job: {
          ...activeNodeProbeFixture.job,
          finishedAt: "2026-10-03T03:59:00Z",
        },
      },
      {
        ...nodeProbeSnapshotFixture,
        limits: { ...nodeProbeSnapshotFixture.limits, concurrency: 8 },
      },
      {
        ...nodeProbeSnapshotFixture,
        limits: { ...nodeProbeSnapshotFixture.limits, timeoutMs: 10000 },
      },
    ];
    for (const snapshot of invalid)
      expect(() =>
        Schema.decodeUnknownSync(NodeProbeSnapshotSchema)(snapshot),
      ).toThrow();
  });
});
