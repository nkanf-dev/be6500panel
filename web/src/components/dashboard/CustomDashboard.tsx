import { strings } from "../../locales/strings";
import { useEffect, useId, useRef, useState } from "react";
import {
  ArrowDown,
  ArrowUp,
  Check,
  LayoutDashboard,
  Pencil,
  RotateCcw,
  X,
} from "lucide-react";
import {
  Badge,
  Button,
  EmptyState,
  ErrorState,
  Panel,
  PanelHeader,
} from "../ui/primitives";
import type { PageId } from "../../modules/registry";
import {
  defaultLayout,
  moveWidget,
  readLayout,
  saveLayout,
  widgetDefinitions,
  type DashboardLayout,
  type WidgetId,
  type WidgetSize,
} from "./layout";
import { DashboardWidget } from "./widgets";
import "./dashboard.css";

/** Router-native dashboard. Only layout settings are stored in this browser. */
export function CustomDashboard({
  navigate,
}: {
  navigate: (id: PageId) => void;
}) {
  const [loaded] = useState(readLayout);
  const [saved, setSaved] = useState(loaded.layout);
  const [draft, setDraft] = useState<DashboardLayout>();
  const [notice, setNotice] = useState(loaded.notice);
  const [saveError, setSaveError] = useState<string>();
  const [announcement, setAnnouncement] = useState("");
  const layout = draft ?? saved;
  const nameId = useId();
  const editing = draft !== undefined;
  const nameInput = useRef<HTMLInputElement>(null);
  const toolbarActions = useRef<HTMLDivElement>(null);
  const wasEditing = useRef(false);
  useEffect(() => {
    if (editing) nameInput.current?.focus();
    else if (wasEditing.current)
      toolbarActions.current
        ?.querySelector<HTMLButtonElement>("[data-dashboard-edit]")
        ?.focus();
    wasEditing.current = editing;
  }, [editing]);
  const visible = layout.widgets.filter((widget) => widget.visible);
  const edit = () => {
    setDraft({
      ...saved,
      widgets: saved.widgets.map((widget) => ({ ...widget })),
    });
    setSaveError(undefined);
    setAnnouncement("编辑布局：更改将即时预览，保存后才会持久化。");
  };
  const updateWidget = (
    id: WidgetId,
    change: { visible?: boolean; size?: WidgetSize },
  ) => {
    setDraft(
      (current) =>
        current && {
          ...current,
          widgets: current.widgets.map((widget) =>
            widget.id === id ? { ...widget, ...change } : widget,
          ),
        },
    );
    setSaveError(undefined);
  };
  const move = (id: WidgetId, direction: -1 | 1) => {
    setDraft((current) => current && moveWidget(current, id, direction));
    setSaveError(undefined);
    const position =
      layout.widgets.findIndex((widget) => widget.id === id) + direction + 1;
    setAnnouncement(
      `${widgetDefinitions.find((widget) => widget.id === id)!.title}已${direction === -1 ? "上移" : "下移"}到第 ${position} 位。`,
    );
  };
  const save = () => {
    if (!draft) return;
    const result = saveLayout(draft);
    if (!result.ok) {
      setSaveError(result.message);
      return;
    }
    setSaved(result.layout);
    setDraft(undefined);
    setNotice(undefined);
    setSaveError(undefined);
    setAnnouncement("布局已保存到此浏览器。");
  };
  const cancel = () => {
    setDraft(undefined);
    setSaveError(undefined);
    setAnnouncement("已取消编辑，恢复保存的布局。");
  };
  const reset = () => {
    setDraft(defaultLayout());
    setSaveError(undefined);
    setAnnouncement("已预览默认布局。保存后生效，也可取消。");
  };
  return (
    <div className="page-stack custom-dashboard">
      <header className="dashboard-toolbar">
        <div>
          <h2>
            <LayoutDashboard size={17} aria-hidden="true" />
            {layout.name || strings.dashboard.states.unnamed}
            {editing && <Badge tone="warning">{strings.dashboard.states.unsaved}</Badge>}
          </h2>
          <p>原生仪表盘 · 组件布局仅保存在此浏览器，不在路由器运行 Grafana。</p>
        </div>
        <div className="dashboard-actions" ref={toolbarActions}>
          {editing ? (
            <>
              <Button size="small" onClick={reset}>
                <RotateCcw size={14} />{strings.dashboard.actions.reset}
              </Button>
              <Button size="small" onClick={cancel}>
                <X size={14} />{strings.actions.cancel}
              </Button>
              <Button variant="primary" size="small" onClick={save}>
                <Check size={14} />{strings.dashboard.actions.save}
              </Button>
            </>
          ) : (
            <Button data-dashboard-edit size="small" onClick={edit}>
              <Pencil size={14} />{strings.dashboard.actions.edit}
            </Button>
          )}
        </div>
      </header>
      {notice && (
        <p className="dashboard-notice" role="status">
          {notice}
        </p>
      )}
      {saveError && <ErrorState message={saveError} />}
      <p
        className="sr-only"
        role="status"
        aria-live="polite"
        aria-atomic="true"
      >
        {announcement}
      </p>
      {editing && (
        <Panel className="dashboard-editor" aria-label={strings.dashboard.labels.editor}>
          <PanelHeader
            title={strings.dashboard.labels.editorTitle}
            subtitle="勾选显示，使用上移 / 下移调整顺序；更改宽度会即时预览。"
          />
          <div className="dashboard-name-field">
            <label htmlFor={nameId}>{strings.dashboard.labels.name}</label>
            <input
              id={nameId}
              ref={nameInput}
              value={layout.name}
              maxLength={64}
              onChange={(event) => {
                setDraft(
                  (current) =>
                    current && { ...current, name: event.target.value },
                );
                setSaveError(undefined);
              }}
            />
          </div>
          <ol className="dashboard-editor-list" aria-label={strings.dashboard.labels.order}>
            {layout.widgets.map((widget, index) => {
              const definition = widgetDefinitions.find(
                (item) => item.id === widget.id,
              )!;
              return (
                <li key={widget.id} data-widget-editor={widget.id}>
                  <label className="dashboard-visibility">
                    <input
                      type="checkbox"
                      checked={widget.visible}
                      onChange={(event) =>
                        updateWidget(widget.id, {
                          visible: event.target.checked,
                        })
                      }
                      aria-label={`显示${definition.title}`}
                    />
                    <span>
                      <strong>{definition.title}</strong>
                      <small>{definition.description}</small>
                    </span>
                  </label>
                  <div className="dashboard-widget-controls">
                    <label>
                      <span className="sr-only">{definition.title}宽度</span>
                      <select
                        value={widget.size}
                        aria-label={`${definition.title}宽度`}
                        onChange={(event) =>
                          updateWidget(widget.id, {
                            size: event.target.value as WidgetSize,
                          })
                        }
                      >
                        <option value="compact">{strings.dashboard.labels.compact}</option>
                        <option value="wide">{strings.dashboard.labels.wide}</option>
                        <option value="full">{strings.dashboard.labels.full}</option>
                      </select>
                    </label>
                    <Button
                      size="icon"
                      aria-label={`上移${definition.title}`}
                      disabled={index === 0}
                      onClick={() => move(widget.id, -1)}
                    >
                      <ArrowUp size={14} />
                    </Button>
                    <Button
                      size="icon"
                      aria-label={`下移${definition.title}`}
                      disabled={index === layout.widgets.length - 1}
                      onClick={() => move(widget.id, 1)}
                    >
                      <ArrowDown size={14} />
                    </Button>
                  </div>
                </li>
              );
            })}
          </ol>
          <p className="dashboard-data-note">
            窄屏自动单列。只保存名称、显示、顺序和宽度，不保存设备身份或采样数据。
          </p>
        </Panel>
      )}
      {visible.length ? (
        <div
          className="dashboard-widget-grid"
          role="list"
          aria-label={strings.dashboard.labels.widgets}
        >
          {visible.map((widget) => (
            <div
              className={`dashboard-widget dashboard-widget-${widget.size}`}
              key={widget.id}
              role="listitem"
              data-widget-id={widget.id}
              data-widget-size={widget.size}
              aria-label={
                widgetDefinitions.find((item) => item.id === widget.id)!.title
              }
            >
              <DashboardWidget id={widget.id} navigate={navigate} />
            </div>
          ))}
        </div>
      ) : (
        <Panel>
          <EmptyState
            title={strings.dashboard.labels.allHidden}
            detail={
              editing
                ? "在布局编辑器中勾选组件，或恢复默认布局。"
                : "编辑布局以显示组件，或恢复默认布局。"
            }
          >
            {!editing && <Button onClick={edit}>{strings.dashboard.actions.selectWidgets}</Button>}
          </EmptyState>
        </Panel>
      )}
    </div>
  );
}
