import { useState } from "react";
import * as Dialog from "@radix-ui/react-dialog";
import {
  Button,
  ErrorState,
  Field,
  Panel,
  PanelHeader,
} from "../../components/ui/primitives";
import { api, errorMessage, runRequest } from "../../lib/api";
import { nativeConfigDiff } from "./config-diff";
import type { RuntimeController } from "./use-runtime";

export function NativeConfigEditor({
  runtime,
}: {
  runtime: RuntimeController;
}) {
  const [config, setConfig] = useState("");
  const [original, setOriginal] = useState("");
  const [reviewing, setReviewing] = useState(false);
  const [generation, setGeneration] = useState<number>();
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<unknown>();
  const [saved, setSaved] = useState(false);
  const [replaceAction, setReplaceAction] = useState<"reload" | "create">();
  const dirty = config !== original && !saved;
  const requestReplacement = (action: "reload" | "create") => {
    if (dirty) setReplaceAction(action);
    else if (action === "reload") void fetchConfig();
    else createConfig();
  };
  const stale =
    generation !== undefined &&
    runtime.status !== undefined &&
    generation !== runtime.status.generation;
  async function fetchConfig() {
    setLoading(true);
    setError(undefined);
    setSaved(false);
    try {
      const response = await runRequest(api.runtimeConfig(runtime.service));
      setConfig(response.config);
      setOriginal(response.config);
      setReviewing(false);
      setGeneration(response.generation);
    } catch (cause) {
      setError(cause);
    } finally {
      setLoading(false);
    }
  }
  function createConfig() {
    if (!runtime.status || runtime.status.configured || !runtime.enabled)
      return;
    setConfig("");
    setOriginal("");
    setGeneration(runtime.status.generation);
    setError(undefined);
    setSaved(false);
    setReviewing(false);
  }
  async function save(event: React.FormEvent) {
    event.preventDefault();
    if (generation === undefined || stale || !reviewing || loading) return;
    setSaved(false);
    const accepted = await runtime.run(
      () =>
        api.runtimeConfigure({ service: runtime.service, config, generation }),
      "原生配置已校验并保存",
    );
    if (accepted) {
      setGeneration(undefined);
      setSaved(true);
    }
  }
  return (
    <Panel>
      <PanelHeader
        title="原生配置"
        subtitle={`${runtime.service === "sing-box" ? "JSON" : "TOML"} · 校验后保存，是否已生效请查看运行状态`}
        action={
          <Button
            size="small"
            disabled={!runtime.enabled || loading || runtime.pending}
            onClick={() => requestReplacement("reload")}
          >
            {generation !== undefined ? "重新载入配置" : "载入配置"}
          </Button>
        }
      />
      <form className="config-form" onSubmit={save}>
        {runtime.status && !runtime.status.configured && (
          <div className="form-actions">
            <span className="text-muted text-xs">尚无已保存配置</span>
            <Button
              type="button"
              disabled={!runtime.enabled || runtime.pending || loading}
              onClick={() => requestReplacement("create")}
            >
              新建原生配置
            </Button>
          </div>
        )}
        <Field
          label="原生配置内容"
          hint="载入和保存均使用认证 API；内容可能包含私密凭据"
        >
          <textarea
            disabled={
              !runtime.enabled ||
              generation === undefined ||
              runtime.pending ||
              loading
            }
            aria-label="原生配置内容"
            className="mono"
            rows={18}
            spellCheck={false}
            autoComplete="off"
            value={config}
            onChange={(event) => {
              setConfig(event.target.value);
              setSaved(false);
              setReviewing(false);
            }}
          />
        </Field>
        {error !== undefined && <ErrorState message={errorMessage(error)} />}
        {stale && (
          <ErrorState message="配置已被更新。当前编辑内容已保留，请重新载入最新配置后再应用更改。" />
        )}
        {reviewing && (
          <div aria-label="原生配置差异">
            <h3>配置差异</h3>
            <p className="text-muted text-xs">- 已保存 · + 待应用更改</p>
            <pre className="mono wrap">
              {nativeConfigDiff(original, config)}
            </pre>
          </div>
        )}
        {saved && (
          <p role="status">
            配置已保存。再次编辑前请载入最新配置；是否已生效请查看运行状态。
          </p>
        )}
        <div className="form-actions">
          <span className="text-muted text-xs">先载入；保存前执行原生校验</span>
          <div>
            <Button
              type="button"
              disabled={
                !runtime.enabled || generation === undefined || !config.trim()
              }
              onClick={() => setReviewing(true)}
            >
              审阅配置差异
            </Button>{" "}
            <Button
              type="submit"
              variant="primary"
              disabled={
                !runtime.enabled ||
                runtime.pending ||
                loading ||
                generation === undefined ||
                stale ||
                !config.trim() ||
                !reviewing
              }
            >
              校验并应用更改
            </Button>
          </div>
        </div>
      </form>
      <details className="panel-bottom">
        <summary>高级诊断</summary>
        <dl className="key-values">
          <div>
            <dt>编辑基线</dt>
            <dd className="mono">generation {generation ?? "—"}</dd>
          </div>
          <div>
            <dt>当前配置</dt>
            <dd className="mono">
              generation {runtime.status?.generation ?? "—"}
            </dd>
          </div>
          {stale && (
            <div>
              <dt>状态码</dt>
              <dd className="mono">generation_conflict</dd>
            </div>
          )}
        </dl>
      </details>
      <Dialog.Root
        open={replaceAction !== undefined}
        onOpenChange={(open) => {
          if (!open) setReplaceAction(undefined);
        }}
      >
        <Dialog.Portal>
          <Dialog.Overlay className="dialog-overlay" />
          <Dialog.Content className="command-dialog">
            <div className="config-form">
              <Dialog.Title>丢弃未保存的原生配置？</Dialog.Title>
              <Dialog.Description>
                当前编辑尚未应用更改。继续会替换本地文本，不会更改已保存的运行配置。
              </Dialog.Description>
              <div className="form-actions">
                <Dialog.Close asChild>
                  <Button type="button">保留编辑</Button>
                </Dialog.Close>
                <Button
                  type="button"
                  onClick={() => {
                    const action = replaceAction;
                    setReplaceAction(undefined);
                    if (action === "reload") void fetchConfig();
                    else if (action === "create") createConfig();
                  }}
                >
                  {replaceAction === "create" ? "丢弃并新建" : "丢弃并载入"}
                </Button>
              </div>
            </div>
          </Dialog.Content>
        </Dialog.Portal>
      </Dialog.Root>
    </Panel>
  );
}
