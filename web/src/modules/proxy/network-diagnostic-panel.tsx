import { useId, useState } from "react";
import { Button, ErrorState } from "../../components/ui/primitives";
import { RequestWaterfall } from "../../components/visualizations";
import { errorMessage } from "../../lib/api";
import { requestTraceCopy as copy } from "./request-trace-copy";
import type {
  RequestTraceRoute,
  RequestTraceTargetId,
} from "./request-trace-contracts";
import { useRequestTraces } from "./use-request-traces";

/** Production-only active diagnostics. No effect or refresh action starts a probe. */
export function NetworkDiagnosticPanel() {
  const id = useId();
  const { data, error, loading, running, refresh, run } = useRequestTraces();
  const [targetId, setTargetId] = useState<RequestTraceTargetId>("google204");
  const [route, setRoute] = useState<RequestTraceRoute>("direct");
  const targets = data?.targets ?? [];
  const target = targets.find((item) => item.id === targetId);
  const canRun = !!data && !!target && !loading && !running;
  return (
    <div className="page-stack">
      <div className="page-toolbar" aria-label={copy.runAction}>
        <label className="viz-filter" htmlFor={`${id}-target`}>
          <span>{copy.target}</span>
          <select
            id={`${id}-target`}
            aria-label={copy.target}
            value={targetId}
            disabled={loading || running || targets.length === 0}
            onChange={(event) => {
              const value = event.target.value;
              if (value === "google204" || value === "cloudflare")
                setTargetId(value);
            }}
          >
            {targets.map((item) => (
              <option key={item.id} value={item.id}>
                {item.label}
              </option>
            ))}
          </select>
        </label>
        <label className="viz-filter" htmlFor={`${id}-route`}>
          <span>{copy.route}</span>
          <select
            id={`${id}-route`}
            aria-label={copy.route}
            value={route}
            disabled={running}
            onChange={(event) => {
              const value = event.target.value;
              if (value === "direct" || value === "proxy") setRoute(value);
            }}
          >
            <option value="direct">{copy.modeDirect}</option>
            <option value="proxy">{copy.modeProxy}</option>
          </select>
        </label>
        <Button
          type="button"
          variant="primary"
          disabled={!canRun}
          onClick={() => {
            void run({ targetId, route });
          }}
        >
          {running ? copy.testing : copy.runAction}
        </Button>
        <Button
          type="button"
          size="small"
          disabled={loading || running}
          onClick={refresh}
        >
          {copy.refresh}
        </Button>
      </div>
      <p className="text-muted text-xs">
        {copy.explicitOnly}
        {target && <> {target.url}</>}
      </p>
      <p className="text-muted text-xs">{copy.currentPolicy}</p>
      <p className="text-muted text-xs" role="status">
        {running
          ? copy.testing
          : loading
            ? copy.loading
            : data
              ? copy.limits
              : copy.emptyTitle}
      </p>
      {error !== undefined && (
        <ErrorState message={errorMessage(error)} onRetry={refresh} />
      )}
      <RequestWaterfall traces={data?.traces} />
    </div>
  );
}
