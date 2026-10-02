import { Badge } from "../components/ui/primitives";
import type { RouterSnapshot } from "../lib/contracts";

export function RouterObservationErrors({
  errors,
}: {
  errors: RouterSnapshot["errors"];
}) {
  if (!errors.length) return null;
  return (
    <div className="table-scroll" role="alert" aria-label="观察来源错误">
      <table className="data-table" aria-label="观察来源错误">
        <thead>
          <tr>
            <th>来源模块</th>
            <th>错误码</th>
            <th>消息</th>
          </tr>
        </thead>
        <tbody>
          {errors.map((error, index) => (
            <tr key={`${error.module}-${error.code}-${index}`}>
              <td className="mono">{error.module}</td>
              <td>
                <Badge tone="warning">{error.code}</Badge>
              </td>
              <td className="wrap">{error.message}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
