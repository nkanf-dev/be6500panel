import { strings } from "../../locales/strings";
import { useState } from "react";
import { Badge, Button, Panel, PanelHeader } from "../ui/primitives";
import { ConfigurationSurface } from "./ConfigurationSurface";
import { configurationModules, type ConfigurationModule } from "./contracts";
import { NativeEditor } from "./NativeEditor";
import { isDirty, useConfiguration } from "./use-configuration";
import "./configuration.css";

/** Selected changes across native documents are applied together. */
export function ConfigurationWorkspace() {
  const controller = useConfiguration();
  const [module, setModule] = useState<ConfigurationModule>("network");
  return (
    <ConfigurationSurface title="配置工作区" controller={controller}>
      <Panel>
        <PanelHeader
          title="配置文档"
          subtitle="编辑和检查不会改变当前配置；应用更改后生效。切换文档会保留本地编辑。"
        />
        <div className="table-scroll">
          <table
            className="data-table configuration-documents"
            aria-label="配置文档"
          >
            <thead>
              <tr>
                <th>文档</th>
                <th>范围</th>
                <th>编辑状态</th>
                <th>{strings.configuration.queueTitle}</th>
                <th className="align-right">操作</th>
              </tr>
            </thead>
            <tbody>
              {configurationModules.map((item) => {
                const buffer = controller.buffers[item.module];
                const drafts = controller.drafts.filter(
                  (draft) => draft.module === item.module,
                );
                return (
                  <tr
                    key={item.module}
                    className={
                      module === item.module ? "row-selected" : undefined
                    }
                  >
                    <td>
                      <strong>{item.label}</strong>
                      <details className="configuration-advanced-details">
                        <summary>{strings.configuration.advancedDetails}</summary>
                        <p>
                          原生文档：<code>{item.module}</code>
                        </p>
                        {buffer && (
                          <p>
                            配置版本：<code>{buffer.generation}</code>
                          </p>
                        )}
                      </details>
                    </td>
                    <td>{item.description}</td>
                    <td>
                      {buffer ? (
                        <Badge tone={isDirty(buffer) ? "warning" : "neutral"}>
                          {isDirty(buffer)
                            ? buffer.content === buffer.stagedContent
                              ? "已检查，尚未应用"
                              : "有未检查更改"
                            : "与当前配置一致"}
                        </Badge>
                      ) : (
                        <span className="text-muted">暂不可编辑</span>
                      )}
                    </td>
                    <td>{drafts.length}</td>
                    <td className="align-right">
                      <Button
                        size="small"
                        variant="ghost"
                        aria-label={`编辑${item.label}配置`}
                        disabled={!buffer}
                        onClick={() => setModule(item.module)}
                      >
                        编辑
                      </Button>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      </Panel>
      <NativeEditor key={module} module={module} controller={controller} />
    </ConfigurationSurface>
  );
}
