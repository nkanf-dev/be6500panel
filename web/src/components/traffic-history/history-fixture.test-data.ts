import type {
  TrafficHistory,
  TrafficHistoryRange,
} from "../../lib/traffic-history-contracts";

export function historyFixture(
  range: TrafficHistoryRange = "30m",
): TrafficHistory {
  return {
    enabled: true,
    persistent: true,
    retentionDays: 400,
    source: "WAN · test-wan",
    range,
    resolutionSeconds: 30,
    oldestAt: "2026-10-02T17:00:00Z",
    samples: [
      {
        time: "2026-10-02T18:00:00Z",
        rx: 1_000_000,
        tx: 500_000,
        rxPeak: 2_000_000,
        txPeak: 750_000,
        rxBytes: 30_000_000,
        txBytes: 15_000_000,
        coverageSeconds: 30,
      },
      {
        time: "2026-10-02T18:00:30Z",
        rx: 0,
        tx: 0,
        rxPeak: 0,
        txPeak: 0,
        rxBytes: 0,
        txBytes: 0,
        coverageSeconds: 0,
      },
      {
        time: "2026-10-02T18:01:00Z",
        rx: 500_000,
        tx: 250_000,
        rxPeak: 750_000,
        txPeak: 500_000,
        rxBytes: 15_000_000,
        txBytes: 7_500_000,
        coverageSeconds: 30,
      },
    ],
    summary: { rxBytes: 45_000_000, txBytes: 22_500_000, coverageSeconds: 60 },
  };
}
