import { useFeaturesCatalog } from "./use-features-catalog";
import { FeatureDomainPanel } from "./feature-domain-panel";
import { ErrorState, Loading } from "../../components/ui/primitives";
import { errorMessage } from "../../lib/api";

export function WirelessFeaturesPanel({
  defaultReadId,
  onPending,
}: {
  defaultReadId?: string;
  onPending?: (pending: boolean) => void;
}) {
  const { catalog, pendingOperation, getDomain, loading, error, reload } = useFeaturesCatalog();
  const domain = getDomain("wireless");

  if (loading && !domain) {
    return <Loading label="正在载入无线网络能力…" />;
  }

  if (error !== undefined && !domain) {
    return <ErrorState message={errorMessage(error)} onRetry={reload} />;
  }

  if (!domain) {
    return (
      <div className="p-4 text-center text-muted text-sm">
        暂未发现无线配置项或功能未启用
      </div>
    );
  }

  return (
    <FeatureDomainPanel
      domain={domain}
      catalogGeneration={catalog?.generation}
      pendingOperation={pendingOperation}
      onReloadCatalog={reload}
      defaultReadId={defaultReadId}
      onPending={onPending}
    />
  );
}
