import { useMemo, useState } from 'react';
import type { EChartsOption } from 'echarts';
import { ChartFrame, ChartSelect, type VisualizationProps } from './ChartFrame';
import { EChart } from './EChart';
import { axisStyle, baseOption, zoomOption, type ChartPalette } from './chart-theme';
import { phaseLabels, phases, requestSamples } from './demo-data';

export function RequestWaterfall({ demo = false }: VisualizationProps) {
  const [kind, setKind] = useState('all');
  const samples = useMemo(() => requestSamples.filter(request => kind === 'all' || request.kind === kind), [kind]);
  const total = (request: typeof requestSamples[number]) => phases.reduce((sum, phase) => sum + request[phase], 0);
  const option = useMemo(() => (p: ChartPalette): EChartsOption => ({
    ...baseOption(p),
    legend: { ...(baseOption(p).legend as object), data: phases.map(phase => phaseLabels[phase]), selectedMode: false },
    grid: { top: 46, right: 22, bottom: 60, left: 8, containLabel: true },
    xAxis: { type: 'value', name: 'ms', min: 0, ...axisStyle(p) },
    yAxis: { type: 'category', inverse: true, data: samples.map(request => `${request.id}  ${request.resource}`), ...axisStyle(p), axisLabel: { color: p.text, fontSize: 10, width: 110, overflow: 'truncate' }, splitLine: { show: false } },
    dataZoom: zoomOption(p),
    tooltip: { ...baseOption(p).tooltip, trigger: 'axis', axisPointer: { type: 'shadow' }, formatter: parameters => {
      const item = Array.isArray(parameters) ? parameters[0] : parameters;
      const request = samples[item.dataIndex];
      if (!request) return '';
      return `${request.id} · ${request.resource}\n开始 +${request.start} ms · 总耗时 ${total(request)} ms\n${phases.map(phase => `${phaseLabels[phase]}  ${request[phase]} ms`).join('\n')}`;
    } },
    series: [
      { type: 'bar', name: '开始时间', stack: 'request', data: samples.map(request => request.start), silent: true, itemStyle: { color: 'transparent' }, emphasis: { disabled: true }, tooltip: { show: false }, barWidth: 14 },
      ...phases.map(phase => ({ type: 'bar' as const, name: phaseLabels[phase], stack: 'request', data: samples.map(request => request[phase]), barWidth: 14, itemStyle: { color: p[phase] }, emphasis: { focus: 'series' as const } })),
    ],
  }), [samples]);
  const end = Math.max(...samples.map(request => request.start + total(request)));
  return <ChartFrame title="请求瀑布" subtitle="请求阶段 · 相对开始时间" demo={demo} unavailable="未接入请求追踪"
    summary={`${samples.length} 个样本请求 · 完成时间 +${end} ms`}
    controls={<ChartSelect label="协议" value={kind} onChange={setKind}><option value="all">全部请求</option><option value="HTTP">HTTP</option><option value="DNS">DNS</option></ChartSelect>}
    hint="拖动底部范围缩放 · 悬停查看阶段耗时" columns={['请求', '开始 / ms', 'DNS / ms', 'TCP / ms', 'TLS / ms', 'TTFB / ms', '传输 / ms', '总计 / ms']}
    rows={samples.map(request => [`${request.id} ${request.resource}`, request.start, ...phases.map(phase => request[phase]), total(request)])}>
    <EChart option={option} label="按开始时间排列的请求阶段瀑布图，DNS、TCP、TLS、TTFB和传输阶段由不同色块表示" height={300} />
  </ChartFrame>;
}
