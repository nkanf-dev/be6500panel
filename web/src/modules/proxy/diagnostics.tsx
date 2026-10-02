import { Panel, PanelHeader } from "../../components/ui/primitives";
import type { ProxyNodes } from "../../lib/contracts";
import { LogsPanel } from "../logs";

export function ProxyDiagnostics({
  diagnostics = [],
}: {
  diagnostics?: ProxyNodes["diagnostics"];
}) {
  return (
    <div className="page-stack">
      <Panel>
        <PanelHeader
          title="订阅诊断"
          subtitle={`${diagnostics.length} 条校验结果`}
        />
        <div className="table-scroll">
          <table className="data-table">
            <thead>
              <tr>
                <th>范围</th>
                <th>索引</th>
                <th>状态码</th>
                <th>说明</th>
              </tr>
            </thead>
            <tbody>
              {diagnostics.map((item, index) => (
                <tr key={`${item.scope}-${item.index}-${index}`}>
                  <td>{item.scope}</td>
                  <td>{item.index < 0 ? "—" : item.index + 1}</td>
                  <td className="mono">{item.code}</td>
                  <td className="wrap">{item.message}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
        {!diagnostics.length && (
          <p className="panel-bottom text-muted">暂无导入诊断</p>
        )}
      </Panel>
      <LogsPanel />
    </div>
  );
}
