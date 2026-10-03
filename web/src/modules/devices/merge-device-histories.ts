import { canonicalMAC } from "./device-labels";
import type { DeviceHistory, DeviceRange } from "./device-model";

/** Merge exact-MAC detail queries into the range list. Never mix time ranges. */
export function mergeDeviceHistories(
  base: DeviceHistory | undefined,
  details: readonly (DeviceHistory | undefined)[],
  range: DeviceRange,
): DeviceHistory | undefined {
  const inputs = [base, ...details].filter(
    (value): value is DeviceHistory => !!value && value.range === range,
  );
  const first = inputs[0];
  if (!first) return undefined;
  const devices = new Map<string, DeviceHistory["devices"][number]>();
  for (const input of inputs)
    for (const device of input.devices) {
      const key = canonicalMAC(device.id) ?? device.id;
      const previous = devices.get(key);
      if (
        !previous ||
        Date.parse(device.lastSeen) >= Date.parse(previous.lastSeen)
      )
        devices.set(key, device);
    }
  return {
    ...first,
    devices: [...devices.values()],
    sampledAt: inputs
      .map((input) => input.sampledAt)
      .filter((time): time is string => !!time)
      .sort()
      .at(-1),
    deviceCount: Math.max(...inputs.map((input) => input.deviceCount)),
    matchedCount: Math.max(first.matchedCount, devices.size),
  };
}
