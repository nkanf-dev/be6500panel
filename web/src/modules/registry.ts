import {
  Activity,
  ArrowLeftRight,
  Cable,
  Cpu,
  Gauge,
  Globe,
  Laptop,
  Network,
  Shield,
  Wifi,
  type LucideIcon,
} from "lucide-react";

export type PageId =
  | "overview"
  | "system"
  | "network"
  | "devices"
  | "wifi"
  | "dns"
  | "firewall"
  | "proxy"
  | "frpc";
export interface ModuleRegistration {
  id: PageId;
  title: string;
  shortTitle: string;
  description: string;
  icon: LucideIcon;
  group: "overview" | "network" | "services";
  keywords: string;
  command: string;
  summary?: string;
}
export const modules: readonly ModuleRegistration[] = [
  {
    id: "overview",
    title: "运行概览",
    shortTitle: "概览",
    description: "关键指标与运行状态",
    icon: Gauge,
    group: "overview",
    keywords: "dashboard overview 首页",
    command: "打开运行概览",
    summary: "system",
  },
  {
    id: "system",
    title: "系统",
    shortTitle: "系统",
    description: "资源、内核与运行环境",
    icon: Cpu,
    group: "overview",
    keywords: "cpu memory system 内存",
    command: "查看系统资源",
    summary: "system",
  },
  {
    id: "network",
    title: "网络",
    shortTitle: "网络",
    description: "接口与路由观察",
    icon: Network,
    group: "network",
    keywords: "interface route network 接口 路由",
    command: "查看网络接口",
    summary: "network",
  },
  {
    id: "devices",
    title: "设备",
    shortTitle: "设备",
    description: "客户端与设备策略",
    icon: Laptop,
    group: "network",
    keywords: "client devices 客户端",
    command: "打开设备列表",
  },
  {
    id: "wifi",
    title: "无线网络",
    shortTitle: "Wi-Fi",
    description: "无线接入与射频状态",
    icon: Wifi,
    group: "network",
    keywords: "wifi wireless ssid 无线",
    command: "打开无线网络",
  },
  {
    id: "dns",
    title: "DNS",
    shortTitle: "DNS",
    description: "解析路径与分流策略",
    icon: Globe,
    group: "services",
    keywords: "dns resolver 域名 解析",
    command: "打开 DNS",
  },
  {
    id: "firewall",
    title: "防火墙",
    shortTitle: "防火墙",
    description: "转发、访问控制与标记",
    icon: Shield,
    group: "services",
    keywords: "firewall nft iptables 防火墙",
    command: "打开防火墙",
  },
  {
    id: "proxy",
    title: "代理",
    shortTitle: "代理",
    description: "路由策略与联合计划",
    icon: ArrowLeftRight,
    group: "services",
    keywords: "proxy sing-box 分流 代理",
    command: "打开代理策略",
    summary: "plan",
  },
  {
    id: "frpc",
    title: "frpc",
    shortTitle: "frpc",
    description: "反向隧道与服务映射",
    icon: Cable,
    group: "services",
    keywords: "frpc frp tunnel 隧道 穿透",
    command: "打开 frpc",
    summary: "plan",
  },
];
export const groups = [
  { id: "overview", title: "工作空间" },
  { id: "network", title: "网络管理" },
  { id: "services", title: "网络服务" },
] as const;
export function pageFromHash(): PageId {
  const candidate = window.location.hash.replace(/^#\/?/, "").split("?")[0];
  return modules.some((module) => module.id === candidate)
    ? (candidate as PageId)
    : "overview";
}
export const moduleById = (id: PageId) =>
  modules.find((module) => module.id === id)!;
export const ActivityIcon = Activity;
