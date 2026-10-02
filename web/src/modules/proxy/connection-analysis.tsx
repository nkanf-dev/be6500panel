import { useState } from "react";
import { useConsole } from "../../app/console-context";
import {
  LatencyDistribution,
  RequestWaterfall,
  RuleHitChart,
} from "../../components/visualizations";

/** Existing charts stay accessible without treating demo fixtures as telemetry. */
export function ConnectionAnalysis() {
  const { health } = useConsole();
  const [view, setView] = useState("requests");
  const demo = health?.mode === "demo";
  return (
    <div className="page-stack">
      <div className="section-heading">
        <div>
          <h2>连接分析</h2>
          <p>请求阶段、延迟分布与分流规则</p>
        </div>
        <div className="segmented" role="tablist" aria-label="连接分析视图">
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
        <RequestWaterfall demo={demo} />
      ) : view === "latency" ? (
        <LatencyDistribution demo={demo} />
      ) : (
        <RuleHitChart demo={demo} />
      )}
    </div>
  );
}
