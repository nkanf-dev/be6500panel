import { formatBytes } from "./byte-scale";

export function bytes(value: number | null | undefined): string {
  return formatBytes(value);
}
export function uptime(value: number): string {
  const days = Math.floor(value / 86400);
  const hours = Math.floor((value % 86400) / 3600);
  const minutes = Math.floor((value % 3600) / 60);
  return `${days ? `${days} 天 ` : ""}${hours} 时 ${minutes} 分`;
}
export function timestamp(value?: string): string {
  return value
    ? new Date(value).toLocaleTimeString("zh-CN", { hour12: false })
    : "—";
}
