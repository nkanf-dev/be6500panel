import type {
  DeviceActivityHistory,
  DeviceActivityRange,
} from "../../lib/device-activity-contracts";

export function activityFixture(
  range: DeviceActivityRange = "24h",
): DeviceActivityHistory {
  return {
    enabled: true,
    persistent: false,
    retentionDays: 7,
    source: "trafficd",
    direction: "vendor-rx-tx",
    range,
    resolutionSeconds: 3600,
    state: "ok",
    sampledAt: "2026-10-03T02:00:00Z",
    oldestAt: "2026-10-03T00:00:00Z",
    deviceCount: 1,
    matchedCount: 1,
    truncated: false,
    devices: [
      {
        id: "02:00:00:00:00:01",
        name: "测试终端",
        addresses: ["192.0.2.10"],
        interface: "wlan-test",
        associated: true,
        lastSeen: "2026-10-03T02:00:00Z",
        stale: false,
        rxBytes: 1024,
        txBytes: 2048,
        coverageSeconds: 3600,
        rxBytesPerSecond: 10,
        txBytesPerSecond: 20,
        samples: [
          {
            time: "2026-10-03T00:00:00Z",
            rxBytes: 1024,
            txBytes: 2048,
            coverageSeconds: 3600,
          },
          {
            time: "2026-10-03T01:00:00Z",
            rxBytes: null,
            txBytes: null,
            coverageSeconds: 0,
          },
          {
            time: "2026-10-03T02:00:00Z",
            rxBytes: 0,
            txBytes: 0,
            coverageSeconds: 30,
          },
        ],
      },
    ],
    groups: [
      {
        name: "wlan-test",
        deviceCount: 1,
        rxBytes: 1024,
        txBytes: 2048,
        coverageSeconds: 3600,
        rxBytesPerSecond: 10,
        txBytesPerSecond: 20,
      },
    ],
  };
}
