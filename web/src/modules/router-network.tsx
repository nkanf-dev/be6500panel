import type { RouterSnapshot } from "../lib/contracts";
import {
  RouterObservationFrame,
  RouterTable,
  useRouterObservation,
} from "./router-frame";

export function RouterNetworkObservations() {
  const routes = useRouterObservation(["routes"]);
  const traffic = useRouterObservation(["traffic"]);
  return (
    <>
      <RouterObservationFrame
        title="路由表"
        subtitle="内核 IPv4 / IPv6 路由观察"
        observation={routes}
        missingTitle="路由观察未接入"
      >
        {(snapshot) => <Routes snapshot={snapshot} />}
      </RouterObservationFrame>
      <RouterObservationFrame
        title="接口流量计数"
        subtitle="累计字节来自内核计数 · 速率为采样差值，首次采样为 0"
        observation={traffic}
      >
        {(snapshot) => <Traffic snapshot={snapshot} />}
      </RouterObservationFrame>
    </>
  );
}

function Routes({ snapshot }: { snapshot: RouterSnapshot }) {
  return (
    <RouterTable
      title="路由表"
      columns={["地址族", "目标网段", "网关", "接口", "Metric"]}
      count={snapshot.routes.length}
      emptyTitle="未观察到路由"
    >
      {snapshot.routes.map((route, index) => (
        <tr
          key={`${route.family}-${route.destination}-${route.interface}-${index}`}
        >
          <td className="mono">{route.family}</td>
          <td className="mono">{route.destination}</td>
          <td className="mono">{route.gateway || "—"}</td>
          <td className="mono">{route.interface || "—"}</td>
          <td className="mono align-right">{route.metric}</td>
        </tr>
      ))}
    </RouterTable>
  );
}

const counterFormat = new Intl.NumberFormat("zh-CN", {
  maximumFractionDigits: 2,
});
function Traffic({ snapshot }: { snapshot: RouterSnapshot }) {
  return (
    <RouterTable
      title="接口流量计数"
      columns={[
        "接口",
        "接收累计 (B)",
        "发送累计 (B)",
        "接收速率 (B/s)",
        "发送速率 (B/s)",
      ]}
      count={snapshot.traffic.length}
      emptyTitle="未观察到流量计数"
    >
      {snapshot.traffic.map((traffic) => (
        <tr key={traffic.interface}>
          <td className="mono">{traffic.interface}</td>
          <td className="mono align-right">
            {counterFormat.format(traffic.rxBytes)}
          </td>
          <td className="mono align-right">
            {counterFormat.format(traffic.txBytes)}
          </td>
          <td className="mono align-right">
            {counterFormat.format(traffic.rxBytesPerSecond)}
          </td>
          <td className="mono align-right">
            {counterFormat.format(traffic.txBytesPerSecond)}
          </td>
        </tr>
      ))}
    </RouterTable>
  );
}
