import { strings } from "../../locales/strings";

export const widgetDefinitions = [
  {
    id: "systemSummary",
    title: "系统摘要",
    description: "负载、内存、运行时间与可用模块",
    size: "full",
  },
  {
    id: "trafficHistory",
    title: "WAN 历史",
    description: "服务端保存的真实流量历史与数据导出",
    size: "wide",
  },
  {
    id: "deviceActivity",
    title: strings.visualization.heatmap.title,
    description: "按设备与时段展示 trafficd 实测字节流量",
    size: "full",
  },
  {
    id: "networkDiagnostics",
    title: strings.visualization.waterfall.title,
    description: "主动测试真实 DNS、连接、TLS、首字与内容传输耗时",
    size: "full",
  },
  {
    id: "environment",
    title: "运行环境",
    description: "主机、平台、内核与写入策略",
    size: "compact",
  },
  {
    id: "devices",
    title: "设备观察",
    description: "当前租约与 ARP 观察，不推算终端活跃度",
    size: "compact",
  },
  {
    id: "proxy",
    title: "代理遥测",
    description: "实际代理连接、流量与采集能力",
    size: "wide",
  },
  {
    id: "moduleStatus",
    title: "模块状态",
    description: "能力清单与模块快捷入口",
    size: "full",
  },
] as const;

export type WidgetId = (typeof widgetDefinitions)[number]["id"];
export type WidgetSize = "compact" | "wide" | "full";
export interface WidgetLayout {
  id: WidgetId;
  visible: boolean;
  size: WidgetSize;
}
export interface DashboardLayout {
  name: string;
  widgets: WidgetLayout[];
}
export interface LoadedLayout {
  layout: DashboardLayout;
  notice?: string;
}
export const DASHBOARD_STORAGE_KEY = "be6500panel.dashboard.layout";
const MAX_NAME_LENGTH = 64;

export function defaultLayout(): DashboardLayout {
  return {
    name: "我的仪表盘",
    widgets: widgetDefinitions.map(({ id, size }) => ({
      id,
      size,
      visible: true,
    })),
  };
}

function decodeLayout(value: unknown): DashboardLayout | undefined {
  if (typeof value !== "object" || value === null) return;
  const candidate = value as Record<string, unknown>;
  if (
    typeof candidate.name !== "string" ||
    !candidate.name.trim() ||
    candidate.name.length > MAX_NAME_LENGTH ||
    !Array.isArray(candidate.widgets) ||
    candidate.widgets.length > 64
  )
    return;
  const known = new Set<string>(widgetDefinitions.map(({ id }) => id));
  const seen = new Set<string>();
  const widgets: WidgetLayout[] = [];
  for (const raw of candidate.widgets) {
    if (typeof raw !== "object" || raw === null) return;
    const item = raw as Record<string, unknown>;
    if (typeof item.id !== "string") return;
    // Widgets removed by a newer build need not break a saved layout.
    if (!known.has(item.id)) continue;
    if (
      seen.has(item.id) ||
      typeof item.visible !== "boolean" ||
      typeof item.size !== "string" ||
      !["compact", "wide", "full"].includes(item.size)
    )
      return;
    seen.add(item.id);
    widgets.push({
      id: item.id as WidgetId,
      visible: item.visible,
      size: item.size as WidgetSize,
    });
  }
  // Newly introduced widgets start with their default settings.
  for (const item of defaultLayout().widgets) {
    if (!seen.has(item.id)) widgets.push(item);
  }
  return { name: candidate.name.trim(), widgets };
}

export function readLayout(): LoadedLayout {
  try {
    const raw = window.localStorage.getItem(DASHBOARD_STORAGE_KEY);
    if (raw === null) return { layout: defaultLayout() };
    const decoded =
      raw.length <= 16_384 ? decodeLayout(JSON.parse(raw)) : undefined;
    if (decoded) return { layout: decoded };
    return {
      layout: defaultLayout(),
      notice: "保存的布局无效，已恢复默认布局。编辑并保存可修复。",
    };
  } catch (error) {
    return {
      layout: defaultLayout(),
      notice:
        error instanceof SyntaxError
          ? "保存的布局已损坏，已恢复默认布局。编辑并保存可修复。"
          : "无法读取浏览器布局，已使用默认布局。",
    };
  }
}

export function saveLayout(
  layout: DashboardLayout,
): { ok: true; layout: DashboardLayout } | { ok: false; message: string } {
  const decoded = decodeLayout(layout);
  if (!decoded)
    return { ok: false, message: "请填写 1–64 字的布局名称并检查组件设置。" };
  try {
    // Only presentation settings are durable; no router data or device identities.
    window.localStorage.setItem(DASHBOARD_STORAGE_KEY, JSON.stringify(decoded));
    return { ok: true, layout: decoded };
  } catch {
    return {
      ok: false,
      message: "无法保存到浏览器。请检查存储权限或空间；当前编辑尚未保存。",
    };
  }
}

export function moveWidget(
  layout: DashboardLayout,
  id: WidgetId,
  direction: -1 | 1,
): DashboardLayout {
  const index = layout.widgets.findIndex((widget) => widget.id === id);
  const destination = index + direction;
  if (index < 0 || destination < 0 || destination >= layout.widgets.length)
    return layout;
  const widgets = [...layout.widgets];
  [widgets[index], widgets[destination]] = [
    widgets[destination],
    widgets[index],
  ];
  return { ...layout, widgets };
}
