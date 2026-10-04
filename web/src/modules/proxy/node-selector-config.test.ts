import { describe, expect, it } from "vitest";
import { nodeConfigInputs } from "./node-selector-config";
import { validRoutedTUN } from "./node-selector-preferences";

// Exact CompileNative / proxySelect listener shape. Private fields must not escape.
type NativeConfig = {
  inbounds: Record<string, unknown>[];
  route: { rules: Record<string, unknown>[] };
  outbounds: Record<string, unknown>[];
};
function nativeConfig(): NativeConfig {
  return {
    inbounds: [
      {
        type: "mixed",
        tag: "mixed-in",
        listen: "192.168.31.1",
        listen_port: 3200,
      },
      {
        type: "tun",
        tag: "tun-in",
        interface_name: "b6p-pilot_1",
        address: ["10.203.4.1/30"],
        mtu: 1500,
        dns_mode: "disabled",
        auto_route: false,
        auto_redirect: false,
        stack: "system",
        udp_timeout: "2m",
        udp_nat_max: 1024,
      },
      {
        type: "direct",
        tag: "dns-in",
        listen: "192.168.31.1",
        listen_port: 3202,
      },
    ],
    route: {
      rules: [
        { inbound: ["dns-in"], action: "hijack-dns" },
        { ip_version: 6, outbound: "direct" },
        {
          inbound: ["mixed-in", "tun-in"],
          action: "sniff",
          sniffer: ["http", "tls", "dns", "quic"],
          timeout: "300ms",
        },
      ],
    },
    outbounds: [
      { type: "vless", server: "private.example.test", uuid: "private-uuid" },
    ],
  };
}
function rejects(change: (config: NativeConfig) => void) {
  const config = nativeConfig();
  change(config);
  expect(() => nodeConfigInputs(JSON.stringify(config))).toThrow(
    /不受节点选择器支持/,
  );
}

describe("accepted routed-TUN compiler inputs", () => {
  it("extracts only safe backend, actual mixed/DNS ports and direct IPv6", () => {
    expect(nodeConfigInputs(JSON.stringify(nativeConfig()))).toEqual({
      datapath: "routed-tun",
      ipv6: "direct",
      ports: { mixed: 3200, tproxy: 7893, dns: 3202 },
      routedTUN: { interfaceName: "b6p-pilot_1", address: "10.203.4.1/30" },
    });
  });

  it("does not treat the unused TPROXY placeholder as an actual TUN listener", () => {
    const config = nativeConfig();
    config.inbounds[0].listen_port = 7893;
    expect(nodeConfigInputs(JSON.stringify(config)).ports).toEqual({
      mixed: 7893,
      tproxy: 7893,
      dns: 3202,
    });
  });

  it.each([
    ["netns", "private-ns"],
    ["unknown", "private-token"],
    ["stack", "gvisor"],
    ["dns_mode", "hijack"],
    ["mtu", 9000],
    ["auto_route", true],
    ["auto_redirect", true],
    ["auto_route", "false"],
    ["auto_redirect", null],
    ["udp_timeout", "1m"],
    ["udp_nat_max", 2048],
    ["interface_name", "br-lan"],
    ["interface_name", "b6p-a\n"],
    ["address", ["10.203.4.2/30"]],
    ["address", ["10.203.4.1/30", "fd00::1/126"]],
  ])(
    "rejects unsupported TUN field %s=%j without normalizing",
    (key, value) => {
      rejects((config) => Object.assign(config.inbounds[1], { [key]: value }));
    },
  );

  it.each([
    "auto_route",
    "auto_redirect",
    "mtu",
    "stack",
    "udp_timeout",
    "udp_nat_max",
    "dns_mode",
  ])("rejects missing required TUN field %s", (key) =>
    rejects((config) => {
      delete (config.inbounds[1] as Record<string, unknown>)[key];
    }),
  );

  it.each([
    [
      "missing IPv6 direct",
      (config: NativeConfig) => {
        config.route.rules = [];
      },
    ],
    [
      "IPv6 block",
      (config: NativeConfig) => {
        Object.assign(config.route.rules[1], { action: "reject" });
        delete config.route.rules[1].outbound;
      },
    ],
    [
      "duplicate IPv6 rule",
      (config: NativeConfig) => {
        config.route.rules.push({ ip_version: 6, outbound: "direct" });
      },
    ],
    [
      "conditional IPv6 rule",
      (config: NativeConfig) => {
        Object.assign(config.route.rules[1], { inbound: ["tun-in"] });
      },
    ],
    [
      "duplicate mixed inbound",
      (config: NativeConfig) => {
        config.inbounds[2] = { ...config.inbounds[0] };
      },
    ],
    [
      "duplicate TUN inbound",
      (config: NativeConfig) => {
        config.inbounds[2] = { ...config.inbounds[1] };
      },
    ],
    [
      "TUN plus TPROXY",
      (config: NativeConfig) => {
        config.inbounds.push({
          type: "tproxy",
          tag: "tproxy-in",
          listen: "127.0.0.1",
          listen_port: 7893,
        });
      },
    ],
    [
      "unknown inbound",
      (config: NativeConfig) => {
        config.inbounds.push({
          type: "socks",
          tag: "extra",
          listen: "127.0.0.1",
          listen_port: 9999,
        });
      },
    ],
    [
      "mixed DNS collision",
      (config: NativeConfig) => {
        config.inbounds[2].listen_port = 3200;
      },
    ],
    [
      "invalid mixed port",
      (config: NativeConfig) => {
        config.inbounds[0].listen_port = 1.5;
      },
    ],
    [
      "invalid DNS port",
      (config: NativeConfig) => {
        config.inbounds[2].listen_port = 0;
      },
    ],
    [
      "unsafe listener field",
      (config: NativeConfig) => {
        Object.assign(config.inbounds[0], {
          users: [{ password: "private-token" }],
        });
      },
    ],
  ])("rejects %s", (_label, change) => rejects(change));

  it("keeps the legacy strict listener/IPv6 contract and native UDP fields", () => {
    const config = nativeConfig();
    config.inbounds[1] = {
      type: "tproxy",
      tag: "tproxy-in",
      listen: "::",
      listen_port: 3201,
      udp_timeout: "2m",
      udp_nat_max: 1024,
    };
    config.inbounds[2].listen = "::";
    config.route.rules = [];
    expect(nodeConfigInputs(JSON.stringify(config))).toEqual({
      datapath: "tproxy",
      ipv6: "follow",
      ports: { mixed: 3200, tproxy: 3201, dns: 3202 },
    });
    for (const fields of [
      { network: "tcp" },
      { ipv6_only: true },
      { udp_timeout: "5s" },
      { udp_nat_max: 8192 },
      { netns: "private-ns" },
    ]) {
      const changed = structuredClone(config);
      Object.assign(changed.inbounds[1], fields);
      expect(() => nodeConfigInputs(JSON.stringify(changed))).toThrow(
        /不受节点选择器支持/,
      );
    }
  });

  it.each(["{", "null", "[]", "{}"])(
    "rejects missing or malformed config %s",
    (value) => {
      expect(() => nodeConfigInputs(value)).toThrow(/不受节点选择器支持/);
    },
  );
});

describe("safe routed-TUN preferences match Go literal validation", () => {
  it.each([
    "10.0.0.1/30",
    "172.16.0.1/30",
    "172.31.255.253/30",
    "192.168.255.253/30",
  ])("allows RFC1918 first usable /30 %s", (address) =>
    expect(validRoutedTUN({ interfaceName: "b6p-test_1", address })).toBe(true),
  );
  it.each([
    "10.0.0.0/30",
    "10.0.0.2/30",
    "10.0.0.3/30",
    "10.0.0.1/29",
    "010.0.0.1/30",
    "10.0.0.01/30",
    "10.0.0.1/030",
    "10.0.0.1/30\n",
    "172.15.0.1/30",
    "172.32.0.1/30",
    "192.169.0.1/30",
    "100.64.0.1/30",
    "198.18.0.1/30",
    "127.0.0.1/30",
    "8.8.8.1/30",
    "fd00::1/30",
    "::ffff:10.0.0.1/30",
    "10.0.256.1/30",
    "10.0.-1.1/30",
  ])("rejects nonliteral, public or non-first host %s", (address) => {
    expect(validRoutedTUN({ interfaceName: "b6p-test", address })).toBe(false);
  });
  it.each([
    "b6p-",
    "tun0",
    "br-lan",
    "b6p-a;id",
    "b6p-a:1",
    "b6p-a/b",
    "b6p-a\n",
    "b6p-abcdefghijkl",
  ])("rejects unowned interface %j", (interfaceName) =>
    expect(validRoutedTUN({ interfaceName, address: "10.0.0.1/30" })).toBe(
      false,
    ),
  );
  it("rejects unsafe extra persisted fields", () => {
    expect(
      validRoutedTUN({
        interfaceName: "b6p-a",
        address: "10.0.0.1/30",
        netns: "private-token",
      }),
    ).toBe(false);
  });
});
