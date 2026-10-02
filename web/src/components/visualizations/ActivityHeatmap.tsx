import { useMemo, useState } from 'react';
import type { EChartsOption } from 'echarts';
import { ChartFrame, ChartSelect, type VisualizationProps } from './ChartFrame';
import { EChart } from './EChart';
import { axisStyle, baseOption, type ChartPalette } from './chart-theme';
import { activitySamples, heatBuckets, heatDevices } from './demo-data';

export function ActivityHeatmap({ demo = false }: VisualizationProps) {
  const [device, setDevice] = useState('all');
  const devices = useMemo(() => device === 'all' ? heatDevices : heatDevices.filter(name => name === device), [device]);
  const samples = useMemo(() => activitySamples.filter(point => devices.includes(point.device)), [devices]);
  const option = useMemo(() => (p: ChartPalette): EChartsOption => ({
    ...baseOption(p), legend: { show: false },
    grid: { top: 12, left: 58, right: 18, bottom: 60, containLabel: true },
    xAxis: { type: 'category', data: heatBuckets, ...axisStyle(p), splitArea: { show: false }, splitLine: { show: false }, axisLabel: { color: p.text, interval: 3, fontSize: 10 } },
    yAxis: { type: 'category', inverse: true, data: devices, ...axisStyle(p), splitLine: { show: false } },
    visualMap: { min: 0, max: 64, orient: 'horizontal', left: 'center', bottom: 8, itemWidth: 12, itemHeight: 120, calculable: true, text: ['高', '低'], textStyle: { color: p.text, fontSize: 10 }, inRange: { color: [p.heatLow, p.heatHigh] }, outOfRange: { color: p.grid, opacity: .3 } },
    tooltip: { ...baseOption(p).tooltip, trigger: 'item', formatter: parameter => {
      const item = Array.isArray(parameter) ? parameter[0] : parameter;
      const values = item.value as number[];
      return `${devices[values[1]]} · ${heatBuckets[values[0]]}\n请求 ${values[2]} 次 / 2 分钟`;
    } },
    series: [{ type: 'heatmap', name: '请求活跃度', data: samples.map(point => [point.column, devices.indexOf(point.device), point.requests]), itemStyle: { borderWidth: 2, borderColor: p.surface }, emphasis: { itemStyle: { borderWidth: 2, borderColor: p.foreground } } }],
  }), [devices, samples]);
  return <ChartFrame title="终端活跃度" subtitle="终端 × 时间 · 2 分钟桶" demo={demo} unavailable="未接入终端活跃度"
    summary={`${devices.length} 个样本终端 · ${samples.reduce((sum, point) => sum + point.requests, 0)} 次请求`}
    controls={<ChartSelect label="终端" value={device} onChange={setDevice}><option value="all">全部终端</option>{heatDevices.map(name => <option key={name}>{name}</option>)}</ChartSelect>}
    hint="拖动色阶筛选活跃度" columns={['终端', '时间', '请求 / 次']} rows={samples.map(point => [point.device, point.time, point.requests])}>
    <EChart option={option} label="终端请求数量热力图，较深色表示较多请求，下方提供每个时间桶的数据表" />
  </ChartFrame>;
}
