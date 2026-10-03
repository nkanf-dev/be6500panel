import { afterEach, describe, expect, it, vi } from "vitest";
import { Schema } from "effect";
import {
  loadDeviceActivity,
  normalizeDeviceActivitySearch,
} from "./device-activity-api";
import {
  DeviceActivityHistorySchema,
  DEVICE_ACTIVITY_RANGES,
} from "./device-activity-contracts";
import { activityFixture } from "../components/device-activity/activity-fixture.test-data";
const respond = (body: unknown) =>
  new Response(JSON.stringify(body), {
    headers: { "Content-Type": "application/json" },
  });
afterEach(() => vi.unstubAllGlobals());

describe("trafficd device activity HTTP contract", () => {
  it.each(DEVICE_ACTIVITY_RANGES)(
    "reads %s with server-side point and device bounds",
    async (range) => {
      const fetch = vi.fn().mockResolvedValue(respond(activityFixture(range)));
      vi.stubGlobal("fetch", fetch);
      await expect(loadDeviceActivity(range, "终端 + MAC")).resolves.toEqual(
        activityFixture(range),
      );
      const [url, options] = fetch.mock.calls[0];
      const parsed = new URL(url, "http://localhost");
      expect(parsed.pathname).toBe("/api/devices/activity");
      expect(Object.fromEntries(parsed.searchParams)).toEqual({
        range,
        maxPoints: "288",
        limit: "32",
        search: "终端 + MAC",
      });
      expect(options).toMatchObject({
        method: "GET",
        credentials: "same-origin",
        signal: expect.any(AbortSignal),
      });
      expect(options.body).toBeUndefined();
    },
  );
  it("bounds and encodes search without treating it as URL syntax", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue(respond(activityFixture())),
    );
    await loadDeviceActivity("24h", "  " + "x".repeat(80) + "&range=7d  ");
    const url = new URL(
      vi.mocked(fetch).mock.calls[0][0] as string,
      "http://localhost",
    );
    expect(url.searchParams.get("search")).toBe("x".repeat(64));
    expect(url.searchParams.get("range")).toBe("24h");
    expect(normalizeDeviceActivitySearch("😀".repeat(70))).toBe(
      "😀".repeat(64),
    );
  });
  it("keeps measured zero distinct from a missing bucket and accepts empty waiting data", () => {
    const fixture = activityFixture();
    const decoded = Schema.decodeUnknownSync(DeviceActivityHistorySchema)(
      fixture,
    );
    expect(decoded.devices[0].samples[1].rxBytes).toBeNull();
    expect(decoded.devices[0].samples[2].rxBytes).toBe(0);
    const empty = {
      ...fixture,
      enabled: false,
      state: "waiting",
      devices: [],
      groups: [],
      deviceCount: 0,
      matchedCount: 0,
    };
    expect(
      Schema.decodeUnknownSync(DeviceActivityHistorySchema)(empty),
    ).toEqual(empty);
  });
  it("rejects invalid units, timestamps, source claims and unbounded device/point arrays", () => {
    const valid = activityFixture();
    const device = valid.devices[0];
    const point = device.samples[0];
    for (const invalid of [
      { ...valid, persistent: true },
      { ...valid, source: "connections" },
      { ...valid, retentionDays: 365 },
      { ...valid, devices: [{ ...device, id: "not-a-MAC" }] },
      { ...valid, devices: [{ ...device, rxBytesPerSecond: -1 }] },
      {
        ...valid,
        devices: [
          { ...device, samples: [{ ...point, time: "00:00", rxBytes: -1 }] },
        ],
      },
      {
        ...valid,
        devices: [{ ...device, samples: [{ ...point, txBytes: Infinity }] }],
      },
      {
        ...valid,
        devices: [
          { ...device, samples: Array.from({ length: 289 }, () => point) },
        ],
      },
      { ...valid, devices: Array.from({ length: 65 }, () => device) },
    ])
      expect(() =>
        Schema.decodeUnknownSync(DeviceActivityHistorySchema)(invalid),
      ).toThrow();
  });
  it("accepts bounded workspace responses with 64 devices while the panel still queries 32", () => {
    const valid = activityFixture();
    const devices = Array.from({ length: 64 }, (_, index) => ({
      ...valid.devices[0],
      id: `02:00:00:00:01:${index.toString(16).padStart(2, "0")}`,
    }));
    expect(Schema.decodeUnknownSync(DeviceActivityHistorySchema)({ ...valid, devices }).devices).toHaveLength(64);
  });
  it("accepts all 169 hourly buckets for seven days including the current partial edge", () => {
    const valid = activityFixture("7d");
    const samples = Array.from({ length: 169 }, (_, index) => ({
      time: new Date(Date.UTC(2026, 8, 26, index)).toISOString(),
      rxBytes: index === 168 ? null : index,
      txBytes: index === 168 ? null : index * 2,
      coverageSeconds: index === 168 ? 0 : 3600,
    }));
    const history = { ...valid, devices: [{ ...valid.devices[0], samples }] };
    expect(
      Schema.decodeUnknownSync(DeviceActivityHistorySchema)(history).devices[0]
        .samples,
    ).toHaveLength(169);
  });
  it("preserves optional raw counters and vendor link labels without turning negotiation into throughput", () => {
    const valid = activityFixture();
    const device = {
      ...valid.devices[0],
      rawRXBytes: 123456,
      rawTXBytes: 789,
      onlineSeconds: 90,
      ageingSeconds: 0,
      counters: [{ address: "192.0.2.10", rxBytes: 123456, txBytes: 789 }],
      links: [
        {
          interface: "wlan-test",
          protocol: "802.11be",
          mld: true,
          signalDBM: -44,
          noiseDBM: -95,
          negotiatedRX: "2161+154Mbps",
          negotiatedTX: "2882Mbps",
          ageingSeconds: 0,
        },
      ],
      addressConflicts: ["192.0.2.11"],
    };
    const history = { ...valid, devices: [device] };
    expect(
      Schema.decodeUnknownSync(DeviceActivityHistorySchema)(history),
    ).toEqual(history);
    expect(() =>
      Schema.decodeUnknownSync(DeviceActivityHistorySchema)({
        ...history,
        devices: [
          {
            ...device,
            links: [{ interface: "wlan-test", signalDBM: Infinity }],
          },
        ],
      }),
    ).toThrow();
  });
  it("rejects a different range rather than relabelling retained data", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue(respond(activityFixture("7d"))),
    );
    await expect(loadDeviceActivity("24h")).rejects.toMatchObject({
      code: "invalid_response",
    });
  });
  it("cancels fetch when the caller abandons a filter", async () => {
    let signal: AbortSignal | undefined;
    vi.stubGlobal(
      "fetch",
      vi.fn((_url, init: RequestInit) => {
        signal = init.signal as AbortSignal;
        return new Promise((_resolve, reject) =>
          signal?.addEventListener("abort", () =>
            reject(new DOMException("aborted", "AbortError")),
          ),
        );
      }),
    );
    const controller = new AbortController();
    const pending = loadDeviceActivity("24h", "", controller.signal).catch(
      (error) => error,
    );
    controller.abort();
    expect(await pending).toBeDefined();
    expect(signal?.aborted).toBe(true);
  });
});
