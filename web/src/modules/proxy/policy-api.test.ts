import { Effect, Schema } from "effect";
import { afterEach, describe, expect, it, vi } from "vitest";
import { api, runRequest } from "../../lib/api";
import type { ProxySelectInput } from "../../lib/contracts";
import { jsonResponse, runtimeStatus } from "../production-fixtures.test-data";
import { selectProxyPolicy } from "./policy-api";
import { ProxyPolicySummarySchema } from "./policy-contracts";

const input: ProxySelectInput = {
  nodeId: "test",
  ipv6: "direct",
  failure: "direct",
  ports: { mixed: 2080, tproxy: 7893, dns: 6450 },
};
const result = {
  status: runtimeStatus,
  configSHA256: "b".repeat(64),
  diagnostics: [],
};
afterEach(() => vi.unstubAllGlobals());

describe("proxy policy API boundary", () => {
  it("delegates to api.proxySelect so existing spies and runtime request behavior remain intact", async () => {
    const spy = vi
      .spyOn(api, "proxySelect")
      .mockReturnValue(Effect.succeed(result));
    await runRequest(
      selectProxyPolicy({ ...input, acknowledgedRevision: "a".repeat(64) }),
    );
    expect(spy).toHaveBeenCalledExactlyOnceWith({
      ...input,
      acknowledgedRevision: "a".repeat(64),
    });
  });

  it.each([undefined, "a".repeat(64)])(
    "sends optional acknowledgment %s only when provided",
    async (acknowledgedRevision) => {
      const fetch = vi.fn(() => Promise.resolve(jsonResponse(result)));
      vi.stubGlobal("fetch", fetch);
      const body = {
        ...input,
        ...(acknowledgedRevision ? { acknowledgedRevision } : {}),
      };
      const effect = selectProxyPolicy(body);
      expect(fetch).not.toHaveBeenCalled();
      await runRequest(effect);
      expect(fetch).toHaveBeenCalledExactlyOnceWith(
        "/api/proxy/select",
        expect.objectContaining({ method: "POST", body: JSON.stringify(body) }),
      );
    },
  );

  it("decodes policy-only data including zero-based omissions without node credentials", () => {
    const policy = {
      total: 2,
      supported: 1,
      omitted: 1,
      reasons: [
        {
          code: "unsupported-process-rule",
          count: 1,
          message: "gateway cannot classify client processes",
        },
      ],
      omittedRules: [
        {
          index: 0,
          code: "unsupported-process-rule",
          message: "gateway cannot classify client processes",
        },
      ],
      revision: "a".repeat(64),
    };
    expect(
      Schema.decodeUnknownSync(ProxyPolicySummarySchema)({
        ...policy,
        credentials: "not part of policy",
      }),
    ).toEqual(policy);
    expect(() =>
      Schema.decodeUnknownSync(ProxyPolicySummarySchema)({
        ...policy,
        omittedRules: undefined,
      }),
    ).toThrow();
  });
});
