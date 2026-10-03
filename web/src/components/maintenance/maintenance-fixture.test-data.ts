import type { ImportChange, ImportPreview, ImportStage } from "./contracts";

export const syntheticChange: ImportChange = {
  module: "network",
  kind: "modified",
  beforeBytes: 30,
  afterBytes: 31,
  beforeDigest: "fake-before-digest",
  afterDigest: "fake-after-digest",
  diff: "--- network\n+++ network\n@@ -1 +1 @@\n- option ipaddr '192.0.2.1'\n+ option ipaddr '192.0.2.2'",
  stageable: true,
  valid: true,
  errors: [],
  dependencies: [
    { code: "fake_dependency", message: "请一并核对相关防火墙草稿" },
  ],
  risks: [{ code: "fake_management_change", message: "管理地址可能改变" }],
};
export const syntheticPreview: ImportPreview = {
  id: "preview-synthetic",
  generation: 7,
  sourceModel: "synthetic-router-a",
  currentModel: "synthetic-router-a",
  modelMismatch: false,
  expiresAt: "2099-01-01T00:00:00Z",
  summary: { added: 0, modified: 1, deleted: 0, unchanged: 0, uncompared: 0 },
  changes: [syntheticChange],
  warnings: [{ code: "fake_backup_warning", message: "备份包含私有测试配置" }],
};
export const syntheticStage: ImportStage = {
  generation: 7,
  drafts: [
    {
      id: "draft-synthetic",
      module: "network",
      generation: 7,
      diff: syntheticChange.diff,
      risks: syntheticChange.risks,
      valid: true,
      errors: [],
      dependencies: syntheticChange.dependencies,
      createdAt: "2026-01-01T00:00:00Z",
    },
  ],
  warnings: [
    { code: "fake_stage_warning", message: "应用前仍需核对依赖与风险" },
  ],
};
export const syntheticRawBackup = `{
  "model":"synthetic-router-a", "build":"test-only", "generation":7,
  "createdAt":"2026-01-01T00:00:00Z", "scopes":["network"],
  "documents":[{"module":"network","content":"fake private test config","digest":"fake-digest"}]
}
`;
export const respond = (body: unknown, status = 200) =>
  new Response(JSON.stringify(body), {
    status,
    headers: { "Content-Type": "application/json" },
  });
