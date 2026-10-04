import { describe, expect, it, vi, afterEach } from "vitest";
import { runRequest } from "../../lib/api";
import { localRulesApi, type LocalPolicy } from "./local-rules-api";

const policy: LocalPolicy = {
  rules: [
    {
      id: "local-one",
      enabled: true,
      label: "用户要求",
      note: "",
      rule: {
        kind: "domain",
        value: "gpt.kanglives.top",
        target: "direct",
        index: 0,
      },
    },
  ],
  subscriptionEdits: [
    {
      id: "edit-one",
      sourceFingerprint: `${"a".repeat(64)}:1`,
      disabled: true,
      label: "",
      note: "",
    },
  ],
};
const state = {
  draft: { policy, revision: "saved-revision" },
  subscriptionRevision: "subscription-revision",
  subscriptionRules: [],
  preview: { rules: [], provenance: [], diagnostics: [] },
  applied: { state: "unknown" },
  runtimeGeneration: 9,
};
afterEach(() => vi.unstubAllGlobals());
function response(json: unknown, status = 200) {
  return new Response(JSON.stringify(json), {
    status,
    headers: { "Content-Type": "application/json" },
  });
}

describe("local rules public API", () => {
  it("decodes all public provenance layers and keeps orphan diagnostics intact", async () => {
    const rules = [
      policy.rules[0].rule,
      { kind: "match", target: "block", index: 7 },
    ];
    const preview = {
      rules,
      provenance: [
        {
          effectiveIndex: 0,
          layer: "local",
          stableId: "local-one",
          label: "用户要求",
          sourceIndex: -1,
          sourceOrdinal: 0,
          kind: "domain",
          value: "gpt.kanglives.top",
          target: "direct",
        },
        {
          effectiveIndex: 1,
          layer: "subscription",
          stableId: "edit-two",
          label: "改写",
          sourceFingerprint: `${"b".repeat(64)}:1`,
          sourceIndex: 7,
          sourceOrdinal: 1,
          kind: "match",
          target: "block",
        },
      ],
      diagnostics: [
        {
          scope: "subscription-edit",
          index: 1,
          code: "orphaned-edit",
          message: "subscription edit reference is absent and remains inactive",
        },
      ],
    };
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue(response({ ...state, preview })),
    );
    expect((await runRequest(localRulesApi.state())).preview).toEqual(preview);
  });
  it("does not accept native reject aliases or null policy arrays", async () => {
    const fetcher = vi
      .fn()
      .mockResolvedValueOnce(
        response({
          ...state,
          draft: { ...state.draft, policy: { ...policy, rules: null } },
        }),
      )
      .mockResolvedValueOnce(
        response({
          ...state,
          draft: {
            ...state.draft,
            policy: {
              ...policy,
              rules: [
                {
                  ...policy.rules[0],
                  rule: { ...policy.rules[0].rule, target: "reject" },
                },
              ],
            },
          },
        }),
      );
    vi.stubGlobal("fetch", fetcher);
    await expect(runRequest(localRulesApi.state())).rejects.toMatchObject({
      code: "invalid_response",
    });
    await expect(runRequest(localRulesApi.state())).rejects.toMatchObject({
      code: "invalid_response",
    });
  });

  it("loads public state with GET and decodes optional omission review", async () => {
    const fetcher = vi.fn().mockResolvedValue(
      response({
        ...state,
        policySummary: {
          total: 2,
          supported: 1,
          omitted: 1,
          reasons: [],
          omittedRules: [],
          revision: "review-token",
        },
      }),
    );
    vi.stubGlobal("fetch", fetcher);
    const value = await runRequest(localRulesApi.state());
    expect(value.draft.policy).toEqual(policy);
    expect(value.policySummary?.revision).toBe("review-token");
    expect(fetcher).toHaveBeenCalledExactlyOnceWith(
      "/api/proxy/local-rules",
      expect.objectContaining({ method: "GET", credentials: "same-origin" }),
    );
  });
  it("save and preview send policy only and never call apply", async () => {
    const fetcher = vi
      .fn()
      .mockResolvedValueOnce(response(state))
      .mockResolvedValueOnce(response(state.preview));
    vi.stubGlobal("fetch", fetcher);
    await runRequest(localRulesApi.save(policy));
    await runRequest(localRulesApi.preview(policy));
    expect(fetcher.mock.calls.map(([url]) => url)).toEqual([
      "/api/proxy/local-rules",
      "/api/proxy/local-rules/preview",
    ]);
    for (const [, options] of fetcher.mock.calls) {
      expect(options.method).toBe("POST");
      expect(JSON.parse(options.body)).toEqual({ policy });
    }
  });
  it("apply sends the saved revision, runtime generation and explicit review token", async () => {
    const fetcher = vi.fn().mockResolvedValue(
      response({
        status: {
          service: "sing-box",
          state: "running",
          generation: 10,
          configured: true,
          artifactAvailable: true,
          rssBytes: 0,
          rssAvailable: false,
          desired: true,
          restarts: 0,
        },
        draftRevision: "saved-revision",
        configSHA256: "b".repeat(64),
        applied: true,
      }),
    );
    vi.stubGlobal("fetch", fetcher);
    await runRequest(
      localRulesApi.apply({
        revision: "saved-revision",
        generation: 9,
        acknowledgedRevision: "review-token",
      }),
    );
    expect(fetcher).toHaveBeenCalledExactlyOnceWith(
      "/api/proxy/local-rules/apply",
      expect.objectContaining({
        method: "POST",
        body: JSON.stringify({
          revision: "saved-revision",
          generation: 9,
          acknowledgedRevision: "review-token",
        }),
      }),
    );
  });
  it("rejects unbounded or malformed public state", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue(
        response({
          ...state,
          draft: {
            ...state.draft,
            policy: {
              ...policy,
              rules: Array.from({ length: 513 }, () => policy.rules[0]),
            },
          },
        }),
      ),
    );
    await expect(runRequest(localRulesApi.state())).rejects.toMatchObject({
      code: "invalid_response",
    });
  });
  it("returns fixed API failures without retrying POST", async () => {
    const fetcher = vi
      .fn()
      .mockResolvedValue(
        response(
          { error: { code: "draft_conflict", message: "规则草稿已变化" } },
          409,
        ),
      );
    vi.stubGlobal("fetch", fetcher);
    await expect(runRequest(localRulesApi.save(policy))).rejects.toMatchObject({
      code: "draft_conflict",
    });
    expect(fetcher).toHaveBeenCalledTimes(1);
  });
});
