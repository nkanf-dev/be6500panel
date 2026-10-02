import {
  Button,
  ErrorState,
  Panel,
  PanelHeader,
} from "../../components/ui/primitives";
import { errorMessage } from "../../lib/api";
import { bytes } from "../../lib/format";
import { useProxyTelemetry } from "./use-proxy-telemetry";

/** The dashboard reads passive metrics only. It never starts a request probe. */
export function ProxyTelemetryOverview() {
  const { data, error, loading, refresh } = useProxyTelemetry();
  const usable = !!data && data.state !== "unavailable";
  const retained = data?.state === "stale" && !!data.sampledAt;
  const hasConnections =
    usable && (data.capabilities.connections.available || retained);
  const hasTraffic =
    usable && (data.capabilities.traffic.available || retained);
  const latest = hasTraffic ? data.traffic.at(-1) : undefined;
  const stale = data?.state === "stale" || (error !== undefined && !!data);
  return (
    <Panel aria-label="代理遥测">
      <PanelHeader
        title="代理遥测"
        subtitle="活动连接与代理计数器 · 不自动发起探测"
        action={
          <Button size="small" disabled={loading} onClick={refresh}>
            刷新代理采样
          </Button>
        }
      />
      <p className="panel-bottom text-muted text-xs" role="status">
        {data
          ? `${data.state === "unavailable" ? "不可用" : stale ? "上次采样（已过期）" : "实时采样"} · 来源：${data.source || "代理控制器"} · 采样时间：${data.sampledAt || "未提供"} · ${data.reason}${loading ? " · 正在刷新" : ""}`
          : loading
            ? "正在读取代理采样"
            : error !== undefined
              ? "代理采样读取失败"
              : "尚未取得代理采样"}
      </p>
      {error !== undefined && (
        <ErrorState message={errorMessage(error)} onRetry={refresh} />
      )}
      <dl className="key-values">
        <div>
          <dt>活动连接</dt>
          <dd>
            {hasConnections ? data.activeConnections : "—"}
            {hasConnections && data.truncated && " · 连接列表已截断"}
          </dd>
        </div>
        <div>
          <dt>累计上传</dt>
          <dd>{hasTraffic ? bytes(data.totals.uploadBytes) : "—"}</dd>
        </div>
        <div>
          <dt>累计下载</dt>
          <dd>{hasTraffic ? bytes(data.totals.downloadBytes) : "—"}</dd>
        </div>
        <div>
          <dt>最新上传 / 下载</dt>
          <dd>
            {latest && !latest.reset
              ? `${latest.uploadRate} / ${latest.downloadRate} B/s`
              : latest?.reset
                ? "计数器重置，等待速率采样"
                : "—"}
          </dd>
        </div>
      </dl>
      {data ? (
        <ul
          className="panel-bottom text-muted text-xs"
          aria-label="代理遥测能力"
        >
          {(
            [
              ["connections", "活动连接"],
              ["traffic", "代理流量"],
              ["routing", "活动分流"],
              ["latency", "主动探测"],
              ["requestPhases", "请求阶段"],
            ] as const
          ).map(([key, label]) => (
            <li key={key}>
              {label}：
              {usable && data.capabilities[key].available
                ? `可用${data.capabilities[key].reason ? ` · ${data.capabilities[key].reason}` : ""}`
                : `不可用 · ${data.capabilities[key].reason || data.reason}`}
            </li>
          ))}
        </ul>
      ) : (
        <p className="panel-bottom text-muted text-xs">
          等待能力采样，不使用演示数据补齐指标。
        </p>
      )}
    </Panel>
  );
}
