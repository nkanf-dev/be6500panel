import { strings } from "../locales/strings";
import {
  CheckCircle2,
  Circle,
  Cpu,
  FileText,
  RefreshCw,
  Terminal,
} from "lucide-react";
import {
  Badge,
  Button,
  EmptyState,
  ErrorState,
  Panel,
  PanelHeader,
} from "../components/ui/primitives";
import { useConsole } from "../app/console-context";
import { errorMessage } from "../lib/api";
import { bytes, timestamp, uptime } from "../lib/format";
import {
  ConfigurationEditor,
  ConfigurationWorkspace,
} from "../components/configuration";
import { LogsPanel } from "./logs";
import { useEffect, useState } from "react";
import { ServiceStatusPanel } from "./service-status-panel";
import { MaintenanceBackupPanel } from "../components/maintenance";

export function SystemPage() {
  const { system, systemError, connection, capabilities, refresh, refreshing } =
    useConsole();
  const [tab, setTab] = useState(
    window.location.hash.includes("tab=diagnostics")
      ? "diagnostics"
      : "resources",
  );
  useEffect(() => {
    const listener = () =>
      setTab(
        window.location.hash.includes("tab=diagnostics")
          ? "diagnostics"
          : "resources",
      );
    window.addEventListener("hashchange", listener);
    return () => window.removeEventListener("hashchange", listener);
  }, []);
  const memoryUsed = system
    ? system.memory.totalBytes - system.memory.availableBytes
    : 0;
  return (
    <div className="page-stack">
      <div className="page-toolbar">
        <div className="segmented" role="tablist" aria-label="系统视图">
          {[
            { id: "resources", label: "系统资源", icon: Cpu },
            { id: "diagnostics", label: "诊断与日志", icon: FileText },
            { id: "services", label: "服务管理", icon: Terminal },
            { id: "backup", label: "备份与导入", icon: FileText },
            { id: "configuration", label: "系统配置", icon: Terminal },
            { id: "management", label: "SSH 配置", icon: Terminal },
            { id: "changes", label: "配置变更", icon: FileText },
          ].map((item) => (
            <button
              role="tab"
              key={item.id}
              aria-selected={tab === item.id}
              onClick={() => setTab(item.id)}
            >
              <item.icon size={14} />
              {item.label}
            </button>
          ))}
        </div>
        <Button size="small" onClick={refresh} disabled={refreshing}>
          <RefreshCw size={14} className={refreshing ? "spin" : ""} />
          刷新
        </Button>
      </div>
      {systemError !== undefined && (
        <ErrorState message={errorMessage(systemError)} onRetry={refresh} />
      )}
      {tab === "backup" ? (
        <MaintenanceBackupPanel onOpenConfiguration={() => setTab("changes")} />
      ) : tab === "services" ? (
        <ServiceStatusPanel />
      ) : tab === "configuration" ? (
        <ConfigurationEditor module="system" />
      ) : tab === "management" ? (
        <ConfigurationEditor module="dropbear" />
      ) : tab === "changes" ? (
        <ConfigurationWorkspace />
      ) : tab === "resources" ? (
        system ? (
          <>
            <div className="two-column">
              <Panel>
                <PanelHeader
                  title="资源概况"
                  subtitle="最近一次系统快照"
                  action={
                    <Badge tone={connection === "live" ? "success" : "warning"}>
                      {connection === "live"
                        ? "实时"
                        : strings.dashboard.states.previous}
                    </Badge>
                  }
                />
                <div className="resource-body">
                  <div className="resource-heading">
                    <span>内存</span>
                    <strong>
                      {bytes(memoryUsed)}{" "}
                      <small>/ {bytes(system.memory.totalBytes)}</small>
                    </strong>
                  </div>
                  <div
                    className="meter"
                    role="meter"
                    aria-label="内存使用率"
                    aria-valuemin={0}
                    aria-valuemax={100}
                    aria-valuenow={
                      system.memory.totalBytes
                        ? (memoryUsed / system.memory.totalBytes) * 100
                        : 0
                    }
                  >
                    <span
                      style={{
                        width: `${system.memory.totalBytes ? (memoryUsed / system.memory.totalBytes) * 100 : 0}%`,
                      }}
                    />
                  </div>
                  <div className="resource-foot">
                    <span>可用 {bytes(system.memory.availableBytes)}</span>
                    <Badge tone="primary">{system.cpuCount} 核</Badge>
                  </div>
                  <div className="load-grid">
                    {system.load.map((value, index) => (
                      <div key={index}>
                        <strong>{value.toFixed(2)}</strong>
                        <span>{["1", "5", "15"][index]} 分钟负载</span>
                      </div>
                    ))}
                  </div>
                </div>
              </Panel>
              <Panel>
                <PanelHeader title="宿主信息" />
                <dl className="key-values">
                  <div>
                    <dt>主机名</dt>
                    <dd>{system.hostname}</dd>
                  </div>
                  <div>
                    <dt>操作系统</dt>
                    <dd>{system.os}</dd>
                  </div>
                  <div>
                    <dt>架构</dt>
                    <dd className="mono">{system.arch}</dd>
                  </div>
                  <div>
                    <dt>内核</dt>
                    <dd className="mono wrap">{system.kernel}</dd>
                  </div>
                  <div>
                    <dt>运行时长</dt>
                    <dd>{uptime(system.uptimeSeconds)}</dd>
                  </div>
                  <div>
                    <dt>采样时间</dt>
                    <dd>{timestamp(system.sampledAt)}</dd>
                  </div>
                </dl>
              </Panel>
            </div>
          </>
        ) : (
          <Panel>
            <EmptyState
              icon={<Cpu size={24} />}
              title="系统观察不可用"
              detail="查看诊断以获取状态码"
            />
          </Panel>
        )
      ) : (
        <>
          <LogsPanel />
          <Panel>
            <PanelHeader
              title="连接诊断"
              subtitle="状态与 API 接入路径"
              action={<Terminal size={16} />}
            />
            <dl className="key-values">
              <div>
                <dt>事件流</dt>
                <dd>
                  <Badge tone={connection === "live" ? "success" : "warning"}>
                    {connection}
                  </Badge>
                </dd>
              </div>
              <div>
                <dt>采样端点</dt>
                <dd className="mono">GET /api/system</dd>
              </div>
              <div>
                <dt>事件端点</dt>
                <dd className="mono">GET /api/events · snapshot</dd>
              </div>
              <div>
                <dt>最近采样</dt>
                <dd>{timestamp(system?.sampledAt)}</dd>
              </div>
              <div>
                <dt>最近异常</dt>
                <dd className="wrap">
                  {systemError ? errorMessage(systemError) : "无"}
                </dd>
              </div>
            </dl>
          </Panel>
          <Panel>
            <PanelHeader title="能力诊断" subtitle="来自后端模块注册表" />
            <div className="table-scroll">
              <table className="data-table">
                <thead>
                  <tr>
                    <th>模块</th>
                    <th>能力 ID</th>
                    <th>状态</th>
                    <th>说明</th>
                  </tr>
                </thead>
                <tbody>
                  {capabilities.flatMap((module) =>
                    module.capabilities.map((capability) => (
                      <tr key={`${module.id}-${capability.id}`}>
                        <td>{module.title}</td>
                        <td className="mono">{capability.id}</td>
                        <td>
                          <span
                            className={
                              capability.supported
                                ? "status-text text-success"
                                : "status-text text-muted"
                            }
                          >
                            {capability.supported ? (
                              <CheckCircle2 size={13} />
                            ) : (
                              <Circle size={13} />
                            )}
                            {capability.supported
                              ? "支持"
                              : strings.dashboard.states.unavailable}
                          </span>
                        </td>
                        <td className="text-muted wrap">
                          {capability.reason ?? "—"}
                        </td>
                      </tr>
                    )),
                  )}
                </tbody>
              </table>
            </div>
          </Panel>
        </>
      )}
    </div>
  );
}
