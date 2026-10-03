import { Schema } from "effect";
import {
  ConfigurationModuleSchema,
  DiagnosticSchema,
  DraftSchema,
  configurationModules,
  type ConfigurationModule,
} from "../configuration/contracts";

export const MAX_BACKUP_BYTES = 2 * 1024 * 1024;
export const runtimeBackupScopes = [
  "runtime.frpc",
  "runtime.sing-box",
] as const;
export const BackupScopeSchema = Schema.Union(
  ConfigurationModuleSchema,
  Schema.Literal(...runtimeBackupScopes),
);
export type BackupScope = typeof BackupScopeSchema.Type;
export const backupScopes: readonly {
  module: BackupScope;
  label: string;
  description: string;
  runtime?: boolean;
}[] = [
  ...configurationModules,
  {
    module: "runtime.frpc",
    label: "FRPC 运行配置",
    description: "本次仅预览，运行配置恢复需在对应服务中确认",
    runtime: true,
  },
  {
    module: "runtime.sing-box",
    label: "sing-box 运行配置",
    description: "本次仅预览，运行配置恢复需在对应服务中确认",
    runtime: true,
  },
];
export function isNativeModule(module: string): module is ConfigurationModule {
  return configurationModules.some((item) => item.module === module);
}
export function backupModuleLabel(module: string) {
  return backupScopes.find((item) => item.module === module)?.label ?? module;
}

export const BackupEnvelopeSchema = Schema.Struct({
  model: Schema.String,
  build: Schema.String,
  createdAt: Schema.String,
  generation: Schema.Number,
  scopes: Schema.Array(BackupScopeSchema),
  documents: Schema.Array(
    Schema.Struct({
      module: BackupScopeSchema,
      content: Schema.String,
      digest: Schema.String,
      generation: Schema.optional(Schema.Number),
    }),
  ),
});
export const ImportChangeSchema = Schema.Struct({
  module: BackupScopeSchema,
  kind: Schema.Literal(
    "added",
    "modified",
    "deleted",
    "unchanged",
    "uncompared",
  ),
  beforeBytes: Schema.Number,
  afterBytes: Schema.Number,
  beforeDigest: Schema.String,
  afterDigest: Schema.String,
  diff: Schema.String,
  stageable: Schema.Boolean,
  valid: Schema.Boolean,
  errors: Schema.Array(DiagnosticSchema),
  dependencies: Schema.Array(DiagnosticSchema),
  risks: Schema.Array(DiagnosticSchema),
});
export const ImportPreviewSchema = Schema.Struct({
  id: Schema.String,
  generation: Schema.Number,
  sourceModel: Schema.String,
  currentModel: Schema.String,
  modelMismatch: Schema.Boolean,
  expiresAt: Schema.String,
  summary: Schema.Struct({
    added: Schema.Number,
    modified: Schema.Number,
    deleted: Schema.Number,
    unchanged: Schema.Number,
    uncompared: Schema.Number,
  }),
  changes: Schema.Array(ImportChangeSchema),
  warnings: Schema.Array(DiagnosticSchema),
});
export const ImportStageSchema = Schema.Struct({
  generation: Schema.Number,
  drafts: Schema.Array(DraftSchema),
  warnings: Schema.Array(DiagnosticSchema),
});
export type BackupEnvelope = typeof BackupEnvelopeSchema.Type;
export type ImportChange = typeof ImportChangeSchema.Type;
export type ImportPreview = typeof ImportPreviewSchema.Type;
export type ImportStage = typeof ImportStageSchema.Type;
export interface ImportStageInput {
  previewId: string;
  generation: number;
  modules: readonly ConfigurationModule[];
  acknowledgeModelMismatch: boolean;
}
export function canStageChange(change: ImportChange) {
  return (
    isNativeModule(change.module) &&
    change.kind !== "unchanged" &&
    change.kind !== "uncompared" &&
    change.stageable &&
    change.valid
  );
}
