import { useMemo, useState } from 'react';
import type { EChartsOption } from 'echarts';
import { ChartFrame, ChartSelect, type VisualizationProps } from './ChartFrame';
import { EChart } from './EChart';
import { axisStyle, baseOption, zoomOption, type ChartPalette } from './chart-theme';
import { trafficSamples } from './demo-data';

export function TrafficTrend({ demo = false }: VisualizationProps) {
  const [windowMinutes, setWindowMinutes] = useState('60');
  const [metric, setMetric] = useState('all');
  const samples = useMemo(() => trafficSamples.slice(-(Number(windowMinutes) + 1)), [windowMinutes]);
  const option = useMemo(() => (p: ChartPalette): EChartsOption => ({
    ...baseOption(p),
    xAxis: { type: 'category', boundaryGap: false, data: samples.map(point => point.time), ...axisStyle(p), splitLine: { show: false } },
    yAxis: [
      { type: 'value', name: 'Mbps', min: 0, ...axisStyle(p) },
      { type: 'value', name: 'ms', min: 0, position: 'right', ...axisStyle(p), splitLine: { show: false } },
    ],
    dataZoom: zoomOption(p),
    series: [
      ...(metric !== 'latency' ? [
        { type: 'line' as const, name: 'RX', data: samples.map(point => point.rx), showSymbol: false, smooth: .2, lineStyle: { width: 2, color: p.rx }, itemStyle: { color: p.rx }, areaStyle: { opacity: .07 } },
        { type: 'line' as const, name: 'TX', data: samples.map(point => point.tx), showSymbol: false, smooth: .2, lineStyle: { width: 2, color: p.tx }, itemStyle: { color: p.tx } },
      ] : []),
      ...(metric !== 'traffic' ? [{ type: 'line' as const, name: '延迟', yAxisIndex: 1, data: samples.map(point => point.latency), showSymbol: false, smooth: .2, lineStyle: { width: 1.5, type: 'dashed' as const, color: p.latency }, itemStyle: { color: p.latency } }] : []),
    ],
    tooltip: { ...baseOption(p).tooltip, formatter: parameters => {
      const items = Array.isArray(parameters) ? parameters : [parameters];
      return `${items[0]?.name ?? ''}\n${items.map(item => `${item.seriesName}  ${item.value} ${item.seriesName === '延迟' ? 'ms' : 'Mbps'}`).join('\n')}`;
    } },
  }), [samples, metric]);
  const latest = samples[samples.length - 1];
  return <ChartFrame title="流量与延迟" subtitle="RX / TX · 1 分钟采样" demo={demo} unavailable="未接入流量遥测"
    summary={`最新 RX ${latest.rx} Mbps · TX ${latest.tx} Mbps · 延迟 ${latest.latency} ms`}
    controls={<><ChartSelect label="时间范围" value={windowMinutes} onChange={setWindowMinutes}><option value="60">最近 60 分钟</option><option value="30">最近 30 分钟</option><option value="15">最近 15 分钟</option></ChartSelect><ChartSelect label="指标" value={metric} onChange={setMetric}><option value="all">流量 + 延迟</option><option value="traffic">RX / TX</option><option value="latency">延迟</option></ChartSelect></>}
    hint="拖动底部范围缩放 · 图例切换指标" columns={['时间', 'RX / Mbps', 'TX / Mbps', '延迟 / ms']}
    rows={samples.map(point => [point.time, point.rx, point.tx, point.latency])}>
    <EChart option={option} label="每分钟 RX、TX 流量和延迟趋势，使用下方数据表读取准确值" />
  </ChartFrame>;
}
