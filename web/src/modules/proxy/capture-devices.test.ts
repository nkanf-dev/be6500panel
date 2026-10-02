import { describe, expect, it } from "vitest";
import { routerSnapshot } from "../production-fixtures.test-data";
import type { RouterSnapshot } from "../../lib/contracts";
import {
  captureDevices,
  captureEligibilityAvailable,
  captureMAC,
} from "./capture-devices";

const sample: RouterSnapshot = {
  ...routerSnapshot,
  routes: [
    {
      family: "ipv4",
      destination: "192.0.2.0/24",
      gateway: "",
      interface: "br-lan",
      metric: 0,
    },
  ],
};
const device = sample.devices[0];

describe("capture device eligibility", () => {
  it("uses only connected br-lan subnets on legacy snapshots, not WAN or default routes", () => {
    const devices = captureDevices({
      ...sample,
      devices: [
        device,
        {
          ...device,
          mac: "02:00:00:00:00:30",
          ip: "192.168.1.3",
          hostname: "upstream",
        },
      ],
      routes: [
        ...sample.routes,
        ...routerSnapshot.routes,
        {
          ...sample.routes[0],
          destination: "192.168.1.0/24",
          interface: "wan",
        },
      ],
    });
    expect(devices).toEqual([device]);
  });
  it("provides no broad fallback when LAN route observations are unavailable", () => {
    expect(captureDevices(routerSnapshot)).toEqual([]);
    expect(captureEligibilityAvailable(routerSnapshot)).toBe(false);
    expect(
      captureDevices({
        ...sample,
        routes: [{ ...sample.routes[0], destination: "0.0.0.0/0" }],
      }),
    ).toEqual([]);
    expect(
      captureDevices({
        ...sample,
        routes: [{ ...sample.routes[0], gateway: "192.0.2.1" }],
      }),
    ).toEqual([]);
  });
  it("honors authoritative eligibility and rejects explicitly ineligible stale/WAN devices", () => {
    const eligible = { ...device, eligible: true };
    expect(
      captureDevices({
        ...routerSnapshot,
        devices: [
          eligible,
          { ...device, eligible: false, mac: "02:00:00:00:00:30" },
        ],
      }),
    ).toEqual([eligible]);
    expect(
      captureEligibilityAvailable({
        ...routerSnapshot,
        devices: [{ ...device, eligible: false }],
      }),
    ).toBe(true);
  });
  it("deduplicates canonical MACs with a current lease ahead of stale ARP entries", () => {
    const leased = {
      ...device,
      mac: device.mac.toUpperCase(),
      ip: "192.0.2.50",
      online: false,
      expiresAt: new Date(Date.now() + 3600_000).toISOString(),
    };
    expect(
      captureDevices({
        ...sample,
        devices: [
          device,
          leased,
          {
            ...device,
            ip: "192.0.2.40",
            expiresAt: new Date(Date.now() - 3600_000).toISOString(),
          },
        ],
      }),
    ).toEqual([{ ...leased, mac: device.mac }]);
  });
  it("selects the latest current lease and online observation when lease data is equal", () => {
    const expiry = Date.now() + 3600_000;
    const recent = {
      ...device,
      ip: "192.0.2.50",
      expiresAt: new Date(expiry + 1000).toISOString(),
    };
    expect(
      captureDevices({
        ...sample,
        devices: [
          { ...device, expiresAt: new Date(expiry).toISOString() },
          recent,
        ],
      }),
    ).toEqual([recent]);
    expect(
      captureDevices({
        ...sample,
        devices: [{ ...device, online: false }, device],
      }),
    ).toEqual([device]);
  });
  it("keeps authoritative eligibility ahead of a duplicate legacy row with a later lease", () => {
    const eligible = { ...device, ip: "192.0.2.50", eligible: true };
    expect(
      captureDevices({
        ...sample,
        devices: [
          eligible,
          {
            ...device,
            expiresAt: new Date(Date.now() + 3600_000).toISOString(),
          },
        ],
      }),
    ).toEqual([eligible]);
  });
  it("rejects invalid MACs and IPv4 addresses, without treating IPv6 as IPv4", () => {
    expect(captureMAC(" AA:BB:CC:DD:EE:FF ")).toBe("aa:bb:cc:dd:ee:ff");
    expect(captureMAC("not-a-mac")).toBe("");
    expect(
      captureDevices({
        ...sample,
        devices: [
          { ...device, ip: "192.0.2.999", eligible: true },
          { ...device, ip: "2001:db8::20", eligible: true },
          { ...device, mac: "", eligible: true },
        ],
      }),
    ).toEqual([]);
  });
});
