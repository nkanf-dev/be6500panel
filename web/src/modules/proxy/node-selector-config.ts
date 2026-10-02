import { useEffect, useState } from "react";
import { api, runRequest } from "../../lib/api";
import type { IPv6Policy, ProxySelectInput } from "../../lib/contracts";
import type { RuntimeController } from "../runtime/use-runtime";
import { validPort } from "./node-selector-preferences";

type NodeConfig = Pick<ProxySelectInput, "ipv6" | "ports">;
const unsupported =
  "当前配置的监听或 IPv6 规则不受节点选择器支持。请在运行管理中检查配置后再切换节点，避免覆盖已有设置。";
function object(value: unknown): Record<string, unknown> {
  if (!value || typeof value !== "object" || Array.isArray(value))
    throw new Error(unsupported);
  return value as Record<string, unknown>;
}

/** Read only supported compiler inputs. Do not retain private native JSON. */
export function nodeConfigInputs(config: string): NodeConfig {
  let raw: unknown;
  try {
    raw = JSON.parse(config);
  } catch {
    throw new Error(unsupported);
  }
  const native = object(raw);
  if (!Array.isArray(native.inbounds) || native.inbounds.length !== 3)
    throw new Error(unsupported);
  const inbounds = native.inbounds.map(object);
  const mixed = inbounds.find(
    (inbound) => inbound.tag === "mixed-in" && inbound.type === "mixed",
  );
  const tproxy = inbounds.find(
    (inbound) => inbound.tag === "tproxy-in" && inbound.type === "tproxy",
  );
  const dns = inbounds.find(
    (inbound) => inbound.tag === "dns-in" && inbound.type === "direct",
  );
  if (
    !mixed ||
    !tproxy ||
    !dns ||
    !validPort(mixed.listen_port) ||
    !validPort(tproxy.listen_port) ||
    !validPort(dns.listen_port)
  )
    throw new Error(unsupported);
  const ports = {
    mixed: mixed.listen_port,
    tproxy: tproxy.listen_port,
    dns: dns.listen_port,
  };
  if (new Set(Object.values(ports)).size !== 3) throw new Error(unsupported);
  const route = object(native.route);
  if (!Array.isArray(route.rules)) throw new Error(unsupported);
  const ipv6Rules = route.rules
    .map(object)
    .filter((rule) => rule.ip_version === 6);
  let ipv6: IPv6Policy = "follow";
  if (ipv6Rules.length) {
    const rule = ipv6Rules[0];
    if (ipv6Rules.length !== 1 || Object.keys(rule).length !== 2)
      throw new Error(unsupported);
    if (rule.outbound === "direct") ipv6 = "direct";
    else if (rule.action === "reject") ipv6 = "block";
    else throw new Error(unsupported);
  }
  if (
    mixed.listen !== "192.168.31.1" ||
    tproxy.listen !== (ipv6 === "follow" ? "::" : "127.0.0.1") ||
    dns.listen !== (ipv6 === "follow" ? "::" : "192.168.31.1")
  )
    throw new Error(unsupported);
  return { ipv6, ports };
}

export function useAcceptedNodeConfig(
  runtime: RuntimeController,
  importing: boolean,
) {
  const generation = runtime.status?.generation;
  const configured = runtime.status?.configured;
  const [retry, setRetry] = useState(0);
  const [state, setState] = useState<{
    generation?: number;
    inputs?: NodeConfig;
    error?: unknown;
  }>({});
  useEffect(() => {
    if (
      !runtime.enabled ||
      configured !== true ||
      generation === undefined ||
      runtime.pending ||
      importing
    )
      return;
    const controller = new AbortController();
    let active = true;
    void runRequest(api.runtimeConfig("sing-box"), controller.signal)
      .then((response) => {
        if (!active) return;
        if (response.generation !== generation)
          throw new Error("节点配置已变化，请重新读取当前设置。");
        setState({ generation, inputs: nodeConfigInputs(response.config) });
      })
      .catch((error: unknown) => {
        if (active) setState({ generation, error });
      });
    return () => {
      active = false;
      controller.abort();
    };
  }, [
    runtime.enabled,
    runtime.pending,
    importing,
    configured,
    generation,
    retry,
  ]);
  const ready =
    configured === false ||
    (configured === true && state.generation === generation && !!state.inputs);
  return { ...state, ready, reload: () => setRetry((value) => value + 1) };
}
