import { strings } from "../../locales/strings";
import { ArrowUpRight } from "lucide-react";
import { Button } from "../ui/primitives";
import type { PageId } from "../../modules/registry";
import { TrafficHistoryWidget } from "../traffic-history/TrafficHistoryPanel";
import { ProxyTelemetryOverview } from "../../modules/proxy/telemetry-overview";
import type { WidgetId } from "./layout";
import { useConsole } from "../../app/console-context";
import { DeviceActivityPanel } from "../device-activity";
import { RequestWaterfall } from "../visualizations";
import { useDeviceLabels } from "../../modules/devices";
import { NetworkDiagnosticPanel } from "../../modules/proxy/network-diagnostic-panel";
import {
  DevicesWidget,
  EnvironmentWidget,
  ModuleStatusWidget,
  SystemSummaryWidget,
} from "./observed-widgets";

export function DashboardWidget({
  id,
  navigate,
}: {
  navigate: (id: PageId) => void;
  id: WidgetId;
}) {
  const { health, selectDevice } = useConsole();
  const labels = useDeviceLabels();
  switch (id) {
    case "systemSummary":
      return <SystemSummaryWidget />;
    case "trafficHistory":
      return <TrafficHistoryWidget />;
    case "deviceActivity":
      return (
        <DeviceActivityPanel
          active={health?.mode !== "demo"}
          getDeviceName={labels.displayName}
          onSelectDevice={(mac) => {
            selectDevice?.(mac);
            navigate("devices");
          }}
        />
      );
    case "networkDiagnostics":
      return health?.mode === "demo" ? (
        <RequestWaterfall />
      ) : (
        <NetworkDiagnosticPanel />
      );
    case "environment":
      return <EnvironmentWidget navigate={navigate} />;
    case "moduleStatus":
      return <ModuleStatusWidget navigate={navigate} />;
    case "devices":
      return <DevicesWidget navigate={navigate} />;
    case "proxy":
      return (
        <div className="dashboard-proxy-widget">
          <ProxyTelemetryOverview />
          <div className="dashboard-proxy-link">
            <Button
              variant="ghost"
              size="small"
              onClick={() => navigate("proxy")}
            >
              {strings.dashboard.actions.proxyDetails}{" "}
              <ArrowUpRight size={14} />
            </Button>
          </div>
        </div>
      );
  }
}
