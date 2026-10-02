import { afterEach, describe, expect, it, vi } from "vitest";
import { Schema } from "effect";
import {
  routerSnapshot,
  runtimeStatus,
  jsonResponse,
  proxyNodes,
} from "../modules/production-fixtures.test-data";
import { api, ApiError, request, runRequest } from "./api";
const health = { status: "ok", mode: "demo", readOnly: true };
const respond = (body: unknown, status = 200) =>
  new Response(JSON.stringify(body), {
    status,
    headers: { "Content-Type": "application/json" },
  });
afterEach(() => {
  vi.unstubAllGlobals();
  vi.useRealTimers();
});
describe("typed Effect HTTP boundary", () => {
  it("decodes real JSON and sends same-origin credentials", async () => {
    const fetch = vi.fn().mockResolvedValue(respond(health));
    vi.stubGlobal("fetch", fetch);
    await expect(runRequest(api.health())).resolves.toEqual(health);
    expect(fetch).toHaveBeenCalledWith(
      "/api/health",
      expect.objectContaining({
        method: "GET",
        credentials: "same-origin",
        signal: expect.any(AbortSignal),
      }),
    );
  });
  it("rejects malformed success payloads", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue(respond({ mode: "demo" })),
    );
    await expect(runRequest(api.health())).rejects.toMatchObject({
      code: "invalid_response",
    });
  });
  it("keeps backend error codes and does not retry unsupported observation", async () => {
    const fetch = vi
      .fn()
      .mockResolvedValue(
        respond(
          { error: { code: "observation_unavailable", message: "未接入" } },
          503,
        ),
      );
    vi.stubGlobal("fetch", fetch);
    await expect(runRequest(api.system())).rejects.toMatchObject({
      code: "observation_unavailable",
      status: 503,
    });
    expect(fetch).toHaveBeenCalledTimes(1);
  });
  it("retries transient reads at most twice", async () => {
    vi.useFakeTimers();
    const fetch = vi.fn().mockRejectedValue(new TypeError("offline"));
    vi.stubGlobal("fetch", fetch);
    const result = runRequest(api.health()).catch((error: unknown) => error);
    await vi.runAllTimersAsync();
    expect(await result).toBeInstanceOf(ApiError);
    expect(fetch).toHaveBeenCalledTimes(3);
  });
  it("posts the exact credential-free plan without automatic retry", async () => {
    const fetch = vi
      .fn()
      .mockResolvedValue(
        respond(
          { error: { code: "invalid_input", message: "invalid nodes" } },
          400,
        ),
      );
    vi.stubGlobal("fetch", fetch);
    const input = {
      mode: "split",
      dnsStrategy: "split",
      ipv6Policy: "follow",
      failurePolicy: "block-proxy",
      nodeCount: 2,
    } as const;
    await expect(runRequest(api.proxyPlan(input))).rejects.toMatchObject({
      code: "invalid_input",
    });
    expect(fetch).toHaveBeenCalledTimes(1);
    expect(fetch).toHaveBeenCalledWith(
      "/api/proxy/plan",
      expect.objectContaining({ method: "POST", body: JSON.stringify(input) }),
    );
  });
  it("expires the session on unauthorized responses", async () => {
    const listener = vi.fn();
    window.addEventListener("be6500panel:unauthorized", listener);
    vi.stubGlobal(
      "fetch",
      vi
        .fn()
        .mockResolvedValue(
          respond(
            { error: { code: "unauthorized", message: "登录已过期" } },
            401,
          ),
        ),
    );
    await expect(runRequest(api.network())).rejects.toMatchObject({
      code: "unauthorized",
    });
    expect(listener).toHaveBeenCalledOnce();
    window.removeEventListener("be6500panel:unauthorized", listener);
  });
  it("supports caller cancellation", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(
        (_url, init: RequestInit) =>
          new Promise((_resolve, reject) => {
            init.signal?.addEventListener("abort", () =>
              reject(new DOMException("aborted", "AbortError")),
            );
          }),
      ),
    );
    const controller = new AbortController();
    const pending = runRequest(api.health(), controller.signal);
    const result = pending.catch((error: unknown) => error);
    controller.abort();
    expect(await result).toBeDefined();
  });
});

describe("production contract decoding", () => {
  it("decodes router nullable expiry, numeric channel, boolean runtime desired, public uTLS and recovery steps", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn((url: string) =>
        Promise.resolve(
          jsonResponse(
            url === "/api/router"
              ? routerSnapshot
              : url === "/api/runtime"
                ? {
                    enabled: true,
                    services: [{ ...runtimeStatus, recoveryPlan: ["acquire"] }],
                  }
                : proxyNodes,
          ),
        ),
      ),
    );
    await expect(runRequest(api.router())).resolves.toMatchObject({
      wifi: [{ channel: 36 }],
      devices: [{ expiresAt: null }],
    });
    await expect(runRequest(api.runtime())).resolves.toMatchObject({
      services: [{ desired: false, recoveryPlan: ["acquire"] }],
    });
    await expect(runRequest(api.proxyNodes())).resolves.toMatchObject({
      nodes: [{ utls: true }],
    });
  });
  it("rejects wrong production field types instead of weakening schema", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue(
        jsonResponse({
          ...routerSnapshot,
          wifi: [{ ...routerSnapshot.wifi[0], channel: "36" }],
        }),
      ),
    );
    await expect(runRequest(api.router())).rejects.toMatchObject({
      code: "invalid_response",
    });
  });
  it("uses bounded operation timeout options and never retries DELETE", async () => {
    const timeout = vi.spyOn(AbortSignal, "timeout");
    const fetch = vi
      .fn()
      .mockImplementation(() => Promise.resolve(jsonResponse(runtimeStatus)));
    vi.stubGlobal("fetch", fetch);
    await runRequest(
      api.runtimeAcquire({
        service: "sing-box",
        artifact: {
          url: "https://example.test/core.gz",
          sha256: "a".repeat(64),
          compression: "gzip",
          version: "test",
        },
      }),
    );
    expect(timeout).toHaveBeenLastCalledWith(420_000);
    await runRequest(
      api.runtimeConfigure({
        service: "sing-box",
        config: "{}",
        generation: 4,
      }),
    );
    expect(timeout).toHaveBeenLastCalledWith(90_000);
    fetch.mockResolvedValue(
      jsonResponse(
        { error: { code: "draft_not_found", message: "missing" } },
        404,
      ),
    );
    await expect(
      runRequest(
        request("/configuration/drafts?id=test", Schema.Unknown, {
          method: "DELETE",
        }),
      ),
    ).rejects.toMatchObject({ code: "draft_not_found" });
    expect(fetch).toHaveBeenCalledTimes(3);
    timeout.mockRestore();
  });
});
