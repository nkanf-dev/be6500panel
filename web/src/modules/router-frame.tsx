import type { ReactNode } from "react";
import { Activity, RefreshCw } from "lucide-react";
import { useConsole } from "../app/console-context";
import {
  Badge,
  Button,
  EmptyState,
  ErrorState,
  Loading,
  Panel,
  PanelHeader,
} from "../components/ui/primitives";
import { errorMessage } from "../lib/api";
import type { RouterSnapshot } from "../lib/contracts";
import { RouterObservationErrors } from "./router-errors";
export { RouterTable } from "./router-table";

export function observationErrors(
  snapshot: RouterSnapshot | undefined,
  modules: readonly string[],
) {
  return (
    snapshot?.errors.filter((error) =>
      modules.some(
        (module) =>
          error.module === module || error.module.startsWith(`${module}.`),
      ),
    ) ?? []
  );
}

export function useRouterObservation(modules: readonly string[]) {
  const {
    router,
    routerError,
    routerLoading = false,
    refreshRouter,
  } = useConsole();
  // The context can retain its last response after a failed refresh. It must
  // not be presented as the current successful router observation.
  const snapshot = routerError === undefined ? router : undefined;
  return {
    snapshot,
    error: routerError,
    loading: routerLoading,
    refresh: refreshRouter,
    errors: observationErrors(snapshot, modules),
  };
}

type Observation = ReturnType<typeof useRouterObservation>;

export function RouterTime({ value }: { value: string | null | undefined }) {
  if (!value) return <>—</>;
  return (
    <time dateTime={value} title={value}>
      {new Date(value).toLocaleString("zh-CN", { hour12: false })}
    </time>
  );
}

export function RouterToolbar({ observation }: { observation: Observation }) {
  return (
    <div className="page-toolbar">
      <span className="status-text text-muted">
        <Activity size={14} />
        路由器观察
      </span>
      <Button
        size="small"
        aria-label="刷新路由器观察"
        onClick={observation.refresh}
        disabled={observation.loading || !observation.refresh}
      >
        <RefreshCw size={14} className={observation.loading ? "spin" : ""} />
        刷新
      </Button>
    </div>
  );
}

export function RouterObservationFrame({
  title,
  subtitle,
  observation,
  missingTitle = "尚无路由器采样",
  children,
}: {
  title: string;
  subtitle: string;
  observation: Observation;
  missingTitle?: string;
  children: (snapshot: RouterSnapshot) => ReactNode;
}) {
  const { snapshot, error, loading, errors, refresh } = observation;
  const status =
    error !== undefined
      ? "读取失败"
      : loading
        ? snapshot
          ? "更新中 · 上次采样"
          : "正在读取"
        : snapshot
          ? errors.length
            ? "部分观察"
            : "采样快照"
          : "等待采样";
  return (
    <Panel>
      <PanelHeader
        title={title}
        subtitle={subtitle}
        action={
          <Badge
            tone={
              error !== undefined
                ? "danger"
                : errors.length
                  ? "warning"
                  : "neutral"
            }
          >
            {status}
          </Badge>
        }
      />
      {error !== undefined ? (
        <ErrorState
          message={errorMessage(error)}
          onRetry={loading ? undefined : refresh}
        />
      ) : snapshot ? (
        <>
          <RouterObservationErrors errors={errors} />
          {children(snapshot)}
          <div className="table-footer">
            <span>采样时间</span>
            <RouterTime value={snapshot.sampledAt} />
          </div>
        </>
      ) : loading ? (
        <Loading label="正在读取路由器观察" />
      ) : (
        <EmptyState title={missingTitle} />
      )}
    </Panel>
  );
}
