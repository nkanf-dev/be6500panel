import { act, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ReactNode } from "react";
import { App } from "./app";
import { useConsole } from "./console-context";
import {
  routerSnapshot,
  jsonResponse,
} from "../modules/production-fixtures.test-data";
import { errorMessage } from "../lib/api";
import { ThemeProvider } from "../theme";

vi.mock("./shell", () => ({
  Shell: ({
    children,
    recoveryBanner,
  }: {
    children: ReactNode;
    recoveryBanner?: ReactNode;
  }) => (
    <>
      {recoveryBanner}
      {children}
    </>
  ),
}));
vi.mock("./pages", () => ({
  ModulePage: () => {
    const value = useConsole();
    return (
      <>
        <span data-testid="system-host">{value.system?.hostname}</span>
        <span data-testid="system-error">
          {value.systemError ? errorMessage(value.systemError) : "none"}
        </span>
        <span data-testid="connection">{value.connection}</span>
      </>
    );
  },
}));

class EventSourceMock {
  static instances: EventSourceMock[] = [];
  onopen?: () => void;
  onerror?: () => void;
  listeners = new Map<string, (event: MessageEvent) => void>();
  close = vi.fn();
  constructor(public url: string) {
    EventSourceMock.instances.push(this);
  }
  addEventListener(type: string, listener: (event: MessageEvent) => void) {
    this.listeners.set(type, listener);
  }
}
const system = {
  mode: "host",
  hostname: "sample-host",
  os: "linux",
  arch: "arm",
  kernel: "sample",
  uptimeSeconds: 10,
  cpuCount: 4,
  memory: { totalBytes: 1000, availableBytes: 500 },
  load: [1, 1, 1],
  sampledAt: "2026-01-01T00:00:00Z",
};
const respond = (body: unknown, status = 200) =>
  new Response(JSON.stringify(body), {
    status,
    headers: { "Content-Type": "application/json" },
  });
let sessionExpired = false;
let observationFailed = false;
let fetchMock: ReturnType<typeof vi.fn>;
beforeEach(() => {
  sessionExpired = false;
  observationFailed = false;
  EventSourceMock.instances = [];
  vi.stubGlobal("EventSource", EventSourceMock);
  vi.spyOn(document, "hidden", "get").mockReturnValue(false);
  vi.spyOn(navigator, "onLine", "get").mockReturnValue(true);
  fetchMock = vi.fn((url: string) => {
    if (url === "/api/session")
      return Promise.resolve(
        respond({ authenticated: !sessionExpired, authRequired: true }),
      );
    if (url === "/api/health")
      return Promise.resolve(
        respond({ status: "ok", mode: "host", readOnly: true }),
      );
    if (url === "/api/modules")
      return Promise.resolve(respond({ modules: [] }));
    if (url === "/api/configuration/status")
      return Promise.resolve(respond({ enabled: true, generation: 1 }));
    if (url === "/api/router")
      return Promise.resolve(jsonResponse(routerSnapshot));
    if (url === "/api/system")
      return Promise.resolve(
        observationFailed
          ? respond(
              {
                error: {
                  code: "observation_unavailable",
                  message: "系统采样不可用",
                },
              },
              503,
            )
          : respond(system),
      );
    throw new Error(`Unexpected request: ${url}`);
  });
  vi.stubGlobal("fetch", fetchMock);
});
afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});
const renderConsole = () =>
  render(
    <ThemeProvider>
      <App />
    </ThemeProvider>,
  );

describe("SSE failure reconciliation", () => {
  it("probes system after a stream failure and exposes observation_unavailable", async () => {
    const { unmount } = renderConsole();
    await waitFor(() =>
      expect(screen.getByTestId("system-host")).toHaveTextContent(
        "sample-host",
      ),
    );
    observationFailed = true;
    act(() => EventSourceMock.instances.at(-1)?.onerror?.());
    await waitFor(() =>
      expect(screen.getByTestId("system-error")).toHaveTextContent(
        "observation_unavailable",
      ),
    );
    expect(screen.getByTestId("connection")).toHaveTextContent("offline");
    expect(screen.getByTestId("system-host")).toHaveTextContent("sample-host");
    expect(
      fetchMock.mock.calls.filter(([url]) => url === "/api/session"),
    ).toHaveLength(2);
    expect(
      fetchMock.mock.calls.filter(([url]) => url === "/api/system"),
    ).toHaveLength(2);
    unmount();
  });
  it("returns expired sessions to login and stops stream reconnects", async () => {
    renderConsole();
    await waitFor(() =>
      expect(screen.getByTestId("system-host")).toHaveTextContent(
        "sample-host",
      ),
    );
    const source = EventSourceMock.instances.at(-1)!;
    sessionExpired = true;
    act(() => source.onerror?.());
    expect(
      await screen.findByRole("heading", { name: "登录控制中心" }),
    ).toBeInTheDocument();
    expect(source.close).toHaveBeenCalled();
    expect(
      fetchMock.mock.calls.filter(([url]) => url === "/api/system"),
    ).toHaveLength(1);
    expect(screen.queryByTestId("system-host")).not.toBeInTheDocument();
  });
  it("serializes failure probes and aborts pending work on unmount", async () => {
    const { unmount } = renderConsole();
    await waitFor(() =>
      expect(screen.getByTestId("system-host")).toHaveTextContent(
        "sample-host",
      ),
    );
    let probeSignal: AbortSignal | undefined;
    fetchMock.mockImplementation((url: string, init: RequestInit) => {
      if (url === "/api/session")
        return new Promise((_resolve, reject) => {
          probeSignal = init.signal as AbortSignal;
          probeSignal.addEventListener("abort", () =>
            reject(new DOMException("aborted", "AbortError")),
          );
        });
      return Promise.resolve(respond(system));
    });
    const source = EventSourceMock.instances.at(-1)!;
    act(() => {
      source.onerror?.();
      source.onerror?.();
    });
    await waitFor(() =>
      expect(
        fetchMock.mock.calls.filter(([url]) => url === "/api/session"),
      ).toHaveLength(2),
    );
    expect(probeSignal?.aborted).toBe(false);
    unmount();
    await waitFor(() => expect(probeSignal?.aborted).toBe(true));
    expect(source.close).toHaveBeenCalled();
  });
});
