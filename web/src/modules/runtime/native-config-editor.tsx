import { useState } from "react";
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
        subtitle={`${runtime.service === "sing-box" ? "JSON" : "TOML"} · generation ${generation ?? "—"}`}
        action={
          <Button
            size="small"
            disabled={!runtime.enabled || loading || runtime.pending}
            onClick={() => void fetchConfig()}
          >
            {generation !== undefined ? "重新载入配置" : "载入配置"}
          </Button>
        }
      />
      <form className="config-form" onSubmit={save}>
        <Field
          label="原生配置内容"
          hint="载入和保存均使用认证 API；内容可能包含私密凭据"
        >
          <textarea
            disabled={runtime.pending || loading}
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
          <ErrorState message="配置 generation 已变化 · generation_conflict。保留当前编辑内容；重新载入后再保存。" />
        )}
        {reviewing && (
          <div aria-label="原生配置差异">
            <h3>配置差异</h3>
            <p className="text-muted text-xs">- 已保存 · + 待提交</p>
            <pre className="mono wrap">
              {nativeConfigDiff(original, config)}
            </pre>
          </div>
        )}
        {saved && (
          <p role="status">保存完成。再次编辑前请载入最新 generation。</p>
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
              校验并 Commit 配置
            </Button>
          </div>
        </div>
      </form>
    </Panel>
  );
}
