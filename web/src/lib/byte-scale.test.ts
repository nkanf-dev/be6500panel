import { describe, expect, it } from "vitest";
import { bytes } from "./format";
import {
  byteNumber,
  chooseByteScale,
  formatByteAxisValue,
  formatByteRate,
  formatBytes,
  formatByteValue,
} from "./byte-scale";

describe("decimal byte scales", () => {
  it.each([
    [0, "B", 1],
    [0.25, "B", 1],
    [999, "B", 1],
    [1000, "KB", 1000],
    [1023, "KB", 1000],
    [1024, "KB", 1000],
    [1e6, "MB", 1e6],
    [1e9, "GB", 1e9],
    [1e12, "TB", 1e12],
  ] as const)("chooses %s bytes as %s", (value, unit, divisor) => {
    expect(chooseByteScale([value])).toEqual({ unit, divisor });
  });
  it("uses the finite actual maximum, ignores missing/invalid values and never mutates input", () => {
    const values = Object.freeze([10, NaN, -1, Infinity, 2e9, 1000]);
    expect(chooseByteScale(values)).toEqual({ unit: "GB", divisor: 1e9 });
    expect(chooseByteScale([])).toEqual({ unit: "B", divisor: 1 });
    expect(chooseByteScale([NaN, -1, Infinity])).toEqual({
      unit: "B",
      divisor: 1,
    });
    expect(values[4]).toBe(2e9);
  });
  it("keeps numeric chart values exact rather than rounding them to tick precision", () => {
    const scale = chooseByteScale([2e9]);
    expect(byteNumber(1, scale)).toBe(1e-9);
    expect(byteNumber(123456789, scale)).toBe(0.123456789);
    expect(byteNumber(0, scale)).toBe(0);
  });
  it.each([undefined, null, NaN, Infinity, -Infinity, -1])(
    "leaves invalid or missing %s unavailable",
    (value) => {
      expect(formatBytes(value)).toBe("—");
      expect(formatByteRate(value)).toBe("—");
      expect(formatByteValue(value, chooseByteScale([1e9]))).toBe("—");
      expect(byteNumber(value, chooseByteScale([1e9]))).toBeNull();
    },
  );
  it("formats totals and rates in explicit decimal units with at most two decimals", () => {
    expect(formatBytes(1023)).toBe("1.02 KB");
    expect(formatBytes(1024)).toBe("1.02 KB");
    expect(formatBytes(45e6)).toBe("45 MB");
    expect(formatBytes(1e12)).toBe("1 TB");
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(0.25)).toBe("0.25 B");
    expect(formatByteRate(10)).toBe("10 B/s");
    expect(formatByteRate(1234)).toBe("1.23 KB/s");
    expect(formatByteRate(2.5e6)).toBe("2.5 MB/s");
    expect(formatByteRate(2e9)).toBe("2 GB/s");
    expect(formatByteRate(1e12)).toBe("1 TB/s");
    expect(formatByteValue(2000, chooseByteScale([1e6]), true)).toBe(
      "<0.01 MB/s",
    );
    expect(bytes(1024)).toBe("1.02 KB");
  });
  it("keeps small positives distinct from measured zero and tick labels short", () => {
    expect(formatByteAxisValue(0)).toBe("0");
    expect(formatByteAxisValue(0.001)).toBe("<0.01");
    expect(formatByteAxisValue(1.234567)).toBe("1.23");
    expect(formatByteAxisValue(5000)).toBe("5000");
    expect(formatByteAxisValue(NaN)).toBe("—");
    expect(formatByteAxisValue(1e12)).toBe("1e+12");
    expect(formatBytes(Number.MAX_VALUE).length).toBeLessThan(20);
  });
});
