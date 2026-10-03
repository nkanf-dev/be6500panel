import { useEffect, useRef, useState } from "react";
import * as Dialog from "@radix-ui/react-dialog";
import { Download, FileDiff, ShieldAlert, Upload, X } from "lucide-react";
import { errorMessage, runRequest } from "../../lib/api";
import { bytes, timestamp } from "../../lib/format";
import {
  Badge,
  Button,
  ErrorState,
  Loading,
  Panel,
  PanelHeader,
} from "../ui/primitives";
import {
  configurationModules,
  type Diagnostic,
} from "../configuration/contracts";
import {
  backupModuleLabel,
  backupScopes,
  canStageChange,
  isNativeModule,
  type BackupScope,
  type ImportChange,
  type ImportPreview,
  type ImportStage,
} from "./contracts";
import {
  downloadBackup,
  maintenanceApi,
  readBackupFile,
  retainedDraftIds,
} from "./client";
import { ConfigurationWorkspace } from "../configuration";
import "./maintenance.css";

export interface MaintenanceBackupPanelProps {
  onOpenConfiguration?: () => void;
}

function Diagnostics({
  items,
  title,
  danger = false,
}: {
  items: readonly Diagnostic[];
  title: string;
  danger?: boolean;
}) {
  if (!items.length) return null;
  return (
    <section
      className={`maintenance-diagnostics${danger ? " maintenance-danger" : ""}`}
      aria-label={title}
    >
      <h4>{title}</h4>
      <ul>
        {items.map((item, index) => (
          <li key={`${item.code}-${index}`}>
            <span>{item.message}</span> <code>{item.code}</code>
          </li>
        ))}
      </ul>
    </section>
  );
}
const kindLabels: Record<ImportChange["kind"], string> = {
  added: "新增",
  modified: "修改",
  deleted: "删除",
  unchanged: "未改变",
  uncompared: "无法比较",
};

function ChangeDiff({ change }: { change: ImportChange }) {
  const label = backupModuleLabel(change.module);
  const runtime = !isNativeModule(change.module);
  return (
    <details className="maintenance-change-detail">
      <summary>
        <FileDiff size={14} /> 查看{label}差异与检查结果
      </summary>
      <div className="maintenance-change-metadata">
        <span>
          大小：
          {change.kind === "uncompared"
            ? "未知（未读取当前配置）"
            : bytes(change.beforeBytes)}{" "}
          → {bytes(change.afterBytes)}
        </span>
        <span>
          原摘要：<code>{change.beforeDigest || "—"}</code>
        </span>
        <span>
          导入摘要：<code>{change.afterDigest || "—"}</code>
        </span>
      </div>
      {runtime && (
        <p className="maintenance-note">本次未进行运行配置原生校验。</p>
      )}
      <Diagnostics
        items={change.errors}
        title={runtime ? `${label}校验说明` : `${label}检查问题`}
        danger={!runtime}
      />
      <Diagnostics items={change.dependencies} title={`${label}依赖提示`} />
      <Diagnostics items={change.risks} title={`${label}更改风险`} />
      <pre className="maintenance-diff" aria-label={`${label}配置差异`}>
        {change.diff
          ? change.diff.split("\n").map((line, index) => (
              <span
                key={index}
                className={
                  line.startsWith("+") && !line.startsWith("+++")
                    ? "maintenance-diff-add"
                    : line.startsWith("-") && !line.startsWith("---")
                      ? "maintenance-diff-remove"
                      : line.startsWith("@@")
                        ? "maintenance-diff-hunk"
                        : undefined
                }
              >
                {line || "\u00a0"}
              </span>
            ))
          : runtime
            ? "运行配置本次以摘要和大小比较；恢复需到对应服务确认。"
            : "无配置差异"}
      </pre>
    </details>
  );
}

export function MaintenanceBackupPanel({
  onOpenConfiguration,
}: MaintenanceBackupPanelProps) {
  const [scopes, setScopes] = useState<readonly BackupScope[]>(
    configurationModules.map((item) => item.module),
  );
  const [preview, setPreview] = useState<ImportPreview>();
  const [selected, setSelected] = useState<readonly string[]>([]);
  const [acknowledgeModelMismatch, setAcknowledgeModelMismatch] =
    useState(false);
  const [busy, setBusy] = useState<"export" | "preview" | "stage">();
  const [error, setError] = useState<unknown>();
  const [result, setResult] = useState<ImportStage>();
  const [exported, setExported] = useState(false);
  const [showConfiguration, setShowConfiguration] = useState(false);
  const [expired, setExpired] = useState(false);
  const [needsNewPreview, setNeedsNewPreview] = useState(false);
  const operation = useRef<AbortController | undefined>(undefined);
  const disposals = useRef(new Set<AbortController>());
  const mounted = useRef(false);
  const fileInput = useRef<HTMLInputElement>(null);

  const discardPreview = (id: string) => {
    const controller = new AbortController();
    disposals.current.add(controller);
    void runRequest(maintenanceApi.discard(id), controller.signal)
      .catch(() => {
        /* Server expiry remains the fallback for failed disposal. */
      })
      .finally(() => disposals.current.delete(controller));
  };
  const clearPreview = () => {
    if (preview) discardPreview(preview.id);
    operation.current?.abort();
    operation.current = undefined;
    setBusy(undefined);
    setPreview(undefined);
    setSelected([]);
    setAcknowledgeModelMismatch(false);
    setError(undefined);
    setExpired(false);
    setNeedsNewPreview(false);
  };
  useEffect(() => {
    mounted.current = true;
    const clearSession = () => {
      disposals.current.forEach((controller) => controller.abort());
      disposals.current.clear();
      operation.current?.abort();
      operation.current = undefined;
      setBusy(undefined);
      setPreview(undefined);
      setSelected([]);
      setAcknowledgeModelMismatch(false);
      setError(undefined);
      setResult(undefined);
      setExported(false);
      setShowConfiguration(false);
      setExpired(false);
      setNeedsNewPreview(false);
      if (fileInput.current) fileInput.current.value = "";
    };
    window.addEventListener("be6500panel:unauthorized", clearSession);
    window.addEventListener("be6500panel:logout", clearSession);
    return () => {
      mounted.current = false;
      disposals.current.forEach((controller) => controller.abort());
      disposals.current.clear();
      operation.current?.abort();
      operation.current = undefined;
      window.removeEventListener("be6500panel:unauthorized", clearSession);
      window.removeEventListener("be6500panel:logout", clearSession);
    };
  }, []);
  useEffect(() => {
    if (!preview) return;
    const deadline = Date.parse(preview.expiresAt);
    const remaining = deadline - Date.now();
    setExpired(!Number.isFinite(remaining) || remaining <= 0);
    if (!Number.isFinite(remaining) || remaining <= 0) return;
    const timer = window.setTimeout(
      () => setExpired(true),
      Math.min(remaining, 2_147_483_647),
    );
    return () => window.clearTimeout(timer);
  }, [preview]);

  const begin = (name: "export" | "preview" | "stage") => {
    if (operation.current || !mounted.current) return;
    const controller = new AbortController();
    operation.current = controller;
    setBusy(name);
    setError(undefined);
    setResult(undefined);
    return controller;
  };
  const active = (controller: AbortController) =>
    mounted.current &&
    operation.current === controller &&
    !controller.signal.aborted;
  const finish = (controller: AbortController) => {
    if (!active(controller)) return;
    operation.current = undefined;
    setBusy(undefined);
  };
  const exportBackup = async () => {
    if (!scopes.length) return;
    const controller = begin("export");
    if (!controller) return;
    setExported(false);
    try {
      const file = await runRequest(
        maintenanceApi.backup(scopes),
        controller.signal,
      );
      if (!active(controller)) return;
      downloadBackup(file.blob, file.filename);
      setExported(true);
    } catch (cause) {
      if (active(controller)) setError(cause);
    } finally {
      finish(controller);
    }
  };
  const importFile = async (file: File) => {
    const controller = begin("preview");
    if (!controller) return;
    if (preview) discardPreview(preview.id);
    setPreview(undefined);
    setSelected([]);
    setExpired(false);
    setNeedsNewPreview(false);
    setAcknowledgeModelMismatch(false);
    // Private original text is a request-local variable, never React state or storage.
    let rawText = "";
    try {
      rawText = await readBackupFile(file, controller.signal);
      if (!active(controller)) return;
      const next = await runRequest(
        maintenanceApi.preview(rawText),
        controller.signal,
      );
      if (!active(controller)) return;
      setPreview(next);
      setSelected(
        next.changes.filter(canStageChange).map((change) => change.module),
      );
    } catch (cause) {
      if (active(controller)) setError(cause);
    } finally {
      rawText = "";
      finish(controller);
    }
  };
  const stageImport = async () => {
    if (
      !preview ||
      expired ||
      needsNewPreview ||
      !selected.length ||
      (preview.modelMismatch && !acknowledgeModelMismatch) ||
      Date.parse(preview.expiresAt) <= Date.now()
    )
      return;
    const modules = preview.changes
      .filter(
        (change) => selected.includes(change.module) && canStageChange(change),
      )
      .map((change) => change.module)
      .filter(isNativeModule);
    if (!modules.length) return;
    const controller = begin("stage");
    if (!controller) return;
    try {
      const next = await runRequest(
        maintenanceApi.stage({
          previewId: preview.id,
          generation: preview.generation,
          modules,
          acknowledgeModelMismatch,
        }),
        controller.signal,
      );
      if (!active(controller)) return;
      setResult(next);
      setPreview(undefined);
      setSelected([]);
      setAcknowledgeModelMismatch(false);
    } catch (cause) {
      if (active(controller)) {
        setError(cause);
        // Never replay an uncertain Stage. Upload again to obtain fresh generation checks.
        setNeedsNewPreview(true);
      }
    } finally {
      finish(controller);
    }
  };
  const openQueue = () => {
    clearPreview();
    if (onOpenConfiguration) onOpenConfiguration();
    else setShowConfiguration(true);
  };
  const retained = retainedDraftIds(error);

  if (showConfiguration)
    return (
      <div className="maintenance-body">
        <Button onClick={() => setShowConfiguration(false)}>
          返回备份与导入
        </Button>
        <ConfigurationWorkspace />
      </div>
    );

  return (
    <Panel className="maintenance-backup" aria-label="配置备份与导入">
      <PanelHeader
        title="配置备份与导入"
        subtitle="先预览差异，再暂存草稿；应用更改需到配置队列单独确认。"
      />
      <div className="maintenance-body">
        <div className="maintenance-private-warning" role="note">
          <ShieldAlert size={18} />
          <div>
            <strong>备份包含 Wi-Fi 密钥、Token 和私有配置，请妥善保管。</strong>
            <p>
              仅手动下载到您选择的位置，不写入浏览器本地存储。救援通道、passwd、shadow
              与 SSH 密钥文件不包含在备份中。
            </p>
          </div>
        </div>
        <fieldset className="maintenance-scope-fieldset" disabled={!!busy}>
          <legend>导出范围</legend>
          <div className="maintenance-scope-grid">
            {backupScopes.map((scope) => (
              <label className="maintenance-checkbox" key={scope.module}>
                <input
                  type="checkbox"
                  aria-label={`备份${scope.label}`}
                  checked={scopes.includes(scope.module)}
                  onChange={(event) =>
                    setScopes((previous) =>
                      event.target.checked
                        ? [...previous, scope.module]
                        : previous.filter((item) => item !== scope.module),
                    )
                  }
                />
                <span>
                  <strong>{scope.label}</strong>
                  <small>{scope.description}</small>
                </span>
              </label>
            ))}
          </div>
          <p className="maintenance-note">
            运行配置需主动勾选。SSH
            仅包含面板管理配置，不包含独立救援配置或密钥文件。
          </p>
        </fieldset>
        <div className="maintenance-import-row">
          <Button
            variant="primary"
            disabled={!!busy || !scopes.length}
            onClick={() => void exportBackup()}
          >
            <Download size={15} />
            {busy === "export" ? "正在导出…" : "导出配置备份"}
          </Button>
          <label className="maintenance-file-label">
            <Upload size={15} />
            <span>选择 JSON 备份文件</span>
            <input
              ref={fileInput}
              type="file"
              accept=".json,application/json"
              aria-label="选择 JSON 备份文件"
              disabled={!!busy}
              onChange={(event) => {
                const file = event.currentTarget.files?.[0];
                event.currentTarget.value = "";
                if (file) void importFile(file);
              }}
            />
          </label>
        </div>
        <p className="maintenance-note">
          仅支持 UTF-8 JSON，最大 2
          MiB。上传只生成预览，不会自动下载、应用、重启或启动服务。原生配置由服务端校验，暂存时会再次检查。
        </p>
        {busy === "preview" && (
          <div className="maintenance-actions">
            <Loading label="正在检查导入差异" />
            <Button onClick={clearPreview}>取消上传</Button>
          </div>
        )}
        {exported && (
          <p role="status" className="maintenance-success">
            已请求下载备份，请确认文件已保存并妥善保管。
          </p>
        )}
        {error !== undefined && !preview && (
          <ErrorState message={errorMessage(error)} />
        )}
        {result && (
          <section
            className="maintenance-result"
            role="status"
            aria-label="导入暂存结果"
          >
            <strong>
              已暂存 {result.drafts.length} 个配置草稿，尚未应用。
            </strong>
            <p>
              配置版本：<code>{result.generation}</code>
              。请到配置队列检查依赖和风险，再单独确认应用。
            </p>
            <Diagnostics items={result.warnings} title="暂存提示" />
            <Button onClick={openQueue}>前往配置队列</Button>
          </section>
        )}
      </div>
      <Dialog.Root
        open={!!preview}
        onOpenChange={(open) => {
          if (!open && busy !== "stage") clearPreview();
        }}
      >
        <Dialog.Portal>
          <Dialog.Overlay className="maintenance-dialog-overlay" />
          <Dialog.Content
            className="maintenance-preview-dialog"
            onEscapeKeyDown={(event) => {
              if (busy === "stage") event.preventDefault();
            }}
            onPointerDownOutside={(event) => {
              if (busy === "stage") event.preventDefault();
            }}
          >
            <header className="maintenance-dialog-header">
              <Dialog.Title>导入配置差异预览</Dialog.Title>
              <Button
                variant="ghost"
                size="icon"
                aria-label="放弃导入预览"
                disabled={busy === "stage"}
                onClick={clearPreview}
              >
                <X size={18} />
              </Button>
            </header>
            <Dialog.Description>
              只暂存所选原生配置，不立即应用。差异可能包含密钥和
              Token，请勿分享截图。
            </Dialog.Description>
            {preview && (
              <>
                <p className="maintenance-summary">
                  检测到变更：{preview.summary.added} 项新增，
                  {preview.summary.modified} 项修改，{preview.summary.deleted}{" "}
                  项删除；{preview.summary.unchanged} 项未改变；
                  {preview.summary.uncompared} 项无法比较。
                </p>
                <dl className="maintenance-preview-meta">
                  <div>
                    <dt>来源型号</dt>
                    <dd>{preview.sourceModel || "未知"}</dd>
                  </div>
                  <div>
                    <dt>当前型号</dt>
                    <dd>{preview.currentModel || "未知"}</dd>
                  </div>
                  <div>
                    <dt>预览配置版本</dt>
                    <dd>
                      <code>{preview.generation}</code>
                    </dd>
                  </div>
                  <div>
                    <dt>预览有效期</dt>
                    <dd>{timestamp(preview.expiresAt)}</dd>
                  </div>
                </dl>
                {preview.modelMismatch && (
                  <section className="maintenance-model-warning" role="alert">
                    <strong>
                      设备型号不一致：{preview.sourceModel || "未知"} →{" "}
                      {preview.currentModel || "未知"}
                    </strong>
                    <p>
                      请确认配置适用于当前设备。错误配置可能影响网络与管理连接。
                    </p>
                    <label className="maintenance-checkbox">
                      <input
                        type="checkbox"
                        checked={acknowledgeModelMismatch}
                        disabled={!!busy || needsNewPreview}
                        onChange={(event) =>
                          setAcknowledgeModelMismatch(event.target.checked)
                        }
                      />
                      <span>我已核对设备型号差异，仍要暂存所选原生配置</span>
                    </label>
                  </section>
                )}
                <Diagnostics items={preview.warnings} title="导入提示" />
                {expired && (
                  <p className="maintenance-expired" role="alert">
                    预览已过期，请放弃后重新上传备份文件。
                  </p>
                )}
                <div className="maintenance-changes" aria-label="导入配置分组">
                  {preview.changes.map((change) => (
                    <section className="maintenance-change" key={change.module}>
                      <div className="maintenance-change-heading">
                        <label className="maintenance-checkbox">
                          <input
                            type="checkbox"
                            aria-label={`暂存${backupModuleLabel(change.module)}`}
                            checked={selected.includes(change.module)}
                            disabled={
                              !!busy ||
                              expired ||
                              needsNewPreview ||
                              !canStageChange(change)
                            }
                            onChange={(event) =>
                              setSelected((previous) =>
                                event.target.checked
                                  ? [...previous, change.module]
                                  : previous.filter(
                                      (module) => module !== change.module,
                                    ),
                              )
                            }
                          />
                          <span>
                            <strong>{backupModuleLabel(change.module)}</strong>
                            <small>{change.module}</small>
                          </span>
                        </label>
                        <Badge
                          tone={
                            !isNativeModule(change.module)
                              ? "neutral"
                              : !change.valid
                                ? "danger"
                                : change.kind === "unchanged"
                                  ? "neutral"
                                  : "primary"
                          }
                        >
                          {kindLabels[change.kind]}
                        </Badge>
                        {isNativeModule(change.module) &&
                          change.kind !== "unchanged" && (
                            <Badge tone={change.valid ? "success" : "danger"}>
                              {change.valid ? "服务端检查通过" : "检查未通过"}
                            </Badge>
                          )}
                      </div>
                      {!isNativeModule(change.module) && (
                        <p className="maintenance-note">
                          本次仅预览，运行配置恢复需在对应服务中确认。
                        </p>
                      )}
                      {isNativeModule(change.module) &&
                        change.kind !== "unchanged" &&
                        !change.stageable && (
                          <p className="maintenance-note">
                            此配置当前不能暂存，请查看检查结果。
                          </p>
                        )}
                      <ChangeDiff change={change} />
                    </section>
                  ))}
                </div>
                {error !== undefined && (
                  <>
                    <ErrorState message={errorMessage(error)} />
                    <p className="maintenance-note">
                      暂存未完成。请重新上传核对；不会自动重试或声称更改已应用。
                    </p>
                    {retained.length > 0 && (
                      <section className="maintenance-retained" role="alert">
                        <strong>
                          清理未完成，队列中可能保留 {retained.length} 个草稿。
                        </strong>
                        <p>请到配置队列核对并删除不需要的草稿，避免误用。</p>
                        <details>
                          <summary>保留草稿标识</summary>
                          <ul>
                            {retained.map((id) => (
                              <li key={id}>
                                <code>{id}</code>
                              </li>
                            ))}
                          </ul>
                        </details>
                        <Button onClick={openQueue}>前往配置队列</Button>
                      </section>
                    )}
                  </>
                )}
                <footer className="maintenance-dialog-footer">
                  <Button disabled={!!busy} onClick={clearPreview}>
                    放弃
                  </Button>
                  <Button
                    variant="primary"
                    disabled={
                      !!busy ||
                      expired ||
                      needsNewPreview ||
                      !selected.length ||
                      (preview.modelMismatch && !acknowledgeModelMismatch)
                    }
                    onClick={() => void stageImport()}
                  >
                    {busy === "stage"
                      ? "正在暂存…"
                      : `暂存为草稿 (稍后应用) (${selected.length})`}
                  </Button>
                </footer>
              </>
            )}
          </Dialog.Content>
        </Dialog.Portal>
      </Dialog.Root>
    </Panel>
  );
}
