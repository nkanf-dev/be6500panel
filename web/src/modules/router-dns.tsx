import type { RouterSnapshot } from "../lib/contracts";
import { observationErrors, RouterTable } from "./router-frame";

export function RouterDns({ snapshot }: { snapshot: RouterSnapshot }) {
  const leaseErrors = observationErrors(snapshot, ["devices.leases"]);
  const leaseUnavailable = leaseErrors.some(
    (error) => error.code !== "invalid",
  );
  return (
    <>
      <div className="table-footer">
        <span>
          DHCP 租约数：{leaseUnavailable ? "—" : snapshot.dns.leaseCount}
        </span>
      </div>
      <RouterTable
        title="DNS 解析器"
        columns={["解析器地址"]}
        count={snapshot.dns.resolvers.length}
        emptyTitle="未观察到解析器"
      >
        {snapshot.dns.resolvers.map((resolver) => (
          <tr key={resolver}>
            <td className="mono">{resolver}</td>
          </tr>
        ))}
      </RouterTable>
    </>
  );
}
