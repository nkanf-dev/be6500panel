import { useEffect, useMemo, useState } from "react";
import { FileCheck2 } from "lucide-react";
import {
  Button,
  ErrorState,
  Panel,
  PanelHeader,
} from "../components/ui/primitives";
import { api, errorMessage } from "../lib/api";
import type { FrpcPlanInput } from "../lib/contracts";
import { compileFrpcConfig } from "./frpc-config";
import { FrpcConnectionForm } from "./frpc-connection-form";
import { createFrpcProxy, FrpcTunnelEditor } from "./frpc-tunnel-editor";
import { useRuntime } from "./runtime/use-runtime";
import { RuntimeControls } from "./runtime/controls";
import { NativeConfigEditor } from "./runtime/native-config-editor";

const initial: FrpcPlanInput = {
  serverAddress: "",
  serverPort: 7000,
  tls: true,
  transport: "tcp",
  proxies: [createFrpcProxy(1)],
};

export function FrpcPage() {
  const runtime = useRuntime("frpc");
  const [input, setInput] = useState<FrpcPlanInput>(initial);
  const [token, setToken] = useState("");
  const [saved, setSaved] = useState(false);
  const [error, setError] = useState<unknown>();
  const [tab, setTab] = useState("form");
  const [editing, setEditing] = useState(false);
  const [expectedGeneration, setExpectedGeneration] = useState<number>();
  const currentGeneration = runtime.status?.generation;
  const generation = expectedGeneration ?? currentGeneration;
  const stale =
    expectedGeneration !== undefined &&
    currentGeneration !== undefined &&
    expectedGeneration !== currentGeneration;
  useEffect(() => {
    if (editing && currentGeneration !== undefined)
      setExpectedGeneration((previous) => previous ?? currentGeneration);
  }, [editing, currentGeneration]);
  function markEditing() {
    setEditing(true);
    if (currentGeneration !== undefined)
      setExpectedGeneration((previous) => previous ?? currentGeneration);
  }
  const preview = useMemo(() => {
    if (!input.serverAddress.trim()) return {};
    try {
      return { config: compileFrpcConfig(input, token ? "[令牌已隐藏]" : "") };
    } catch (cause) {
      return { error: cause };
    }
  }, [input, token]);
  const canConfigure =
    runtime.enabled &&
    !runtime.pending &&
    !runtime.loading &&
    !stale &&
    generation !== undefined &&
    runtime.status?.artifactAvailable === true &&
    preview.config !== undefined;
  function update<K extends keyof FrpcPlanInput>(
    key: K,
    value: FrpcPlanInput[K],
  ) {
    markEditing();
    setInput((previous) => ({ ...previous, [key]: value }));
    setError(undefined);
  }
  async function submit(event: React.FormEvent) {
    event.preventDefault();
    if (!canConfigure || generation === undefined) return;
    setError(undefined);
    try {
      const config = compileFrpcConfig(input, token);
      const accepted = await runtime.run(
        () => api.runtimeConfigure({ service: "frpc", config, generation }),
        "frpc 配置 Commit 完成，已校验并保存。可在运行管理中启动。",
      );
      if (accepted) {
        setEditing(false);
        setExpectedGeneration(undefined);
        setToken("");
        setSaved(true);
      }
    } catch (cause) {
      setError(cause);
    }
  }
  return (
    <div className="page-stack">
      <RuntimeControls runtime={runtime} />
      <div className="page-toolbar">
        <div className="segmented" role="tablist" aria-label="frpc 配置视图">
          {[
            { id: "form", label: "连接与映射" },
            { id: "native", label: "原生配置" },
          ].map((item) => (
            <button
              type="button"
              role="tab"
              id={`frpc-tab-${item.id}`}
              aria-controls={`frpc-panel-${item.id}`}
              aria-selected={tab === item.id}
              key={item.id}
              onClick={() => setTab(item.id)}
            >
              {item.label}
            </button>
          ))}
        </div>
        <span className="text-muted text-xs">
          映射 {input.proxies.length} / 64
        </span>
      </div>
      {tab === "native" ? (
        <div
          role="tabpanel"
          id="frpc-panel-native"
          aria-labelledby="frpc-tab-native"
        >
          <NativeConfigEditor runtime={runtime} />
        </div>
      ) : (
        <form
          className="page-stack"
          role="tabpanel"
          id="frpc-panel-form"
          aria-labelledby="frpc-tab-form"
          onSubmit={submit}
        >
          <FrpcConnectionForm
            input={input}
            token={token}
            disabled={runtime.pending}
            onChange={update}
            onTokenChange={(value) => {
              markEditing();
              setToken(value);
              setError(undefined);
            }}
          />
          <FrpcTunnelEditor
            proxies={input.proxies}
            disabled={runtime.pending}
            onChange={(proxies) => update("proxies", proxies)}
          />
          <Panel>
            <PanelHeader
              title="生成配置预览"
              subtitle="仅本地预览；令牌已隐藏。最终 Commit 才会发送私密原生配置并保存。"
            />
            <div className="config-form">
              {stale && (
                <div>
                  <ErrorState message="配置 generation 已变化 · generation_conflict。当前输入已保留。" />
                  <Button
                    type="button"
                    disabled={runtime.pending}
                    onClick={() => setExpectedGeneration(currentGeneration)}
                  >
                    使用最新 generation 审阅
                  </Button>
                </div>
              )}
              {preview.error !== undefined && (
                <ErrorState message={errorMessage(preview.error)} />
              )}
              <pre aria-label="frpc TOML 预览" className="mono wrap">
                {preview.config ?? "填写服务器和有效服务映射后生成 TOML 预览"}
              </pre>
              {error !== undefined && (
                <ErrorState message={errorMessage(error)} />
              )}
              {saved && (
                <p className="text-muted text-xs">
                  令牌输入已清空。再次 Commit
                  需显式填写令牌；留空会生成无令牌认证配置。要保留已保存的认证配置，请在原生配置中载入后编辑。
                </p>
              )}
              <div className="form-actions">
                <span className="text-muted text-xs">
                  {generation === undefined
                    ? "等待运行状态 generation；可刷新状态重试"
                    : !runtime.status?.artifactAvailable
                      ? "请先在运行管理中获取 frpc 运行文件，再执行原生校验"
                      : `POST /api/runtime/configure · generation ${generation}`}
                </span>
                <Button
                  variant="primary"
                  disabled={!canConfigure}
                  type="submit"
                >
                  <FileCheck2 size={15} />
                  生成并 Commit frpc 配置
                </Button>
              </div>
            </div>
          </Panel>
        </form>
      )}
    </div>
  );
}
