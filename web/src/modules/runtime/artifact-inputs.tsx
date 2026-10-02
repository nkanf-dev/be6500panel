import { useState } from "react";
import { Button, Field } from "../../components/ui/primitives";
import { api } from "../../lib/api";
import type { RuntimeArtifactInput } from "../../lib/contracts";
import type { RuntimeController } from "./use-runtime";

export function ArtifactInputs({ runtime }: { runtime: RuntimeController }) {
  const [artifact, setArtifact] = useState<RuntimeArtifactInput>({
    url: "",
    sha256: "",
    version: "",
    compression: "gzip",
  });
  function update<K extends keyof RuntimeArtifactInput>(
    key: K,
    value: RuntimeArtifactInput[K],
  ) {
    setArtifact((previous) => ({ ...previous, [key]: value }));
  }
  async function submit(event: React.FormEvent) {
    event.preventDefault();
    await runtime.run(
      () => api.runtimeAcquire({ service: runtime.service, artifact }),
      "校验下载完成，运行文件已激活",
    );
  }
  return (
    <details>
      <summary className="panel-bottom">运行文件 · HTTPS / SHA-256</summary>
      <form className="config-form" onSubmit={submit}>
        <Field
          label="运行文件 URL"
          hint="HTTPS 下载到 RAM；SHA-256 校验下载字节（gzip 时为压缩文件），成功后激活"
        >
          <input
            disabled={runtime.pending}
            aria-label="运行文件 URL"
            type="url"
            pattern="https://.*"
            autoComplete="off"
            required
            value={artifact.url}
            onChange={(event) => update("url", event.target.value)}
          />
        </Field>
        <div className="form-grid">
          <Field label="SHA-256">
            <input
              disabled={runtime.pending}
              className="mono"
              required
              pattern="[a-fA-F0-9]{64}"
              maxLength={64}
              value={artifact.sha256}
              onChange={(event) => update("sha256", event.target.value)}
            />
          </Field>
          <Field label="文件版本">
            <input
              disabled={runtime.pending}
              required
              maxLength={64}
              value={artifact.version}
              onChange={(event) => update("version", event.target.value)}
            />
          </Field>
          <Field label="压缩格式">
            <select
              className="select-trigger"
              disabled={runtime.pending}
              value={artifact.compression}
              onChange={(event) =>
                update(
                  "compression",
                  event.target.value as RuntimeArtifactInput["compression"],
                )
              }
            >
              <option value="gzip">gzip</option>
              <option value="none">未压缩</option>
            </select>
          </Field>
        </div>
        <div className="form-actions">
          <span className="text-muted text-xs">
            {runtime.service} · RAM 运行文件
          </span>
          <Button type="submit" disabled={!runtime.enabled || runtime.pending}>
            校验并获取运行文件
          </Button>
        </div>
      </form>
    </details>
  );
}
