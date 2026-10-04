import { useEffect, useState } from "react";
import { api, runRequest } from "../../lib/api";
import type { IPv6Policy, ProxySelectInput } from "../../lib/contracts";
import type { RuntimeController } from "../runtime/use-runtime";
import {
  TPROXY_RESERVED_PORT,
  validPort,
  validRoutedTUN,
} from "./node-selector-preferences";

type NodeConfig = Pick<ProxySelectInput, "ipv6" | "ports" | "routedTUN"> & {
  // Old TPROXY is read only for a safe transition; new saves use routed TUN.
  datapath: "tproxy" | "routed-tun";
};
const unsupported =
  "当前配置的监听或 IPv6 规则不受节点选择器支持。请在运行管理中检查接管后端与配置后再切换节点，避免覆盖已有设置。";
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
  const tun = inbounds.find(
    (inbound) => inbound.tag === "tun-in" && inbound.type === "tun",
  );
  const dns = inbounds.find(
    (inbound) => inbound.tag === "dns-in" && inbound.type === "direct",
  );
  const listenerKeys = ["type", "tag", "listen", "listen_port"];
  if (
    !mixed ||
    !dns ||
    (!tproxy && !tun) ||
    (tproxy && tun) ||
    Object.keys(mixed).some((key) => !listenerKeys.includes(key)) ||
    Object.keys(dns).some((key) => !listenerKeys.includes(key)) ||
    !validPort(mixed.listen_port) ||
    !validPort(dns.listen_port)
  )
    throw new Error(unsupported);
  if (
    tproxy &&
    (!validPort(tproxy.listen_port) ||
      Object.keys(tproxy).some(
        (key) => ![...listenerKeys, "udp_timeout", "udp_nat_max"].includes(key),
      ) ||
      ("udp_timeout" in tproxy && tproxy.udp_timeout !== "2m") ||
      ("udp_nat_max" in tproxy && tproxy.udp_nat_max !== 1024))
  )
    throw new Error(unsupported);
  const tunKeys = [
    "type",
    "tag",
    "interface_name",
    "address",
    "mtu",
    "dns_mode",
    "auto_route",
    "auto_redirect",
    "stack",
    "udp_timeout",
    "udp_nat_max",
  ];
  const routedTUN = tun && {
    interfaceName: tun.interface_name,
    address:
      Array.isArray(tun.address) && tun.address.length === 1
        ? tun.address[0]
        : undefined,
  };
  if (
    tun &&
    (Object.keys(tun).length !== tunKeys.length ||
      Object.keys(tun).some((key) => !tunKeys.includes(key)) ||
      tun.mtu !== 1500 ||
      tun.dns_mode !== "disabled" ||
      tun.auto_route !== false ||
      tun.auto_redirect !== false ||
      tun.stack !== "system" ||
      tun.udp_timeout !== "2m" ||
      tun.udp_nat_max !== 1024 ||
      !validRoutedTUN(routedTUN))
  )
    throw new Error(unsupported);
  const ports = {
    mixed: mixed.listen_port,
    tproxy: tproxy ? (tproxy.listen_port as number) : TPROXY_RESERVED_PORT,
    dns: dns.listen_port,
  };
  if (
    ports.mixed === ports.dns ||
    (tproxy && new Set(Object.values(ports)).size !== 3)
  )
    throw new Error(unsupported);
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
    (tproxy && tproxy.listen !== (ipv6 === "follow" ? "::" : "127.0.0.1")) ||
    dns.listen !== (ipv6 === "follow" ? "::" : "192.168.31.1") ||
    (tun && ipv6 !== "direct")
  )
    throw new Error(unsupported);
  return {
    ipv6,
    ports,
    datapath: tun ? "routed-tun" : "tproxy",
    ...(validRoutedTUN(routedTUN) ? { routedTUN } : {}),
  };
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
  // Never expose prior-generation inputs while the actual accepted config loads.
  const current =
    configured === true && state.generation === generation ? state : {};
  return { ...current, ready, reload: () => setRetry((value) => value + 1) };
}
