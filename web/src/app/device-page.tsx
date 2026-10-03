import { useCallback, useEffect, useState } from "react";
import { useConsole } from "./console-context";
import { useDeviceWorkspaceHistory } from "./use-device-workspace-history";
import { DeviceWorkspace, useDeviceLabels } from "../modules/devices";
import { DeviceActivityPanel } from "../components/device-activity";
import { ConfigurationEditor } from "../components/configuration";
import { Button } from "../components/ui/primitives";
import type { DeviceActivityRange } from "../lib/device-activity-contracts";
import { useProxyTelemetry } from "../modules/proxy/use-proxy-telemetry";

export function DevicePage() {
  const {
    router,
    routerError,
    routerLoading,
    refreshRouter,
    selectedDeviceMAC,
    selectDevice,
  } = useConsole();
  const labels = useDeviceLabels();
  const proxy = useProxyTelemetry();
  const [range, setRange] = useState<DeviceActivityRange>("24h");
  const [selected, setSelected] = useState<readonly string[]>(
    selectedDeviceMAC ? [selectedDeviceMAC] : [],
  );
  const changeSelected = useCallback((macs: readonly string[]) => {
    setSelected((previous) =>
      previous.join("|") === macs.join("|") ? previous : macs,
    );
  }, []);
  const [search, setSearch] = useState("");
  const [sourceSearch, setSourceSearch] = useState("");
  useEffect(() => {
    const timer = window.setTimeout(() => setSourceSearch(search), 250);
    return () => window.clearTimeout(timer);
  }, [search]);
  const activity = useDeviceWorkspaceHistory(range, selected, sourceSearch);
  const [configuration, setConfiguration] = useState<"dhcp" | "firewall">();
  return (
    <div className="page-stack">
      {configuration ? (
        <>
          <Button onClick={() => setConfiguration(undefined)}>
            返回设备管理
          </Button>
          <ConfigurationEditor module={configuration} />
        </>
      ) : (
        <>
          <DeviceWorkspace
            snapshot={router}
            snapshotError={routerError}
            activity={activity.data}
            activityError={activity.error}
            proxy={proxy.data}
            proxyError={proxy.error}
            loading={routerLoading || activity.loading}
            onRefresh={() => {
              refreshRouter();
              activity.refresh();
              proxy.refresh();
            }}
            range={range}
            onRangeChange={setRange}
            selectedMAC={selectedDeviceMAC}
            onSelectDevice={selectDevice}
            onSelectedDevicesChange={changeSelected}
            onSearchChange={setSearch}
            onConfigure={(module) => setConfiguration(module)}
          />
          <DeviceActivityPanel
            getDeviceName={labels.displayName}
            onSelectDevice={selectDevice}
          />
        </>
      )}
    </div>
  );
}
