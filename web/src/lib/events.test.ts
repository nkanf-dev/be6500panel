import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { connectStatusStream } from "./events";
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
  emit(type: string, data: unknown) {
    this.listeners.get(type)?.(
      new MessageEvent(type, { data: JSON.stringify(data) }),
    );
  }
}
const system = {
  mode: "demo",
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
beforeEach(() => {
  EventSourceMock.instances = [];
  vi.useFakeTimers();
  vi.stubGlobal("EventSource", EventSourceMock);
  vi.spyOn(document, "hidden", "get").mockReturnValue(false);
  vi.spyOn(navigator, "onLine", "get").mockReturnValue(true);
});
afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
  vi.useRealTimers();
});
it("marks live only after a decoded snapshot", () => {
  const snapshot = vi.fn(),
    state = vi.fn();
  const dispose = connectStatusStream(snapshot, state);
  const source = EventSourceMock.instances[0];
  source.onopen?.();
  expect(state).toHaveBeenLastCalledWith("connecting");
  source.emit("snapshot", { system, sampledAt: system.sampledAt });
  expect(snapshot).toHaveBeenCalledWith(system);
  expect(state).toHaveBeenLastCalledWith("live");
  source.onerror?.();
  expect(state).toHaveBeenLastCalledWith("offline");
  expect(source.close).toHaveBeenCalled();
  dispose();
  expect(vi.getTimerCount()).toBe(0);
});
it("pauses streams while hidden and cleans up reconnect work", () => {
  const state = vi.fn();
  const dispose = connectStatusStream(vi.fn(), state);
  const source = EventSourceMock.instances[0];
  vi.spyOn(document, "hidden", "get").mockReturnValue(true);
  document.dispatchEvent(new Event("visibilitychange"));
  expect(source.close).toHaveBeenCalled();
  expect(state).toHaveBeenLastCalledWith("paused");
  vi.spyOn(document, "hidden", "get").mockReturnValue(false);
  document.dispatchEvent(new Event("visibilitychange"));
  expect(EventSourceMock.instances).toHaveLength(2);
  dispose();
  expect(EventSourceMock.instances[1].close).toHaveBeenCalled();
});
it("rejects invalid snapshots instead of claiming healthy data", () => {
  const snapshot = vi.fn(),
    state = vi.fn();
  const dispose = connectStatusStream(snapshot, state);
  EventSourceMock.instances[0].emit("snapshot", {
    system: { cpuCount: "wrong" },
  });
  expect(snapshot).not.toHaveBeenCalled();
  expect(state).toHaveBeenLastCalledWith("offline");
  dispose();
});
