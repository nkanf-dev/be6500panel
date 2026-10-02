import { Schema } from "effect";

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
