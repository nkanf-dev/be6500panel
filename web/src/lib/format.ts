export function bytes(value: number): string {
  if (!Number.isFinite(value) || value < 0) return "—";
  if (value < 1024) return `${value} B`;
  const exponent = Math.min(3, Math.floor(Math.log(value) / Math.log(1024)));
  return `${(value / 1024 ** exponent).toFixed(1)} ${["B", "KiB", "MiB", "GiB"][exponent]}`;
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
