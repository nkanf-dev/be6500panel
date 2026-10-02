import { strings } from "../../locales/strings";
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
import { nativeSectionLabel, nativeSections } from "./native-document";
import { NativeFields } from "./NativeFields";
import { SectionControls } from "./SectionControls";
import { sectionSchema } from "./field-schema";
import { isDirty, type ConfigurationController } from "./use-configuration";

export function NativeEditor({
  module,
  controller,
}: {
  module: ConfigurationModule;
  controller: ConfigurationController;
}) {
  const buffer = controller.buffers[module];
  const [view, setView] = useState<"editor" | "fields">("fields");
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
      ? "检查通过，待应用"
      : "检查未通过"
    : dirty
      ? "有未检查的更改"
      : "与已保存设置一致";
  const stageDisabled =
    !!controller.busy ||
    !!controller.status?.pendingCommit ||
    !controller.status?.enabled ||
    stale ||
    !dirty;
  return (
    <Panel
      className="configuration-native"
      onKeyDown={(event) => {
        if ((event.ctrlKey || event.metaKey) && event.key === "Enter") {
          event.preventDefault();
          if (!stageDisabled) void controller.stage(module);
        }
      }}
    >
      <PanelHeader
        title={`${info.label}配置`}
        subtitle={`${info.description} · 编辑不会立即影响路由器`}
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
      {module === "network" && (
        <p className="configuration-inline-warning" role="note">
          当前版本保留现有 LAN 管理地址和网桥。更改其他网络设置前会检查配置；LAN
          地址迁移请使用路由器原厂管理页面。
        </p>
      )}
      <div className="configuration-editor-toolbar">
        <div
          className="configuration-tabs"
          role="tablist"
          aria-label={`${module} 配置视图`}
        >
          <button
            type="button"
            id={`${tabId}-fields`}
            role="tab"
            aria-selected={view === "fields"}
            aria-controls={`${tabId}-panel`}
            onClick={() => setView("fields")}
          >
            <Layers size={14} />
            字段编辑
          </button>
          <button
            type="button"
            id={`${tabId}-editor`}
            role="tab"
            aria-selected={view === "editor"}
            aria-controls={`${tabId}-panel`}
            onClick={() => setView("editor")}
          >
            <FileCode2 size={14} />
            高级：原生编辑
          </button>
        </div>
        <details className="configuration-technical-details">
          <summary>技术详情</summary>
          <span className="configuration-generation">
            /etc/config/{module} · 编辑版本 g{buffer.generation} · 当前版本 g
            {controller.status?.generation}
          </span>
        </details>
      </div>
      {stale && (
        <div className="configuration-inline-warning" role="status">
          路由器上的设置已改变。你的编辑仍保留，但不能直接应用。请载入最新设置后重新检查。
        </div>
      )}
      <SectionControls
        module={module}
        content={buffer.content}
        section={section}
        documents={Object.fromEntries(
          Object.entries(controller.buffers).map(([name, value]) => [
            name,
            value.content,
          ]),
        )}
        disabled={
          !!controller.busy ||
          !!controller.status?.pendingCommit ||
          !controller.status?.enabled
        }
        onChange={(content, name) => {
          controller.edit(module, content);
          if (name)
            setSectionId(
              nativeSections(content).find((item) => item.name === name)?.id,
            );
          else setSectionId(undefined);
          setView("fields");
        }}
      />
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
              <strong>{nativeSectionLabel(item)}</strong>
              <span>{sectionSchema(module, item.type).label}</span>
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
            <>
              <p className="configuration-advanced-note">
                高级编辑可直接修改完整 UCI
                文本（包含密码）。一般设置请使用字段编辑；两种视图共享本地草稿。
              </p>
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
                />
              </div>
            </>
          ) : section ? (
            <NativeFields
              key={section.id}
              module={module}
              section={section}
              content={buffer.content}
              onChange={(content) => controller.edit(module, content)}
            />
          ) : (
            <EmptyState title="暂无原生章节" />
          )}
        </div>
      </div>
      <footer className="configuration-editor-footer">
        <span>
          更改先检查，再应用 · <kbd>⌘ / Ctrl + Enter</kbd>
          {strings.actions.checkChanges}
        </span>
        <div className="configuration-actions">
          <Button
            size="small"
            variant="ghost"
            onClick={() => controller.reset(module)}
            disabled={!!controller.busy || (!dirty && !stale)}
          >
            <RotateCcw size={14} />
            {stale ? "载入最新设置" : "撤销本地编辑"}
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
            {controller.busy === "stage"
              ? "正在检查…"
              : strings.actions.checkChanges}
          </Button>
        </div>
      </footer>
    </Panel>
  );
}
