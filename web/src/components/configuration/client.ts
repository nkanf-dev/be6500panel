import { Schema } from "effect";
import { request } from "../../lib/api";
import {
  CommitSchema,
  ConfigurationSchema,
  ConfigurationStatusSchema,
  DraftSchema,
  DraftsSchema,
  type ConfigurationModule,
} from "./contracts";

export const configurationApi = {
  status: () => request("/configuration/status", ConfigurationStatusSchema),
  read: () => request("/configuration", ConfigurationSchema),
  drafts: () => request("/configuration/drafts", DraftsSchema),
  stage: (body: {
    module: ConfigurationModule;
    content: string;
    generation: number;
  }) => request("/configuration/stage", DraftSchema, { method: "POST", body }),
  remove: (id: string) =>
    // The API acknowledges deletion without returning private document content.
    request(
      `/configuration/drafts?id=${encodeURIComponent(id)}`,
      Schema.Unknown,
      { method: "DELETE" },
    ),
  commit: (body: {
    draftIds: readonly string[];
    generation: number;
    acknowledgeRisks: boolean;
  }) =>
    request("/configuration/commit", CommitSchema, {
      method: "POST",
      body,
      timeoutMs: 90_000,
    }),
  confirm: (id: string) =>
    request("/configuration/confirm", CommitSchema, {
      method: "POST",
      body: { id },
      timeoutMs: 90_000,
    }),
  rollback: (id: string) =>
    request("/configuration/rollback", CommitSchema, {
      method: "POST",
      body: { id },
      timeoutMs: 90_000,
    }),
};
