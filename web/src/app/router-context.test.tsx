import { act, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ConsoleProvider, useConsole } from "./console-context";
import {
  jsonResponse,
  routerSnapshot,
} from "../modules/production-fixtures.test-data";
import { errorMessage } from "../lib/api";
vi.mock("../lib/events", () => ({ connectStatusStream: () => () => {} }));
const noUnauthorized = vi.fn();
const system = {
  mode: "host",
  hostname: "synthetic",
  os: "linux",
  arch: "arm",
  kernel: "test",
  uptimeSeconds: 1,
  cpuCount: 4,
  memory: { totalBytes: 1000, availableBytes: 500 },
  load: [0, 0, 0],
  sampledAt: routerSnapshot.sampledAt,
};
function View() {
  const value = useConsole();
  return (
    <>
      <span data-testid="points">{value.trafficSamples.length}</span>
      <span data-testid="source">{value.trafficSource}</span>
      <span data-testid="last-rate">{value.trafficSamples.at(-1)?.rx}</span>
      <span data-testid="router-error">
        {value.routerError ? errorMessage(value.routerError) : "none"}
      </span>
      <button onClick={value.refreshRouter}>refresh-router</button>
    </>
  );
}
const renderView = () =>
  render(
    <ConsoleProvider onUnauthorized={noUnauthorized}>
      <View />
    </ConsoleProvider>,
  );
beforeEach(() => {
  vi.useFakeTimers();
  vi.spyOn(document, "hidden", "get").mockReturnValue(false);
  vi.spyOn(navigator, "onLine", "get").mockReturnValue(true);
});
afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
  vi.useRealTimers();
  noUnauthorized.mockClear();
});
const basic = (url: string) =>
  jsonResponse(
    url === "/api/health"
      ? { status: "ok", mode: "host", readOnly: false }
      : url === "/api/modules"
        ? { modules: [] }
        : system,
  );
const flush = () =>
  act(async () => {
    await vi.advanceTimersByTimeAsync(0);
  });
describe("central router observations", () => {
  it("samples actual selected WAN, retains first zero and bounds history at 300", async () => {
    let sample = 0;
    const fetch = vi.fn((url: string) => {
      if (url !== "/api/router") return Promise.resolve(basic(url));
      const index = sample++;
      return Promise.resolve(
        jsonResponse({
          ...routerSnapshot,
          sampledAt: new Date(
            Date.UTC(2026, 0, 1) + index * 2000,
          ).toISOString(),
          traffic: [
            { ...routerSnapshot.traffic[0], rxBytesPerSecond: index * 10 },
          ],
        }),
      );
    });
    vi.stubGlobal("fetch", fetch);
    const { unmount } = renderView();
    await flush();
    expect(screen.getByTestId("points")).toHaveTextContent("1");
    expect(screen.getByTestId("last-rate")).toHaveTextContent("0");
    expect(screen.getByTestId("source")).toHaveTextContent("WAN · wan-test");
    await act(async () => {
      await vi.advanceTimersByTimeAsync(610_000);
    });
    expect(screen.getByTestId("points")).toHaveTextContent("300");
    expect(
      fetch.mock.calls.filter(([url]) => url === "/api/router"),
    ).toHaveLength(306);
    unmount();
  });
  it("serializes manual and interval refresh and aborts the active read on unmount", async () => {
    let signal: AbortSignal | undefined;
    const fetch = vi.fn((url: string, init: RequestInit) => {
      if (url !== "/api/router") return Promise.resolve(basic(url));
      signal = init.signal as AbortSignal;
      return new Promise<Response>((_resolve, reject) =>
        signal?.addEventListener("abort", () =>
          reject(new DOMException("aborted", "AbortError")),
        ),
      );
    });
    vi.stubGlobal("fetch", fetch);
    const { unmount } = renderView();
    await flush();
    act(() => {
      screen.getByRole("button", { name: "refresh-router" }).click();
    });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(4000);
    });
    expect(
      fetch.mock.calls.filter(([url]) => url === "/api/router"),
    ).toHaveLength(1);
    expect(signal?.aborted).toBe(false);
    unmount();
    await flush();
    expect(signal?.aborted).toBe(true);
  });
  it("clears live chart points on read failure and preserves the router error code", async () => {
    let failed = false;
    vi.stubGlobal(
      "fetch",
      vi.fn((url: string) =>
        Promise.resolve(
          url !== "/api/router"
            ? basic(url)
            : failed
              ? jsonResponse(
                  {
                    error: {
                      code: "observation_unavailable",
                      message: "采样失败",
                    },
                  },
                  503,
                )
              : jsonResponse(routerSnapshot),
        ),
      ),
    );
    renderView();
    await flush();
    expect(screen.getByTestId("points")).toHaveTextContent("1");
    failed = true;
    await act(async () => {
      await vi.advanceTimersByTimeAsync(2000);
    });
    expect(screen.getByTestId("points")).toHaveTextContent("0");
    expect(screen.getByTestId("router-error")).toHaveTextContent(
      "observation_unavailable",
    );
  });
  it("resets history when the observed default WAN interface changes", async () => {
    let changed = false;
    vi.stubGlobal(
      "fetch",
      vi.fn((url: string) =>
        Promise.resolve(
          url !== "/api/router"
            ? basic(url)
            : !changed
              ? jsonResponse(routerSnapshot)
              : jsonResponse({
                  ...routerSnapshot,
                  sampledAt: "2026-01-01T00:00:02Z",
                  routes: [
                    { ...routerSnapshot.routes[0], interface: "ppp-test" },
                  ],
                  traffic: [
                    {
                      ...routerSnapshot.traffic[0],
                      interface: "ppp-test",
                      rxBytesPerSecond: 100,
                    },
                  ],
                }),
        ),
      ),
    );
    renderView();
    await flush();
    changed = true;
    await act(async () => {
      await vi.advanceTimersByTimeAsync(2000);
    });
    expect(screen.getByTestId("points")).toHaveTextContent("1");
    expect(screen.getByTestId("source")).toHaveTextContent("WAN · ppp-test");
    expect(screen.getByTestId("last-rate")).toHaveTextContent("100");
  });
  it("does not chart zero-valued unavailable traffic as a successful sample", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn((url: string) =>
        Promise.resolve(
          url === "/api/router"
            ? jsonResponse({
                ...routerSnapshot,
                errors: [
                  {
                    module: "traffic",
                    code: "traffic_unavailable",
                    message: "读取失败",
                  },
                ],
              })
            : basic(url),
        ),
      ),
    );
    renderView();
    await flush();
    expect(screen.getByTestId("points")).toHaveTextContent("0");
  });
});
