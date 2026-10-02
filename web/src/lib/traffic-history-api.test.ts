import { afterEach, describe, expect, it, vi } from "vitest";
import { Schema } from "effect";
import { loadTrafficHistory } from "./traffic-history-api";
import {
  TrafficHistorySchema,
  TRAFFIC_HISTORY_RANGES,
} from "./traffic-history-contracts";
import { historyFixture } from "../components/traffic-history/history-fixture.test-data";
import { trafficHistoryCSV, trafficTimestamp } from "./traffic-history-format";
const respond = (body: unknown) =>
  new Response(JSON.stringify(body), {
    headers: { "Content-Type": "application/json" },
  });
afterEach(() => vi.unstubAllGlobals());

describe("durable history HTTP contract", () => {
  it.each(TRAFFIC_HISTORY_RANGES)(
    "requests the exact %s time window with a server-side 1500-point bound",
    async (range) => {
      const fetch = vi.fn().mockResolvedValue(respond(historyFixture(range)));
      vi.stubGlobal("fetch", fetch);
      await expect(loadTrafficHistory(range)).resolves.toEqual(
        historyFixture(range),
      );
      expect(fetch).toHaveBeenCalledWith(
        `/api/traffic/history?range=${range}&maxPoints=1500`,
        expect.objectContaining({
          credentials: "same-origin",
          method: "GET",
          signal: expect.any(AbortSignal),
        }),
      );
    },
  );
  it("rejects a mismatched range instead of displaying it under a different selection", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue(respond(historyFixture("1d"))),
    );
    await expect(loadTrafficHistory("1y")).rejects.toMatchObject({
      code: "invalid_response",
    });
  });
  it("decodes disabled/unpersisted history without inventing a sample", () => {
    const empty = {
      ...historyFixture(),
      enabled: false,
      persistent: false,
      samples: [],
      summary: { rxBytes: 0, txBytes: 0, coverageSeconds: 0 },
      oldestAt: undefined,
      error: "history disabled",
    };
    expect(Schema.decodeUnknownSync(TrafficHistorySchema)(empty)).toEqual(
      empty,
    );
  });
  it("rejects invalid units, time, unbounded arrays and legacy rate-only samples", () => {
    const valid = historyFixture();
    const sample = valid.samples[0];
    for (const invalid of [
      { ...valid, samples: [{ ...sample, rx: -1 }] },
      { ...valid, samples: [{ ...sample, time: "18:00" }] },
      { ...valid, samples: Array.from({ length: 1501 }, () => sample) },
      { ...valid, samples: [{ time: sample.time, rx: 1, tx: 2 }] },
      { ...valid, summary: { ...valid.summary, rxBytes: Infinity } },
    ])
      expect(() =>
        Schema.decodeUnknownSync(TrafficHistorySchema)(invalid),
      ).toThrow();
  });
  it("aborts fetch when the caller abandons its range", async () => {
    let signal: AbortSignal | undefined;
    vi.stubGlobal(
      "fetch",
      vi.fn((_url, init: RequestInit) => {
        signal = init.signal as AbortSignal;
        return new Promise((_resolve, reject) =>
          signal?.addEventListener("abort", () =>
            reject(new DOMException("aborted", "AbortError")),
          ),
        );
      }),
    );
    const controller = new AbortController();
    const pending = loadTrafficHistory("30m", controller.signal).catch(
      (error) => error,
    );
    controller.abort();
    expect(await pending).toBeDefined();
    expect(signal?.aborted).toBe(true);
  });
});

describe("range export and UTC", () => {
  it("exports every server bucket, raw counter totals, peaks and missing rates, not a sliced chart viewport", () => {
    const history = historyFixture("1y");
    const lines = trafficHistoryCSV(history).split("\r\n");
    expect(lines).toHaveLength(4);
    expect(lines[0]).toContain("rxBytes,txBytes,coverageSeconds");
    expect(lines[1]).toBe(
      "1y,30,2026-10-02T18:00:00.000Z,1000000,500000,2000000,750000,30000000,15000000,30",
    );
    expect(lines[2]).toBe("1y,30,2026-10-02T18:00:30.000Z,,,,,,,0");
    expect(lines[3]).toContain("15000000,7500000,30");
  });
  it("normalizes offset timestamps consistently to explicit UTC", () => {
    expect(trafficTimestamp("2026-10-03T02:00:00+08:00")).toBe(
      "2026-10-02T18:00:00.000Z",
    );
  });
});
