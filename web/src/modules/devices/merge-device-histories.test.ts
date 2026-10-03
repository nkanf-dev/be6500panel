import { describe, expect, it } from "vitest";
import type {
  ActivityDevice,
  DeviceHistory,
  DeviceRange,
} from "./device-model";
import { mergeDeviceHistories } from "./merge-device-histories";

const time = "2026-10-03T00:00:00.000Z";
const earlier = (seconds: number) =>
  new Date(Date.parse(time) - seconds * 1000).toISOString();
const mac = "AA:BB:CC:DD:EE:FF";
const otherMAC = "02:00:00:00:00:21";
const device = (fields: Partial<ActivityDevice> = {}): ActivityDevice => ({
  id: mac,
  name: "test-device",
  addresses: ["192.0.2.20"],
  interface: "wlan0",
  associated: true,
  lastSeen: time,
  stale: false,
  rxBytes: 100,
  txBytes: 50,
  coverageSeconds: 30,
  counters: [{ address: "192.0.2.20", rxBytes: 100, txBytes: 50 }],
  links: [{ interface: "wlan0", protocol: "802.11ax" }],
  addressConflicts: [],
  samples: [
    { time: earlier(30), rxBytes: 100, txBytes: 50, coverageSeconds: 30 },
  ],
  ...fields,
});
const history = (
  devices: readonly ActivityDevice[],
  fields: Partial<DeviceHistory> = {},
): DeviceHistory => ({
  range: "24h",
  state: "ok",
  source: "trafficd",
  direction: "router-relative",
  resolutionSeconds: 30,
  sampledAt: time,
  deviceCount: devices.length,
  matchedCount: devices.length,
  truncated: false,
  devices,
  ...fields,
});
const deepFreeze = <T>(value: T): T => {
  if (value && typeof value === "object") {
    for (const nested of Object.values(value)) deepFreeze(nested);
    Object.freeze(value);
  }
  return value;
};

describe("merge device list and exact-MAC histories", () => {
  it("returns no history when all inputs are absent", () => {
    expect(mergeDeviceHistories(undefined, [], "24h")).toBeUndefined();
    expect(
      mergeDeviceHistories(undefined, [undefined, undefined], "24h"),
    ).toBeUndefined();
  });

  it.each<DeviceRange>(["30m", "7d"])(
    "excludes a mismatched %s base and details",
    (range) => {
      const wrong = history([device()], { range });
      expect(
        mergeDeviceHistories(wrong, [undefined, wrong], "24h"),
      ).toBeUndefined();
    },
  );

  it("uses matching detail data when base is absent or from another range", () => {
    const selected = device();
    const detail = history([selected]);
    const wrongBase = history([device({ id: otherMAC })], { range: "7d" });
    for (const base of [undefined, wrongBase]) {
      expect(mergeDeviceHistories(base, [undefined, detail], "24h")).toEqual(
        detail,
      );
    }
  });

  it("ignores absent and range-mismatched details rather than replacing matching rows", () => {
    const baseDevice = device({ name: "selected range" });
    const base = history([baseDevice]);
    const wrong = history(
      [
        device({ name: "other range", lastSeen: earlier(-60) }),
        device({ id: otherMAC }),
      ],
      {
        range: "30m",
        sampledAt: earlier(-60),
        deviceCount: 900,
      },
    );
    expect(mergeDeviceHistories(base, [undefined, wrong], "24h")).toEqual(base);
  });

  it("adds exact-MAC detail omitted by the capped list without replacing list coverage metadata", () => {
    const listed = device({ id: otherMAC });
    const selected = device({ name: "omitted from capped list" });
    const base = history([listed], {
      truncated: true,
      deviceCount: 257,
      matchedCount: 1,
      sampledAt: earlier(10),
    });
    const detail = history([selected], {
      deviceCount: 257,
      matchedCount: 1,
      source: "exact MAC query",
      sampledAt: time,
    });
    expect(mergeDeviceHistories(base, [detail], "24h")).toEqual({
      ...base,
      devices: [listed, selected],
      sampledAt: time,
      matchedCount: 2,
    });
  });

  it.each([false, true])(
    "deduplicates canonical MACs using device lastSeen, not aggregate sampledAt (reverse=%s)",
    (reverse) => {
      const older = device({
        id: "aa-bb-cc-dd-ee-ff",
        lastSeen: earlier(60),
        name: "older device observation",
      });
      const newer = device({
        id: "aa:bb:cc:dd:ee:ff",
        lastSeen: earlier(5),
        name: "newer device observation",
      });
      const oldDetail = history([older], { sampledAt: time });
      const newDetail = history([newer], { sampledAt: earlier(5) });
      const details = reverse ? [newDetail, oldDetail] : [oldDetail, newDetail];
      const result = mergeDeviceHistories(history([]), details, "24h");
      expect(result?.devices).toEqual([newer]);
      expect(result?.sampledAt).toBe(time);
      expect(result?.matchedCount).toBe(1);
    },
  );

  it("does not let an older detail replace a newer base row even when its aggregate sample is newer", () => {
    const baseDevice = device({
      name: "newest device observation",
      lastSeen: earlier(5),
    });
    const detailDevice = device({
      id: "aa-bb-cc-dd-ee-ff",
      name: "stale detail observation",
      lastSeen: earlier(60),
    });
    const base = history([baseDevice], { sampledAt: earlier(10) });
    const detail = history([detailDevice], { sampledAt: time });
    expect(mergeDeviceHistories(base, [detail], "24h")).toMatchObject({
      devices: [baseDevice],
      sampledAt: time,
      matchedCount: 1,
    });
  });

  it("deduplicates the first matching input using latest lastSeen regardless of row order", () => {
    const newest = device({ name: "newest row", lastSeen: time });
    const older = device({
      id: "aa-bb-cc-dd-ee-ff",
      name: "older duplicate",
      lastSeen: earlier(60),
    });
    for (const rows of [
      [newest, older],
      [older, newest],
    ]) {
      const result = mergeDeviceHistories(history(rows), [], "24h");
      expect(result?.devices).toEqual([newest]);
    }
  });

  it("does not mutate either input, device rows or nested observation collections", () => {
    const base = deepFreeze(
      history([device({ lastSeen: earlier(60) })], {
        truncated: true,
        sampledAt: earlier(60),
      }),
    );
    const detail = deepFreeze(
      history([
        device({ id: "aa-bb-cc-dd-ee-ff", name: "updated" }),
        device({ id: otherMAC }),
      ]),
    );
    const baseBefore = structuredClone(base);
    const detailBefore = structuredClone(detail);
    const details = Object.freeze([undefined, detail]);
    const result = mergeDeviceHistories(base, details, "24h");
    expect(result).not.toBe(base);
    expect(result?.devices).toEqual(detail.devices);
    expect(result?.devices).not.toBe(base.devices);
    expect(result?.devices).not.toBe(detail.devices);
    expect(base).toEqual(baseBefore);
    expect(detail).toEqual(detailBefore);
    expect(details).toEqual([undefined, detail]);
  });
});
