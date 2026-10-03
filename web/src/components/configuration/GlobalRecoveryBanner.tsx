import { useEffect, useRef, useState } from "react";
import { CheckCheck, RefreshCw, RotateCcw, X } from "lucide-react";
import { errorMessage } from "../../lib/api";
import { strings } from "../../locales/strings";
import { Badge, Button } from "../ui/primitives";
import { useGlobalRecovery } from "./GlobalRecoveryProvider";
import "./global-recovery.css";

export function GlobalRecoveryBanner() {
  const controller = useGlobalRecovery();
  const [now, setNow] = useState(Date.now);
  const operation = controller.status?.operation;
  const pending = controller.status?.pendingCommit;
  const phase = operation?.phase ?? (pending ? "pending" : undefined);
  const unknown = !operation && !pending && !!controller.lastUnresolved;
  const recovering = phase === "rolling_back" || phase === "applying";
  const errorCode = operation?.errorCode ?? controller.status?.errorCode;
  const deadline =
    phase === "pending"
      ? (operation?.deadline ?? pending?.deadline)
      : undefined;
  const deadlineTime = deadline ? Date.parse(deadline) : NaN;
  const remaining = Number.isFinite(deadlineTime)
    ? Math.max(0, Math.ceil((deadlineTime - now) / 1000))
    : 0;
  const countdown = useRef({ id: "", total: 1 });
  const id = operation?.id ?? pending?.id ?? controller.lastUnresolved?.id;
  if (countdown.current.id !== id)
    countdown.current = { id: id ?? "", total: Math.max(1, remaining) };
  useEffect(() => {
    setNow(Date.now());
    if (!deadline) return;
    const interval = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(interval);
  }, [deadline]);
  const visible =
    phase === "pending" ||
    recovering ||
    unknown ||
    !!errorCode ||
    !!controller.outcome ||
    !!controller.error;
  if (!visible) return null;
  const canConfirm =
    !controller.stale &&
    phase === "pending" &&
    remaining > 0 &&
    (operation ? operation.canConfirm : controller.status?.enabled);
  const canRollback =
    !controller.stale &&
    !unknown &&
    (operation
      ? operation.canRollback && (phase === "pending" || recovering)
      : !!pending);
  const busy = !!controller.busy;
  const title = controller.stale
    ? "连接中断，尚未核对配置恢复状态"
    : errorCode
      ? "上一配置恢复尚未完成，请检查后重试恢复"
      : unknown
        ? "未能确认配置操作结果，请重新核对"
        : phase === "rolling_back"
          ? "正在恢复上一配置"
          : phase === "applying"
            ? "正在应用配置，请等待状态核对"
            : phase === "pending"
              ? remaining > 0
                ? "新配置已临时应用，等待连接确认"
                : strings.dialogs.rollbackExpired
              : controller.outcome?.phase === "rolled_back"
                ? strings.configuration.restoredOutcome
                : controller.outcome?.phase === "committed"
                  ? strings.configuration.appliedOutcome
                  : "配置状态读取失败，请重新核对";
  const diagnostic =
    operation ?? controller.lastUnresolved ?? controller.outcome;
  return (
    <section
      className="global-recovery-banner"
      aria-label="全局配置恢复"
      data-phase={unknown ? "unknown" : phase}
    >
      <div className="global-recovery-summary" role="status" aria-live="polite">
        <Badge
          tone={
            errorCode || controller.stale
              ? "danger"
              : controller.outcome && !recovering && phase !== "pending"
                ? "success"
                : "warning"
          }
        >
          {title}
        </Badge>
        {phase === "pending" && (
          <>
            <strong role="timer" aria-label="自动恢复剩余时间">
              {remaining}s
            </strong>
            <progress
              aria-label="配置确认剩余时间"
              value={remaining}
              max={countdown.current.total}
            />
            <span>
              {remaining > 0
                ? "超时未确认将自动恢复上一配置。"
                : "尚未收到设备恢复完成的确认。"}
            </span>
          </>
        )}
        {(recovering || unknown || errorCode) && (
          <span>恢复完成前不会接受新的配置更改。请保持当前页面可连接。</span>
        )}
      </div>
      <div className="global-recovery-actions">
        <Button
          size="small"
          variant="ghost"
          disabled={busy || controller.refreshing}
          onClick={() => {
            void controller.refresh();
          }}
          aria-label="刷新全局配置状态"
        >
          <RefreshCw size={14} />
          {strings.actions.refresh}
        </Button>
        {(phase === "pending" || recovering) && (
          <>
            <Button
              size="small"
              disabled={busy || !canRollback}
              onClick={() => {
                void controller.rollback();
              }}
            >
              <RotateCcw size={14} />
              {controller.busy === "rollback"
                ? "正在恢复…"
                : strings.actions.restorePrevious}
            </Button>
            {phase === "pending" && (
              <Button
                size="small"
                variant="primary"
                disabled={busy || !canConfirm}
                onClick={() => {
                  void controller.confirm();
                }}
              >
                <CheckCheck size={14} />
                {controller.busy === "confirm"
                  ? "正在确认…"
                  : strings.actions.confirmWorking}
              </Button>
            )}
          </>
        )}
        {controller.outcome &&
          !recovering &&
          phase !== "pending" &&
          !unknown &&
          !errorCode && (
            <Button
              size="icon"
              variant="ghost"
              aria-label="关闭配置完成提示"
              onClick={controller.dismissOutcome}
            >
              <X size={14} />
            </Button>
          )}
      </div>
      {!!controller.error && (
        <p className="global-recovery-error" role="alert">
          {errorMessage(controller.error)}。未自动重试配置操作；请先核对状态。
        </p>
      )}
      {(diagnostic || id || errorCode) && (
        <details className="global-recovery-details">
          <summary>{strings.configuration.advancedDetails}</summary>
          {id && (
            <p>
              {strings.configuration.operationId}：<code>{id}</code>
            </p>
          )}
          {diagnostic && (
            <p>
              恢复阶段：<code>{diagnostic.phase}</code> ·{" "}
              {strings.configuration.configVersion}：
              <code>{diagnostic.generation}</code>
            </p>
          )}
          {errorCode && (
            <p>
              诊断代码：<code>{errorCode}</code>
            </p>
          )}
        </details>
      )}
    </section>
  );
}
