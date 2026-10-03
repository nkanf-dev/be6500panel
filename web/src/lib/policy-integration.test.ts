import { afterEach, describe, expect, it, vi } from "vitest";
import { api, runRequest } from "./api";
import {
  jsonResponse,
  proxyNodes,
  runtimeStatus,
} from "../modules/production-fixtures.test-data";
const policySummary = {
  total: 3,
  supported: 2,
  omitted: 1,
  revision: "policy-test",
  reasons: [
    { code: "unsupported_rule", count: 1, message: "进程规则无法在网关编译" },
  ],
  omittedRules: [
    { index: 1, code: "unsupported_rule", message: "PROCESS-NAME" },
  ],
};
afterEach(() => vi.unstubAllGlobals());
describe("shared proxy policy contract", () => {
  it("preserves parsed policy summary from nodes and private import responses", async () => {
    vi.stubGlobal(
      "fetch",
      vi
        .fn()
        .mockImplementation(() =>
          Promise.resolve(jsonResponse({ ...proxyNodes, policySummary })),
        ),
    );
    await expect(runRequest(api.proxyNodes())).resolves.toMatchObject({
      policySummary,
    });
    await expect(
      runRequest(api.proxyImport({ content: "proxies: []" })),
    ).resolves.toMatchObject({ policySummary });
  });
  it("accepts old nodes responses but rejects malformed optional summaries", async () => {
    const fetch = vi.fn().mockResolvedValue(jsonResponse(proxyNodes));
    vi.stubGlobal("fetch", fetch);
    await expect(runRequest(api.proxyNodes())).resolves.toEqual(proxyNodes);
    fetch.mockResolvedValue(
      jsonResponse({
        ...proxyNodes,
        policySummary: { ...policySummary, omitted: "1" },
      }),
    );
    await expect(runRequest(api.proxyNodes())).rejects.toMatchObject({
      code: "invalid_response",
    });
  });
  it("sends only an explicit acknowledgment with the existing compiler input", async () => {
    const fetch = vi
      .fn()
      .mockResolvedValue(
        jsonResponse({
          status: runtimeStatus,
          configSHA256: "a".repeat(64),
          diagnostics: [],
        }),
      );
    vi.stubGlobal("fetch", fetch);
    const body = {
      nodeId: "node-synthetic",
      ipv6: "direct" as const,
      failure: "direct" as const,
      ports: { mixed: 2080, tproxy: 7893, dns: 6450 },
      acknowledgedRevision: policySummary.revision,
    };
    await runRequest(api.proxySelect(body));
    expect(fetch).toHaveBeenCalledOnce();
    expect(fetch).toHaveBeenCalledWith(
      "/api/proxy/select",
      expect.objectContaining({ method: "POST", body: JSON.stringify(body) }),
    );
  });
});
