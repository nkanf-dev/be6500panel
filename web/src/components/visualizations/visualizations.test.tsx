import { cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { ThemeProvider } from '../../theme';
import { ActivityHeatmap, LatencyDistribution, RequestWaterfall, RuleHitChart, TrafficTrend } from './index';
vi.mock('./EChart', () => ({ EChart: ({ label }: { label: string }) => <div role="img" aria-label={label} /> }));
afterEach(cleanup);
const components = [TrafficTrend, ActivityHeatmap, RequestWaterfall, LatencyDistribution, RuleHitChart];
const renderChart = (Component: typeof TrafficTrend, demo = true) => render(<ThemeProvider defaultMode="light"><Component demo={demo} /></ThemeProvider>);

describe('visualization public contract', () => {
  it.each(components)('never replaces unavailable telemetry with samples (%s)', Component => {
    renderChart(Component, false);
    expect(screen.getByText('未接入')).toBeTruthy();
    expect(screen.queryByText('演示数据')).toBeNull();
    expect(screen.queryByRole('img')).toBeNull();
    expect(screen.queryByRole('table')).toBeNull();
    expect(screen.getByRole('status').textContent).toContain('未接入');
  });
  it('defaults to an honest empty state', () => {
    render(<ThemeProvider><TrafficTrend /></ThemeProvider>);
    expect(screen.getByRole('status').textContent).toBe('—未接入流量遥测');
  });
  it.each(components)('labels demo samples with a readable table alternative (%s)', Component => {
    renderChart(Component);
    expect(screen.getByText('演示数据')).toBeTruthy();
    expect(screen.getByText('来源：固定样本')).toBeTruthy();
    expect(screen.getByRole('img')).toBeTruthy();
    fireEvent.click(screen.getByText(/查看数据表/));
    expect(screen.getByRole('table', { hidden: true })).toBeTruthy();
    expect(screen.getAllByRole('columnheader', { hidden: true }).length).toBeGreaterThan(2);
  });
  it('changes traffic range and metrics', () => {
    renderChart(TrafficTrend);
    fireEvent.change(screen.getByLabelText('时间范围'), { target: { value: '15' } });
    expect(screen.getByText('16 条')).toBeTruthy();
    fireEvent.change(screen.getByLabelText('指标'), { target: { value: 'latency' } });
    expect((screen.getByLabelText('指标') as HTMLSelectElement).value).toBe('latency');
  });
  it('gives chart filters concise accessible names without option text', () => {
    renderChart(ActivityHeatmap);
    expect(screen.getByRole('combobox', { name: '终端' }).getAttribute('aria-label')).toBe('终端');
  });
  it('filters heatmap by a neutral sample device', () => {
    renderChart(ActivityHeatmap);
    fireEvent.change(screen.getByLabelText('终端'), { target: { value: '终端 B' } });
    expect(screen.getByText('24 条')).toBeTruthy();
    const table = screen.getByRole('table', { hidden: true });
    expect(within(table).queryAllByText('终端 A')).toHaveLength(0);
  });
  it('filters the waterfall by protocol', () => {
    renderChart(RequestWaterfall);
    fireEvent.change(screen.getByLabelText('协议'), { target: { value: 'DNS' } });
    expect(screen.getByText('1 条')).toBeTruthy();
    expect(screen.getByText('1 个样本请求 · 完成时间 +208 ms')).toBeTruthy();
  });
  it('updates latency percentiles and bucket counts', () => {
    renderChart(LatencyDistribution);
    const summary = screen.getByText(/P50/).textContent;
    fireEvent.change(screen.getByLabelText('方向'), { target: { value: 'tx' } });
    expect(screen.getByText(/P50/).textContent).not.toBe(summary);
    const before = screen.getByText(/条$/).textContent;
    fireEvent.change(screen.getByLabelText('桶宽'), { target: { value: '10' } });
    expect(screen.getByText(/条$/).textContent).not.toBe(before);
  });
  it('filters rule actions and changes percentage view', () => {
    renderChart(RuleHitChart);
    fireEvent.change(screen.getByLabelText('动作'), { target: { value: 'blocked' } });
    expect(screen.getByText('1 条')).toBeTruthy();
    expect(screen.getByText('1 类规则 · 42 次所选命中')).toBeTruthy();
    fireEvent.change(screen.getByLabelText('显示'), { target: { value: 'percent' } });
    expect((screen.getByLabelText('显示') as HTMLSelectElement).value).toBe('percent');
  });
});
