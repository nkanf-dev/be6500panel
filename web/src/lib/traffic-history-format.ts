import type { TrafficHistory } from "./traffic-history-contracts";

/** Date, chart tooltip, table, start-of-recording and export all use explicit UTC. */
export function trafficTimestamp(value: string): string {
  const time = Date.parse(value);
  return Number.isFinite(time) ? new Date(time).toISOString() : value;
}
export function trafficDuration(seconds: number): string {
  if (seconds < 60) return `${Number(seconds.toFixed(1))} 秒`;
  const days = Math.floor(seconds / 86400);
  const hours = Math.floor((seconds % 86400) / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  const remainder = Number((seconds % 60).toFixed(1));
  return [
    days && `${days} 天`,
    hours && `${hours} 小时`,
    minutes && `${minutes} 分钟`,
    remainder && `${remainder} 秒`,
  ]
    .filter(Boolean)
    .join(" ");
}
export function trafficHistoryCSV(history: TrafficHistory): string {
  const header = [
    "range",
    "resolutionSeconds",
    "timeUTC",
    "rxBytesPerSecond",
    "txBytesPerSecond",
    "rxPeakBytesPerSecond",
    "txPeakBytesPerSecond",
    "rxBytes",
    "txBytes",
    "coverageSeconds",
  ];
  const rows = history.samples.map((point) => [
    history.range,
    history.resolutionSeconds,
    trafficTimestamp(point.time),
    ...(point.coverageSeconds > 0
      ? [
          point.rx,
          point.tx,
          point.rxPeak,
          point.txPeak,
          point.rxBytes,
          point.txBytes,
        ]
      : ["", "", "", "", "", ""]),
    point.coverageSeconds,
  ]);
  return [header, ...rows].map((row) => row.join(",")).join("\r\n");
}
export function downloadTrafficHistory(history: TrafficHistory): void {
  const url = URL.createObjectURL(
    new Blob(["\uFEFF", trafficHistoryCSV(history)], {
      type: "text/csv;charset=utf-8",
    }),
  );
  const link = document.createElement("a");
  link.href = url;
  link.download = `wan-traffic-${history.range}.csv`;
  link.click();
  URL.revokeObjectURL(url);
}
