import { strings } from "../../locales/strings";
import {
  Badge,
  Button,
  ErrorState,
  Loading,
  Panel,
  PanelHeader,
} from "../../components/ui/primitives";
import { api, errorMessage } from "../../lib/api";
import { bytes } from "../../lib/format";
import { ArtifactInputs } from "./artifact-inputs";
import type { RuntimeController } from "./use-runtime";

const stateLabels: Record<string, string> = {
  running: strings.states.running,
  stopped: "已停止",
  starting: "启动中",
  stopping: "停止中",
  failed: "故障",
  backoff: "等待重试",
  notconfigured: "待配置",
  downloading: "下载中",
  rebuilding: "重建中",
  checking: "校验中",
  error: "故障",
  unavailable: "未就绪",
};
export function RuntimeControls({ runtime }: { runtime: RuntimeController }) {
  const status = runtime.status;
  return (
    <Panel>
      <PanelHeader
        title={`${runtime.service} 运行管理`}
        subtitle={status ? "配置与核心运行状态" : "读取进程状态"}
        action={
          <Button
            size="small"
            onClick={runtime.refresh}
            disabled={runtime.pending || runtime.loading}
          >
            {strings.actions.refresh}
          </Button>
        }
      />
      {runtime.loading && !status ? (
        <Loading />
      ) : (
        <dl className="key-values">
          <div>
            <dt>状态</dt>
            <dd>
              <Badge
                tone={
                  status?.state === "running"
                    ? "success"
                    : status?.errorCode
                      ? "danger"
                      : "neutral"
                }
              >
                {status
                  ? stateLabels[status.state] || status.state
                  : "暂无状态"}
              </Badge>
            </dd>
          </div>
          <div>
            <dt>运行文件 / 配置</dt>
            <dd>
              {status?.artifactAvailable ? "已获取" : "待获取"} /{" "}
              {status?.configured ? "已验证" : "待配置"}
            </dd>
          </div>
          <div>
            <dt>进程 / RSS</dt>
            <dd>
              {status?.pid || "—"} /{" "}
              {status?.rssAvailable ? bytes(status.rssBytes) : "—"}
            </dd>
          </div>
          <div>
            <dt>目标 / 重启</dt>
            <dd>
              {status ? (status.desired ? "运行" : "停止") : "—"} /{" "}
              {status?.restarts ?? "—"}
            </dd>
          </div>
          {status?.state === "backoff" && status.retryAt && (
            <div>
              <dt>下次重试</dt>
              <dd>{status.retryAt}</dd>
            </div>
          )}
          {status && (
            <details className="configuration-advanced-details">
              <summary>高级诊断</summary>
              <p>{`generation ${status.generation} · ${status.version || "版本未登记"}`}</p>
              {status.errorCode && <p>状态码：<code>{status.errorCode}</code></p>}
            </details>
          )}
          {status?.recoveryPlan && (
            <div>
              <dt>恢复步骤</dt>
              <dd className="wrap">{status.recoveryPlan.join(" · ")}</dd>
            </div>
          )}
        </dl>
      )}
      {status?.restored && (
        <p className="configuration-inline-warning" role="status">
          本次更改未能运行，已恢复上一可运行配置。
        </p>
      )}
      {status?.needsRecovery && (
        <p className="configuration-inline-warning" role="alert">
          恢复尚未完成。请检查运行状态；客户端接管不会被视为已恢复。
        </p>
      )}
      {runtime.error !== undefined && (
        <ErrorState message={errorMessage(runtime.error)} />
      )}
      {runtime.result && (
        <p className="panel-bottom" role="status">
          {runtime.result}
        </p>
      )}
      {!runtime.enabled && !runtime.loading && (
        <p className="panel-bottom text-muted">运行管理未启用</p>
      )}
      <div className="config-form compact-form">
        <div className="form-actions">
          <span className="text-muted text-xs">原生校验通过后启动</span>
          <div>
            <Button
              disabled={
                !runtime.enabled ||
                runtime.pending ||
                !status?.configured ||
                !status?.artifactAvailable ||
                status?.state === "running"
              }
              onClick={() =>
                void runtime.run(
                  () => api.runtimeStart(runtime.service),
                  "启动请求已接受",
                )
              }
            >
              启动 {runtime.service}
            </Button>{" "}
            <Button
              disabled={!runtime.enabled || runtime.pending || !status}
              onClick={() =>
                void runtime.run(
                  () => api.runtimeStop(runtime.service),
                  runtime.service === "sing-box"
                    ? "进程已停止，所属接管规则已清理"
                    : "frpc 进程已停止",
                )
              }
            >
              停止 {runtime.service}
            </Button>
          </div>
        </div>
      </div>
      <ArtifactInputs runtime={runtime} />
    </Panel>
  );
}
