import { useMemo } from "react";
import { ArrowRight } from "lucide-react";
import { Button } from "../../components/ui/primitives";

interface StateDataViewerProps {
  data: Record<string, unknown>;
  onSelectTarget?: (item: Record<string, unknown>) => void;
}

function formatLabel(key: string): string {
  const dictionary: Record<string, string> = {
    enabled: "启用状态",
    configured: "配置状态",
    enable: "启用状态",
    status: "运行状态",
    proto: "协议类型",
    protocol: "协议",
    ip: "IP 地址",
    ipaddr: "IP 地址",
    netmask: "子网掩码",
    gateway: "网关地址",
    dns: "DNS 服务器",
    mac: "MAC 地址",
    name: "名称",
    ssid: "Wi-Fi 名称 (SSID)",
    encryption: "加密方式",
    channel: "信道",
    band: "频段",
    txpower: "发射功率",
    port: "端口",
    src_port: "外部端口",
    srcport: "外部端口",
    sport: "外部端口",
    dest_port: "内部端口",
    destport: "内部端口",
    dport: "内部端口",
    target_ip: "目标 IP",
    dest_ip: "目标 IP",
    destip: "目标 IP",
    hostname: "主机名",
    lease_time: "租约时间",
    domain: "域名",
    username: "账号 / 用户名",
    server: "服务器地址",
    mtu: "MTU（字节）",
    timer_status: "定时开关",
    timer_open: "开启时间",
    timer_close: "关闭时间",
    time_open: "开启时间",
    time_close: "关闭时间",
    switch: "开关状态",
  };
  return dictionary[key] ?? key;
}

function isTruthy(val: unknown): boolean {
  if (
    val === false ||
    val === 0 ||
    val === "0" ||
    val === "false" ||
    val === "off" ||
    val === "disabled" ||
    val === null ||
    val === undefined ||
    val === ""
  ) {
    return false;
  }
  return Boolean(val);
}

const BOOLEAN_KEYS = new Set([
  "enabled",
  "enable",
  "on",
  "timer_on",
  "switch",
  "status",
  "timer_status",
  "weakenable",
  "hidden",
  "peerdns",
  "autoset",
  "special",
  "txbf",
  "mlo_enable",
  "bsd",
  "ax",
  "guest_enabled",
  "guest_wifi",
  "configured",
]);

function isBooleanKey(key: string): boolean {
  const k = key.toLowerCase();
  return (
    BOOLEAN_KEYS.has(k) ||
    k.endsWith("_enable") ||
    k.endsWith("_enabled") ||
    k.endsWith("_on") ||
    k.endsWith("_switch")
  );
}

function formatValue(key: string, value: unknown): string {
  if (value === null || value === undefined) return "—";
  if (key === "configured") {
    return value ? "已配置" : "未配置";
  }
  if (typeof value === "boolean") {
    return value ? "已启用" : "已关闭";
  }
  if (isBooleanKey(key) && (value === "0" || value === "1" || value === 0 || value === 1)) {
    return isTruthy(value) ? "已启用" : "已关闭";
  }
  if (Array.isArray(value)) {
    if (value.length === 0) return "无";
    if (value.every((item) => typeof item === "object" && item !== null && "ip" in item)) {
      return value
        .map((item: Record<string, unknown>) => `${item.ip}${item.mask ? ` (${item.mask})` : ""}`)
        .join(", ");
    }
    return value
      .map((item) => (typeof item === "object" && item !== null ? JSON.stringify(item) : String(item)))
      .join(", ");
  }
  if (typeof value === "object") {
    return Object.entries(value as Record<string, unknown>)
      .map(([k, v]) => `${formatLabel(k)}: ${v}`)
      .join(", ");
  }
  return String(value);
}

export function StateDataViewer({ data, onSelectTarget }: StateDataViewerProps) {
  // Separate list/array entries from scalar/object entries
  const { lists, scalars, nestedObjects } = useMemo(() => {
    const listEntries: Array<{ key: string; items: Array<Record<string, unknown>> }> = [];
    const scalarEntries: Array<{ key: string; value: unknown }> = [];
    const objectEntries: Array<{ key: string; data: Record<string, unknown> }> = [];

    for (const [key, val] of Object.entries(data)) {
      if (key.endsWith("Configured")) continue; // Handled by secret display

      if (Array.isArray(val)) {
        const objectItems = val.filter(
          (item): item is Record<string, unknown> => typeof item === "object" && item !== null,
        );
        if (objectItems.length > 0) {
          listEntries.push({ key, items: objectItems });
        } else {
          scalarEntries.push({ key, value: val.join(", ") });
        }
      } else if (typeof val === "object" && val !== null) {
        objectEntries.push({ key, data: val as Record<string, unknown> });
      } else {
        scalarEntries.push({ key, value: val });
      }
    }
    return { lists: listEntries, scalars: scalarEntries, nestedObjects: objectEntries };
  }, [data]);

  return (
    <div className="space-y-4">
      {/* 1. Scalar Key-Values Summary */}
      {scalars.length > 0 && (
        <div className="space-y-2">
          <h4 className="text-xs font-semibold text-muted uppercase tracking-wider">
            运行参数摘要
          </h4>
          <dl className="key-values">
            {scalars.map(({ key, value }) => {
              const isSecretConfigured = Boolean(data[`${key}Configured`]);
              const display = isSecretConfigured ? "•••••••• (已配置)" : formatValue(key, value);
              return (
                <div key={key}>
                  <dt>{formatLabel(key)}</dt>
                  <dd className="font-mono text-sm">{display}</dd>
                </div>
              );
            })}
          </dl>
        </div>
      )}

      {/* 2. Nested Objects (like info / wifi / status) */}
      {nestedObjects.map(({ key, data: subData }) => (
        <div key={key} className="space-y-2 border-t border-border/50 pt-3">
          <h4 className="text-xs font-semibold text-muted uppercase tracking-wider">
            {formatLabel(key)} 信息
          </h4>
          <dl className="key-values">
            {Object.entries(subData).map(([subK, subV]) => (
              <div key={subK}>
                <dt>{formatLabel(subK)}</dt>
                <dd className="font-mono text-sm">{formatValue(subK, subV)}</dd>
              </div>
            ))}
          </dl>
        </div>
      ))}

      {/* 3. Table / Card Lists (leases, forwardings, items) */}
      {lists.map(({ key, items }) => {
        // Collect column headers from first few items
        const columns = Array.from(
          new Set(
            items.flatMap((item) => Object.keys(item)).filter((col) => !col.endsWith("Configured")),
          ),
        ).slice(0, 6);

        return (
          <div key={key} className="space-y-2 border-t border-border/50 pt-3">
            <div className="flex items-center justify-between">
              <h4 className="text-xs font-semibold text-muted uppercase tracking-wider">
                {formatLabel(key)} · 共 {items.length} 项
              </h4>
            </div>

            <div className="table-scroll">
              <table className="data-table" aria-label={formatLabel(key)}>
                <thead>
                  <tr>
                    {columns.map((col) => (
                      <th key={col}>{formatLabel(col)}</th>
                    ))}
                    {onSelectTarget && <th>操作</th>}
                  </tr>
                </thead>
                <tbody>
                  {items.map((item, idx) => (
                    <tr key={idx}>
                      {columns.map((col) => (
                        <td key={col} className="font-mono text-xs">
                          {formatValue(col, item[col])}
                        </td>
                      ))}
                      {onSelectTarget && (
                        <td>
                          <Button
                            type="button"
                            size="small"
                            variant="ghost"
                            onClick={() => onSelectTarget(item)}
                          >
                            选择编辑 <ArrowRight size={12} className="ml-1 inline" />
                          </Button>
                        </td>
                      )}
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </div>
        );
      })}
    </div>
  );
}
