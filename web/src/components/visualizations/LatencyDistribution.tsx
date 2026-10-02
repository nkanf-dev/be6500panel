import { useMemo, useState } from 'react';
import type { EChartsOption } from 'echarts';
import { ChartFrame, ChartSelect, type VisualizationProps } from './ChartFrame';
import { EChart } from './EChart';
import { axisStyle, baseOption, zoomOption, type ChartPalette } from './chart-theme';
import { latencySamples, percentile } from './demo-data';

export function LatencyDistribution({ demo = false }: VisualizationProps) {
  const [direction, setDirection] = useState('all');
  const [bucketWidth, setBucketWidth] = useState('20');
  const samples = useMemo(() => latencySamples.filter(sample => direction === 'all' || sample.direction === direction), [direction]);
  const bins = useMemo(() => {
    const width = Number(bucketWidth);
    const count = Math.floor(Math.max(...samples.map(sample => sample.ms)) / width) + 1;
    return Array.from({ length: count }, (_, i) => ({
      label: `${i * width}–${(i + 1) * width}`,
      count: samples.filter(sample => sample.ms >= i * width && sample.ms < (i + 1) * width).length,
    }));
  }, [samples, bucketWidth]);
  const option = useMemo(() => (p: ChartPalette): EChartsOption => ({
    ...baseOption(p), legend: { show: false },
    grid: { top: 28, right: 18, bottom: 60, left: 40, containLabel: true },
    xAxis: { type: 'category', name: 'ms', data: bins.map(bin => bin.label), ...axisStyle(p), splitLine: { show: false } },
    yAxis: { type: 'value', name: '请求数', minInterval: 1, ...axisStyle(p) },
    dataZoom: zoomOption(p),
    tooltip: { ...baseOption(p).tooltip, trigger: 'axis', axisPointer: { type: 'shadow' }, formatter: parameters => {
      const item = Array.isArray(parameters) ? parameters[0] : parameters;
      const bin = bins[item.dataIndex];
      return `${bin.label} ms\n${bin.count} 次请求 · ${(bin.count / samples.length * 100).toFixed(1)}%`;
    } },
    series: [{ type: 'bar', name: '请求数', data: bins.map(bin => bin.count), barMaxWidth: 40,
      itemStyle: { color: p.latency, borderRadius: [3, 3, 0, 0] },
      label: { show: true, position: 'top', color: p.text, fontSize: 10, formatter: parameter => Number(parameter.value) > 0 ? String(parameter.value) : '' },
    }],
  }), [bins, samples.length]);
  const values = samples.map(sample => sample.ms);
  return <ChartFrame title="延迟分布" subtitle="请求延迟 · 等宽直方图" demo={demo} unavailable="未接入延迟采样"
    summary={`P50 ${percentile(values, .5)} ms · P95 ${percentile(values, .95)} ms · P99 ${percentile(values, .99)} ms · ${samples.length} 次请求`}
    controls={<><ChartSelect label="方向" value={direction} onChange={setDirection}><option value="all">RX + TX</option><option value="rx">RX</option><option value="tx">TX</option></ChartSelect><ChartSelect label="桶宽" value={bucketWidth} onChange={setBucketWidth}><option value="20">20 ms</option><option value="10">10 ms</option></ChartSelect></>}
    hint="拖动底部范围缩放" columns={['延迟 / ms', '请求数', '占比']} rows={bins.map(bin => [bin.label, bin.count, `${(bin.count / samples.length * 100).toFixed(1)}%`])}>
    <EChart option={option} label="延迟等宽直方图，摘要提供P50、P95、P99，下方数据表提供每个桶的数量和比例" />
  </ChartFrame>;
}
