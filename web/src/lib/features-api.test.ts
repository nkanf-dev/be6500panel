import { describe, expect, it, vi, afterEach } from "vitest";
import { Schema } from "effect";
import {
  FeatureCatalogSchema,
  FeatureStateSchema,
  FeatureOperationEnvelopeSchema,
  featuresApi,
  formatReconnectUrl,
  pollOperation,
} from "./features-api";
import { runRequest } from "./api";

const catalogFixture = {
  generation: 42,
  domains: [
    {
      id: "network",
      title: "网络配置",
      reads: [
        {
          id: "wan",
          title: "WAN 设置",
          fields: [
            { key: "proto", label: "接入协议", kind: "select", required: true, options: ["dhcp", "pppoe", "static"] },
            { key: "username", label: "账号", kind: "text", required: false },
            { key: "password", label: "密码", kind: "secret", required: false },
          ],
        },
      ],
      actions: [
        {
          id: "set_wan",
          title: "保存 WAN 设置",
          fields: [
            { key: "proto", label: "接入协议", kind: "select", required: true, options: ["dhcp", "pppoe", "static"] },
            { key: "username", label: "账号", kind: "text", required: false },
            { key: "password", label: "密码", kind: "secret", required: false },
          ],
          impact: "network",
          readback: "wan",
        },
      ],
    },
  ],
};

const stateFixture = {
  available: true,
  readId: "wan",
  sampledAt: "2026-10-05T08:00:00Z",
  generation: 42,
  data: {
    proto: "pppoe",
    username: "user123",
    passwordConfigured: true,
  },
};

afterEach(() => vi.restoreAllMocks());

describe("features-api contracts", () => {
  it("decodes a valid catalog schema", () => {
    const decoded = Schema.decodeUnknownSync(FeatureCatalogSchema)(catalogFixture);
    expect(decoded.generation).toBe(42);
    expect(decoded.domains).toHaveLength(1);
    expect(decoded.domains[0].reads[0].fields).toHaveLength(3);
    expect(decoded.domains[0].actions[0].impact).toBe("network");
  });

  it("decodes a valid state schema with masked secrets", () => {
    const decoded = Schema.decodeUnknownSync(FeatureStateSchema)(stateFixture);
    expect(decoded.available).toBe(true);
    expect(decoded.readId).toBe("wan");
    expect(decoded.generation).toBe(42);
    expect(decoded.data.passwordConfigured).toBe(true);
  });

  it("encodes query parameters properly in featuresApi.state", async () => {
    const mockFetch = vi.fn().mockResolvedValue(
      new Response(JSON.stringify(stateFixture), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      }),
    );
    vi.stubGlobal("fetch", mockFetch);

    await runRequest(featuresApi.state("network", "wan", { ifname: "eth0", enabled: true }));
    expect(mockFetch).toHaveBeenCalledWith(
      expect.stringContaining("/api/features/network/state?read=wan&ifname=eth0&enabled=true"),
      expect.anything(),
    );
  });

  it("sends CAS generation and acknowledgeImpact in featuresApi.apply", async () => {
    const applyFixture = {
      operation: {
        id: "op-101",
        state: "completed",
        actionId: "set_wan",
        domain: "network",
        generation: 43,
      },
    };
    const mockFetch = vi.fn().mockResolvedValue(
      new Response(JSON.stringify(applyFixture), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      }),
    );
    vi.stubGlobal("fetch", mockFetch);

    await runRequest(
      featuresApi.apply("network", {
        actionId: "set_wan",
        input: { proto: "dhcp" },
        generation: 42,
        acknowledgeImpact: true,
      }),
    );

    const call = mockFetch.mock.calls[0];
    expect(call[0]).toBe("/api/features/network/apply");
    const body = JSON.parse(call[1].body);
    expect(body.generation).toBe(42);
    expect(body.acknowledgeImpact).toBe(true);
    expect(body.input.proto).toBe("dhcp");
  });

  it("polls pending operation until completed", async () => {
    let callCount = 0;
    const mockFetch = vi.fn().mockImplementation(() => {
      callCount++;
      const state = callCount >= 2 ? "completed" : "pending";
      return Promise.resolve(
        new Response(
          JSON.stringify({
            operation: {
              id: "op-999",
              state,
              actionId: "reboot",
              domain: "services",
              generation: 10,
            },
          }),
          { status: 200, headers: { "Content-Type": "application/json" } },
        ),
      );
    });
    vi.stubGlobal("fetch", mockFetch);

    const result = await pollOperation("op-999", { intervalMs: 10, timeoutMs: 1000 });
    expect(result.state).toBe("completed");
    expect(callCount).toBe(2);
  });

  it("decodes operation with canConfirm and reconnectAddress", () => {
    const opFixture = {
      operation: {
        id: "op-lan-1",
        state: "pending",
        actionId: "set_lan",
        domain: "network",
        generation: 50,
        canConfirm: true,
        reconnectAddress: "192.168.10.1",
        waitingFor: "等待新地址连通确认",
      },
    };
    const decoded = Schema.decodeUnknownSync(FeatureOperationEnvelopeSchema)(opFixture);
    expect(decoded.operation.canConfirm).toBe(true);
    expect(decoded.operation.reconnectAddress).toBe("192.168.10.1");
  });

  it("calls featuresApi.confirm with id", async () => {
    const confirmResponse = {
      operation: {
        id: "op-lan-1",
        state: "completed",
        actionId: "set_lan",
        domain: "network",
        generation: 51,
      },
    };
    const mockFetch = vi.fn().mockResolvedValue(
      new Response(JSON.stringify(confirmResponse), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      }),
    );
    vi.stubGlobal("fetch", mockFetch);

    const result = await runRequest(featuresApi.confirm("op-lan-1"));
    expect(result.state).toBe("completed");
    expect(mockFetch).toHaveBeenCalledWith(
      "/api/features/confirm",
      expect.objectContaining({
        method: "POST",
        body: JSON.stringify({ id: "op-lan-1" }),
      }),
    );
  });

  it("returns early from pollOperation if canConfirm is true", async () => {
    const mockFetch = vi.fn().mockResolvedValue(
      new Response(
        JSON.stringify({
          operation: {
            id: "op-lan-2",
            state: "pending",
            actionId: "set_lan",
            domain: "network",
            generation: 52,
            canConfirm: true,
            reconnectAddress: "192.168.5.1",
          },
        }),
        { status: 200, headers: { "Content-Type": "application/json" } },
      ),
    );
    vi.stubGlobal("fetch", mockFetch);

    const result = await pollOperation("op-lan-2", { intervalMs: 10, timeoutMs: 1000 });
    expect(result.canConfirm).toBe(true);
    expect(result.reconnectAddress).toBe("192.168.5.1");
    expect(mockFetch).toHaveBeenCalledTimes(1);
  });

  it("formats bare IP address with current protocol and port in formatReconnectUrl", () => {
    const url = formatReconnectUrl("192.168.50.1");
    expect(url).toContain("192.168.50.1");
    expect(url).toMatch(/^http:\/\//);
  });

  it("re-throws 401 unauthorized error during pollOperation", async () => {
    const mockFetch = vi.fn().mockResolvedValue(
      new Response(
        JSON.stringify({ error: { code: "unauthenticated", message: "Authentication required." } }),
        { status: 401, headers: { "Content-Type": "application/json" } },
      ),
    );
    vi.stubGlobal("fetch", mockFetch);

    await expect(pollOperation("op-unauth", { timeoutMs: 200, intervalMs: 10 })).rejects.toThrow();
  });
});

