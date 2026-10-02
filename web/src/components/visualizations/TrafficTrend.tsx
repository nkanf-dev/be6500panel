import { useMemo, useState } from 'react';
import type { EChartsOption } from 'echarts';
import { ChartFrame, ChartSelect, type VisualizationProps } from './ChartFrame';
import { EChart } from './EChart';
import { axisStyle, baseOption, zoomOption, type ChartPalette } from './chart-theme';
import { trafficSamples } from './demo-data';

export interface TrafficSample {
  time: string;
  /** Interface counters expressed as bytes per second. */
  rx: number;
  tx: number;
  /** Optional measured latency in milliseconds. */
  latency?: number;
}
export interface TrafficTrendProps extends VisualizationProps {
  samples?: readonly TrafficSample[];
  source?: string;
}
const MAX_SAMPLES = 300;
const toMB = (bytesPerSecond: number) => bytesPerSecond / 1_000_000;
const formatRate = (bytesPerSecond: number) => toMB(bytesPerSecond).toFixed(3);
const measuredLatency = (latency: number | undefined): latency is number => typeof latency === 'number' && Number.isFinite(latency) && latency >= 0;
// The original deterministic fixtures express Mbps; normalize to the public bytes/s contract.
const demoSamples: readonly TrafficSample[] = trafficSamples.map(point => ({ ...point, rx: point.rx * 125_000, tx: point.tx * 125_000 }));

export function TrafficTrend({ demo = false, samples: actualSamples, source = '接口采样' }: TrafficTrendProps) {
  const [windowMinutes, setWindowMinutes] = useState('60');
  const [pointLimit, setPointLimit] = useState('300');
  const [metric, setMetric] = useState('all');
  const actual = !!actualSamples?.length;
  const isDemo = !actual && demo;
  const samples = useMemo(() => actual
    ? actualSamples!.slice(-Math.min(MAX_SAMPLES, Number(pointLimit)))
    : isDemo ? demoSamples.slice(-(Number(windowMinutes) + 1)) : [],
  [actual, actualSamples, isDemo, pointLimit, windowMinutes]);
  const hasLatency = samples.some(point => measuredLatency(point.latency));
  const selectedMetric = hasLatency ? metric : 'traffic';
  const option = useMemo(() => (p: ChartPalette): EChartsOption => ({
    ...baseOption(p),
    xAxis: { type: 'category', boundaryGap: false, data: samples.map(point => point.time), ...axisStyle(p), splitLine: { show: false } },
    yAxis: [
      { type: 'value', name: 'MB/s', min: 0, ...axisStyle(p) },
      ...(hasLatency ? [{ type: 'value' as const, name: 'ms', min: 0, position: 'right' as const, ...axisStyle(p), splitLine: { show: false } }] : []),
    ],
    dataZoom: zoomOption(p),
    series: [
      ...(selectedMetric !== 'latency' ? [
        { type: 'line' as const, name: 'RX', data: samples.map(point => toMB(point.rx)), showSymbol: false, smooth: isDemo ? .2 : false, lineStyle: { width: 2, color: p.rx }, itemStyle: { color: p.rx }, areaStyle: { opacity: .07 } },
        { type: 'line' as const, name: 'TX', data: samples.map(point => toMB(point.tx)), showSymbol: false, smooth: isDemo ? .2 : false, lineStyle: { width: 2, color: p.tx }, itemStyle: { color: p.tx } },
      ] : []),
      ...(hasLatency && selectedMetric !== 'traffic' ? [{ type: 'line' as const, name: '延迟', yAxisIndex: 1, data: samples.map(point => measuredLatency(point.latency) ? point.latency : null), connectNulls: false, showSymbol: false, smooth: isDemo ? .2 : false, lineStyle: { width: 1.5, type: 'dashed' as const, color: p.latency }, itemStyle: { color: p.latency } }] : []),
    ],
    tooltip: { ...baseOption(p).tooltip, formatter: parameters => {
      const items = Array.isArray(parameters) ? parameters : [parameters];
      return `${items[0]?.name ?? ''}\n${items.filter(item => typeof item.value === 'number').map(item => `${item.seriesName}  ${item.seriesName === '延迟' ? item.value : Number(item.value).toFixed(3)} ${item.seriesName === '延迟' ? 'ms' : 'MB/s'}`).join('\n')}`;
    } },
  }), [samples, hasLatency, selectedMetric, isDemo]);
  const latest = samples[samples.length - 1];
  const summary = latest ? `最新 RX ${formatRate(latest.rx)} MB/s · TX ${formatRate(latest.tx)} MB/s${measuredLatency(latest.latency) ? ` · 延迟 ${latest.latency} ms` : ''}` : undefined;
  return <ChartFrame title={hasLatency ? '流量与延迟' : '接口流量'} subtitle={isDemo ? 'RX / TX · 1 分钟采样' : 'RX / TX · 接口计数器速率'}
    demo={isDemo} hasData={samples.length > 0} source={source} emptyLabel="未采样" unavailable="未采样" summary={summary}
    controls={<>{actual
      ? <ChartSelect label="样本范围" value={pointLimit} onChange={setPointLimit}><option value="300">最近 300 点</option><option value="120">最近 120 点</option><option value="30">最近 30 点</option></ChartSelect>
      : <ChartSelect label="时间范围" value={windowMinutes} onChange={setWindowMinutes}><option value="60">最近 60 分钟</option><option value="30">最近 30 分钟</option><option value="15">最近 15 分钟</option></ChartSelect>}
      {hasLatency && <ChartSelect label="指标" value={metric} onChange={setMetric}><option value="all">流量 + 延迟</option><option value="traffic">RX / TX</option><option value="latency">延迟</option></ChartSelect>}
    </>}
    hint="拖动底部范围缩放 · 图例切换指标" columns={['时间', 'RX / MB/s', 'TX / MB/s', ...(hasLatency ? ['延迟 / ms'] : [])]}
    rows={samples.map(point => [point.time, formatRate(point.rx), formatRate(point.tx), ...(hasLatency ? [measuredLatency(point.latency) ? point.latency : '—'] : [])])}>
    <EChart option={option} label={`接口 RX、TX 流量${hasLatency ? '和延迟' : ''}趋势，使用下方数据表读取准确值`} />
  </ChartFrame>;
}
