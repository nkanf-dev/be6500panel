import * as Tooltip from "@radix-ui/react-tooltip";
import { LoaderCircle } from "lucide-react";
import { Button } from "../../components/ui/primitives";
import { errorMessage } from "../../lib/api";
import {
  NODE_PROBE_MAX_NODES,
  NODE_PROBE_TARGET,
  type NodeProbeResult,
} from "./node-probe-contracts";
import type { NodeProbeController } from "./use-node-probes";
import "./node-probe.css";

const guidance: Readonly<Record<string, string>> = {
  empty_subscription: "暂无节点。请先导入订阅，再手动测速。",
  artifact_unavailable:
    "测速需要 sing-box 运行文件。请先在运行管理中获取运行文件。",
  probes_unavailable: "节点测速暂不可用。请检查控制服务与诊断日志。",
  probe_busy: "已有测速任务正在运行。请等待完成或停止测速。",
  revision_mismatch: "订阅节点已变化。请刷新节点列表后重新测速。",
  invalid_nodes: "测速节点已失效或数量不符合限制。请刷新节点列表后重试。",
  probe_closed: "测速服务已关闭。请检查控制服务与诊断日志。",
  core_unavailable: "临时测速核心未能启动。请检查运行文件与诊断日志。",
};
function failureCode(error: unknown) {
  return typeof error === "object" &&
    error !== null &&
    "code" in error &&
    typeof error.code === "string"
    ? error.code
    : undefined;
}

export function NodeProbeControls({
  controller,
  pageNodeIds,
  allCount,
  disabled = false,
}: {
  controller: NodeProbeController;
  pageNodeIds: readonly string[];
  allCount: number;
  disabled?: boolean;
}) {
  const ids = [...new Set(pageNodeIds)];
  const canStart = controller.canStart && !disabled;
  const maxNodes = controller.snapshot?.limits.maxNodes ?? NODE_PROBE_MAX_NODES;
  const unavailable = controller.snapshot?.available === false;
  const unavailableCode = controller.snapshot?.unavailableCode;
  const job = controller.stale ? undefined : controller.snapshot?.job;
  const total =
    controller.pending === "start" && controller.starting?.all
      ? allCount
      : controller.progress.total;
  const message =
    allCount === 0
      ? guidance.empty_subscription
      : unavailable
        ? (guidance[unavailableCode ?? ""] ?? guidance.probes_unavailable)
        : controller.stale
          ? "订阅节点已变化，旧测速结果已隐藏。请刷新节点列表后重新测速。"
          : controller.loading && !controller.snapshot
            ? "正在读取节点测速状态…"
            : !controller.snapshot
              ? "暂未读取到测速状态。请刷新重试。"
              : job?.status === "invalidated"
                ? "订阅已变化，本次测速已失效。请重新测速。"
                : job?.status === "cancelled"
                  ? "测速已停止。未完成的节点不会显示为成功。"
                  : job?.status === "failed"
                    ? (guidance[job.errorCode ?? ""] ??
                      "测速任务失败。请检查诊断日志后重试。")
                    : undefined;
  const code = failureCode(controller.error);
  return (
    <section className="node-probe-controls" aria-label="节点延迟测速">
      <div className="node-probe-actions">
        {controller.running ? (
          <>
            <span
              role="status"
              aria-live="polite"
              className="node-probe-progress"
            >
              <LoaderCircle size={14} className="spin" aria-hidden="true" />
              {controller.pending === "stop"
                ? "正在停止测速…"
                : `正在测速 ${controller.progress.completed}/${total}…`}
            </span>
            <Button
              type="button"
              size="small"
              className="node-probe-stop"
              disabled={controller.pending !== undefined}
              onClick={(event) => {
                event.stopPropagation();
                void controller.stop();
              }}
            >
              停止测速
            </Button>
          </>
        ) : (
          <>
            <Button
              type="button"
              size="small"
              variant="primary"
              disabled={!canStart || ids.length === 0 || ids.length > maxNodes}
              onClick={(event) => {
                event.stopPropagation();
                void controller.start({ all: false, nodeIds: ids });
              }}
            >
              测速当前页 ({ids.length})
            </Button>
            <Button
              type="button"
              size="small"
              disabled={!canStart || allCount === 0 || allCount > maxNodes}
              onClick={(event) => {
                event.stopPropagation();
                void controller.start({ all: true, nodeIds: [] });
              }}
            >
              测速全部 ({allCount})
            </Button>
          </>
        )}
        <Button
          type="button"
          size="small"
          variant="ghost"
          disabled={controller.loading || controller.pending !== undefined}
          onClick={(event) => {
            event.stopPropagation();
            controller.refresh();
          }}
        >
          刷新测速状态
        </Button>
      </div>
      <p className="node-probe-guidance text-muted text-xs">
        仅在点击后测速；使用独立临时核心，不切换当前节点，也不修改已保存配置。
        固定目标：{NODE_PROBE_TARGET}。并发 1，每节点最长 3000 ms。
      </p>
      {allCount > maxNodes && (
        <p className="node-probe-guidance text-muted text-xs">
          每次最多测速 {maxNodes} 个节点。当前共 {allCount}{" "}
          个，请使用“测速当前页”。
        </p>
      )}
      {allCount > 0 && ids.length === 0 && (
        <p className="node-probe-guidance text-muted text-xs">
          当前页没有节点。可调整筛选或测速全部节点。
        </p>
      )}
      {message && (
        <p className="node-probe-guidance text-muted text-xs" role="status">
          {message}
        </p>
      )}
      {controller.error !== undefined && (
        <p className="node-probe-error" role="alert">
          {code && guidance[code]
            ? `${guidance[code]} · ${code}`
            : errorMessage(controller.error)}
        </p>
      )}
    </section>
  );
}

function presentation(result?: NodeProbeResult) {
  if (!result) return { label: "-- ms", tone: "unknown", busy: false };
  if (result.status === "queued")
    return { label: "等待测速…", tone: "pending", busy: true };
  if (result.status === "probing")
    return { label: "测速中…", tone: "pending", busy: true };
  if (result.status === "timeout")
    return { label: "超时", tone: "failure", busy: false };
  if (result.status === "unreachable")
    return { label: "不可达", tone: "failure", busy: false };
  if (result.status !== "success" || result.delayMs === undefined)
    return { label: "-- ms", tone: "unknown", busy: false };
  return {
    label: `${Math.round(result.delayMs)} ms`,
    tone:
      result.delayMs < 120 ? "fast" : result.delayMs <= 300 ? "medium" : "slow",
    busy: false,
  };
}

/** Place beside the node selection button, never inside a button or radio label. */
export function NodeProbeBadge({
  controller,
  nodeId,
  nodeLabel,
  disabled = false,
}: {
  controller: NodeProbeController;
  nodeId: string;
  nodeLabel: string;
  disabled?: boolean;
}) {
  const result = controller.results.find((item) => item.nodeId === nodeId);
  const starting =
    controller.pending === "start" &&
    (controller.starting?.all || controller.starting?.nodeIds.includes(nodeId));
  const view = starting
    ? { label: "测速中…", tone: "pending", busy: true }
    : presentation(result);
  const time = result?.measuredAt
    ? new Date(result.measuredAt).toLocaleTimeString("zh-CN", { hour12: false })
    : "尚未测速";
  const detail = `${nodeLabel} · 时延：${view.label} · 目标：${NODE_PROBE_TARGET} · 测试时间：${time}${result?.measuredAt ? ` (${result.measuredAt})` : ""}${result?.status === "cancelled" ? " · 测速已取消" : ""}${result?.errorCode ? ` · ${result.errorCode}` : ""} · 单独测速此节点`;
  return (
    <Tooltip.Provider delayDuration={350}>
      <Tooltip.Root>
        <Tooltip.Trigger asChild>
          <Button
            type="button"
            size="small"
            className={`node-probe-badge node-probe-badge-${view.tone}`}
            aria-label={`测速节点 ${nodeLabel}：${view.label}`}
            aria-busy={view.busy}
            title={detail}
            disabled={disabled || !controller.canStart}
            onPointerDown={(event) => event.stopPropagation()}
            onKeyDown={(event) => event.stopPropagation()}
            onKeyUp={(event) => event.stopPropagation()}
            onClick={(event) => {
              event.stopPropagation();
              void controller.start({ all: false, nodeIds: [nodeId] });
            }}
          >
            {view.busy && (
              <LoaderCircle size={12} className="spin" aria-hidden="true" />
            )}
            {view.label}
          </Button>
        </Tooltip.Trigger>
        <Tooltip.Portal>
          <Tooltip.Content
            className="tooltip node-probe-tooltip"
            sideOffset={6}
          >
            {detail}
            <Tooltip.Arrow />
          </Tooltip.Content>
        </Tooltip.Portal>
      </Tooltip.Root>
    </Tooltip.Provider>
  );
}
