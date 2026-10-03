import { strings } from "../locales/strings";
import { useEffect, useMemo, useRef, useState } from "react";
import { FileCheck2 } from "lucide-react";
import {
  Button,
  ErrorState,
  Panel,
  PanelHeader,
} from "../components/ui/primitives";
import { api, errorMessage, runRequest } from "../lib/api";
import type { FrpcPlanInput } from "../lib/contracts";
import { compileFrpcConfig } from "./frpc-config";
import {
  frpcProxySourceId,
  patchFrpcDocument,
  readFrpcDocument,
  rebaseFrpcInput,
  redactFrpcDocument,
  type FrpcTokenIntent,
} from "./frpc-document";
import {
  acceptFrpcFormSession,
  frpcFormSessionRevision,
  emptyFrpcFormSession,
  frpcFormSessionEpoch,
  readFrpcFormSession,
  subscribeFrpcFormSessionClear,
  writeFrpcFormSession,
  type FrpcFormSession,
} from "./frpc-session";
import { FrpcConnectionForm } from "./frpc-connection-form";
import { FrpcTunnelEditor } from "./frpc-tunnel-editor";
import { useRuntime } from "./runtime/use-runtime";
import { RuntimeControls } from "./runtime/controls";
import { NativeConfigEditor } from "./runtime/native-config-editor";

export function FrpcPage() {
  const runtime = useRuntime("frpc");
  const [session, setSession] = useState(readFrpcFormSession);
  const current = useRef(session);
  const epoch = useRef(frpcFormSessionEpoch());
  const lifetime = useRef<AbortController | undefined>(undefined);
  const requestId = useRef(0);
  const [loading, setLoading] = useState(false);
  const [loadFailed, setLoadFailed] = useState(false);
  const [saved, setSaved] = useState(false);
  const [error, setError] = useState<unknown>();
  const [tab, setTab] = useState("form");
  const [discard, setDiscard] = useState(false);
  const currentGeneration = runtime.status?.generation;
  const generation = session.generation ?? currentGeneration;
  const stale =
    session.generation !== undefined &&
    currentGeneration !== undefined &&
    session.generation !== currentGeneration;
  const unsupported =
    session.document && !session.document.supported
      ? session.document.reason
      : undefined;
  const needsReadback =
    runtime.status?.configured === true && !session.document;
  const { input, token } = session;
  function replace(next: FrpcFormSession) {
    current.current = next;
    writeFrpcFormSession(next, epoch.current);
    setSession(next);
  }
  useEffect(() => {
    lifetime.current = new AbortController();
    const unsubscribe = subscribeFrpcFormSessionClear(() => {
      lifetime.current?.abort();
      lifetime.current = new AbortController();
      requestId.current++;
      epoch.current = frpcFormSessionEpoch();
      replace(emptyFrpcFormSession());
      setLoading(false);
      setLoadFailed(false);
      setError(undefined);
      setSaved(false);
      setDiscard(false);
    });
    return () => {
      unsubscribe();
      lifetime.current?.abort();
      lifetime.current = undefined;
    };
  }, []);
  useEffect(() => {
    if (
      !runtime.enabled ||
      runtime.loading ||
      runtime.pending ||
      currentGeneration === undefined ||
      loading ||
      loadFailed
    )
      return;
    const active = current.current;
    if (active.dirty) {
      if (active.generation === undefined)
        replace({ ...active, generation: currentGeneration });
      return;
    }
    if (
      runtime.status?.configured &&
      (!active.document || active.generation !== currentGeneration)
    )
      void fetchAccepted("load");
  }, [
    currentGeneration,
    runtime.enabled,
    runtime.loading,
    runtime.pending,
    runtime.status?.configured,
  ]);

  async function fetchAccepted(action: "load" | "merge" | "discard") {
    const signal = lifetime.current?.signal;
    const requestEpoch = epoch.current;
    const id = ++requestId.current;
    const before = current.current;
    const active = () =>
      !!signal &&
      !signal.aborted &&
      lifetime.current?.signal === signal &&
      requestEpoch === frpcFormSessionEpoch() &&
      id === requestId.current;
    setLoading(true);
    setError(undefined);
    setLoadFailed(false);
    try {
      const response = await runRequest(api.runtimeConfig("frpc"), signal);
      if (!active()) return;
      // Never overwrite input typed while a read was pending.
      if (current.current !== before) return;
      const document = readFrpcDocument(response.config);
      if (action === "merge" && before.dirty) {
        if (!document.supported)
          throw new Error(
            `最新配置不能安全合并：${document.reason}。当前输入已保留。`,
          );
        if (before.document && !before.document.supported)
          throw new Error("当前配置只能使用原生编辑器");
        const baseline = before.document?.supported
          ? before.document
          : {
              ...document,
              source: "",
              input: emptyFrpcFormSession().input,
              mappings: [],
              statements: [],
              tables: [],
            };
        const rebased = rebaseFrpcInput(baseline, before.input, document);
        // Keep latest credentials by default. Explicit credential replacement/clear remains local intent.
        replace({
          ...before,
          document,
          input: rebased,
          generation: response.generation,
        });
      } else {
        replace({
          service: "frpc",
          generation: response.generation,
          document,
          input: document.supported
            ? document.input
            : emptyFrpcFormSession().input,
          token: { mode: "preserve", value: "" },
          dirty: false,
        });
      }
      setSaved(false);
    } catch (cause) {
      if (active()) {
        setError(cause);
        setLoadFailed(true);
      }
    } finally {
      if (active()) setLoading(false);
    }
  }
  function update<K extends keyof FrpcPlanInput>(
    key: K,
    value: FrpcPlanInput[K],
  ) {
    replace({
      ...current.current,
      input: { ...current.current.input, [key]: value },
      generation: current.current.generation ?? currentGeneration,
      dirty: true,
    });
    setSaved(false);
    setError(undefined);
  }
  function updateToken(next: FrpcTokenIntent) {
    replace({
      ...current.current,
      token: next,
      generation: current.current.generation ?? currentGeneration,
      dirty: true,
    });
    setSaved(false);
    setError(undefined);
  }
  function compile(value: FrpcFormSession) {
    if (value.document && !value.document.supported)
      throw new Error(value.document.reason);
    if (value.document?.supported)
      return patchFrpcDocument(value.document, value.input, value.token);
    if (value.token.mode === "replace" && !value.token.value)
      throw new Error("请输入新密钥，或选择保留 / 清除");
    return compileFrpcConfig(
      value.input,
      value.token.mode === "replace" ? value.token.value : "",
    );
  }
  const preview = useMemo(() => {
    if (!session.document && !input.serverAddress.trim()) return {};
    try {
      return { config: redactFrpcDocument(compile(session)) };
    } catch (cause) {
      return { error: cause };
    }
  }, [session]);
  const canConfigure =
    runtime.enabled &&
    !runtime.pending &&
    !runtime.loading &&
    !loading &&
    !loadFailed &&
    !needsReadback &&
    !(session.document && session.generation === undefined) &&
    !unsupported &&
    !stale &&
    generation !== undefined &&
    runtime.status?.artifactAvailable === true &&
    preview.config !== undefined;
  const formDisabled =
    runtime.pending || loading || needsReadback || !!unsupported;
  async function submit(event: React.FormEvent) {
    event.preventDefault();
    if (!canConfigure || generation === undefined) return;
    const submitted = current.current;
    const submittedRevision = frpcFormSessionRevision();
    const requestEpoch = epoch.current;
    setError(undefined);
    try {
      const config = compile(submitted);
      const accepted = await runtime.run(
        () => api.runtimeConfigure({ service: "frpc", config, generation }),
        "frpc 配置已校验并保存。可在运行管理中启动；运行状态不代表远端连通。",
      );
      if (requestEpoch !== frpcFormSessionEpoch()) return;
      if (accepted) {
        // The write response may arrive after a page switch. Retain the accepted document,
        // clear only the submitted token, and require authoritative generation readback.
        const document = readFrpcDocument(config);
        const next: FrpcFormSession = {
          service: "frpc",
          document,
          input: document.supported ? document.input : submitted.input,
          token: { mode: "preserve", value: "" },
          dirty: false,
        };
        if (!acceptFrpcFormSession(next, requestEpoch, submittedRevision))
          return;
        if (!lifetime.current || lifetime.current.signal.aborted) return;
        replace(next);
        setSaved(true);
        await fetchAccepted("load");
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
            { id: "native", label: strings.configuration.title },
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
          <Panel>
            <div className="config-form">
              <div className="form-actions">
                <span role="status" className="text-muted text-xs">
                  {loading
                    ? "正在读取已保存配置…"
                    : session.dirty
                      ? "未保存更改已在本次会话中保留"
                      : session.document
                        ? "已读回已保存配置；未修改选项原样保留"
                        : "尚无已保存配置"}
                </span>
                <Button
                  type="button"
                  disabled={
                    !runtime.enabled ||
                    runtime.pending ||
                    loading ||
                    !runtime.status?.configured
                  }
                  onClick={() =>
                    session.dirty
                      ? setDiscard(true)
                      : void fetchAccepted("load")
                  }
                >
                  重新读取配置
                </Button>
              </div>
              {discard && (
                <div role="group" aria-label="放弃表单更改确认">
                  <p>放弃未保存更改并重新读取？此操作不会修改已保存配置。</p>
                  <Button type="button" onClick={() => setDiscard(false)}>
                    保留编辑
                  </Button>{" "}
                  <Button
                    type="button"
                    onClick={() => {
                      setDiscard(false);
                      void fetchAccepted("discard");
                    }}
                  >
                    放弃并重新读取
                  </Button>
                </div>
              )}
              {unsupported && (
                <ErrorState
                  message={`当前配置不能安全使用表单编辑：${unsupported}。配置未被修改，请使用原生配置。`}
                />
              )}
              {loadFailed && (
                <p className="text-muted">
                  读取未完成，保存已暂停。请重新读取配置；本地输入不会自动丢弃。
                </p>
              )}
              {stale && (
                <div>
                  <ErrorState message="配置已被更新。当前输入已保留，保存已暂停。请读取最新配置并合并本地更改后再审阅。" />
                  <Button
                    type="button"
                    disabled={runtime.pending || loading}
                    onClick={() => {
                      if (!runtime.status?.configured && !session.document)
                        replace({ ...session, generation: currentGeneration });
                      else void fetchAccepted("merge");
                    }}
                  >
                    读取最新配置并合并更改
                  </Button>
                </div>
              )}
              {error !== undefined && (
                <ErrorState message={errorMessage(error)} />
              )}
            </div>
          </Panel>
          <FrpcConnectionForm
            input={input}
            token={token}
            hasToken={
              session.document?.supported === true && session.document.hasToken
            }
            disabled={formDisabled}
            onChange={update}
            onTokenChange={updateToken}
          />
          <FrpcTunnelEditor
            proxies={input.proxies}
            isSavedMapping={(proxy) => !!frpcProxySourceId(proxy)}
            disabled={formDisabled}
            onChange={(proxies) => update("proxies", proxies)}
          />
          <Panel>
            <PanelHeader
              title="配置预览"
              subtitle="仅本地预览；密钥已隐藏。点击保存后执行原生校验，不会自动启动。"
            />
            <div className="config-form">
              {preview.error !== undefined && !unsupported && (
                <ErrorState message={errorMessage(preview.error)} />
              )}
              <pre aria-label="frpc TOML 预览" className="mono wrap">
                {preview.config ?? "填写服务器和有效服务映射后生成预览"}
              </pre>
              {saved && (
                <p className="text-muted text-xs">
                  配置已保存。密钥输入已清空；再次编辑默认保留已保存密钥和未修改选项。
                </p>
              )}
              <div className="form-actions">
                <span className="text-muted text-xs">
                  {generation === undefined
                    ? "等待运行状态；可刷新重试"
                    : !runtime.status?.artifactAvailable
                      ? "请先获取 frpc 运行文件，再校验配置"
                      : "保存不会自动启动；可在运行管理中检查状态"}
                </span>
                <Button
                  variant="primary"
                  disabled={!canConfigure}
                  type="submit"
                >
                  <FileCheck2 size={15} />
                  校验并保存 frpc 配置
                </Button>
              </div>
            </div>
            <details className="panel-bottom">
              <summary>原生与扩展参数（原样保留）</summary>
              <p className="text-muted text-xs">
                未修改的注释、扩展字段和映射选项保持原样。删除映射会移除该映射及其扩展参数。需编辑扩展字段时可使用原生配置。
              </p>
              <Button type="button" onClick={() => setTab("native")}>
                打开原生配置编辑器
              </Button>
            </details>
            <details className="panel-bottom">
              <summary>高级诊断</summary>
              <p className="mono">
                POST /api/runtime/configure · 编辑基线 generation{" "}
                {generation ?? "—"} · 当前 generation {currentGeneration ?? "—"}
              </p>
              {stale && <p className="mono">generation_conflict</p>}
            </details>
          </Panel>
        </form>
      )}
    </div>
  );
}
