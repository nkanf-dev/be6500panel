import type { Page } from "@playwright/test";
import { Schema } from "effect";
import {
  DevicesSchema,
  FrpcSchema,
  HealthSchema,
  LogsSchema,
  ModulesSchema,
  NetworkSchema,
  ProxyCaptureSchema,
  ProxyNodesSchema,
  RouterSchema,
  RuntimeConfigSchema,
  RuntimeSchema,
  RuntimeStatusSchema,
  SessionSchema,
  SnapshotSchema,
  SystemSchema,
} from "../src/lib/contracts";
import { DeviceActivityHistorySchema } from "../src/lib/device-activity-contracts";
import { TrafficHistorySchema } from "../src/lib/traffic-history-contracts";
import {
  ServiceActionResultSchema,
  ServiceStatusSchema,
} from "../src/lib/service-status-api";
import { ProxyMetricsSchema } from "../src/modules/proxy/telemetry-api";
import { DeviceAnnotationsSchema } from "../src/modules/devices/annotations-api";
import {
  RequestTraceHistorySchema,
  RequestTraceSchema,
} from "../src/modules/proxy/request-trace-contracts";
import {
  CommitSchema,
  ConfigurationSchema,
  ConfigurationStatusSchema,
  DraftsSchema,
} from "../src/components/configuration/contracts";
import {
  BackupEnvelopeSchema,
  ImportPreviewSchema,
  ImportStageSchema,
} from "../src/components/maintenance/contracts";
import { readFrpcDocument } from "../src/modules/frpc-document";

/** Entirely synthetic source fixtures. No router captures, files or real credentials. */
export const fixtureMAC = {
  laptop: "02:00:00:00:00:21",
  tv: "02:00:00:00:00:22",
} as const;
export const fixtureName = {
  laptop: "fixture-laptop",
  tv: "fixture-tv",
} as const;
export const fixtureToken = "fixture-only-not-a-real-token";
export const fixtureAcceptedToml = `# fixture accepted FRPC document: preserve this comment
serverAddr = "fixture-frps.example.test"
serverPort = 7443
transport.protocol = "quic"
transport.tls.enable = false
transport.poolCount = 4
auth.token = "${fixtureToken}"
[[proxies]]
name = "fixture-ssh"
type = "tcp"
localIP = "192.0.2.21"
localPort = 22
remotePort = 0 # native assigned port, not a measured value
transport.useCompression = true
[[proxies]]
name = "fixture-web"
type = "http"
localIP = "127.0.0.1"
localPort = 8081
customDomains = ["fixture-home.example.test"]
healthCheck.type = "http"
healthCheck.path = "/fixture-ready"
`;

const checked = <A, I>(schema: Schema.Schema<A, I>, value: unknown): A =>
  Schema.decodeUnknownSync(schema)(value);
const iso = (time: number) => new Date(time).toISOString();
const fixtureDocument = "config interface 'lan'\n option ipaddr '192.0.2.1'\n";
const fixtureImportDocument = fixtureDocument.replace("192.0.2.1", "192.0.2.2");
const fixtureDiff =
  "--- network\n+++ network\n@@ -1,2 +1,2 @@\n config interface 'lan'\n- option ipaddr '192.0.2.1'\n+ option ipaddr '192.0.2.2'";
/** BOM and whitespace are deliberately kept: preview must send original bytes, not JSON.stringify. */
export const fixtureRawBackup =
  '\uFEFF  {\n  "model":"fixture-router", "build":"fixture-only", "generation":7,\n  "createdAt":"2026-01-01T00:00:00Z", "scopes":["network"],\n  "documents":[{"module":"network","content":' +
  JSON.stringify(fixtureImportDocument) +
  ',"digest":"fixture-after-digest"}]\n}\n';

type Write = {
  method: string;
  path: string;
  raw: string | null;
  body: unknown;
};
export function createMaturityFixture(now = Date.now()) {
  let conflicting = false;
  let revision = 3;
  let annotations = checked(DeviceAnnotationsSchema, { revision, devices: {} });
  let generation = 7;
  let acceptedToml = fixtureAcceptedToml;
  let configurationStatus = checked(ConfigurationStatusSchema, {
    enabled: true,
    generation: 7,
  });
  let drafts = checked(DraftsSchema, { drafts: [] });
  let traces = checked(RequestTraceHistorySchema, {
    traces: [],
    targets: [
      {
        id: "google204",
        label: "fixture Google 204",
        url: "https://www.google.com/generate_204",
      },
      {
        id: "cloudflare",
        label: "fixture Cloudflare",
        url: "https://www.cloudflare.com/cdn-cgi/trace",
      },
    ],
    limits: {
      timeoutMs: 10000,
      bodyBytes: 65536,
      concurrency: 1,
      capacity: 64,
    },
    running: false,
  });
  const system = () =>
    checked(SystemSchema, {
      mode: "host",
      hostname: "fixture-router",
      os: "fixture OpenWrt",
      arch: "aarch64",
      kernel: "fixture-kernel",
      uptimeSeconds: 3600,
      cpuCount: 4,
      memory: { totalBytes: 536870912, availableBytes: 268435456 },
      load: [0.2, 0.3, 0.4],
      sampledAt: iso(Date.now()),
    });
  const runtimeStatus = () =>
    checked(RuntimeStatusSchema, {
      service: "frpc",
      state: "stopped",
      configured: true,
      artifactAvailable: true,
      generation,
      version: "fixture-0.1",
      rssBytes: 0,
      rssAvailable: false,
      desired: false,
      restarts: 0,
    });
  const router = () =>
    checked(RouterSchema, {
      platform: {
        model: "fixture-router",
        firmware: "fixture-only",
        kernel: "fixture-kernel",
        architecture: "aarch64",
      },
      currentClientIP: "192.0.2.21",
      devices: [
        {
          ip: "192.0.2.21",
          mac: fixtureMAC.laptop,
          hostname: fixtureName.laptop,
          expiresAt: null,
          online: true,
          eligible: true,
        },
        {
          ip: conflicting ? "192.0.2.21" : "192.0.2.22",
          mac: fixtureMAC.tv,
          hostname: fixtureName.tv,
          expiresAt: null,
          online: true,
          eligible: true,
        },
      ],
      wifi: [
        {
          name: "fixture-radio",
          ssid: "fixture-ssid",
          band: "5g",
          channel: 36,
          bandwidth: "HE80",
          disabled: false,
          encryption: "sae",
        },
      ],
      dns: { resolvers: ["192.0.2.53"], leaseCount: 2 },
      firewall: {
        ipv4: { input: "ACCEPT", forward: "DROP", output: "ACCEPT", rules: 4 },
        ipv6: { input: "ACCEPT", forward: "DROP", output: "ACCEPT", rules: 2 },
      },
      traffic: [
        {
          interface: "fixture-wan",
          rxBytes: 900000,
          txBytes: 450000,
          rxBytesPerSecond: 120,
          txBytesPerSecond: 60,
        },
      ],
      routes: [
        {
          family: "ipv4",
          destination: "0.0.0.0/0",
          gateway: "192.0.2.1",
          interface: "fixture-wan",
          metric: 1,
        },
      ],
      sampledAt: iso(Date.now()),
      errors: [],
    });
  const activity = (url: URL) => {
    const range = url.searchParams.get("range") ?? "24h";
    const search = (url.searchParams.get("search") ?? "").toLowerCase();
    const devices = Object.entries(fixtureMAC)
      .map(([key, id], index) => ({
        id,
        name: fixtureName[key as keyof typeof fixtureName],
        addresses: [conflicting ? "192.0.2.21" : `192.0.2.${21 + index}`],
        interface: "fixture-wlan",
        associated: true,
        lastSeen: iso(Date.now()),
        stale: false,
        rxBytes: 1200 * (index + 1),
        txBytes: 600 * (index + 1),
        coverageSeconds: 60,
        rxBytesPerSecond: 20 * (index + 1),
        txBytesPerSecond: 10 * (index + 1),
        rawRXBytes: 9000 * (index + 1),
        rawTXBytes: 4500 * (index + 1),
        addressConflicts: conflicting ? ["192.0.2.21"] : [],
        links: [
          {
            interface: "fixture-wlan",
            protocol: "fixture-802.11ax",
            signalDBM: -48,
            noiseDBM: -92,
            negotiatedRX: "fixture-866Mbps",
            negotiatedTX: "fixture-866Mbps",
          },
        ],
        // The first bucket is missing, not measured zero. The second is partial (60 / 120s).
        samples: [
          {
            time: iso(now - 240000),
            rxBytes: null,
            txBytes: null,
            coverageSeconds: 0,
          },
          {
            time: iso(now - 120000),
            rxBytes: 1200 * (index + 1),
            txBytes: 600 * (index + 1),
            coverageSeconds: 60,
          },
        ],
      }))
      .filter(
        (device) =>
          !search ||
          `${device.id} ${device.name} ${device.addresses.join(" ")}`
            .toLowerCase()
            .includes(search),
      );
    return checked(DeviceActivityHistorySchema, {
      enabled: true,
      persistent: false,
      retentionDays: 7,
      source: "trafficd",
      direction: "vendor-rx-tx",
      range,
      resolutionSeconds: 120,
      state: "ok",
      sampledAt: iso(Date.now()),
      oldestAt: iso(now - 240000),
      deviceCount: 2,
      matchedCount: devices.length,
      truncated: false,
      devices,
      groups: devices.length
        ? [
            {
              name: "fixture-wlan",
              deviceCount: devices.length,
              rxBytes: devices.reduce((sum, d) => sum + d.rxBytes, 0),
              txBytes: devices.reduce((sum, d) => sum + d.txBytes, 0),
              coverageSeconds: 60,
              rxBytesPerSecond: devices.reduce(
                (sum, d) => sum + d.rxBytesPerSecond,
                0,
              ),
              txBytesPerSecond: devices.reduce(
                (sum, d) => sum + d.txBytesPerSecond,
                0,
              ),
            },
          ]
        : [],
    });
  };
  const metrics = () =>
    checked(ProxyMetricsSchema, {
      state: "ready",
      reason: "fixture synthetic controller observations",
      source: "fixture-controller",
      sampledAt: iso(Date.now()),
      capabilities: {
        connections: { available: true, reason: "fixture observations" },
        traffic: { available: true, reason: "fixture counters" },
        routing: { available: true, reason: "fixture rows" },
        latency: { available: false, reason: "no fixture probe has run" },
        requestPhases: {
          available: false,
          reason: "ordinary encrypted traffic has no request phases",
        },
      },
      totals: { uploadBytes: 600, downloadBytes: 1200 },
      activeConnections: 1,
      truncated: false,
      connections: [
        {
          id: "fixture-connection",
          startedAt: iso(now - 1000),
          ageMs: 1000,
          network: "tcp",
          sourceIP: "192.0.2.21",
          sourcePort: 42000,
          destinationIP: "198.51.100.10",
          destinationPort: 443,
          host: "fixture-origin.example.test",
          uploadBytes: 600,
          downloadBytes: 1200,
          outbound: "proxy",
          ruleId: "fixture-rule",
          rule: "fixture rule",
        },
      ],
      traffic: [
        { time: iso(now), uploadRate: 10, downloadRate: 20, reset: false },
      ],
      probes: [],
    });
  const services = () =>
    checked(ServiceStatusSchema, {
      source: "fixture procd+/proc",
      sampledAt: iso(Date.now()),
      checkedAt: iso(Date.now()),
      stale: false,
      errors: [],
      services: [
        {
          name: "be6500-rescue",
          instance: "fixture-rescue",
          configured: "present",
          registered: "registered",
          processState: "running",
          pid: 101,
          protected: true,
          actions: ["start", "stop", "reload", "restart"],
        },
        {
          name: "dnsmasq",
          instance: "fixture-dns",
          configured: "present",
          registered: "registered",
          processState: "running",
          pid: 102,
          protected: false,
          actions: ["reload", "restart"],
          actionImpact: "fixture: DNS/DHCP can be interrupted",
        },
      ],
    });
  const preview = checked(ImportPreviewSchema, {
    id: "fixture-preview",
    generation: 7,
    sourceModel: "fixture-router",
    currentModel: "fixture-router",
    modelMismatch: false,
    expiresAt: iso(now + 3600000),
    summary: { added: 0, modified: 1, deleted: 0, unchanged: 0, uncompared: 0 },
    changes: [
      {
        module: "network",
        kind: "modified",
        beforeBytes: fixtureDocument.length,
        afterBytes: fixtureImportDocument.length,
        beforeDigest: "fixture-before-digest",
        afterDigest: "fixture-after-digest",
        diff: fixtureDiff,
        stageable: true,
        valid: true,
        errors: [],
        dependencies: [],
        risks: [
          {
            code: "fixture_management_change",
            message: "fixture 管理地址可能改变",
          },
        ],
      },
    ],
    warnings: [],
  });
  const stage = checked(ImportStageSchema, {
    generation: 7,
    drafts: [
      {
        id: "fixture-import-draft",
        module: "network",
        generation: 7,
        diff: fixtureDiff,
        risks: preview.changes[0].risks,
        valid: true,
        errors: [],
        dependencies: [],
        createdAt: iso(now),
      },
    ],
    warnings: [],
  });
  const failedTrace = checked(RequestTraceSchema, {
    id: "fixture-proxy-tls-failed",
    targetId: "google204",
    targetLabel: "fixture Google 204",
    url: "https://www.google.com/generate_204",
    route: "proxy",
    startedAt: iso(now),
    finishedAt: iso(now + 400),
    totalMs: 400,
    outcome: "failed",
    statusCode: null,
    bytesRead: 0,
    bodyLimitReached: false,
    peerAddress: "127.0.0.1:2080",
    peerScope: "proxy",
    failurePhase: "tls",
    errorCode: "fixture_tls_failure",
    phases: [
      {
        id: "dns",
        observed: false,
        startMs: null,
        endMs: null,
        durationMs: null,
        reason: "proxy_origin_dns_not_observable",
      },
      { id: "tcp", observed: true, startMs: 5, endMs: 25, durationMs: 20 },
      {
        id: "connect",
        observed: false,
        startMs: null,
        endMs: null,
        durationMs: null,
        reason: "connect_timing_not_exposed_by_httptrace",
      },
      { id: "tls", observed: true, startMs: 25, endMs: null, durationMs: null },
      {
        id: "ttfb",
        observed: false,
        startMs: null,
        endMs: null,
        durationMs: null,
      },
      {
        id: "transfer",
        observed: false,
        startMs: null,
        endMs: null,
        durationMs: null,
      },
    ],
  });
  const get = (url: URL): unknown => {
    switch (url.pathname) {
      case "/api/session":
        return checked(SessionSchema, {
          authenticated: true,
          authRequired: false,
        });
      case "/api/health":
        return checked(HealthSchema, {
          status: "ok",
          mode: "host",
          readOnly: false,
        });
      case "/api/modules":
        return checked(ModulesSchema, {
          modules: ["system", "network", "devices", "proxy", "frpc"].map(
            (id) => ({
              id,
              title: `fixture-${id}`,
              description: "synthetic fixture capability",
              state: "ready",
              capabilities: [
                {
                  id: `fixture-${id}-read`,
                  title: "fixture read",
                  supported: true,
                },
              ],
            }),
          ),
        });
      case "/api/system":
        return system();
      case "/api/events":
        return checked(SnapshotSchema, {
          system: system(),
          sampledAt: iso(Date.now()),
        });
      case "/api/router":
        return router();
      case "/api/network":
        return checked(NetworkSchema, {
          interfaces: [
            {
              name: "fixture-wan",
              addresses: ["192.0.2.1/24"],
              up: true,
              mtu: 1500,
            },
          ],
          routes: [],
          routeObservationSupported: true,
        });
      case "/api/devices":
        return checked(DevicesSchema, {
          devices: [],
          supported: false,
          reason: "fixture uses router inventory",
        });
      case "/api/frpc":
        return checked(FrpcSchema, {
          supported: true,
          running: false,
          reason: "fixture stopped runtime",
          proxies: [],
        });
      case "/api/logs":
        return checked(LogsSchema, {
          entries: [
            {
              sequence: 1,
              time: iso(now),
              level: "info",
              code: "fixture_loaded",
              module: "fixture",
              message: "synthetic fixture only",
            },
          ],
          capacity: 100,
        });
      case "/api/runtime":
        return checked(RuntimeSchema, {
          enabled: true,
          services: [
            runtimeStatus(),
            { ...runtimeStatus(), service: "sing-box" },
          ],
        });
      case "/api/runtime/config":
        return checked(RuntimeConfigSchema, {
          service: url.searchParams.get("service"),
          config: acceptedToml,
          generation,
        });
      case "/api/devices/activity":
        return activity(url);
      case "/api/devices/annotations":
        return checked(DeviceAnnotationsSchema, annotations);
      case "/api/proxy/metrics":
        return metrics();
      case "/api/proxy/nodes":
        return checked(ProxyNodesSchema, {
          nodes: [],
          diagnostics: [],
          selectedNodeId: "",
        });
      case "/api/proxy/capture":
        return checked(ProxyCaptureSchema, {
          active: false,
          desired: false,
          clients: [],
          commands: 0,
        });
      case "/api/proxy/request-traces":
        return checked(RequestTraceHistorySchema, traces);
      case "/api/system/services":
        return services();
      case "/api/configuration/status":
        return checked(ConfigurationStatusSchema, configurationStatus);
      case "/api/configuration":
        return checked(ConfigurationSchema, {
          generation: 7,
          documents: [{ module: "network", content: fixtureDocument }],
        });
      case "/api/configuration/drafts":
        return checked(DraftsSchema, drafts);
      case "/api/traffic/history":
        return checked(TrafficHistorySchema, {
          enabled: true,
          persistent: false,
          retentionDays: 7,
          source: "fixture-wan",
          range: url.searchParams.get("range") ?? "30m",
          resolutionSeconds: 120,
          samples: [
            {
              time: iso(now - 120000),
              rx: 20,
              tx: 10,
              rxPeak: 24,
              txPeak: 12,
              rxBytes: 1200,
              txBytes: 600,
              coverageSeconds: 60,
            },
          ],
          summary: { rxBytes: 1200, txBytes: 600, coverageSeconds: 60 },
          oldestAt: iso(now - 120000),
        });
      default:
        throw new Error(`Unmapped fixture GET: ${url.pathname}`);
    }
  };
  const setPending = (id: string, nextGeneration = 8) => {
    const deadline = iso(Date.now() + 120000);
    configurationStatus = checked(ConfigurationStatusSchema, {
      enabled: true,
      generation: nextGeneration,
      pendingCommit: { id, deadline },
      operation: {
        id,
        deadline,
        generation: nextGeneration,
        changedModules: ["network"],
        state: "pending_confirmation",
        phase: "pending",
        canConfirm: true,
        canRollback: true,
      },
    });
  };
  const write = (entry: Write): unknown => {
    // All handlers model server readback. Nothing touches a router or the filesystem.
    const body = entry.body as Record<string, unknown>;
    switch (`${entry.method} ${entry.path}`) {
      case "POST /api/devices/annotations": {
        revision++;
        annotations = checked(DeviceAnnotationsSchema, {
          revision,
          devices: {
            ...annotations.devices,
            [String(body.mac)]: {
              label: body.label,
              note: body.note,
              tags: body.tags,
            },
          },
        });
        return annotations;
      }
      case "POST /api/proxy/request-traces":
        traces = checked(RequestTraceHistorySchema, {
          ...traces,
          traces: [failedTrace],
        });
        return failedTrace;
      case "POST /api/configuration/confirm":
      case "POST /api/configuration/rollback": {
        const phase = entry.path.endsWith("confirm")
          ? "committed"
          : "rolled_back";
        const result = checked(CommitSchema, {
          id: body.id,
          state: phase,
          generation: configurationStatus.generation,
          changedModules: ["network"],
        });
        configurationStatus = checked(ConfigurationStatusSchema, {
          enabled: true,
          generation: result.generation,
          operation: {
            ...result,
            phase,
            canConfirm: false,
            canRollback: false,
          },
        });
        return result;
      }
      case "POST /api/system/services/action":
        return checked(ServiceActionResultSchema, {
          service: body.service,
          action: body.action,
          commandAccepted: true,
          snapshot: services(),
        });
      case "POST /api/maintenance/import/preview":
        return checked(ImportPreviewSchema, preview);
      case "POST /api/maintenance/import/stage":
        drafts = checked(DraftsSchema, { drafts: stage.drafts });
        return checked(ImportStageSchema, stage);
      case "POST /api/runtime/configure":
        acceptedToml = String(body.config);
        generation++;
        return runtimeStatus();
      default:
        throw new Error(
          `Unmapped fixture write: ${entry.method} ${entry.path}`,
        );
    }
  };
  return {
    get,
    write,
    setPending,
    setConflicting: () => {
      conflicting = true;
    },
    failedTrace,
    preview,
    stage,
  };
}

export async function installMaturityFixture(page: Page, baseURL: string) {
  const origin = new URL(baseURL).origin;
  const source = createMaturityFixture();
  const reads: string[] = [];
  const writes: Write[] = [];
  const unexpected: string[] = [];
  const armed = new Map<string, number>();
  const armWrite = (path: string, method = "POST") => {
    const key = `${method} ${path}`;
    armed.set(key, (armed.get(key) ?? 0) + 1);
  };
  await page.route(
    (url) => url.origin === origin && url.pathname.startsWith("/api/"),
    async (route) => {
      const request = route.request();
      const url = new URL(request.url());
      const method = request.method();
      try {
        // GET dispatch runs first. Unknown reads fail closed; no live API fallback.
        if (method === "GET") {
          reads.push(`${url.pathname}${url.search}`);
          const value = source.get(url);
          if (url.pathname === "/api/events") {
            await route.fulfill({
              status: 200,
              contentType: "text/event-stream",
              body: `event: snapshot\ndata: ${JSON.stringify(value)}\n\n`,
            });
          } else {
            await route.fulfill({
              status: 200,
              contentType: "application/json",
              body: JSON.stringify(value),
            });
          }
          return;
        }
        const raw = request.postData();
        let body: unknown;
        if (url.pathname === "/api/maintenance/import/preview")
          body = undefined;
        else body = raw ? JSON.parse(raw) : undefined;
        const entry = { method, path: url.pathname, raw, body };
        writes.push(entry);
        const key = `${method} ${url.pathname}`;
        if (!armed.get(key))
          throw new Error(`Unarmed fixture mutation: ${key}`);
        armed.set(key, armed.get(key)! - 1);
        await route.fulfill({
          status: 200,
          contentType: "application/json",
          body: JSON.stringify(source.write(entry)),
        });
      } catch (error) {
        unexpected.push(error instanceof Error ? error.message : String(error));
        await route.fulfill({
          status: 409,
          contentType: "application/json",
          body: JSON.stringify({
            error: {
              code: "fixture_unexpected_request",
              message: "Synthetic fixture refused an unexpected request",
            },
          }),
        });
      }
    },
  );
  return { ...source, reads, writes, unexpected, armWrite };
}

/** Run with the project's Bun, without launching Playwright, to validate fixture contracts. */
export function validateMaturityFixtures() {
  const source = createMaturityFixture();
  for (const path of [
    "session",
    "health",
    "modules",
    "system",
    "events",
    "router",
    "network",
    "devices",
    "frpc",
    "logs",
    "runtime",
    "runtime/config?service=frpc",
    "devices/activity?range=24h",
    "devices/activity?range=7d&search=02%3A00%3A00%3A00%3A00%3A21",
    "devices/activity?range=30m",
    "devices/annotations",
    "proxy/metrics",
    "proxy/nodes",
    "proxy/capture",
    "proxy/request-traces",
    "system/services",
    "configuration/status",
    "configuration",
    "configuration/drafts",
    "traffic/history?range=30m",
  ]) {
    source.get(new URL(`/api/${path}`, "http://fixture.example.test"));
  }
  checked(
    BackupEnvelopeSchema,
    JSON.parse(fixtureRawBackup.replace(/^\uFEFF/, "").trim()),
  );
  source.setConflicting();
  source.get(
    new URL("/api/devices/activity?range=24h", "http://fixture.example.test"),
  );
  source.setPending("fixture-contract-check");
  source.get(
    new URL("/api/configuration/status", "http://fixture.example.test"),
  );
  const document = readFrpcDocument(fixtureAcceptedToml);
  if (
    !document.supported ||
    !document.hasToken ||
    document.input.proxies.length !== 2
  ) {
    throw new Error(
      "FRPC fixture does not have the expected supported native TOML shape",
    );
  }
}
