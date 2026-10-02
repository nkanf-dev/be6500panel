import { CheckCircle2, FileCheck2, Lock, TriangleAlert } from "lucide-react";
import {
  Badge,
  Button,
  EmptyState,
  Panel,
  PanelHeader,
} from "../components/ui/primitives";
import type { OperationPlan } from "../lib/contracts";

export function PlanView({
  plan,
  stale = false,
}: {
  plan?: OperationPlan;
  stale?: boolean;
}) {
  return (
    <Panel className="plan-panel">
      <PanelHeader
        title="变更计划"
        subtitle={
          plan
            ? `generation ${plan.generation} · ${plan.id}`
            : "验证后生成协调步骤"
        }
        action={
          plan && (
            <Badge tone={stale ? "warning" : "success"}>
              {stale ? "输入已变更" : "校验通过"}
            </Badge>
          )
        }
      />
      {plan ? (
        <>
          <div className="plan-summary">
            <FileCheck2 size={18} />
            <div>
              <strong>{plan.summary}</strong>
              <span>只读计划</span>
            </div>
          </div>
          <ol className="plan-steps">
            {plan.steps.map((step, index) => (
              <li key={`${step.module}-${index}`}>
                <span className="step-index">{index + 1}</span>
                <div>
                  <div className="step-title">
                    <strong>{step.action}</strong>
                    <Badge>{step.module}</Badge>
                  </div>
                  <p>{step.detail}</p>
                </div>
                <CheckCircle2 size={15} className="text-success" />
              </li>
            ))}
          </ol>
          {plan.warnings.length > 0 && (
            <div className="plan-notes">
              <h3>
                <TriangleAlert size={14} />
                校验提示
              </h3>
              <ul>
                {plan.warnings.map((warning) => (
                  <li key={warning}>{warning}</li>
                ))}
              </ul>
            </div>
          )}
          <div className="plan-footer">
            <span className="text-muted text-xs">执行能力未接入</span>
            <Button disabled size="small">
              <Lock size={13} />
              应用计划
            </Button>
          </div>
        </>
      ) : (
        <EmptyState
          icon={<FileCheck2 size={26} />}
          title="等待校验"
          detail="提交配置以生成计划"
        />
      )}
    </Panel>
  );
}
