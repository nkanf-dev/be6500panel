// Bounded, deterministic public fixtures. No addresses, names or device snapshots.
export const trafficSamples = Array.from({ length: 61 }, (_, i) => ({
  time: `${12 + Math.floor(i / 60)}:${String(i % 60).padStart(2, '0')}`,
  rx: Math.round((17 + 6 * Math.sin(i / 5) + 9 * Math.exp(-(((i - 37) / 7) ** 2))) * 10) / 10,
  tx: Math.round((5 + 2 * Math.sin(i / 6 + 1) + 3 * Math.exp(-(((i - 37) / 8) ** 2))) * 10) / 10,
  latency: Math.round((19 + 4 * Math.cos(i / 7) + 11 * Math.exp(-(((i - 39) / 4) ** 2))) * 10) / 10,
}));
export const heatDevices = ['终端 A', '终端 B', '终端 C', '终端 D', '终端 E', '终端 F'];
export const heatBuckets = Array.from({ length: 24 }, (_, i) => `12:${String(i * 2).padStart(2, '0')}`);
export const activitySamples = heatDevices.flatMap((device, row) => heatBuckets.map((time, column) => ({
  device, time, row, column,
  requests: Math.max(0, Math.round(14 + 12 * Math.sin(column / 3 + row) + (row === 1 && column > 10 && column < 17 ? 38 : 0))),
})));
export const requestSamples = [
  { id: 'R01', resource: '/sample/index', kind: 'HTTP', start: 0, dns: 9, connect: 12, tls: 18, wait: 37, transfer: 14 },
  { id: 'R02', resource: '/sample/styles', kind: 'HTTP', start: 92, dns: 0, connect: 0, tls: 0, wait: 23, transfer: 8 },
  { id: 'R03', resource: '/sample/script', kind: 'HTTP', start: 95, dns: 0, connect: 0, tls: 0, wait: 48, transfer: 32 },
  { id: 'R04', resource: 'sample.test / A', kind: 'DNS', start: 184, dns: 24, connect: 0, tls: 0, wait: 0, transfer: 0 },
  { id: 'R05', resource: '/sample/image', kind: 'HTTP', start: 212, dns: 7, connect: 10, tls: 22, wait: 62, transfer: 41 },
  { id: 'R06', resource: '/sample/status', kind: 'HTTP', start: 228, dns: 0, connect: 0, tls: 0, wait: 27, transfer: 5 },
] as const;
export const phases = ['dns', 'connect', 'tls', 'wait', 'transfer'] as const;
export const phaseLabels = { dns: 'DNS', connect: 'TCP', tls: 'TLS', wait: 'TTFB', transfer: '传输' } as const;
export const latencySamples = Array.from({ length: 160 }, (_, i) => ({
  direction: i % 3 === 0 ? 'tx' : 'rx',
  ms: Math.round(12 + (i % 17) * 1.7 + 6 * Math.sin(i * .9) + (i % 13 === 0 ? 65 : 0) + (i % 39 === 0 ? 40 : 0)),
}));
export const ruleSamples = [
  { rule: '显式直连', direct: 248, proxy: 0, blocked: 0 },
  { rule: '局域网目标', direct: 186, proxy: 0, blocked: 0 },
  { rule: '域名策略', direct: 120, proxy: 302, blocked: 0 },
  { rule: '目标网段', direct: 166, proxy: 90, blocked: 0 },
  { rule: '拒绝列表', direct: 0, proxy: 0, blocked: 42 },
  { rule: '默认策略', direct: 35, proxy: 274, blocked: 0 },
];
export function percentile(values: readonly number[], quantile: number): number {
  if (!values.length) return 0;
  const sorted = [...values].sort((a, b) => a - b);
  return sorted[Math.max(0, Math.ceil(sorted.length * quantile) - 1)];
}
