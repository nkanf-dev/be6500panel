import { useState } from "react";
import { ConfigurationEditor } from "../components/configuration";
import { RouterViewTabs, type RouterView } from "./router-view-tabs";
import { RouterDevices } from "./router-devices";
import { RouterDns } from "./router-dns";
import { RouterFirewall } from "./router-firewall";
import {
  RouterObservationFrame,
  RouterToolbar,
  useRouterObservation,
} from "./router-frame";
import { moduleById, type PageId } from "./registry";
import { RouterWifi } from "./router-wifi";

const observationPages = {
  devices: {
    modules: ["devices"],
    description: "DHCP 租约与 ARP 记录 · ARP 状态不等同于可达性探测",
    component: RouterDevices,
  },
  wifi: {
    modules: ["wifi"],
    description: "UCI 无线配置 · 配置信道不等同于实际射频信道",
    component: RouterWifi,
    configurationModule: "wireless",
  },
  dns: {
    modules: ["dns", "devices.leases"],
    description: "上游解析器与 DHCP 租约计数",
    component: RouterDns,
    configurationModule: "dhcp",
  },
  firewall: {
    modules: ["firewall"],
    description: "filter 默认策略 · 规则数为各表总计",
    component: RouterFirewall,
    configurationModule: "firewall",
  },
} as const;

// Keep the export name used by existing application routes.
export function UnavailablePage({ id }: { id: PageId }) {
  const page = observationPages[id as keyof typeof observationPages];
  const observation = useRouterObservation(page?.modules ?? []);
  const [view, setView] = useState<RouterView>("observation");
  if (!page) return null;
  const configurationModule =
    "configurationModule" in page ? page.configurationModule : undefined;
  const Content = page.component;
  return (
    <div className="page-stack">
      {configurationModule && (
        <div className="page-toolbar">
          <RouterViewTabs
            label={`${moduleById(id).title}视图`}
            value={view}
            onChange={setView}
          />
        </div>
      )}
      {configurationModule && view === "configuration" ? (
        <ConfigurationEditor
          key={configurationModule}
          module={configurationModule}
        />
      ) : (
        <>
          <RouterToolbar observation={observation} />
          <RouterObservationFrame
            title={moduleById(id).title}
            subtitle={page.description}
            observation={observation}
          >
            {(snapshot) => <Content snapshot={snapshot} />}
          </RouterObservationFrame>
        </>
      )}
    </div>
  );
}
