import { Activity, ChevronRight, Plus } from "lucide-react";
import { useState } from "react";
import { useConsole } from "../app/console-context";
import {
  Badge,
  Button,
  EmptyState,
  Panel,
  PanelHeader,
} from "../components/ui/primitives";
import { moduleById, type PageId } from "./registry";
import { ActivityHeatmap } from "../components/visualizations";

const columns: Record<string, readonly string[]> = {
  devices: ["设备名称", "IP 地址", "连接方式", "状态", "策略"],
  wifi: ["无线网络", "频段", "信道", "安全协议", "状态"],
  dns: ["解析器", "协议", "地址", "路由策略", "状态"],
  firewall: ["规则", "链", "匹配条件", "动作", "命中"],
};
const entities: Record<string, string> = {
  devices: "设备发现",
  wifi: "无线适配器",
  dns: "DNS 适配器",
  firewall: "防火墙适配器",
};
export function UnavailablePage({ id }: { id: PageId }) {
  const registration = moduleById(id);
  const { capabilities, health } = useConsole();
  const module = capabilities.find((item) => item.id === id);
  const [showDetails, setShowDetails] = useState(false);
  const Icon = registration.icon;
  return (
    <div className="page-stack">
      <div className="page-toolbar">
        <span className="status-text text-muted">
          <Activity size={14} />
          {entities[id]} · 未接入
        </span>
        <Button size="small" disabled>
          <Plus size={14} />
          {id === "devices" ? "添加策略" : "新建配置"}
        </Button>
      </div>
      <Panel>
        <PanelHeader
          title={registration.title}
          subtitle={registration.description}
          action={<Badge>未接入</Badge>}
        />
        <div className="table-scroll">
          <table className="data-table">
            <thead>
              <tr>
                {columns[id]?.map((column) => (
                  <th key={column}>{column}</th>
                ))}
              </tr>
            </thead>
          </table>
        </div>
        <EmptyState
          icon={<Icon size={26} />}
          title={`${entities[id]}未接入`}
          detail="等待设备适配器"
        >
          <Button
            variant="ghost"
            size="small"
            onClick={() => setShowDetails(!showDetails)}
          >
            {showDetails ? "收起能力" : "能力详情"}
            <ChevronRight size={14} />
          </Button>
        </EmptyState>
        {showDetails && (
          <div className="capability-detail">
            {module?.capabilities.map((capability) => (
              <div key={capability.id}>
                <span>{capability.title}</span>
                <Badge tone={capability.supported ? "success" : "neutral"}>
                  {capability.supported ? "支持" : "未接入"}
                </Badge>
                <p>{capability.reason}</p>
              </div>
            )) ?? <p>尚未读取能力清单</p>}
          </div>
        )}
      </Panel>
      {id === "devices" && <ActivityHeatmap demo={health?.mode === "demo"} />}
    </div>
  );
}
