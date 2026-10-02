import { strings } from "../../locales/strings";
import { ArrowUpRight } from "lucide-react";
import { Button } from "../ui/primitives";
import type { PageId } from "../../modules/registry";
import { TrafficHistoryWidget } from "../traffic-history/TrafficHistoryPanel";
import { ProxyTelemetryOverview } from "../../modules/proxy/telemetry-overview";
import type { WidgetId } from "./layout";
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
  switch (id) {
    case "systemSummary":
      return <SystemSummaryWidget />;
    case "trafficHistory":
      return <TrafficHistoryWidget />;
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
            >{strings.dashboard.actions.proxyDetails} <ArrowUpRight size={14} />
            </Button>
          </div>
        </div>
      );
  }
}
