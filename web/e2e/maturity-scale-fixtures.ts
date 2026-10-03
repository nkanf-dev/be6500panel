import type { Page } from "@playwright/test";
import { Schema } from "effect";
import {
  ProxyNodesSchema,
  RouterSchema,
  RuntimeConfigSchema,
} from "../src/lib/contracts";
import {
  DEVICE_ACTIVITY_MAX_POINTS,
  DEVICE_ACTIVITY_MAX_QUERY_DEVICES,
  DeviceActivityHistorySchema,
} from "../src/lib/device-activity-contracts";
import { nodeConfigInputs } from "../src/modules/proxy/node-selector-config";
import { createMaturityFixture } from "./maturity-fixtures";

/** All identities, endpoints, counters and times are generated; no router data or credentials. */
export const SCALE_NODE_COUNT = 220;
export const SCALE_DEVICE_COUNT = 128;
export const scaleDeviceMAC = (number: number) =>
  `02:00:00:00:01:${number.toString(16).padStart(2, "0").toUpperCase()}`;
export const scaleDeviceName = (number: number) =>
  `scale-trafficd-only-${String(number).padStart(3, "0")}`;
export const scaleNodeID = (number: number) =>
  `scale-node-${String(number).padStart(3, "0")}`;
export const scaleNodeLabel = (number: number) =>
  `${["香港", "日本", "美国", "新加坡"][(number - 1) % 4]} scale-${String(number).padStart(3, "0")}`;
const checked = <A, I>(schema: Schema.Schema<A, I>, value: unknown): A =>
  Schema.decodeUnknownSync(schema)(value);
const iso = (time: number) => new Date(time).toISOString();

export function createScaleFixture(now = Date.now()) {
  const ordinary = createMaturityFixture(now);
  const bucketEnd = Math.floor(now / 120_000) * 120_000;
  const nodes = checked(ProxyNodesSchema, {
    nodes: Array.from({ length: SCALE_NODE_COUNT }, (_, index) => ({
      id: scaleNodeID(index + 1),
      label: scaleNodeLabel(index + 1),
      server: `scale-node-${index + 1}.example.test`,
      port: 443,
      protocol: "vless",
      transport: "tcp",
      reality: false,
      vision: false,
      utls: false,
      udp: true,
    })),
    diagnostics: [],
    selectedNodeId: scaleNodeID(1),
  });
  // Only supported compiler inputs are needed. The public node API carries no UUID/token.
  const nativeConfig = JSON.stringify({
    inbounds: [
      {
        tag: "mixed-in",
        type: "mixed",
        listen: "192.168.31.1",
        listen_port: 2080,
      },
      {
        tag: "tproxy-in",
        type: "tproxy",
        listen: "127.0.0.1",
        listen_port: 7893,
      },
      {
        tag: "dns-in",
        type: "direct",
        listen: "192.168.31.1",
        listen_port: 6450,
      },
    ],
    route: { rules: [{ ip_version: 6, outbound: "direct" }] },
  });
  const allDevices = Array.from({ length: SCALE_DEVICE_COUNT }, (_, index) => {
    const number = index + 1;
    return {
      id: scaleDeviceMAC(number),
      name: scaleDeviceName(number),
      addresses: [`192.0.2.${number}`],
      interface: "scale-wlan",
      associated: number <= 64,
      lastSeen: iso(number <= 64 ? now : now - 3_600_000),
      stale: number > 64,
      rxBytes: number * 120 * (DEVICE_ACTIVITY_MAX_POINTS - 1),
      txBytes: number * 60 * (DEVICE_ACTIVITY_MAX_POINTS - 1),
      coverageSeconds: 120 * (DEVICE_ACTIVITY_MAX_POINTS - 1),
      rxBytesPerSecond: number,
      txBytesPerSecond: number / 2,
      rawRXBytes: number * 1_000_000,
      rawTXBytes: number * 500_000,
      samples: Array.from(
        { length: DEVICE_ACTIVITY_MAX_POINTS },
        (_, bucket) => ({
          time: iso(
            bucketEnd - (DEVICE_ACTIVITY_MAX_POINTS - 1 - bucket) * 120_000,
          ),
          rxBytes: bucket === 0 ? null : number * 120,
          txBytes: bucket === 0 ? null : number * 60,
          coverageSeconds: bucket === 0 ? 0 : 120,
        }),
      ),
    };
  });
  const activity = (url: URL) => {
    const search = (url.searchParams.get("search") ?? "").trim().toLowerCase();
    const limit = Number(url.searchParams.get("limit") ?? 32);
    const maxPoints = Number(url.searchParams.get("maxPoints") ?? 288);
    if (
      !Number.isInteger(limit) ||
      limit < 1 ||
      limit > DEVICE_ACTIVITY_MAX_QUERY_DEVICES
    )
      throw new Error(`Unbounded scale device limit: ${limit}`);
    if (
      !Number.isInteger(maxPoints) ||
      maxPoints < 1 ||
      maxPoints > DEVICE_ACTIVITY_MAX_POINTS
    )
      throw new Error(`Unbounded scale history points: ${maxPoints}`);
    if (Array.from(search).length > 64)
      throw new Error("Unbounded scale search");
    const matched = allDevices.filter((device) =>
      `${device.id} ${device.name} ${device.addresses.join(" ")}`
        .toLowerCase()
        .includes(search),
    );
    const devices = matched.slice(0, limit).map((device) => ({
      ...device,
      samples: device.samples.slice(-maxPoints),
    }));
    return checked(DeviceActivityHistorySchema, {
      enabled: true,
      persistent: false,
      retentionDays: 7,
      source: "trafficd",
      direction: "vendor-rx-tx",
      range: url.searchParams.get("range") ?? "24h",
      resolutionSeconds: 120,
      state: "ok",
      sampledAt: iso(now),
      oldestAt: allDevices[0].samples[0].time,
      deviceCount: SCALE_DEVICE_COUNT,
      matchedCount: matched.length,
      truncated: matched.length > devices.length,
      devices,
      groups: matched.length
        ? [
            {
              name: "scale-wlan",
              deviceCount: matched.length,
              rxBytes: matched.reduce((sum, device) => sum + device.rxBytes, 0),
              txBytes: matched.reduce((sum, device) => sum + device.txBytes, 0),
              coverageSeconds: 120 * (DEVICE_ACTIVITY_MAX_POINTS - 1),
            },
          ]
        : [],
    });
  };
  const get = (url: URL): unknown => {
    if (url.pathname === "/api/proxy/nodes") return nodes;
    if (url.pathname === "/api/devices/activity") return activity(url);
    if (url.pathname === "/api/router") {
      const base = ordinary.get(url) as typeof RouterSchema.Type;
      return checked(RouterSchema, {
        ...base,
        currentClientIP: "192.0.2.1",
        // Older trafficd identities 65..128 must be found by backend search, not DHCP shortcuts.
        devices: allDevices.slice(0, 4).map((device) => ({
          ip: device.addresses[0],
          mac: device.id,
          hostname: device.name,
          expiresAt: null,
          online: true,
          eligible: true,
        })),
        dns: { resolvers: ["192.0.2.53"], leaseCount: 4 },
      });
    }
    if (
      url.pathname === "/api/runtime/config" &&
      url.searchParams.get("service") === "sing-box"
    )
      return checked(RuntimeConfigSchema, {
        service: "sing-box",
        config: nativeConfig,
        generation: 7,
      });
    return ordinary.get(url);
  };
  return { get, nodes, allDevices, nativeConfig };
}

export async function installScaleFixture(page: Page, baseURL: string) {
  const origin = new URL(baseURL).origin;
  const source = createScaleFixture();
  const reads: string[] = [];
  const writes: string[] = [];
  const unexpected: string[] = [];
  const activityResponses: {
    query: string;
    deviceCount: number;
    matchedCount: number;
    returned: number;
    points: number;
  }[] = [];
  const browserUBus: string[] = [];
  page.on("request", (request) => {
    if (/\bubus\b/i.test(new URL(request.url()).pathname))
      browserUBus.push(request.url());
  });
  await page.route(
    (url) =>
      (url.origin === origin && url.pathname.startsWith("/api/")) ||
      /\bubus\b/i.test(url.pathname),
    async (route) => {
      const request = route.request();
      const url = new URL(request.url());
      try {
        if (/\bubus\b/i.test(url.pathname))
          throw new Error("Browser ubus is forbidden");
        if (request.method() !== "GET") {
          writes.push(`${request.method()} ${url.pathname}`);
          throw new Error(
            `Scale fixture refuses mutation: ${request.method()} ${url.pathname}`,
          );
        }
        reads.push(`${url.pathname}${url.search}`);
        const value = source.get(url);
        if (url.pathname === "/api/devices/activity") {
          const history = checked(DeviceActivityHistorySchema, value);
          activityResponses.push({
            query: url.search,
            deviceCount: history.deviceCount,
            matchedCount: history.matchedCount,
            returned: history.devices.length,
            points: Math.max(
              0,
              ...history.devices.map((device) => device.samples.length),
            ),
          });
        }
        await route.fulfill({
          status: 200,
          contentType:
            url.pathname === "/api/events"
              ? "text/event-stream"
              : "application/json",
          body:
            url.pathname === "/api/events"
              ? `event: snapshot\ndata: ${JSON.stringify(value)}\n\n`
              : JSON.stringify(value),
        });
      } catch (error) {
        unexpected.push(error instanceof Error ? error.message : String(error));
        await route.fulfill({
          status: 409,
          contentType: "application/json",
          body: JSON.stringify({
            error: {
              code: "scale_fixture_unexpected",
              message: "Synthetic scale fixture refused this request",
            },
          }),
        });
      }
    },
  );
  return {
    ...source,
    reads,
    writes,
    unexpected,
    activityResponses,
    browserUBus,
  };
}

/** Native Bun check. It does not import the spec, launch a browser, or contact a router. */
export function validateScaleFixtures() {
  const source = createScaleFixture(1_800_000_000_000);
  const query = (suffix: string) => {
    const url = new URL(
      "/api/devices/activity?range=24h&maxPoints=288",
      "http://fixture.example.test",
    );
    for (const [key, value] of new URLSearchParams(suffix))
      url.searchParams.set(key, value);
    return checked(DeviceActivityHistorySchema, source.get(url));
  };
  const base = query("limit=64&search=");
  const name = query(`limit=64&search=${scaleDeviceName(128)}`);
  const exact = query(
    `limit=1&search=${encodeURIComponent(scaleDeviceMAC(128))}`,
  );
  if (
    source.nodes.nodes.length !== 220 ||
    source.nodes.selectedNodeId !== scaleNodeID(1)
  )
    throw new Error(
      "Scale source must contain exactly 220 nodes and an accepted current node",
    );
  if (
    source.allDevices.length !== 128 ||
    base.deviceCount !== 128 ||
    base.matchedCount !== 128 ||
    base.devices.length !== 64 ||
    !base.truncated
  )
    throw new Error(
      "Scale device source must retain128, match128 and return bounded64",
    );
  for (const result of [name, exact])
    if (
      result.deviceCount !== 128 ||
      result.matchedCount !== 1 ||
      result.devices[0]?.id !== scaleDeviceMAC(128)
    )
      throw new Error("Backend search must find the old device128 beyond64");
  if (
    base.devices.some(
      (device) =>
        device.samples.length !== 288 ||
        device.samples[0].rxBytes !== null ||
        device.samples[0].txBytes !== null ||
        device.samples[0].coverageSeconds !== 0,
    )
  )
    throw new Error(
      "288-point history must retain null baseline, never a measured zero",
    );
  const router = checked(
    RouterSchema,
    source.get(new URL("/api/router", "http://fixture.example.test")),
  );
  if (
    router.devices.length !== 4 ||
    router.devices.some((device) => device.mac === scaleDeviceMAC(128))
  )
    throw new Error(
      "Device128 must not be reachable through a DHCP inventory shortcut",
    );
  nodeConfigInputs(source.nativeConfig);
  if (
    Schema.is(DeviceActivityHistorySchema)({
      ...base,
      devices: source.allDevices.slice(0, 65),
    })
  )
    throw new Error(
      "The browser decoder must reject65-device responses, not accept the full128 source",
    );
  const oversized = {
    ...base.devices[0],
    samples: [...base.devices[0].samples, base.devices[0].samples[0]],
  };
  if (Schema.is(DeviceActivityHistorySchema)({ ...base, devices: [oversized] }))
    throw new Error("The browser decoder must reject289-point history");
  const rejected = (suffix: string) => {
    try {
      query(suffix);
      return false;
    } catch {
      return true;
    }
  };
  if (
    !rejected("limit=65") ||
    !rejected("limit=64&maxPoints=289") ||
    !rejected(`limit=64&search=${"x".repeat(65)}`)
  )
    throw new Error("Scale fixture must reject unbounded queries");
}
