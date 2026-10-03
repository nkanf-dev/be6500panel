import { describe, expect, it } from "vitest";
import type { RouterSnapshot } from "../../lib/contracts";
import type { ProxyMetrics } from "../proxy/telemetry-api";
import { routerSnapshot } from "../production-fixtures.test-data";
import {
  correlateDeviceProxy,
  mergeDeviceInventory,
  type ActivityDevice,
  type ConnectionRow,
  type DeviceHistory,
  type WorkspaceDevice,
} from "./device-model";

const time = "2026-10-03T00:00:00.000Z";
const now = Date.parse(time);
const mac = "AA:BB:CC:DD:EE:FF";
const otherMAC = "02:00:00:00:00:21";
const earlier = (seconds: number) =>
  new Date(now - seconds * 1000).toISOString();
const lease = (fields: Partial<RouterSnapshot["devices"][number]> = {}) => ({
  ...routerSnapshot.devices[0],
  mac,
  ip: "192.0.2.20",
  hostname: "system-client",
  ...fields,
});
const snapshot = (
  devices: RouterSnapshot["devices"],
  sampledAt = time,
): RouterSnapshot => ({ ...routerSnapshot, devices, sampledAt });
const activityDevice = (
  fields: Partial<ActivityDevice> = {},
): ActivityDevice => ({
  id: mac,
  name: "trafficd-client",
  addresses: ["192.0.2.20"],
  interface: "wlan0",
  associated: true,
  lastSeen: time,
  stale: false,
  rxBytes: 100,
  txBytes: 50,
  coverageSeconds: 30,
  counters: [],
  links: [],
  addressConflicts: [],
  samples: [
    { time: earlier(60), rxBytes: 100, txBytes: 50, coverageSeconds: 30 },
  ],
  ...fields,
});
const history = (
  devices: readonly ActivityDevice[],
  sampledAt = time,
): DeviceHistory => ({
  range: "24h",
  state: "ok",
  source: "trafficd",
  direction: "router-relative",
  resolutionSeconds: 30,
  sampledAt,
  deviceCount: devices.length,
  matchedCount: devices.length,
  truncated: false,
  devices,
});
const workspaceDevice = (
  fields: Partial<WorkspaceDevice> = {},
): WorkspaceDevice => ({
  mac,
  hostname: "system-client",
  addresses: ["192.0.2.20"],
  currentAddresses: ["192.0.2.20"],
  currentAddressSampledAt: time,
  leases: [],
  ...fields,
});
const connection = (sourceIP: string, id = sourceIP): ConnectionRow => ({
  id,
  startedAt: earlier(10),
  ageMs: 10_000,
  network: "tcp",
  sourceIP,
  sourcePort: 12345,
  destinationIP: "198.51.100.1",
  destinationPort: 443,
  host: "example.test",
  uploadBytes: 20,
  downloadBytes: 30,
  outbound: "proxy",
  ruleId: "rule-id",
  rule: "domain=example.test => route(proxy)",
});
const available = { available: true, reason: "current observed core data" };
const metrics = (fields: Partial<ProxyMetrics> = {}): ProxyMetrics => ({
  state: "ready",
  reason: "",
  source: "sing-box Clash API",
  sampledAt: time,
  capabilities: {
    connections: available,
    traffic: available,
    routing: available,
    latency: available,
    requestPhases: { available: false, reason: "HTTPS content is opaque" },
  },
  totals: { uploadBytes: 900_000, downloadBytes: 800_000 },
  activeConnections: 999,
  truncated: false,
  connections: [connection("192.0.2.20")],
  traffic: [
    { time: earlier(60), uploadRate: 1000, downloadRate: 2000, reset: false },
  ],
  probes: [],
  ...fields,
});

describe("MAC device inventory", () => {
  it("merges multiple IPv4/IPv6 observations by canonical MAC and keeps all leases", () => {
    const first = lease({ mac: "aa-bb-cc-dd-ee-ff" });
    const second = lease({
      mac: "aa:bb:cc:dd:ee:ff",
      ip: "2001:db8::20",
      hostname: "",
    });
    const duplicate = lease({ ip: first.ip });
    const activity = activityDevice({
      id: "aa:bb:cc:dd:ee:ff",
      addresses: ["2001:db8::20", "192.0.2.21"],
    });
    const merged = mergeDeviceInventory(
      snapshot([first, second, duplicate]),
      history([activity]),
    );
    expect(merged).toHaveLength(1);
    expect(merged[0]).toEqual({
      mac,
      hostname: "system-client",
      addresses: ["192.0.2.20", "2001:db8::20", "192.0.2.21"],
      currentAddresses: ["192.0.2.20", "2001:db8::20", "192.0.2.21"],
      currentAddressSampledAt: time,
      currentAddressObservedAt: {
        "192.0.2.20": time,
        "2001:db8::20": time,
        "192.0.2.21": time,
      },
      leases: [first, second, duplicate],
      activity,
    });
  });

  it("does not create editable identity from an IP-only or invalid MAC observation", () => {
    const merged = mergeDeviceInventory(
      snapshot([
        lease({ mac: "", ip: "192.0.2.30" }),
        lease({ mac: "192.0.2.31", ip: "192.0.2.31" }),
        lease({ mac: "not-a-mac" }),
      ]),
      history([
        activityDevice({ id: "2001:db8::20" }),
        activityDevice({ id: "192.0.2.32" }),
        activityDevice({ id: "" }),
      ]),
    );
    expect(merged).toEqual([]);
    expect(mergeDeviceInventory()).toEqual([]);
  });

  it("keeps distinct MAC owners distinct even when the observed IP is shared", () => {
    const merged = mergeDeviceInventory(
      snapshot([lease(), lease({ mac: otherMAC })]),
    );
    expect(merged.map((entry) => entry.mac)).toEqual([mac, otherMAC]);
    expect(correlateDeviceProxy(mac, merged, metrics(), now)).toMatchObject({
      connections: [],
      ambiguousCount: 1,
    });
  });

  it("retains historical addresses without treating offline, stale or unassociated rows as current", () => {
    const merged = mergeDeviceInventory(
      snapshot([
        lease(),
        lease({ ip: "192.0.2.19", online: false }),
        lease({ mac: otherMAC, ip: "192.0.2.21", online: false }),
      ]),
      history([
        activityDevice({ stale: true, addresses: ["192.0.2.18"] }),
        activityDevice({
          id: otherMAC,
          associated: false,
          addresses: ["192.0.2.22"],
        }),
      ]),
    );
    expect(merged[0].addresses).toEqual([
      "192.0.2.20",
      "192.0.2.19",
      "192.0.2.18",
    ]);
    expect(merged[0].currentAddresses).toEqual(["192.0.2.20"]);
    expect(merged[1].addresses).toEqual(["192.0.2.21", "192.0.2.22"]);
    expect(merged[1].currentAddresses).toEqual([]);
  });

  it("supports trafficd-only MAC identities but rejects their expired current addresses", () => {
    const merged = mergeDeviceInventory(
      undefined,
      history([activityDevice({ lastSeen: earlier(31) })], earlier(31)),
    );
    expect(merged[0]).toMatchObject({
      mac,
      hostname: "trafficd-client",
      leases: [],
    });
    expect(correlateDeviceProxy(mac, merged, metrics(), now)).toMatchObject({
      connections: [],
      unmatchedCount: 1,
      stale: false,
    });
  });

  it("does not refresh an absent device from another device's newer trafficd aggregate sample", () => {
    const absent = activityDevice({
      lastSeen: earlier(90),
      addresses: ["192.0.2.20"],
    });
    const present = activityDevice({
      id: otherMAC,
      lastSeen: time,
      addresses: ["192.0.2.21"],
    });
    const merged = mergeDeviceInventory(undefined, history([absent, present]));
    const absentRow = connection("192.0.2.20", "absent-device");
    const presentRow = connection("192.0.2.21", "present-device");
    const currentMetrics = metrics({ connections: [absentRow, presentRow] });
    expect(
      correlateDeviceProxy(mac, merged, currentMetrics, now),
    ).toMatchObject({
      connections: [],
      unmatchedCount: 1,
      stale: false,
    });
    expect(
      correlateDeviceProxy(otherMAC, merged, currentMetrics, now),
    ).toMatchObject({
      connections: [presentRow],
      unmatchedCount: 1,
    });
  });

  it("does not expire a fresh snapshot address when older trafficd data repeats that address", () => {
    const merged = mergeDeviceInventory(
      snapshot([lease()]),
      history([activityDevice({ lastSeen: earlier(60) })], earlier(60)),
    );
    expect(correlateDeviceProxy(mac, merged, metrics(), now)).toMatchObject({
      connections: [connection("192.0.2.20")],
      unmatchedCount: 0,
    });
  });

  it("does not refresh an expired snapshot address when fresh trafficd data has a different address", () => {
    const merged = mergeDeviceInventory(
      snapshot([lease({ ip: "192.0.2.19" })], earlier(60)),
      history([activityDevice({ addresses: ["192.0.2.20"] })]),
    );
    const oldRow = connection("192.0.2.19", "old-address");
    const currentRow = connection("192.0.2.20", "current-address");
    expect(merged[0].addresses).toEqual(["192.0.2.19", "192.0.2.20"]);
    expect(
      correlateDeviceProxy(
        mac,
        merged,
        metrics({ connections: [oldRow, currentRow] }),
        now,
      ),
    ).toMatchObject({ connections: [currentRow], unmatchedCount: 1 });
  });
});

describe("current device proxy correlation", () => {
  it("uses only current unambiguous ownership, not global active counts, totals or traffic history", () => {
    const current = connection("192.0.2.20", "selected");
    const other = connection("192.0.2.21", "other");
    const historic = connection("192.0.2.19", "historical-address");
    const unmatched = connection("192.0.2.99", "unknown");
    const ambiguous = connection("192.0.2.22", "shared");
    const devices = [
      workspaceDevice({
        addresses: ["192.0.2.20", "192.0.2.19", "192.0.2.22"],
        currentAddresses: ["192.0.2.20", "192.0.2.22"],
      }),
      workspaceDevice({
        mac: otherMAC,
        addresses: ["192.0.2.21", "192.0.2.22"],
        currentAddresses: ["192.0.2.21", "192.0.2.22"],
      }),
    ];
    const observation = correlateDeviceProxy(
      mac,
      devices,
      metrics({
        connections: [current, other, historic, unmatched, ambiguous],
        truncated: true,
      }),
      now,
    );
    expect(observation).toMatchObject({
      connections: [current],
      observedCount: 5,
      unmatchedCount: 2,
      ambiguousCount: 1,
      stale: false,
      sourceAgeSeconds: 0,
      sampledAt: time,
      source: "sing-box Clash API",
      truncated: true,
    });
    expect(observation).not.toHaveProperty("traffic");
    expect(observation).not.toHaveProperty("totals");
    expect(
      correlateDeviceProxy(mac, devices, metrics({ connections: [] }), now)
        .observedCount,
    ).toBe(0);
  });

  it("never attributes rows from historical addresses when there is no current address sample", () => {
    const device = workspaceDevice({
      currentAddresses: [],
      activity: activityDevice({ stale: true }),
    });
    expect(correlateDeviceProxy(mac, [device], metrics(), now)).toMatchObject({
      connections: [],
      unmatchedCount: 1,
      ambiguousCount: 0,
    });
  });

  it("rejects explicitly conflicted addresses rather than picking their apparent MAC owner", () => {
    const device = workspaceDevice({
      activity: activityDevice({ addressConflicts: ["192.0.2.20"] }),
    });
    expect(correlateDeviceProxy(mac, [device], metrics(), now)).toMatchObject({
      connections: [],
      ambiguousCount: 1,
      unmatchedCount: 0,
    });
  });

  it("deduplicates repeated current rows for the same MAC without making ownership ambiguous", () => {
    const devices = [workspaceDevice(), workspaceDevice()];
    expect(correlateDeviceProxy(mac, devices, metrics(), now)).toMatchObject({
      connections: [connection("192.0.2.20")],
      ambiguousCount: 0,
    });
  });

  it.each([-5, 0, 29.999, 30])(
    "accepts ready metrics sampled %s seconds ago",
    (age) => {
      const observation = correlateDeviceProxy(
        mac,
        [workspaceDevice()],
        metrics({ sampledAt: earlier(age) }),
        now,
      );
      expect(observation.stale).toBe(false);
      expect(observation.connections).toHaveLength(1);
      expect(observation.sourceAgeSeconds).toBeCloseTo(age);
    },
  );

  it.each([
    { sampledAt: earlier(30.001) },
    { sampledAt: earlier(-5.001) },
    { sampledAt: undefined },
    { sampledAt: "not-a-date" },
    { state: "stale" as const },
    { state: "unavailable" as const },
  ])("hides per-device rows from stale or unavailable metrics %j", (fields) => {
    const observation = correlateDeviceProxy(
      mac,
      [workspaceDevice()],
      metrics(fields),
      now,
    );
    expect(observation.stale).toBe(true);
    expect(observation.connections).toEqual([]);
  });

  it("hides previously observed rows after a metrics request fails", () => {
    expect(
      correlateDeviceProxy(mac, [workspaceDevice()], metrics(), now, true),
    ).toMatchObject({
      stale: true,
      connections: [],
    });
    expect(
      correlateDeviceProxy(mac, [workspaceDevice()], undefined, now),
    ).toMatchObject({
      stale: true,
      connections: [],
      observedCount: 0,
    });
  });

  it.each([undefined, "not-a-date", earlier(30.001), earlier(-6)])(
    "ignores current address ownership with an invalid or expired sample %s",
    (currentAddressSampledAt) => {
      const device = workspaceDevice({ currentAddressSampledAt });
      expect(correlateDeviceProxy(mac, [device], metrics(), now)).toMatchObject(
        {
          stale: false,
          connections: [],
          unmatchedCount: 1,
        },
      );
    },
  );

  it("uses per-address timestamps rather than refreshing all addresses from a shared timestamp", () => {
    const device = workspaceDevice({
      addresses: ["192.0.2.19", "192.0.2.20"],
      currentAddresses: ["192.0.2.19", "192.0.2.20"],
      currentAddressSampledAt: time,
      currentAddressObservedAt: {
        "192.0.2.19": earlier(31),
        "192.0.2.20": time,
      },
    });
    const fresh = connection("192.0.2.20", "fresh");
    const old = connection("192.0.2.19", "expired");
    expect(
      correlateDeviceProxy(
        mac,
        [device],
        metrics({ connections: [old, fresh] }),
        now,
      ),
    ).toMatchObject({ connections: [fresh], unmatchedCount: 1 });
  });

  it("does not fall back to a shared timestamp for addresses missing from the per-address map", () => {
    const device = workspaceDevice({ currentAddressObservedAt: {} });
    expect(correlateDeviceProxy(mac, [device], metrics(), now)).toMatchObject({
      connections: [],
      unmatchedCount: 1,
    });
  });

  it("accepts current address observations at the exact thirty-second boundary", () => {
    const device = workspaceDevice({ currentAddressSampledAt: earlier(30) });
    expect(
      correlateDeviceProxy(mac, [device], metrics(), now).connections,
    ).toHaveLength(1);
  });

  it("exposes unavailable capability reasons without returning attributed connections", () => {
    const sample = metrics();
    const observation = correlateDeviceProxy(
      mac,
      [workspaceDevice()],
      {
        ...sample,
        capabilities: {
          ...sample.capabilities,
          connections: {
            available: false,
            reason: "core connections endpoint unavailable",
          },
        },
      },
      now,
    );
    expect(observation).toMatchObject({
      connections: [],
      stale: false,
      reason: "core connections endpoint unavailable",
    });
  });
});
