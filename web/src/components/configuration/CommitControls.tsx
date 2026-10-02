import { useEffect, useState } from "react";
import * as Dialog from "@radix-ui/react-dialog";
import {
  Check,
  CheckCheck,
  CheckCircle2,
  RefreshCw,
  RotateCcw,
  X,
} from "lucide-react";
import { Badge, Button } from "../ui/primitives";
import { changedFields, nativeSections } from "./native-document";
import { fieldSchema, sectionSchema } from "./field-schema";
import type { ConfigurationController } from "./use-configuration";
import {
  configurationModules,
  type ConfigurationDraft,
  type PendingCommit,
} from "./contracts";

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
    <section className="configuration-pending" aria-label="更改连接确认">
      <div className="configuration-pending-summary">
        <Badge tone="warning">待确认生效</Badge>
        <strong role="timer" aria-label="连接确认剩余时间">
          {remaining}s
        </strong>
        <span>临时应用</span>
        <span>
          {expired
            ? "确认期限已到，正在核对恢复状态"
            : `配置已应用，正在等待连接确认（剩余${remaining}s）。超时未确认将自动恢复上一配置。`}
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
          aria-label="刷新应用状态"
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
          {controller.busy === "rollback" ? "正在恢复…" : "恢复上一配置"}
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
          {controller.busy === "confirm" ? "正在确认…" : "确认生效"}
        </Button>
      </div>
      <span className="configuration-pending-meta">
        确认期限 {new Date(pending.deadline).toLocaleTimeString()} · 每 10
        秒核对状态
      </span>
      <details className="configuration-advanced-details">
        <summary>高级详情</summary>
        <p>
          操作标识：<code>{pending.id}</code>
        </p>
      </details>
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
  const readableFields = fields.map((field) => {
    const [, module, kind, name] = field.match(
      /^(\S+) \/ (option|list|config) (.+)$/,
    )!;
    const matchingDrafts = drafts.filter((draft) => draft.module === module);
    const draft = matchingDrafts[0]!;
    const sections = matchingDrafts.flatMap((item) =>
      nativeSections(
        item.diff
          .split("\n")
          .filter((line) => /^[ +\-]/.test(line) && !/^(\+\+\+|---)/.test(line))
          .map((line) => line.slice(1))
          .join("\n"),
      ),
    );
    const section = sections.find((item) =>
      item.fields.some((item) => item.name === name),
    );
    const label =
      kind === "config"
        ? sectionSchema(draft.module, name).label
        : fieldSchema(draft.module, section?.type ?? "", name).label;
    const document = configurationModules.find(
      (item) => item.module === draft.module,
    )!.label;
    return { field, label: `${document} · ${label}` };
  });
  return (
    <Dialog.Root open={open} onOpenChange={onOpenChange}>
      <Dialog.Portal>
        <Dialog.Overlay className="configuration-dialog-overlay" />
        <Dialog.Content className="configuration-risk-dialog">
          <div className="configuration-dialog-header">
            <div>
              <Badge tone="danger">高风险更改</Badge>
              <Dialog.Title>确认应用高风险更改</Dialog.Title>
            </div>
            <Dialog.Close asChild>
              <Button variant="ghost" size="icon" aria-label="关闭风险确认">
                <X size={16} />
              </Button>
            </Dialog.Close>
          </div>
          <Dialog.Description>
            将应用 {drafts.length}{" "}
            个配置文档的更改。涉及管理连接的更改会先临时应用，需要在期限内确认生效；未确认时，设备将自动恢复上一配置。
          </Dialog.Description>
          <section className="configuration-changed-fields">
            <h3>涉及配置</h3>
            <ul>
              {configurationModules
                .filter((item) =>
                  drafts.some((draft) => draft.module === item.module),
                )
                .map((item) => (
                  <li key={item.module}>{item.label}</li>
                ))}
            </ul>
          </section>
          {readableFields.length > 0 && (
            <section className="configuration-changed-fields">
              <h3>更改字段</h3>
              <ul>
                {readableFields.map((item) => (
                  <li key={item.field}>{item.label}</li>
                ))}
              </ul>
            </section>
          )}
          <ul className="configuration-diagnostics configuration-risks">
            {risks.map((risk) => (
              <li key={`${risk.code}:${risk.message}`}>
                <span>{risk.message}</span>
              </li>
            ))}
          </ul>
          <details className="configuration-advanced-details">
            <summary>高级详情</summary>
            <p>
              配置版本：<code>{generation}</code>
            </p>
            <ul className="configuration-diagnostics">
              {drafts.map((draft) => (
                <li key={draft.id}>
                  检查标识：<code>{draft.id}</code>
                </li>
              ))}
              {risks.map((risk) => (
                <li key={`${risk.code}:${risk.message}`}>
                  风险代码：<code>{risk.code}</code>
                </li>
              ))}
            </ul>
            {!!fields.length && (
              <section className="configuration-changed-fields">
                <h3>原生变更字段</h3>
                <ul>
                  {fields.map((field) => (
                    <li key={field}>
                      <code>{field}</code>
                    </li>
                  ))}
                </ul>
              </section>
            )}
          </details>
          <footer className="configuration-dialog-footer">
            <Dialog.Close asChild>
              <Button>返回检查</Button>
            </Dialog.Close>
            <Button className="configuration-danger-button" onClick={onCommit}>
              <CheckCircle2 size={15} />
              确认并应用
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
            <CheckCircle2 size={16} />
            应用更改
          </strong>
          <span>
            {selected.length} 个已选更改 · {controller.drafts.length} 个检查结果
            {hasRisks ? " · 包含高风险更改" : ""}
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
          aria-label={`应用已选更改 (${selected.length})`}
          onClick={() => {
            if (hasRisks) {
              setRiskDrafts(selected);
              setRiskGeneration(controller.status?.generation);
            } else void controller.commit(false);
          }}
        >
          <CheckCircle2 size={15} />
          {controller.busy === "commit"
            ? "正在应用…"
            : `应用更改 (${selected.length})`}
        </Button>
      </div>
      {operation && !pending && (
        <div className="configuration-operation-result" role="status">
          {operation.state === "pending_confirmation" ? (
            <RefreshCw size={15} />
          ) : (
            <Check size={15} />
          )}
          <span>
            {operation.state === "rolled_back"
              ? "已恢复上一配置"
              : operation.state === "committed"
                ? "更改已生效"
                : "已核对当前配置，请检查更改结果"}
            {operation.changedModules.length > 0 &&
              ` · ${operation.changedModules
                .map(
                  (module) =>
                    configurationModules.find((item) => item.module === module)!
                      .label,
                )
                .join("、")}`}
          </span>
          <details className="configuration-advanced-details">
            <summary>高级详情</summary>
            <p>
              操作标识：<code>{operation.id}</code>
            </p>
            <p>
              配置版本：<code>{operation.generation}</code>
            </p>
          </details>
        </div>
      )}
      {pending && (
        <PendingConfirmation pending={pending} controller={controller} />
      )}
      {(operation?.captureDisabled || operation?.warning) && (
        <section
          className="configuration-operation-warning"
          role="status"
          aria-label="配置应用提示"
        >
          {operation.captureDisabled && (
            <>
              <Badge tone="warning">设备代理捕获已停用</Badge>
              <p>本次更改已停用代理捕获。设备选择仍保留，不会自动重新启用。</p>
              <p>
                {pending
                  ? "请先确认生效或恢复上一配置，再到代理页面检查并重新应用所选设备。"
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
