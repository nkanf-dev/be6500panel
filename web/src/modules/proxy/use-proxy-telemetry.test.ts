import { act, renderHook, waitFor } from "@testing-library/react";
import { Effect } from "effect";
import { describe, expect, it, vi, afterEach } from "vitest";
import { ApiError } from "../../lib/api";
import { useProxyTelemetry } from "./use-proxy-telemetry";

const fixture = vi.hoisted(() => ({
  state: "ready",
  reason: "",
  source: "actual",
  capabilities: {},
  totals: {},
  activeConnections: 0,
  truncated: false,
  connections: [],
  traffic: [],
  probes: [],
}));
const loaders = vi.hoisted(() => ({ metrics: vi.fn(), probe: vi.fn() }));
vi.mock("./telemetry-api", () => ({ proxyTelemetry: loaders }));
afterEach(() => {
  vi.clearAllMocks();
  vi.useRealTimers();
});
describe("passive proxy telemetry hook", () => {
  it("loads observed data without automatic latency probes", async () => {
    loaders.metrics.mockReturnValue(Effect.succeed(fixture));
    loaders.probe.mockReturnValue(Effect.succeed(fixture));
    const { result, unmount } = renderHook(() => useProxyTelemetry());
    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.data).toEqual(fixture);
    expect(loaders.probe).not.toHaveBeenCalled();
    unmount();
  });
  it("only sends a probe on explicit action and keeps probe failures visible after passive refresh", async () => {
    loaders.metrics.mockReturnValue(Effect.succeed(fixture));
    loaders.probe.mockReturnValue(
      Effect.fail(
        new ApiError({
          code: "probe_failed",
          message: "explicit probe failed",
        }),
      ),
    );
    const { result, unmount } = renderHook(() => useProxyTelemetry());
    await waitFor(() => expect(result.current.data).toEqual(fixture));
    await act(async () => {
      await result.current.probe();
    });
    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(loaders.probe).toHaveBeenCalledTimes(1);
    expect(result.current.error).toMatchObject({ code: "probe_failed" });
    expect(result.current.data).toEqual(fixture);
    unmount();
  });
  it("keeps last observed rows on read error without changing them to samples", async () => {
    loaders.metrics
      .mockReturnValueOnce(Effect.succeed(fixture))
      .mockReturnValue(
        Effect.fail(
          new ApiError({
            code: "observation_unavailable",
            message: "core unavailable",
          }),
        ),
      );
    const { result, unmount } = renderHook(() => useProxyTelemetry());
    await waitFor(() => expect(result.current.data).toEqual(fixture));
    act(() => result.current.refresh());
    await waitFor(() =>
      expect(result.current.error).toMatchObject({
        code: "observation_unavailable",
      }),
    );
    expect(result.current.data).toEqual(fixture);
    unmount();
  });
  it("keeps the same source and failed-read provenance during pending passive and manual refresh", async () => {
    vi.useFakeTimers();
    const pending = Effect.async<typeof fixture>(() => {});
    loaders.metrics
      .mockReturnValueOnce(Effect.succeed(fixture))
      .mockReturnValueOnce(pending);
    const { result, unmount } = renderHook(() => useProxyTelemetry());
    await act(async () => {});
    const observed = result.current.data;
    await act(async () => vi.advanceTimersByTimeAsync(4000));
    expect(result.current.loading).toBe(false);
    expect(result.current.data).toBe(observed);
    expect(result.current.data?.source).toBe("actual");
    await act(async () => vi.advanceTimersByTimeAsync(8000));
    expect(loaders.metrics).toHaveBeenCalledTimes(2);
    const cause = new ApiError({
      code: "observation_unavailable",
      message: "core unavailable",
    });
    loaders.metrics.mockReturnValueOnce(Effect.fail(cause));
    act(() => result.current.refresh());
    await act(async () => {});
    expect(result.current.error).toBe(cause);
    loaders.metrics.mockReturnValueOnce(pending);
    act(() => result.current.refresh());
    await act(async () => {});
    expect(result.current.data).toBe(observed);
    expect(result.current.loading).toBe(false);
    expect(result.current.error).toBe(cause);
    expect(loaders.probe).not.toHaveBeenCalled();
    unmount();
  });
  it("cancels the read effect when its consumer unmounts", async () => {
    const aborted = vi.fn();
    loaders.metrics.mockReturnValue(
      Effect.async((_resume, signal) => {
        signal.addEventListener("abort", aborted);
      }),
    );
    const { unmount } = renderHook(() => useProxyTelemetry());
    await act(async () => {
      await Promise.resolve();
    });
    unmount();
    await waitFor(() => expect(aborted).toHaveBeenCalled());
  });
});
