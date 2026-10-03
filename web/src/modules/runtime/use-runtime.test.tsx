import { act, renderHook } from "@testing-library/react";
import { Effect } from "effect";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ApiError, api } from "../../lib/api";
import type { RuntimeService } from "../../lib/contracts";
import { runtimeStatus } from "../production-fixtures.test-data";
import { useRuntime } from "./use-runtime";

const consoleState = vi.hoisted(() => ({ health: { mode: "host" } }));
vi.mock("../../app/console-context", () => ({
  useConsole: () => consoleState,
}));
const response = { enabled: true, services: [runtimeStatus] };
afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
  consoleState.health.mode = "host";
});
describe("runtime observation reads", () => {
  it("keeps compatible status and failed observation provenance through pending polls and retries", async () => {
    vi.useFakeTimers();
    const aborted = vi.fn();
    const pending = Effect.async<typeof response, ApiError>(
      (_resume, signal) => {
        signal.addEventListener("abort", aborted);
      },
    );
    const cause = new ApiError({
      code: "observation_unavailable",
      message: "runtime unavailable",
    });
    const load = vi
      .spyOn(api, "runtime")
      .mockReturnValueOnce(Effect.succeed(response))
      .mockReturnValueOnce(pending);
    const { result, unmount } = renderHook(() => useRuntime("sing-box"));
    await act(async () => {});
    const observed = result.current.status;
    await act(async () => vi.advanceTimersByTimeAsync(3000));
    expect(result.current.status).toBe(observed);
    expect(result.current.loading).toBe(false);
    await act(async () => vi.advanceTimersByTimeAsync(6000));
    expect(load).toHaveBeenCalledTimes(2);
    load.mockReturnValueOnce(Effect.fail(cause));
    act(() => result.current.refresh());
    await act(async () => {});
    expect(result.current.enabled).toBe(false);
    load.mockReturnValueOnce(pending);
    act(() => result.current.refresh());
    await act(async () => {});
    expect(result.current.status).toBe(observed);
    expect(result.current.loading).toBe(false);
    expect(result.current.error).toBe(cause);
    expect(result.current.enabled).toBe(false);
    await act(async () => {
      unmount();
      await vi.advanceTimersByTimeAsync(0);
    });
    expect(aborted).toHaveBeenCalledTimes(2);
  });
  it("hides old-service status and disables mutations while a new service is pending", async () => {
    const pending = Effect.async<typeof response, ApiError>(() => {});
    vi.spyOn(api, "runtime")
      .mockReturnValueOnce(Effect.succeed(response))
      .mockReturnValue(pending);
    const { result, rerender, unmount } = renderHook(
      ({ service }) => useRuntime(service),
      { initialProps: { service: "sing-box" as RuntimeService } },
    );
    await act(async () => {});
    expect(result.current.status?.service).toBe("sing-box");
    rerender({ service: "frpc" });
    expect(result.current.status).toBeUndefined();
    expect(result.current.enabled).toBe(false);
    expect(result.current.loading).toBe(true);
    unmount();
  });
});
