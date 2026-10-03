import { act, renderHook, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type {
  DeviceActivityHistory,
  DeviceActivityRange,
} from "../../lib/device-activity-contracts";
import { loadDeviceActivity } from "../../lib/device-activity-api";
import {
  useDeviceActivity,
  DEVICE_ACTIVITY_REFRESH_MS,
} from "./use-device-activity";
import { activityFixture } from "./activity-fixture.test-data";
vi.mock("../../lib/device-activity-api", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../../lib/device-activity-api")>()),
  loadDeviceActivity: vi.fn(),
}));
const load = vi.mocked(loadDeviceActivity);
function deferred() {
  let resolve!: (data: DeviceActivityHistory) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<DeviceActivityHistory>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}
beforeEach(() => load.mockReset());
afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
});

describe("device activity read ownership", () => {
  it("aborts filters and unmount, ignoring old responses even when cancellation is ignored", async () => {
    const old = deferred(),
      current = deferred();
    load.mockReturnValueOnce(old.promise).mockReturnValueOnce(current.promise);
    const { result, rerender, unmount } = renderHook(
      ({ range, search }) => useDeviceActivity(range, search),
      { initialProps: { range: "24h" as DeviceActivityRange, search: "" } },
    );
    const oldSignal = load.mock.calls[0][2]!;
    rerender({ range: "7d", search: "new" });
    expect(oldSignal.aborted).toBe(true);
    expect(result.current.data).toBeUndefined();
    await act(async () => current.resolve(activityFixture("7d")));
    await act(async () => old.resolve(activityFixture("24h")));
    expect(result.current.data?.range).toBe("7d");
    unmount();
    expect(load.mock.calls[1][2]?.aborted).toBe(true);
  });
  it("never flashes previous search data or errors while a new search is pending", async () => {
    load
      .mockResolvedValueOnce(activityFixture())
      .mockReturnValueOnce(deferred().promise);
    const { result, rerender } = renderHook(
      ({ search }) => useDeviceActivity("24h", search),
      { initialProps: { search: "" } },
    );
    await waitFor(() => expect(result.current.data).toBeDefined());
    rerender({ search: "different MAC" });
    expect(result.current.data).toBeUndefined();
    expect(result.current.error).toBeUndefined();
    expect(result.current.loading).toBe(true);
  });
  it("does not read inactive/demo sources and clears prior data when disabled", async () => {
    load.mockResolvedValue(activityFixture());
    const { result, rerender } = renderHook(
      ({ active }) => useDeviceActivity("24h", "", active),
      { initialProps: { active: false } },
    );
    expect(load).not.toHaveBeenCalled();
    rerender({ active: true });
    await waitFor(() => expect(result.current.data).toBeDefined());
    rerender({ active: false });
    expect(result.current.data).toBeUndefined();
    expect(result.current.loading).toBe(false);
    expect(load.mock.calls[0][2]?.aborted).toBe(true);
  });
  it("polls every 30 seconds without overlapping requests or appending duplicate buckets", async () => {
    vi.useFakeTimers();
    const first = deferred();
    load
      .mockReturnValueOnce(first.promise)
      .mockResolvedValue(activityFixture());
    const { result } = renderHook(() => useDeviceActivity("24h"));
    await act(async () =>
      vi.advanceTimersByTimeAsync(DEVICE_ACTIVITY_REFRESH_MS * 2),
    );
    expect(load).toHaveBeenCalledOnce();
    await act(async () => first.resolve(activityFixture()));
    await act(async () =>
      vi.advanceTimersByTimeAsync(DEVICE_ACTIVITY_REFRESH_MS),
    );
    expect(load).toHaveBeenCalledTimes(2);
    expect(result.current.data?.devices[0].samples).toHaveLength(3);
  });
  it("keeps the observed source and failure visible while background and focus reads are pending", async () => {
    vi.useFakeTimers();
    const sample = { ...activityFixture(), state: "stale" as const };
    const second = deferred();
    const retry = deferred();
    load
      .mockResolvedValueOnce(sample)
      .mockReturnValueOnce(second.promise)
      .mockReturnValueOnce(retry.promise);
    const { result, unmount } = renderHook(() => useDeviceActivity("24h"));
    await act(async () => {});
    expect(result.current.loading).toBe(false);
    const observed = result.current.data;
    await act(async () =>
      vi.advanceTimersByTimeAsync(DEVICE_ACTIVITY_REFRESH_MS),
    );
    expect(result.current.loading).toBe(false);
    expect(result.current.data).toBe(observed);
    expect(result.current.data?.source).toBe("trafficd");
    expect(result.current.data?.state).toBe("stale");
    await act(async () =>
      vi.advanceTimersByTimeAsync(DEVICE_ACTIVITY_REFRESH_MS),
    );
    expect(load).toHaveBeenCalledTimes(2);
    const cause = new Error("source gone");
    await act(async () => second.reject(cause));
    act(() => document.dispatchEvent(new Event("visibilitychange")));
    expect(result.current.loading).toBe(false);
    expect(result.current.data).toBe(observed);
    expect(result.current.error).toBe(cause);
    expect(result.current.data?.sampledAt).toBe(sample.sampledAt);
    unmount();
    expect(load.mock.calls[2][2]?.aborted).toBe(true);
  });
  it("retains only this query on refresh failure and clears errors on read-only retry", async () => {
    load
      .mockResolvedValueOnce(activityFixture())
      .mockRejectedValueOnce(new Error("source gone"))
      .mockResolvedValueOnce(activityFixture());
    const { result } = renderHook(() => useDeviceActivity("24h"));
    await waitFor(() => expect(result.current.data).toBeDefined());
    act(() => result.current.reload());
    await waitFor(() =>
      expect(result.current.error).toEqual(new Error("source gone")),
    );
    expect(result.current.data?.devices).toHaveLength(1);
    act(() => result.current.reload());
    await waitFor(() => expect(result.current.error).toBeUndefined());
  });
  it("does not read hidden tabs and resumes once visible", async () => {
    let hidden = true;
    vi.spyOn(document, "hidden", "get").mockImplementation(() => hidden);
    vi.useFakeTimers();
    load.mockResolvedValue(activityFixture());
    const { result } = renderHook(() => useDeviceActivity("24h"));
    await act(async () =>
      vi.advanceTimersByTimeAsync(DEVICE_ACTIVITY_REFRESH_MS * 2),
    );
    expect(load).not.toHaveBeenCalled();
    hidden = false;
    await act(async () =>
      document.dispatchEvent(new Event("visibilitychange")),
    );
    expect(load).toHaveBeenCalledOnce();
    expect(result.current.data).toBeDefined();
  });
  it("aborts work on hide and ignores that completion after a visible replacement", async () => {
    let hidden = false;
    vi.spyOn(document, "hidden", "get").mockImplementation(() => hidden);
    const old = deferred(),
      current = deferred();
    load.mockReturnValueOnce(old.promise).mockReturnValueOnce(current.promise);
    const { result } = renderHook(() => useDeviceActivity("24h"));
    hidden = true;
    act(() => document.dispatchEvent(new Event("visibilitychange")));
    expect(load.mock.calls[0][2]?.aborted).toBe(true);
    hidden = false;
    act(() => document.dispatchEvent(new Event("visibilitychange")));
    await act(async () =>
      current.resolve({ ...activityFixture(), state: "stale" }),
    );
    await act(async () => old.resolve(activityFixture()));
    expect(result.current.data?.state).toBe("stale");
  });
});
