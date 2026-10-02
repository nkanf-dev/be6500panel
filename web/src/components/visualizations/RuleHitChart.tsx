import { useMemo, useState } from 'react';
import type { EChartsOption } from 'echarts';
import { ChartFrame, ChartSelect, type VisualizationProps } from './ChartFrame';
import { EChart } from './EChart';
import { axisStyle, baseOption, type ChartPalette } from './chart-theme';
import { ruleSamples } from './demo-data';
const actions = ['direct', 'proxy', 'blocked'] as const;
const labels = { direct: '直连', proxy: '代理', blocked: '拒绝' } as const;

export function RuleHitChart({ demo = false }: VisualizationProps) {
  const [action, setAction] = useState('all');
  const [unit, setUnit] = useState('count');
  const selected = useMemo(() => actions.filter(key => action === 'all' || key === action), [action]);
  const samples = useMemo(() => [...ruleSamples].filter(rule => selected.some(key => rule[key] > 0)).sort((a, b) => selected.reduce((sum, key) => sum + b[key] - a[key], 0)), [selected]);
  const count = (rule: typeof ruleSamples[number]) => selected.reduce((sum, key) => sum + rule[key], 0);
  const total = samples.reduce((sum, rule) => sum + count(rule), 0);
  const option = useMemo(() => (p: ChartPalette): EChartsOption => ({
    ...baseOption(p),
    grid: { top: 42, right: 28, bottom: 28, left: 8, containLabel: true },
    xAxis: { type: 'value', name: unit === 'count' ? '次' : '%', min: 0, ...axisStyle(p), axisLabel: { color: p.text, formatter: unit === 'count' ? '{value}' : '{value}%' } },
    yAxis: { type: 'category', inverse: true, data: samples.map(rule => rule.rule), ...axisStyle(p), splitLine: { show: false } },
    tooltip: { ...baseOption(p).tooltip, trigger: 'axis', axisPointer: { type: 'shadow' }, formatter: parameters => {
      const item = Array.isArray(parameters) ? parameters[0] : parameters;
      const rule = samples[item.dataIndex];
      return `${rule.rule}\n${selected.map(key => `${labels[key]}  ${rule[key]} 次`).join('\n')}\n占所选命中 ${(count(rule) / total * 100).toFixed(1)}%`;
    } },
    series: selected.map(key => ({ type: 'bar', name: labels[key], stack: 'hits', barWidth: 15, data: samples.map(rule => unit === 'count' ? rule[key] : Number((rule[key] / total * 100).toFixed(2))), itemStyle: { color: p[key] }, emphasis: { focus: 'series' } })),
  }), [samples, selected, unit, total]);
  return <ChartFrame title="规则命中" subtitle="策略分类 · 按命中次数排序" demo={demo} unavailable="未接入规则计数器"
    summary={`${samples.length} 类规则 · ${total.toLocaleString()} 次所选命中`}
    controls={<><ChartSelect label="动作" value={action} onChange={setAction}><option value="all">全部动作</option>{actions.map(key => <option key={key} value={key}>{labels[key]}</option>)}</ChartSelect><ChartSelect label="显示" value={unit} onChange={setUnit}><option value="count">命中次数</option><option value="percent">所选总量占比</option></ChartSelect></>}
    hint="图例可切换动作" columns={['规则', ...selected.map(key => `${labels[key]} / 次`), '占所选命中']} rows={samples.map(rule => [rule.rule, ...selected.map(key => rule[key]), `${(count(rule) / total * 100).toFixed(1)}%`])}>
    <EChart option={option} label="规则命中堆叠条形图，按直连、代理和拒绝分类，可筛选动作及查看占比" />
  </ChartFrame>;
}
