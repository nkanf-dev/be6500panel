import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useState,
  type ReactNode,
} from "react";
import { api, ApiError, runRequest } from "../lib/api";
import type { Health, ModuleInfo, SystemInfo } from "../lib/contracts";
import { connectStatusStream, type ConnectionState } from "../lib/events";

interface ConsoleContextValue {
  health?: Health;
  capabilities: readonly ModuleInfo[];
  system?: SystemInfo;
  systemError?: unknown;
  connection: ConnectionState;
  refresh: () => void;
  refreshing: boolean;
  error?: unknown;
}
const ConsoleContext = createContext<ConsoleContextValue | null>(null);
export function ConsoleProvider({
  children,
  onUnauthorized,
}: {
  children: ReactNode;
  onUnauthorized: () => void;
}) {
  const [health, setHealth] = useState<Health>();
  const [capabilities, setCapabilities] = useState<readonly ModuleInfo[]>([]);
  const [system, setSystem] = useState<SystemInfo>();
  const [systemError, setSystemError] = useState<unknown>();
  const [error, setError] = useState<unknown>();
  const [connection, setConnection] = useState<ConnectionState>("connecting");
  const [refreshing, setRefreshing] = useState(true);
  const refresh = useCallback(() => {
    setRefreshing(true);
    const handle = (error: unknown) => {
      if (error instanceof ApiError && error.status === 401) onUnauthorized();
      return error;
    };
    Promise.allSettled([
      runRequest(api.health())
        .then(setHealth)
        .catch((error) => {
          setError(handle(error));
        }),
      runRequest(api.modules())
        .then((response) => {
          setCapabilities(response.modules);
          setError(undefined);
        })
        .catch((error) => setError(handle(error))),
      runRequest(api.system())
        .then((response) => {
          setSystem(response);
          setSystemError(undefined);
        })
        .catch((error) => setSystemError(handle(error))),
    ]).finally(() => setRefreshing(false));
  }, [onUnauthorized]);
  useEffect(() => {
    refresh();
  }, [refresh]);
  useEffect(() => {
    let disposed = false;
    let probing = false;
    const controller = new AbortController();
    let disposeStream = () => {};
    const expireSession = () => {
      disposeStream();
      if (!disposed) onUnauthorized();
    };
    // EventSource hides HTTP status. Probe existing APIs once per disconnect,
    // serialized across retries. The stream remains the only reconnect timer.
    const probe = async () => {
      if (disposed || probing || document.hidden || !navigator.onLine) return;
      probing = true;
      try {
        const session = await runRequest(api.session(), controller.signal);
        if (disposed) return;
        if (session.authRequired && !session.authenticated) {
          expireSession();
          return;
        }
        const snapshot = await runRequest(api.system(), controller.signal);
        if (!disposed) {
          setSystem(snapshot);
          setSystemError(undefined);
        }
      } catch (error) {
        if (disposed) return;
        if (error instanceof ApiError && error.status === 401) expireSession();
        else setSystemError(error);
      } finally {
        probing = false;
      }
    };
    disposeStream = connectStatusStream(
      (snapshot) => {
        setSystem(snapshot);
        setSystemError(undefined);
      },
      (state) => {
        setConnection(state);
        if (state === "offline") {
          setSystemError(
            new ApiError({
              code: "status_stream_unavailable",
              message: "状态采样中断",
            }),
          );
          void probe();
        }
      },
      (error) => setSystemError(new ApiError(error)),
    );
    return () => {
      disposed = true;
      controller.abort();
      disposeStream();
    };
  }, [onUnauthorized]);
  return (
    <ConsoleContext.Provider
      value={{
        health,
        capabilities,
        system,
        systemError,
        connection,
        refresh,
        refreshing,
        error,
      }}
    >
      {children}
    </ConsoleContext.Provider>
  );
}
export function useConsole() {
  const value = useContext(ConsoleContext);
  if (!value) throw new Error("ConsoleProvider is required");
  return value;
}
