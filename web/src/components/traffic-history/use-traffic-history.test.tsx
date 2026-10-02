import { act, renderHook, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { TrafficHistory } from "../../lib/traffic-history-contracts";
import { loadTrafficHistory } from "../../lib/traffic-history-api";
import {
  useTrafficHistory,
  TRAFFIC_HISTORY_REFRESH_MS,
} from "./use-traffic-history";
import { historyFixture } from "./history-fixture.test-data";
vi.mock("../../lib/traffic-history-api", () => ({
  loadTrafficHistory: vi.fn(),
}));
const load = vi.mocked(loadTrafficHistory);
function deferred() {
  let resolve!: (data: TrafficHistory) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<TrafficHistory>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}
beforeEach(() => {
  load.mockReset();
});
afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
});

describe("history range ownership and cancellation", () => {
  it("aborts the old window and rejects a late result even if the loader ignores cancellation", async () => {
    const old = deferred();
    const current = deferred();
    load.mockReturnValueOnce(old.promise).mockReturnValueOnce(current.promise);
    const { result, rerender, unmount } = renderHook(
      ({ range }) => useTrafficHistory(range),
      { initialProps: { range: "30m" as "30m" | "1y" } },
    );
    const oldSignal = load.mock.calls[0][1]!;
    rerender({ range: "1y" });
    expect(oldSignal.aborted).toBe(true);
    expect(result.current.data).toBeUndefined();
    await act(async () => current.resolve(historyFixture("1y")));
    expect(result.current.data?.range).toBe("1y");
    await act(async () => old.resolve(historyFixture("30m")));
    expect(result.current.data?.range).toBe("1y");
    unmount();
    expect(load.mock.calls[1][1]?.aborted).toBe(true);
  });
  it("does not show previous range data, errors or totals while the new request is pending", async () => {
    load
      .mockResolvedValueOnce(historyFixture())
      .mockReturnValueOnce(deferred().promise);
    const { result, rerender } = renderHook(
      ({ range }) => useTrafficHistory(range),
      { initialProps: { range: "30m" as "30m" | "1d" } },
    );
    await waitFor(() => expect(result.current.data?.range).toBe("30m"));
    rerender({ range: "1d" });
    expect(result.current.data).toBeUndefined();
    expect(result.current.error).toBeUndefined();
    expect(result.current.loading).toBe(true);
  });
  it("does not query inactive/demo history and aborts active requests when disabled", () => {
    load.mockReturnValue(deferred().promise);
    const { result, rerender } = renderHook(
      ({ active }) => useTrafficHistory("30m", active),
      { initialProps: { active: false } },
    );
    expect(load).not.toHaveBeenCalled();
    expect(result.current.loading).toBe(false);
    rerender({ active: true });
    expect(load).toHaveBeenCalledOnce();
    rerender({ active: false });
    expect(load.mock.calls[0][1]?.aborted).toBe(true);
    expect(result.current.data).toBeUndefined();
  });
  it("refreshes serially every 30 seconds rather than appending or overlapping requests", async () => {
    vi.useFakeTimers();
    const first = deferred();
    load.mockReturnValueOnce(first.promise).mockResolvedValue(historyFixture());
    const { result } = renderHook(() => useTrafficHistory("30m"));
    await act(async () =>
      vi.advanceTimersByTimeAsync(TRAFFIC_HISTORY_REFRESH_MS * 2),
    );
    expect(load).toHaveBeenCalledOnce();
    await act(async () => first.resolve(historyFixture()));
    await act(async () =>
      vi.advanceTimersByTimeAsync(TRAFFIC_HISTORY_REFRESH_MS),
    );
    expect(load).toHaveBeenCalledTimes(2);
    expect(result.current.data?.samples).toHaveLength(3);
  });
  it("retains only the current range last successful history after a refresh error and exposes retry", async () => {
    load
      .mockResolvedValueOnce(historyFixture())
      .mockRejectedValueOnce(new Error("disk unavailable"))
      .mockResolvedValueOnce(historyFixture());
    const { result } = renderHook(() => useTrafficHistory("30m"));
    await waitFor(() => expect(result.current.data).toBeDefined());
    act(() => result.current.reload());
    await waitFor(() =>
      expect(result.current.error).toEqual(new Error("disk unavailable")),
    );
    expect(result.current.data?.samples).toHaveLength(3);
    act(() => result.current.reload());
    await waitFor(() => expect(result.current.error).toBeUndefined());
    expect(load.mock.calls[0][1]?.aborted).toBe(true);
  });
  it("pauses hidden-page reads and refreshes once visible", async () => {
    let hidden = true;
    vi.spyOn(document, "hidden", "get").mockImplementation(() => hidden);
    load.mockResolvedValue(historyFixture());
    const { result } = renderHook(() => useTrafficHistory("30m"));
    expect(load).not.toHaveBeenCalled();
    hidden = false;
    act(() => document.dispatchEvent(new Event("visibilitychange")));
    await waitFor(() => expect(result.current.data).toBeDefined());
    expect(load).toHaveBeenCalledOnce();
  });
});
