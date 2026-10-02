import type { RouterSnapshot } from "../lib/contracts";
import { observationErrors, RouterTable } from "./router-frame";

export function RouterFirewall({ snapshot }: { snapshot: RouterSnapshot }) {
  return (
    <RouterTable
      title="防火墙策略观察"
      columns={[
        "地址族",
        "INPUT 策略",
        "FORWARD 策略",
        "OUTPUT 策略",
        "规则数",
      ]}
      count={2}
      emptyTitle="未观察到防火墙策略"
    >
      {(["ipv4", "ipv6"] as const).map((family) => {
        const policies = snapshot.firewall[family];
        const errors = observationErrors(snapshot, [`firewall.${family}`]);
        const unavailable = errors.some((error) => error.code !== "invalid");
        return (
          <tr key={family}>
            <td className="mono">{family}</td>
            <td className="mono">{policies.input || "—"}</td>
            <td className="mono">{policies.forward || "—"}</td>
            <td className="mono">{policies.output || "—"}</td>
            <td className="mono align-right">
              {unavailable ? "—" : policies.rules}
            </td>
          </tr>
        );
      })}
    </RouterTable>
  );
}
