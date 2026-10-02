import { useEffect, useState } from "react";
import {
  Badge,
  Button,
  ErrorState,
  Field,
  Panel,
  PanelHeader,
} from "../../components/ui/primitives";
import { api, errorMessage, runRequest } from "../../lib/api";
import { useResource } from "../../lib/use-resource";
import type { IPv6Policy } from "../../lib/contracts";
import type { RuntimeController } from "../runtime/use-runtime";

export function CapturePanel({ runtime }: { runtime: RuntimeController }) {
  const observation = useResource(api.proxyCapture);
  const [clientIPv4, setIPv4] = useState("");
  const [clientIPv6, setIPv6] = useState("");
  const [ipv6, setPolicy] = useState<IPv6Policy>("direct");
  const [confirming, setConfirming] = useState(false);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<unknown>();
  const capture =
    observation.error === undefined ? observation.data : undefined;
  useEffect(() => {
    observation.reload();
  }, [
    runtime.status?.state,
    runtime.status?.restarts,
    runtime.status?.pid,
    runtime.status?.generation,
    observation.reload,
  ]);
  function stage(event: React.FormEvent) {
    event.preventDefault();
    setConfirming(true);
  }
  async function apply() {
    setPending(true);
    setError(undefined);
    try {
      await runRequest(
        api.proxyCaptureApply({
          clientIPv4,
          ...(clientIPv6 ? { clientIPv6 } : {}),
          ipv6,
        }),
      );
      setConfirming(false);
    } catch (cause) {
      setError(cause);
    } finally {
      observation.reload();
      setPending(false);
    }
  }
  async function stop() {
    const stopped = await runtime.run(
      () => api.runtimeStop("sing-box"),
      "sing-box 已停止，接管规则已清理",
    );
    if (stopped) {
      observation.reload();
    }
  }
  return (
    <Panel>
      <PanelHeader
        title="单客户端接管"
        subtitle="明确指定地址，不自动接管整个 LAN"
        action={
          <Badge tone={capture?.active ? "success" : "neutral"}>
            {capture?.active ? "接管中" : "未接管"}
          </Badge>
        }
      />
      {capture?.active && (
        <dl className="key-values">
          <div>
            <dt>客户端 IPv4</dt>
            <dd className="mono">{capture.clientIPv4}</dd>
          </div>
          {capture.clientIPv6 && (
            <div>
              <dt>客户端 IPv6</dt>
              <dd className="mono">{capture.clientIPv6}</dd>
            </div>
          )}
          <div>
            <dt>所属规则</dt>
            <dd>{capture.commands} 条命令</dd>
          </div>
        </dl>
      )}
      <form className="config-form" onSubmit={stage}>
        <div className="form-grid">
          <Field label="客户端 IPv4">
            <input
              disabled={pending || runtime.pending}
              required
              placeholder="192.168.31.x"
              autoComplete="off"
              value={clientIPv4}
              onChange={(event) => {
                setIPv4(event.target.value);
                setConfirming(false);
              }}
            />
          </Field>
          <Field
            label="客户端 IPv6"
            hint={
              ipv6 === "direct"
                ? "IPv6 直连时可选"
                : "跟随代理或阻断时必须指定单客户端 IPv6"
            }
          >
            <input
              required={ipv6 !== "direct"}
              disabled={pending || runtime.pending}
              autoComplete="off"
              value={clientIPv6}
              onChange={(event) => {
                setIPv6(event.target.value);
                setConfirming(false);
              }}
            />
          </Field>
          <Field label="接管 IPv6 策略">
            <select
              className="select-trigger"
              disabled={pending || runtime.pending}
              value={ipv6}
              onChange={(event) => {
                setPolicy(event.target.value as IPv6Policy);
                setConfirming(false);
              }}
            >
              <option value="follow">跟随代理</option>
              <option value="direct">直连</option>
              <option value="block">阻断</option>
            </select>
          </Field>
        </div>
        {observation.error !== undefined && (
          <ErrorState
            message={errorMessage(observation.error)}
            onRetry={observation.reload}
          />
        )}
        {error !== undefined && <ErrorState message={errorMessage(error)} />}
        {confirming && (
          <div role="alertdialog" aria-label="确认客户端接管">
            <p>
              将对{" "}
              <strong className="mono">
                {clientIPv4}
                {clientIPv6 ? ` / ${clientIPv6}` : ""}
              </strong>{" "}
              应用透明代理和 DNS 接管，IPv6 策略为 {ipv6}。
            </p>
            <Button
              type="button"
              disabled={pending}
              onClick={() => void apply()}
            >
              确认接管客户端
            </Button>{" "}
            <Button
              type="button"
              disabled={pending}
              onClick={() => setConfirming(false)}
            >
              取消
            </Button>
          </div>
        )}
        <div className="form-actions">
          <span className="text-muted text-xs">
            {runtime.status?.state === "running"
              ? "核心运行中"
              : "先启动 sing-box"}
          </span>
          <div>
            <Button
              type="button"
              disabled={
                !runtime.enabled ||
                runtime.pending ||
                pending ||
                !capture?.active
              }
              onClick={() => void stop()}
            >
              停止核心并撤销接管
            </Button>{" "}
            <Button
              type="submit"
              variant="primary"
              disabled={
                !runtime.enabled ||
                runtime.pending ||
                pending ||
                runtime.status?.state !== "running"
              }
            >
              审阅客户端接管
            </Button>
          </div>
        </div>
      </form>
    </Panel>
  );
}
