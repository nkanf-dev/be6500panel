import type { RouterSnapshot } from "../../lib/contracts";
import type { ProxyMetrics } from "../proxy/telemetry-api";
import { canonicalMAC } from "./device-labels";

// Structural subset of trafficd activity history. Root can pass DeviceActivityHistory directly.
function latestObservation(
  previous: Readonly<Record<string, string>> = {},
  addresses: readonly string[],
  sampledAt?: string,
): Readonly<Record<string, string>> {
  const next = { ...previous };
  if (!sampledAt || !Number.isFinite(Date.parse(sampledAt))) return next;
  for (const address of addresses)
    if (!next[address] || Date.parse(sampledAt) > Date.parse(next[address]))
      next[address] = sampledAt;
  return next;
}
export type DeviceRange = "30m" | "24h" | "7d";
export interface DeviceActivitySample {
  time: string;
  rxBytes: number | null;
  txBytes: number | null;
  coverageSeconds: number;
}
export interface DeviceActivityLink {
  interface: string;
  protocol?: string;
  mld?: boolean;
  signalDBM?: number;
  noiseDBM?: number;
  negotiatedRX?: string;
  negotiatedTX?: string;
  ageingSeconds?: number;
}
export interface ActivityDevice {
  id: string;
  name: string;
  addresses: readonly string[];
  interface: string;
  associated: boolean;
  lastSeen: string;
  stale: boolean;
  rxBytes: number;
  txBytes: number;
  coverageSeconds: number;
  rxBytesPerSecond?: number;
  txBytesPerSecond?: number;
  rawRXBytes?: number;
  rawTXBytes?: number;
  onlineSeconds?: number;
  ageingSeconds?: number;
  counters?: readonly { address: string; rxBytes: number; txBytes: number }[];
  links?: readonly DeviceActivityLink[];
  addressConflicts?: readonly string[];
  samples: readonly DeviceActivitySample[];
  vendor?: string;
}
export interface DeviceHistory {
  range: DeviceRange;
  state: "waiting" | "ok" | "stale" | "unavailable";
  source: string;
  direction: string;
  resolutionSeconds: number;
  sampledAt?: string;
  oldestAt?: string;
  error?: string;
  deviceCount: number;
  matchedCount: number;
  truncated: boolean;
  devices: readonly ActivityDevice[];
}
export interface WorkspaceDevice {
  mac: string;
  hostname: string;
  addresses: readonly string[];
  currentAddresses: readonly string[];
  currentAddressSampledAt?: string;
  currentAddressObservedAt?: Readonly<Record<string, string>>;
  leases: readonly RouterSnapshot["devices"][number][];
  activity?: ActivityDevice;
}
/** Never infer identity from IP alone. DHCP/ARP rows without a MAC are not editable devices. */
export function mergeDeviceInventory(
  snapshot?: RouterSnapshot,
  activity?: DeviceHistory,
): WorkspaceDevice[] {
  const map = new Map<string, WorkspaceDevice>();
  for (const entry of snapshot?.devices ?? []) {
    const mac = canonicalMAC(entry.mac);
    if (!mac) continue;
    const previous = map.get(mac);
    map.set(mac, {
      mac,
      hostname: previous?.hostname || entry.hostname,
      addresses: [
        ...new Set([
          ...(previous?.addresses ?? []),
          ...(entry.ip ? [entry.ip] : []),
        ]),
      ],
      currentAddresses: [
        ...new Set([
          ...(previous?.currentAddresses ?? []),
          ...(entry.online && entry.ip ? [entry.ip] : []),
        ]),
      ],
      currentAddressSampledAt: snapshot?.sampledAt,
      currentAddressObservedAt: latestObservation(
        previous?.currentAddressObservedAt,
        entry.online && entry.ip ? [entry.ip] : [],
        snapshot?.sampledAt,
      ),
      leases: [...(previous?.leases ?? []), entry],
    });
  }
  for (const entry of activity?.devices ?? []) {
    const mac = canonicalMAC(entry.id);
    if (!mac) continue;
    const previous = map.get(mac);
    map.set(mac, {
      mac,
      hostname: previous?.hostname || entry.name,
      addresses: [
        ...new Set([...(previous?.addresses ?? []), ...entry.addresses]),
      ],
      currentAddresses: [
        ...new Set([
          ...(previous?.currentAddresses ?? []),
          ...(!entry.stale && entry.associated ? entry.addresses : []),
        ]),
      ],
      currentAddressSampledAt:
        !entry.stale && entry.associated
          ? entry.lastSeen
          : previous?.currentAddressSampledAt,
      currentAddressObservedAt: latestObservation(
        previous?.currentAddressObservedAt,
        !entry.stale && entry.associated ? entry.addresses : [],
        entry.lastSeen,
      ),
      leases: previous?.leases ?? [],
      activity: entry,
    });
  }
  return [...map.values()];
}
export type ConnectionRow = ProxyMetrics["connections"][number];
export interface DeviceProxyObservation {
  connections: readonly ConnectionRow[];
  observedCount: number;
  unmatchedCount: number;
  ambiguousCount: number;
  stale: boolean;
  sampledAt?: string;
  sourceAgeSeconds?: number;
  source: string;
  reason: string;
  truncated: boolean;
}
/** Current addresses can correlate current core rows only. Historical flows are not re-attributed. */
export function correlateDeviceProxy(
  mac: string,
  devices: readonly WorkspaceDevice[],
  metrics?: ProxyMetrics,
  now = Date.now(),
  failed = false,
): DeviceProxyObservation {
  const sampledAt = metrics?.sampledAt;
  const age = sampledAt ? (now - Date.parse(sampledAt)) / 1000 : undefined;
  const stale =
    failed ||
    !metrics ||
    metrics.state !== "ready" ||
    age === undefined ||
    !Number.isFinite(age) ||
    age < -5 ||
    age > 30;
  const addresses = new Map<string, Set<string>>();
  for (const device of devices)
    for (const address of device.currentAddresses) {
      const addressTime = device.currentAddressObservedAt
        ? device.currentAddressObservedAt[address]
        : device.currentAddressSampledAt;
      const addressAge = addressTime
        ? (now - Date.parse(addressTime)) / 1000
        : undefined;
      if (
        addressAge === undefined ||
        !Number.isFinite(addressAge) ||
        addressAge < -5 ||
        addressAge > 30
      )
        continue;
      const owners = addresses.get(address) ?? new Set<string>();
      owners.add(device.mac);
      addresses.set(address, owners);
    }
  for (const device of devices)
    for (const address of device.activity?.addressConflicts ?? []) {
      const owners = addresses.get(address) ?? new Set<string>();
      owners.add("conflict");
      addresses.set(address, owners);
    }
  const connections: ConnectionRow[] = [];
  let unmatchedCount = 0,
    ambiguousCount = 0;
  for (const connection of metrics?.connections ?? []) {
    const owners = addresses.get(connection.sourceIP);
    if (!owners?.size) unmatchedCount++;
    else if (owners.size !== 1) ambiguousCount++;
    else if (owners.has(mac)) connections.push(connection);
  }
  return {
    connections:
      stale || !metrics?.capabilities.connections.available ? [] : connections,
    observedCount: metrics?.connections.length ?? 0,
    unmatchedCount,
    ambiguousCount,
    stale,
    sampledAt,
    sourceAgeSeconds: age,
    source: metrics?.source ?? "代理控制器",
    reason:
      metrics?.capabilities.connections.reason ||
      metrics?.reason ||
      "等待代理连接采样",
    truncated: metrics?.truncated ?? false,
  };
}
