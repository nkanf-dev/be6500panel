import { useEffect, useMemo, useState, type ReactNode } from "react";
import {
  ArrowLeft,
  ChevronLeft,
  ChevronRight,
  GitCompareArrows,
  Pencil,
  RefreshCw,
  Search,
  X,
} from "lucide-react";
import {
  Badge,
  Button,
  EmptyState,
  ErrorState,
  Loading,
  Panel,
  PanelHeader,
} from "../../components/ui/primitives";
import { errorMessage } from "../../lib/api";
import type { RouterSnapshot } from "../../lib/contracts";
import { bytes, uptime } from "../../lib/format";
import type { ProxyMetrics } from "../proxy/telemetry-api";
import { canonicalMAC, useDeviceLabels } from "./device-labels";
import { DeviceHistoryCharts } from "./device-history-charts";
import { DeviceProxyDetails } from "./device-proxy-details";
import { EditDeviceAnnotation } from "./edit-device-annotation";
import {
  mergeDeviceInventory,
  type DeviceHistory,
  type DeviceRange,
  type WorkspaceDevice,
} from "./device-model";
import "./devices.css";

export interface DeviceWorkspaceProps {
  snapshot?: RouterSnapshot;
  snapshotError?: unknown;
  activity?: DeviceHistory;
  activityError?: unknown;
  proxy?: ProxyMetrics;
  proxyError?: unknown;
  loading?: boolean;
  onRefresh?: () => void;
  range?: DeviceRange;
  onRangeChange?: (range: DeviceRange) => void;
  selectedMAC?: string;
  onSelectDevice?: (mac: string | undefined) => void;
  /** Exact MACs needed for detail/compare history. Bound to eight and never causes writes. */
  onSelectedDevicesChange?: (macs: readonly string[]) => void;
  onConfigure?: (module: "dhcp" | "firewall", mac: string) => void;
  /** Root-owned confirmed network actions can be mounted for this MAC. */
  renderActions?: (device: WorkspaceDevice) => ReactNode;
}
const pageSize = 25;
export function DeviceWorkspace({
  snapshot,
  snapshotError,
  activity,
  activityError,
  proxy,
  proxyError,
  loading = false,
  onRefresh,
  range,
  onRangeChange,
  selectedMAC,
  onSelectDevice,
  onSelectedDevicesChange,
  onConfigure,
  renderActions,
}: DeviceWorkspaceProps) {
  const labels = useDeviceLabels();
  const inventory = useMemo(
    () =>
      mergeDeviceInventory(
        snapshotError === undefined ? snapshot : undefined,
        activity,
      ),
    [snapshot, snapshotError, activity],
  );
  const [selected, setSelected] = useState<string | undefined>(
    selectedMAC ? canonicalMAC(selectedMAC) : undefined,
  );
  const [retained, setRetained] = useState<WorkspaceDevice>();
  const [query, setQuery] = useState("");
  const [status, setStatus] = useState("all");
  const [page, setPage] = useState(0);
  const [compared, setCompared] = useState<string[]>([]);
  const [comparing, setComparing] = useState(false);
  useEffect(() => {
    onSelectedDevicesChange?.(
      comparing && compared.length >= 2 ? compared : selected ? [selected] : [],
    );
  }, [comparing, compared, selected, onSelectedDevicesChange]);
  const [editing, setEditing] = useState<WorkspaceDevice>();
  const [localRange, setLocalRange] = useState<DeviceRange>(
    range ?? activity?.range ?? "24h",
  );
  useEffect(() => {
    if (selectedMAC !== undefined) setSelected(canonicalMAC(selectedMAC));
  }, [selectedMAC]);
  const selectedDevice = inventory.find((device) => device.mac === selected);
  useEffect(() => {
    if (selectedDevice) setRetained(selectedDevice);
  }, [selectedDevice]);
  const detail =
    selectedDevice ?? (retained?.mac === selected ? retained : undefined);
  const displayName = (device: WorkspaceDevice) =>
    labels.displayName(device.mac, device.hostname);
  const selectedRange = range ?? localRange;
  const filterDevices = useMemo(
    () =>
      inventory.filter((device) => {
        const annotation = labels.annotations[device.mac];
        const match =
          `${labels.displayName(device.mac, device.hostname)} ${device.hostname} ${device.mac} ${device.addresses.join(" ")} ${annotation?.note ?? ""} ${annotation?.tags.join(" ") ?? ""}`
            .toLowerCase()
            .includes(query.trim().toLowerCase());
        return (
          match &&
          (status === "all" ||
            (status === "associated"
              ? device.activity?.associated && !device.activity.stale
              : status === "arp"
                ? device.leases.some((lease) => lease.online)
                : device.activity?.stale ||
                  (!device.activity?.associated &&
                    !device.leases.some((lease) => lease.online))))
        );
      }),
    [inventory, query, status, labels.annotations, labels.displayName],
  );
  const pageCount = Math.max(1, Math.ceil(filterDevices.length / pageSize));
  const currentPage = Math.min(page, pageCount - 1);
  const pageDevices = filterDevices.slice(
    currentPage * pageSize,
    (currentPage + 1) * pageSize,
  );
  const compareDevices = compared.map(
    (mac) =>
      inventory.find((device) => device.mac === mac) ??
      (detail?.mac === mac
        ? detail
        : ({
            mac,
            hostname: "",
            addresses: [],
            currentAddresses: [],
            leases: [],
          } as WorkspaceDevice)),
  );
  const select = (mac: string | undefined) => {
    setSelected(mac);
    setComparing(false);
    onSelectDevice?.(mac);
  };
  const toggleCompare = (mac: string) =>
    setCompared((previous) =>
      previous.includes(mac)
        ? previous.filter((item) => item !== mac)
        : previous.length < 8
          ? [...previous, mac]
          : previous,
    );
  const changeRange = (value: DeviceRange) => {
    setLocalRange(value);
    onRangeChange?.(value);
  };
  const actionRefresh = () => {
    labels.refresh();
    onRefresh?.();
  };
  const activeDetail = (
    comparing && compared.length >= 2 ? compareDevices : detail ? [detail] : []
  ).map((device) => ({
    ...device,
    activity:
      activity?.range === selectedRange
        ? inventory.find((current) => current.mac === device.mac)?.activity
        : undefined,
  }));
  return (
    <div className="page-stack device-workspace">
      <div className="page-toolbar device-workspace-toolbar">
        <div className="toolbar-left">
          <label className="search-field">
            <Search size={15} />
            <input
              aria-label="搜索设备"
              placeholder="搜索备注、名称、MAC、IP、标签…"
              value={query}
              onChange={(event) => {
                setQuery(event.target.value);
                setPage(0);
              }}
            />
          </label>
          <label className="device-filter">
            <span>设备状态</span>
            <select
              aria-label="设备状态"
              value={status}
              onChange={(event) => {
                setStatus(event.target.value);
                setPage(0);
              }}
            >
              <option value="all">全部设备</option>
              <option value="associated">已关联</option>
              <option value="arp">ARP 已观测</option>
              <option value="stale">离线 / 上次采样</option>
            </select>
          </label>
        </div>
        <Button
          size="small"
          onClick={actionRefresh}
          disabled={loading || labels.loading}
        >
          <RefreshCw size={14} className={loading ? "spin" : ""} />
          刷新设备
        </Button>
      </div>
      {snapshotError !== undefined && (
        <ErrorState
          message={`设备地址与租约读取失败：${errorMessage(snapshotError)}`}
          onRetry={onRefresh}
        />
      )}
      {activityError !== undefined && (
        <ErrorState
          message={`设备流量读取失败：${errorMessage(activityError)}`}
          onRetry={onRefresh}
        />
      )}
      {labels.error !== undefined && (
        <ErrorState
          message={`设备备注读取失败：${errorMessage(labels.error)}`}
          onRetry={labels.refresh}
        />
      )}
      <div className="device-workspace-grid">
        <Panel className="device-inventory">
          <PanelHeader
            title="设备管理"
            subtitle="按 MAC 汇总设备 · 自定义名称与备注 · 详情与对比"
            action={<Badge>{inventory.length} 个设备</Badge>}
          />
          {selected &&
            !pageDevices.some((device) => device.mac === selected) && (
              <div className="device-pinned-selection">
                <span>当前查看：{detail ? displayName(detail) : selected}</span>
                <Button size="small" onClick={() => select(selected)}>
                  查看详情
                </Button>
                <Button
                  size="icon"
                  variant="ghost"
                  aria-label="关闭设备详情"
                  onClick={() => select(undefined)}
                >
                  <X size={14} />
                </Button>
              </div>
            )}
          <div className="device-compare-toolbar">
            <span>
              {compared.length
                ? `已选择 ${compared.length} / 8 个设备`
                : "勾选 2–8 个设备进行同范围对比"}
            </span>
            <Button
              size="small"
              onClick={() => setComparing(true)}
              disabled={compared.length < 2}
            >
              <GitCompareArrows size={14} />
              对比设备
            </Button>
            {compared.length > 0 && (
              <Button
                size="small"
                variant="ghost"
                onClick={() => {
                  setCompared([]);
                  setComparing(false);
                }}
              >
                清除选择
              </Button>
            )}
          </div>
          {loading && !inventory.length ? (
            <Loading label="正在读取设备与流量采样" />
          ) : (
            <div className="table-scroll">
              <table className="data-table device-list">
                <thead>
                  <tr>
                    <th>对比</th>
                    <th>设备 / 地址</th>
                    <th>连接状态</th>
                    <th>所选范围 RX / TX</th>
                    <th>操作</th>
                  </tr>
                </thead>
                <tbody>
                  {pageDevices.map((device) => (
                    <tr
                      key={device.mac}
                      className={selected === device.mac ? "row-selected" : ""}
                      onClick={() => select(device.mac)}
                    >
                      <td onClick={(event) => event.stopPropagation()}>
                        <input
                          type="checkbox"
                          aria-label={`对比 ${displayName(device)}`}
                          checked={compared.includes(device.mac)}
                          disabled={
                            !compared.includes(device.mac) &&
                            compared.length >= 8
                          }
                          onChange={() => toggleCompare(device.mac)}
                        />
                      </td>
                      <th scope="row">
                        <button
                          className="device-title-button"
                          onClick={(event) => {
                            event.stopPropagation();
                            select(device.mac);
                          }}
                        >
                          {displayName(device)}
                        </button>
                        <small className="device-subtitle">
                          系统名称：{device.hostname || "未提供"}
                        </small>
                        <small className="device-subtitle mono">
                          {device.addresses.join(" · ") || "暂无地址"} ·{" "}
                          {device.mac}
                        </small>
                        {device.activity?.interface && (
                          <small className="device-subtitle">
                            {device.activity.interface}
                          </small>
                        )}
                        {labels.annotations[device.mac]?.tags.length ? (
                          <div className="device-tags">
                            {labels.annotations[device.mac].tags.map((tag) => (
                              <Badge key={tag}>{tag}</Badge>
                            ))}
                          </div>
                        ) : null}
                      </th>
                      <td>
                        <DeviceState device={device} />
                      </td>
                      <td>
                        {activity?.range === selectedRange &&
                        device.activity &&
                        device.activity.coverageSeconds > 0 ? (
                          <>
                            {bytes(device.activity.rxBytes)}
                            <small className="device-subtitle">
                              {bytes(device.activity.txBytes)}
                            </small>
                          </>
                        ) : (
                          <span className="text-muted">等待有效计数</span>
                        )}
                      </td>
                      <td onClick={(event) => event.stopPropagation()}>
                        <Button
                          size="icon"
                          variant="ghost"
                          aria-label={`编辑 ${displayName(device)} 备注`}
                          onClick={() => setEditing(device)}
                        >
                          <Pencil size={14} />
                        </Button>
                        <Button
                          size="small"
                          variant="ghost"
                          aria-label={`查看 ${displayName(device)} 详情`}
                          onClick={() => select(device.mac)}
                        >
                          详情
                        </Button>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
              {!pageDevices.length && (
                <EmptyState
                  title={
                    query || status !== "all"
                      ? "没有匹配设备"
                      : "尚未取得设备采样"
                  }
                  detail={
                    query || status !== "all"
                      ? "调整搜索词或设备状态，当前详情和对比选择会保留。"
                      : "刷新设备以读取 DHCP / ARP 地址和 trafficd 关联计数。"
                  }
                >
                  {onRefresh && (
                    <Button size="small" onClick={actionRefresh}>
                      刷新设备采样
                    </Button>
                  )}
                </EmptyState>
              )}
            </div>
          )}
          <div className="table-footer">
            <span>
              {filterDevices.length} 条 · 每页最多 {pageSize} 个
            </span>
            <div className="device-paging">
              <Button
                size="icon"
                aria-label="设备上一页"
                disabled={currentPage === 0}
                onClick={() => setPage(currentPage - 1)}
              >
                <ChevronLeft size={14} />
              </Button>
              <span>
                {currentPage + 1} / {pageCount}
              </span>
              <Button
                size="icon"
                aria-label="设备下一页"
                disabled={currentPage + 1 >= pageCount}
                onClick={() => setPage(currentPage + 1)}
              >
                <ChevronRight size={14} />
              </Button>
            </div>
          </div>
          {activity?.truncated && (
            <p className="device-source-note">
              流量列表当前显示 {activity.devices.length} /{" "}
              {activity.matchedCount} 个匹配设备。打开设备详情以读取该 MAC
              的范围记录。
            </p>
          )}
        </Panel>
        {activeDetail.length > 0 ? (
          <section
            className="device-detail-stack"
            aria-label={comparing ? "设备对比详情" : "设备详情"}
          >
            <Panel>
              <PanelHeader
                title={
                  comparing
                    ? `设备对比 · ${activeDetail.length} 个`
                    : displayName(activeDetail[0])
                }
                subtitle={
                  comparing
                    ? "相同时间范围、计数器方向和数据源"
                    : activeDetail[0].mac
                }
                action={
                  <Button
                    size="small"
                    variant="ghost"
                    onClick={() =>
                      comparing ? setComparing(false) : select(undefined)
                    }
                  >
                    <ArrowLeft size={14} />
                    {comparing ? "返回详情" : "关闭详情"}
                  </Button>
                }
              />
              <div className="device-range">
                <span>流量时间范围</span>
                <div className="segmented" aria-label="设备流量范围">
                  {(
                    [
                      ["30m", "30 分钟"],
                      ["24h", "24 小时"],
                      ["7d", "7 天"],
                    ] as const
                  ).map(([id, text]) => (
                    <button
                      key={id}
                      aria-pressed={selectedRange === id}
                      onClick={() => changeRange(id)}
                    >
                      {text}
                    </button>
                  ))}
                </div>
              </div>
              {activity?.range !== selectedRange ? (
                <p className="device-source-note" role="status">
                  正在等待所选时间范围记录；其他范围的计数不会混用。
                </p>
              ) : null}
              {activity?.state === "stale" || activityError !== undefined ? (
                <p className="device-source-note" role="status">
                  流量采样已过期，显示上次采样；刷新后查看当前计数。
                </p>
              ) : null}
              <p className="device-source-note">
                流量来源：{activity?.source || "trafficd"} · 采样时间：
                {activity?.sampledAt
                  ? new Date(activity.sampledAt).toLocaleString("zh-CN")
                  : "等待采样"}
                {activity?.error ? ` · ${activity.error}` : ""}
              </p>
              {comparing ? (
                <div className="device-compare-chips">
                  {activeDetail.map((device) => (
                    <Badge key={device.mac}>
                      {displayName(device)}
                      <button
                        aria-label={`移除对比 ${displayName(device)}`}
                        onClick={() => {
                          toggleCompare(device.mac);
                          if (compared.length <= 2) setComparing(false);
                        }}
                      >
                        <X size={12} />
                      </button>
                    </Badge>
                  ))}
                </div>
              ) : (
                <DeviceHardware
                  device={activeDetail[0]}
                  onEdit={() => setEditing(activeDetail[0])}
                  onConfigure={onConfigure}
                  renderActions={renderActions}
                />
              )}
            </Panel>
            <DeviceHistoryCharts
              devices={activeDetail}
              resolutionSeconds={activity?.resolutionSeconds ?? 0}
              source={activity?.source ?? "trafficd"}
            />
            <DeviceProxyDetails
              devices={activeDetail}
              inventory={inventory}
              metrics={proxy}
              error={proxyError}
              snapshot={snapshotError === undefined ? snapshot : undefined}
              onRefresh={onRefresh}
            />
          </section>
        ) : (
          <Panel className="device-detail-placeholder">
            <EmptyState
              title="选择设备查看综合详情"
              detail="查看设备身份、租约、无线链路、实际流量、核心连接与备注。勾选设备可对比同一时间范围的数据。"
            />
          </Panel>
        )}
      </div>
      {editing && (
        <EditDeviceAnnotation
          key={editing.mac}
          device={editing}
          onClose={() => setEditing(undefined)}
        />
      )}
    </div>
  );
}
function DeviceState({ device }: { device: WorkspaceDevice }) {
  return (
    <div className="device-state">
      <Badge
        tone={
          device.activity?.stale
            ? "warning"
            : device.activity?.associated
              ? "success"
              : "neutral"
        }
      >
        {device.activity?.stale
          ? "上次关联采样"
          : device.activity?.associated
            ? "已关联"
            : device.leases.some((lease) => lease.online)
              ? "ARP 已观测"
              : "未见关联 / ARP"}
      </Badge>
      {device.activity?.lastSeen && (
        <small className="device-subtitle">
          {new Date(device.activity.lastSeen).toLocaleString("zh-CN")}
        </small>
      )}
    </div>
  );
}
function DeviceHardware({
  device,
  onEdit,
  onConfigure,
  renderActions,
}: {
  device: WorkspaceDevice;
  onEdit: () => void;
  onConfigure?: DeviceWorkspaceProps["onConfigure"];
  renderActions?: DeviceWorkspaceProps["renderActions"];
}) {
  const annotation = useDeviceLabels().annotations[device.mac];
  return (
    <div className="device-hardware">
      <div className="device-detail-actions">
        <Button size="small" onClick={onEdit}>
          <Pencil size={14} />
          编辑设备备注
        </Button>
        {onConfigure && (
          <>
            <Button
              size="small"
              onClick={() => onConfigure("dhcp", device.mac)}
            >
              静态地址配置
            </Button>
            <Button
              size="small"
              onClick={() => onConfigure("firewall", device.mac)}
            >
              端口映射配置
            </Button>
          </>
        )}
        {renderActions?.(device)}
      </div>
      <dl className="key-values">
        <div>
          <dt>系统原始名称</dt>
          <dd>{device.hostname || "未提供"}</dd>
        </div>
        <div>
          <dt>MAC 地址</dt>
          <dd className="mono">{device.mac}</dd>
        </div>
        <div>
          <dt>设备地址</dt>
          <dd className="mono">
            {device.addresses.join(" · ") || "暂未取得地址"}
          </dd>
        </div>
        <div>
          <dt>连接方式 / 接口</dt>
          <dd>{device.activity?.interface || "等待关联接口采样"}</dd>
        </div>
        <div>
          <dt>关联状态</dt>
          <dd>
            <DeviceState device={device} />
          </dd>
        </div>
        {device.activity?.vendor && (
          <div>
            <dt>厂商</dt>
            <dd>{device.activity.vendor}</dd>
          </div>
        )}
        {device.activity?.onlineSeconds !== undefined && (
          <div>
            <dt>固件累计在线时间</dt>
            <dd>{uptime(device.activity.onlineSeconds)}</dd>
          </div>
        )}
        {device.activity?.ageingSeconds !== undefined && (
          <div>
            <dt>计数器 ageing</dt>
            <dd>{device.activity.ageingSeconds} 秒</dd>
          </div>
        )}
        <div>
          <dt>详细备注</dt>
          <dd className="device-note-text">
            {annotation?.note || "添加备注，记录设备用途和维护事项。"}
          </dd>
        </div>
        <div>
          <dt>设备标签</dt>
          <dd className="device-tags">
            {annotation?.tags.length
              ? annotation.tags.map((tag) => <Badge key={tag}>{tag}</Badge>)
              : "尚未添加"}
          </dd>
        </div>
      </dl>
      <details className="device-hardware-details" open>
        <summary>DHCP 租约与地址计数</summary>
        <div className="table-scroll">
          <table className="data-table">
            <thead>
              <tr>
                <th>地址</th>
                <th>租约到期</th>
                <th>ARP 状态</th>
                <th>原始 RX / TX (B)</th>
              </tr>
            </thead>
            <tbody>
              {device.addresses.map((address) => {
                const lease = device.leases.find((row) => row.ip === address);
                const counter = device.activity?.counters?.find(
                  (row) => row.address === address,
                );
                return (
                  <tr key={address}>
                    <td className="mono">{address}</td>
                    <td>
                      {lease?.expiresAt
                        ? new Date(lease.expiresAt).toLocaleString("zh-CN")
                        : lease
                          ? "未提供到期时间"
                          : "未见 DHCP 租约"}
                    </td>
                    <td>
                      {lease ? (lease.online ? "已观测" : "未观测") : "—"}
                    </td>
                    <td>
                      {counter
                        ? `${counter.rxBytes} / ${counter.txBytes}`
                        : "未导出计数"}
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
        <p className="device-source-note">
          原始 RX / TX 为 trafficd
          固件当前累计计数；与上方所选范围的采样差值分开显示。
        </p>
      </details>
      {device.activity?.links?.length ? (
        <details className="device-hardware-details" open>
          <summary>无线 / 关联链路</summary>
          <div className="table-scroll">
            <table className="data-table">
              <thead>
                <tr>
                  <th>接口</th>
                  <th>协议 / MLO</th>
                  <th>信号 / 噪声</th>
                  <th>协商 RX / TX</th>
                </tr>
              </thead>
              <tbody>
                {device.activity.links.map((link, index) => (
                  <tr key={`${link.interface}-${index}`}>
                    <td>{link.interface}</td>
                    <td>
                      {link.protocol || "未导出"}
                      {link.mld === true && " · MLO"}
                    </td>
                    <td>
                      {link.signalDBM !== undefined
                        ? `${link.signalDBM} dBm`
                        : "未导出信号"}{" "}
                      /{" "}
                      {link.noiseDBM !== undefined
                        ? `${link.noiseDBM} dBm`
                        : "未导出噪声"}
                    </td>
                    <td>
                      {link.negotiatedRX || "未导出"} /{" "}
                      {link.negotiatedTX || "未导出"}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </details>
      ) : null}
      {device.activity?.addressConflicts?.length ? (
        <ErrorState
          message={`地址存在多个设备身份：${device.activity.addressConflicts.join("、")}。这些地址不用于代理连接归属。`}
        />
      ) : null}
    </div>
  );
}
