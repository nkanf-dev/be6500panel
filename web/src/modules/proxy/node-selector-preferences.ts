import { useEffect, useState } from "react";
import type { ProxySelectInput, RoutedTUNConfig } from "../../lib/contracts";

import { validRegion, type NodeRegion } from "./node-selector-regions";

const VIEW_KEY = "be6500panel.proxy.node-view";
const FAVORITES_KEY = "be6500panel.proxy.node-favorites";
export const NODE_PAGE_SIZE = 20;
export const TPROXY_RESERVED_PORT = 7893;
export const DEFAULT_ROUTED_TUN: RoutedTUNConfig = {
  interfaceName: "b6p-tun",
  address: "172.31.255.253/30",
};

export type NodePreferences = {
  query: string;
  protocol: string;
  transport: string;
  favoritesOnly: boolean;
  region: NodeRegion;
  page: number;
  selectedId: string;
  acceptedGeneration?: number;
  ports: Pick<ProxySelectInput["ports"], "mixed" | "dns">;
};

function readStorage(
  kind: "localStorage" | "sessionStorage",
  key: string,
): unknown {
  try {
    const value = window[kind].getItem(key);
    return value ? JSON.parse(value) : undefined;
  } catch {
    return undefined;
  }
}
function writeStorage(
  kind: "localStorage" | "sessionStorage",
  key: string,
  value: unknown,
) {
  try {
    window[kind].setItem(key, JSON.stringify(value));
  } catch {
    // Browsing still works when storage is blocked or full.
  }
}
function record(value: unknown): Record<string, unknown> {
  return value && typeof value === "object"
    ? (value as Record<string, unknown>)
    : {};
}
function text(value: unknown) {
  return typeof value === "string" ? value.slice(0, 512) : "";
}
export function validPort(value: unknown): value is number {
  return (
    typeof value === "number" &&
    Number.isInteger(value) &&
    value >= 1 &&
    value <= 65535
  );
}
/** Same owned interface and literal RFC1918 first-host /30 contract as Go. */
export function validRoutedTUN(value: unknown): value is RoutedTUNConfig {
  if (!value || typeof value !== "object" || Array.isArray(value)) return false;
  const fields = value as Record<string, unknown>;
  if (
    Object.keys(fields).length !== 2 ||
    typeof fields.interfaceName !== "string" ||
    fields.interfaceName.match(/^b6p-[A-Za-z0-9_][A-Za-z0-9_-]{0,10}$/)?.[0] !==
      fields.interfaceName ||
    typeof fields.address !== "string"
  )
    return false;
  const match = fields.address.match(/^(\d+)\.(\d+)\.(\d+)\.(\d+)\/30$/);
  if (!match) return false;
  const octets = match.slice(1).map(Number);
  if (
    octets.some((octet) => !Number.isInteger(octet) || octet > 255) ||
    `${octets.join(".")}/30` !== fields.address ||
    octets[3] % 4 !== 1
  )
    return false;
  return (
    octets[0] === 10 ||
    (octets[0] === 172 && octets[1] >= 16 && octets[1] <= 31) ||
    (octets[0] === 192 && octets[1] === 168)
  );
}
function readPreferences(): NodePreferences {
  const saved = record(readStorage("sessionStorage", VIEW_KEY));
  const ports = record(saved.ports);
  return {
    query: text(saved.query),
    protocol: text(saved.protocol),
    transport: text(saved.transport),
    favoritesOnly: saved.favoritesOnly === true,
    region: validRegion(saved.region) ? saved.region : "",
    page:
      typeof saved.page === "number" &&
      Number.isSafeInteger(saved.page) &&
      saved.page >= 1
        ? saved.page
        : 1,
    selectedId: text(saved.selectedId),
    // Re-read older backend-specific drafts instead of trusting their generation.
    acceptedGeneration:
      !("datapath" in saved) &&
      !("routedTUN" in saved) &&
      !("ipv6" in saved) &&
      !("tproxy" in ports) &&
      validPort(ports.mixed) &&
      validPort(ports.dns) &&
      ports.mixed !== ports.dns &&
      typeof saved.acceptedGeneration === "number" &&
      Number.isSafeInteger(saved.acceptedGeneration)
        ? saved.acceptedGeneration
        : undefined,
    ports: {
      mixed: validPort(ports.mixed) ? ports.mixed : 2080,
      dns: validPort(ports.dns) ? ports.dns : 6450,
    },
  };
}

/** Store only view/config preferences and IDs, never imported nodes or credentials. */
export function useNodePreferences() {
  const [preferences, setPreferences] = useState(readPreferences);
  useEffect(() => {
    writeStorage("sessionStorage", VIEW_KEY, preferences);
  }, [preferences]);
  return [preferences, setPreferences] as const;
}

export function useNodeFavorites(nodeIds: readonly string[] | undefined) {
  const [favorites, setFavorites] = useState<string[]>(() => {
    const saved = readStorage("localStorage", FAVORITES_KEY);
    return Array.isArray(saved)
      ? [
          ...new Set(
            saved.filter(
              (id): id is string =>
                typeof id === "string" && id.length > 0 && id.length <= 512,
            ),
          ),
        ]
      : [];
  });
  useEffect(() => {
    if (!nodeIds) return;
    const available = new Set(nodeIds);
    setFavorites((previous) => {
      const next = previous.filter((id) => available.has(id));
      return next.length === previous.length ? previous : next;
    });
  }, [nodeIds]);
  useEffect(() => {
    writeStorage("localStorage", FAVORITES_KEY, favorites);
  }, [favorites]);
  function toggleFavorite(id: string) {
    setFavorites((previous) =>
      previous.includes(id)
        ? previous.filter((favorite) => favorite !== id)
        : [...previous, id],
    );
  }
  return [favorites, toggleFavorite] as const;
}
