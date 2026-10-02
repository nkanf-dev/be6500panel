/** Labels are subscription text hints, not measured exit locations. */
export const NODE_REGIONS = [
  { id: "hk", label: "香港", pattern: /香港|hong[\s-]*kong|\bHK\b|🇭🇰/i },
  { id: "jp", label: "日本", pattern: /日本|东京|東京|大阪|japan|\bJP\b|🇯🇵/i },
  {
    id: "us",
    label: "美国",
    pattern: /美国|美國|united[\s-]*states|america|\bUS(?:A)?\b|🇺🇸/i,
  },
  { id: "sg", label: "新加坡", pattern: /新加坡|singapore|\bSG\b|🇸🇬/i },
] as const;
export type NodeRegion = "" | "hk" | "jp" | "us" | "sg" | "other";
export function validRegion(value: unknown): value is NodeRegion {
  return (
    value === "" ||
    value === "other" ||
    NODE_REGIONS.some((region) => region.id === value)
  );
}
export function matchesRegion(label: string, region: NodeRegion) {
  if (!region) return true;
  if (region === "other")
    return !NODE_REGIONS.some((item) => item.pattern.test(label));
  return (
    NODE_REGIONS.find((item) => item.id === region)?.pattern.test(label) ??
    false
  );
}
