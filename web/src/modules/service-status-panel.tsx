import { useCallback, useEffect, useRef, useState } from "react";
import * as Dialog from "@radix-ui/react-dialog";
import { RefreshCw, Search } from "lucide-react";
import {
  Badge,
  Button,
  EmptyState,
  ErrorState,
  Loading,
  Panel,
  PanelHeader,
} from "../components/ui/primitives";
import { ApiError, errorMessage } from "../lib/api";
import { bytes, uptime } from "../lib/format";
import {
  loadServiceStatus,
  runServiceAction,
  type ServiceAction,
  type ServiceActionInput,
  type ServiceActionResult,
  type ServiceObservation,
  type ServiceStatusSnapshot,
} from "../lib/service-status-api";

const POLL_MS = 5000;
const STALE_MS = 15000;

function observationIsStale(snapshot: ServiceStatusSnapshot, now: number) {
  if (snapshot.stale || snapshot.sampledAt === null) return true;
  const sampledAt = Date.parse(snapshot.sampledAt);
  return !Number.isFinite(sampledAt) || now - sampledAt >= STALE_MS;
}

/** Mounted observation and command work share one in-flight guard and cancellation. */
export function useServiceStatus() {
  const [data, setData] = useState<ServiceStatusSnapshot>();
  const [error, setError] = useState<unknown>();
  const [loading, setLoading] = useState(true);
  const [fetching, setFetching] = useState(false);
  const [actionPending, setActionPending] = useState(false);
  const [actionError, setActionError] = useState<unknown>();
  const [actionResult, setActionResult] = useState<ServiceActionResult>();
  const [now, setNow] = useState(Date.now);
  const loadRef = useRef<() => void>(() => {});
  const actionRef = useRef<(input: ServiceActionInput) => Promise<void>>(
    async () => {},
  );
  const refresh = useCallback(() => loadRef.current(), []);
  const runAction = useCallback(
    (input: ServiceActionInput) => actionRef.current(input),
    [],
  );

  useEffect(() => {
    let current = true;
    let controller: AbortController | undefined;
    let lastSnapshot: ServiceStatusSnapshot | undefined;
    const remember = (snapshot: ServiceStatusSnapshot) => {
      lastSnapshot = snapshot;
      setData(snapshot);
      setNow(Date.now());
    };
    const load = async () => {
      if (!current || controller) return;
      const requestController = new AbortController();
      controller = requestController;
      setFetching(true);
      try {
        const value = await loadServiceStatus(requestController.signal);
        if (current) {
          remember(value);
          setError(undefined);
        }
      } catch (cause) {
        if (current) {
          setError(cause);
          if (lastSnapshot) remember({ ...lastSnapshot, stale: true });
        }
      } finally {
        if (current) {
          controller = undefined;
          setLoading(false);
          setFetching(false);
        }
      }
    };
    actionRef.current = async (input) => {
      if (!current || controller) return;
      const permitted =
        lastSnapshot &&
        !observationIsStale(lastSnapshot, Date.now()) &&
        lastSnapshot.services.some(
          (service) =>
            service.name === input.service &&
            availableActions(service, false).includes(input.action),
        );
      if (!permitted || (input.service === "dnsmasq" && !input.confirmImpact)) {
        setActionError(
          new ApiError({
            code: "service_action_refused",
            message: "最新观察不允许此操作，或影响尚未确认",
          }),
        );
        return;
      }
      const requestController = new AbortController();
      controller = requestController;
      setActionPending(true);
      setActionError(undefined);
      setActionResult(undefined);
      if (lastSnapshot) remember({ ...lastSnapshot, stale: true });
      try {
        const result = await runServiceAction(input, requestController.signal);
        if (current) {
          setActionResult(result);
          remember(result.snapshot);
          setError(undefined);
        }
      } catch (cause) {
        if (current) setActionError(cause);
      } finally {
        if (current) {
          controller = undefined;
          setActionPending(false);
          // An HTTP error may hide a readback envelope. Always observe again.
          void load();
        }
      }
    };
    loadRef.current = () => void load();
    void load();
    const poll = setInterval(() => void load(), POLL_MS);
    // Age still advances if the read is pending or the backend returns an old sample.
    const clock = setInterval(() => setNow(Date.now()), 1000);
    return () => {
      current = false;
      loadRef.current = () => {};
      actionRef.current = async () => {};
      clearInterval(poll);
      clearInterval(clock);
      controller?.abort();
    };
  }, []);

  const snapshot =
    data && observationIsStale(data, now) && !data.stale
      ? { ...data, stale: true }
      : data;
  return {
    snapshot,
    error,
    loading,
    fetching,
    refresh,
    now,
    actionPending,
    actionError,
    actionResult,
    runAction,
  };
}

function verifiedRunning(service: ServiceObservation) {
  return (
    service.processState === "running" &&
    service.pid !== undefined &&
    Number.isInteger(service.pid) &&
    service.pid > 0
  );
}
function processLabel(service: ServiceObservation) {
  if (verifiedRunning(service)) return `运行中 (PID ${service.pid})`;
  if (service.processState === "not_running") return "未运行";
  if (service.processState === "failed") return "启动异常";
  return "状态未知";
}
function processTone(service: ServiceObservation) {
  if (verifiedRunning(service)) return "success";
  if (service.processState === "failed") return "danger";
  if (service.processState === "not_running") return "neutral";
  return "warning";
}
function displayTime(value: string | null) {
  return value
    ? new Date(value).toLocaleString("zh-CN", { hour12: false })
    : "从未成功采样";
}
function rowIsUnknown(service: ServiceObservation, stale: boolean) {
  return (
    stale ||
    service.processState === "unknown" ||
    (service.processState === "running" && !verifiedRunning(service)) ||
    service.configured === "unknown" ||
    service.registered === "unknown"
  );
}
function rowHasFailure(service: ServiceObservation) {
  return service.processState === "failed" || Boolean(service.errorCode);
}
type ServiceFilter = "all" | "attention" | "failed" | "unknown";

const actionLabels: Record<ServiceAction, string> = {
  start: "启动",
  stop: "停止",
  restart: "重启",
  reload: "重载",
};
function rescueProtected(service: ServiceObservation) {
  return (
    service.protected ||
    service.name === "be6500-rescue" ||
    service.name === "rescue"
  );
}
function availableActions(
  service: ServiceObservation,
  stale: boolean,
): ServiceAction[] {
  if (stale || rescueProtected(service) || service.configured !== "present")
    return [];
  const allowlist: ServiceAction[] =
    service.name === "ddns"
      ? ["start", "stop", "restart", "reload"]
      : service.name === "dnsmasq"
        ? ["reload", "restart"]
        : [];
  return allowlist.filter(
    (action) =>
      service.actions?.includes(action) &&
      (action !== "stop" || verifiedRunning(service)),
  );
}

function ServiceCard({
  service,
  stale,
  busy,
  onSelect,
}: {
  service: ServiceObservation;
  stale: boolean;
  busy: boolean;
  onSelect?: (service: ServiceObservation, action: ServiceAction) => void;
}) {
  const actions = onSelect ? availableActions(service, stale) : [];
  return (
    <article
      className="panel min-w-0"
      aria-label={`${service.name} · ${service.instance}`}
    >
      <header className="flex min-w-0 flex-wrap items-start justify-between gap-3 border-b border-border p-4">
        <div className="min-w-0">
          <h3 className="mono wrap font-semibold">{service.name}</h3>
          <p className="text-muted wrap text-xs">
            实例：{service.instance || "（未提供）"}
          </p>
          {rescueProtected(service) && (
            <p className="text-muted mt-2 text-xs">
              {service.name === "be6500-rescue" || service.name === "rescue"
                ? "独立救援通道 · 受保护 · 仅观察"
                : service.name === "dropbear" || service.name === "be6500panel"
                  ? "管理服务 · 受保护（本区域不操作）"
                  : "受保护 · 本区域仅观察"}
            </p>
          )}
        </div>
        <Badge tone={stale ? "warning" : processTone(service)}>
          {stale ? "状态未知（旧快照）" : processLabel(service)}
        </Badge>
      </header>
      {stale && (
        <p className="text-muted wrap px-4 pt-3 text-xs">
          上次记录：{processLabel(service)}
        </p>
      )}
      <dl className="key-values">
        <div>
          <dt>init 脚本</dt>
          <dd>
            {service.configured === "present"
              ? "脚本存在"
              : service.configured === "absent"
                ? "脚本不存在"
                : "脚本状态未知"}
          </dd>
        </div>
        <div>
          <dt>procd 注册</dt>
          <dd>
            {service.registered === "registered"
              ? "已注册"
              : service.registered === "unregistered"
                ? "未注册"
                : "注册状态未知"}
          </dd>
        </div>
        <div>
          <dt>procd 运行报告</dt>
          <dd>
            {service.procdRunning === undefined
              ? "未知"
              : service.procdRunning
                ? "报告运行（非进程核验）"
                : "报告未运行"}
          </dd>
        </div>
        <div>
          <dt>procd 报告 PID</dt>
          <dd className="mono">{service.reportedPID ?? "未提供"}</dd>
        </div>
        <div>
          <dt>已核验 PID</dt>
          <dd className="mono">{service.pid ?? "未核验"}</dd>
        </div>
        <div>
          <dt>可执行文件</dt>
          <dd className="mono wrap">{service.executable ?? "未提供"}</dd>
        </div>
        <div>
          <dt>运行时长</dt>
          <dd>
            {service.uptimeSeconds === undefined
              ? "未提供"
              : uptime(service.uptimeSeconds)}
          </dd>
        </div>
        <div>
          <dt>RSS 内存</dt>
          <dd className="mono">
            {service.rssBytes === undefined
              ? "未提供"
              : bytes(service.rssBytes)}
          </dd>
        </div>
        <div>
          <dt>观察错误码</dt>
          <dd className="mono wrap">{service.errorCode ?? "无"}</dd>
        </div>
      </dl>
      {service.processState === "running" && !verifiedRunning(service) && (
        <p className="text-muted px-4 pb-3 text-xs">缺少经 /proc 核验的 PID</p>
      )}
      {actions.length > 0 && (
        <div
          className="flex flex-wrap gap-2 border-t border-border p-4"
          aria-label={`${service.name} 受限操作`}
        >
          {actions.map((action) => (
            <Button
              key={action}
              type="button"
              size="small"
              disabled={busy}
              onClick={() => onSelect?.(service, action)}
            >
              {actionLabels[action]} {service.name}
            </Button>
          ))}
          <p className="text-muted wrap w-full text-xs">
            来自最新观察的已安装服务许可；操作需单独确认。
          </p>
        </div>
      )}
    </article>
  );
}

/** Pure view. Optional command callback exposes only backend-listed allowed actions. */
export function ServiceStatusView({
  snapshot,
  error,
  loading = false,
  fetching = false,
  onRefresh,
  onAction,
  actionPending = false,
  actionError,
  actionResult,
  now = Date.now(),
}: {
  snapshot?: ServiceStatusSnapshot;
  error?: unknown;
  loading?: boolean;
  fetching?: boolean;
  onRefresh?: () => void;
  onAction?: (input: ServiceActionInput) => Promise<void>;
  actionPending?: boolean;
  actionError?: unknown;
  actionResult?: ServiceActionResult;
  now?: number;
}) {
  const [query, setQuery] = useState("");
  const [filter, setFilter] = useState<ServiceFilter>("all");
  const [selected, setSelected] = useState<{
    name: string;
    instance: string;
    action: ServiceAction;
  }>();
  const [impactConfirmed, setImpactConfirmed] = useState(false);
  const [confirming, setConfirming] = useState(false);
  const stale = Boolean(
    snapshot && (error !== undefined || observationIsStale(snapshot, now)),
  );
  const selectedRow = snapshot?.services.find(
    (service) =>
      service.name === selected?.name &&
      service.instance === selected?.instance,
  );
  const canConfirm = Boolean(
    onAction &&
    selected &&
    selectedRow &&
    availableActions(selectedRow, stale).includes(selected.action) &&
    (selected.name !== "dnsmasq" || impactConfirmed) &&
    !loading &&
    !fetching &&
    !actionPending &&
    !confirming,
  );
  const selectedAvailable = Boolean(
    selected &&
    selectedRow &&
    availableActions(selectedRow, stale).includes(selected.action),
  );
  const selectAction = (service: ServiceObservation, action: ServiceAction) => {
    setSelected({ name: service.name, instance: service.instance, action });
    setImpactConfirmed(false);
  };
  const confirmAction = async () => {
    if (!canConfirm || !selected || !onAction) return;
    setConfirming(true);
    try {
      await onAction({
        service: selected.name,
        action: selected.action,
        confirmImpact: impactConfirmed,
      });
    } finally {
      setConfirming(false);
      setSelected(undefined);
    }
  };
  const search = query.trim().toLocaleLowerCase();
  const services = (snapshot?.services ?? []).filter((service) => {
    const matchesSearch = [
      service.name,
      service.instance,
      service.executable,
      service.errorCode,
    ]
      .filter(Boolean)
      .join(" ")
      .toLocaleLowerCase()
      .includes(search);
    const failed = rowHasFailure(service);
    const unknown = rowIsUnknown(service, stale);
    return (
      matchesSearch &&
      (filter === "all" ||
        (filter === "attention" && (failed || unknown)) ||
        (filter === "failed" && failed) ||
        (filter === "unknown" && unknown))
    );
  });
  return (
    <Panel aria-label="系统服务真实状态">
      <PanelHeader
        title="系统服务真实状态"
        subtitle="读取 procd 与 /proc；脚本存在、已注册均不代表进程运行。仅提供最新状态允许的受限操作，操作需单独确认。"
        action={
          onRefresh && (
            <Button
              type="button"
              size="small"
              onClick={onRefresh}
              disabled={loading || fetching || actionPending || confirming}
            >
              <RefreshCw size={14} className={loading ? "spin" : ""} />
              刷新观察
            </Button>
          )
        }
      />
      <div className="page-toolbar p-4">
        <div className="toolbar-left">
          <label className="search-field">
            <Search size={14} />
            <input
              type="search"
              aria-label="搜索服务与实例"
              placeholder="搜索服务、实例或错误码"
              value={query}
              onChange={(event) => setQuery(event.target.value)}
            />
          </label>
          <label className="field">
            <span className="sr-only">服务状态筛选</span>
            <select
              className="select-trigger"
              aria-label="服务状态筛选"
              value={filter}
              onChange={(event) =>
                setFilter(event.target.value as ServiceFilter)
              }
            >
              <option value="all">全部服务</option>
              <option value="attention">异常 / 未知</option>
              <option value="failed">异常</option>
              <option value="unknown">未知</option>
            </select>
          </label>
        </div>
        <a
          className="text-primary text-xs underline"
          href="#/system?tab=diagnostics"
        >
          查看诊断与日志
        </a>
      </div>
      {error !== undefined && <ErrorState message={errorMessage(error)} />}
      {actionError !== undefined && (
        <ErrorState message={errorMessage(actionError)} />
      )}
      {actionResult && (
        <div role="status" className="wrap px-5 py-3 text-xs">
          <p className="mono">
            {actionResult.service} · {actionLabels[actionResult.action]}
          </p>
          <p>
            {actionResult.commandAccepted
              ? "命令已接受，不代表服务运行或健康；以下以实际观察为准。"
              : "命令未被接受；请查看错误码与实际观察。"}
          </p>
          {actionResult.errorCode && (
            <p>
              操作错误码：<code>{actionResult.errorCode}</code>
            </p>
          )}
        </div>
      )}
      {snapshot && (
        <>
          <dl className="key-values">
            <div>
              <dt>观察来源</dt>
              <dd className="mono wrap">{snapshot.source}</dd>
            </div>
            <div>
              <dt>最后成功采样</dt>
              <dd>
                <time dateTime={snapshot.sampledAt ?? undefined}>
                  {displayTime(snapshot.sampledAt)}
                </time>
              </dd>
            </div>
            <div>
              <dt>后端最近尝试</dt>
              <dd>
                <time dateTime={snapshot.checkedAt}>
                  {displayTime(snapshot.checkedAt)}
                </time>
              </dd>
            </div>
            <div>
              <dt>快照状态</dt>
              <dd>
                <Badge tone={stale ? "warning" : "neutral"}>
                  {stale ? "已过期 / 当前未知" : "最近观察（非健康检查）"}
                </Badge>
              </dd>
            </div>
          </dl>
          {stale && (
            <p role="status" className="text-muted wrap px-5 py-3 text-xs">
              观察已过期，当前进程状态未知。以下仅为上次记录。
            </p>
          )}
          {snapshot.errorCode && (
            <p className="wrap px-5 py-2 text-xs">
              采样错误码：<code>{snapshot.errorCode}</code>
            </p>
          )}
          {snapshot.errors.length > 0 && (
            <div
              role="alert"
              aria-label="服务观察来源错误"
              className="px-5 py-3 text-xs"
            >
              {snapshot.errors.map((item, index) => (
                <p
                  className="mono wrap"
                  key={`${item.module}-${item.code}-${index}`}
                >
                  {item.module} · {item.code} · {item.message}
                </p>
              ))}
            </div>
          )}
        </>
      )}
      {!snapshot && loading && <Loading label="正在读取服务观察" />}
      {!snapshot && !loading && (
        <EmptyState
          title="服务观察不可用"
          detail="刷新仅重试观察；请在诊断与日志中查看错误。"
        />
      )}
      {snapshot && services.length > 0 && (
        <div className="grid min-w-0 gap-3 p-4 xl:grid-cols-2">
          {services.map((service, index) => (
            <ServiceCard
              key={`${service.name}-${service.instance}-${index}`}
              service={service}
              stale={stale}
              busy={loading || fetching || actionPending || confirming}
              onSelect={onAction ? selectAction : undefined}
            />
          ))}
        </div>
      )}
      {snapshot && services.length === 0 && (
        <EmptyState
          title={
            snapshot.services.length === 0
              ? "暂无服务观察记录"
              : "没有匹配的服务"
          }
          detail={
            snapshot.services.length === 0
              ? "没有记录不代表所有服务已停止。请查看采样错误与诊断。"
              : "调整搜索或状态筛选。"
          }
        />
      )}
      <p className="text-muted wrap px-5 py-3 text-xs">
        挂载时每 5 秒读取一次，15
        秒未更新会标记旧快照。刷新只重试读取，不启动、停止或重载服务。
      </p>
      <Dialog.Root
        open={selected !== undefined}
        onOpenChange={(open) => {
          if (!open && !confirming && !actionPending) setSelected(undefined);
        }}
      >
        <Dialog.Portal>
          <Dialog.Overlay className="dialog-overlay" />
          <Dialog.Content className="command-dialog max-h-[75dvh] overflow-y-auto p-5">
            <Dialog.Title className="text-base font-semibold">
              {selected
                ? `确认${actionLabels[selected.action]} ${selected.name}`
                : "确认服务操作"}
            </Dialog.Title>
            <Dialog.Description className="text-muted wrap mt-2 text-xs">
              仅执行本次选定的 init
              服务命令。操作作用于整个服务，可能影响全部实例。
              所选实例仅用于显示观察上下文，不是单独操作目标。
              命令被接受不代表进程运行、网络连通或服务健康。
            </Dialog.Description>
            {selected && (
              <p className="mono wrap my-4">
                {selected.name} · {selected.instance} ·{" "}
                {actionLabels[selected.action]}
              </p>
            )}
            {selectedRow?.actionImpact && (
              <p className="wrap my-3 text-xs">{selectedRow.actionImpact}</p>
            )}
            {selected?.name === "dnsmasq" && (
              <label className="flex items-start gap-2 text-xs">
                <input
                  type="checkbox"
                  checked={impactConfirmed}
                  disabled={confirming || actionPending}
                  onChange={(event) => setImpactConfirmed(event.target.checked)}
                />
                <span>我确认本次操作可能短暂中断 DNS/DHCP 服务</span>
              </label>
            )}
            {!selectedAvailable && !actionPending && (
              <p role="alert" className="wrap my-3 text-xs">
                最新观察已不允许此操作，请关闭确认并刷新观察。
              </p>
            )}
            <div className="mt-5 flex flex-wrap justify-end gap-2">
              <Button
                type="button"
                disabled={confirming || actionPending}
                onClick={() => setSelected(undefined)}
              >
                取消
              </Button>
              <Button
                type="button"
                variant="primary"
                disabled={!canConfirm}
                onClick={() => void confirmAction()}
              >
                {confirming || actionPending
                  ? "正在执行选定操作…"
                  : selected
                    ? `确认${actionLabels[selected.action]} ${selected.name}`
                    : "确认操作"}
              </Button>
            </div>
          </Dialog.Content>
        </Dialog.Portal>
      </Dialog.Root>
    </Panel>
  );
}

export function ServiceStatusPanel() {
  const observation = useServiceStatus();
  return (
    <ServiceStatusView
      {...observation}
      onRefresh={observation.refresh}
      onAction={observation.runAction}
    />
  );
}
