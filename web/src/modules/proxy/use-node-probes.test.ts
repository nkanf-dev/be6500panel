import { act, renderHook, waitFor } from "@testing-library/react";
import { Effect } from "effect";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ApiError } from "../../lib/api";
import {
  NODE_PROBE_POLL_MS,
  type NodeProbeSnapshot,
} from "./node-probe-contracts";
import {
  activeNodeProbeFixture,
  nodeProbeSnapshotFixture,
} from "./node-probe-fixture.test-data";
import { nodeProbeApi } from "./node-probe-api";
import { useNodeProbes } from "./use-node-probes";

const loaders = {
  snapshot: vi.fn(),
  start: vi.fn(),
  stop: vi.fn(),
};
beforeEach(() => {
  vi.resetAllMocks();
  vi.spyOn(nodeProbeApi, "snapshot").mockImplementation(loaders.snapshot);
  vi.spyOn(nodeProbeApi, "start").mockImplementation(loaders.start);
  vi.spyOn(nodeProbeApi, "stop").mockImplementation(loaders.stop);
  loaders.snapshot.mockReturnValue(Effect.succeed(nodeProbeSnapshotFixture));
});
afterEach(() => {
  vi.clearAllTimers();
  vi.useRealTimers();
});

describe("mounted observation of explicit service-owned node probes", () => {
  it("only GETs on mount, refresh, and node revision changes", async () => {
    const { result, rerender } = renderHook(
      ({ revision }) => useNodeProbes(revision),
      {
        initialProps: { revision: "nodes-revision-1" },
      },
    );
    await waitFor(() => expect(result.current.canStart).toBe(true));
    act(() => result.current.refresh());
    await waitFor(() => expect(loaders.snapshot).toHaveBeenCalledTimes(2));
    rerender({ revision: "nodes-revision-2" });
    await waitFor(() => expect(loaders.snapshot).toHaveBeenCalledTimes(3));
    expect(loaders.start).not.toHaveBeenCalled();
    expect(loaders.stop).not.toHaveBeenCalled();
  });
  it("polls an observed active job with GET only and stops after completion", async () => {
    vi.useFakeTimers();
    loaders.snapshot
      .mockReturnValueOnce(Effect.succeed(activeNodeProbeFixture))
      .mockReturnValue(
        Effect.succeed({
          ...activeNodeProbeFixture,
          running: false,
          job: {
            ...activeNodeProbeFixture.job!,
            status: "completed",
            completed: 2,
          },
        }),
      );
    const { result, unmount } = renderHook(() => useNodeProbes());
    await act(async () => {
      await vi.advanceTimersByTimeAsync(0);
    });
    expect(result.current.running).toBe(true);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(NODE_PROBE_POLL_MS);
    });
    expect(loaders.snapshot).toHaveBeenCalledTimes(2);
    expect(result.current.running).toBe(false);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(NODE_PROBE_POLL_MS * 3);
    });
    expect(loaders.snapshot).toHaveBeenCalledTimes(2);
    unmount();
    expect(loaders.start).not.toHaveBeenCalled();
    expect(loaders.stop).not.toHaveBeenCalled();
  });
  it("keeps polling active jobs after a transient read error without replaying POST", async () => {
    vi.useFakeTimers();
    loaders.snapshot
      .mockReturnValueOnce(Effect.succeed(activeNodeProbeFixture))
      .mockReturnValueOnce(
        Effect.fail(
          new ApiError({ code: "read_failed", message: "read failed" }),
        ),
      )
      .mockReturnValue(Effect.succeed(nodeProbeSnapshotFixture));
    const { result, unmount } = renderHook(() => useNodeProbes());
    await act(async () => {
      await vi.advanceTimersByTimeAsync(0);
    });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(NODE_PROBE_POLL_MS);
    });
    expect(result.current.error).toMatchObject({ code: "read_failed" });
    expect(result.current.running).toBe(true);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(NODE_PROBE_POLL_MS);
    });
    expect(result.current.error).toBeUndefined();
    expect(result.current.running).toBe(false);
    expect(loaders.start).not.toHaveBeenCalled();
    unmount();
  });
  it("starts once with current revision, indicates pending rows, and does not cancel POST on unmount", async () => {
    const aborted = vi.fn();
    let complete:
      | ((value: Effect.Effect<NodeProbeSnapshot, ApiError>) => void)
      | undefined;
    loaders.start.mockReturnValue(
      Effect.async<NodeProbeSnapshot, ApiError>((resume, signal) => {
        complete = resume;
        signal.addEventListener("abort", aborted);
      }),
    );
    const { result, unmount } = renderHook(() =>
      useNodeProbes("nodes-revision-1"),
    );
    await waitFor(() => expect(result.current.canStart).toBe(true));
    let pending: Promise<void> | undefined;
    act(() => {
      pending = result.current.start({ all: false, nodeIds: ["node-2"] });
    });
    await waitFor(() => expect(complete).toBeDefined());
    expect(result.current.running).toBe(true);
    expect(result.current.starting?.nodeIds).toEqual(["node-2"]);
    expect(result.current.progress).toEqual({ completed: 0, total: 1 });
    await act(async () => {
      await result.current.start({ all: true, nodeIds: [] });
    });
    expect(loaders.start).toHaveBeenCalledTimes(1);
    expect(loaders.start).toHaveBeenCalledWith({
      all: false,
      nodeIds: ["node-2"],
      revision: "nodes-revision-1",
    });
    unmount();
    expect(aborted).not.toHaveBeenCalled();
    expect(loaders.stop).not.toHaveBeenCalled();
    complete?.(Effect.succeed(activeNodeProbeFixture));
    await pending;
    expect(aborted).not.toHaveBeenCalled();
  });
  it("uses the server revision when none is supplied and preserves accepted results after a failed GET", async () => {
    loaders.snapshot
      .mockReturnValueOnce(Effect.succeed(nodeProbeSnapshotFixture))
      .mockReturnValue(
        Effect.fail(
          new ApiError({ code: "read_failed", message: "read failed" }),
        ),
      );
    const accepted = {
      ...nodeProbeSnapshotFixture,
      results: [
        {
          ...nodeProbeSnapshotFixture.results[0],
          nodeId: "node-2",
          delayMs: 185,
        },
      ],
    };
    loaders.start.mockReturnValue(Effect.succeed(accepted));
    const { result } = renderHook(() => useNodeProbes());
    await waitFor(() => expect(result.current.canStart).toBe(true));
    await act(async () => {
      await result.current.start({ all: true, nodeIds: [] });
    });
    expect(loaders.start).toHaveBeenCalledWith({
      all: true,
      nodeIds: [],
      revision: "nodes-revision-1",
    });
    await waitFor(() =>
      expect(result.current.error).toMatchObject({ code: "read_failed" }),
    );
    expect(result.current.results).toEqual(accepted.results);
  });
  it("blocks starts for active or unavailable snapshots", async () => {
    for (const snapshot of [
      activeNodeProbeFixture,
      { ...nodeProbeSnapshotFixture, available: false },
    ]) {
      loaders.snapshot.mockReturnValue(Effect.succeed(snapshot));
      const { result, unmount } = renderHook(() => useNodeProbes());
      await waitFor(() => expect(result.current.loading).toBe(false));
      await act(async () => {
        await result.current.start({ all: false, nodeIds: ["node-1"] });
      });
      expect(result.current.canStart).toBe(false);
      unmount();
    }
    expect(loaders.start).not.toHaveBeenCalled();
  });
  it("hides old revision results immediately and refreshes passive state on revision changes", async () => {
    const { result, rerender } = renderHook(
      ({ revision }) => useNodeProbes(revision),
      {
        initialProps: { revision: "nodes-revision-1" },
      },
    );
    await waitFor(() => expect(result.current.results).toHaveLength(1));
    loaders.snapshot.mockReturnValue(Effect.async(() => {}));
    rerender({ revision: "nodes-revision-2" });
    expect(result.current.stale).toBe(true);
    expect(result.current.results).toEqual([]);
    expect(result.current.canStart).toBe(false);
    await act(async () => {
      await result.current.start({ all: false, nodeIds: ["node-1"] });
    });
    expect(loaders.start).not.toHaveBeenCalled();
    loaders.snapshot.mockReturnValue(
      Effect.succeed({
        ...nodeProbeSnapshotFixture,
        revision: "nodes-revision-2",
        results: [],
      }),
    );
    act(() => result.current.refresh());
    await waitFor(() => expect(result.current.stale).toBe(false));
    expect(result.current.results).toEqual([]);
    expect(result.current.canStart).toBe(true);
  });
  it("DELETE is explicit and guarded, remains possible for stale jobs, and survives unmount", async () => {
    loaders.snapshot.mockReturnValue(Effect.succeed(activeNodeProbeFixture));
    const aborted = vi.fn();
    let complete:
      | ((value: Effect.Effect<NodeProbeSnapshot, ApiError>) => void)
      | undefined;
    loaders.stop.mockReturnValue(
      Effect.async<NodeProbeSnapshot, ApiError>((resume, signal) => {
        complete = resume;
        signal.addEventListener("abort", aborted);
      }),
    );
    const { result, unmount } = renderHook(() =>
      useNodeProbes("changed-revision"),
    );
    await waitFor(() => expect(result.current.running).toBe(true));
    expect(result.current.stale).toBe(true);
    let pending: Promise<void> | undefined;
    act(() => {
      pending = result.current.stop();
    });
    await waitFor(() => expect(complete).toBeDefined());
    await act(async () => {
      await result.current.stop();
    });
    expect(loaders.stop).toHaveBeenCalledTimes(1);
    expect(result.current.pending).toBe("stop");
    unmount();
    expect(aborted).not.toHaveBeenCalled();
    complete?.(Effect.succeed(nodeProbeSnapshotFixture));
    await pending;
    expect(aborted).not.toHaveBeenCalled();
  });
  it("does not DELETE idle state and cancels only a passive GET on unmount", async () => {
    const first = renderHook(() => useNodeProbes());
    await waitFor(() => expect(first.result.current.loading).toBe(false));
    await act(async () => {
      await first.result.current.stop();
    });
    expect(loaders.stop).not.toHaveBeenCalled();
    first.unmount();
    const aborted = vi.fn();
    loaders.snapshot.mockReturnValue(
      Effect.async<NodeProbeSnapshot, ApiError>((_resume, signal) => {
        signal.addEventListener("abort", aborted);
      }),
    );
    const passive = renderHook(() => useNodeProbes());
    await act(async () => {
      await Promise.resolve();
    });
    passive.unmount();
    await waitFor(() => expect(aborted).toHaveBeenCalledTimes(1));
    expect(loaders.start).not.toHaveBeenCalled();
    expect(loaders.stop).not.toHaveBeenCalled();
  });
  it("does not let an older GET replace an accepted start snapshot", async () => {
    const accepted = {
      ...nodeProbeSnapshotFixture,
      results: [
        {
          ...nodeProbeSnapshotFixture.results[0],
          delayMs: 185,
        },
      ],
    };
    let finishRead:
      | ((value: Effect.Effect<NodeProbeSnapshot, ApiError>) => void)
      | undefined;
    loaders.snapshot
      .mockReturnValueOnce(Effect.succeed(nodeProbeSnapshotFixture))
      .mockReturnValueOnce(
        Effect.async<NodeProbeSnapshot, ApiError>((resume) => {
          finishRead = resume;
        }),
      )
      .mockReturnValue(
        Effect.fail(
          new ApiError({ code: "read_failed", message: "read failed" }),
        ),
      );
    loaders.start.mockReturnValue(Effect.succeed(accepted));
    const { result } = renderHook(() => useNodeProbes());
    await waitFor(() => expect(result.current.canStart).toBe(true));
    act(() => result.current.refresh());
    await waitFor(() => expect(finishRead).toBeDefined());
    await act(async () => {
      const pending = result.current.start({ all: false, nodeIds: ["node-1"] });
      finishRead?.(
        Effect.succeed({ ...nodeProbeSnapshotFixture, results: [] }),
      );
      await pending;
    });
    await waitFor(() =>
      expect(result.current.error).toMatchObject({ code: "read_failed" }),
    );
    expect(result.current.results).toEqual(accepted.results);
  });
  it("hides an accepted old-revision POST response if nodes changed while it was pending", async () => {
    let finishStart:
      | ((value: Effect.Effect<NodeProbeSnapshot, ApiError>) => void)
      | undefined;
    loaders.start.mockReturnValue(
      Effect.async<NodeProbeSnapshot, ApiError>((resume) => {
        finishStart = resume;
      }),
    );
    const { result, rerender } = renderHook(
      ({ revision }) => useNodeProbes(revision),
      {
        initialProps: { revision: "nodes-revision-1" },
      },
    );
    await waitFor(() => expect(result.current.canStart).toBe(true));
    let pending: Promise<void> | undefined;
    act(() => {
      pending = result.current.start({ all: false, nodeIds: ["node-1"] });
    });
    await waitFor(() => expect(finishStart).toBeDefined());
    loaders.snapshot.mockReturnValue(Effect.async(() => {}));
    rerender({ revision: "nodes-revision-2" });
    await act(async () => {
      finishStart?.(Effect.succeed(nodeProbeSnapshotFixture));
      await pending;
    });
    expect(result.current.stale).toBe(true);
    expect(result.current.results).toEqual([]);
    expect(loaders.start).toHaveBeenCalledTimes(1);
    expect(loaders.stop).not.toHaveBeenCalled();
  });
  it("keeps action errors distinct from successful passive refreshes", async () => {
    loaders.start.mockReturnValue(
      Effect.fail(
        new ApiError({
          code: "revision_mismatch",
          message: "subscription changed",
        }),
      ),
    );
    const { result } = renderHook(() => useNodeProbes());
    await waitFor(() => expect(result.current.canStart).toBe(true));
    await act(async () => {
      await result.current.start({ all: false, nodeIds: ["node-1"] });
    });
    await waitFor(() => expect(loaders.snapshot).toHaveBeenCalledTimes(2));
    expect(result.current.error).toMatchObject({ code: "revision_mismatch" });
    expect(result.current.pending).toBeUndefined();
    expect(result.current.running).toBe(false);
  });
});
