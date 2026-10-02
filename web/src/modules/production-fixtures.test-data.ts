export const runtimeStatus = {
  service: "sing-box" as const,
  state: "stopped",
  generation: 4,
  configured: true,
  artifactAvailable: true,
  version: "synthetic-1",
  rssBytes: 0,
  rssAvailable: false,
  desired: false,
  restarts: 0,
};
export const routerSnapshot = {
  platform: {
    model: "Test Router",
    firmware: "synthetic",
    kernel: "test",
    architecture: "arm",
  },
  devices: [
    {
      ip: "192.0.2.20",
      mac: "02:00:00:00:00:20",
      hostname: "test-client",
      expiresAt: null,
      online: true,
    },
  ],
  wifi: [
    {
      name: "radio-test",
      ssid: "Test SSID",
      band: "5g",
      channel: 36,
      bandwidth: "HE80",
      disabled: false,
      encryption: "sae",
    },
  ],
  dns: { resolvers: ["192.0.2.53"], leaseCount: 1 },
  firewall: {
    ipv4: { input: "ACCEPT", forward: "DROP", output: "ACCEPT", rules: 4 },
    ipv6: { input: "ACCEPT", forward: "DROP", output: "ACCEPT", rules: 2 },
  },
  routes: [
    {
      family: "ipv4",
      destination: "0.0.0.0/0",
      gateway: "192.0.2.1",
      interface: "wan-test",
      metric: 1,
    },
  ],
  traffic: [
    {
      interface: "wan-test",
      rxBytes: 100,
      txBytes: 50,
      rxBytesPerSecond: 0,
      txBytesPerSecond: 0,
    },
  ],
  sampledAt: "2026-01-01T00:00:00Z",
  errors: [],
};
export const proxyNodes = {
  nodes: [
    {
      id: "node-synthetic",
      label: "Synthetic Node",
      server: "node.example.test",
      port: 443,
      protocol: "vless",
      transport: "tcp",
      reality: true,
      vision: true,
      utls: true,
      udp: false,
    },
  ],
  diagnostics: [],
  selectedNodeId: "",
};
export const jsonResponse = (body: unknown, status = 200) =>
  new Response(JSON.stringify(body), {
    status,
    headers: { "Content-Type": "application/json" },
  });
