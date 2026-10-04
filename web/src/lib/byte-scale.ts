export interface ByteScale {
  unit: "B" | "KB" | "MB" | "GB" | "TB";
  divisor: number;
}

const units: readonly ByteScale["unit"][] = ["B", "KB", "MB", "GB", "TB"];
const compactNumber = new Intl.NumberFormat("zh-CN", {
  maximumFractionDigits: 2,
  useGrouping: false,
});
const measured = (value: number | null | undefined): value is number =>
  typeof value === "number" && Number.isFinite(value) && value >= 0;

/** Decimal bytes. Select one scale from actual measurements for a whole chart axis. */
export function chooseByteScale(values: readonly number[]): ByteScale {
  let maximum = 0;
  for (const value of values)
    if (measured(value)) maximum = Math.max(maximum, value);
  let exponent = 0;
  while (exponent < units.length - 1 && maximum >= 1000 ** (exponent + 1))
    exponent++;
  return { unit: units[exponent], divisor: 1000 ** exponent };
}

/** Numeric chart projection only: retain precision, preserve missing values as gaps. */
export function byteNumber(
  raw: number | null | undefined,
  scale: ByteScale,
): number | null {
  return measured(raw) ? raw / scale.divisor : null;
}

/** Already-scaled axis ticks and display numbers, never raw chart data. */
export function formatByteAxisValue(value: number): string {
  if (!measured(value)) return "—";
  if (value > 0 && value < 0.01) return "<0.01";
  // Protect labels even when data exceeds the largest supported byte unit.
  if (value >= 1e6) return value.toExponential(2).replace(/\.?0+e/, "e");
  return compactNumber.format(value);
}

export function formatByteValue(
  raw: number | null | undefined,
  scale?: ByteScale,
  rates = false,
): string {
  if (!measured(raw)) return "—";
  const selected = scale ?? chooseByteScale([raw]);
  return `${formatByteAxisValue(raw / selected.divisor)} ${selected.unit}${rates ? "/s" : ""}`;
}

export function formatBytes(
  raw: number | null | undefined,
  scale?: ByteScale,
): string {
  return formatByteValue(raw, scale);
}

export function formatByteRate(
  raw: number | null | undefined,
  scale?: ByteScale,
): string {
  return formatByteValue(raw, scale, true);
}
