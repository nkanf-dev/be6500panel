import { useResource } from "../../lib/use-resource";
import { featuresApi, type FeatureDomain } from "../../lib/features-api";

export function useFeaturesCatalog() {
  const resource = useResource(featuresApi.catalog);

  const getDomain = (domainId: string): FeatureDomain | undefined => {
    return resource.data?.domains.find((d) => d.id === domainId);
  };

  return {
    catalog: resource.data,
    pendingOperation: resource.data?.pendingOperation,
    loading: resource.loading,
    error: resource.error,
    reload: resource.reload,
    getDomain,
  };
}
