import { Schema } from "effect";
import { afterEach, describe, expect, it, vi } from "vitest";
import { runRequest } from "../../lib/api";
import { requestTraceApi } from "./request-trace-api";
import {
  RequestTraceHistorySchema,
  RequestTraceSchema,
} from "./request-trace-contracts";
import {
  requestTraceFixture,
  requestTraceHistoryFixture,
} from "./request-trace-fixture.test-data";

const response = (body: unknown, status = 200) =>
  new Response(JSON.stringify(body), {
    status,
    headers: { "Content-Type": "application/json" },
  });
afterEach(() => vi.unstubAllGlobals());
describe("active request trace API", () => {
  it("reads server history with unknown and incomplete phases intact", async () => {
    const fetch = vi
      .fn()
      .mockResolvedValue(response(requestTraceHistoryFixture));
    vi.stubGlobal("fetch", fetch);
    const history = await runRequest(requestTraceApi.history());
    expect(history.traces[0].phases[0].durationMs).toBeNull();
    expect(history.traces[0].phases[3].endMs).toBeNull();
    expect(fetch).toHaveBeenCalledWith(
      "/api/proxy/request-traces",
      expect.objectContaining({ method: "GET" }),
    );
  });
  it("sends only one fixed target/route POST with a 12-second client timeout", async () => {
    const fetch = vi.fn().mockResolvedValue(response(requestTraceFixture));
    vi.stubGlobal("fetch", fetch);
    const timeout = vi.spyOn(AbortSignal, "timeout");
    await runRequest(
      requestTraceApi.run({ targetId: "cloudflare", route: "proxy" }),
    );
    expect(fetch).toHaveBeenCalledTimes(1);
    expect(fetch).toHaveBeenCalledWith(
      "/api/proxy/request-traces",
      expect.objectContaining({
        method: "POST",
        body: '{"targetId":"cloudflare","route":"proxy"}',
        credentials: "same-origin",
      }),
    );
    expect(timeout).toHaveBeenCalledWith(12000);
  });
  it("never retries an explicit run and keeps API failure codes", async () => {
    const fetch = vi
      .fn()
      .mockResolvedValue(
        response({ error: { code: "diagnostic_busy", message: "busy" } }, 503),
      );
    vi.stubGlobal("fetch", fetch);
    await expect(
      runRequest(
        requestTraceApi.run({ targetId: "google204", route: "direct" }),
      ),
    ).rejects.toMatchObject({ code: "diagnostic_busy" });
    expect(fetch).toHaveBeenCalledTimes(1);
  });
  it("rejects arbitrary targets or routes before sending a request", async () => {
    const fetch = vi.fn();
    vi.stubGlobal("fetch", fetch);
    for (const input of [
      { targetId: "https://example.test", route: "direct" },
      { targetId: "google204", route: "node-secret" },
    ]) {
      await expect(
        runRequest(
          requestTraceApi.run(
            input as Parameters<typeof requestTraceApi.run>[0],
          ),
        ),
      ).rejects.toMatchObject({ code: "invalid_request" });
    }
    expect(fetch).not.toHaveBeenCalled();
  });
  it("decodes cancelled, timeout and HTTP error traces without replacing them with samples", () => {
    for (const outcome of ["cancelled", "timeout", "http_error"] as const) {
      expect(
        Schema.decodeUnknownSync(RequestTraceSchema)({
          ...requestTraceFixture,
          outcome,
        }).outcome,
      ).toBe(outcome);
    }
  });
  it("rejects invalid dates, values, phase states, duplicate phases and unbounded histories", () => {
    const invalidTraces = [
      { ...requestTraceFixture, startedAt: "not-a-date" },
      { ...requestTraceFixture, totalMs: -1 },
      { ...requestTraceFixture, bytesRead: 65537 },
      { ...requestTraceFixture, statusCode: 700 },
      { ...requestTraceFixture, outcome: "invented" },
      { ...requestTraceFixture, peerScope: "dns-server" },
      {
        ...requestTraceFixture,
        phases: [{ ...requestTraceFixture.phases[0], durationMs: 0 }],
      },
      {
        ...requestTraceFixture,
        phases: [{ ...requestTraceFixture.phases[1], endMs: 5 }],
      },
      {
        ...requestTraceFixture,
        phases: [{ ...requestTraceFixture.phases[1], startMs: Infinity }],
      },
      {
        ...requestTraceFixture,
        phases: [requestTraceFixture.phases[1], requestTraceFixture.phases[1]],
      },
    ];
    for (const trace of invalidTraces)
      expect(() =>
        Schema.decodeUnknownSync(RequestTraceSchema)(trace),
      ).toThrow();
    expect(() =>
      Schema.decodeUnknownSync(RequestTraceHistorySchema)({
        ...requestTraceHistoryFixture,
        traces: Array.from({ length: 65 }, () => requestTraceFixture),
      }),
    ).toThrow();
    expect(() =>
      Schema.decodeUnknownSync(RequestTraceHistorySchema)({
        ...requestTraceHistoryFixture,
        limits: { ...requestTraceHistoryFixture.limits, concurrency: 2 },
      }),
    ).toThrow();
  });
});
