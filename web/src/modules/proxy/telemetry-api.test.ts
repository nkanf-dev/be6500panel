import { Schema } from "effect";
import { describe, expect, it, vi, afterEach } from "vitest";
import { runRequest } from "../../lib/api";
import { ProxyMetricsSchema, proxyTelemetry } from "./telemetry-api";

const time = "2026-10-03T00:00:00Z";
const capability = { available: true, reason: "actual observed core data" };
export const telemetryFixture = {
  state: "ready" as const,
  reason: "",
  source: "sing-box Clash API · localhost",
  sampledAt: time,
  capabilities: {
    connections: capability,
    traffic: capability,
    routing: capability,
    latency: capability,
    requestPhases: { available: false, reason: "HTTPS content is opaque" },
  },
  totals: { uploadBytes: 1000, downloadBytes: 2000 },
  activeConnections: 1,
  truncated: false,
  connections: [
    {
      id: "hash-id",
      startedAt: time,
      ageMs: 1000,
      network: "tcp" as const,
      sourceIP: "192.168.31.2",
      sourcePort: 12345,
      destinationIP: "198.51.100.1",
      destinationPort: 443,
      host: "example.test",
      uploadBytes: 20,
      downloadBytes: 30,
      outbound: "proxy" as const,
      ruleId: "hash-rule",
      rule: "domain=example.test => route(proxy)",
    },
  ],
  traffic: [{ time, uploadRate: 10, downloadRate: 20, reset: false }],
  probes: [{ time, delayMs: 93, status: "ok" as const }],
};
const response = (body: unknown) =>
  new Response(JSON.stringify(body), {
    status: 200,
    headers: { "Content-Type": "application/json" },
  });
afterEach(() => vi.unstubAllGlobals());
describe("bounded proxy telemetry API", () => {
  it("decodes real counters without inventing request phases", async () => {
    const fetch = vi.fn().mockResolvedValue(response(telemetryFixture));
    vi.stubGlobal("fetch", fetch);
    const value = await runRequest(proxyTelemetry.metrics());
    expect(value.connections[0].host).toBe("example.test");
    expect(value.capabilities.requestPhases.available).toBe(false);
    expect(fetch.mock.calls[0][0]).toBe("/api/proxy/metrics");
  });
  it("sends an explicit empty POST without accepting user URLs or node secrets", async () => {
    const fetch = vi.fn().mockResolvedValue(response(telemetryFixture));
    vi.stubGlobal("fetch", fetch);
    await runRequest(proxyTelemetry.probe());
    expect(fetch).toHaveBeenCalledWith(
      "/api/proxy/probe",
      expect.objectContaining({
        method: "POST",
        body: "{}",
        credentials: "same-origin",
      }),
    );
  });
  it("rejects wrong shape, invalid dates, negatives, nonfinite numbers and oversized collections", () => {
    for (const fixture of [
      { ...telemetryFixture, state: "demo" },
      { ...telemetryFixture, sampledAt: "not-a-date" },
      { ...telemetryFixture, activeConnections: 4097 },
      {
        ...telemetryFixture,
        connections: [
          { ...telemetryFixture.connections[0], destinationPort: 65536 },
        ],
      },
      { ...telemetryFixture, totals: { uploadBytes: -1, downloadBytes: 0 } },
      {
        ...telemetryFixture,
        traffic: [
          { time, uploadRate: Infinity, downloadRate: 0, reset: false },
        ],
      },
      {
        ...telemetryFixture,
        connections: Array.from(
          { length: 129 },
          () => telemetryFixture.connections[0],
        ),
      },
      {
        ...telemetryFixture,
        traffic: Array.from({ length: 901 }, () => telemetryFixture.traffic[0]),
      },
      {
        ...telemetryFixture,
        probes: Array.from({ length: 33 }, () => telemetryFixture.probes[0]),
      },
      {
        ...telemetryFixture,
        connections: [
          { ...telemetryFixture.connections[0], host: "a".repeat(254) },
        ],
      },
    ])
      expect(() =>
        Schema.decodeUnknownSync(ProxyMetricsSchema)(fixture),
      ).toThrow();
  });
  it("propagates cancellation to fetch", async () => {
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
    const result = runRequest(proxyTelemetry.metrics(), controller.signal);
    await Promise.resolve();
    controller.abort();
    await expect(result).rejects.toBeDefined();
    expect(signal?.aborted).toBe(true);
  });
  it("returns explicit unavailable rather than demo samples", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue(
        response({
          ...telemetryFixture,
          state: "unavailable",
          reason: "with_clash_api missing",
          sampledAt: undefined,
          connections: [],
          traffic: [],
          probes: [],
          activeConnections: 0,
          totals: { uploadBytes: 0, downloadBytes: 0 },
        }),
      ),
    );
    const value = await runRequest(proxyTelemetry.metrics());
    expect(value.state).toBe("unavailable");
    expect(value.traffic).toEqual([]);
  });
});
