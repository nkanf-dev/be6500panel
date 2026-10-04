import { Effect } from "effect";
import { useEffect, useRef, useState } from "react";
import {
  Badge,
  Button,
  ErrorState,
  Panel,
  PanelHeader,
} from "../../components/ui/primitives";
import { api, errorMessage, runRequest } from "../../lib/api";
import type { ProxyNodes } from "../../lib/contracts";
import { useResource } from "../../lib/use-resource";
import type { RuntimeController } from "../runtime/use-runtime";
import { nodeProbeApi } from "./node-probe-api";

export type GatewaySetup = "runtime" | "subscription" | "node";

export function GatewayPanel({
  runtime,
  nodes,
  nodesLoading = false,
  pending: otherPending = false,
  refreshVersion = 0,
  onPending,
  onSetup,
}: {
  runtime: RuntimeController;
  nodes?: ProxyNodes;
  nodesLoading?: boolean;
  pending?: boolean;
  refreshVersion?: number;
  onPending: (pending: boolean) => void;
  onSetup: (setup: GatewaySetup) => void;
}) {
  const observation = useResource(api.proxyCapture);
  // One read-only snapshot for the compact card; never starts a probe job.
  const probes = useResource(nodeProbeApi.snapshot);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<unknown>();
  const submitting = useRef(false);
  const capture =
    observation.error === undefined ? observation.data : undefined;
  const status = runtime.status;
  const node = nodes?.nodes.find((item) => item.id === nodes.selectedNodeId);
  const busy = pending || otherPending || runtime.pending;
  const declared = capture?.lanIPv4Prefixes;
  const installed = capture?.installedLanIPv4Prefixes;
  const scopeChanged =
    capture?.scopeState === "changed" ||
    capture?.scopeState === "unresolved" ||
    capture?.state === "scope-changed" ||
    (declared !== undefined &&
      installed !== undefined &&
      capture?.active &&
      (declared.length !== installed.length ||
        declared.some((prefix) => !installed.includes(prefix))));
  const suspendedGateway =
    capture?.scope === "gateway" &&
    !capture.active &&
    capture.state === "suspended" &&
    !installed?.length;
  const withdrawing = !!(
    capture?.active ||
    (capture?.desired && !suspendedGateway) ||
    capture?.cleanupPending ||
    capture?.error
  );
  const active =
    capture?.scope === "gateway" &&
    capture.active &&
    !!installed?.length &&
    !scopeChanged &&
    !capture.cleanupPending &&
    !capture.error &&
    (capture.state === undefined || capture.state === "active") &&
    status?.state === "running" &&
    runtime.error === undefined &&
    error === undefined;
  const inactive =
    capture !== undefined &&
    !withdrawing &&
    (capture.state === undefined ||
      capture.state === "inactive" ||
      suspendedGateway);
  const stateLabel = capture?.cleanupPending
    ? "待撤回"
    : capture?.error || scopeChanged
      ? "接管异常"
      : active
        ? "运行中"
        : inactive
          ? "已停用"
          : capture?.active && capture.scope !== "gateway"
            ? "设备接管中"
            : observation.loading
              ? "读取中"
              : "状态未知";
  const currentProbe =
    probes.error === undefined && probes.data?.revision === nodes?.revision
      ? probes.data?.results.find((item) => item.nodeId === node?.id)
      : undefined;
  const delay =
    currentProbe?.status === "success" && currentProbe.delayMs !== undefined
      ? `${Math.round(currentProbe.delayMs)} ms`
      : "未测";
  const setup =
    !runtime.loading && runtime.enabled && status
      ? !status.artifactAvailable
        ? {
            kind: "runtime" as const,
            message: "先获取校验过的 sing-box 运行文件",
            action: "获取运行文件",
          }
        : nodes && !nodes.nodes.length
          ? {
              kind: "subscription" as const,
              message: "先导入订阅，再选择节点",
              action: "导入订阅",
            }
          : nodes && (!node || !status.configured)
            ? {
                kind: "node" as const,
                message: "先选择节点并保存配置",
                action: "选择并保存节点",
              }
            : undefined
      : undefined;
  const canEnable =
    !busy &&
    inactive &&
    !observation.loading &&
    !nodesLoading &&
    runtime.enabled &&
    !runtime.loading &&
    !!node &&
    !!status?.artifactAvailable &&
    status.configured &&
    (status.state === "running" || status.state === "stopped");

  useEffect(() => {
    observation.reload();
  }, [
    refreshVersion,
    status?.state,
    status?.generation,
    status?.restarts,
    status?.pid,
    observation.reload,
  ]);
  useEffect(() => {
    probes.reload();
  }, [refreshVersion, nodes?.revision, nodes?.selectedNodeId, probes.reload]);

  async function change(enable: boolean) {
    if (submitting.current || busy || (enable && !canEnable)) return;
    submitting.current = true;
    setPending(true);
    onPending(true);
    setError(undefined);
    try {
      if (enable && status?.state !== "running") {
        let running = false;
        const started = await runtime.run(
          () =>
            api.runtimeStart("sing-box").pipe(
              Effect.map((next) => {
                running = next.state === "running";
                return next;
              }),
            ),
          "核心已启动",
        );
        if (!started || !running) {
          setError(new Error("核心未能运行，请检查运行管理后重试。"));
          return;
        }
      }
      await runRequest(
        enable
          ? api.proxyCaptureApply({ scope: "gateway", ipv6: "direct" })
          : api.proxyCaptureDisable(),
      );
    } catch (cause) {
      setError(cause);
    } finally {
      observation.reload();
      runtime.refresh();
      submitting.current = false;
      setPending(false);
      onPending(false);
    }
  }

  return (
    <Panel aria-label="网关路由代理">
      <PanelHeader
        title="网关路由代理"
        action={
          <Badge
            tone={
              active
                ? "success"
                : capture?.error || capture?.cleanupPending || scopeChanged
                  ? "danger"
                  : "neutral"
            }
          >
            {stateLabel}
          </Badge>
        }
      />
      <div className="config-form compact-form">
        <dl className="key-values">
          <div>
            <dt>当前节点</dt>
            <dd>{node?.label ?? (nodes ? "未选择" : "状态未知")}</dd>
          </div>
          <div>
            <dt>当前延迟</dt>
            <dd>{delay}</dd>
          </div>
          <div>
            <dt>分流配置</dt>
            <dd>
              {status
                ? status.configured
                  ? "规则分流"
                  : "未配置"
                : "状态未知"}
            </dd>
          </div>
          <div>
            <dt>接管范围</dt>
            <dd>
              {installed?.length ? (
                <div>
                  已安装：<span className="mono">{installed.join("、")}</span>
                </div>
              ) : (
                <div>{inactive ? "未接管" : "范围未知"}</div>
              )}
              {declared !== undefined && (
                <div>
                  当前声明：
                  <span className="mono">
                    {declared.length ? declared.join("、") : "无"}
                  </span>
                </div>
              )}
            </dd>
          </div>
          <div>
            <dt>IPv6</dt>
            <dd>
              {capture?.ipv6 && capture.ipv6 !== "direct"
                ? capture.ipv6 === "follow"
                  ? "跟随代理（设备诊断）"
                  : "阻断（设备诊断）"
                : "直连"}
            </dd>
          </div>
        </dl>
        {setup && !withdrawing && (
          <div className="form-actions">
            <span>{setup.message}</span>
            <Button disabled={busy} onClick={() => onSetup(setup.kind)}>
              {setup.action}
            </Button>
          </div>
        )}
        {!runtime.enabled &&
          !runtime.loading &&
          runtime.error === undefined && <p>服务端未启用运行管理</p>}
        {!status &&
          !runtime.loading &&
          runtime.enabled &&
          runtime.error === undefined && (
            <ErrorState
              message="未返回核心运行状态，请重新读取。"
              onRetry={busy ? undefined : runtime.refresh}
            />
          )}
        {runtime.error !== undefined && (
          <ErrorState
            message={errorMessage(runtime.error)}
            onRetry={busy ? undefined : runtime.refresh}
          />
        )}
        {observation.error !== undefined && (
          <ErrorState
            message={errorMessage(observation.error)}
            onRetry={busy ? undefined : observation.reload}
          />
        )}
        {capture?.cleanupPending && (
          <ErrorState
            message="接管规则尚未撤回，请重试撤回。"
            onRetry={busy ? undefined : () => void change(false)}
          />
        )}
        {capture?.error && (
          <ErrorState
            message={capture.error}
            onRetry={busy ? undefined : () => void change(false)}
          />
        )}
        {scopeChanged && !capture?.error && (
          <ErrorState
            message="当前声明与已安装范围不一致，请先撤回接管。"
            onRetry={busy ? undefined : () => void change(false)}
          />
        )}
        {error !== undefined && (
          <ErrorState
            message={errorMessage(error)}
            onRetry={
              withdrawing && !busy ? () => void change(false) : undefined
            }
          />
        )}
        <div className="form-actions">
          <Button
            size="small"
            disabled={busy || nodesLoading}
            onClick={() => onSetup("node")}
          >
            更换节点
          </Button>
          <Button
            variant={withdrawing ? "secondary" : "primary"}
            disabled={busy || (!withdrawing && !canEnable)}
            onClick={() => void change(!withdrawing)}
          >
            {pending
              ? "处理中…"
              : withdrawing
                ? capture?.cleanupPending || capture?.error || scopeChanged
                  ? "重试撤回"
                  : "关闭"
                : "开启网关代理"}
          </Button>
        </div>
      </div>
    </Panel>
  );
}
