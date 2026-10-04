import { Schema } from "effect";
import { ProxyPolicySummarySchema } from "../modules/proxy/policy-contracts";

export const HealthSchema = Schema.Struct({
  status: Schema.Literal("ok"),
  mode: Schema.Literal("demo", "host"),
  readOnly: Schema.Boolean,
});
export const SessionSchema = Schema.Struct({
  authenticated: Schema.Boolean,
  authRequired: Schema.Boolean,
});
export const CapabilitySchema = Schema.Struct({
  id: Schema.String,
  title: Schema.String,
  supported: Schema.Boolean,
  reason: Schema.optional(Schema.String),
});
export const ModuleSchema = Schema.Struct({
  id: Schema.String,
  title: Schema.String,
  description: Schema.String,
  state: Schema.Literal("ready", "unavailable"),
  capabilities: Schema.Array(CapabilitySchema),
});
export const ModulesSchema = Schema.Struct({
  modules: Schema.Array(ModuleSchema),
});
export const SystemSchema = Schema.Struct({
  mode: Schema.Literal("demo", "host"),
  hostname: Schema.String,
  os: Schema.String,
  arch: Schema.String,
  kernel: Schema.String,
  uptimeSeconds: Schema.Number,
  cpuCount: Schema.Number,
  memory: Schema.Struct({
    totalBytes: Schema.Number,
    availableBytes: Schema.Number,
  }),
  load: Schema.Tuple(Schema.Number, Schema.Number, Schema.Number),
  sampledAt: Schema.String,
});
export const NetworkSchema = Schema.Struct({
  interfaces: Schema.Array(
    Schema.Struct({
      name: Schema.String,
      addresses: Schema.Array(Schema.String),
      up: Schema.Boolean,
      mtu: Schema.Number,
    }),
  ),
  routes: Schema.Array(Schema.Unknown),
  routeObservationSupported: Schema.Boolean,
});
export const DevicesSchema = Schema.Struct({
  devices: Schema.Array(Schema.Unknown),
  supported: Schema.Boolean,
  reason: Schema.String,
});
export const FrpcSchema = Schema.Struct({
  supported: Schema.Boolean,
  running: Schema.Boolean,
  reason: Schema.String,
  proxies: Schema.Array(Schema.Unknown),
});
export const LogsSchema = Schema.Struct({
  entries: Schema.Array(
    Schema.Struct({
      sequence: Schema.Number,
      time: Schema.String,
      level: Schema.String,
      code: Schema.String,
      module: Schema.String,
      message: Schema.String,
    }),
  ),
  capacity: Schema.Number,
});
export const SnapshotSchema = Schema.Struct({
  system: SystemSchema,
  sampledAt: Schema.String,
});
export const PlanSchema = Schema.Struct({
  id: Schema.String,
  generation: Schema.Number,
  readOnly: Schema.Boolean,
  summary: Schema.String,
  steps: Schema.Array(
    Schema.Struct({
      module: Schema.String,
      action: Schema.String,
      detail: Schema.String,
    }),
  ),
  warnings: Schema.Array(Schema.String),
  canApply: Schema.Boolean,
});

export type Health = typeof HealthSchema.Type;
export type Session = typeof SessionSchema.Type;
export type ModuleInfo = typeof ModuleSchema.Type;
export type SystemInfo = typeof SystemSchema.Type;
export type NetworkInfo = typeof NetworkSchema.Type;
export type FrpcInfo = typeof FrpcSchema.Type;
export type OperationPlan = typeof PlanSchema.Type;
export type ProxyPlanInput = {
  mode: "split" | "global" | "direct";
  dnsStrategy: "split" | "direct";
  ipv6Policy: "follow" | "direct" | "block";
  failurePolicy: "direct" | "block-proxy";
  nodeCount: number;
};
export type FrpcProxy = {
  name: string;
  type: "tcp" | "udp" | "http" | "https";
  localAddress: string;
  localPort: number;
  remotePort?: number;
  domains?: string[];
};
export type FrpcPlanInput = {
  serverAddress: string;
  serverPort: number;
  tls: boolean;
  transport: "tcp" | "quic";
  proxies: FrpcProxy[];
};

// Authenticated production observations. No device identities or private configs
// are persisted by the browser.
export const RouterSchema = Schema.Struct({
  platform: Schema.Struct({
    model: Schema.String,
    firmware: Schema.String,
    kernel: Schema.String,
    architecture: Schema.String,
  }),
  devices: Schema.Array(
    Schema.Struct({
      ip: Schema.String,
      mac: Schema.String,
      hostname: Schema.String,
      expiresAt: Schema.NullOr(Schema.String),
      online: Schema.Boolean,
      eligible: Schema.optional(Schema.Boolean),
    }),
  ),
  wifi: Schema.Array(
    Schema.Struct({
      name: Schema.String,
      ssid: Schema.String,
      band: Schema.String,
      channel: Schema.Number,
      bandwidth: Schema.String,
      disabled: Schema.Boolean,
      encryption: Schema.String,
    }),
  ),
  dns: Schema.Struct({
    resolvers: Schema.Array(Schema.String),
    leaseCount: Schema.Number,
  }),
  firewall: Schema.Struct({
    ipv4: Schema.Struct({
      input: Schema.String,
      forward: Schema.String,
      output: Schema.String,
      rules: Schema.Number,
    }),
    ipv6: Schema.Struct({
      input: Schema.String,
      forward: Schema.String,
      output: Schema.String,
      rules: Schema.Number,
    }),
  }),
  traffic: Schema.Array(
    Schema.Struct({
      interface: Schema.String,
      rxBytes: Schema.Number,
      txBytes: Schema.Number,
      rxBytesPerSecond: Schema.Number,
      txBytesPerSecond: Schema.Number,
    }),
  ),
  routes: Schema.Array(
    Schema.Struct({
      family: Schema.String,
      destination: Schema.String,
      gateway: Schema.String,
      interface: Schema.String,
      metric: Schema.Number,
    }),
  ),
  sampledAt: Schema.String,
  currentClientIP: Schema.optional(Schema.String),
  errors: Schema.Array(
    Schema.Struct({
      module: Schema.String,
      code: Schema.String,
      message: Schema.String,
    }),
  ),
});
export const RuntimeServiceSchema = Schema.Literal("sing-box", "frpc");
export const RuntimeStatusSchema = Schema.Struct({
  service: RuntimeServiceSchema,
  state: Schema.String,
  generation: Schema.Number,
  configured: Schema.Boolean,
  artifactAvailable: Schema.Boolean,
  version: Schema.optional(Schema.String),
  pid: Schema.optional(Schema.Number),
  rssBytes: Schema.Number,
  rssAvailable: Schema.Boolean,
  desired: Schema.Boolean,
  restarts: Schema.Number,
  retryAt: Schema.optional(Schema.String),
  errorCode: Schema.optional(Schema.String),
  recoveryPlan: Schema.optional(Schema.Array(Schema.String)),
  restored: Schema.optional(Schema.Boolean),
  needsRecovery: Schema.optional(Schema.Boolean),
});
export const RuntimeSchema = Schema.Struct({
  enabled: Schema.Boolean,
  services: Schema.Array(RuntimeStatusSchema),
});
export const RuntimeConfigSchema = Schema.Struct({
  service: RuntimeServiceSchema,
  config: Schema.String,
  generation: Schema.Number,
});
export const ProxyDiagnosticSchema = Schema.Struct({
  scope: Schema.String,
  index: Schema.Number,
  code: Schema.String,
  message: Schema.String,
});
export const ProxyNodesSchema = Schema.Struct({
  nodes: Schema.Array(
    Schema.Struct({
      id: Schema.String,
      label: Schema.String,
      server: Schema.String,
      port: Schema.Number,
      protocol: Schema.String,
      transport: Schema.String,
      reality: Schema.Boolean,
      vision: Schema.Boolean,
      utls: Schema.Boolean,
      udp: Schema.Boolean,
    }),
  ),
  diagnostics: Schema.Array(ProxyDiagnosticSchema),
  policySummary: Schema.optional(ProxyPolicySummarySchema),
  revision: Schema.optional(Schema.String),
  selectedNodeId: Schema.String,
});
export const ProxySelectSchema = Schema.Struct({
  status: RuntimeStatusSchema,
  configSHA256: Schema.String,
  diagnostics: Schema.Array(ProxyDiagnosticSchema),
});
export const ProxyCaptureSchema = Schema.Struct({
  active: Schema.Boolean,
  scope: Schema.optional(Schema.Literal("gateway", "devices")),
  lanIPv4Prefixes: Schema.optional(Schema.Array(Schema.String)),
  installedLanIPv4Prefixes: Schema.optional(Schema.Array(Schema.String)),
  desired: Schema.optional(Schema.Boolean),
  clients: Schema.optional(
    Schema.Array(
      Schema.Struct({
        mac: Schema.String,
        ip: Schema.String,
        hostname: Schema.String,
      }),
    ),
  ),
  ipv6: Schema.optional(Schema.Literal("follow", "direct", "block")),
  error: Schema.optional(Schema.String),
  state: Schema.optional(Schema.String),
  cleanupPending: Schema.optional(Schema.Boolean),
  scopeState: Schema.optional(
    Schema.Literal("current", "changed", "unresolved"),
  ),
  installedClients: Schema.optional(
    Schema.Array(
      Schema.Struct({
        mac: Schema.String,
        ip: Schema.String,
        hostname: Schema.String,
      }),
    ),
  ),
  clientIPv4: Schema.optional(Schema.String),
  clientIPv6: Schema.optional(Schema.String),
  commands: Schema.Number,
});

export type RouterSnapshot = typeof RouterSchema.Type;
export type RuntimeService = typeof RuntimeServiceSchema.Type;
export type RuntimeStatus = typeof RuntimeStatusSchema.Type;
export type RuntimeInfo = typeof RuntimeSchema.Type;
export type RuntimeConfig = typeof RuntimeConfigSchema.Type;
export type ProxyNodes = typeof ProxyNodesSchema.Type;
export type ProxyCapture = typeof ProxyCaptureSchema.Type;
export type IPv6Policy = "follow" | "direct" | "block";
export type RuntimeArtifactInput = {
  url: string;
  sha256: string;
  compression: "none" | "gzip";
  version: string;
};
export type RuntimeAcquireInput = {
  service: RuntimeService;
  artifact: RuntimeArtifactInput;
};
export type RuntimeConfigureInput = {
  service: RuntimeService;
  config: string;
  generation: number;
};
export type ProxyImportInput =
  | { url: string; content?: never }
  | { content: string; url?: never };
export type DatapathMode = "routed-tun";
export type RoutedTUNConfig = {
  interfaceName: string;
  address: string;
};
export type ProxySelectInput = {
  nodeId: string;
  datapath?: DatapathMode;
  routedTUN?: RoutedTUNConfig;
  acknowledgedRevision?: string;
  ipv6: IPv6Policy;
  failure: "direct";
  ports: { mixed: number; tproxy: number; dns: number };
};
export type ProxyCaptureInput =
  | {
      scope: "gateway";
      ipv6: "direct";
      devices?: never;
      clientIPv4?: never;
      clientIPv6?: never;
      lanIPv4Prefixes?: never;
    }
  | ((
      | { devices: readonly { mac: string }[]; clientIPv4?: never }
      | { clientIPv4: string; devices?: never }
    ) & {
      scope?: "devices";
      clientIPv6?: string;
      ipv6: IPv6Policy;
    });
