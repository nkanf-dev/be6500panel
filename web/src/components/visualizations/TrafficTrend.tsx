import { useMemo, useState } from "react";
import type { EChartsOption } from "echarts";
import { ChartFrame, ChartSelect, type VisualizationProps } from "./ChartFrame";
import { EChart } from "./EChart";
import {
  axisStyle,
  baseOption,
  zoomOption,
  type ChartPalette,
} from "./chart-theme";
import { trafficSamples } from "./demo-data";
import {
  trafficHistoryRangeLabels,
  trafficHistoryRangeSeconds,
  type TrafficHistoryRange,
} from "../../lib/traffic-history-contracts";
import { trafficTimestamp } from "../../lib/traffic-history-format";

export interface TrafficSample {
  time: string;
  /** Interface counter rates expressed as bytes per second. */
  rx: number;
  tx: number;
  latency?: number;
  rxPeak?: number;
  txPeak?: number;
  rxBytes?: number;
  txBytes?: number;
  coverageSeconds?: number;
}
export interface TrafficTrendProps extends VisualizationProps {
  samples?: readonly TrafficSample[];
  source?: string;
  title?: string;
  range?: TrafficHistoryRange;
  resolutionSeconds?: number;
  /** Server bucket bounds. No browser-clock backfill or invented measurements. */
  rangeStart?: number;
  rangeEnd?: number;
  unavailable?: string;
}
const toMB = (rate: number) => rate / 1_000_000;
const formatRate = (rate: number) => toMB(rate).toFixed(3);
const measuredLatency = (latency: number | undefined): latency is number =>
  typeof latency === "number" && Number.isFinite(latency) && latency >= 0;
const demoSamples: readonly TrafficSample[] = trafficSamples.map((point) => ({
  ...point,
  rx: point.rx * 125_000,
  tx: point.tx * 125_000,
}));
const covered = (point: TrafficSample) =>
  point.coverageSeconds === undefined || point.coverageSeconds > 0;
const complete = (point: TrafficSample, resolution?: number) =>
  covered(point) &&
  !(
    resolution &&
    point.coverageSeconds !== undefined &&
    point.coverageSeconds + 0.001 < resolution
  );
type ChartPoint = { time: string; point?: TrafficSample };

/** Only null chart separators are added. They are never measurements or table/export rows. */
function chartPoints(
  samples: readonly TrafficSample[],
  resolution?: number,
): ChartPoint[] {
  return samples.flatMap((point, index) => {
    const previous = samples[index - 1];
    const previousTime = previous && Date.parse(previous.time);
    const time = Date.parse(point.time);
    const gap =
      resolution &&
      previous &&
      complete(previous, resolution) &&
      complete(point, resolution) &&
      Number.isFinite(previousTime) &&
      time - previousTime > resolution * 1000 + 1;
    return [
      ...(gap
        ? [{ time: new Date(previousTime + resolution * 1000).toISOString() }]
        : []),
      { time: point.time, point },
    ];
  });
}
export function TrafficTrend({
  demo = false,
  samples: actualSamples,
  source = "接口采样",
  title,
  range,
  resolutionSeconds,
  rangeStart,
  rangeEnd,
  unavailable = "未采样",
}: TrafficTrendProps) {
  const [windowMinutes, setWindowMinutes] = useState("60");
  const [metric, setMetric] = useState("all");
  const actual = !!actualSamples?.length;
  const isDemo = !actual && demo;
  // History is already bounded/downsampled by the server. Never cut off the selected time window.
  const samples = useMemo(
    () =>
      actual
        ? actualSamples!
        : isDemo
          ? demoSamples.slice(-(Number(windowMinutes) + 1))
          : [],
    [actual, actualSamples, isDemo, windowMinutes],
  );
  const history = samples.some((point) => point.coverageSeconds !== undefined);
  const hasLatency = samples.some((point) => measuredLatency(point.latency));
  const selectedMetric = hasLatency ? metric : "traffic";
  const temporal =
    !isDemo &&
    samples.length > 0 &&
    samples.every((point) => Number.isFinite(Date.parse(point.time)));
  const displayPoints = useMemo(
    () => chartPoints(samples, resolutionSeconds),
    [samples, resolutionSeconds],
  );
  const option = useMemo(
    () =>
      (p: ChartPalette): EChartsOption => {
        const values = (key: "rx" | "tx") =>
          displayPoints.map(({ time, point }) => {
            const value =
              point && complete(point, resolutionSeconds)
                ? toMB(point[key])
                : null;
            return temporal ? [Date.parse(time), value] : value;
          });
        return {
          ...baseOption(p),
          xAxis: temporal
            ? {
                type: "time",
                boundaryGap: [0, 0],
                ...axisStyle(p),
                splitLine: { show: false },
                ...(range && rangeEnd
                  ? {
                      min:
                        rangeStart ??
                        rangeEnd - trafficHistoryRangeSeconds[range] * 1000,
                      max: rangeEnd,
                    }
                  : {}),
                axisLabel: {
                  ...axisStyle(p).axisLabel,
                  formatter: (value: number) =>
                    new Date(value)
                      .toISOString()
                      .slice(
                        range && trafficHistoryRangeSeconds[range] >= 86400
                          ? 0
                          : 11,
                        range && trafficHistoryRangeSeconds[range] >= 86400
                          ? 16
                          : 19,
                      )
                      .replace("T", " "),
                },
              }
            : {
                type: "category",
                boundaryGap: false,
                data: displayPoints.map((point) => point.time),
                ...axisStyle(p),
                splitLine: { show: false },
              },
          yAxis: [
            { type: "value", name: "MB/s", min: 0, ...axisStyle(p) },
            ...(hasLatency
              ? [
                  {
                    type: "value" as const,
                    name: "ms",
                    min: 0,
                    position: "right" as const,
                    ...axisStyle(p),
                    splitLine: { show: false },
                  },
                ]
              : []),
          ],
          dataZoom: zoomOption(p),
          series: [
            ...(selectedMetric !== "latency"
              ? [
                  {
                    type: "line" as const,
                    name: "RX",
                    data: values("rx"),
                    connectNulls: false,
                    showSymbol: false,
                    smooth: isDemo ? 0.2 : false,
                    lineStyle: { width: 2, color: p.rx },
                    itemStyle: { color: p.rx },
                    areaStyle: { opacity: 0.07 },
                  },
                  {
                    type: "line" as const,
                    name: "TX",
                    data: values("tx"),
                    connectNulls: false,
                    showSymbol: false,
                    smooth: isDemo ? 0.2 : false,
                    lineStyle: { width: 2, color: p.tx },
                    itemStyle: { color: p.tx },
                  },
                ]
              : []),
            ...(hasLatency && selectedMetric !== "traffic"
              ? [
                  {
                    type: "line" as const,
                    name: "延迟",
                    yAxisIndex: 1,
                    data: displayPoints.map(({ time, point }) => {
                      const value =
                        point && measuredLatency(point.latency)
                          ? point.latency
                          : null;
                      return temporal ? [Date.parse(time), value] : value;
                    }),
                    connectNulls: false,
                    showSymbol: false,
                    smooth: isDemo ? 0.2 : false,
                    lineStyle: {
                      width: 1.5,
                      type: "dashed" as const,
                      color: p.latency,
                    },
                    itemStyle: { color: p.latency },
                  },
                ]
              : []),
          ],
          tooltip: {
            ...baseOption(p).tooltip,
            formatter: (parameters) => {
              const items = Array.isArray(parameters)
                ? parameters
                : [parameters];
              const firstValue = items[0]?.value;
              const time =
                temporal && Array.isArray(firstValue)
                  ? trafficTimestamp(
                      new Date(Number(firstValue[0])).toISOString(),
                    )
                  : (items[0]?.name ?? "");
              const lines = items.flatMap((item) => {
                const value = Array.isArray(item.value)
                  ? item.value[1]
                  : item.value;
                return typeof value === "number"
                  ? [
                      `${item.seriesName}  ${item.seriesName === "延迟" ? value : value.toFixed(3)} ${item.seriesName === "延迟" ? "ms" : "MB/s"}`,
                    ]
                  : [];
              });
              return `${time}\n${lines.join("\n")}`;
            },
          },
        };
      },
    [
      displayPoints,
      temporal,
      hasLatency,
      selectedMetric,
      isDemo,
      resolutionSeconds,
      range,
      rangeStart,
      rangeEnd,
    ],
  );
  const latest = samples.at(-1);
  const summary = latest
    ? covered(latest)
      ? `${complete(latest, resolutionSeconds) ? "最新" : "最新桶覆盖不足 · 已测量"} RX ${formatRate(latest.rx)} MB/s · TX ${formatRate(latest.tx)} MB/s${measuredLatency(latest.latency) ? ` · 延迟 ${latest.latency} ms` : ""}`
      : "最新时间桶未采样"
    : undefined;
  return (
    <ChartFrame
      title={title || (hasLatency ? "流量与延迟" : "接口流量")}
      subtitle={
        isDemo
          ? "RX / TX · 1 分钟采样"
          : `RX / TX · 接口计数器速率${range ? ` · ${trafficHistoryRangeLabels[range]} · UTC` : ""}`
      }
      demo={isDemo}
      hasData={samples.length > 0}
      source={source}
      emptyLabel="未采样"
      unavailable={unavailable}
      summary={summary}
      controls={
        <>
          {isDemo && (
            <ChartSelect
              label="时间范围"
              value={windowMinutes}
              onChange={setWindowMinutes}
            >
              <option value="60">演示 60 分钟</option>
              <option value="30">演示 30 分钟</option>
              <option value="15">演示 15 分钟</option>
            </ChartSelect>
          )}
          {hasLatency && (
            <ChartSelect label="指标" value={metric} onChange={setMetric}>
              <option value="all">流量 + 延迟</option>
              <option value="traffic">RX / TX</option>
              <option value="latency">延迟</option>
            </ChartSelect>
          )}
        </>
      }
      hint={`拖动底部范围缩放 · 图例切换指标${temporal ? " · UTC" : ""}`}
      columns={[
        temporal ? "时间（UTC 桶起点）" : "时间",
        "RX / MB/s",
        "TX / MB/s",
        ...(history
          ? [
              "RX 峰值 / MB/s",
              "TX 峰值 / MB/s",
              "RX 总量 / bytes",
              "TX 总量 / bytes",
              "覆盖 / 秒",
            ]
          : []),
        ...(hasLatency ? ["延迟 / ms"] : []),
      ]}
      rows={samples.map((point) => [
        temporal ? trafficTimestamp(point.time) : point.time,
        ...(covered(point)
          ? [formatRate(point.rx), formatRate(point.tx)]
          : ["—", "—"]),
        ...(history
          ? [
              covered(point) && point.rxPeak !== undefined
                ? formatRate(point.rxPeak)
                : "—",
              covered(point) && point.txPeak !== undefined
                ? formatRate(point.txPeak)
                : "—",
              covered(point) ? (point.rxBytes ?? "—") : "—",
              covered(point) ? (point.txBytes ?? "—") : "—",
              point.coverageSeconds ?? "—",
            ]
          : []),
        ...(hasLatency
          ? [measuredLatency(point.latency) ? point.latency : "—"]
          : []),
      ])}
    >
      <EChart
        option={option}
        label={`接口 RX、TX 流量${hasLatency ? "和延迟" : ""}趋势${temporal ? "，UTC 时间，缺失和不完整时间桶留空" : ""}，使用下方数据表读取准确值`}
      />
    </ChartFrame>
  );
}
