import { Schema } from "effect";
import { request } from "../../lib/api";
import { RuntimeStatusSchema } from "../../lib/contracts";
import { ProxyPolicySummarySchema } from "./policy-contracts";

const Text = Schema.String.pipe(Schema.maxLength(1024));
const Revision = Schema.String.pipe(Schema.maxLength(128));
const Integer = Schema.Number.pipe(Schema.int());
const Generation = Integer.pipe(Schema.nonNegative());
export const RuleKindSchema = Schema.Literal(
  "domain",
  "domain-suffix",
  "domain-keyword",
  "ip-cidr",
  "rule-set",
  "match",
);
export const RuleTargetSchema = Schema.Literal("direct", "proxy", "block");
export const LocalRuleSchema = Schema.Struct({
  id: Schema.String.pipe(Schema.maxLength(64)),
  enabled: Schema.Boolean,
  label: Schema.String.pipe(Schema.maxLength(256)),
  note: Schema.String.pipe(Schema.maxLength(1024)),
  rule: Schema.Struct({
    kind: RuleKindSchema,
    value: Schema.optional(Schema.String.pipe(Schema.maxLength(253))),
    target: RuleTargetSchema,
    noResolve: Schema.optional(Schema.Boolean),
    index: Integer,
  }),
});
export const RuleSchema = LocalRuleSchema.fields.rule;
export const SubscriptionEditSchema = Schema.Struct({
  id: Schema.String.pipe(Schema.maxLength(64)),
  sourceFingerprint: Revision,
  disabled: Schema.Boolean,
  replacement: Schema.optional(RuleSchema),
  label: Schema.String.pipe(Schema.maxLength(256)),
  note: Schema.String.pipe(Schema.maxLength(1024)),
});
export const LocalPolicySchema = Schema.Struct({
  rules: Schema.Array(LocalRuleSchema).pipe(Schema.maxItems(512)),
  subscriptionEdits: Schema.Array(SubscriptionEditSchema).pipe(
    Schema.maxItems(1024),
  ),
});
export const LocalRulesPreviewSchema = Schema.Struct({
  rules: Schema.Array(RuleSchema).pipe(Schema.maxItems(8704)),
  provenance: Schema.Array(
    Schema.Struct({
      effectiveIndex: Integer,
      layer: Schema.Literal("local", "subscription"),
      stableId: Revision,
      label: Text,
      sourceFingerprint: Schema.optional(Revision),
      sourceIndex: Integer,
      sourceOrdinal: Integer,
      kind: RuleKindSchema,
      value: Schema.optional(Text),
      target: RuleTargetSchema,
    }),
  ).pipe(Schema.maxItems(8704)),
  diagnostics: Schema.Array(
    Schema.Struct({ scope: Text, index: Integer, code: Text, message: Text }),
  ).pipe(Schema.maxItems(20000)),
});
export const LocalRulesStateSchema = Schema.Struct({
  draft: Schema.Struct({ policy: LocalPolicySchema, revision: Revision }),
  subscriptionRevision: Revision,
  subscriptionRules: Schema.Array(
    Schema.Struct({ fingerprint: Revision, rule: RuleSchema }),
  ).pipe(Schema.maxItems(8192)),
  preview: LocalRulesPreviewSchema,
  applied: Schema.Struct({
    state: Schema.Literal("known", "unknown", "none"),
    revision: Schema.optional(Revision),
    generation: Schema.optional(Generation),
  }),
  runtimeGeneration: Generation,
  policySummary: Schema.optional(ProxyPolicySummarySchema),
});
export const LocalRulesApplySchema = Schema.Struct({
  status: RuntimeStatusSchema,
  draftRevision: Revision,
  configSHA256: Schema.String.pipe(Schema.maxLength(64)),
  applied: Schema.Literal(true),
});
export type Rule = typeof RuleSchema.Type;
export type LocalRule = typeof LocalRuleSchema.Type;
export type SubscriptionEdit = typeof SubscriptionEditSchema.Type;
export type LocalPolicy = typeof LocalPolicySchema.Type;
export type LocalRulesPreview = typeof LocalRulesPreviewSchema.Type;
export type LocalRulesState = typeof LocalRulesStateSchema.Type;
export type LocalRulesApplyInput = {
  revision: string;
  generation: number;
  acknowledgedRevision?: string;
};

/** Public policy only. No native config, YAML, node credentials or positional edit references. */
export const localRulesApi = {
  state: () => request("/proxy/local-rules", LocalRulesStateSchema),
  save: (policy: LocalPolicy) =>
    request("/proxy/local-rules", LocalRulesStateSchema, {
      method: "POST",
      body: { policy },
    }),
  preview: (policy: LocalPolicy) =>
    request("/proxy/local-rules/preview", LocalRulesPreviewSchema, {
      method: "POST",
      body: { policy },
    }),
  apply: (body: LocalRulesApplyInput) =>
    request("/proxy/local-rules/apply", LocalRulesApplySchema, {
      method: "POST",
      body,
      timeoutMs: 90_000,
    }),
};
