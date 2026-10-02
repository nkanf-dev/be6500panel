import { useMemo, useState } from "react";
import { Network, RefreshCw, Search } from "lucide-react";
import {
  Badge,
  Button,
  EmptyState,
  ErrorState,
  Loading,
  Panel,
  PanelHeader,
} from "../components/ui/primitives";
import { api, errorMessage } from "../lib/api";
import { useResource } from "../lib/use-resource";
import { useConsole } from "../app/console-context";
import { RouterNetworkObservations } from "./router-network";
import { ConfigurationEditor } from "../components/configuration";
import { RouterViewTabs, type RouterView } from "./router-view-tabs";

export function NetworkPage() {
  const { data, error, loading, reload } = useResource(api.network);
  const { routerLoading = false, refreshRouter } = useConsole();
  const refreshObservations = () => {
    reload();
    refreshRouter?.();
  };
  const refreshing = loading || routerLoading;
  const [view, setView] = useState<RouterView>("observation");
  const [query, setQuery] = useState("");
  const [filter, setFilter] = useState("all");
  const [selected, setSelected] = useState<string>();
  const interfaces = useMemo(
    () =>
      data?.interfaces.filter(
        (item) =>
          `${item.name} ${item.addresses.join(" ")}`
            .toLowerCase()
            .includes(query.toLowerCase()) &&
          (filter === "all" || item.up === (filter === "up")),
      ) ?? [],
    [data, query, filter],
  );
  const detail = data?.interfaces.find((item) => item.name === selected);
  return (
    <div className="page-stack">
      <div className="page-toolbar">
        <RouterViewTabs label="网络视图" value={view} onChange={setView} />
      </div>
      {view === "configuration" ? (
        <ConfigurationEditor module="network" />
      ) : (
        <>
          <div className="page-toolbar">
            <div className="toolbar-left">
              <label className="search-field">
                <Search size={15} />
                <input
                  aria-label="筛选接口"
                  placeholder="筛选名称或地址…"
                  value={query}
                  onChange={(event) => setQuery(event.target.value)}
                />
                <kbd>/</kbd>
              </label>
              <div className="segmented" aria-label="接口状态">
                {[
                  { id: "all", label: "全部" },
                  { id: "up", label: "UP" },
                  { id: "down", label: "DOWN" },
                ].map((item) => (
                  <button
                    key={item.id}
                    aria-pressed={filter === item.id}
                    onClick={() => setFilter(item.id)}
                  >
                    {item.label}
                  </button>
                ))}
              </div>
            </div>
            <Button
              size="small"
              onClick={refreshObservations}
              disabled={refreshing}
            >
              <RefreshCw size={14} className={refreshing ? "spin" : ""} />
              刷新
            </Button>
          </div>
          {error !== undefined && (
            <ErrorState message={errorMessage(error)} onRetry={reload} />
          )}
          <div className={detail ? "detail-layout" : ""}>
            <Panel>
              <PanelHeader
                title="网络接口"
                subtitle="当前宿主 · Go net observation"
                action={<Badge>{interfaces.length} 个接口</Badge>}
              />
              {loading && !data ? (
                <Loading />
              ) : (
                <div className="table-scroll">
                  <table className="data-table">
                    <thead>
                      <tr>
                        <th>接口</th>
                        <th>链路</th>
                        <th>地址</th>
                        <th className="align-right">MTU</th>
                        <th className="align-right">详情</th>
                      </tr>
                    </thead>
                    <tbody>
                      {interfaces.map((item) => (
                        <tr
                          key={item.name}
                          className={
                            selected === item.name ? "row-selected" : ""
                          }
                        >
                          <td>
                            <span className="table-title">
                              <Network size={16} />
                              <strong className="mono">{item.name}</strong>
                            </span>
                          </td>
                          <td>
                            <Badge tone={item.up ? "success" : "neutral"}>
                              {item.up ? "UP" : "DOWN"}
                            </Badge>
                          </td>
                          <td>
                            <div className="address-list">
                              {item.addresses.length ? (
                                item.addresses.map((address) => (
                                  <span className="mono" key={address}>
                                    {address}
                                  </span>
                                ))
                              ) : (
                                <span className="text-muted">无地址</span>
                              )}
                            </div>
                          </td>
                          <td className="mono align-right">{item.mtu}</td>
                          <td className="align-right">
                            <Button
                              variant="ghost"
                              size="small"
                              aria-label={`查看 ${item.name}`}
                              onClick={() =>
                                setSelected(
                                  selected === item.name
                                    ? undefined
                                    : item.name,
                                )
                              }
                            >
                              查看
                            </Button>
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                  {!interfaces.length && (
                    <EmptyState title={query ? "无匹配接口" : "无接口数据"} />
                  )}
                </div>
              )}
              <div className="table-footer">
                <span>{interfaces.length} 条记录</span>
                <span>选择接口查看详情</span>
              </div>
            </Panel>
            {detail && (
              <Panel className="detail-panel">
                <PanelHeader
                  title={detail.name}
                  subtitle="接口详情"
                  action={
                    <Button
                      variant="ghost"
                      size="small"
                      onClick={() => setSelected(undefined)}
                    >
                      关闭
                    </Button>
                  }
                />
                <dl className="key-values">
                  <div>
                    <dt>名称</dt>
                    <dd className="mono">{detail.name}</dd>
                  </div>
                  <div>
                    <dt>状态</dt>
                    <dd>{detail.up ? "UP" : "DOWN"}</dd>
                  </div>
                  <div>
                    <dt>MTU</dt>
                    <dd>{detail.mtu} bytes</dd>
                  </div>
                  <div>
                    <dt>地址数</dt>
                    <dd>{detail.addresses.length}</dd>
                  </div>
                </dl>
                <div className="detail-addresses">
                  {detail.addresses.map((address) => (
                    <code key={address}>{address}</code>
                  ))}
                </div>
              </Panel>
            )}
          </div>
          <RouterNetworkObservations />
        </>
      )}
    </div>
  );
}
