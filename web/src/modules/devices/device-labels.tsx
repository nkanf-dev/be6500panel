import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { ApiError, runRequest } from "../../lib/api";
import {
  deviceAnnotationsAPI,
  type DeviceAnnotation,
  type DeviceAnnotations,
  type SaveDeviceAnnotation,
} from "./annotations-api";

export const DEVICE_LABELS_CHANGED = "be6500panel:device-labels-changed";
export function canonicalMAC(value: string): string | undefined {
  const normalized = value.trim().replaceAll("-", ":").toUpperCase();
  return /^(?:[0-9A-F]{2}:){5}[0-9A-F]{2}$/.test(normalized)
    ? normalized
    : undefined;
}
export function deviceDisplayName(
  mac: string,
  hostname: string | undefined,
  annotations: Readonly<Record<string, DeviceAnnotation>> = {},
): string {
  const key = canonicalMAC(mac);
  return (
    (key ? annotations[key]?.label.trim() : "") ||
    hostname?.trim() ||
    key ||
    mac ||
    "未命名设备"
  );
}
interface DeviceLabelsValue {
  annotations: Readonly<Record<string, DeviceAnnotation>>;
  revision: number;
  loading: boolean;
  error?: unknown;
  refresh: () => void;
  save: (input: SaveDeviceAnnotation) => Promise<void>;
  displayName: (mac: string, hostname?: string) => string;
}
const DeviceLabelsContext = createContext<DeviceLabelsValue | null>(null);
const emptyAnnotations: DeviceAnnotations = { revision: 0, devices: {} };
/** One authenticated console provider. Notes stay in session memory, never browser storage. */
export function DeviceLabelsProvider({
  children,
  initial,
}: {
  children: ReactNode;
  initial?: DeviceAnnotations;
}) {
  const [data, setData] = useState<DeviceAnnotations>(
    initial ?? emptyAnnotations,
  );
  const [loading, setLoading] = useState(!initial);
  const [error, setError] = useState<unknown>();
  const [tick, setTick] = useState(0);
  const mounted = useRef(false);
  const expired = useRef(false);
  const requests = useRef(new Set<AbortController>());
  const refresh = useCallback(() => {
    if (!expired.current) setTick((value) => value + 1);
  }, []);
  const publish = useCallback((next: DeviceAnnotations) => {
    if (!mounted.current || expired.current) return;
    setData((previous) =>
      next.revision >= previous.revision ? next : previous,
    );
    window.dispatchEvent(new CustomEvent(DEVICE_LABELS_CHANGED));
  }, []);
  useEffect(() => {
    mounted.current = true;
    expired.current = false;
    const clear = () => {
      expired.current = true;
      for (const request of requests.current) request.abort();
      requests.current.clear();
      setData(emptyAnnotations);
      setLoading(false);
      setError(undefined);
      window.dispatchEvent(new CustomEvent(DEVICE_LABELS_CHANGED));
    };
    window.addEventListener("be6500panel:unauthorized", clear);
    window.addEventListener("be6500panel:logout", clear);
    return () => {
      mounted.current = false;
      for (const request of requests.current) request.abort();
      requests.current.clear();
      window.removeEventListener("be6500panel:unauthorized", clear);
      window.removeEventListener("be6500panel:logout", clear);
    };
  }, []);
  useEffect(() => {
    if (expired.current || (initial && tick === 0)) return;
    const controller = new AbortController();
    requests.current.add(controller);
    let current = true;
    setLoading(true);
    void runRequest(deviceAnnotationsAPI.get(), controller.signal)
      .then((next) => {
        if (current && !expired.current) {
          publish(next);
          setError(undefined);
        }
      })
      .catch((cause) => {
        if (current && !expired.current) setError(cause);
      })
      .finally(() => {
        requests.current.delete(controller);
        if (current && !expired.current) setLoading(false);
      });
    return () => {
      current = false;
      controller.abort();
      requests.current.delete(controller);
    };
  }, [initial, tick, publish]);
  const save = useCallback(
    async (input: SaveDeviceAnnotation) => {
      if (expired.current || !mounted.current)
        throw new ApiError({
          code: "session_expired",
          message: "请重新登录后保存备注",
        });
      const mac = canonicalMAC(input.mac);
      if (!mac)
        throw new ApiError({
          code: "invalid_mac",
          message: "选择有效的 MAC 设备后保存备注",
        });
      const controller = new AbortController();
      requests.current.add(controller);
      try {
        const next = await runRequest(
          deviceAnnotationsAPI.save({ ...input, mac }),
          controller.signal,
        );
        if (expired.current || !mounted.current)
          throw new ApiError({
            code: "session_expired",
            message: "请重新登录并检查备注状态",
          });
        publish(next);
        setError(undefined);
      } finally {
        requests.current.delete(controller);
      }
    },
    [publish],
  );
  const displayName = useCallback(
    (mac: string, hostname?: string) =>
      deviceDisplayName(mac, hostname, data.devices),
    [data.devices],
  );
  return (
    <DeviceLabelsContext.Provider
      value={{
        annotations: data.devices,
        revision: data.revision,
        loading,
        error,
        refresh,
        save,
        displayName,
      }}
    >
      {children}
    </DeviceLabelsContext.Provider>
  );
}
const fallback: DeviceLabelsValue = {
  annotations: {},
  revision: 0,
  loading: false,
  refresh: () => {},
  save: async () => {
    throw new ApiError({
      code: "annotations_not_mounted",
      message: "设备备注服务尚未连接，请刷新页面",
    });
  },
  displayName: (mac, hostname) => deviceDisplayName(mac, hostname),
};
export function useDeviceLabels(): DeviceLabelsValue {
  return useContext(DeviceLabelsContext) ?? fallback;
}
export function DeviceLabel({
  mac,
  hostname,
}: {
  mac: string;
  hostname?: string;
}) {
  return <>{useDeviceLabels().displayName(mac, hostname)}</>;
}
