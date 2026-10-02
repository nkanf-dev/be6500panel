import { useEffect, useState } from "react";
import * as Dialog from "@radix-ui/react-dialog";
import {
  Check,
  CheckCheck,
  GitCommitHorizontal,
  RefreshCw,
  RotateCcw,
  X,
} from "lucide-react";
import { Badge, Button } from "../ui/primitives";
import { changedFields } from "./native-document";
import type { ConfigurationController } from "./use-configuration";
import type { ConfigurationDraft, PendingCommit } from "./contracts";

function PendingConfirmation({
  pending,
  controller,
}: {
  pending: PendingCommit;
  controller: ConfigurationController;
}) {
  const [now, setNow] = useState(Date.now);
  const deadline = Date.parse(pending.deadline);
  const remaining = Number.isFinite(deadline)
    ? Math.max(0, Math.ceil((deadline - now) / 1000))
    : 0;
  const expired = remaining === 0;
  useEffect(() => {
    setNow(Date.now());
    const interval = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(interval);
  }, [pending.id, pending.deadline]);
  return (
    <section className="configuration-pending" aria-label="提交连接确认">
      <div className="configuration-pending-summary">
        <Badge tone="warning">等待连接确认</Badge>
        <strong role="timer" aria-label="自动回滚倒计时">
          {remaining}s
        </strong>
        <span>
          {expired
            ? "确认期限已到，正在核对回滚状态"
            : "配置已应用。连接可用后确认；逾期自动恢复先前配置。"}
        </span>
      </div>
      <div className="configuration-actions">
        <Button
          size="small"
          variant="ghost"
          disabled={!!controller.busy}
          onClick={() => {
            void controller.reconcile();
          }}
          aria-label="刷新提交状态"
        >
          <RefreshCw size={14} />
          刷新状态
        </Button>
        <Button
          size="small"
          disabled={!!controller.busy}
          onClick={() => {
            void controller.rollback();
          }}
        >
          <RotateCcw size={14} />
          {controller.busy === "rollback" ? "正在恢复…" : "恢复先前配置"}
        </Button>
        <Button
          size="small"
          variant="primary"
          disabled={!!controller.busy || expired}
          onClick={() => {
            void controller.confirm();
          }}
        >
          <CheckCheck size={14} />
          {controller.busy === "confirm" ? "正在确认…" : "确认当前连接"}
        </Button>
      </div>
      <span className="configuration-pending-meta">
        {pending.id} · 期限 {new Date(pending.deadline).toLocaleTimeString()} ·
        状态每 10s 核对
      </span>
    </section>
  );
}

function RiskDialog({
  drafts,
  generation,
  open,
  onOpenChange,
  onCommit,
}: {
  drafts: readonly ConfigurationDraft[];
  generation: number;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onCommit: () => void;
}) {
  const risks = [
    ...new Map(
      drafts
        .flatMap((draft) => draft.risks)
        .map((risk) => [`${risk.code}:${risk.message}`, risk]),
    ).values(),
  ];
  const fields = changedFields(drafts);
  return (
    <Dialog.Root open={open} onOpenChange={onOpenChange}>
      <Dialog.Portal>
        <Dialog.Overlay className="configuration-dialog-overlay" />
        <Dialog.Content className="configuration-risk-dialog">
          <div className="configuration-dialog-header">
            <div>
              <Badge tone="danger">高风险变更</Badge>
              <Dialog.Title>确认高风险 Commit</Dialog.Title>
            </div>
            <Dialog.Close asChild>
              <Button variant="ghost" size="icon" aria-label="关闭风险确认">
                <X size={16} />
              </Button>
            </Dialog.Close>
          </div>
          <Dialog.Description>
            将应用 {drafts.length} 个草稿（g{generation}
            ）。管理连接相关变更需在期限内确认，未确认将自动回滚。
          </Dialog.Description>
          {!!fields.length && (
            <section className="configuration-changed-fields">
              <h3>变更字段</h3>
              <ul>
                {fields.map((field) => (
                  <li key={field}>
                    <code>{field}</code>
                  </li>
                ))}
              </ul>
            </section>
          )}
          <ul className="configuration-diagnostics configuration-risks">
            {risks.map((risk) => (
              <li key={`${risk.code}:${risk.message}`}>
                <code>{risk.code}</code>
                <span>{risk.message}</span>
              </li>
            ))}
          </ul>
          <footer className="configuration-dialog-footer">
            <Dialog.Close asChild>
              <Button>返回检查</Button>
            </Dialog.Close>
            <Button className="configuration-danger-button" onClick={onCommit}>
              <GitCommitHorizontal size={15} />
              确认风险并 Commit
            </Button>
          </footer>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

export function CommitControls({
  controller,
}: {
  controller: ConfigurationController;
}) {
  const [riskDrafts, setRiskDrafts] = useState<readonly ConfigurationDraft[]>(
    [],
  );
  const [riskGeneration, setRiskGeneration] = useState<number>();
  const selected = controller.drafts.filter((draft) =>
    controller.selectedIds.includes(draft.id),
  );
  const valid =
    selected.length > 0 &&
    selected.every(
      (draft) =>
        draft.valid && draft.generation === controller.status?.generation,
    );
  const hasRisks = selected.some((draft) => draft.risks.length > 0);
  const pending = controller.status?.pendingCommit;
  const operation = controller.operation;
  useEffect(() => {
    if (
      riskGeneration !== undefined &&
      riskGeneration !== controller.status?.generation
    ) {
      setRiskDrafts([]);
      setRiskGeneration(undefined);
    }
  }, [riskGeneration, controller.status?.generation]);
  return (
    <>
      <div className="configuration-commit-bar">
        <div>
          <strong>
            <GitCommitHorizontal size={16} />
            Commit 配置
          </strong>
          <span>
            {selected.length} 个已选草稿 · {controller.drafts.length} 个暂存
            {hasRisks ? " · 包含高风险变更" : ""}
          </span>
        </div>
        <Button
          variant="primary"
          disabled={
            !valid ||
            !!controller.busy ||
            !!pending ||
            !controller.status?.enabled
          }
          aria-label={`Commit 已选草稿 (${selected.length})`}
          onClick={() => {
            if (hasRisks) {
              setRiskDrafts(selected);
              setRiskGeneration(controller.status?.generation);
            } else void controller.commit(false);
          }}
        >
          <GitCommitHorizontal size={15} />
          {controller.busy === "commit"
            ? "正在提交…"
            : `Commit (${selected.length})`}
        </Button>
      </div>
      {operation && !pending && (
        <div className="configuration-operation-result" role="status">
          <Check size={15} />
          <span>
            {operation.state === "rolled_back"
              ? "已回滚"
              : operation.state === "committed"
                ? "已提交"
                : "待确认提交已结束 · 已核对当前配置"}{" "}
            · {operation.changedModules.join(", ")} · g{operation.generation}
          </span>
          <code>{operation.id}</code>
        </div>
      )}
      {pending && (
        <PendingConfirmation pending={pending} controller={controller} />
      )}
      {(operation?.captureDisabled || operation?.warning) && (
        <section
          className="configuration-operation-warning"
          role="status"
          aria-label="配置提交提示"
        >
          {operation.captureDisabled && (
            <>
              <Badge tone="warning">设备代理捕获已停用</Badge>
              <p>
                原生配置事务已停用代理捕获。设备选择仍保留，不会自动重新启用。
              </p>
              <p>
                {pending
                  ? "请先确认当前连接或恢复先前配置，再到代理页面检查并重新应用所选设备。"
                  : "如需继续代理，请到代理页面检查并重新应用所选设备。"}
              </p>
            </>
          )}
          {operation.warning && <p>{operation.warning}</p>}
        </section>
      )}
      <RiskDialog
        drafts={riskDrafts}
        generation={riskGeneration ?? controller.status?.generation ?? 0}
        open={riskDrafts.length > 0}
        onOpenChange={(open) => {
          if (!open) {
            setRiskDrafts([]);
            setRiskGeneration(undefined);
          }
        }}
        onCommit={() => {
          const ids = riskDrafts.map((draft) => draft.id);
          setRiskDrafts([]);
          setRiskGeneration(undefined);
          void controller.commit(true, ids);
        }}
      />
    </>
  );
}
