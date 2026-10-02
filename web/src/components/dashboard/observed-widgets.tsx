import { strings } from "../../locales/strings";
import {
  Activity,
  ArrowUpRight,
  Cpu,
  HardDrive,
  Network,
  Server,
} from "lucide-react";
import { useConsole } from "../../app/console-context";
import {
  Badge,
  Button,
  EmptyState,
  ErrorState,
  Loading,
  Panel,
  PanelHeader,
} from "../ui/primitives";
import { errorMessage } from "../../lib/api";
import { bytes, timestamp, uptime } from "../../lib/format";
import { modules, type PageId } from "../../modules/registry";

interface NavigationProps {
  navigate: (id: PageId) => void;
}

/** Focused versions of the original overview's observed system widgets. */
export function SystemSummaryWidget() {
  const { system, systemError, health, capabilities, connection } =
    useConsole();
  const memoryUsed = system
    ? Math.max(0, system.memory.totalBytes - system.memory.availableBytes)
    : undefined;
  const memoryPercent =
    system && system.memory.totalBytes > 0
      ? (memoryUsed! / system.memory.totalBytes) * 100
      : undefined;
  return (
    <Panel>
      <PanelHeader
        title="系统摘要"
        subtitle="系统快照 · 不估算 CPU 使用率"
        action={
          <Badge
            tone={
              system?.mode === "demo"
                ? "warning"
                : connection === "live"
                  ? "success"
                  : "neutral"
            }
          >
            {system?.mode === "demo"
              ? strings.dashboard.states.demo
              : system
                ? connection === "live" && systemError === undefined
                  ? strings.dashboard.states.live
                  : strings.dashboard.states.previous
                : strings.dashboard.states.waitingSample}
          </Badge>
        }
      />
      {systemError !== undefined && (
        <ErrorState message={errorMessage(systemError)} />
      )}
      {!system && systemError === undefined && (
        <p className="dashboard-data-note" role="status">
          等待系统采样，不显示虚构指标。
        </p>
      )}
      <div className="overview-stats dashboard-system-stats">
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
            {capabilities.length
              ? capabilities.filter((module) => module.state === "ready").length
              : "—"}
            <small>/ {capabilities.length || "—"}</small>
          </strong>
          <span className="stat-context">
            {health?.mode === "demo"
              ? strings.dashboard.states.demoCapabilities
              : connection === "live"
                ? strings.dashboard.states.streamConnected
                : strings.dashboard.states.streamWaiting}
          </span>
        </div>
      </div>
    </Panel>
  );
}

export function EnvironmentWidget({ navigate }: NavigationProps) {
  const { system, systemError, health } = useConsole();
  return (
    <Panel>
      <PanelHeader
        title="运行环境"
        subtitle="控制平面 · 当前进程宿主"
        action={<Cpu size={16} />}
      />
      {systemError !== undefined && (
        <ErrorState message={errorMessage(systemError)} />
      )}
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
                ? strings.dashboard.states.demo
                : health?.mode === "host"
                  ? "宿主观察"
                  : strings.dashboard.states.waitingConnection}
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
        <Button variant="ghost" size="small" onClick={() => navigate("system")}>{strings.dashboard.actions.systemDetails} <ArrowUpRight size={14} />
        </Button>
      </div>
    </Panel>
  );
}

export function ModuleStatusWidget({ navigate }: NavigationProps) {
  const { capabilities, error, refresh, refreshing, health } = useConsole();
  return (
    <Panel>
      <PanelHeader
        title="模块状态"
        subtitle="能力清单与快捷入口"
        action={
          health?.mode === "demo" ? (
            <Badge tone="warning">{strings.dashboard.states.demo}</Badge>
          ) : undefined
        }
      />
      {error !== undefined && (
        <ErrorState message={errorMessage(error)} onRetry={refresh} />
      )}
      {capabilities.length === 0 && (
        <p className="dashboard-data-note" role="status">
          {refreshing ? "正在读取能力清单…" : strings.dashboard.states.noCapabilities}
        </p>
      )}
      <div className="table-scroll">
        <table className="data-table">
          <caption className="sr-only">模块能力与入口</caption>
          <thead>
            <tr>
              <th scope="col">模块</th>
              <th scope="col">状态</th>
              <th scope="col">能力</th>
              <th scope="col" className="align-right">
                操作
              </th>
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
                    <th scope="row">
                      <span className="table-title">
                        <Icon size={16} />
                        <span>
                          <strong>{module.title}</strong>
                          <small>{module.description}</small>
                        </span>
                      </span>
                    </th>
                    <td>
                      <Badge
                        tone={remote?.state === "ready" ? "success" : "neutral"}
                      >
                        {!remote
                          ? strings.dashboard.states.unknown
                          : remote.state === "ready"
                            ? strings.dashboard.states.ready
                            : strings.dashboard.states.unavailable}
                      </Badge>
                    </td>
                    <td>
                      <span className="capability-list">
                        {remote?.capabilities.map((capability) => (
                          <span
                            key={capability.id}
                            title={capability.reason}
                            className={
                              capability.supported
                                ? "text-primary"
                                : "text-muted"
                            }
                          >
                            {capability.title}
                          </span>
                        )) ?? strings.dashboard.states.capabilityNotRead}
                      </span>
                    </td>
                    <td className="align-right">
                      <Button
                        variant="ghost"
                        size="small"
                        aria-label={`打开${module.title}`}
                        onClick={() => navigate(module.id)}
                      >{strings.dashboard.actions.open} <ArrowUpRight size={14} />
                      </Button>
                    </td>
                  </tr>
                );
              })}
          </tbody>
        </table>
      </div>
    </Panel>
  );
}

export function DevicesWidget({ navigate }: NavigationProps) {
  const { router, routerError, routerLoading, refreshRouter, health } =
    useConsole();
  const errors =
    router?.errors.filter(
      (error) =>
        error.module === "devices" || error.module.startsWith("devices."),
    ) ?? [];
  const arpIncomplete = errors.some((error) => error.module === "devices.arp");
  const observed =
    router?.devices.filter((device) => device.online).length ?? 0;
  return (
    <Panel>
      <PanelHeader
        title="设备观察"
        subtitle="当前租约与 ARP 状态 · 不代表实时在线"
        action={
          <Badge tone={health?.mode === "demo" ? "warning" : "neutral"}>
            {health?.mode === "demo" ? strings.dashboard.states.demo : "路由器观察"}
          </Badge>
        }
      />
      {routerError !== undefined && (
        <ErrorState
          message={errorMessage(routerError)}
          onRetry={refreshRouter}
        />
      )}
      {errors.map((error, index) => (
        <ErrorState
          key={`${error.code}-${index}`}
          message={`${error.message} · ${error.code}`}
          onRetry={refreshRouter}
        />
      ))}
      {!router && routerLoading && routerError === undefined && (
        <Loading label="正在读取设备观察" />
      )}
      {!router && !routerLoading && routerError === undefined && (
        <EmptyState
          title="未取得设备观察"
          detail="等待路由器快照，不使用演示设备填充。"
        />
      )}
      {router && (
        <>
          <p className="dashboard-data-note">
            {routerError !== undefined ? "上次采样 · " : ""}租约{" "}
            {router.devices.length} · ARP 已观测 {observed}
            {arpIncomplete ? "（观察不完整）" : ""} · 采样{" "}
            {timestamp(router.sampledAt)}
          </p>
          {router.devices.length === 0 ? (
            <EmptyState
              title={errors.length ? "设备观察不完整" : "未观察到设备"}
              detail="没有可展示的设备记录。ARP 未出现不等于设备离线。"
            />
          ) : (
            <ul className="dashboard-device-list">
              {router.devices.slice(0, 5).map((device) => (
                <li key={`${device.ip}-${device.mac}`}>
                  <span>
                    <strong>
                      {device.hostname || device.ip || "未命名设备"}
                    </strong>
                    <small className="mono">{device.ip || "—"}</small>
                  </span>
                  <Badge tone={device.online ? "success" : "neutral"}>
                    {device.online
                      ? "ARP 已观测"
                      : arpIncomplete
                        ? "ARP 观察不完整"
                        : "未见 ARP"}
                  </Badge>
                </li>
              ))}
            </ul>
          )}
          {router.devices.length > 5 && (
            <p className="dashboard-data-note">
              显示前 5 条，共 {router.devices.length} 条；在设备页查看全部。
            </p>
          )}
        </>
      )}
      <div className="panel-bottom">
        <Button
          variant="ghost"
          size="small"
          onClick={() => navigate("devices")}
        >{strings.dashboard.actions.deviceDetails} <ArrowUpRight size={14} />
        </Button>
      </div>
    </Panel>
  );
}
