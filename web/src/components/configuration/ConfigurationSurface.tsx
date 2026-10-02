import { RefreshCw } from "lucide-react";
import {
  Badge,
  Button,
  EmptyState,
  ErrorState,
  Loading,
} from "../ui/primitives";
import { errorMessage } from "../../lib/api";
import { CommitControls } from "./CommitControls";
import { DraftQueue } from "./DraftQueue";
import { isDirty, type ConfigurationController } from "./use-configuration";
import type { ReactNode } from "react";

export function ConfigurationSurface({
  title,
  controller,
  children,
}: {
  title: string;
  controller: ConfigurationController;
  children: ReactNode;
}) {
  const unsaved = Object.values(controller.buffers).filter(
    (buffer) => isDirty(buffer) && buffer.content !== buffer.stagedContent,
  ).length;
  return (
    <section className="configuration-surface" aria-label={title}>
      <header className="configuration-surface-header">
        <div>
          <h2>{title}</h2>
          <p>编辑配置 → 检查更改 → 应用更改</p>
        </div>
        <div className="configuration-actions">
          {controller.status?.enabled && (
            <>
              {unsaved > 0 && (
                <Badge tone="warning">{unsaved} 个文档有未检查更改</Badge>
              )}
              <Badge>{controller.drafts.length} 个检查结果</Badge>
              <details className="configuration-advanced-details">
                <summary>高级详情</summary>
                <p>
                  配置版本：<code>{controller.status.generation}</code>
                </p>
              </details>
            </>
          )}
          <Button
            size="small"
            disabled={!!controller.busy}
            onClick={() => {
              void controller.refresh();
            }}
            aria-label="刷新配置"
          >
            <RefreshCw
              size={14}
              className={controller.busy === "refresh" ? "spin" : ""}
            />
            刷新
          </Button>
        </div>
      </header>
      {controller.error !== undefined && (
        <ErrorState
          message={errorMessage(controller.error)}
          onRetry={() => {
            void controller.refresh();
          }}
        />
      )}
      {controller.loading &&
      !controller.snapshot &&
      controller.status?.enabled !== false ? (
        <Loading label="正在读取配置状态" />
      ) : controller.status?.enabled ? (
        <>
          <CommitControls controller={controller} />
          {children}
          <DraftQueue controller={controller} />
        </>
      ) : controller.status ? (
        <EmptyState
          title="配置服务未启用"
          detail="当前主机不支持在此编辑和应用配置。"
        />
      ) : null}
    </section>
  );
}
