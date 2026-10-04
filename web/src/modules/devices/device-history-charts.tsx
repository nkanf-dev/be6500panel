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
import {
  byteNumber,
  chooseByteScale,
  formatByteAxisValue,
  formatByteRate,
  formatBytes,
} from "../../lib/byte-scale";
import { useDeviceLabels } from "./device-labels";
import type { DeviceActivitySample, WorkspaceDevice } from "./device-model";

const rawScale = { unit: "B" as const, divisor: 1 };
function measuredRate(
  sample: DeviceActivitySample,
  direction: "RX" | "TX",
): number | null {
  if (!Number.isFinite(sample.coverageSeconds) || sample.coverageSeconds <= 0)
    return null;
  const bytes = byteNumber(
    direction === "RX" ? sample.rxBytes : sample.txBytes,
    rawScale,
  );
  return bytes === null
    ? null
    : byteNumber(bytes / sample.coverageSeconds, rawScale);
}
function combinedRate(sample: DeviceActivitySample): number | null {
  const rx = measuredRate(sample, "RX");
  const tx = measuredRate(sample, "TX");
  return rx === null || tx === null
    ? null
    : byteNumber(
        (sample.rxBytes! + sample.txBytes!) / sample.coverageSeconds,
        rawScale,
      );
}

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
  const rateScale = useMemo(
    () =>
      chooseByteScale(
        values.flatMap(({ samples }) =>
          samples.flatMap((sample) =>
            [measuredRate(sample, "RX"), measuredRate(sample, "TX")].filter(
              (rate): rate is number => rate !== null,
            ),
          ),
        ),
      ),
    [values],
  );
  const heatScale = useMemo(
    () =>
      chooseByteScale(
        values.flatMap(({ samples }) =>
          samples.flatMap((sample) => {
            const rate = combinedRate(sample);
            return rate === null ? [] : [rate];
          }),
        ),
      ),
    [values],
  );
  const observed = values.some(({ samples }) =>
    samples.some(
      (sample) =>
        measuredRate(sample, "RX") !== null ||
        measuredRate(sample, "TX") !== null,
    ),
  );
  const heatObserved = values.some(({ samples }) =>
    samples.some((sample) => combinedRate(sample) !== null),
  );
  const option = useMemo(
    () =>
      (palette: ChartPalette): EChartsOption => ({
        ...baseOption(palette),
        xAxis: { type: "time", ...axisStyle(palette) },
        yAxis: {
          type: "value",
          name: `${rateScale.unit}/s`,
          min: 0,
          ...axisStyle(palette),
          axisLabel: {
            ...axisStyle(palette).axisLabel,
            formatter: formatByteAxisValue,
          },
        },
        dataZoom: zoomOption(palette),
        tooltip: {
          ...baseOption(palette).tooltip,
          formatter: (parameters) => {
            const items = Array.isArray(parameters) ? parameters : [parameters];
            const first = items[0]?.value;
            const time = Array.isArray(first)
              ? new Date(
                  typeof first[0] === "number" ? first[0] : String(first[0]),
                ).toLocaleString("zh-CN")
              : "";
            const lines = items.flatMap((item) => {
              const value = Array.isArray(item.value)
                ? item.value[1]
                : item.value;
              return typeof value === "number" &&
                Number.isFinite(value) &&
                value >= 0
                ? [
                    `${item.seriesName}  ${formatByteRate(value * rateScale.divisor, rateScale)}`,
                  ]
                : [];
            });
            return `${time}\n${lines.join("\n")}`;
          },
        },
        series: values.flatMap(({ device, name, samples }) =>
          ["RX", "TX"].map((direction) => ({
            name: `${name} · ${direction}`,
            type: "line" as const,
            showSymbol: false,
            connectNulls: false,
            data: samples.map((sample) => [
              sample.time,
              byteNumber(
                measuredRate(sample, direction as "RX" | "TX"),
                rateScale,
              ),
            ]),
            // IDs retain identity even when display names are identical.
            id: `${device.mac}-${direction}`,
          })),
        ),
      }),
    [values, rateScale],
  );
  const heatOption = useMemo(
    () =>
      (palette: ChartPalette): EChartsOption => {
        const points = values.flatMap(({ samples }, row) =>
          samples.flatMap((sample) => {
            const rate = combinedRate(sample);
            return rate === null
              ? []
              : [
                  [
                    times.indexOf(sample.time),
                    row,
                    byteNumber(rate, heatScale)!,
                  ],
                ];
          }),
        );
        const maximum = points.reduce(
          (max, point) => Math.max(max, point[2]),
          0,
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
            max: maximum || 1,
            orient: "horizontal",
            left: "center",
            bottom: 4,
            calculable: true,
            text: [
              formatByteRate(maximum * heatScale.divisor, heatScale),
              formatByteRate(0, heatScale),
            ],
            formatter: (value) =>
              typeof value === "number"
                ? `${formatByteAxisValue(value)} ${heatScale.unit}/s`
                : "—",
            textStyle: { color: palette.text },
            inRange: { color: [palette.heatLow, palette.heatHigh] },
          },
          tooltip: {
            ...baseOption(palette).tooltip,
            trigger: "item",
            formatter: (parameter) => {
              const item = Array.isArray(parameter) ? parameter[0] : parameter;
              const point = item.value;
              if (!Array.isArray(point)) return "";
              const device = values[Number(point[1])];
              const time = times[Number(point[0])];
              const sample = device?.samples.find(
                (sample) => sample.time === time,
              );
              return sample
                ? `${device.name} · ${device.device.mac}\n${new Date(time).toLocaleString("zh-CN")}\nRX + TX ${formatByteRate(combinedRate(sample), heatScale)}\n覆盖 ${sample.coverageSeconds} 秒`
                : "";
            },
          },
          series: [
            {
              type: "heatmap",
              name: `RX + TX ${heatScale.unit}/s`,
              data: points,
              itemStyle: { borderWidth: 1, borderColor: palette.surface },
            },
          ],
        };
      },
    [values, times, heatScale],
  );
  const rows = useMemo(
    () =>
      values.flatMap(({ device, name, samples }) =>
        samples.map((sample) => [
          name,
          device.mac,
          new Date(sample.time).toLocaleString("zh-CN"),
          sample.coverageSeconds > 0 ? formatBytes(sample.rxBytes) : "—",
          sample.coverageSeconds > 0 ? formatBytes(sample.txBytes) : "—",
          sample.coverageSeconds,
          formatByteRate(measuredRate(sample, "RX"), rateScale),
          formatByteRate(measuredRate(sample, "TX"), rateScale),
        ]),
      ),
    [values, rateScale],
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
            "RX 总量",
            "TX 总量",
            "有效秒数",
            `RX / ${rateScale.unit}/s`,
            `TX / ${rateScale.unit}/s`,
          ]}
          rows={rows}
          tablePageSize={100}
          lazyTable
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
          subtitle={`设备 × 时间 · RX + TX 实测速率 ${heatScale.unit}/s`}
          demo={false}
          hasData={heatObserved}
          source={source || "trafficd"}
          unavailable="等待所选设备的有效流量采样。"
          summary="深色表示较高的实测流量速率；未采样的格子留空。"
          columns={[
            "设备",
            "MAC",
            "时间",
            "RX 总量",
            "TX 总量",
            "有效秒数",
            `RX / ${rateScale.unit}/s`,
            `TX / ${rateScale.unit}/s`,
          ]}
          rows={rows}
          tablePageSize={100}
          lazyTable
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
                    ? `${formatBytes(device.activity.rxBytes)} / ${formatBytes(device.activity.txBytes)}`
                    : "未取得该范围有效计数"}
                </td>
                <td>
                  {device.activity &&
                  device.activity.coverageSeconds > 0 &&
                  !device.activity.stale
                    ? `${formatByteRate(device.activity.rxBytesPerSecond)} / ${formatByteRate(device.activity.txBytesPerSecond)}`
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
