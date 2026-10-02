import { Schema } from "effect";

export const ConfigurationModuleSchema = Schema.Literal(
  "network",
  "wireless",
  "dhcp",
  "firewall",
  "system",
  "dropbear",
);
export type ConfigurationModule = typeof ConfigurationModuleSchema.Type;
export const configurationModules: readonly {
  module: ConfigurationModule;
  label: string;
  description: string;
}[] = [
  { module: "network", label: "网络", description: "接口、设备与路由" },
  { module: "wireless", label: "无线", description: "射频、SSID 与接入" },
  { module: "dhcp", label: "DHCP / DNS", description: "地址分配与解析" },
  { module: "firewall", label: "防火墙", description: "区域、转发与规则" },
  { module: "system", label: "系统", description: "主机、时间与服务" },
  { module: "dropbear", label: "SSH", description: "管理访问与认证" },
];
export const DiagnosticSchema = Schema.Struct({
  code: Schema.String,
  message: Schema.String,
});
export const DraftSchema = Schema.Struct({
  id: Schema.String,
  module: ConfigurationModuleSchema,
  generation: Schema.Number,
  diff: Schema.String,
  risks: Schema.Array(DiagnosticSchema),
  valid: Schema.Boolean,
  errors: Schema.Array(DiagnosticSchema),
  createdAt: Schema.String,
});
export const PendingCommitSchema = Schema.Struct({
  id: Schema.String,
  deadline: Schema.String,
});
export const ConfigurationSchema = Schema.Struct({
  generation: Schema.Number,
  documents: Schema.Array(
    Schema.Struct({
      module: ConfigurationModuleSchema,
      content: Schema.String,
    }),
  ),
  pendingCommit: Schema.optional(PendingCommitSchema),
});
export const DraftsSchema = Schema.Struct({
  drafts: Schema.Array(DraftSchema),
});
export const ConfigurationStatusSchema = Schema.Struct({
  enabled: Schema.Boolean,
  generation: Schema.Number,
  pendingCommit: Schema.optional(PendingCommitSchema),
});
export const CommitSchema = Schema.Struct({
  id: Schema.String,
  state: Schema.Literal("committed", "pending_confirmation", "rolled_back"),
  generation: Schema.Number,
  deadline: Schema.optional(Schema.String),
  changedModules: Schema.Array(ConfigurationModuleSchema),
  captureDisabled: Schema.optional(Schema.Boolean),
  warning: Schema.optional(Schema.String),
});
export type ConfigurationSnapshot = typeof ConfigurationSchema.Type;
export type ConfigurationDraft = typeof DraftSchema.Type;
export type ConfigurationStatus = typeof ConfigurationStatusSchema.Type;
export type ConfigurationCommit = typeof CommitSchema.Type;
export type PendingCommit = typeof PendingCommitSchema.Type;
export type Diagnostic = typeof DiagnosticSchema.Type;
