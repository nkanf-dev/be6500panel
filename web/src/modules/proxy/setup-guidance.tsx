import { strings } from "../../locales/strings";
import { useConsole } from "../../app/console-context";
import {
  Badge,
  Button,
  ErrorState,
  Panel,
  PanelHeader,
} from "../../components/ui/primitives";
import { errorMessage } from "../../lib/api";
import type { ProxyNodes } from "../../lib/contracts";
import type { RuntimeController } from "../runtime/use-runtime";

export function ProxySetupGuidance({
  runtime,
  nodes,
  busy,
  onRuntime,
  onCapture,
}: {
  runtime: RuntimeController;
  nodes?: ProxyNodes;
  busy: boolean;
  onRuntime: () => void;
  onCapture: () => void;
}) {
  const { health } = useConsole();
  const status =
    runtime.enabled || runtime.error === undefined ? runtime.status : undefined;
  const next =
    runtime.loading && !status
      ? "正在读取运行管理能力"
      : !runtime.enabled
        ? health?.mode === "demo"
          ? "演示模式不执行代理配置或运行操作"
          : "服务端未启用运行管理；需要持久数据目录和访问密码，启用后重新读取状态"
        : !status
          ? "未返回 sing-box 运行状态；请重新读取状态或查看运行管理诊断"
          : !status.artifactAvailable
            ? "先获取校验过的 sing-box 运行文件"
            : !nodes?.nodes.length
              ? strings.proxy.setup.nextImport
              : !status.configured
                ? strings.proxy.setup.nextSelect
                : status.state !== "running"
                  ? strings.proxy.setup.nextStart
                  : "核心运行中；需要透明代理时再单独审阅一个客户端的接管";
  return (
    <Panel aria-label="代理设置步骤">
      <PanelHeader
        title={strings.proxy.setup.title}
        subtitle={strings.proxy.setup.subtitle}
      />
      <div className="config-form compact-form">
        {runtime.error !== undefined ? (
          <>
            <ErrorState message={errorMessage(runtime.error)} />
            <p className="text-muted">
              {runtime.enabled
                ? "前次操作失败。查看运行管理中的当前状态后再决定下一步；不会自动重放写操作。"
                : "运行状态未确认；保留当前输入，重新读取后再操作。"}
            </p>
            {!runtime.enabled && (
              <Button
                type="button"
                size="small"
                disabled={busy || runtime.loading}
                onClick={runtime.refresh}
              >
                重新读取运行状态
              </Button>
            )}
          </>
        ) : (
          <p role="status">{next}</p>
        )}
        <ol>
          <li>
            运行文件：
            <Badge>
              {status
                ? status.artifactAvailable
                  ? "已获取"
                  : "待获取"
                : strings.dashboard.states.unknown}
            </Badge>{" "}
            在运行管理中获取适配设备的运行文件，下载后先校验再使用。
          </li>
          <li>
            导入并选择节点：
            <Badge>
              {nodes ? `${nodes.nodes.length} 个节点` : "读取中"}
            </Badge>{" "}
            当前只支持 Clash YAML 中的 VLESS / TCP 节点，不是任意订阅格式转换。
          </li>
          <li>
            应用更改：
            <Badge>
              {status
                ? status.configured
                  ? "已有已校验配置"
                  : "待配置"
                : strings.dashboard.states.unknown}
            </Badge>{" "}
            更换节点后需再次应用更改。核心未运行时只保存配置；预览策略不影响这里。服务端还需预置已校验的本地规则集，浏览器尚无规则集下载入口。
          </li>
          <li>
            启动核心：
            <Badge>
              {status?.state === "running"
                ? strings.states.running
                : status
                  ? strings.states.stopped
                  : strings.dashboard.states.unknown}
            </Badge>{" "}
            启动只验证本地监听，不代表订阅服务器或互联网已连通。
          </li>
          <li>
            可选客户端接管：手动指定一个客户端，审阅后另行确认。未接管时 LAN
            不自动使用代理。
          </li>
        </ol>
        <details>
          <summary>高级诊断</summary>
          <p className="text-muted text-xs">
            本地规则集缺失时返回 rules_unavailable；运行文件下载需校验
            SHA-256。应用更改成功不代表远端节点已连通。
          </p>
        </details>
        <div className="form-actions">
          <Button
            type="button"
            size="small"
            disabled={busy}
            onClick={onRuntime}
          >
            {status?.artifactAvailable ? "打开运行管理" : "获取运行文件"}
          </Button>
          <Button
            type="button"
            size="small"
            disabled={busy || !runtime.enabled || status?.state !== "running"}
            onClick={onCapture}
          >
            查看客户端接管
          </Button>
        </div>
      </div>
    </Panel>
  );
}
