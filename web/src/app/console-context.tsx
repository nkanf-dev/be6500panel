import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useState,
  useRef,
  type ReactNode,
} from "react";
import { api, ApiError, runRequest } from "../lib/api";
import type {
  Health,
  ModuleInfo,
  SystemInfo,
  RouterSnapshot,
} from "../lib/contracts";
import { connectStatusStream, type ConnectionState } from "../lib/events";
import { canonicalMAC } from "../modules/devices";

interface ConsoleContextValue {
  health?: Health;
  capabilities: readonly ModuleInfo[];
  system?: SystemInfo;
  systemError?: unknown;
  router?: RouterSnapshot;
  routerError?: unknown;
  routerLoading: boolean;
  refreshRouter: () => void;
  selectedDeviceMAC?: string;
  selectDevice: (mac: string | undefined) => void;
  trafficSamples: readonly { time: string; rx: number; tx: number }[];
  trafficSource?: string;
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
  const [router, setRouter] = useState<RouterSnapshot>();
  const [routerError, setRouterError] = useState<unknown>();
  const [routerLoading, setRouterLoading] = useState(true);
  const [selectedDeviceMAC, setSelectedDeviceMAC] = useState<string>();
  const selectDevice = useCallback((mac: string | undefined) => {
    setSelectedDeviceMAC(mac ? canonicalMAC(mac) : undefined);
  }, []);
  const sampleRouter = useRef<() => void>(() => {});
  const [trafficSamples, setTrafficSamples] = useState<
    { time: string; rx: number; tx: number }[]
  >([]);
  const [trafficSource, setTrafficSource] = useState<string>();
  const trafficInterface = useRef("");
  const refreshRouter = useCallback(() => sampleRouter.current(), []);
  const [error, setError] = useState<unknown>();
  const [connection, setConnection] = useState<ConnectionState>("connecting");
  const [refreshing, setRefreshing] = useState(true);
  const refresh = useCallback(() => {
    setRefreshing(true);
    refreshRouter();
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
  }, [onUnauthorized, refreshRouter]);
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
  // One serialized router sampler for all pages. Keep only real WAN points.
  useEffect(() => {
    const controller = new AbortController();
    let active = true;
    let inFlight = false;
    const sample = async () => {
      if (inFlight || document.hidden || !navigator.onLine) return;
      inFlight = true;
      try {
        const snapshot = await runRequest(api.router(), controller.signal);
        if (!active) return;
        setRouter(snapshot);
        setRouterError(undefined);
        const defaults = snapshot.routes
          .filter(
            (route) =>
              route.destination === "0.0.0.0/0" ||
              route.destination === "default" ||
              route.destination === "::/0",
          )
          .slice()
          .sort((a, b) => a.metric - b.metric);
        const wan =
          defaults.find((route) => route.family === "ipv4")?.interface ||
          defaults[0]?.interface;
        const counter =
          snapshot.traffic.find((item) => item.interface === wan) ||
          snapshot.traffic.find((item) =>
            /^(wan|ppp|eth0\.1)/.test(item.interface),
          );
        if (
          counter &&
          !snapshot.errors.some(
            (error) =>
              error.module === "traffic" || error.module.startsWith("traffic."),
          )
        ) {
          const changed = trafficInterface.current !== counter.interface;
          trafficInterface.current = counter.interface;
          setTrafficSource(`WAN · ${counter.interface}`);
          setTrafficSamples((previous) => {
            const points = changed ? [] : previous;
            if (points.at(-1)?.time === snapshot.sampledAt) return points;
            return [
              ...points,
              {
                time: snapshot.sampledAt,
                rx: counter.rxBytesPerSecond,
                tx: counter.txBytesPerSecond,
              },
            ].slice(-300);
          });
        } else {
          trafficInterface.current = "";
          setTrafficSamples([]);
          setTrafficSource(undefined);
        }
      } catch (cause) {
        if (!active) return;
        if (cause instanceof ApiError && cause.status === 401) onUnauthorized();
        setRouterError(cause);
      } finally {
        inFlight = false;
        if (active) setRouterLoading(false);
      }
    };
    sampleRouter.current = () => {
      void sample();
    };
    setRouterLoading(true);
    void sample();
    const timer = window.setInterval(() => {
      void sample();
    }, 2000);
    const visible = () => {
      if (!document.hidden) void sample();
    };
    document.addEventListener("visibilitychange", visible);
    return () => {
      active = false;
      controller.abort();
      window.clearInterval(timer);
      sampleRouter.current = () => {};
      document.removeEventListener("visibilitychange", visible);
    };
  }, [onUnauthorized]);
  return (
    <ConsoleContext.Provider
      value={{
        health,
        capabilities,
        system,
        systemError,
        router,
        routerError,
        routerLoading,
        refreshRouter,
        selectedDeviceMAC,
        selectDevice,
        trafficSamples: routerError ? [] : trafficSamples,
        trafficSource,
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
export function useOptionalConsole() {
  return useContext(ConsoleContext);
}
export function useConsole() {
  const value = useContext(ConsoleContext);
  if (!value) throw new Error("ConsoleProvider is required");
  return value;
}
