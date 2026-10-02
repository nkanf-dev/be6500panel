import {
  Activity,
  ArrowDownLeft,
  ArrowUpRight,
  Cpu,
  HardDrive,
  Network,
  Server,
} from "lucide-react";
import { TrafficTrend, ActivityHeatmap } from "../components/visualizations";
import {
  Badge,
  Button,
  ErrorState,
  Panel,
  PanelHeader,
} from "../components/ui/primitives";
import { useConsole } from "../app/console-context";
import { errorMessage } from "../lib/api";
import { bytes, timestamp, uptime } from "../lib/format";
import { modules, type PageId } from "./registry";

export function OverviewPage({ navigate }: { navigate: (id: PageId) => void }) {
  const {
    system,
    systemError,
    health,
    capabilities,
    connection,
    trafficSamples,
    trafficSource,
  } = useConsole();
  const memoryUsed = system
    ? Math.max(0, system.memory.totalBytes - system.memory.availableBytes)
    : undefined;
  const memoryPercent =
    system && system.memory.totalBytes > 0
      ? (memoryUsed! / system.memory.totalBytes) * 100
      : undefined;
  return (
    <div className="page-stack">
      <div className="overview-stats">
        <div className="stat">
          <span className="stat-label">
            <Activity size={14} />
            系统负载 · 1 min
          </span>
          <strong>
            {system?.load[0].toFixed(2) ?? "—"}
            <small>/ {system?.cpuCount ?? "—"} 核</small>
          </strong>
          <span className="stat-context">
            {system
              ? `5 min ${system.load[1].toFixed(2)} · 15 min ${system.load[2].toFixed(2)}`
              : "等待系统采样"}
            {system && connection !== "live" && " · 上次采样"}
          </span>
        </div>
        <div className="stat">
          <span className="stat-label">
            <HardDrive size={14} />
            内存使用
          </span>
          <strong>
            {memoryPercent?.toFixed(1) ?? "—"}
            <small>%</small>
          </strong>
          <span className="stat-context">
            {memoryUsed !== undefined
              ? `${bytes(memoryUsed)} / ${bytes(system!.memory.totalBytes)}`
              : "等待系统采样"}
          </span>
        </div>
        <div className="stat">
          <span className="stat-label">
            <Server size={14} />
            持续运行
          </span>
          <strong className="stat-time">
            {system ? uptime(system.uptimeSeconds) : "—"}
          </strong>
          <span className="stat-context">
            最近采样 {timestamp(system?.sampledAt)}
          </span>
        </div>
        <div className="stat">
          <span className="stat-label">
            <Network size={14} />
            可用模块
          </span>
          <strong>
            {capabilities.filter((module) => module.state === "ready").length}
            <small>/ {capabilities.length || "—"}</small>
          </strong>
          <span className="stat-context">
            {connection === "live" ? "事件流已连接" : "事件流等待连接"}
          </span>
        </div>
      </div>
      {systemError !== undefined && (
        <ErrorState message={errorMessage(systemError)} />
      )}
      <div className="dashboard-grid">
        <TrafficTrend
          demo={health?.mode === "demo"}
          samples={trafficSamples}
          source={trafficSource}
        />
        <Panel className="environment-panel">
          <PanelHeader
            title="运行环境"
            subtitle="控制平面 · 当前进程宿主"
            action={<Cpu size={16} />}
          />
          <dl className="key-values">
            <div>
              <dt>主机名</dt>
              <dd>{system?.hostname ?? "—"}</dd>
            </div>
            <div>
              <dt>平台</dt>
              <dd>{system ? `${system.os} / ${system.arch}` : "—"}</dd>
            </div>
            <div>
              <dt>内核</dt>
              <dd className="truncate">{system?.kernel ?? "—"}</dd>
            </div>
            <div>
              <dt>数据源</dt>
              <dd>
                <Badge tone={health?.mode === "demo" ? "warning" : "neutral"}>
                  {health?.mode === "demo"
                    ? "Demo sample"
                    : health?.mode === "host"
                      ? "Host observation"
                      : "等待连接"}
                </Badge>
              </dd>
            </div>
            <div>
              <dt>写入策略</dt>
              <dd>
                {health
                  ? health.readOnly
                    ? "观察模式"
                    : "Commit 配置 / 运行管理"
                  : "—"}
              </dd>
            </div>
          </dl>
          <div className="panel-bottom">
            <Button
              variant="ghost"
              size="small"
              onClick={() => navigate("system")}
            >
              系统详情 <ArrowUpRight size={14} />
            </Button>
          </div>
        </Panel>
      </div>
      <div className="section-heading">
        <div>
          <h2>模块状态</h2>
          <p>独立能力注册 · 按需接入</p>
        </div>
        <span className="text-muted text-xs">
          {capabilities.length} 个已注册模块
        </span>
      </div>
      <Panel>
        <div className="table-scroll">
          <table className="data-table">
            <thead>
              <tr>
                <th>模块</th>
                <th>状态</th>
                <th>能力</th>
                <th className="align-right">操作</th>
              </tr>
            </thead>
            <tbody>
              {modules
                .filter((module) => module.id !== "overview")
                .map((module) => {
                  const remote = capabilities.find(
                    (item) => item.id === module.id,
                  );
                  const Icon = module.icon;
                  return (
                    <tr key={module.id}>
                      <td>
                        <span className="table-title">
                          <Icon size={16} />
                          <span>
                            <strong>{module.title}</strong>
                            <small>{module.description}</small>
                          </span>
                        </span>
                      </td>
                      <td>
                        <Badge
                          tone={
                            remote?.state === "ready" ? "success" : "neutral"
                          }
                        >
                          {!remote
                            ? "读取中"
                            : remote.state === "ready"
                              ? "就绪"
                              : "未接入"}
                        </Badge>
                      </td>
                      <td>
                        <span className="capability-list">
                          {remote?.capabilities.map((capability) => (
                            <span
                              key={capability.id}
                              className={
                                capability.supported
                                  ? "text-primary"
                                  : "text-muted"
                              }
                            >
                              {capability.title}
                            </span>
                          )) ?? "读取能力清单…"}
                        </span>
                      </td>
                      <td className="align-right">
                        <Button
                          variant="ghost"
                          size="small"
                          onClick={() => navigate(module.id)}
                        >
                          打开{" "}
                          <ArrowDownLeft className="rotate-arrow" size={14} />
                        </Button>
                      </td>
                    </tr>
                  );
                })}
            </tbody>
          </table>
        </div>
      </Panel>
      <ActivityHeatmap demo={health?.mode === "demo"} />
    </div>
  );
}
