import { useState } from "react";
import { useConsole } from "../../app/console-context";
import { TrafficTrend } from "../visualizations";
import { ChartSelect } from "../visualizations/ChartFrame";
import { Button, ErrorState } from "../ui/primitives";
import { errorMessage } from "../../lib/api";
import { formatBytes } from "../../lib/byte-scale";
import {
  TRAFFIC_HISTORY_RANGES,
  trafficHistoryRangeLabels,
  trafficHistoryRangeSeconds,
  type TrafficHistoryRange,
} from "../../lib/traffic-history-contracts";
import {
  downloadTrafficHistory,
  trafficDuration,
  trafficTimestamp,
} from "../../lib/traffic-history-format";
import { useTrafficHistory } from "./use-traffic-history";
import "./traffic-history.css";

export function TrafficHistoryPanel({
  demo = false,
  active = true,
}: {
  demo?: boolean;
  active?: boolean;
}) {
  const [range, setRange] = useState<TrafficHistoryRange>("30m");
  const { data, error, loading, reload } = useTrafficHistory(
    range,
    active && !demo,
  );
  const coverage = data
    ? Math.min(
        100,
        (data.summary.coverageSeconds / trafficHistoryRangeSeconds[range]) *
          100,
      )
    : 0;
  const first = data?.samples.find((point) => point.coverageSeconds > 0);
  const last = data?.samples
    .slice()
    .reverse()
    .find((point) => point.coverageSeconds > 0);
  const hasMeasurements = !!first;
  const rangeStart = data?.samples.length
    ? Date.parse(data.samples[0].time)
    : undefined;
  const rangeEnd =
    data?.samples.length && data.resolutionSeconds > 0
      ? Date.parse(data.samples[data.samples.length - 1].time) +
        data.resolutionSeconds * 1000
      : undefined;
  const unavailable = !active
    ? "等待服务连接"
    : loading
      ? "正在读取流量历史"
      : error
        ? "流量历史读取失败"
        : data && !data.enabled
          ? "服务器未启用流量记录"
          : "此时间范围没有真实记录";
  return (
    <div className="traffic-history" role="group" aria-label="WAN 流量历史">
      {!demo && (
        <div className="traffic-history-controls">
          <ChartSelect
            label="时间范围"
            value={range}
            onChange={(value) => setRange(value as TrafficHistoryRange)}
          >
            {TRAFFIC_HISTORY_RANGES.map((value) => (
              <option key={value} value={value}>
                {trafficHistoryRangeLabels[value]}
              </option>
            ))}
          </ChartSelect>
          <Button
            size="small"
            variant="ghost"
            onClick={reload}
            disabled={loading || !active}
          >
            刷新历史
          </Button>
          <Button
            size="small"
            variant="ghost"
            onClick={() => data && downloadTrafficHistory(data)}
            disabled={!hasMeasurements}
          >
            导出 CSV
          </Button>
          <span className="text-muted text-xs">UTC · 每 30 秒刷新</span>
        </div>
      )}
      {demo ? (
        <p className="traffic-history-note">
          演示模式：固定 1 小时样本，不代表服务器历史记录。
        </p>
      ) : (
        <>
          {error !== undefined && (
            <ErrorState
              message={`${errorMessage(error)}${data ? " · 以下为此范围上次成功读取的记录" : ""}`}
              onRetry={reload}
            />
          )}
          {data?.error && <ErrorState message={data.error} onRetry={reload} />}
          {data && (
            <>
              <dl className="traffic-history-metadata">
                <div>
                  <dt>范围 RX 总量</dt>
                  <dd title={`${data.summary.rxBytes} bytes`}>
                    {data.summary.coverageSeconds > 0
                      ? formatBytes(data.summary.rxBytes)
                      : "—"}
                  </dd>
                </div>
                <div>
                  <dt>范围 TX 总量</dt>
                  <dd title={`${data.summary.txBytes} bytes`}>
                    {data.summary.coverageSeconds > 0
                      ? formatBytes(data.summary.txBytes)
                      : "—"}
                  </dd>
                </div>
                <div>
                  <dt>有效采样覆盖</dt>
                  <dd>
                    {trafficDuration(data.summary.coverageSeconds)} ·{" "}
                    {coverage.toFixed(1)}%
                  </dd>
                </div>
                <div>
                  <dt>聚合分辨率</dt>
                  <dd>
                    {data.resolutionSeconds > 0
                      ? trafficDuration(data.resolutionSeconds)
                      : "—"}
                  </dd>
                </div>
                <div>
                  <dt>服务器保留期</dt>
                  <dd>
                    {data.retentionDays > 0 ? `${data.retentionDays} 天` : "—"}
                  </dd>
                </div>
                <div>
                  <dt>持久存储</dt>
                  <dd>
                    {data.persistent ? "持久存储已启用" : "持久存储未启用"}
                    {!data.enabled && " · 记录未启用"}
                  </dd>
                </div>
                <div className="traffic-history-wide">
                  <dt>最早保留记录（UTC）</dt>
                  <dd>
                    {data.oldestAt
                      ? trafficTimestamp(data.oldestAt)
                      : "尚无真实记录"}
                  </dd>
                </div>
              </dl>
              {data.persistent && (
                <p className="traffic-history-note">
                  持久存储已启用不表示最新测量已全部写入。
                  {data.maxUnsyncedSeconds !== undefined
                    ? `存储正常时，异常断电或进程退出可能丢失最近最多 ${trafficDuration(data.maxUnsyncedSeconds)}的未写入测量；写入故障时可能超过此窗口。`
                    : "服务未报告未写入测量的最大丢失窗口。"}
                  {data.lastFlushAt
                    ? `最近成功持久写入：${trafficTimestamp(data.lastFlushAt)}（UTC）。`
                    : "尚未报告成功持久写入时间。"}
                </p>
              )}
              {data.enabled && !data.persistent && (
                <p className="traffic-history-note" role="alert">
                  历史未写入持久存储；请检查服务存储配置和错误。重启后记录可能丢失。
                </p>
              )}
              <p className="traffic-history-note">
                历史为各时段默认路由 WAN
                的聚合记录；当前接口标签不代表每条历史记录归属同一接口。
              </p>
              <p className="traffic-history-note">
                仅显示实际记录，不补齐启用前的历史。总量来自接口计数器差值，不是速率求和。缺失或不完整时间桶在曲线上留空，已有测量仍可在数据表中查看。
              </p>
              {first && last && (
                <p className="traffic-history-note">
                  所选范围内记录：{trafficTimestamp(first.time)} 至{" "}
                  {trafficTimestamp(last.time)}（UTC 桶起点）
                  {loading && " · 刷新中"}
                </p>
              )}
            </>
          )}
        </>
      )}
      <TrafficTrend
        demo={demo}
        samples={hasMeasurements ? data?.samples : []}
        source={`服务器 WAN 聚合历史（当前接口：${data?.source || "尚未选择"}）`}
        title="WAN 流量历史"
        range={data?.range}
        resolutionSeconds={data?.resolutionSeconds}
        rangeStart={rangeStart}
        rangeEnd={rangeEnd}
        unavailable={unavailable}
      />
    </div>
  );
}

/** A no-props dashboard entry point. Production never falls back to browser-owned samples. */
export function TrafficHistoryWidget() {
  const { health } = useConsole();
  return (
    <TrafficHistoryPanel demo={health?.mode === "demo"} active={!!health} />
  );
}
