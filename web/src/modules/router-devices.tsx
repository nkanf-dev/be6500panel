import { Badge } from "../components/ui/primitives";
import type { RouterSnapshot } from "../lib/contracts";
import { observationErrors, RouterTable, RouterTime } from "./router-frame";

export function RouterDevices({ snapshot }: { snapshot: RouterSnapshot }) {
  const arpIncomplete = observationErrors(snapshot, ["devices.arp"]).length > 0;
  return (
    <RouterTable
      title="设备观察"
      columns={["主机名", "IP 地址", "MAC 地址", "租约到期", "ARP 状态"]}
      count={snapshot.devices.length}
      emptyTitle="未观察到设备"
    >
      {snapshot.devices.map((device) => (
        <tr key={`${device.ip}-${device.mac}`}>
          <td>{device.hostname || "—"}</td>
          <td className="mono">{device.ip || "—"}</td>
          <td className="mono">{device.mac || "—"}</td>
          <td>
            <RouterTime value={device.expiresAt} />
          </td>
          <td>
            <Badge tone={device.online ? "success" : "neutral"}>
              {device.online
                ? "ARP 已观测"
                : arpIncomplete
                  ? "ARP 观察不完整"
                  : "未见 ARP"}
            </Badge>
          </td>
        </tr>
      ))}
    </RouterTable>
  );
}
