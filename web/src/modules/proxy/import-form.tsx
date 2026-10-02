import { useState } from "react";
import {
  Button,
  ErrorState,
  Field,
  Panel,
  PanelHeader,
} from "../../components/ui/primitives";
import { api, errorMessage, runRequest } from "../../lib/api";
import type { ProxyNodes } from "../../lib/contracts";

export function ProxyImportForm({
  enabled,
  onImported,
  onPending,
}: {
  enabled: boolean;
  onImported: (response: ProxyNodes) => void;
  onPending: (pending: boolean) => void;
}) {
  const [kind, setKind] = useState<"url" | "content">("url");
  const [url, setUrl] = useState("");
  const [content, setContent] = useState("");
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<unknown>();
  const [result, setResult] = useState<string>();
  async function submit(event: React.FormEvent) {
    event.preventDefault();
    if (!enabled || pending) return;
    setPending(true);
    onPending(true);
    setError(undefined);
    setResult(undefined);
    try {
      const response = await runRequest(
        api.proxyImport(kind === "url" ? { url } : { content }),
      );
      onImported(response);
      setUrl("");
      setContent("");
      setResult(`已导入 ${response.nodes.length} 个节点`);
    } catch (cause) {
      setError(cause);
    } finally {
      setPending(false);
      onPending(false);
    }
  }
  return (
    <Panel>
      <PanelHeader
        title="导入订阅"
        subtitle="本机解析 Clash YAML · 最大 2 MiB"
      />
      <form className="config-form" onSubmit={submit}>
        <div className="segmented" aria-label="导入来源">
          {[
            { id: "url", label: "私密 URL" },
            { id: "content", label: "YAML 内容" },
          ].map((item) => (
            <button
              type="button"
              disabled={pending}
              key={item.id}
              aria-pressed={kind === item.id}
              onClick={() => setKind(item.id as "url" | "content")}
            >
              {item.label}
            </button>
          ))}
        </div>
        {kind === "url" ? (
          <Field
            label="私密订阅 URL"
            hint="仅 HTTPS；由路由器有界下载，不使用外部转换服务"
          >
            <input
              disabled={pending}
              aria-label="私密订阅 URL"
              type="password"
              required
              pattern="https://.*"
              autoComplete="off"
              value={url}
              onChange={(event) => setUrl(event.target.value)}
            />
          </Field>
        ) : (
          <Field label="订阅 YAML">
            <textarea
              disabled={pending}
              required
              aria-label="订阅 YAML"
              className="mono"
              rows={10}
              spellCheck={false}
              autoComplete="off"
              value={content}
              onChange={(event) => setContent(event.target.value)}
            />
          </Field>
        )}
        {error !== undefined && <ErrorState message={errorMessage(error)} />}
        {result && <p role="status">{result}</p>}
        <div className="form-actions">
          <span className="text-muted text-xs">
            VLESS · TCP · REALITY / Vision
          </span>
          <Button
            variant="primary"
            type="submit"
            disabled={!enabled || pending}
          >
            {pending ? "导入中…" : "解析并导入节点"}
          </Button>
        </div>
      </form>
    </Panel>
  );
}
