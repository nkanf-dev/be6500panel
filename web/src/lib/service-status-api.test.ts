import { Schema } from "effect";
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  loadServiceStatus,
  runServiceAction,
  ServiceStatusSchema,
} from "./service-status-api";

const fixture = {
  source: "procd · /proc",
  sampledAt: "2026-10-03T10:00:00Z",
  checkedAt: "2026-10-03T10:00:00Z",
  stale: false,
  errors: [],
  services: [
    {
      name: "dnsmasq",
      instance: "main",
      configured: "present",
      registered: "registered",
      processState: "running",
      procdRunning: true,
      reportedPID: 1820,
      pid: 1820,
      executable: "/usr/sbin/dnsmasq",
      uptimeSeconds: 123,
      rssBytes: 2097152,
      startTicks: 5600,
      protected: false,
    },
  ],
};
const response = (body: unknown) =>
  new Response(JSON.stringify(body), {
    status: 200,
    headers: { "Content-Type": "application/json" },
  });
afterEach(() => vi.unstubAllGlobals());

describe("read-only service status API", () => {
  it("decodes verified process observations through an authenticated GET", async () => {
    const fetch = vi.fn().mockResolvedValue(response(fixture));
    vi.stubGlobal("fetch", fetch);
    const status = await loadServiceStatus();
    expect(status.services[0]).toEqual(fixture.services[0]);
    expect(fetch).toHaveBeenCalledWith(
      "/api/system/services",
      expect.objectContaining({ method: "GET", credentials: "same-origin" }),
    );
    expect(fetch.mock.calls[0][1]).not.toHaveProperty("body");
  });

  it("keeps unknown observations, module errors, and a null last-success time", () => {
    const unavailable = {
      ...fixture,
      sampledAt: null,
      stale: true,
      errorCode: "procd_unavailable",
      errors: [{ module: "procd", code: "ubus_failed", message: "无法读取" }],
      services: [
        {
          name: "dropbear-rescue",
          instance: "rescue",
          configured: "unknown",
          registered: "unknown",
          processState: "unknown",
          errorCode: "proc_unavailable",
          protected: true,
        },
      ],
    };
    expect(Schema.decodeUnknownSync(ServiceStatusSchema)(unavailable)).toEqual(
      unavailable,
    );
  });

  it("rejects malformed process states and invalid runtime metrics", () => {
    for (const service of [
      { ...fixture.services[0], configured: true },
      { ...fixture.services[0], registered: "configured" },
      { ...fixture.services[0], processState: "healthy" },
      { ...fixture.services[0], pid: 0 },
      { ...fixture.services[0], reportedPID: 1.5 },
      { ...fixture.services[0], uptimeSeconds: -1 },
      { ...fixture.services[0], rssBytes: Infinity },
      { ...fixture.services[0], startTicks: -1 },
      { ...fixture.services[0], protected: undefined },
    ]) {
      expect(() =>
        Schema.decodeUnknownSync(ServiceStatusSchema)({
          ...fixture,
          services: [service],
        }),
      ).toThrow();
    }
    expect(() =>
      Schema.decodeUnknownSync(ServiceStatusSchema)({
        ...fixture,
        sampledAt: "invalid-time",
      }),
    ).toThrow();
  });

  it("propagates cancellation to the observation fetch", async () => {
    let signal: AbortSignal | undefined;
    vi.stubGlobal(
      "fetch",
      vi.fn(
        (_path, options) =>
          new Promise((_resolve, reject) => {
            signal = options.signal;
            signal?.addEventListener("abort", () =>
              reject(new DOMException("Aborted", "AbortError")),
            );
          }),
      ),
    );
    const controller = new AbortController();
    const result = loadServiceStatus(controller.signal);
    await Promise.resolve();
    controller.abort();
    await expect(result).rejects.toBeDefined();
    expect(signal?.aborted).toBe(true);
  });
});

describe("allowlisted service action API", () => {
  it("sends one explicit selected operation and confirmation, then decodes actual readback", async () => {
    const result = {
      service: "dnsmasq",
      action: "reload",
      commandAccepted: true,
      snapshot: {
        ...fixture,
        services: [
          { ...fixture.services[0], processState: "unknown", pid: undefined },
        ],
      },
    };
    const fetch = vi.fn().mockResolvedValue(response(result));
    vi.stubGlobal("fetch", fetch);
    const value = await runServiceAction({
      service: "dnsmasq",
      action: "reload",
      confirmImpact: true,
    });
    expect(value.commandAccepted).toBe(true);
    expect(value.snapshot.services[0].processState).toBe("unknown");
    expect(fetch).toHaveBeenCalledTimes(1);
    expect(fetch).toHaveBeenCalledWith(
      "/api/system/services/action",
      expect.objectContaining({
        method: "POST",
        credentials: "same-origin",
        body: JSON.stringify({
          service: "dnsmasq",
          action: "reload",
          confirmImpact: true,
        }),
      }),
    );
  });

  it("retains a refused command result and never retries a failed mutation", async () => {
    const refused = {
      service: "ddns",
      action: "start",
      commandAccepted: false,
      errorCode: "service_action_refused",
      snapshot: fixture,
    };
    const fetch = vi
      .fn()
      .mockResolvedValueOnce(response(refused))
      .mockResolvedValueOnce(
        new Response(
          JSON.stringify({
            ...refused,
            error: {
              code: "service_action_refused",
              message: "服务操作被拒绝",
            },
          }),
          { status: 409 },
        ),
      );
    vi.stubGlobal("fetch", fetch);
    expect(
      await runServiceAction({
        service: "ddns",
        action: "start",
        confirmImpact: false,
      }),
    ).toEqual(refused);
    await expect(
      runServiceAction({
        service: "ddns",
        action: "start",
        confirmImpact: false,
      }),
    ).rejects.toMatchObject({ code: "service_action_refused" });
    expect(fetch).toHaveBeenCalledTimes(2);
  });

  it("accepts optional advertised actions without requiring them on legacy rows", () => {
    const observed = Schema.decodeUnknownSync(ServiceStatusSchema)({
      ...fixture,
      services: [
        {
          ...fixture.services[0],
          actions: ["reload", "restart"],
          actionImpact: "可能短暂中断 DNS/DHCP",
        },
      ],
    });
    expect(observed.services[0].actions).toEqual(["reload", "restart"]);
    expect(
      Schema.decodeUnknownSync(ServiceStatusSchema)(fixture).services[0]
        .actions,
    ).toBeUndefined();
  });
});
