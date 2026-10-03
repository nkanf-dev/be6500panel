import { Schema } from "effect";

/** Counts describe parsed rules eligible for compilation, not live routing or hits. */
export const ProxyPolicySummarySchema = Schema.Struct({
  total: Schema.Number,
  supported: Schema.Number,
  omitted: Schema.Number,
  reasons: Schema.Array(
    Schema.Struct({
      code: Schema.String,
      count: Schema.Number,
      message: Schema.String,
    }),
  ),
  omittedRules: Schema.Array(
    Schema.Struct({
      // The API uses zero-based subscription indices; the UI displays index + 1.
      index: Schema.Number,
      code: Schema.String,
      message: Schema.String,
    }),
  ),
  // Backend identity for normalized rules and omission/DNS/group semantics.
  // It excludes node credentials and is never written to browser storage.
  revision: Schema.String,
});

export type ProxyPolicySummary = typeof ProxyPolicySummarySchema.Type;
