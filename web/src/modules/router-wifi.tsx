import { Badge } from "../components/ui/primitives";
import type { RouterSnapshot } from "../lib/contracts";
import { RouterTable } from "./router-frame";

export function RouterWifi({ snapshot }: { snapshot: RouterSnapshot }) {
  return (
    <RouterTable
      title="无线配置观察"
      columns={[
        "名称",
        "SSID",
        "频段",
        "配置信道",
        "带宽",
        "加密方式",
        "配置状态",
      ]}
      count={snapshot.wifi.length}
      emptyTitle="未观察到无线配置"
    >
      {snapshot.wifi.map((wifi, index) => (
        <tr key={`${wifi.name}-${index}`}>
          <td className="mono">{wifi.name || "—"}</td>
          <td>{wifi.ssid || "—"}</td>
          <td>{wifi.band || "—"}</td>
          <td className="mono">{wifi.channel || "自动 / 未知"}</td>
          <td className="mono">{wifi.bandwidth || "—"}</td>
          <td className="mono">{wifi.encryption || "—"}</td>
          <td>
            <Badge tone={wifi.disabled ? "neutral" : "success"}>
              {wifi.disabled ? "禁用" : "启用"}
            </Badge>
          </td>
        </tr>
      ))}
    </RouterTable>
  );
}
