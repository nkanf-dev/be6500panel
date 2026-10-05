import { useState, useEffect } from "react";
import { AlertTriangle, ExternalLink, Loader2, RefreshCw } from "lucide-react";
import { Button, ErrorState } from "../../components/ui/primitives";
import { errorMessage, runRequest } from "../../lib/api";
import {
  featuresApi,
  formatReconnectUrl,
  pollOperation,
  type FeatureOperation,
} from "../../lib/features-api";

interface GlobalFeatureOperationBannerProps {
  operation: FeatureOperation;
  onConfirmed?: () => void;
  onRefresh?: () => void;
}

export function GlobalFeatureOperationBanner({
  operation: initialOperation,
  onConfirmed,
  onRefresh,
}: GlobalFeatureOperationBannerProps) {
  const [operation, setOperation] = useState<FeatureOperation>(initialOperation);
  const [confirming, setConfirming] = useState(false);
  const [error, setError] = useState<unknown>();

  // Active polling if operation is pending and not yet ready to confirm
  useEffect(() => {
    setOperation(initialOperation);
    if (initialOperation.state === "pending" && !initialOperation.canConfirm) {
      const controller = new AbortController();
      pollOperation(initialOperation.id, {
        signal: controller.signal,
        initialOperation,
        onProgress: (op) => setOperation(op),
      })
        .then((finalOp) => {
          setOperation(finalOp);
          if (finalOp.state === "completed") {
            onConfirmed?.();
          }
        })
        .catch((cause) => {
          if (!controller.signal.aborted) {
            setError(cause);
          }
        });
      return () => controller.abort();
    }
  }, [initialOperation.id, initialOperation.state, initialOperation.canConfirm]);

  const handleConfirm = async () => {
    setConfirming(true);
    setError(undefined);
    try {
      const res = await runRequest(featuresApi.confirm(operation.id));
      if (res.state === "completed") {
        onConfirmed?.();
      } else if (res.state === "failed") {
        throw new Error(res.error || "配置确认未成功");
      }
    } catch (cause) {
      setError(cause);
    } finally {
      setConfirming(false);
    }
  };

  const isPending = operation.state === "pending" && !operation.canConfirm;
  const reconnectUrl = formatReconnectUrl(operation.reconnectAddress);

  return (
    <div
      role="region"
      aria-label="连接与任务确认"
      className="p-4 rounded-lg border border-warning/80 bg-warning/10 space-y-3 shadow-sm"
    >
      <div className="flex items-start justify-between gap-3">
        <div className="flex items-start gap-2.5">
          {isPending ? (
            <Loader2 className="text-warning mt-0.5 shrink-0 animate-spin" size={18} />
          ) : (
            <AlertTriangle className="text-warning mt-0.5 shrink-0" size={18} />
          )}
          <div className="text-xs space-y-1">
            <h4 className="font-semibold text-sm text-foreground">
              {operation.canConfirm
                ? "配置已应用，等待连接确认"
                : "配置正在后台应用中…"}
            </h4>
            {operation.waitingFor && (
              <p className="text-muted leading-relaxed">{operation.waitingFor}</p>
            )}
            {operation.reconnectAddress && (
              <p className="text-foreground pt-0.5">
                新管理地址：
                <a
                  href={reconnectUrl}
                  target="_blank"
                  rel="noreferrer"
                  className="font-mono font-medium underline text-primary hover:text-primary/80 inline-flex items-center gap-1"
                >
                  {reconnectUrl || operation.reconnectAddress}
                  <ExternalLink size={11} className="inline" />
                </a>
              </p>
            )}
          </div>
        </div>

        <div className="flex items-center gap-2 shrink-0">
          {onRefresh && isPending && (
            <Button
              type="button"
              variant="ghost"
              size="small"
              onClick={onRefresh}
            >
              <RefreshCw size={13} className="mr-1 inline" />
              刷新状态
            </Button>
          )}

          {operation.canConfirm && (
            <Button
              type="button"
              variant="primary"
              size="small"
              disabled={confirming}
              onClick={handleConfirm}
            >
              {confirming ? "正在确认…" : "确认连接正常"}
            </Button>
          )}
        </div>
      </div>

      {error !== undefined && (
        <ErrorState message={errorMessage(error)} onRetry={handleConfirm} />
      )}
    </div>
  );
}
