import { useEffect, useMemo, useState, type ReactNode } from "react";
import type { EChartsOption } from "echarts";
import { ChartFrame } from "../../components/visualizations/ChartFrame";
import { EChart } from "../../components/visualizations/EChart";
import {
  axisStyle,
  baseOption,
  zoomOption,
  type ChartPalette,
} from "../../components/visualizations/chart-theme";
import { bytes } from "../../lib/format";
import { useDeviceLabels } from "./device-labels";
import type { WorkspaceDevice } from "./device-model";

function DeviceChartDisclosure({
  title,
  children,
}: {
  title: string;
  children: ReactNode;
}) {
  const [compact, setCompact] = useState(
    () => window.matchMedia?.("(max-width: 640px)").matches ?? false,
  );
  const [expanded, setExpanded] = useState(!compact);
  useEffect(() => {
    const media = window.matchMedia?.("(max-width: 640px)");
    if (!media) return;
    const change = () => {
      setCompact(media.matches);
      setExpanded(!media.matches);
    };
    change();
    media.addEventListener("change", change);
    return () => media.removeEventListener("change", change);
  }, []);
  return (
    <details
      className="device-chart-disclosure"
      open={expanded}
      onToggle={(event) => setExpanded(event.currentTarget.open)}
    >
      <summary>
        {title}
        <span>{expanded ? "收起图表" : "展开图表"}</span>
      </summary>
      {(expanded || !compact) && children}
    </details>
  );
}

export function DeviceHistoryCharts({
  devices,
  resolutionSeconds,
  source,
}: {
  devices: readonly WorkspaceDevice[];
  resolutionSeconds: number;
  source: string;
}) {
  const labels = useDeviceLabels();
  const values = useMemo(
    () =>
      devices.map((device) => ({
        device,
        name: labels.displayName(device.mac, device.hostname),
        samples: device.activity?.samples ?? [],
      })),
    [devices, labels.displayName],
  );
  const times = useMemo(
    () =>
      [
        ...new Set(
          values.flatMap((device) =>
            device.samples.map((sample) => sample.time),
          ),
        ),
      ].sort(),
    [values],
  );
  const observed = values.some((device) =>
    device.samples.some(
      (sample) =>
        sample.rxBytes !== null &&
        sample.txBytes !== null &&
        sample.coverageSeconds > 0,
    ),
  );
  const option = useMemo(
    () =>
      (palette: ChartPalette): EChartsOption => ({
        ...baseOption(palette),
        xAxis: { type: "time", ...axisStyle(palette) },
        yAxis: { type: "value", name: "B/s", ...axisStyle(palette) },
        dataZoom: zoomOption(palette),
        series: values.flatMap(({ device, name, samples }) =>
          ["RX", "TX"].map((direction) => ({
            name: `${name} · ${direction}`,
            type: "line" as const,
            showSymbol: false,
            connectNulls: false,
            data: samples.map((sample) => [
              sample.time,
              sample.coverageSeconds > 0
                ? (direction === "RX" ? sample.rxBytes : sample.txBytes) ===
                  null
                  ? null
                  : (direction === "RX" ? sample.rxBytes! : sample.txBytes!) /
                    sample.coverageSeconds
                : null,
            ]),
            // IDs retain identity even when display names are identical.
            id: `${device.mac}-${direction}`,
          })),
        ),
      }),
    [values],
  );
  const heatOption = useMemo(
    () =>
      (palette: ChartPalette): EChartsOption => {
        const points = values.flatMap(({ samples }, row) =>
          samples
            .filter(
              (sample) =>
                sample.rxBytes !== null &&
                sample.txBytes !== null &&
                sample.coverageSeconds > 0,
            )
            .map((sample) => [
              times.indexOf(sample.time),
              row,
              (sample.rxBytes! + sample.txBytes!) / sample.coverageSeconds,
            ]),
        );
        return {
          ...baseOption(palette),
          legend: { show: false },
          grid: {
            top: 12,
            left: 22,
            right: 20,
            bottom: 62,
            containLabel: true,
          },
          xAxis: {
            type: "category",
            data: times.map((time) =>
              new Date(time).toLocaleString("zh-CN", {
                month: "2-digit",
                day: "2-digit",
                hour: "2-digit",
                minute: "2-digit",
              }),
            ),
            ...axisStyle(palette),
            axisLabel: { color: palette.text, interval: "auto", fontSize: 10 },
          },
          yAxis: {
            type: "category",
            inverse: true,
            data: values.map((value) => value.name),
            ...axisStyle(palette),
          },
          visualMap: {
            min: 0,
            max: Math.max(1, ...points.map((point) => point[2])),
            orient: "horizontal",
            left: "center",
            bottom: 4,
            calculable: true,
            text: ["高 B/s", "低"],
            textStyle: { color: palette.text },
            inRange: { color: [palette.heatLow, palette.heatHigh] },
          },
          tooltip: { ...baseOption(palette).tooltip, trigger: "item" },
          series: [
            {
              type: "heatmap",
              name: "RX + TX B/s",
              data: points,
              itemStyle: { borderWidth: 1, borderColor: palette.surface },
            },
          ],
        };
      },
    [values, times],
  );
  const rows = values.flatMap(({ device, name, samples }) =>
    samples.map((sample) => [
      name,
      device.mac,
      new Date(sample.time).toLocaleString("zh-CN"),
      sample.rxBytes === null ? "缺测" : sample.rxBytes,
      sample.txBytes === null ? "缺测" : sample.txBytes,
      sample.coverageSeconds,
      sample.coverageSeconds > 0 && sample.rxBytes !== null
        ? Number((sample.rxBytes / sample.coverageSeconds).toFixed(2))
        : "—",
      sample.coverageSeconds > 0 && sample.txBytes !== null
        ? Number((sample.txBytes / sample.coverageSeconds).toFixed(2))
        : "—",
    ]),
  );
  return (
    <div className="device-chart-stack">
      <DeviceChartDisclosure
        title={devices.length > 1 ? "设备流量对比" : "设备流量趋势"}
      >
        <ChartFrame
          title={devices.length > 1 ? "设备流量对比" : "设备流量趋势"}
          subtitle={`同一时间范围 · ${resolutionSeconds} 秒桶 · 原始 trafficd RX / TX 方向`}
          demo={false}
          hasData={observed}
          source={source || "trafficd"}
          unavailable="所选范围尚无有效计数差值。刷新采样后可查看流量趋势。"
          summary={`${devices.length} 个设备 · 缺测与重置保持空隙，不补零。`}
          columns={[
            "设备",
            "MAC",
            "时间",
            "RX 字节",
            "TX 字节",
            "有效秒数",
            "RX B/s",
            "TX B/s",
          ]}
          rows={rows}
          hint="RX / TX 为固件计数器方向；按有效覆盖秒数计算速率。"
        >
          <EChart
            option={option}
            label="所选设备 RX 与 TX 实测速率曲线，缺测为空隙，下方有同源数据表"
            height={280}
          />
        </ChartFrame>
      </DeviceChartDisclosure>
      <DeviceChartDisclosure title="设备活动热力图">
        <ChartFrame
          title="设备活动热力图"
          subtitle="设备 × 时间 · RX + TX 实测速率 B/s"
          demo={false}
          hasData={observed}
          source={source || "trafficd"}
          unavailable="等待所选设备的有效流量采样。"
          summary="深色表示较高的实测流量速率；未采样的格子留空。"
          columns={[
            "设备",
            "MAC",
            "时间",
            "RX 字节",
            "TX 字节",
            "有效秒数",
            "RX B/s",
            "TX B/s",
          ]}
          rows={rows}
        >
          <EChart
            option={heatOption}
            label="所选设备实际流量速率热力图，下方有字节与覆盖秒数表"
            height={Math.max(220, values.length * 34 + 110)}
          />
        </ChartFrame>
      </DeviceChartDisclosure>
      <div className="table-scroll">
        <table className="data-table">
          <caption>所选设备 · 同一时间范围数据汇总</caption>
          <thead>
            <tr>
              <th>设备</th>
              <th>RX / TX 字节</th>
              <th>最新 RX / TX</th>
              <th>有效覆盖</th>
              <th>关联 / 最后采样</th>
            </tr>
          </thead>
          <tbody>
            {values.map(({ device, name }) => (
              <tr key={device.mac}>
                <th scope="row">
                  {name}
                  <small className="device-subtitle mono">{device.mac}</small>
                </th>
                <td>
                  {device.activity && device.activity.coverageSeconds > 0
                    ? `${bytes(device.activity.rxBytes)} / ${bytes(device.activity.txBytes)}`
                    : "未取得该范围有效计数"}
                </td>
                <td>
                  {device.activity?.rxBytesPerSecond !== undefined &&
                  device.activity.txBytesPerSecond !== undefined
                    ? `${bytes(device.activity.rxBytesPerSecond)}/s / ${bytes(device.activity.txBytesPerSecond)}/s`
                    : "等待有效速率"}
                </td>
                <td>
                  {device.activity
                    ? `${device.activity.coverageSeconds} 秒`
                    : "—"}
                </td>
                <td>
                  {device.activity
                    ? `${device.activity.associated ? "已关联" : "未关联"} · ${new Date(device.activity.lastSeen).toLocaleString("zh-CN")}${device.activity.stale ? " · 上次采样" : ""}`
                    : "—"}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}
