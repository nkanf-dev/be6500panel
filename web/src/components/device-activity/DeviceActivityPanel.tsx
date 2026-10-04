import { useId, useMemo, useState } from "react";
import { Activity, RefreshCw } from "lucide-react";
import { errorMessage } from "../../lib/api";
import { normalizeDeviceActivitySearch } from "../../lib/device-activity-api";
import {
  DEVICE_ACTIVITY_RANGES,
  DEVICE_ACTIVITY_MAX_DEVICES,
  DEVICE_ACTIVITY_SEARCH_LIMIT,
  deviceActivityRangeLabels,
  deviceActivityRangeSeconds,
  type DeviceActivityRange,
} from "../../lib/device-activity-contracts";
import { formatByteRate, formatBytes } from "../../lib/byte-scale";
import { Badge, Button, ErrorState } from "../ui/primitives";
import {
  ActivityHeatmap,
  deviceActivityTimestamp,
} from "../visualizations/ActivityHeatmap";
import { ChartSelect } from "../visualizations/ChartFrame";
import { useDeviceActivity } from "./use-device-activity";
import "../visualizations/visualizations.css";
import "./device-activity.css";

export interface DeviceActivityPanelProps {
  active?: boolean;
  demo?: boolean;
  onSelectDevice?: (id: string) => void;
  getDeviceName?: (mac: string, hostname: string) => string;
}
const stateLabels = {
  waiting: "等待积累",
  ok: "已有采样",
  stale: "采样陈旧",
  unavailable: "统计不可用",
} as const;

/** Passive trafficd reads only. Mount in dashboard or devices without shared console changes. */
export function DeviceActivityPanel({
  active = true,
  demo = false,
  onSelectDevice,
  getDeviceName,
}: DeviceActivityPanelProps) {
  const id = useId();
  const [range, setRange] = useState<DeviceActivityRange>("24h");
  const [searchInput, setSearchInput] = useState("");
  const [search, setSearch] = useState("");
  const { data, error, loading, reload } = useDeviceActivity(
    range,
    search,
    active && !demo,
  );
  const devices = useMemo(
    () =>
      data?.devices.map((device) => ({
        ...device,
        name:
          getDeviceName?.(device.id, device.name) || device.name || device.id,
      })) ?? [],
    [data, getDeviceName],
  );
  const hasMeasurements = devices.some((device) => device.coverageSeconds > 0);
  const stale =
    data?.state === "stale" ||
    (error !== undefined && !!data) ||
    devices.some((device) => device.stale);
  const sourceRatesUnavailable =
    data?.state === "stale" ||
    data?.state === "unavailable" ||
    error !== undefined;
  const detail = !active
    ? "等待服务连接；不会读取设备时段记录。"
    : loading && !data
      ? "正在读取后台已有记录；刷新不会发起网络探测。"
      : error !== undefined && !data
        ? "设备时段记录读取失败，可刷新重试；不使用演示数据填充。"
        : !data?.enabled && data
          ? "服务器未启用设备时段记录。"
          : data?.state === "unavailable"
            ? "系统流量统计暂不可用；没有真实新记录可显示。"
            : search && data?.matchedCount === 0
              ? `没有匹配“${search}”的设备，请调整名称、MAC 或地址筛选。`
              : "后台统计服务正在积累时段数据，或当前设备在所选时段内无活动。首个计数器读数只建立基线，不是零流量。";
  const stateLabel = demo
    ? "演示数据"
    : !active
      ? "等待连接"
      : loading && !data
        ? "读取中"
        : error !== undefined
          ? "读取失败"
          : data
            ? stateLabels[data.state]
            : "暂无记录";
  return (
    <section
      className="device-activity"
      aria-labelledby={`${id}-title`}
      aria-busy={loading}
    >
      <header className="device-activity-header">
        <div>
          <h2 id={`${id}-title`}>设备流量观察</h2>
          <p>设备时段分布、接口分组与最新派生速率 · 不解密用户日常流量</p>
        </div>
        <div className="device-activity-badges">
          <Badge>数据源：系统流量统计 (trafficd)</Badge>
          <Badge
            tone={demo || stale || error !== undefined ? "warning" : "neutral"}
          >
            {stateLabel}
          </Badge>
        </div>
      </header>
      {!demo && (
        <form
          className="device-activity-controls"
          onSubmit={(event) => {
            event.preventDefault();
            const next = normalizeDeviceActivitySearch(searchInput);
            if (next === search) reload();
            else setSearch(next);
          }}
        >
          <ChartSelect
            label="时间范围"
            value={range}
            onChange={(value) => setRange(value as DeviceActivityRange)}
          >
            {DEVICE_ACTIVITY_RANGES.map((value) => (
              <option value={value} key={value}>
                {deviceActivityRangeLabels[value]}
              </option>
            ))}
          </ChartSelect>
          <label className="device-activity-search">
            <span>搜索设备</span>
            <input
              type="search"
              aria-label="搜索设备"
              placeholder="名称、MAC 或 IP"
              value={searchInput}
              maxLength={DEVICE_ACTIVITY_SEARCH_LIMIT}
              onChange={(event) => setSearchInput(event.target.value)}
            />
          </label>
          <Button type="submit" size="small" disabled={!active}>
            应用搜索
          </Button>
          {search && (
            <Button
              type="button"
              size="small"
              variant="ghost"
              onClick={() => {
                setSearchInput("");
                setSearch("");
              }}
            >
              清除搜索
            </Button>
          )}
          <Button
            type="button"
            size="small"
            variant="ghost"
            onClick={reload}
            disabled={loading || !active}
          >
            <RefreshCw size={14} aria-hidden="true" />
            刷新数据
          </Button>
          <span className="device-activity-poll-note">
            可见页面每 30 秒读取 · UTC
          </span>
        </form>
      )}
      <p className="device-activity-note">
        {demo
          ? "演示模式：以下为固定、中立的字节样本，不代表路由器设备。"
          : "最近最多 7 天的内存记录，服务重启后清空。新启动服务正在积累历史时段采样；未采样的时段留空，不补零。"}
      </p>
      <p className="device-activity-note">
        RX / TX 沿用厂商计数方向，尚未核验为下载 /
        上传。流量总量是所选范围的已观察字节差值；最新 B/s
        由最近有效的连续采样间隔派生，不是范围平均值。
      </p>
      <p className="device-activity-note">
        没有活跃连接数数据源，不显示连接数或请求次数；刷新只读取后台已有统计，不发起诊断测试。
      </p>
      {!demo && (
        <>
          {error !== undefined && (
            <ErrorState
              message={`${errorMessage(error)}${data ? " · 以下保留此范围和筛选的上次成功记录，并非新采样" : ""}`}
              onRetry={reload}
            />
          )}
          {data?.error && <ErrorState message={data.error} onRetry={reload} />}
          {data && (
            <dl className="device-activity-metadata">
              <div>
                <dt>观察设备 / 筛选匹配</dt>
                <dd>
                  {data.deviceCount} / {data.matchedCount}
                </dd>
              </div>
              <div>
                <dt>聚合分辨率</dt>
                <dd>
                  {data.resolutionSeconds > 0
                    ? `${data.resolutionSeconds} 秒`
                    : "—"}
                </dd>
              </div>
              <div>
                <dt>最近源采样（UTC）</dt>
                <dd>
                  {data.sampledAt
                    ? deviceActivityTimestamp(data.sampledAt)
                    : "尚无采样"}
                </dd>
              </div>
              <div>
                <dt>最早保留记录（UTC）</dt>
                <dd>
                  {data.oldestAt
                    ? deviceActivityTimestamp(data.oldestAt)
                    : "尚无记录"}
                </dd>
              </div>
            </dl>
          )}
          {stale && (
            <p
              className="device-activity-note device-activity-warning"
              role="status"
            >
              源采样或部分设备记录陈旧。保留历史字节，但陈旧设备的最新速率不代表当前流量。
            </p>
          )}
          {data?.truncated && (
            <p className="device-activity-note" role="status">
              匹配 {data.matchedCount} 个设备，表格仅返回按流量排序的前{" "}
              {DEVICE_ACTIVITY_MAX_DEVICES}{" "}
              个；缩小搜索可查看其他设备。接口分组覆盖全部匹配设备，不受此行上限影响。
            </p>
          )}
          {!hasMeasurements && (
            <div className="device-activity-empty" role="status">
              <Activity size={22} aria-hidden="true" />
              <strong>
                {loading && !data ? "正在读取时段活跃数据" : "暂无时段活跃数据"}
              </strong>
              <p>{detail}</p>
            </div>
          )}
        </>
      )}
      <ActivityHeatmap
        demo={demo}
        devices={demo ? undefined : devices}
        resolutionSeconds={data?.resolutionSeconds}
        unavailable={detail}
        onSelectDevice={onSelectDevice}
      />
      {!demo && !!data?.groups.length && (
        <details className="device-activity-details" open>
          <summary>
            接口分组 · 全部匹配设备 <span>{data.groups.length} 组</span>
          </summary>
          <div
            className="device-activity-table-scroll"
            tabIndex={0}
            role="region"
            aria-label="接口分组表，可横向滚动"
          >
            <table>
              <caption>
                系统接口分组 · 所选范围观察总量与最新有效派生速率
              </caption>
              <thead>
                <tr>
                  <th scope="col">接口</th>
                  <th scope="col">设备数</th>
                  <th scope="col">范围 RX 总量</th>
                  <th scope="col">范围 TX 总量</th>
                  <th scope="col">最新 RX 速率</th>
                  <th scope="col">最新 TX 速率</th>
                </tr>
              </thead>
              <tbody>
                {data.groups.map((group) => {
                  const groupCovered =
                    group.coverageSeconds !== undefined
                      ? group.coverageSeconds > 0
                      : devices.some(
                          (device) =>
                            device.interface === group.name &&
                            device.coverageSeconds > 0,
                        );
                  return (
                    <tr key={group.name}>
                      <th scope="row">{group.name || "未报告接口"}</th>
                      <td>{group.deviceCount}</td>
                      <td
                        title={
                          groupCovered ? `${group.rxBytes} bytes` : undefined
                        }
                      >
                        {groupCovered ? formatBytes(group.rxBytes) : "—"}
                      </td>
                      <td
                        title={
                          groupCovered ? `${group.txBytes} bytes` : undefined
                        }
                      >
                        {groupCovered ? formatBytes(group.txBytes) : "—"}
                      </td>
                      <td>
                        {formatByteRate(
                          !groupCovered || sourceRatesUnavailable
                            ? undefined
                            : group.rxBytesPerSecond,
                        )}
                      </td>
                      <td>
                        {formatByteRate(
                          !groupCovered || sourceRatesUnavailable
                            ? undefined
                            : group.txBytesPerSecond,
                        )}
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        </details>
      )}
      {!demo && devices.length > 0 && (
        <details className="device-activity-details" open>
          <summary>
            设备流量排行与明细 <span>{devices.length} 个设备</span>
          </summary>
          <p className="device-activity-note">
            按所选范围已观察总量排序。覆盖不足的字节仅代表已采样时段，不外推为全天流量。关联状态不等于互联网可达。
          </p>
          <div
            className="device-activity-table-scroll"
            tabIndex={0}
            role="region"
            aria-label="设备流量明细表，可横向滚动"
          >
            <table>
              <caption>
                设备流量 · 厂商 RX / TX 方向 · 当前筛选最多 32 行
              </caption>
              <thead>
                <tr>
                  <th scope="col">设备 / MAC</th>
                  <th scope="col">地址 / 接口</th>
                  <th scope="col">关联与新鲜度</th>
                  <th scope="col">范围 RX 总量</th>
                  <th scope="col">范围 TX 总量</th>
                  <th scope="col">最新 RX 速率</th>
                  <th scope="col">最新 TX 速率</th>
                  <th scope="col">范围覆盖 / 秒</th>
                  <th scope="col">最后观察（UTC）</th>
                </tr>
              </thead>
              <tbody>
                {devices.slice(0, DEVICE_ACTIVITY_MAX_DEVICES).map((device) => {
                  const coverage = Math.min(
                    100,
                    (device.coverageSeconds /
                      deviceActivityRangeSeconds[range]) *
                      100,
                  );
                  const old = device.stale || sourceRatesUnavailable;
                  return (
                    <tr key={device.id}>
                      <th scope="row">
                        {onSelectDevice ? (
                          <Button
                            type="button"
                            size="small"
                            variant="ghost"
                            onClick={() => onSelectDevice(device.id)}
                            aria-label={`查看设备 ${device.name}`}
                          >
                            {device.name}
                          </Button>
                        ) : (
                          device.name
                        )}
                        <small>{device.id}</small>
                      </th>
                      <td>
                        {device.addresses.join(", ") || "—"}
                        <small>{device.interface || "—"}</small>
                        {!!device.addressConflicts?.length && (
                          <small className="device-activity-warning">
                            地址归属冲突：{device.addressConflicts.join(", ")}
                          </small>
                        )}
                      </td>
                      <td>
                        {device.associated ? "已关联" : "未见关联"}
                        {old ? " · 陈旧" : " · 已观察"}
                      </td>
                      <td
                        title={
                          device.coverageSeconds > 0
                            ? `${device.rxBytes} bytes`
                            : undefined
                        }
                      >
                        {device.coverageSeconds > 0
                          ? formatBytes(device.rxBytes)
                          : "—"}
                      </td>
                      <td
                        title={
                          device.coverageSeconds > 0
                            ? `${device.txBytes} bytes`
                            : undefined
                        }
                      >
                        {device.coverageSeconds > 0
                          ? formatBytes(device.txBytes)
                          : "—"}
                      </td>
                      <td>
                        {formatByteRate(
                          old || device.coverageSeconds <= 0
                            ? undefined
                            : device.rxBytesPerSecond,
                        )}
                      </td>
                      <td>
                        {formatByteRate(
                          old || device.coverageSeconds <= 0
                            ? undefined
                            : device.txBytesPerSecond,
                        )}
                      </td>
                      <td>
                        {device.coverageSeconds} 秒 · {coverage.toFixed(1)}%
                      </td>
                      <td>{deviceActivityTimestamp(device.lastSeen)}</td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        </details>
      )}
    </section>
  );
}
