import { renderHook, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { useDeviceWorkspaceHistory } from "./use-device-workspace-history";
import { activityFixture } from "../components/device-activity/activity-fixture.test-data";
import { jsonResponse } from "../modules/production-fixtures.test-data";
import type { DeviceActivityRange } from "../lib/device-activity-contracts";
afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
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
});
