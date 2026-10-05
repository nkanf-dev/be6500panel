import { useState, useEffect, useCallback, useMemo, useRef } from "react";
import { RefreshCw, Search, AlertCircle } from "lucide-react";
import {
  Badge,
  Button,
  ErrorState,
  Loading,
  Panel,
  PanelHeader,
} from "../../components/ui/primitives";
import {errorMessage,runRequest} from "../../lib/api";
import {
  featuresApi,
  type FeatureDomain,
  type FeatureOperation,
  type FeatureState,
} from "../../lib/features-api";
import { FeatureFieldInput } from "./feature-field-input";
import { FeatureActionForm } from "./feature-action-form";
import { StateDataViewer } from "./state-data-viewer";
import { GlobalFeatureOperationBanner } from "./global-feature-operation-banner";

interface FeatureDomainPanelProps {
  domain: FeatureDomain;
  catalogGeneration?: number;
  pendingOperation?: FeatureOperation;
  defaultReadId?: string;
  onPending?: (pending: boolean) => void;
  onReloadCatalog?: () => void;
}

export function FeatureDomainPanel({
  domain,
  catalogGeneration,
  pendingOperation,
  defaultReadId,
  onPending,
  onReloadCatalog,
}: FeatureDomainPanelProps) {
  const initialRead =
    domain.reads.find((r) => r.id === defaultReadId) ?? domain.reads[0];
  const [selectedReadId, setSelectedReadId] = useState<string>(
    initialRead?.id ?? "",
  );

  const currentRead = domain.reads.find((r) => r.id === selectedReadId);

  // Getter query parameters for currentRead.fields
  const [queryParams, setQueryParams] = useState<Record<string, unknown>>({});
  const [targetPreFill, setTargetPreFill] = useState<Record<string, unknown>>();

  const [state, setState] = useState<FeatureState>();
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<unknown>();
  const [refreshRevision, setRefreshRevision] = useState(0);
  const activeReqSeq = useRef(0);

  // Check if current read has required getter parameters
  const hasRequiredParams = useMemo(() => {
    return currentRead?.fields.some((f) => f.required) ?? false;
  }, [currentRead]);

  const areRequiredParamsFilled = useMemo(() => {
    if (!currentRead) return true;
    for (const f of currentRead.fields) {
      if (f.required && (queryParams[f.key] === undefined || queryParams[f.key] === "")) {
        return false;
      }
    }
    return true;
  }, [currentRead, queryParams]);

  const loadState = useCallback(
    async (signal?: AbortSignal) => {
      if (!currentRead) return;
      // If required parameters are not filled, don't execute empty query
      if (hasRequiredParams && !areRequiredParamsFilled) return;

      const seq = ++activeReqSeq.current;
      setLoading(true);

      try {
        const stringParams: Record<string, string | number | boolean> = {};
        for (const [k, v] of Object.entries(queryParams)) {
          if (v !== undefined && v !== "") {
            stringParams[k] = v as string | number | boolean;
          }
        }
        const result = await runRequest(
          featuresApi.state(domain.id, currentRead.id, stringParams),
          signal,
        );
        if (seq !== activeReqSeq.current || signal?.aborted) return;
        setState(result);
        setError(undefined);
      } catch (cause) {
        if (seq !== activeReqSeq.current || signal?.aborted) return;
        setError(cause);
      } finally {
        if (seq === activeReqSeq.current && !signal?.aborted) {
          setLoading(false);
        }
      }
    },
    [domain.id, currentRead, queryParams, hasRequiredParams, areRequiredParamsFilled],
  );

  // The read-tab click resets its context before the next request. Do not
  // reset queryParams again on mount: that starts and cancels duplicate reads.

  // Auto-fetch ONLY if there are no required getter fields
  useEffect(() => {
    if (!hasRequiredParams) {
      const controller = new AbortController();
      void loadState(controller.signal);
      return () => controller.abort();
    }
  }, [selectedReadId, refreshRevision, hasRequiredParams, loadState]);

  const handleManualFetch = () => {
    void loadState();
  };

  const handleRefresh = () => {
    setRefreshRevision((v) => v + 1);
  };

  // Find actions corresponding to this read view, allowing creation actions like ddns_add on list view
  const matchingActions = domain.actions.filter(
    (action) =>
      !action.readback ||
      action.readback === selectedReadId ||
      (selectedReadId === "ddns" && action.id === "ddns_add"),
  );

  const isUnconfigured = state?.data?.configured === false;

  const statusBadge = loading && !state ? (
    <Badge tone="neutral">读取中</Badge>
  ) : error !== undefined ? (
    <Badge tone="danger">{state ? "读取失败 · 上次数据" : "读取失败"}</Badge>
  ) : isUnconfigured ? (
    <Badge tone="neutral">未配置</Badge>
  ) : state?.available ? (
    <Badge tone="success">已读取</Badge>
  ) : !state ? (
    <Badge tone="neutral">未读取</Badge>
  ) : (
    <Badge tone="warning">不可用</Badge>
  );

  return (
    <div className="page-stack">
      {pendingOperation && (
        <GlobalFeatureOperationBanner
          operation={pendingOperation}
          onConfirmed={onReloadCatalog}
          onRefresh={() => {
            onReloadCatalog?.();
            handleRefresh();
          }}
        />
      )}
      {domain.reads.length > 0 && (
        <div className="page-toolbar">
          <div className="segmented" role="tablist" aria-label={`${domain.title}功能分组`}>
            {domain.reads.map((read) => (
              <button
                key={read.id}
                role="tab"
                aria-selected={read.id === selectedReadId}
                disabled={loading}
                onClick={() => {
                  setSelectedReadId(read.id);
                  setQueryParams({});
                  setTargetPreFill(undefined);
                  setState(undefined);
                  setError(undefined);
                }}
              >
                {read.title}
              </button>
            ))}
          </div>
          <Button
            size="small"
            variant="ghost"
            disabled={loading}
            onClick={handleRefresh}
          >
            <RefreshCw
              size={14}
              className={`mr-1 inline ${loading ? "animate-spin" : ""}`}
            />
            刷新
          </Button>
        </div>
      )}

      {/* Getter Query Parameter Controls */}
      {currentRead && currentRead.fields.length > 0 && (
        <Panel aria-label="查询参数">
          <PanelHeader
            title={`${currentRead.title} · 查询条件`}
            subtitle="输入指定参数以获取对应配置数据"
          />
          <div className="config-form page-stack">
            <div className="form-grid gap-3">
              {currentRead.fields.map((field) => (
                <FeatureFieldInput
                  key={field.key}
                  field={field}
                  value={queryParams[field.key]}
                  disabled={loading}
                  onChange={(val) =>
                    setQueryParams((prev) => ({ ...prev, [field.key]: val }))
                  }
                />
              ))}
            </div>
            <div className="form-actions justify-end">
              <Button
                type="button"
                variant="primary"
                size="small"
                disabled={loading || !areRequiredParamsFilled}
                onClick={handleManualFetch}
              >
                <Search size={14} className="mr-1 inline" />
                读取数据
              </Button>
            </div>
          </div>
        </Panel>
      )}

      {loading && !state && <Loading label={`正在读取 ${currentRead?.title || domain.title}…`} />}

      {error !== undefined && (
        <ErrorState
          message={`${errorMessage(error)}${state ? " · 当前保留上次成功读取的记录" : ""}`}
          onRetry={handleManualFetch}
        />
      )}

      {/* Main State Data & Action Panel */}
      {state && (
        <Panel aria-label={currentRead?.title || domain.title}>
          <PanelHeader
            title={currentRead?.title || domain.title}
            subtitle={
              state.sampledAt
                ? `采样时间：${new Date(state.sampledAt).toLocaleTimeString()}`
                : undefined
            }
            action={statusBadge}
          />

          <div className="config-form page-stack">
            {state.errors && state.errors.length > 0 && (
              <div className="space-y-1">
                {state.errors.map((err, idx) => (
                  <div
                    key={idx}
                    className="flex items-center gap-2 text-xs text-danger bg-danger/10 px-3 py-2 rounded"
                  >
                    <AlertCircle size={14} />
                    <span>
                      {err.code}: {err.message}
                    </span>
                  </div>
                ))}
              </div>
            )}

            {/* Projected State Data (scalars, tables, cards) */}
            <StateDataViewer
              data={state.data}
              onSelectTarget={(item) => setTargetPreFill(item)}
            />

            {/* Editable Action Forms */}
            {matchingActions.map((action) => (
              <FeatureActionForm
                key={action.id}
                domain={domain.id}
                action={action}
                state={state}
                catalogGeneration={catalogGeneration}
                targetPreFill={targetPreFill}
                onSuccess={handleRefresh}
                onPending={onPending}
              />
            ))}
          </div>
        </Panel>
      )}
    </div>
  );
}
