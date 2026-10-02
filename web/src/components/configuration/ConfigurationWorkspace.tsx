import { useState } from "react";
import { Badge, Button, Panel, PanelHeader } from "../ui/primitives";
import { ConfigurationSurface } from "./ConfigurationSurface";
import { configurationModules, type ConfigurationModule } from "./contracts";
import { NativeEditor } from "./NativeEditor";
import { isDirty, useConfiguration } from "./use-configuration";
import "./configuration.css";

/** All native documents share a generation and one selected-draft Commit. */
export function ConfigurationWorkspace() {
  const controller = useConfiguration();
  const [module, setModule] = useState<ConfigurationModule>("network");
  return (
    <ConfigurationSurface title="配置工作区" controller={controller}>
      <Panel>
        <PanelHeader
          title="配置文档"
          subtitle="已保存配置与草稿独立；切换文档保留本地编辑。"
        />
        <div className="table-scroll">
          <table
            className="data-table configuration-documents"
            aria-label="原生配置文档"
          >
            <thead>
              <tr>
                <th>文档</th>
                <th>范围</th>
                <th>编辑状态</th>
                <th>草稿</th>
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
                      <small className="configuration-draft-id">
                        {item.module}
                      </small>
                    </td>
                    <td>{item.description}</td>
                    <td>
                      {buffer ? (
                        <Badge tone={isDirty(buffer) ? "warning" : "neutral"}>
                          {isDirty(buffer)
                            ? buffer.content === buffer.stagedContent
                              ? "已暂存"
                              : "未暂存"
                            : `已保存 g${buffer.generation}`}
                        </Badge>
                      ) : (
                        <span className="text-muted">文档未返回</span>
                      )}
                    </td>
                    <td>{drafts.length}</td>
                    <td className="align-right">
                      <Button
                        size="small"
                        variant="ghost"
                        aria-label={`编辑 ${item.module}`}
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
