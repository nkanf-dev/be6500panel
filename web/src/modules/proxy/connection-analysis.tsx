import { strings } from "../../locales/strings";
import { useState } from "react";
import { useConsole } from "../../app/console-context";
import { Button, ErrorState } from "../../components/ui/primitives";
import {
  LatencyDistribution,
  RequestWaterfall,
  RuleHitChart,
} from "../../components/visualizations";
import { errorMessage } from "../../lib/api";
import { useProxyTelemetry } from "./use-proxy-telemetry";
import {
  ActiveRoutingChart,
  ConnectionTimeline,
  ProbeLatencyChart,
  ProxyTrafficChart,
} from "./telemetry-charts";

/** Demo fixtures stay separate from actual telemetry and never fill production gaps. */
export function ConnectionAnalysis() {
  const { health } = useConsole();
  return health?.mode === "demo" ? (
    <DemoConnectionAnalysis />
  ) : (
    <ActualConnectionAnalysis />
  );
}

function ActualConnectionAnalysis() {
  const telemetry = useProxyTelemetry();
  const [view, setView] = useState("connections");
  const { data, error, loading, refresh, probe, probing } = telemetry;
  const failed = error !== undefined;
  const stale = data?.state === "stale" || (failed && !!data);
  const canProbe =
    data?.state === "ready" &&
    data.capabilities.latency.available &&
    !loading &&
    !probing;
  const chartProps = { metrics: data, loading, failed };
  return (
    <div className="page-stack">
      <div className="section-heading">
        <div>
          <h2>连接分析</h2>
          <p>实际活动连接、代理流量、主动探测与当前分流</p>
        </div>
        <Button size="small" disabled={loading} onClick={refresh}>
          刷新代理采样
        </Button>
      </div>
      <p className="text-muted text-xs" role="status">
        {data
          ? `${data.state === "unavailable" ? "不可用" : stale ? "上次采样（已过期）" : strings.dashboard.states.live} · 来源：${data.source || "代理控制器"} · 采样时间：${data.sampledAt || "未提供"} · ${data.reason}`
          : loading
            ? "正在读取代理采样"
            : failed
              ? "代理采样读取失败"
              : "尚未取得代理采样"}
      </p>
      {failed && <ErrorState message={errorMessage(error)} onRetry={refresh} />}
      {data && !data.capabilities.requestPhases.available && (
        <p className="text-muted text-xs">
          请求阶段不可用：{data.capabilities.requestPhases.reason}
          。活动连接存续时长不等于请求耗时。
        </p>
      )}
      <div className="segmented" role="tablist" aria-label="连接分析视图">
        {[
          { id: "connections", label: "活动连接" },
          { id: "traffic", label: "代理流量" },
          { id: "latency", label: "探测延迟" },
          { id: "rules", label: "活动连接分流" },
        ].map((item) => (
          <button
            key={item.id}
            type="button"
            role="tab"
            aria-selected={view === item.id}
            onClick={() => setView(item.id)}
          >
            {item.label}
          </button>
        ))}
      </div>
      {view === "latency" && (
        <div className="page-toolbar">
          <Button
            size="small"
            disabled={!canProbe}
            onClick={() => {
              void probe();
            }}
          >
            {probing ? "正在探测选中节点" : "探测当前选中节点"}
          </Button>
          <span className="text-muted text-xs">
            仅在点击后向固定目标发起一次选中节点请求探测，不自动测速。
            {data &&
              !data.capabilities.latency.available &&
              ` 不可用：${data.capabilities.latency.reason}`}
          </span>
        </div>
      )}
      {view === "connections" ? (
        <ConnectionTimeline {...chartProps} />
      ) : view === "traffic" ? (
        <ProxyTrafficChart {...chartProps} />
      ) : view === "latency" ? (
        <ProbeLatencyChart {...chartProps} />
      ) : (
        <ActiveRoutingChart {...chartProps} />
      )}
    </div>
  );
}

function DemoConnectionAnalysis() {
  const [view, setView] = useState("requests");
  return (
    <div className="page-stack">
      <div className="section-heading">
        <div>
          <h2>连接分析 · 演示</h2>
          <p>固定演示样本，不代表当前代理遥测</p>
        </div>
        <div className="segmented" role="tablist" aria-label="连接分析演示视图">
          {[
            { id: "requests", label: "请求阶段" },
            { id: "latency", label: "延迟分布" },
            { id: "rules", label: "规则命中" },
          ].map((item) => (
            <button
              key={item.id}
              type="button"
              role="tab"
              aria-selected={view === item.id}
              onClick={() => setView(item.id)}
            >
              {item.label}
            </button>
          ))}
        </div>
      </div>
      {view === "requests" ? (
        <RequestWaterfall demo />
      ) : view === "latency" ? (
        <LatencyDistribution demo />
      ) : (
        <RuleHitChart demo />
      )}
    </div>
  );
}
