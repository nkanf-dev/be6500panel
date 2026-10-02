import { strings } from "../../locales/strings";
import { useEffect, useState } from "react";
import {
  Badge,
  Button,
  EmptyState,
  ErrorState,
  Field,
  Loading,
  Panel,
  PanelHeader,
} from "../../components/ui/primitives";
import { api, errorMessage, runRequest } from "../../lib/api";
import { useResource } from "../../lib/use-resource";
import type { IPv6Policy, ProxyCapture } from "../../lib/contracts";
import type { RuntimeController } from "../runtime/use-runtime";
import {
  captureDevices,
  captureEligibilityAvailable,
  captureMAC,
} from "./capture-devices";

function savedMACs(
  capture: ProxyCapture,
  devices: ReturnType<typeof captureDevices>,
) {
  if (capture.clients?.length)
    return [
      ...new Set(
        capture.clients.map((client) => captureMAC(client.mac)).filter(Boolean),
      ),
    ];
  if (!(capture.desired ?? capture.active)) return [];
  // An old single-IP status stays readable; migrate only through an explicit Apply.
  const legacy = devices.find((device) => device.ip === capture.clientIPv4);
  return legacy ? [legacy.mac] : [];
}

export function CapturePanel({
  runtime,
  onPending,
}: {
  runtime: RuntimeController;
  onPending?: (pending: boolean) => void;
}) {
  const observation = useResource(api.proxyCapture);
  const router = useResource(api.router);
  const [selected, setSelected] = useState<string[]>([]);
  const [initialized, setInitialized] = useState(false);
  const [clientIPv6, setIPv6] = useState("");
  const [ipv6, setPolicy] = useState<IPv6Policy>("direct");
  const [confirming, setConfirming] = useState<"apply" | "disable">();
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<unknown>();
  const capture =
    observation.error === undefined ? observation.data : undefined;
  const snapshot = router.error === undefined ? router.data : undefined;
  const devices = snapshot ? captureDevices(snapshot) : [];
  const desired = capture?.desired ?? capture?.active ?? false;
  const retained = (capture?.clients?.length ?? 0) > 0;
  const busy = pending || runtime.pending;
  const saved = capture ? savedMACs(capture, devices) : [];
  const choicesByMAC = new Map(
    devices.map((device) => [device.mac, { ...device, unresolved: false }]),
  );
  // Keep saved and draft identities removable when current observations disappear.
  // Never use an old IP as a new checkbox candidate or an Apply input.
  for (const client of [
    ...(capture?.clients ?? []),
    ...selected.map((mac) => ({ mac, hostname: "" })),
  ]) {
    const mac = captureMAC(client.mac);
    if (mac && !choicesByMAC.has(mac))
      choicesByMAC.set(mac, {
        mac,
        ip: "",
        hostname: client.hostname,
        online: false,
        expiresAt: null,
        unresolved: true,
      });
  }
  const choices = [...choicesByMAC.values()];
  const arpIncomplete = snapshot?.errors.some(
    (item) => item.module === "devices.arp",
  );
  const canApply =
    runtime.enabled &&
    runtime.status?.state === "running" &&
    !busy &&
    initialized &&
    !observation.loading &&
    !router.loading &&
    !!capture &&
    !capture.cleanupPending &&
    !!snapshot &&
    selected.length > 0 &&
    (ipv6 === "direct" || (selected.length === 1 && clientIPv6.trim() !== ""));
  const stateLabel = !capture
    ? observation.loading
      ? "读取中"
      : "状态未知"
    : capture.cleanupPending
      ? strings.states.cleaningPending
      : capture.active && capture.state === "partial"
        ? strings.states.partialActive
        : capture.error
          ? "接管异常"
          : capture.active
            ? strings.states.active
            : desired
              ? "已暂停"
              : retained
                ? "已停用（选择保留）"
                : strings.states.notCaptured;

  useEffect(() => {
    setConfirming(undefined);
    observation.reload();
  }, [
    runtime.status?.state,
    runtime.status?.restarts,
    runtime.status?.pid,
    runtime.status?.generation,
    observation.reload,
  ]);
  useEffect(() => {
    if (
      initialized ||
      !capture ||
      !snapshot ||
      observation.loading ||
      router.loading
    )
      return;
    const currentDevices = captureDevices(snapshot);
    const restored = savedMACs(capture, currentDevices);
    const currentMatches = currentDevices.filter(
      (device) => device.ip === snapshot.currentClientIP,
    );
    const current = currentMatches.length === 1 ? currentMatches[0] : undefined;
    setSelected(
      capture.clients?.length || (capture.desired ?? capture.active)
        ? restored
        : current
          ? [current.mac]
          : [],
    );
    setPolicy(capture.ipv6 ?? "direct");
    setIPv6(capture.clientIPv6 ?? "");
    setInitialized(true);
  }, [initialized, capture, snapshot, observation.loading, router.loading]);

  function restoreSelection() {
    setSelected(saved);
    setPolicy(capture?.ipv6 ?? "direct");
    setIPv6(capture?.clientIPv6 ?? "");
    setConfirming(undefined);
  }
  function stage(event: React.FormEvent) {
    event.preventDefault();
    if (canApply) setConfirming("apply");
  }
  async function apply() {
    if (!canApply) return;
    setPending(true);
    onPending?.(true);
    setError(undefined);
    try {
      await runRequest(
        api.proxyCaptureApply({
          devices: selected.map((mac) => ({ mac })),
          ipv6,
          ...(ipv6 !== "direct" ? { clientIPv6: clientIPv6.trim() } : {}),
        }),
      );
      setConfirming(undefined);
    } catch (cause) {
      setError(cause);
      setConfirming(undefined);
    } finally {
      observation.reload();
      router.reload();
      setPending(false);
      onPending?.(false);
    }
  }
  async function disable() {
    setPending(true);
    onPending?.(true);
    setError(undefined);
    try {
      await runRequest(api.proxyCaptureDisable());
      setSelected([]);
      setIPv6("");
      setPolicy("direct");
      setConfirming(undefined);
    } catch (cause) {
      setError(cause);
      setConfirming(undefined);
    } finally {
      observation.reload();
      setPending(false);
      onPending?.(false);
    }
  }
  async function stop() {
    setError(undefined);
    setConfirming(undefined);
    const stopped = await runtime.run(
      () => api.runtimeStop("sing-box"),
      "sing-box 已停止；实时接管已撤回，已保存的设备选择保留",
    );
    observation.reload();
    if (!stopped)
      setError(new Error("停止核心未完成；请检查运行状态与接管诊断。"));
  }

  return (
    <Panel>
      <PanelHeader
        title={strings.proxy.capture.title}
        subtitle={strings.proxy.capture.subtitle}
        action={
          <Badge
            tone={
              capture?.cleanupPending ||
              (capture?.error && capture.state !== "partial")
                ? "danger"
                : capture?.active && capture.state === "partial"
                  ? "warning"
                  : capture?.active
                    ? "success"
                    : desired
                      ? "warning"
                      : "neutral"
            }
          >
            {stateLabel}
          </Badge>
        }
      />
      <div
        className="config-form"
        role="region"
        aria-label={strings.proxy.capture.savedTitle}
      >
        <h3>{strings.proxy.capture.savedTitle}</h3>
        <p className="text-muted text-xs">
          {desired
            ? "核心停止或重启时保留此选择；只根据当前设备地址恢复，不使用过期地址。"
            : retained
              ? "设备选择保留，但自动恢复已停用；只有明确应用后才启用接管。"
              : capture
                ? "未启用已保存的接管选择。勾选设备不会自动应用。"
                : "正在读取已保存的选择；无法确认接管状态。"}
        </p>
        {capture?.clients?.length ? (
          <ul>
            {capture.clients.map((client) => (
              <li key={client.mac}>
                {client.hostname || "未命名设备"} ·{" "}
                <span className="mono">{client.mac}</span> ·{" "}
                <span className="mono">
                  {client.ip || strings.proxy.capture.waitingAddress}
                </span>
                {capture.active &&
                !capture.cleanupPending &&
                (!capture.error || capture.state === "partial") &&
                client.ip
                  ? " · 实时规则已生效"
                  : !client.ip
                    ? " · 等待当前地址"
                    : " · 实时接管未生效"}
              </li>
            ))}
          </ul>
        ) : desired && capture?.clientIPv4 ? (
          <p>
            兼容旧版地址选择：<span className="mono">{capture.clientIPv4}</span>
            。重新应用将按勾选的 MAC 保存。
          </p>
        ) : desired ? (
          <p>已启用，但设备选择暂不可读取。</p>
        ) : null}
        {capture?.clientIPv6 && (
          <p>
            已保存客户端 IPv6：
            <span className="mono">{capture.clientIPv6}</span>
          </p>
        )}
        <p>
          实时接管：
          {capture?.active &&
          !capture.cleanupPending &&
          capture.state === "partial"
            ? "仅已解析设备的接管规则已生效；其余设备等待当前地址（不代表互联网连通性已验证）"
            : capture?.active && !capture.cleanupPending && !capture.error
              ? "接管规则已生效（不代表互联网连通性已验证）"
              : capture
                ? "未确认生效"
                : "状态未知"}
        </p>
        {capture && (
          <details>
            <summary>高级诊断</summary>
            <dl className="key-values">
              {capture.state && (
                <div>
                  <dt>接管状态</dt>
                  <dd className="mono">{capture.state}</dd>
                </div>
              )}
              <div>
                <dt>实时规则命令数</dt>
                <dd>{capture.commands}</dd>
              </div>
            </dl>
          </details>
        )}
        {capture?.cleanupPending && (
          <ErrorState message="接管规则清理尚未完成；请重试禁用接管并检查诊断，不能视为已撤回。" />
        )}
        {capture?.error && (
          <ErrorState
            message={
              capture.error === "capture_devices_pending"
                ? "部分已选设备尚未解析到当前 LAN 地址；未解析设备不会接管，也不会使用过期 IP。 · capture_devices_pending"
                : capture.error
            }
          />
        )}
        {observation.error !== undefined && (
          <ErrorState
            message={errorMessage(observation.error)}
            onRetry={observation.reload}
          />
        )}
      </div>
      <form className="config-form" onSubmit={stage}>
        <div className="form-actions">
          <h3>{strings.proxy.capture.selectTitle}</h3>
          <div>
            <Button
              type="button"
              size="small"
              disabled={busy || router.loading}
              onClick={() => {
                setConfirming(undefined);
                router.reload();
                observation.reload();
              }}
            >
              刷新设备与接管状态
            </Button>{" "}
            <Button
              type="button"
              size="small"
              disabled={busy || !capture || observation.loading}
              onClick={restoreSelection}
            >
              {strings.actions.restoreSaved}
            </Button>
          </div>
        </div>
        <p className="text-muted text-xs">
          只列出可接管的 LAN 设备，按 MAC 识别。在线状态来自 ARP
          观察；离线或未解析的设备等待当前地址，不扩大到整个 LAN。
        </p>
        {router.loading && <Loading label="正在读取当前设备" />}
        {router.error !== undefined && (
          <ErrorState
            message={errorMessage(router.error)}
            onRetry={router.reload}
          />
        )}
        {snapshot?.errors
          .filter(
            (item) =>
              item.module.startsWith("devices") ||
              item.module.startsWith("routes"),
          )
          .map((item, index) => (
            <ErrorState
              key={`${item.module}-${index}`}
              message={`${item.module} · ${item.code} · ${item.message}`}
            />
          ))}
        {snapshot && !captureEligibilityAvailable(snapshot) && (
          <ErrorState message="无法确认 LAN 设备范围：未读取到 br-lan 网段或设备接管资格，不提供新的设备选择。" />
        )}
        {snapshot && choices.length === 0 && !router.loading && (
          <EmptyState
            title="未观察到可接管的 LAN 设备"
            detail="请让设备连接 LAN 后刷新。不会默认接管所有设备。"
          />
        )}
        {choices.length > 0 && (
          <div className="table-scroll">
            <table className="data-table" aria-label="接管设备选择">
              <thead>
                <tr>
                  <th>{strings.proxy.nodes.columns.select}</th>
                  <th>设备名称</th>
                  <th>IP 地址</th>
                  <th>MAC 地址</th>
                  <th>在线状态</th>
                </tr>
              </thead>
              <tbody>
                {choices.map((device) => (
                  <tr key={device.mac}>
                    <td>
                      <input
                        type="checkbox"
                        aria-label={`选择设备 ${device.hostname || "未命名设备"} ${device.mac}`}
                        checked={selected.includes(device.mac)}
                        disabled={
                          busy ||
                          !initialized ||
                          router.loading ||
                          router.error !== undefined
                        }
                        onChange={(event) => {
                          setSelected((value) =>
                            event.target.checked
                              ? [...value, device.mac]
                              : value.filter((mac) => mac !== device.mac),
                          );
                          setConfirming(undefined);
                        }}
                      />
                    </td>
                    <td>
                      {device.hostname || "未命名设备"}
                      {device.ip && device.ip === snapshot?.currentClientIP && (
                        <>
                          {" "}
                          <Badge tone="primary">
                            {strings.states.currentDevice}
                          </Badge>
                        </>
                      )}
                    </td>
                    <td className="mono">
                      {device.ip || strings.states.waitingCurrentAddress}
                    </td>
                    <td className="mono">{device.mac}</td>
                    <td>
                      <Badge tone={device.online ? "success" : "neutral"}>
                        {device.unresolved
                          ? "未在当前 LAN 观察到"
                          : device.online
                            ? "在线"
                            : arpIncomplete
                              ? "在线观察不完整"
                              : "离线 / 未见 ARP"}
                      </Badge>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
        <p className="text-muted text-xs">
          本次待应用：{selected.length} 台设备。
          {snapshot && !snapshot.currentClientIP
            ? "服务未提供当前终端地址，请手动勾选。"
            : ""}
        </p>
        <Field
          label="接管 IPv6 策略"
          hint="IPv6 直连不会被透明代理接管；跟随或阻断仅支持一台设备的明确 IPv6 地址。"
        >
          <select
            aria-label="接管 IPv6 策略"
            className="select-trigger"
            disabled={busy}
            value={ipv6}
            onChange={(event) => {
              setPolicy(event.target.value as IPv6Policy);
              setConfirming(undefined);
            }}
          >
            <option value="direct">直连（默认）</option>
            <option value="follow">跟随代理</option>
            <option value="block">阻断</option>
          </select>
        </Field>
        {ipv6 !== "direct" && (
          <details open>
            <summary>高级：单设备 IPv6 地址</summary>
            <Field
              label="客户端 IPv6"
              hint="仅接管此明确地址；不会自动覆盖设备的其他 IPv6 地址。"
            >
              <input
                aria-label="客户端 IPv6"
                required
                disabled={busy}
                autoComplete="off"
                value={clientIPv6}
                onChange={(event) => {
                  setIPv6(event.target.value);
                  setConfirming(undefined);
                }}
              />
            </Field>
            {selected.length !== 1 && (
              <p role="alert">
                跟随代理或阻断 IPv6 需要只选择一台设备，并提供该设备的 IPv6
                地址。
              </p>
            )}
          </details>
        )}
        {error !== undefined && <ErrorState message={errorMessage(error)} />}
        {runtime.error !== undefined && (
          <ErrorState message={errorMessage(runtime.error)} />
        )}
        {runtime.result && <p role="status">{runtime.result}</p>}
        {confirming === "apply" && (
          <div role="alertdialog" aria-label={strings.dialogs.captureTitle}>
            <p>
              {strings.dialogs.captureBody(
                selected
                  .map(
                    (mac) =>
                      choices.find((item) => item.mac === mac)?.hostname || mac,
                  )
                  .join("、"),
              )}
            </p>
            <p className="text-muted text-xs">
              将替换已保存的选择，仅接管以下 {selected.length} 台设备。IPv6
              策略为 {ipv6}。离线设备等待当前地址。
            </p>
            <ul>
              {selected.map((mac) => {
                const device = choices.find((item) => item.mac === mac);
                return (
                  <li key={mac}>
                    {device?.hostname || "未命名设备"} ·{" "}
                    <span className="mono">{mac}</span> ·{" "}
                    <span className="mono">
                      {device?.ip || strings.states.waitingCurrentAddress}
                    </span>
                  </li>
                );
              })}
            </ul>
            {ipv6 !== "direct" && (
              <p>
                仅指定的客户端 IPv6：<span className="mono">{clientIPv6}</span>
              </p>
            )}
            <Button
              type="button"
              disabled={!canApply}
              onClick={() => void apply()}
            >
              {strings.actions.confirmCapture}
            </Button>{" "}
            <Button
              type="button"
              disabled={busy}
              onClick={() => setConfirming(undefined)}
            >
              {strings.actions.cancel}
            </Button>
          </div>
        )}
        {confirming === "disable" && (
          <div
            role="alertdialog"
            aria-label={strings.dialogs.disableCaptureTitle}
          >
            <p>{strings.dialogs.disableCaptureBody}</p>
            <Button
              type="button"
              disabled={busy}
              onClick={() => void disable()}
            >
              确认禁用接管
            </Button>{" "}
            <Button
              type="button"
              disabled={busy}
              onClick={() => setConfirming(undefined)}
            >
              {strings.actions.cancel}
            </Button>
          </div>
        )}
        <div className="form-actions">
          <span className="text-muted text-xs">
            {runtime.status?.state === "running"
              ? "核心运行中；停止核心保留选择，禁用接管清除选择。"
              : "核心未运行；已保存选择保留，启动后可恢复。应用新的选择需先启动 sing-box。"}
          </span>
          <div>
            <Button
              type="button"
              disabled={
                busy ||
                !(
                  desired ||
                  retained ||
                  capture?.active ||
                  capture?.cleanupPending ||
                  observation.error !== undefined
                )
              }
              onClick={() => setConfirming("disable")}
            >
              {strings.actions.disableCapture}
            </Button>{" "}
            <Button
              type="button"
              disabled={
                !runtime.enabled || busy || runtime.status?.state !== "running"
              }
              onClick={() => void stop()}
            >
              {strings.actions.stopCoreKeepSelection}
            </Button>{" "}
            <Button type="submit" variant="primary" disabled={!canApply}>
              {strings.actions.reviewCapture}
            </Button>
          </div>
        </div>
      </form>
    </Panel>
  );
}
