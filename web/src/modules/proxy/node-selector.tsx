import { strings } from "../../locales/strings";
import { Effect } from "effect";
import { useEffect, useMemo, useRef, useState } from "react";
import {
  Badge,
  Button,
  EmptyState,
  ErrorState,
  Field,
  Panel,
  PanelHeader,
} from "../../components/ui/primitives";
import { errorMessage } from "../../lib/api";
import type { ProxyNodes } from "../../lib/contracts";
import type { RuntimeController } from "../runtime/use-runtime";
import { useAcceptedNodeConfig } from "./node-selector-config";
import { NodeProbeBadge, NodeProbeControls } from "./node-probe-controls";
import { useNodeProbes } from "./use-node-probes";
import {
  DEFAULT_ROUTED_TUN,
  NODE_PAGE_SIZE,
  TPROXY_RESERVED_PORT,
  useNodeFavorites,
  useNodePreferences,
  validPort,
  type NodePreferences,
} from "./node-selector-preferences";
import { matchesRegion, NODE_REGIONS } from "./node-selector-regions";
import { selectProxyPolicy } from "./policy-api";
import type { ProxyPolicySummary } from "./policy-contracts";
import { PolicyReview } from "./policy-review";
import "./node-selector.css";

type Node = ProxyNodes["nodes"][number];
function endpoint(node: Node) {
  return `${node.server.includes(":") ? `[${node.server}]` : node.server}:${node.port}`;
}
function NodeSummary({
  title,
  node,
  fallback,
}: {
  title: string;
  node?: Node;
  fallback: string;
}) {
  return (
    <div className="node-selector-node-summary">
      <span className="text-muted text-xs">{title}</span>
      <strong>{node?.label ?? fallback}</strong>
      {node && (
        <span className="mono text-muted text-xs">
          {endpoint(node)} · {node.protocol.toUpperCase()} /{" "}
          {node.transport.toUpperCase()}
        </span>
      )}
    </div>
  );
}

export function NodeSelector({
  nodes,
  runtime,
  onSelected,
  importing = false,
}: {
  nodes?: ProxyNodes & {
    policySummary?: ProxyPolicySummary;
    revision?: string;
  };
  importing?: boolean;
  runtime: RuntimeController;
  onSelected: () => void;
}) {
  const probes = useNodeProbes(nodes?.revision);
  const [preferences, setPreferences] = useNodePreferences();
  const { query, protocol, transport, favoritesOnly, region, ports } =
    preferences;
  const nodeIds = useMemo(
    () => nodes?.nodes.map((node) => node.id),
    [nodes?.nodes],
  );
  const [favorites, toggleFavorite] = useNodeFavorites(nodeIds);
  const favoriteIds = useMemo(() => new Set(favorites), [favorites]);
  const accepted = useAcceptedNodeConfig(runtime, importing);
  const [pending, setPending] = useState(false);
  const submitting = useRef(false);
  const [saved, setSaved] = useState<string>();
  // Deliberate review is component memory only, never a persisted preference.
  const [acknowledgedRevision, setAcknowledgedRevision] = useState<string>();
  const policySummary = nodes?.policySummary;
  const policyRevision = policySummary?.revision;
  const policyAcknowledged =
    !!policyRevision && acknowledgedRevision === policyRevision;
  const policyReady =
    !policySummary || policySummary.omitted === 0 || policyAcknowledged;
  const [focusId, setFocusId] = useState("");
  const rowRefs = useRef(new Map<string, HTMLButtonElement>());
  const listRef = useRef<HTMLDivElement>(null);
  const all = nodes?.nodes ?? [];
  const selectedId = preferences.selectedId || nodes?.selectedNodeId || "";
  const selectedNode = all.find((node) => node.id === selectedId);
  const currentNode = all.find((node) => node.id === nodes?.selectedNodeId);
  const currentProbe = probes.results.find(
    (item) => item.nodeId === currentNode?.id,
  );
  const currentDelay =
    currentProbe?.status === "success" && currentProbe.delayMs !== undefined
      ? `${Math.round(currentProbe.delayMs)} ms`
      : currentProbe?.status === "timeout"
        ? "超时"
        : currentProbe?.status === "unreachable"
          ? "不可达"
          : currentProbe?.status === "probing"
            ? "测速中…"
            : currentProbe?.status === "queued"
              ? "等待测速…"
              : "未测";
  const busy = runtime.pending || pending || importing;
  const configReady =
    accepted.ready &&
    (runtime.status?.configured === false ||
      preferences.acceptedGeneration === accepted.generation);
  const validPorts =
    Object.values(ports).every(validPort) &&
    new Set(Object.values(ports)).size === 2;
  const canSave =
    !busy &&
    runtime.enabled &&
    !!selectedNode &&
    !!runtime.status?.artifactAvailable &&
    configReady &&
    validPorts &&
    policyReady;
  const filtered = useMemo(() => {
    const terms = query.trim().toLocaleLowerCase().split(/\s+/).filter(Boolean);
    return (nodes?.nodes ?? []).filter((node) => {
      const text =
        `${node.label} ${node.server} ${endpoint(node)} ${node.protocol} ${node.transport}`.toLocaleLowerCase();
      return (
        terms.every((term) => text.includes(term)) &&
        (!protocol || node.protocol === protocol) &&
        (!transport || node.transport === transport) &&
        (!favoritesOnly || favoriteIds.has(node.id)) &&
        matchesRegion(node.label, region)
      );
    });
  }, [
    nodes?.nodes,
    query,
    protocol,
    transport,
    favoritesOnly,
    favoriteIds,
    region,
  ]);
  const pageCount = Math.max(1, Math.ceil(filtered.length / NODE_PAGE_SIZE));
  const page = Math.min(preferences.page, pageCount);
  const offset = (page - 1) * NODE_PAGE_SIZE;
  const visibleNodes = filtered.slice(offset, offset + NODE_PAGE_SIZE);
  const selectionFiltered =
    !!selectedNode && !filtered.some((node) => node.id === selectedId);
  const filterActive =
    !!query || !!protocol || !!transport || favoritesOnly || !!region;
  const protocols = [...new Set(all.map((node) => node.protocol))].sort();
  const transports = [...new Set(all.map((node) => node.transport))].sort();
  const draft = !!selectedId && selectedId !== nodes?.selectedNodeId;

  useEffect(() => {
    // Clearing also prevents an old revision from reviving after a missing summary.
    setAcknowledgedRevision(undefined);
  }, [policyRevision]);
  useEffect(() => {
    if (accepted.inputs && accepted.generation !== undefined) {
      const inputs = accepted.inputs;
      setPreferences((previous) =>
        previous.acceptedGeneration === accepted.generation
          ? previous
          : {
              ...previous,
              ports: {
                mixed: inputs.ports.mixed,
                dns: inputs.ports.dns,
              },
              acceptedGeneration: accepted.generation,
            },
      );
    }
  }, [accepted.inputs, accepted.generation, setPreferences]);
  useEffect(() => {
    if (nodes && page !== preferences.page)
      setPreferences((previous) => ({ ...previous, page }));
  }, [nodes, page, preferences.page, setPreferences]);
  useEffect(() => {
    // Page changes stay inside the bounded list, not at the old list scroll offset.
    if (listRef.current) listRef.current.scrollTop = 0;
  }, [page, query, protocol, transport, favoritesOnly, region]);
  useEffect(() => {
    if (!focusId) return;
    const row = rowRefs.current.get(focusId);
    if (row) {
      row.focus({ preventScroll: true });
      row.scrollIntoView?.({ block: "nearest" });
      setFocusId("");
    }
  }, [focusId, page, filtered]);

  function updateFilter(update: Partial<NodePreferences>) {
    setPreferences((previous) => ({ ...previous, ...update, page: 1 }));
  }
  function clearFilters() {
    updateFilter({
      query: "",
      protocol: "",
      transport: "",
      favoritesOnly: false,
      region: "",
    });
  }
  function jumpToNode(id: string) {
    const index = all.findIndex((node) => node.id === id);
    if (index < 0) return;
    setPreferences((previous) => ({
      ...previous,
      query: "",
      protocol: "",
      transport: "",
      favoritesOnly: false,
      region: "",
      page: Math.floor(index / NODE_PAGE_SIZE) + 1,
    }));
    setFocusId(id);
  }
  function selectNode(id: string) {
    setPreferences((previous) => ({ ...previous, selectedId: id }));
    setSaved(undefined);
  }
  async function submit(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!canSave || submitting.current || !event.currentTarget.checkValidity())
      return;
    submitting.current = true;
    setPending(true);
    setSaved(undefined);
    try {
      await runtime.run(
        () =>
          selectProxyPolicy({
            nodeId: selectedId,
            ipv6: "direct",
            failure: "direct",
            ports: {
              mixed: ports.mixed,
              tproxy: TPROXY_RESERVED_PORT,
              dns: ports.dns,
            },
            datapath: "routed-tun",
            routedTUN: {
              ...(accepted.inputs?.routedTUN ?? DEFAULT_ROUTED_TUN),
            },
            ...(policyAcknowledged
              ? { acknowledgedRevision: policyRevision }
              : {}),
          }).pipe(
            Effect.map((response) => {
              setSaved(
                response.status.state === "running"
                  ? strings.proxy.nodes.switchSuccess
                  : "节点配置已保存",
              );
              onSelected();
              return response.status;
            }),
          ),
        "节点配置已保存",
      );
    } finally {
      submitting.current = false;
      setPending(false);
    }
  }
  return (
    <Panel className="node-selector">
      <PanelHeader
        title="网关路由代理"
        subtitle="搜索、收藏并更换节点"
        action={<Badge>{all.length} 个节点</Badge>}
      />
      <form onSubmit={submit}>
        <section className="node-selector-summary" aria-label="节点选择与保存">
          <div className="node-selector-summaries">
            <NodeSummary
              title="当前节点"
              node={currentNode}
              fallback={
                nodes?.selectedNodeId
                  ? "当前节点已不在订阅中"
                  : "尚未保存节点配置"
              }
            />
            <NodeSummary
              title={draft ? "待保存节点" : "已选节点"}
              node={selectedNode}
              fallback={selectedId ? "已选节点已移除" : "尚未选择节点"}
            />
          </div>
          <PolicyReview
            summary={policySummary}
            acknowledgment={{
              checked: policyAcknowledged,
              disabled: busy,
              onChange: (checked) =>
                setAcknowledgedRevision(checked ? policyRevision : undefined),
            }}
          />
          <div className="node-selector-apply">
            <Badge tone={draft ? "warning" : "neutral"}>
              {!selectedId
                ? "尚未选择"
                : draft
                  ? "已选择 · 尚未保存"
                  : "与当前配置一致"}
            </Badge>
            <span className="text-muted text-xs">
              核心：
              {!runtime.status
                ? "状态读取中"
                : runtime.status.state === "running"
                  ? strings.states.running
                  : runtime.status.state === "stopped"
                    ? "已停用"
                    : strings.states.stopped}{" "}
              · 分流配置：
              {runtime.status?.configured ? "规则分流" : "未保存"}· 当前延迟：
              {currentDelay}
            </span>
            <div className="node-selector-quick-actions">
              <Button
                type="button"
                size="small"
                disabled={!currentNode}
                onClick={() => jumpToNode(currentNode!.id)}
              >
                更换节点
              </Button>
              <Button
                type="button"
                size="small"
                disabled={importing || !currentNode || !probes.canStart}
                onClick={() =>
                  void probes.start({ all: false, nodeIds: [currentNode!.id] })
                }
              >
                测试当前延迟
              </Button>
              {draft && (
                <Button
                  type="button"
                  size="small"
                  disabled={busy || !currentNode}
                  onClick={() => {
                    selectNode(currentNode!.id);
                    jumpToNode(currentNode!.id);
                  }}
                >
                  恢复当前节点选择
                </Button>
              )}
              {selectedNode && selectedId !== currentNode?.id && (
                <Button
                  type="button"
                  size="small"
                  onClick={() => jumpToNode(selectedId)}
                >
                  定位已选节点
                </Button>
              )}
            </div>
            <Button type="submit" variant="primary" disabled={!canSave}>
              {pending ? "正在保存…" : "保存配置"}
            </Button>
          </div>
          <p className="text-muted text-xs">保存配置不会自动开启接管。</p>
          {selectionFiltered && (
            <p className="text-muted text-xs">
              已选节点不在当前筛选结果中，保存仍使用此节点。
            </p>
          )}
          {nodes && selectedId && !selectedNode && (
            <p role="alert">已选节点已不在当前订阅中，请重新选择。</p>
          )}
          {!runtime.status?.artifactAvailable && (
            <p className="text-muted text-xs">先在运行管理中获取运行文件。</p>
          )}
          {runtime.status?.configured &&
            !configReady &&
            accepted.error === undefined && (
              <p role="status" className="text-muted text-xs">
                正在读取当前配置…
              </p>
            )}
          {accepted.error !== undefined && (
            <ErrorState
              message={errorMessage(accepted.error)}
              onRetry={accepted.reload}
            />
          )}
          {runtime.error !== undefined && (
            <ErrorState message={errorMessage(runtime.error)} />
          )}
          {saved && <p role="status">{saved}</p>}
        </section>

        <div className="node-selector-controls">
          <div className="node-selector-search-row">
            <Field
              label="搜索节点"
              hint="名称（含标签中的地区文字）、地址、协议或传输；不推断实际地理位置"
            >
              <input
                type="search"
                aria-label="搜索节点"
                value={query}
                placeholder="输入名称、地址或协议"
                autoComplete="off"
                onChange={(event) =>
                  updateFilter({ query: event.target.value })
                }
                onKeyDown={(event) => {
                  if (event.key === "Enter") event.preventDefault();
                }}
              />
            </Field>
            <Button
              type="button"
              size="small"
              disabled={!query}
              onClick={() => updateFilter({ query: "" })}
            >
              清除搜索
            </Button>
          </div>
          <div
            className="node-selector-region-tags"
            role="group"
            aria-label="按名称地区标识筛选"
          >
            {[
              { id: "", label: strings.proxy.nodes.regions.all },
              ...NODE_REGIONS,
              { id: "other", label: strings.proxy.nodes.regions.other },
            ].map((item) => (
              <Button
                key={item.id}
                type="button"
                size="small"
                aria-pressed={region === item.id}
                onClick={() =>
                  updateFilter({ region: item.id as NodePreferences["region"] })
                }
              >
                {item.label}
              </Button>
            ))}
          </div>
          <div className="node-selector-filter-row">
            <Field label="协议筛选">
              <select
                className="select-trigger"
                value={protocol}
                onChange={(event) =>
                  updateFilter({ protocol: event.target.value })
                }
              >
                <option value="">全部协议</option>
                {protocol && !protocols.includes(protocol) && (
                  <option value={protocol}>
                    {protocol.toUpperCase()}（当前订阅无此协议）
                  </option>
                )}
                {protocols.map((value) => (
                  <option key={value} value={value}>
                    {value.toUpperCase()}
                  </option>
                ))}
              </select>
            </Field>
            <Field label="传输筛选">
              <select
                className="select-trigger"
                value={transport}
                onChange={(event) =>
                  updateFilter({ transport: event.target.value })
                }
              >
                <option value="">全部传输</option>
                {transport && !transports.includes(transport) && (
                  <option value={transport}>
                    {transport.toUpperCase()}（当前订阅无此传输）
                  </option>
                )}
                {transports.map((value) => (
                  <option key={value} value={value}>
                    {value.toUpperCase()}
                  </option>
                ))}
              </select>
            </Field>
            <label className="node-selector-favorites-only">
              <input
                type="checkbox"
                aria-label="仅收藏"
                checked={favoritesOnly}
                onChange={(event) =>
                  updateFilter({ favoritesOnly: event.target.checked })
                }
              />
              仅收藏{" "}
              <span className="text-muted">
                ({favorites.filter((id) => nodeIds?.includes(id)).length})
              </span>
            </label>
            <Button
              type="button"
              size="small"
              disabled={!filterActive}
              onClick={clearFilters}
            >
              重置筛选
            </Button>
          </div>
          <p className="text-muted text-xs" role="status" aria-live="polite">
            {filtered.length} 个匹配 / 共 {all.length} 个节点 · 本页{" "}
            {filtered.length ? offset + 1 : 0}–
            {Math.min(offset + NODE_PAGE_SIZE, filtered.length)} · 每页{" "}
            {NODE_PAGE_SIZE} 个
          </p>
        </div>
        <NodeProbeControls
          controller={probes}
          pageNodeIds={visibleNodes.map((node) => node.id)}
          allCount={all.length}
          disabled={importing || !nodes}
        />
        <nav className="node-selector-pagination" aria-label="节点分页">
          <Button
            type="button"
            size="small"
            disabled={page <= 1}
            onClick={() =>
              setPreferences((previous) => ({ ...previous, page: page - 1 }))
            }
          >
            上一页
          </Button>
          <label>
            页码{" "}
            <select
              className="select-trigger"
              value={page}
              disabled={!filtered.length}
              onChange={(event) =>
                setPreferences((previous) => ({
                  ...previous,
                  page: Number(event.target.value),
                }))
              }
            >
              {Array.from({ length: pageCount }, (_, index) => (
                <option key={index + 1} value={index + 1}>
                  第 {index + 1} / {pageCount} 页
                </option>
              ))}
            </select>
          </label>
          <Button
            type="button"
            size="small"
            disabled={page >= pageCount}
            onClick={() =>
              setPreferences((previous) => ({ ...previous, page: page + 1 }))
            }
          >
            下一页
          </Button>
        </nav>
        <div className="node-selector-list" ref={listRef}>
          <ul className="node-selector-cards" aria-label="代理节点列表">
            {visibleNodes.map((node) => (
              <li
                key={node.id}
                className={`node-selector-card${selectedId === node.id ? " node-selector-card-selected" : ""}`}
              >
                <button
                  ref={(element) => {
                    if (element) rowRefs.current.set(node.id, element);
                    else rowRefs.current.delete(node.id);
                  }}
                  className="node-selector-select"
                  disabled={busy}
                  type="button"
                  aria-label={`选择节点 ${node.label}`}
                  aria-pressed={selectedId === node.id}
                  onClick={() => selectNode(node.id)}
                >
                  <span className="node-selector-name">
                    <strong>{node.label}</strong>
                    <span className="mono text-muted">{endpoint(node)}</span>
                    <span className="node-selector-row-state">
                      {node.id === nodes?.selectedNodeId && (
                        <Badge>当前配置</Badge>
                      )}
                      {selectedId === node.id && (
                        <Badge tone="primary">已选</Badge>
                      )}
                    </span>
                  </span>
                  <span className="node-selector-protocol">
                    <strong>
                      {node.protocol.toUpperCase()} /{" "}
                      {node.transport.toUpperCase()}
                    </strong>
                    <span className="text-muted">
                      {[
                        node.reality && "REALITY",
                        node.vision && "Vision",
                        node.utls && "uTLS",
                        node.udp && "UDP",
                      ]
                        .filter(Boolean)
                        .join(" · ") || "—"}
                    </span>
                  </span>
                </button>
                <NodeProbeBadge
                  controller={probes}
                  nodeId={node.id}
                  nodeLabel={node.label}
                  disabled={importing}
                />
                <Button
                  type="button"
                  className="node-selector-favorite"
                  size="small"
                  aria-label={`${favoriteIds.has(node.id) ? "取消收藏" : "收藏"}节点 ${node.label}`}
                  aria-pressed={favoriteIds.has(node.id)}
                  disabled={importing}
                  onClick={() => toggleFavorite(node.id)}
                >
                  <span aria-hidden="true">
                    {favoriteIds.has(node.id) ? "★" : "☆"}
                  </span>
                </Button>
              </li>
            ))}
          </ul>
        </div>
        {!all.length ? (
          <EmptyState title="暂无节点" detail="先导入订阅" />
        ) : (
          !filtered.length && (
            <EmptyState
              title="没有匹配的节点"
              detail={
                favoritesOnly
                  ? "当前没有匹配的收藏节点。可关闭收藏筛选或重置筛选。"
                  : "尝试更短的关键词，或重置协议和传输筛选。"
              }
            >
              <Button type="button" onClick={clearFilters}>
                显示全部节点
              </Button>
            </EmptyState>
          )
        )}

        <div className="config-form node-selector-config">
          <details className="node-selector-advanced">
            <summary>高级设置 · 监听端口</summary>
            <div className="form-grid">
              {(["mixed", "dns"] as const).map((name) => (
                <Field key={name} label={`${name} 监听端口`}>
                  <input
                    disabled={busy || !configReady}
                    type="number"
                    required
                    min={1}
                    max={65535}
                    value={Number.isNaN(ports[name]) ? "" : ports[name]}
                    onChange={(event) => {
                      setPreferences((previous) => ({
                        ...previous,
                        ports: {
                          ...previous.ports,
                          [name]: event.target.valueAsNumber,
                        },
                      }));
                      setSaved(undefined);
                    }}
                  />
                </Field>
              ))}
            </div>
          </details>
          {!validPorts && (
            <p role="alert">
              监听端口须为 1–65535 的整数，且两个端口不能重复。
            </p>
          )}
        </div>
      </form>
    </Panel>
  );
}
