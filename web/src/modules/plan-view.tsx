import { FileCheck2, Lock, TriangleAlert } from "lucide-react";
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
        subtitle={plan ? "只读预览，不应用运行配置" : "验证后生成协调步骤"}
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
              <span>仅预览 · 未执行任何变更</span>
            </div>
          </div>
          <ol className="plan-steps" aria-label="拟议步骤">
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
                <span className="text-muted text-xs">拟议步骤</span>
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
          <details className="panel-bottom">
            <summary>高级诊断</summary>
            <dl className="key-values">
              <div>
                <dt>计划标识</dt>
                <dd className="mono wrap">{plan.id}</dd>
              </div>
              <div>
                <dt>计划代际</dt>
                <dd>{plan.generation}</dd>
              </div>
            </dl>
          </details>
          <div className="plan-footer">
            <span className="text-muted text-xs">
              此预览不能应用；实际配置请在节点与运行管理中操作
            </span>
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
          detail="校验策略以生成只读预览，不保存或应用运行配置"
        />
      )}
    </Panel>
  );
}
