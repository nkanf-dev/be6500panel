import { useId, useMemo, useRef, useState } from "react";
import { FileCode2, Layers, RotateCcw, Save } from "lucide-react";
import {
  Badge,
  Button,
  EmptyState,
  Panel,
  PanelHeader,
  cn,
} from "../ui/primitives";
import { configurationModules, type ConfigurationModule } from "./contracts";
import { nativeSections } from "./native-document";
import { isDirty, type ConfigurationController } from "./use-configuration";

export function NativeEditor({
  module,
  controller,
}: {
  module: ConfigurationModule;
  controller: ConfigurationController;
}) {
  const buffer = controller.buffers[module];
  const [view, setView] = useState<"editor" | "fields">("editor");
  const [sectionId, setSectionId] = useState<string>();
  const input = useRef<HTMLTextAreaElement>(null);
  const gutter = useRef<HTMLPreElement>(null);
  const tabId = useId();
  const sections = useMemo(
    () => nativeSections(buffer?.content ?? ""),
    [buffer?.content],
  );
  const section = sections.find((item) => item.id === sectionId) ?? sections[0];
  const info = configurationModules.find((item) => item.module === module)!;
  if (!buffer)
    return (
      <Panel>
        <EmptyState title={`暂无 ${module} 配置文档`} />
      </Panel>
    );
  const dirty = isDirty(buffer);
  const stale = buffer.generation !== controller.status?.generation;
  const staged = buffer.stagedId
    ? controller.drafts.find((draft) => draft.id === buffer.stagedId)
    : undefined;
  const stagedCurrent =
    !!staged &&
    staged.generation === controller.status?.generation &&
    buffer.stagedContent === buffer.content;
  const statusLabel = stagedCurrent
    ? staged.valid
      ? "已暂存"
      : "校验失败"
    : dirty
      ? "未暂存"
      : "与已保存一致";
  const stageDisabled =
    !!controller.busy ||
    !!controller.status?.pendingCommit ||
    !controller.status?.enabled ||
    stale ||
    !dirty;
  return (
    <Panel className="configuration-native">
      <PanelHeader
        title={`${info.label}配置`}
        subtitle={`/etc/config/${module} · ${info.description}`}
        action={
          <Badge
            tone={
              stagedCurrent
                ? staged?.valid
                  ? "primary"
                  : "danger"
                : dirty
                  ? "warning"
                  : "neutral"
            }
          >
            {statusLabel}
          </Badge>
        }
      />
      <div className="configuration-editor-toolbar">
        <div
          className="configuration-tabs"
          role="tablist"
          aria-label={`${module} 配置视图`}
        >
          <button
            type="button"
            id={`${tabId}-editor`}
            role="tab"
            aria-selected={view === "editor"}
            aria-controls={`${tabId}-panel`}
            onClick={() => setView("editor")}
          >
            <FileCode2 size={14} />
            原生编辑
          </button>
          <button
            type="button"
            id={`${tabId}-fields`}
            role="tab"
            aria-selected={view === "fields"}
            aria-controls={`${tabId}-panel`}
            onClick={() => setView("fields")}
          >
            <Layers size={14} />
            字段视图
          </button>
        </div>
        <span className="configuration-generation">
          {stale
            ? `编辑基线 g${buffer.generation} · 当前 g${controller.status?.generation}`
            : `已保存版本 g${buffer.generation}`}
        </span>
      </div>
      {stale && (
        <div className="configuration-inline-warning" role="status">
          配置版本已改变。载入当前版本后再编辑；本地文本仍保留。
        </div>
      )}
      <div className="configuration-native-layout">
        <nav
          className="configuration-section-nav"
          aria-label={`${module} 章节`}
        >
          <div className="configuration-caption">
            章节 <span>{sections.length}</span>
          </div>
          {sections.map((item) => (
            <button
              type="button"
              key={item.id}
              className={cn(
                section?.id === item.id && "configuration-section-active",
              )}
              aria-current={section?.id === item.id ? "location" : undefined}
              onClick={() => {
                setSectionId(item.id);
                if (view === "editor" && input.current) {
                  input.current.focus();
                  input.current.setSelectionRange(item.offset, item.offset);
                  const lineHeight =
                    Number.parseFloat(
                      getComputedStyle(input.current).lineHeight,
                    ) || 22;
                  input.current.scrollTop = Math.max(
                    0,
                    (item.line - 2) * lineHeight,
                  );
                  if (gutter.current)
                    gutter.current.scrollTop = input.current.scrollTop;
                }
              }}
            >
              <strong>{item.name}</strong>
              <span>
                {item.type} · L{item.line}
              </span>
            </button>
          ))}
          {!sections.length && (
            <span className="configuration-nav-empty">无章节</span>
          )}
        </nav>
        <div
          id={`${tabId}-panel`}
          role="tabpanel"
          aria-labelledby={`${tabId}-${view}`}
          className="configuration-editor-view"
        >
          {view === "editor" ? (
            <div className="configuration-code-editor">
              <pre
                ref={gutter}
                aria-hidden="true"
                className="configuration-line-numbers"
              >
                {buffer.content
                  .split("\n")
                  .map((_, index) => index + 1)
                  .join("\n")}
              </pre>
              <textarea
                ref={input}
                aria-label={`${module} 原生配置`}
                value={buffer.content}
                spellCheck={false}
                autoCorrect="off"
                autoCapitalize="off"
                wrap="off"
                onChange={(event) =>
                  controller.edit(module, event.target.value)
                }
                onScroll={(event) => {
                  if (gutter.current)
                    gutter.current.scrollTop = event.currentTarget.scrollTop;
                }}
                onKeyDown={(event) => {
                  if (
                    (event.ctrlKey || event.metaKey) &&
                    event.key === "Enter"
                  ) {
                    event.preventDefault();
                    if (!stageDisabled) void controller.stage(module);
                  }
                }}
              />
            </div>
          ) : section ? (
            <div className="table-scroll configuration-field-table">
              <table className="data-table">
                <thead>
                  <tr>
                    <th>字段</th>
                    <th>类型</th>
                    <th>值</th>
                    <th>行</th>
                  </tr>
                </thead>
                <tbody>
                  {section.fields.map((field) => (
                    <tr key={field.line}>
                      <td>
                        <code>{field.name}</code>
                      </td>
                      <td>
                        <code>{field.kind}</code>
                      </td>
                      <td className="configuration-native-value">
                        <code>{field.value}</code>
                      </td>
                      <td className="mono">{field.line}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
              {!section.fields.length && <EmptyState title="此章节暂无字段" />}
            </div>
          ) : (
            <EmptyState title="暂无原生章节" />
          )}
        </div>
      </div>
      <footer className="configuration-editor-footer">
        <span>
          UCI · {buffer.content.split("\n").length} 行 ·{" "}
          <kbd>⌘ / Ctrl + Enter</kbd> 暂存
        </span>
        <div className="configuration-actions">
          <Button
            size="small"
            variant="ghost"
            onClick={() => controller.reset(module)}
            disabled={!!controller.busy || (!dirty && !stale)}
          >
            <RotateCcw size={14} />
            {stale ? "载入当前版本" : "丢弃本地编辑"}
          </Button>
          <Button
            size="small"
            variant="primary"
            onClick={() => {
              void controller.stage(module);
            }}
            disabled={stageDisabled}
          >
            <Save size={14} />
            {controller.busy === "stage" ? "正在校验…" : "暂存并校验"}
          </Button>
        </div>
      </footer>
    </Panel>
  );
}
