import { lazy, Suspense } from "react";
import { Loading } from "../components/ui/primitives";
import type { PageId } from "../modules/registry";

const Overview = lazy(() =>
  import("../modules/overview").then((module) => ({
    default: module.OverviewPage,
  })),
);
const System = lazy(() =>
  import("../modules/system").then((module) => ({
    default: module.SystemPage,
  })),
);
const Network = lazy(() =>
  import("../modules/network").then((module) => ({
    default: module.NetworkPage,
  })),
);
const Proxy = lazy(() =>
  import("../modules/proxy").then((module) => ({ default: module.ProxyPage })),
);
const Frpc = lazy(() =>
  import("../modules/frpc").then((module) => ({ default: module.FrpcPage })),
);
const Unavailable = lazy(() =>
  import("../modules/unavailable").then((module) => ({
    default: module.UnavailablePage,
  })),
);

export function ModulePage({
  page,
  navigate,
}: {
  page: PageId;
  navigate: (id: PageId) => void;
}) {
  const content =
    page === "overview" ? (
      <Overview navigate={navigate} />
    ) : page === "system" ? (
      <System />
    ) : page === "network" ? (
      <Network />
    ) : page === "proxy" ? (
      <Proxy />
    ) : page === "frpc" ? (
      <Frpc />
    ) : (
      <Unavailable key={page} id={page} />
    );
  return (
    <Suspense fallback={<Loading label="载入模块…" />}>{content}</Suspense>
  );
}
