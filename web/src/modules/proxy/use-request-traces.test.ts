import { act, renderHook, waitFor } from "@testing-library/react";
import { Effect } from "effect";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ApiError } from "../../lib/api";
import {
  requestTraceFixture,
  requestTraceHistoryFixture,
} from "./request-trace-fixture.test-data";
import { useRequestTraces } from "./use-request-traces";

const loaders = vi.hoisted(() => ({ history: vi.fn(), run: vi.fn() }));
vi.mock("./request-trace-api", () => ({ requestTraceApi: loaders }));
afterEach(() => vi.clearAllMocks());
describe("explicit active-request hook", () => {
  it("loads only GET history, never runs on mount or history refresh", async () => {
    loaders.history.mockReturnValue(Effect.succeed(requestTraceHistoryFixture));
    const { result } = renderHook(() => useRequestTraces());
    await waitFor(() => expect(result.current.loading).toBe(false));
    act(() => result.current.refresh());
    await waitFor(() => expect(loaders.history).toHaveBeenCalledTimes(2));
    expect(loaders.run).not.toHaveBeenCalled();
  });
  it("guards simultaneous own runs and keeps the accepted trace when the next GET fails", async () => {
    loaders.history
      .mockReturnValueOnce(Effect.succeed(requestTraceHistoryFixture))
      .mockReturnValue(
        Effect.fail(
          new ApiError({ code: "history_failed", message: "history failed" }),
        ),
      );
    let complete:
      | ((value: Effect.Effect<typeof requestTraceFixture>) => void)
      | undefined;
    loaders.run.mockReturnValue(
      Effect.async<typeof requestTraceFixture>((resume) => {
        complete = resume;
      }),
    );
    const { result } = renderHook(() => useRequestTraces());
    await waitFor(() => expect(result.current.loading).toBe(false));
    let pending: Promise<void> | undefined;
    act(() => {
      pending = result.current.run({ targetId: "cloudflare", route: "direct" });
    });
    await waitFor(() => expect(result.current.running).toBe(true));
    await act(async () => {
      await result.current.run({ targetId: "google204", route: "proxy" });
    });
    expect(loaders.run).toHaveBeenCalledTimes(1);
    const accepted = { ...requestTraceFixture, id: "accepted-run" };
    await act(async () => {
      complete?.(Effect.succeed(accepted));
      await pending;
    });
    await waitFor(() =>
      expect(result.current.error).toMatchObject({ code: "history_failed" }),
    );
    expect(result.current.data?.traces.map((trace) => trace.id)).toEqual([
      "accepted-run",
      "trace-actual",
    ]);
  });
  it("does not run when the server says a diagnostic is already running", async () => {
    loaders.history.mockReturnValue(
      Effect.succeed({ ...requestTraceHistoryFixture, running: true }),
    );
    const { result } = renderHook(() => useRequestTraces());
    await waitFor(() => expect(result.current.loading).toBe(false));
    await act(async () => {
      await result.current.run({ targetId: "google204", route: "direct" });
    });
    expect(loaders.run).not.toHaveBeenCalled();
    expect(result.current.running).toBe(true);
  });
  it("never cancels an accepted POST on unmount; its completed trace remains on the backend", async () => {
    const aborted = vi.fn();
    let complete:
      | ((value: Effect.Effect<typeof requestTraceFixture>) => void)
      | undefined;
    loaders.history.mockReturnValue(Effect.succeed(requestTraceHistoryFixture));
    loaders.run.mockReturnValue(
      Effect.async<typeof requestTraceFixture>((resume, signal) => {
        complete = resume;
        signal.addEventListener("abort", aborted);
      }),
    );
    const { result, unmount } = renderHook(() => useRequestTraces());
    await waitFor(() => expect(result.current.loading).toBe(false));
    let pending: Promise<void> | undefined;
    act(() => {
      pending = result.current.run({ targetId: "google204", route: "direct" });
    });
    await waitFor(() => expect(complete).toBeDefined());
    unmount();
    expect(aborted).not.toHaveBeenCalled();
    complete?.(Effect.succeed(requestTraceFixture));
    await pending;
    expect(aborted).not.toHaveBeenCalled();
  });
  it("keeps run errors separate from successful history refreshes and cancels only the passive read", async () => {
    loaders.history.mockReturnValue(Effect.succeed(requestTraceHistoryFixture));
    loaders.run.mockReturnValue(
      Effect.fail(
        new ApiError({
          code: "proxy_unavailable",
          message: "current proxy unavailable",
        }),
      ),
    );
    const { result, unmount } = renderHook(() => useRequestTraces());
    await waitFor(() => expect(result.current.loading).toBe(false));
    await act(async () => {
      await result.current.run({ targetId: "cloudflare", route: "proxy" });
    });
    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.error).toMatchObject({ code: "proxy_unavailable" });
    unmount();
    const aborted = vi.fn();
    loaders.history.mockReturnValue(
      Effect.async((_resume, signal) => {
        signal.addEventListener("abort", aborted);
      }),
    );
    const readOnly = renderHook(() => useRequestTraces());
    await act(async () => {
      await Promise.resolve();
    });
    readOnly.unmount();
    await waitFor(() => expect(aborted).toHaveBeenCalled());
  });
});
