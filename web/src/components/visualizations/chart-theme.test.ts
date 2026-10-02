import { describe, expect, it } from 'vitest';
import { normalizeChartColor } from './chart-theme';
import { activitySamples, heatBuckets, latencySamples, percentile, requestSamples, trafficSamples } from './demo-data';

describe('chart tokens and bounded fixtures', () => {
  it('converts OKLCH to ECharts-compatible sRGB and preserves alpha', () => {
    expect(normalizeChartColor('oklch(100% 0 0)')).toBe('rgba(255, 255, 255, 1)');
    expect(normalizeChartColor('oklch(0% 0 0 / 40%)')).toBe('rgba(0, 0, 0, 0.4)');
    expect(normalizeChartColor('oklch(.5 .1 255 / .07)')).toMatch(/^rgba\(\d+, \d+, \d+, 0.07\)$/);
    expect(normalizeChartColor('rgb(12, 23, 34)')).toBe('rgb(12, 23, 34)');
  });
  it('uses finite, bounded, chronological synthetic samples', () => {
    expect(trafficSamples).toHaveLength(61);
    expect(trafficSamples.every(sample => [sample.rx, sample.tx, sample.latency].every(value => Number.isFinite(value) && value >= 0))).toBe(true);
    expect(trafficSamples.map(sample => sample.time)).toEqual(trafficSamples.map(sample => sample.time).sort());
    expect(activitySamples).toHaveLength(144);
    expect(new Set(heatBuckets).size).toBe(24);
    expect(requestSamples.map(request => request.start)).toEqual(requestSamples.map(request => request.start).sort((a, b) => a - b));
    expect(latencySamples).toHaveLength(160);
  });
  it('calculates exact nearest-rank percentiles', () => {
    expect(percentile([9, 3, 7, 1], .5)).toBe(3);
    expect(percentile([9, 3, 7, 1], .95)).toBe(9);
  });
});
