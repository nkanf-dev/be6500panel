import { act, cleanup, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ThemeProvider } from '../../theme';
import { EChart } from './EChart';

const mocks = vi.hoisted(() => ({ init: vi.fn(), setOption: vi.fn(), resize: vi.fn(), dispose: vi.fn(), on: vi.fn(), disconnect: vi.fn() }));
vi.mock('./echarts-runtime', () => ({ init: mocks.init }));
vi.mock('./chart-theme', () => ({ readChartPalette: () => ({ rx: 'rgb(24, 86, 180)' }) }));
let hidden = false;
let reduced = false;
let resizeCallback: () => void;
let motionListeners: Set<() => void>;
beforeEach(() => {
  vi.clearAllMocks();
  hidden = false; reduced = false;
  motionListeners = new Set();
  localStorage.clear();
  vi.spyOn(HTMLElement.prototype, 'clientWidth', 'get').mockReturnValue(680);
  vi.spyOn(document, 'hidden', 'get').mockImplementation(() => hidden);
  mocks.init.mockReturnValue({ setOption: mocks.setOption, resize: mocks.resize, dispose: mocks.dispose, on: mocks.on });
  vi.stubGlobal('ResizeObserver', class {
    constructor(callback: () => void) { resizeCallback = callback; }
    observe() {} disconnect = mocks.disconnect;
  });
  vi.stubGlobal('matchMedia', (query: string) => ({
    get matches() { return query.includes('reduced-motion') && reduced; },
    addEventListener: (_: string, listener: () => void) => { if (query.includes('reduced-motion')) motionListeners.add(listener); },
    removeEventListener: (_: string, listener: () => void) => motionListeners.delete(listener),
  }));
});
afterEach(() => { cleanup(); vi.unstubAllGlobals(); vi.restoreAllMocks(); });
const option = vi.fn(() => ({ series: [{ type: 'line' as const, data: [1, 2, 3] }] }));
const renderChart = () => render(<ThemeProvider defaultMode="light"><EChart option={option} label="sample trend" /></ThemeProvider>);

describe('lazy ECharts lifecycle', () => {
  it('initializes on demand, resizes and disposes observers/canvas on unmount', async () => {
    const view = renderChart();
    await waitFor(() => expect(mocks.init).toHaveBeenCalledTimes(1));
    expect(screen.queryByText('图表加载中')).toBeNull();
    expect(mocks.setOption).toHaveBeenCalledWith(expect.objectContaining({ animation: true }), expect.anything());
    act(() => resizeCallback());
    await waitFor(() => expect(mocks.resize.mock.calls.length).toBeGreaterThan(1));
    view.unmount();
    expect(mocks.dispose).toHaveBeenCalledTimes(1);
    expect(mocks.disconnect).toHaveBeenCalledTimes(1);
    expect(motionListeners.size).toBe(0);
  });
  it('does not initialize a hidden view and releases work while hidden', async () => {
    hidden = true;
    renderChart();
    await act(async () => {});
    expect(mocks.init).not.toHaveBeenCalled();
    act(() => { hidden = false; document.dispatchEvent(new Event('visibilitychange')); });
    await waitFor(() => expect(mocks.init).toHaveBeenCalledTimes(1));
    act(() => { hidden = true; document.dispatchEvent(new Event('visibilitychange')); });
    expect(mocks.dispose).toHaveBeenCalledTimes(1);
    act(() => resizeCallback());
    expect(mocks.init).toHaveBeenCalledTimes(1);
    act(() => { hidden = false; document.dispatchEvent(new Event('visibilitychange')); });
    await waitFor(() => expect(mocks.init).toHaveBeenCalledTimes(2));
  });
  it('disables animations for reduced motion and follows preference changes', async () => {
    reduced = true;
    renderChart();
    await waitFor(() => expect(mocks.setOption).toHaveBeenCalledWith(expect.objectContaining({ animation: false }), expect.anything()));
    act(() => { reduced = false; motionListeners.forEach(listener => listener()); });
    expect(mocks.setOption).toHaveBeenLastCalledWith(expect.objectContaining({ animation: true }), expect.anything());
  });
  it('exposes render failure without removing the accessible alternative', async () => {
    mocks.init.mockImplementation(() => { throw new Error('unavailable canvas'); });
    renderChart();
    expect(await screen.findByText('图表暂不可用 · 数据表仍可查看')).toBeTruthy();
    expect(screen.getByRole('img', { name: 'sample trend' })).toBeTruthy();
  });
  it('forwards bounded data metadata to the latest callback without recreating the chart', async () => {
    const first = vi.fn();
    const next = vi.fn();
    const view = render(<ThemeProvider defaultMode="light"><EChart option={option} label="sample heatmap" onDataClick={first} /></ThemeProvider>);
    await waitFor(() => expect(mocks.init).toHaveBeenCalledTimes(1));
    expect(mocks.on).toHaveBeenCalledWith('click', expect.any(Function));
    const listener = mocks.on.mock.calls[0][1] as (value: unknown) => void;
    act(() => listener({ value: [3, 2, 100], componentType: 'series', seriesType: 'heatmap', name: 'not forwarded' }));
    expect(first).toHaveBeenCalledWith({ value: [3, 2, 100], componentType: 'series', seriesType: 'heatmap' });
    view.rerender(<ThemeProvider defaultMode="light"><EChart option={option} label="sample heatmap" onDataClick={next} /></ThemeProvider>);
    act(() => listener({ value: [1, 0, 0], componentType: 'series', seriesType: 'heatmap' }));
    expect(next).toHaveBeenCalledTimes(1);
    expect(mocks.init).toHaveBeenCalledTimes(1);
    view.unmount();
    act(() => listener({ value: [1, 0, 0] }));
    expect(next).toHaveBeenCalledTimes(1);
  });

});
