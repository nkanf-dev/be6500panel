import { useEffect, useRef, useState } from 'react';
import type { EChartsOption } from 'echarts';
import type { EChartsType } from 'echarts/core';
import { useTheme } from '../../theme';
import { readChartPalette, type ChartPalette } from './chart-theme';

export interface ChartDataClick { value: unknown; componentType?: string; seriesType?: string }
interface Props { option: (palette: ChartPalette) => EChartsOption; label: string; height?: number; onDataClick?: (point: ChartDataClick) => void }
export function EChart({ option, label, height = 280, onDataClick }: Props) {
  const host = useRef<HTMLDivElement>(null);
  const clickHandler = useRef(onDataClick);
  useEffect(() => { clickHandler.current = onDataClick; }, [onDataClick]);
  const { resolvedTheme } = useTheme();
  const [error, setError] = useState(false);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    const element = host.current;
    if (!element) return;
    let disposed = false;
    let chart: EChartsType | undefined;
    let starting = false;
    let inViewport = !('IntersectionObserver' in window);
    let frame = 0;
    const reduced = window.matchMedia?.('(prefers-reduced-motion: reduce)');
    const visible = () => !document.hidden && inViewport && element.clientWidth > 0;
    setError(false);
    setLoading(true);

    const paint = () => {
      if (disposed || !chart || !visible()) return;
      chart.setOption({ ...option(readChartPalette(element)), animation: !reduced?.matches, animationDuration: 200, animationDurationUpdate: 120 }, { notMerge: true, lazyUpdate: true });
      chart.resize();
      setLoading(false);
    };
    const start = async () => {
      if (disposed || !visible() || chart || starting) return;
      starting = true;
      try {
        const runtime = await import('./echarts-runtime');
        if (disposed || !visible()) return;
        chart = runtime.init(element, undefined, { renderer: 'canvas' });
        chart.on('click', (point: unknown) => {
          if (disposed || !point || typeof point !== 'object') return;
          const data = point as { value?: unknown; componentType?: string; seriesType?: string };
          clickHandler.current?.({ value: data.value, componentType: data.componentType, seriesType: data.seriesType });
        });
        paint();
      } catch {
        if (!disposed) { setError(true); setLoading(false); }
      } finally { starting = false; }
    };
    const synchronize = () => {
      if (!visible()) {
        cancelAnimationFrame(frame);
        // Release animation/render work for offscreen or background views.
        chart?.dispose();
        chart = undefined;
        return;
      }
      if (!chart) { void start(); return; }
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(() => { if (visible()) chart?.resize(); });
    };
    const resize = typeof ResizeObserver !== 'undefined' ? new ResizeObserver(synchronize) : undefined;
    resize?.observe(element);
    const intersection = typeof IntersectionObserver !== 'undefined' ? new IntersectionObserver(entries => {
      inViewport = entries[0]?.isIntersecting ?? false;
      synchronize();
    }, { rootMargin: '80px' }) : undefined;
    intersection?.observe(element);
    const onMotion = () => paint();
    reduced?.addEventListener('change', onMotion);
    document.addEventListener('visibilitychange', synchronize);
    window.addEventListener('resize', synchronize);
    synchronize();
    return () => {
      disposed = true;
      cancelAnimationFrame(frame);
      resize?.disconnect();
      intersection?.disconnect();
      reduced?.removeEventListener('change', onMotion);
      document.removeEventListener('visibilitychange', synchronize);
      window.removeEventListener('resize', synchronize);
      chart?.dispose();
    };
  }, [option, resolvedTheme]);

  return <div className="viz-canvas-wrap" style={{ minHeight: height }}>
    <div ref={host} className="viz-canvas" style={{ height }} role="img" aria-label={label} />
    {loading && !error && <span className="viz-render-status" role="status">图表加载中</span>}
    {error && <span className="viz-render-status" role="status">图表暂不可用 · 数据表仍可查看</span>}
  </div>;
}
