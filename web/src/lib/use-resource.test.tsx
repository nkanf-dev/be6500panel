import { act, renderHook, waitFor } from "@testing-library/react";
import { Effect } from "effect";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ApiError } from "./api";
import { useResource } from "./use-resource";

afterEach(() => vi.useRealTimers());
const sample = {
  source: "actual collector",
  sampledAt: "2026-10-03T02:00:00Z",
  value: 7,
};
function pendingRead() {
  const aborted = vi.fn();
  const effect = Effect.async<typeof sample, ApiError>((_resume, signal) => {
    signal.addEventListener("abort", aborted);
  });
  return { effect, aborted };
}
describe("compatible resource reads", () => {
  it("retains data, source, and error through pending manual retries", async () => {
    const pending = pendingRead();
    const cause = new ApiError({
      code: "observation_unavailable",
      message: "source unavailable",
    });
    const load = vi
      .fn()
      .mockReturnValueOnce(Effect.succeed(sample))
      .mockReturnValueOnce(Effect.fail(cause))
      .mockReturnValue(pending.effect);
    const { result, unmount } = renderHook(() => useResource(load));
    await act(async () => {});
    expect(result.current.loading).toBe(false);
    act(() => result.current.reload());
    await act(async () => {});
    expect(result.current.error).toBe(cause);
    act(() => result.current.reload());
    await act(async () => {});
    expect(result.current.loading).toBe(false);
    expect(result.current.data).toBe(sample);
    expect(result.current.error).toBe(cause);
    unmount();
    await waitFor(() => expect(pending.aborted).toHaveBeenCalledOnce());
  });
  it("does not present old loader data or errors for a new query", async () => {
    const oldLoad = () => Effect.succeed(sample);
    const pending = pendingRead();
    const newLoad = () => pending.effect;
    const { result, rerender, unmount } = renderHook(
      ({ load }) => useResource(load),
      {
        initialProps: {
          load: oldLoad as () => Effect.Effect<typeof sample, ApiError>,
        },
      },
    );
    await act(async () => {});
    rerender({ load: newLoad });
    expect(result.current.data).toBeUndefined();
    expect(result.current.error).toBeUndefined();
    expect(result.current.loading).toBe(true);
    await act(async () => {});
    unmount();
    await waitFor(() => expect(pending.aborted).toHaveBeenCalledOnce());
  });
});
