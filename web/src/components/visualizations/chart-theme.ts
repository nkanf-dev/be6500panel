import type { EChartsOption } from 'echarts';

export interface ChartPalette {
  text: string; foreground: string; grid: string; surface: string; tooltip: string;
  rx: string; tx: string; latency: string; direct: string; proxy: string; blocked: string;
  dns: string; connect: string; tls: string; wait: string; transfer: string;
  heatLow: string; heatHigh: string; font: string;
}
const properties: Record<keyof Omit<ChartPalette, 'font'>, string> = {
  text: '--chart-text', foreground: '--foreground', grid: '--chart-grid', surface: '--chart-surface', tooltip: '--chart-tooltip',
  rx: '--chart-rx', tx: '--chart-tx', latency: '--chart-latency', direct: '--chart-direct', proxy: '--chart-proxy', blocked: '--chart-blocked',
  dns: '--chart-dns', connect: '--chart-connect', tls: '--chart-tls', wait: '--chart-wait', transfer: '--chart-transfer',
  heatLow: '--chart-heat-low', heatHigh: '--chart-heat-high',
};

/** CSS Color 4 OKLCH is not accepted by ZRender's color parser. Convert tokens
 * to sRGB before handing them to ECharts. Preserve percentage or decimal alpha. */
export function normalizeChartColor(input: string): string {
  const value = input.trim();
  if (!value.startsWith('oklch(')) return value;
  const parts = value.slice(6, -1).trim().split(/\s*\/\s*|\s+/);
  const lightness = parts[0].endsWith('%') ? parseFloat(parts[0]) / 100 : Number(parts[0]);
  const chroma = Number(parts[1]);
  const hue = parseFloat(parts[2]) * Math.PI / 180;
  const alpha = parts[3] === undefined ? 1 : parts[3].endsWith('%') ? parseFloat(parts[3]) / 100 : Number(parts[3]);
  if (![lightness, chroma, hue, alpha].every(Number.isFinite)) return value;
  const a = chroma * Math.cos(hue), b = chroma * Math.sin(hue);
  const l = (lightness + .3963377774 * a + .2158037573 * b) ** 3;
  const m = (lightness - .1055613458 * a - .0638541728 * b) ** 3;
  const s = (lightness - .0894841775 * a - 1.291485548 * b) ** 3;
  const encode = (channel: number) => Math.round(Math.min(1, Math.max(0, channel <= .0031308 ? 12.92 * channel : 1.055 * Math.max(channel, 0) ** (1 / 2.4) - .055)) * 255);
  return `rgba(${encode(4.0767416621 * l - 3.3077115913 * m + .2309699292 * s)}, ${encode(-1.2684380046 * l + 2.6097574011 * m - .3413193965 * s)}, ${encode(-.0041960863 * l - .7034186147 * m + 1.707614701 * s)}, ${Math.min(1, Math.max(0, alpha))})`;
}

export function readChartPalette(element: HTMLElement): ChartPalette {
  // A probe resolves var() aliases (getPropertyValue alone can preserve var()).
  const probe = document.createElement('span');
  probe.style.display = 'none';
  element.append(probe);
  const result = {} as ChartPalette;
  for (const [key, property] of Object.entries(properties)) {
    probe.style.color = `var(${property})`;
    result[key as keyof Omit<ChartPalette, 'font'>] = normalizeChartColor(getComputedStyle(probe).color);
  }
  result.font = getComputedStyle(element).fontFamily;
  probe.remove();
  return result;
}

export function baseOption(palette: ChartPalette): EChartsOption {
  return {
    backgroundColor: 'transparent',
    color: [palette.rx, palette.tx, palette.latency, palette.dns],
    textStyle: { color: palette.text, fontFamily: palette.font, fontSize: 11 },
    grid: { top: 46, right: 28, bottom: 60, left: 52, containLabel: true },
    tooltip: {
      trigger: 'axis', renderMode: 'richText', confine: true,
      backgroundColor: palette.tooltip, borderColor: palette.grid, borderWidth: 1,
      textStyle: { color: palette.foreground, fontFamily: palette.font, fontSize: 12 },
      axisPointer: { type: 'line', lineStyle: { color: palette.text, type: 'dashed' } },
    },
    legend: { top: 8, left: 14, textStyle: { color: palette.text, fontFamily: palette.font, fontSize: 11 }, itemWidth: 14, itemHeight: 8 },
    aria: { enabled: true, decal: { show: true } },
  };
}
export const axisStyle = (palette: ChartPalette) => ({
  axisLine: { lineStyle: { color: palette.grid } }, axisTick: { show: false },
  axisLabel: { color: palette.text, fontSize: 11 },
  splitLine: { lineStyle: { color: palette.grid, type: 'dashed' as const } },
  nameTextStyle: { color: palette.text, fontSize: 11 },
});
export const zoomOption = (palette: ChartPalette): EChartsOption['dataZoom'] => [
  { type: 'inside', xAxisIndex: 0, filterMode: 'none', zoomOnMouseWheel: 'ctrl', moveOnMouseWheel: false },
  { type: 'slider', xAxisIndex: 0, bottom: 8, height: 18, borderColor: palette.grid,
    fillerColor: palette.grid, handleStyle: { color: palette.text }, textStyle: { color: palette.text, fontSize: 10 },
    dataBackground: { lineStyle: { color: palette.text }, areaStyle: { color: palette.grid } },
  },
];
