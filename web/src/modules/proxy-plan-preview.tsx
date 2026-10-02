import { useState } from "react";
import { ArrowRight, FileCheck2, SlidersHorizontal } from "lucide-react";
import {
  Badge,
  Button,
  ErrorState,
  Field,
  Panel,
  PanelHeader,
  Select,
} from "../components/ui/primitives";
import {
  LatencyDistribution,
  RequestWaterfall,
  RuleHitChart,
} from "../components/visualizations";
import { api, errorMessage, runRequest } from "../lib/api";
import { type OperationPlan, type ProxyPlanInput } from "../lib/contracts";
import { useConsole } from "../app/console-context";
import { PlanView } from "./plan-view";

const initial: ProxyPlanInput = {
  mode: "split",
  dnsStrategy: "split",
  ipv6Policy: "follow",
  failurePolicy: "block-proxy",
  nodeCount: 1,
};
export function ProxyPlanPreview() {
  const { health } = useConsole();
  const [input, setInput] = useState<ProxyPlanInput>(initial);
  const [plan, setPlan] = useState<OperationPlan>();
  const [plannedInput, setPlannedInput] = useState("");
  const [error, setError] = useState<unknown>();
  const [pending, setPending] = useState(false);
  const [tab, setTab] = useState("policy");
  function update<K extends keyof ProxyPlanInput>(
    key: K,
    value: ProxyPlanInput[K],
  ) {
    setInput((previous) => ({ ...previous, [key]: value }));
  }
  async function submit(event: React.FormEvent) {
    event.preventDefault();
    setPending(true);
    setError(undefined);
    try {
      const response = await runRequest(api.proxyPlan(input));
      setPlan(response);
      setPlannedInput(JSON.stringify(input));
    } catch (error) {
      setError(error);
    } finally {
      setPending(false);
    }
  }
  return (
    <div className="page-stack">
      <div className="page-toolbar">
        <div className="segmented" role="tablist" aria-label="代理视图">
          {[
            { id: "policy", label: "策略计划" },
            { id: "diagnostics", label: "请求诊断" },
          ].map((item) => (
            <button
              role="tab"
              aria-selected={tab === item.id}
              key={item.id}
              onClick={() => setTab(item.id)}
            >
              {item.label}
            </button>
          ))}
        </div>
        <Badge>仅预览 · 不修改运行配置</Badge>
      </div>
      {tab === "policy" ? (
        <div className="planning-layout">
          <Panel>
            <PanelHeader
              title="分流策略"
              subtitle="联合 DNS / IPv6 / 防火墙校验"
              action={<SlidersHorizontal size={16} />}
            />
            <form onSubmit={submit} className="config-form">
              <Field label="运行模式">
                <Select
                  label="运行模式"
                  value={input.mode}
                  onValueChange={(value) =>
                    update("mode", value as ProxyPlanInput["mode"])
                  }
                  options={[
                    { value: "split", label: "规则分流" },
                    { value: "global", label: "全局代理" },
                    { value: "direct", label: "全部直连" },
                  ]}
                />
              </Field>
              <div className="form-divider" />
              <div className="form-grid">
                <Field label="DNS 策略">
                  <Select
                    label="DNS 策略"
                    value={input.dnsStrategy}
                    onValueChange={(value) =>
                      update(
                        "dnsStrategy",
                        value as ProxyPlanInput["dnsStrategy"],
                      )
                    }
                    options={[
                      { value: "split", label: "分流解析" },
                      { value: "direct", label: "直连解析" },
                    ]}
                  />
                </Field>
                <Field label="IPv6 策略">
                  <Select
                    label="IPv6 策略"
                    value={input.ipv6Policy}
                    onValueChange={(value) =>
                      update(
                        "ipv6Policy",
                        value as ProxyPlanInput["ipv6Policy"],
                      )
                    }
                    options={[
                      { value: "follow", label: "跟随分流" },
                      { value: "direct", label: "直连" },
                      { value: "block", label: "阻断" },
                    ]}
                  />
                </Field>
                <Field label="故障策略">
                  <Select
                    label="故障策略"
                    value={input.failurePolicy}
                    onValueChange={(value) =>
                      update(
                        "failurePolicy",
                        value as ProxyPlanInput["failurePolicy"],
                      )
                    }
                    options={[
                      { value: "block-proxy", label: "阻断代理流量" },
                      { value: "direct", label: "回退直连" },
                    ]}
                  />
                </Field>
                <Field label="计划节点数" hint="仅规模校验，不包含节点凭据">
                  <input
                    type="number"
                    min={0}
                    max={4096}
                    required
                    value={input.nodeCount}
                    onChange={(event) =>
                      update("nodeCount", event.target.valueAsNumber)
                    }
                  />
                </Field>
              </div>
              <div className="policy-path">
                <span>LAN</span>
                <ArrowRight size={13} />
                <span>DNS</span>
                <ArrowRight size={13} />
                <span>分流策略</span>
                <ArrowRight size={13} />
                <span>出口</span>
              </div>
              {error !== undefined && (
                <ErrorState message={errorMessage(error)} />
              )}
              <div className="form-actions">
                <span className="text-muted text-xs">POST /api/proxy/plan</span>
                <Button variant="primary" disabled={pending} type="submit">
                  <FileCheck2 size={15} />
                  {pending ? "校验中…" : "校验并生成计划"}
                </Button>
              </div>
            </form>
          </Panel>
          <PlanView
            plan={plan}
            stale={!!plan && plannedInput !== JSON.stringify(input)}
          />
        </div>
      ) : (
        <>
          <RequestWaterfall demo={health?.mode === "demo"} />
          <div className="two-column">
            <LatencyDistribution demo={health?.mode === "demo"} />
            <RuleHitChart demo={health?.mode === "demo"} />
          </div>
        </>
      )}
    </div>
  );
}
