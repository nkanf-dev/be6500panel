import { act, renderHook, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { useDeviceWorkspaceHistory } from "./use-device-workspace-history";
import { activityFixture } from "../components/device-activity/activity-fixture.test-data";
import { jsonResponse } from "../modules/production-fixtures.test-data";
import type { DeviceActivityRange } from "../lib/device-activity-contracts";
afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
  vi.useRealTimers();
});
function install() {
  const fetch = vi.fn((url: string, _init?: RequestInit) => {
    const query = new URL(url, "http://local").searchParams;
    const data = activityFixture(query.get("range") as DeviceActivityRange);
    return Promise.resolve(
      jsonResponse(
        query.get("search")
          ? {
              ...data,
              devices: [{ ...data.devices[0], id: query.get("search")! }],
            }
          : data,
      ),
    );
  });
  vi.stubGlobal("fetch", fetch);
  return fetch;
}
describe("bounded real device workspace history", () => {
  it("merges exact canonical MAC detail queries beyond base rows, caps comparison at eight and never writes", async () => {
    const fetch = install();
    const selected = Array.from(
      { length: 10 },
      (_, i) => `02:00:00:00:00:${(i + 10).toString(16).padStart(2, "0")}`,
    );
    const { result } = renderHook(() =>
      useDeviceWorkspaceHistory("24h", selected),
    );
    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(fetch).toHaveBeenCalledTimes(9);
    expect(result.current.data?.devices).toHaveLength(9);
    expect(
      fetch.mock.calls.every(([, init]) => !init || init.method === "GET"),
    ).toBe(true);
    const queries = fetch.mock.calls.map(
      ([url]) => new URL(url, "http://local").searchParams,
    );
    expect(queries[0].get("limit")).toBe("64");
    expect(
      queries
        .slice(1)
        .every(
          (query) =>
            query.get("limit") === "1" && query.get("maxPoints") === "288",
        ),
    ).toBe(true);
    expect(queries[1].get("search")).toBe("02:00:00:00:00:0A");
  });
  it("keeps selected detail records and provenance stable during pending polls and failed retries", async () => {
    vi.useFakeTimers();
    const mac = "02:00:00:00:00:AA";
    let fail = false;
    let pending = false;
    const signals: AbortSignal[] = [];
    const fetch = vi.fn((url: string, init?: RequestInit) => {
      if (pending) {
        signals.push(init?.signal as AbortSignal);
        return new Promise<Response>(() => {});
      }
      if (fail && url.includes("search=02"))
        return Promise.resolve(
          jsonResponse(
            {
              error: {
                code: "observation_unavailable",
                message: "detail unavailable",
              },
            },
            503,
          ),
        );
      const data = activityFixture();
      return Promise.resolve(
        jsonResponse(
          url.includes("search=02")
            ? { ...data, devices: [{ ...data.devices[0], id: mac }] }
            : data,
        ),
      );
    });
    vi.stubGlobal("fetch", fetch);
    const { result, unmount } = renderHook(() =>
      useDeviceWorkspaceHistory("24h", [mac]),
    );
    await act(async () => {});
    expect(result.current.loading).toBe(false);
    const observed = result.current.data;
    pending = true;
    await act(async () => vi.advanceTimersByTimeAsync(30_000));
    expect(result.current.loading).toBe(false);
    expect(result.current.data).toBe(observed);
    await act(async () => vi.advanceTimersByTimeAsync(30_000));
    expect(fetch).toHaveBeenCalledTimes(4);
    pending = false;
    fail = true;
    act(() => result.current.refresh());
    await act(async () => {});
    expect(result.current.loading).toBe(false);
    expect(result.current.data?.devices.map((device) => device.id)).toContain(
      mac,
    );
    expect(result.current.data?.state).toBe("stale");
    expect(result.current.data?.devices.every((device) => device.stale)).toBe(
      true,
    );
    expect(result.current.data?.source).toBe("trafficd");
    expect(result.current.data?.sampledAt).toBe(observed?.sampledAt);
    const failed = result.current.error;
    const retained = result.current.data;
    pending = true;
    await act(async () => vi.advanceTimersByTimeAsync(30_000));
    expect(result.current.data).toBe(retained);
    expect(result.current.error).toBe(failed);
    expect(result.current.loading).toBe(false);
    await act(async () => {
      unmount();
      await vi.advanceTimersByTimeAsync(0);
    });
    expect(signals.every((signal) => signal.aborted)).toBe(true);
  });
  it("clears old-range history before new range arrives and cancels all old reads", async () => {
    const fetch = install();
    const { result, rerender, unmount } = renderHook(
      ({ range }) => useDeviceWorkspaceHistory(range, []),
      { initialProps: { range: "24h" as DeviceActivityRange } },
    );
    await waitFor(() => expect(result.current.data?.range).toBe("24h"));
    let signal: AbortSignal | undefined;
    fetch.mockImplementation((_url: string, init?: RequestInit) => {
      signal = init?.signal as AbortSignal;
      return new Promise<Response>(() => {});
    });
    rerender({ range: "7d" });
    expect(result.current.data).toBeUndefined();
    await waitFor(() => expect(signal).toBeDefined());
    unmount();
    await waitFor(() => expect(signal?.aborted).toBe(true));
  });
  it("keeps independent base observations while exposing a failed detail source", async () => {
    const fetch = install();
    fetch.mockImplementation((url: string) =>
      Promise.resolve(
        url.includes("search=02")
          ? jsonResponse(
              {
                error: {
                  code: "observation_unavailable",
                  message: "detail unavailable",
                },
              },
              503,
            )
          : jsonResponse(activityFixture()),
      ),
    );
    const { result } = renderHook(() =>
      useDeviceWorkspaceHistory("24h", ["02:00:00:00:00:AA"]),
    );
    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.error).toBeDefined();
    expect(result.current.data?.devices[0].name).toBe("测试终端");
  });
  it("rejects a detail response for another MAC instead of attributing it to selection", async () => {
    vi.stubGlobal(
      "fetch",
      vi
        .fn()
        .mockImplementation(() =>
          Promise.resolve(jsonResponse(activityFixture())),
        ),
    );
    const { result } = renderHook(() =>
      useDeviceWorkspaceHistory("24h", ["02:00:00:00:00:AA"]),
    );
    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.error).toBeInstanceOf(Error);
    expect(result.current.data?.devices.map((item) => item.id)).toEqual([
      "02:00:00:00:00:01",
    ]);
  });
  it("searches the complete collector while preserving bounded exact selected-MAC queries", async () => {
    const fetch = install();
    const expectedMAC = "02:00:00:00:00:AA";
    fetch.mockImplementation((_url: string) => {
      const data = activityFixture("24h");
      return Promise.resolve(
        jsonResponse({
          ...data,
          deviceCount: 128,
          matchedCount: 1,
          truncated: false,
          devices: [{ ...data.devices[0], id: expectedMAC, name: "device128" }],
        }),
      );
    });
    const { result } = renderHook(() =>
      useDeviceWorkspaceHistory("24h", [expectedMAC], "device128"),
    );
    await waitFor(() => expect(result.current.loading).toBe(false));
    const queries = fetch.mock.calls.map(
      ([url]) => new URL(url, "http://local").searchParams,
    );
    expect(queries).toHaveLength(2);
    expect(queries[0].get("search")).toBe("device128");
    expect(queries[0].get("limit")).toBe("64");
    expect(queries[1].get("search")).toBe(expectedMAC);
    expect(queries[1].get("limit")).toBe("1");
    expect(result.current.data?.deviceCount).toBe(128);
    expect(result.current.data?.devices.map((d) => d.id)).toEqual([
      expectedMAC,
    ]);
    expect(
      fetch.mock.calls.every(([, init]) => !init || init.method === "GET"),
    ).toBe(true);
  });
  it("bounds source text search to64codepoints and invalidates old search evidence", async () => {
    const fetch = install();
    fetch.mockImplementation(() =>
      Promise.resolve(jsonResponse(activityFixture("24h"))),
    );
    const { result, rerender } = renderHook(
      ({ search }) => useDeviceWorkspaceHistory("24h", [], search),
      { initialProps: { search: "old" } },
    );
    await waitFor(() => expect(result.current.loading).toBe(false));
    let signal: AbortSignal | undefined;
    fetch.mockImplementation((_url: string, init?: RequestInit) => {
      signal = init?.signal as AbortSignal;
      return new Promise<Response>(() => {});
    });
    rerender({ search: "界".repeat(70) });
    expect(result.current.data).toBeUndefined();
    await waitFor(() => expect(signal).toBeDefined());
    const latest = new URL(fetch.mock.calls.at(-1)![0], "http://local")
      .searchParams;
    expect(Array.from(latest.get("search")!)).toHaveLength(64);
    expect(latest.get("limit")).toBe("64");
  });
  it("retains the unchanged base list when comparison selections change and detail reads are pending", async () => {
    const fetch = install();
    const first = "02:00:00:00:00:0A",
      second = "02:00:00:00:00:0B";
    const { result, rerender } = renderHook(
      ({ selected }) => useDeviceWorkspaceHistory("24h", selected),
      { initialProps: { selected: [first] } },
    );
    await waitFor(() => expect(result.current.loading).toBe(false));
    const original = result.current.data?.devices.find(
      (d) => d.id === "02:00:00:00:00:01",
    );
    fetch.mockImplementation(() => new Promise<Response>(() => {}));
    rerender({ selected: [first, second] });
    expect(
      result.current.data?.devices.find((d) => d.id === "02:00:00:00:00:01"),
    ).toBe(original);
    expect(result.current.loading).toBe(false);
    await waitFor(() => expect(fetch.mock.calls.length).toBe(5));
    expect(result.current.data?.devices.map((d) => d.id)).toContain(first);
  });
});
