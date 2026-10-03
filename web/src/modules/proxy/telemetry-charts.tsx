import { useMemo } from "react";
import { useConnectionDeviceNames } from "../../app/use-connection-device-names";
import type { EChartsOption } from "echarts";
import { ChartFrame } from "../../components/visualizations/ChartFrame";
import { EChart } from "../../components/visualizations/EChart";
import {
  axisStyle,
  baseOption,
  zoomOption,
  type ChartPalette,
} from "../../components/visualizations/chart-theme";
import "../../components/visualizations/visualizations.css";
import type { ProxyMetrics } from "./telemetry-api";

type Capability = keyof ProxyMetrics["capabilities"];
interface TelemetryChartProps {
  metrics?: ProxyMetrics;
  loading?: boolean;
  failed?: boolean;
}
const observed = (metrics: ProxyMetrics | undefined, capability: Capability) =>
  !!metrics &&
  metrics.state !== "unavailable" &&
  (metrics.capabilities[capability].available ||
    (metrics.state === "stale" && !!metrics.sampledAt));
const sourceLabel = ({ metrics, failed }: TelemetryChartProps) =>
  `${metrics?.source || "代理控制器"}${metrics?.state === "stale" || failed ? " · 上次采样" : ""}`;
const emptyReason = (
  { metrics, loading, failed }: TelemetryChartProps,
  capability: Capability,
  empty: string,
) => {
  if (!metrics)
    return loading
      ? "正在读取代理采样"
      : failed
        ? "代理采样读取失败"
        : "尚未取得代理采样";
  if (metrics.state === "unavailable") return `不可用 · ${metrics.reason}`;
  if (!metrics.capabilities[capability].available)
    return `不可用 · ${metrics.capabilities[capability].reason}`;
  return empty;
};
const percentile = (values: readonly number[], fraction: number) => {
  const sorted = [...values].sort((a, b) => a - b);
  return sorted[Math.max(0, Math.ceil(sorted.length * fraction) - 1)];
};

/** Start offsets and age come only from the active connection snapshot. */
export function ConnectionTimeline(props: TelemetryChartProps) {
  const { metrics } = props;
  const deviceNames = useConnectionDeviceNames(metrics);
  const connections = useMemo(
    () =>
      observed(metrics, "connections")
        ? [...metrics!.connections].sort(
            (a, b) => Date.parse(a.startedAt) - Date.parse(b.startedAt),
          )
        : [],
    [metrics],
  );
  const start = Math.min(
    ...connections
      .map((item) => Date.parse(item.startedAt))
      .filter(Number.isFinite),
  );
  const offset = (startedAt: string) =>
    Number.isFinite(start) && Number.isFinite(Date.parse(startedAt))
      ? Math.max(0, Date.parse(startedAt) - start)
      : 0;
  const option = useMemo(
    () =>
      (p: ChartPalette): EChartsOption => ({
        ...baseOption(p),
        legend: { show: false },
        grid: { top: 20, right: 24, bottom: 58, left: 8, containLabel: true },
        xAxis: {
          type: "value",
          name: "相对开始 / ms",
          min: 0,
          ...axisStyle(p),
        },
        yAxis: {
          type: "category",
          inverse: true,
          data: connections.map(
            (item) =>
              `${deviceNames.get(item.id) || item.sourceIP || item.id} → ${item.host || item.destinationIP || "未提供目标"}${item.destinationPort ? `:${item.destinationPort}` : ""}`,
          ),
          ...axisStyle(p),
          axisLabel: { color: p.text, width: 180, overflow: "truncate" },
          splitLine: { show: false },
        },
        dataZoom: zoomOption(p),
        tooltip: {
          ...baseOption(p).tooltip,
          trigger: "axis",
          axisPointer: { type: "shadow" },
          formatter: (parameters) => {
            const item = Array.isArray(parameters) ? parameters[0] : parameters;
            const connection = connections[item.dataIndex];
            return connection
              ? `${connection.id} · ${connection.network}${deviceNames.has(connection.id) ? ` · ${deviceNames.get(connection.id)}` : ""}\n客户端 ${connection.sourceIP || "未提供"} · 端口 ${connection.sourcePort || "未提供"}\n目标 ${connection.host || connection.destinationIP || "未提供"} · 端口 ${connection.destinationPort || "未提供"}\n开始 ${connection.startedAt || "未提供"}\n存续 ${connection.ageMs} ms · 出站 ${connection.outbound}\n上传 ${connection.uploadBytes} B · 下载 ${connection.downloadBytes} B`
              : "";
          },
        },
        series: [
          {
            type: "bar",
            name: "开始偏移",
            stack: "connection",
            data: connections.map((item) => offset(item.startedAt)),
            silent: true,
            itemStyle: { color: "transparent" },
            emphasis: { disabled: true },
            tooltip: { show: false },
            barWidth: 14,
          },
          {
            type: "bar",
            name: "存续时长",
            stack: "connection",
            data: connections.map((item) => item.ageMs),
            barWidth: 14,
            itemStyle: { color: p.proxy },
          },
        ],
      }),
    [connections, start, deviceNames],
  );
  return (
    <ChartFrame
      title="活动连接时间线"
      subtitle={`实际开始时间与存续时长 · 不是请求阶段追踪${metrics?.capabilities.connections.reason ? ` · ${metrics.capabilities.connections.reason}` : ""}`}
      demo={false}
      hasData={connections.length > 0}
      source={sourceLabel(props)}
      emptyLabel="无连接采样"
      unavailable={emptyReason(props, "connections", "当前没有活动连接")}
      summary={`${metrics && metrics.state !== "unavailable" ? metrics.activeConnections : "未知"} 个活动连接 · 列出 ${connections.length} 个${metrics?.truncated ? " · 连接列表已截断" : ""}`}
      hint="横轴从最早可用开始时间计起 · 不推算 DNS、TCP 握手或 TLS 阶段"
      columns={[
        "连接 ID",
        "设备名称",
        "客户端 IP",
        "客户端端口",
        "目标主机",
        "目标 IP",
        "目标端口",
        "开始时间",
        "存续 / ms",
        "网络",
        "上传 / B",
        "下载 / B",
        "出站",
        "规则 ID",
        "规则",
      ]}
      rows={connections.map((item) => [
        item.id,
        deviceNames.get(item.id) || "未关联设备",
        item.sourceIP || "未提供",
        item.sourcePort || "未提供",
        item.host || "未提供",
        item.destinationIP || "未提供",
        item.destinationPort || "未提供",
        item.startedAt || "未提供",
        item.ageMs,
        item.network,
        item.uploadBytes,
        item.downloadBytes,
        item.outbound,
        item.ruleId || "未提供",
        item.rule || "未提供",
      ])}
    >
      <EChart
        option={option}
        label="活动连接开始时间和存续时长时间线，不包含请求阶段"
        height={300}
      />
    </ChartFrame>
  );
}

export function ProxyTrafficChart(props: TelemetryChartProps) {
  const { metrics } = props;
  const points = useMemo(
    () => (observed(metrics, "traffic") ? metrics!.traffic : []),
    [metrics],
  );
  const latest = points.at(-1);
  const option = useMemo(
    () =>
      (p: ChartPalette): EChartsOption => ({
        ...baseOption(p),
        xAxis: {
          type: "category",
          boundaryGap: false,
          data: points.map((point) => point.time),
          ...axisStyle(p),
          splitLine: { show: false },
        },
        yAxis: { type: "value", name: "B/s", min: 0, ...axisStyle(p) },
        dataZoom: zoomOption(p),
        series: [
          {
            type: "line",
            name: "上传",
            data: points.map((point) =>
              point.reset ? null : point.uploadRate,
            ),
            connectNulls: false,
            showSymbol: true,
            smooth: false,
            itemStyle: { color: p.tx },
            lineStyle: { color: p.tx, width: 2 },
          },
          {
            type: "line",
            name: "下载",
            data: points.map((point) =>
              point.reset ? null : point.downloadRate,
            ),
            connectNulls: false,
            showSymbol: true,
            smooth: false,
            itemStyle: { color: p.rx },
            lineStyle: { color: p.rx, width: 2 },
            areaStyle: { opacity: 0.07 },
          },
        ],
      }),
    [points],
  );
  return (
    <ChartFrame
      title="代理流量"
      subtitle={`代理控制器累计计数器差值 · 实际上传 / 下载速率${metrics?.capabilities.traffic.reason ? ` · ${metrics.capabilities.traffic.reason}` : ""}`}
      demo={false}
      hasData={points.length > 0}
      source={sourceLabel(props)}
      emptyLabel="无流量采样"
      unavailable={emptyReason(props, "traffic", "尚无流量速率样本")}
      summary={
        latest
          ? latest.reset
            ? "最新样本为计数器重置，等待下一次速率采样"
            : `最新上传 ${latest.uploadRate} B/s · 下载 ${latest.downloadRate} B/s · ${points.length} 个采样点`
          : undefined
      }
      hint="核心总流量（含直连） · 最近 900 点内存 / 默认约 30 分钟，重启清空，非 WAN 持久历史 · 计数器重置处留空"
      columns={["时间", "上传 / B/s", "下载 / B/s", "采样状态"]}
      rows={points.map((point) => [
        point.time,
        point.reset ? "—" : point.uploadRate,
        point.reset ? "—" : point.downloadRate,
        point.reset ? "计数器重置" : "速率采样",
      ])}
    >
      <EChart
        option={option}
        label="代理上传和下载实际速率时间图，计数器重置处断开"
      />
    </ChartFrame>
  );
}

/** Only explicit fixed-target request probes enter this distribution. */
export function ProbeLatencyChart(props: TelemetryChartProps) {
  const { metrics } = props;
  const probes = useMemo(
    () => (observed(metrics, "latency") ? metrics!.probes : []),
    [metrics],
  );
  const values = useMemo(
    () =>
      probes
        .filter((probe) => probe.status === "ok")
        .map((probe) => probe.delayMs),
    [probes],
  );
  const bins = useMemo(() => {
    if (!values.length) return [];
    const maximum = Math.max(...values);
    // Keep a bounded number of equal-width bins, even for a slow probe.
    const width = Math.max(25, Math.ceil(maximum / 1000) * 25);
    return Array.from(
      { length: Math.floor(maximum / width) + 1 },
      (_, index) => ({
        label: `${index * width}–${(index + 1) * width}`,
        count: values.filter(
          (value) => value >= index * width && value < (index + 1) * width,
        ).length,
      }),
    );
  }, [values]);
  const option = useMemo(
    () =>
      (p: ChartPalette): EChartsOption => ({
        ...baseOption(p),
        legend: { show: false },
        xAxis: {
          type: "category",
          name: "ms",
          data: bins.map((bin) => bin.label),
          ...axisStyle(p),
          splitLine: { show: false },
        },
        yAxis: {
          type: "value",
          name: "成功探测数",
          minInterval: 1,
          ...axisStyle(p),
        },
        dataZoom: zoomOption(p),
        series: [
          {
            type: "bar",
            name: "成功探测",
            data: bins.map((bin) => bin.count),
            barMaxWidth: 40,
            itemStyle: { color: p.latency, borderRadius: [3, 3, 0, 0] },
          },
        ],
      }),
    [bins],
  );
  return (
    <ChartFrame
      title="探测延迟分布"
      subtitle={`当前选中节点 · 固定目标请求探测 · 仅用户主动触发${metrics?.capabilities.latency.reason ? ` · ${metrics.capabilities.latency.reason}` : ""}`}
      demo={false}
      hasData={probes.length > 0}
      source={sourceLabel(props)}
      emptyLabel="无主动探测"
      unavailable={emptyReason(
        props,
        "latency",
        "尚无主动探测样本，请点击探测当前选中节点",
      )}
      summary={`${values.length} 次成功 · ${probes.length - values.length} 次失败${values.length ? ` · P50 ${percentile(values, 0.5)} ms · P95 ${percentile(values, 0.95)} ms` : ""}`}
      hint="失败请求不计为 0 ms，也不进入直方图 · 表格保留每次探测结果"
      columns={["探测时间", "延迟 / ms", "结果"]}
      rows={probes.map((probe) => [
        probe.time,
        probe.status === "ok" ? probe.delayMs : "—",
        probe.status === "ok" ? "成功" : "失败",
      ])}
    >
      {values.length > 0 ? (
        <EChart
          option={option}
          label="选中节点成功请求探测延迟直方图，失败请求不计入"
        />
      ) : (
        <p className="viz-summary" role="status">
          没有成功探测，失败结果见数据表
        </p>
      )}
    </ChartFrame>
  );
}

export function ActiveRoutingChart(props: TelemetryChartProps) {
  const { metrics } = props;
  const groups = useMemo(() => {
    if (!observed(metrics, "routing") || !observed(metrics, "connections"))
      return [];
    const counts = new Map<
      string,
      { ruleId: string; rule: string; outbound: string; count: number }
    >();
    for (const connection of metrics!.connections) {
      const key = JSON.stringify([
        connection.ruleId,
        connection.rule,
        connection.outbound,
      ]);
      const group = counts.get(key);
      if (group) group.count++;
      else
        counts.set(key, {
          ruleId: connection.ruleId,
          rule: connection.rule,
          outbound: connection.outbound,
          count: 1,
        });
    }
    return [...counts.values()].sort((a, b) => b.count - a.count);
  }, [metrics]);
  const option = useMemo(
    () =>
      (p: ChartPalette): EChartsOption => ({
        ...baseOption(p),
        legend: { show: false },
        grid: { top: 20, right: 24, bottom: 28, left: 8, containLabel: true },
        xAxis: {
          type: "value",
          name: "活动连接数",
          minInterval: 1,
          min: 0,
          ...axisStyle(p),
        },
        yAxis: {
          type: "category",
          inverse: true,
          data: groups.map(
            (group) =>
              `${group.rule || group.ruleId || "未提供规则"} · ${group.outbound}`,
          ),
          ...axisStyle(p),
          axisLabel: { color: p.text, width: 180, overflow: "truncate" },
          splitLine: { show: false },
        },
        tooltip: {
          ...baseOption(p).tooltip,
          trigger: "axis",
          axisPointer: { type: "shadow" },
          formatter: (parameters) => {
            const item = Array.isArray(parameters) ? parameters[0] : parameters;
            const group = groups[item.dataIndex];
            return group
              ? `${group.ruleId || "未提供规则"} · ${group.rule || "未提供规则描述"}\n出站 ${group.outbound} · ${group.count} 个已观察活动连接`
              : "";
          },
        },
        series: [
          {
            type: "bar",
            name: "已观察活动连接",
            barWidth: 16,
            data: groups.map((group) => ({
              value: group.count,
              itemStyle: {
                color:
                  group.outbound === "direct"
                    ? p.direct
                    : group.outbound === "proxy"
                      ? p.proxy
                      : p.text,
              },
            })),
          },
        ],
      }),
    [groups],
  );
  return (
    <ChartFrame
      title="活动连接分流"
      subtitle={`按实际规则与出站分组 · 不是累计规则命中${metrics?.capabilities.routing.reason ? ` · ${metrics.capabilities.routing.reason}` : ""}`}
      demo={false}
      hasData={groups.length > 0}
      source={sourceLabel(props)}
      emptyLabel="无分流采样"
      unavailable={emptyReason(
        props,
        observed(metrics, "connections") ? "routing" : "connections",
        "当前没有可观察的活动分流连接",
      )}
      summary={`${groups.length} 个规则 / 出站组合 · ${groups.reduce((total, group) => total + group.count, 0)} 个已观察活动连接${metrics?.truncated ? " · 连接列表已截断，仅统计列出的连接" : ""}`}
      hint="只统计本次快照中的活动连接，不代表历史请求数或累计规则命中"
      columns={["规则 ID", "规则", "出站", "已观察活动连接数"]}
      rows={groups.map((group) => [
        group.ruleId || "未提供",
        group.rule || "未提供",
        group.outbound,
        group.count,
      ])}
    >
      <EChart
        option={option}
        label="实际已观察活动连接按规则和出站分组的条形图，不是累计规则命中"
      />
    </ChartFrame>
  );
}
