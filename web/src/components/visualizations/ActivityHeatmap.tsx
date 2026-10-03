import { useMemo, useState } from "react";
import type { EChartsOption } from "echarts";
import { ChartFrame, ChartSelect, type VisualizationProps } from "./ChartFrame";
import { EChart } from "./EChart";
import { axisStyle, baseOption, type ChartPalette } from "./chart-theme";
import {
  DEVICE_ACTIVITY_CHART_ROWS,
  type DeviceActivityDevice,
  type DeviceActivityPoint,
} from "../../lib/device-activity-contracts";
import { bytes } from "../../lib/format";

export type DeviceByteMetric = "total" | "rx" | "tx";
export interface ActivityHeatmapProps extends VisualizationProps {
  devices?: readonly DeviceActivityDevice[];
  resolutionSeconds?: number;
  unavailable?: string;
  onSelectDevice?: (id: string) => void;
}
interface ByteHeatmapDevice {
  id: string;
  name: string;
  stale: boolean;
  samples: readonly DeviceActivityPoint[];
}
/** Own byte-shaped deterministic demo. No copied observations or user identities. */
const demoByteDevices: readonly ByteHeatmapDevice[] = Array.from(
  { length: 6 },
  (_, row) => ({
    id: `sample-${row + 1}`,
    name: `终端 ${String.fromCharCode(65 + row)}`,
    stale: false,
    samples: Array.from({ length: 24 }, (_, column) => ({
      time: new Date(Date.UTC(2026, 0, 1, 12, column * 2)).toISOString(),
      rxBytes:
        Math.max(0, Math.round(14 + 12 * Math.sin(column / 3 + row))) * 1024,
      txBytes:
        Math.max(0, Math.round(4 + 3 * Math.cos(column / 4 + row))) * 1024,
      coverageSeconds: 120,
    })),
  }),
);
export function deviceActivityTimestamp(time: string): string {
  const date = new Date(time);
  return Number.isFinite(date.getTime()) ? date.toISOString() : "—";
}
export function measuredDeviceBytes(
  point: DeviceActivityPoint,
  metric: DeviceByteMetric,
): number | null {
  if (point.coverageSeconds <= 0) return null;
  if (metric === "rx") return point.rxBytes;
  if (metric === "tx") return point.txBytes;
  return point.rxBytes !== null && point.txBytes !== null
    ? point.rxBytes + point.txBytes
    : null;
}
const metricLabels: Record<DeviceByteMetric, string> = {
  total: "RX + TX",
  rx: "RX",
  tx: "TX",
};
export function ActivityHeatmap({
  demo = false,
  devices: actualDevices,
  resolutionSeconds,
  onSelectDevice,
  unavailable = "暂无时段活跃数据 · 未接入真实时段记录，后台统计服务正在积累数据，或当前设备在所选时段内无活动。",
}: ActivityHeatmapProps) {
  const [device, setDevice] = useState("all");
  const [metric, setMetric] = useState<DeviceByteMetric>("total");
  const isDemo = actualDevices === undefined && demo;
  const available: readonly ByteHeatmapDevice[] =
    actualDevices ?? (isDemo ? demoByteDevices : []);
  const selected = available.some((item) => item.id === device)
    ? device
    : "all";
  const devices = useMemo(
    () =>
      (selected === "all"
        ? available
        : available.filter((item) => item.id === selected)
      ).slice(0, DEVICE_ACTIVITY_CHART_ROWS),
    [available, selected],
  );
  const samples = useMemo(
    () =>
      devices.flatMap((item, row) =>
        item.samples.map((point) => ({
          device: item,
          point,
          row,
          time: deviceActivityTimestamp(point.time),
          value: measuredDeviceBytes(point, metric),
        })),
      ),
    [devices, metric],
  );
  const times = useMemo(
    () => [...new Set(samples.map((item) => item.time))].sort(),
    [samples],
  );
  const measured = samples.filter((item) => item.value !== null);
  const hasData = available.length > 0;
  const partial = (point: DeviceActivityPoint) =>
    !!resolutionSeconds && point.coverageSeconds < resolutionSeconds;
  const option = useMemo(
    () =>
      (p: ChartPalette): EChartsOption => {
        const cells = samples.flatMap((item) =>
          item.value === null
            ? []
            : [[times.indexOf(item.time), item.row, item.value]],
        );
        const maximum = Math.max(1, ...cells.map((item) => item[2]));
        return {
          ...baseOption(p),
          legend: { show: false },
          grid: {
            top: 12,
            left: 12,
            right: 16,
            bottom: 72,
            containLabel: true,
          },
          xAxis: {
            type: "category",
            data: times,
            ...axisStyle(p),
            splitLine: { show: false },
            axisLabel: {
              color: p.text,
              fontSize: 10,
              hideOverlap: true,
              formatter: (value: string) =>
                value.slice(5, 16).replace("T", "\n"),
            },
          },
          yAxis: {
            type: "category",
            inverse: true,
            data: devices.map(
              (item) => `${item.name || item.id}${item.stale ? " · 陈旧" : ""}`,
            ),
            ...axisStyle(p),
            splitLine: { show: false },
            axisLabel: {
              color: p.text,
              width: 80,
              overflow: "truncate",
              fontSize: 10,
            },
          },
          visualMap: {
            min: 0,
            max: maximum,
            orient: "horizontal",
            left: "center",
            bottom: 8,
            itemWidth: 10,
            itemHeight: 100,
            calculable: false,
            text: [bytes(maximum), "0 B"],
            textStyle: { color: p.text, fontSize: 10 },
            inRange: { color: [p.heatLow, p.heatHigh] },
          },
          tooltip: {
            ...baseOption(p).tooltip,
            trigger: "item",
            formatter: (parameter) => {
              const item = Array.isArray(parameter) ? parameter[0] : parameter;
              const values = item.value as number[];
              const match = samples.find(
                (sample) =>
                  sample.row === values[1] && sample.time === times[values[0]],
              );
              return match
                ? `${match.device.name || match.device.id} · ${match.device.id}\n${match.time} (UTC)\n${metricLabels[metric]} ${values[2]} bytes\n覆盖 ${match.point.coverageSeconds} 秒${resolutionSeconds && match.point.coverageSeconds < resolutionSeconds ? " · 部分采样" : ""}${match.device.stale ? "\n设备记录陈旧" : ""}`
                : "";
            },
          },
          series: [
            {
              type: "heatmap",
              name: `${metricLabels[metric]} / bytes`,
              data: cells,
              itemStyle: { borderWidth: 1, borderColor: p.surface },
              emphasis: {
                itemStyle: { borderWidth: 2, borderColor: p.foreground },
              },
            },
          ],
        };
      },
    [samples, times, devices, metric, resolutionSeconds],
  );
  const source = "数据源：系统流量统计 (trafficd)";
  return (
    <ChartFrame
      title="设备活跃热力图"
      subtitle="展示局域网各设备在不同时段的流量分布与活跃状态 · 流量 (Bytes) · UTC"
      demo={isDemo}
      hasData={hasData}
      source={source}
      emptyLabel={source}
      unavailable={unavailable}
      summary={`${devices.length} 个${isDemo ? "样本" : ""}设备 · ${measured.length ? `${metricLabels[metric]} 已测量 ${measured.reduce((sum, item) => sum + (item.value ?? 0), 0)} bytes` : `${metricLabels[metric]} 尚无已测量字节（不是零流量）`}${available.length > devices.length && selected === "all" ? ` · 图表仅显示前 ${DEVICE_ACTIVITY_CHART_ROWS} 个，使用设备筛选查看其他行` : ""}`}
      controls={
        <>
          <ChartSelect label="终端" value={selected} onChange={setDevice}>
            <option value="all">全部终端</option>
            {available.map((item) => (
              <option key={item.id} value={item.id}>
                {item.name || item.id}
              </option>
            ))}
          </ChartSelect>
          <ChartSelect
            label="字节方向"
            value={metric}
            onChange={(value) => setMetric(value as DeviceByteMetric)}
          >
            <option value="total">RX + TX / Bytes</option>
            <option value="rx">RX / Bytes</option>
            <option value="tx">TX / Bytes</option>
          </ChartSelect>
        </>
      }
      hint="空白代表缺失，0 B 代表实测零值；部分桶仅含已覆盖时段的字节量。时间间隔以 UTC 标签为准。"
      columns={[
        "终端",
        "身份",
        "时间（UTC 桶起点）",
        "RX / bytes",
        "TX / bytes",
        "覆盖 / 秒",
        "采样状态",
      ]}
      rows={samples.map(({ device: item, point, time, value }) => [
        item.name || item.id,
        item.id,
        time,
        point.coverageSeconds > 0 ? (point.rxBytes ?? "—") : "—",
        point.coverageSeconds > 0 ? (point.txBytes ?? "—") : "—",
        point.coverageSeconds,
        `${value === null ? "未采样" : partial(point) ? "部分采样" : "已采样"}${item.stale ? " · 设备陈旧" : ""}`,
      ])}
    >
      {measured.length > 0 ? (
        <EChart
          option={option}
          onDataClick={
            !isDemo && onSelectDevice
              ? ({ value, componentType, seriesType }) => {
                  if (
                    componentType !== "series" ||
                    seriesType !== "heatmap" ||
                    !Array.isArray(value) ||
                    !Number.isInteger(value[0]) ||
                    !Number.isInteger(value[1])
                  )
                    return;
                  const [column, row] = value as number[];
                  const selectedDevice = devices[row];
                  if (
                    !selectedDevice ||
                    !/^(?:[0-9a-fA-F]{2}:){5}[0-9a-fA-F]{2}$/.test(
                      selectedDevice.id,
                    )
                  )
                    return;
                  const measuredCell = samples.some(
                    (sample) =>
                      sample.row === row &&
                      sample.time === times[column] &&
                      sample.value !== null,
                  );
                  if (measuredCell) onSelectDevice(selectedDevice.id);
                }
              : undefined
          }
          height={Math.max(280, devices.length * 26 + 120)}
          label="设备 RX/TX 字节热力图，UTC 时间，缺失留空，实测零值为 0 B；下方数据表提供每个时间桶的字节量和采样覆盖"
        />
      ) : (
        <div className="viz-empty" role="status">
          <p>
            所选设备与字节方向暂无已测量数据；可切换设备或方向，缺失不是实测零值。
          </p>
        </div>
      )}
    </ChartFrame>
  );
}
