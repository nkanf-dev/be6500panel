import { Schema } from "effect";
import { SnapshotSchema, type SystemInfo } from "./contracts";

export type ConnectionState = "connecting" | "live" | "offline" | "paused";
/** One stream per console. Close while hidden/offline and use bounded reconnect backoff. */
export function connectStatusStream(
  onSnapshot: (system: SystemInfo) => void,
  onState: (state: ConnectionState) => void,
  onObservationError?: (error: { code: string; message: string }) => void,
) {
  let source: EventSource | undefined;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let disposed = false;
  let failures = 0;
  const close = () => {
    source?.close();
    source = undefined;
    if (timer) clearTimeout(timer);
    timer = undefined;
  };
  const connect = () => {
    close();
    if (disposed) return;
    if (document.hidden) {
      onState("paused");
      return;
    }
    if (!navigator.onLine) {
      onState("offline");
      return;
    }
    onState("connecting");
    source = new EventSource("/api/events");
    source.onopen = () => onState("connecting");
    source.addEventListener("snapshot", (event) => {
      try {
        onSnapshot(
          Schema.decodeUnknownSync(SnapshotSchema)(
            JSON.parse((event as MessageEvent).data),
          ).system,
        );
        failures = 0;
        onState("live");
      } catch {
        onState("offline");
      }
    });
    source.addEventListener("observation_error", (event) => {
      try {
        const envelope = JSON.parse((event as MessageEvent).data) as {
          error: { code: string; message: string };
        };
        onObservationError?.(envelope.error);
      } catch {
        onObservationError?.({
          code: "invalid_event",
          message: "观察事件无效",
        });
      }
    });
    source.onerror = () => {
      close();
      onState("offline");
      const delay = Math.min(30_000, 1000 * 2 ** Math.min(failures++, 5));
      timer = setTimeout(connect, delay);
    };
  };
  const visibility = () => connect();
  document.addEventListener("visibilitychange", visibility);
  window.addEventListener("online", connect);
  window.addEventListener("offline", connect);
  connect();
  return () => {
    disposed = true;
    close();
    document.removeEventListener("visibilitychange", visibility);
    window.removeEventListener("online", connect);
    window.removeEventListener("offline", connect);
  };
}
