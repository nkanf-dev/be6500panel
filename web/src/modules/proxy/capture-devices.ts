import type { RouterSnapshot } from "../../lib/contracts";

export type CaptureDevice = RouterSnapshot["devices"][number];

export function captureMAC(mac: string): string {
  const value = mac.trim().toLowerCase();
  return /^(?:[0-9a-f]{2}:){5}[0-9a-f]{2}$/.test(value) ? value : "";
}

function ipv4Number(ip: string): number | undefined {
  const parts = ip.split(".");
  if (parts.length !== 4 || parts.some((part) => !/^\d{1,3}$/.test(part)))
    return undefined;
  const numbers = parts.map(Number);
  if (numbers.some((part) => part > 255)) return undefined;
  return numbers.reduce((value, part) => (value * 256 + part) >>> 0, 0);
}

function lanPrefixes(snapshot: RouterSnapshot) {
  return snapshot.routes.flatMap((route) => {
    if (
      route.family !== "ipv4" ||
      route.interface !== "br-lan" ||
      (route.gateway !== "" && route.gateway !== "0.0.0.0")
    )
      return [];
    const [address, prefix] = route.destination.split("/");
    const network = ipv4Number(address);
    const bits = Number(prefix);
    if (
      network === undefined ||
      !prefix ||
      !Number.isInteger(bits) ||
      bits < 1 ||
      bits > 32
    )
      return [];
    const mask = (0xffffffff << (32 - bits)) >>> 0;
    return [{ network: (network & mask) >>> 0, mask }];
  });
}

/** New servers decide eligibility. Legacy observations must prove a connected LAN prefix. */
export function captureDevices(snapshot: RouterSnapshot): CaptureDevice[] {
  const prefixes = lanPrefixes(snapshot);
  const now = Date.now();
  const rank = (device: CaptureDevice) => {
    const expiry = device.expiresAt ? Date.parse(device.expiresAt) : NaN;
    return [
      Number(device.eligible === true),
      Number(expiry > now),
      expiry > now ? expiry : 0,
      Number(device.online),
    ];
  };
  const devices = new Map<string, CaptureDevice>();
  for (const device of snapshot.devices) {
    const mac = captureMAC(device.mac);
    const ip = ipv4Number(device.ip);
    if (!mac || ip === undefined || device.eligible === false) continue;
    if (
      device.eligible !== true &&
      !prefixes.some(({ network, mask }) => (ip & mask) >>> 0 === network)
    )
      continue;
    const candidate = { ...device, mac };
    const previous = devices.get(mac);
    const nextRank = rank(candidate);
    const oldRank = previous ? rank(previous) : [];
    const firstDifference = nextRank.findIndex(
      (value, index) => value !== oldRank[index],
    );
    if (
      !previous ||
      (firstDifference >= 0 &&
        nextRank[firstDifference] > oldRank[firstDifference])
    )
      devices.set(mac, candidate);
  }
  return [...devices.values()];
}

export function captureEligibilityAvailable(snapshot: RouterSnapshot): boolean {
  return (
    snapshot.devices.some((device) => device.eligible !== undefined) ||
    lanPrefixes(snapshot).length > 0
  );
}
