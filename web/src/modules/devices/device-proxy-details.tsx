import {
  Badge,
  Button,
  EmptyState,
  ErrorState,
  Panel,
  PanelHeader,
} from "../../components/ui/primitives";
import { errorMessage } from "../../lib/api";
import type { RouterSnapshot } from "../../lib/contracts";
import { bytes } from "../../lib/format";
import type { ProxyMetrics } from "../proxy/telemetry-api";
import { useDeviceLabels } from "./device-labels";
import { correlateDeviceProxy, type WorkspaceDevice } from "./device-model";

export function DeviceProxyDetails({
  devices,
  inventory,
  metrics,
  error,
  snapshot,
  onRefresh,
}: {
  devices: readonly WorkspaceDevice[];
  inventory: readonly WorkspaceDevice[];
  metrics?: ProxyMetrics;
  error?: unknown;
  snapshot?: RouterSnapshot;
  onRefresh?: () => void;
}) {
  const labels = useDeviceLabels();
  const observations = devices.map((device) => ({
    device,
    value: correlateDeviceProxy(
      device.mac,
      inventory,
      metrics,
      Date.now(),
      error !== undefined,
    ),
  }));
  const first = observations[0]?.value;
  const rows = observations.flatMap(({ device, value }) =>
    value.connections.map((connection) => ({ device, connection })),
  );
  const available =
    !!metrics &&
    metrics.state !== "unavailable" &&
    metrics.capabilities.connections.available &&
    !first?.stale;
  const defaults =
    snapshot?.routes
      .filter(
        (route) =>
          route.destination === "0.0.0.0/0" ||
          route.destination === "default" ||
          route.destination === "::/0",
      )
      .slice()
      .sort((a, b) => a.metric - b.metric) ?? [];
  const snapshotAge = snapshot
    ? (Date.now() - Date.parse(snapshot.sampledAt)) / 1000
    : undefined;
  const freshRoute =
    snapshotAge !== undefined &&
    Number.isFinite(snapshotAge) &&
    snapshotAge >= -5 &&
    snapshotAge <= 30 &&
    !snapshot?.errors.some(
      (entry) =>
        entry.module === "routes" || entry.module.startsWith("routes."),
    );
  return (
    <Panel>
      <PanelHeader
        title="设备核心连接与出口"
        subtitle="按当前唯一 IP → MAC 关联已观测核心连接，不代表设备全部请求"
        action={
          onRefresh && (
            <Button size="small" onClick={onRefresh}>
              刷新连接
            </Button>
          )
        }
      />
      <p className="device-source-note" role="status">
        来源：{first?.source || "代理控制器"} · 采样时间：
        {first?.sampledAt
          ? new Date(first.sampledAt).toLocaleString("zh-CN")
          : "等待采样"}{" "}
        · 源数据年龄：
        {first?.sourceAgeSeconds !== undefined &&
        Number.isFinite(first.sourceAgeSeconds)
          ? `${Math.floor(first.sourceAgeSeconds)} 秒`
          : "未知"}
        {first?.stale ? " · 已过期 / 尚未可用" : ""}
        {metrics?.truncated ? " · 核心列表已截断" : ""}
      </p>
      {error !== undefined && (
        <ErrorState
          message={`代理连接读取失败：${errorMessage(error)}`}
          onRetry={onRefresh}
        />
      )}
      {available ? (
        <>
          <div className="device-proxy-summary">
            <Badge>{rows.length} 个匹配的已观测连接</Badge>
            <Badge tone="primary">
              代理出口{" "}
              {rows.filter((row) => row.connection.outbound === "proxy").length}
            </Badge>
            <Badge>
              直连出口{" "}
              {
                rows.filter((row) => row.connection.outbound === "direct")
                  .length
              }
            </Badge>
            {first && first.ambiguousCount > 0 && (
              <Badge tone="warning">
                {first.ambiguousCount} 个地址归属不唯一的核心连接未关联
              </Badge>
            )}
          </div>
          {rows.length ? (
            <div className="table-scroll">
              <table className="data-table">
                <thead>
                  <tr>
                    <th>设备 / 源地址</th>
                    <th>目标 / 协议</th>
                    <th>核心出口</th>
                    <th>已观测字节</th>
                    <th>开始时间 / 核心规则</th>
                  </tr>
                </thead>
                <tbody>
                  {rows.map(({ device, connection }) => (
                    <tr key={`${device.mac}-${connection.id}`}>
                      <th scope="row">
                        {labels.displayName(device.mac, device.hostname)}
                        <small className="device-subtitle mono">
                          {connection.sourceIP}:{connection.sourcePort}
                        </small>
                      </th>
                      <td>
                        {connection.host || connection.destinationIP}
                        <small className="device-subtitle">
                          {connection.destinationIP}:
                          {connection.destinationPort} ·{" "}
                          {connection.network.toUpperCase()}
                        </small>
                      </td>
                      <td>
                        <Badge
                          tone={
                            connection.outbound === "proxy"
                              ? "primary"
                              : "neutral"
                          }
                        >
                          {connection.outbound === "proxy"
                            ? "代理"
                            : connection.outbound === "direct"
                              ? "直连"
                              : "核心未导出"}
                        </Badge>
                      </td>
                      <td>
                        上传 {bytes(connection.uploadBytes)}
                        <small className="device-subtitle">
                          下载 {bytes(connection.downloadBytes)}
                        </small>
                      </td>
                      <td>
                        {new Date(connection.startedAt).toLocaleString("zh-CN")}
                        <small className="device-subtitle">
                          {connection.rule || "未导出规则"}
                        </small>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          ) : (
            <EmptyState
              title="当前没有匹配的核心连接"
              detail="等待该设备产生新连接，或刷新设备地址与代理核心采样。此处只展示核心已观测连接。"
            />
          )}
        </>
      ) : (
        <EmptyState
          title="等待有效的设备核心连接采样"
          detail={
            first?.reason ||
            "刷新代理核心与当前设备地址后，可查看已观测连接及出口。"
          }
        >
          {onRefresh && (
            <Button size="small" onClick={onRefresh}>
              刷新采样
            </Button>
          )}
        </EmptyState>
      )}
      <details className="device-hardware-details">
        <summary>路由表出口参考</summary>
        {freshRoute && defaults.length ? (
          <div className="table-scroll">
            <table className="data-table">
              <caption>内核当前默认路由 · 非逐设备分流命中</caption>
              <thead>
                <tr>
                  <th>地址族</th>
                  <th>出口接口</th>
                  <th>网关</th>
                  <th>Metric</th>
                </tr>
              </thead>
              <tbody>
                {defaults.map((route, index) => (
                  <tr key={`${route.family}-${route.interface}-${index}`}>
                    <td>{route.family}</td>
                    <td>{route.interface}</td>
                    <td>{route.gateway || "直连接口"}</td>
                    <td>{route.metric}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        ) : (
          <p className="device-source-note">
            {snapshot
              ? "路由采样缺失或已过期，刷新后查看当前出口。"
              : "读取路由快照后显示内核默认出口。"}
          </p>
        )}
        <p className="device-source-note">
          核心出口来自连接行实际观测；默认路由用于网络排查，不作为此设备代理成功的判断。
        </p>
      </details>
    </Panel>
  );
}
