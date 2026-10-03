import { useEffect, useRef, useState } from "react";
import type { EChartsOption } from "echarts";
import type { EChartsType } from "echarts/core";
import { useTheme } from "../../theme";
import { readChartPalette, type ChartPalette } from "./chart-theme";

export interface ChartDataClick {
  value: unknown;
  componentType?: string;
  seriesType?: string;
}
interface Props {
  option: (palette: ChartPalette) => EChartsOption;
  label: string;
  height?: number;
  onDataClick?: (point: ChartDataClick) => void;
}
export function EChart({ option, label, height = 280, onDataClick }: Props) {
  const host = useRef<HTMLDivElement>(null);
  const latestOption = useRef(option);
  const clickHandler = useRef(onDataClick);
  const updateChart = useRef<() => void>(() => {});
  const { resolvedTheme } = useTheme();
  const [error, setError] = useState(false);
  const [loading, setLoading] = useState(true);
  useEffect(() => {
    clickHandler.current = onDataClick;
  }, [onDataClick]);
  useEffect(() => {
    latestOption.current = option;
    updateChart.current();
  }, [option, resolvedTheme]);

  useEffect(() => {
    const element = host.current;
    if (!element) return;
    let disposed = false;
    let chart: EChartsType | undefined;
    let starting = false;
    let inViewport = !("IntersectionObserver" in window);
    let frame = 0;
    let firstPaint = true;
    const reduced = window.matchMedia?.("(prefers-reduced-motion: reduce)");
    const visible = () =>
      !document.hidden && inViewport && element.clientWidth > 0;
    const paint = (allowAnimation = false) => {
      if (disposed || !chart || !visible()) return;
      // Preserve user zoom/legend while fresh source samples replace series data.
      const next = latestOption.current(readChartPalette(element));
      const previous = chart.getOption?.() as
        | {
            dataZoom?: Array<{ start?: number; end?: number }>;
            legend?: Array<{ selected?: Record<string, boolean> }>;
          }
        | undefined;
      const zoom = next.dataZoom;
      if (Array.isArray(zoom) && previous?.dataZoom) {
        next.dataZoom = zoom.map((item, index) => {
          const old = previous.dataZoom?.[index];
          return old &&
            typeof old.start === "number" &&
            typeof old.end === "number"
            ? { ...item, start: old.start, end: old.end }
            : item;
        });
      }
      const priorSelection = previous?.legend?.[0]?.selected;
      if (next.legend && !Array.isArray(next.legend) && priorSelection)
        next.legend = { ...next.legend, selected: priorSelection };
      chart.setOption(
        {
          ...next,
          animation: (firstPaint || allowAnimation) && !reduced?.matches,
          animationDuration: 200,
          animationDurationUpdate: 0,
        },
        { notMerge: false, replaceMerge: ["series"], lazyUpdate: false },
      );
      chart.resize();
      firstPaint = false;
      setError(false);
      setLoading(false);
    };
    const start = async () => {
      if (disposed || !visible() || chart || starting) return;
      starting = true;
      try {
        const runtime = await import("./echarts-runtime");
        if (disposed || !visible()) return;
        chart = runtime.init(element, undefined, { renderer: "canvas" });
        chart.on("click", (point: unknown) => {
          if (disposed || !point || typeof point !== "object") return;
          const data = point as {
            value?: unknown;
            componentType?: string;
            seriesType?: string;
          };
          clickHandler.current?.({
            value: data.value,
            componentType: data.componentType,
            seriesType: data.seriesType,
          });
        });
        paint();
      } catch {
        if (!disposed) {
          setError(true);
          setLoading(false);
        }
      } finally {
        starting = false;
      }
    };
    const synchronize = () => {
      cancelAnimationFrame(frame);
      // Offscreen/background charts pause work without losing their canvas/state.
      if (!visible()) return;
      if (!chart) {
        void start();
        return;
      }
      frame = requestAnimationFrame(() => paint());
    };
    updateChart.current = synchronize;
    const resize =
      typeof ResizeObserver !== "undefined"
        ? new ResizeObserver(synchronize)
        : undefined;
    resize?.observe(element);
    const intersection =
      typeof IntersectionObserver !== "undefined"
        ? new IntersectionObserver(
            (entries) => {
              inViewport = entries[0]?.isIntersecting ?? false;
              synchronize();
            },
            { rootMargin: "80px" },
          )
        : undefined;
    intersection?.observe(element);
    const onMotion = () => paint(true);
    reduced?.addEventListener("change", onMotion);
    document.addEventListener("visibilitychange", synchronize);
    window.addEventListener("resize", synchronize);
    synchronize();
    return () => {
      disposed = true;
      updateChart.current = () => {};
      cancelAnimationFrame(frame);
      resize?.disconnect();
      intersection?.disconnect();
      reduced?.removeEventListener("change", onMotion);
      document.removeEventListener("visibilitychange", synchronize);
      window.removeEventListener("resize", synchronize);
      chart?.dispose();
    };
  }, []);
  return (
    <div className="viz-canvas-wrap" style={{ minHeight: height }}>
      <div
        ref={host}
        className="viz-canvas"
        style={{ height }}
        role="img"
        aria-label={label}
      />
      {loading && !error && (
        <span className="viz-render-status" role="status">
          图表加载中
        </span>
      )}
      {error && (
        <span className="viz-render-status" role="status">
          图表暂不可用 · 数据表仍可查看
        </span>
      )}
    </div>
  );
}
