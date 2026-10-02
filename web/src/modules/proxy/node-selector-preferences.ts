import { useEffect, useState } from "react";
import type { IPv6Policy, ProxySelectInput } from "../../lib/contracts";

import { validRegion, type NodeRegion } from "./node-selector-regions";

const VIEW_KEY = "be6500panel.proxy.node-view";
const FAVORITES_KEY = "be6500panel.proxy.node-favorites";
export const NODE_PAGE_SIZE = 20;

export type NodePreferences = {
  query: string;
  protocol: string;
  transport: string;
  favoritesOnly: boolean;
  region: NodeRegion;
  page: number;
  selectedId: string;
  acceptedGeneration?: number;
  ipv6: IPv6Policy;
  ports: ProxySelectInput["ports"];
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
    acceptedGeneration:
      typeof saved.acceptedGeneration === "number" &&
      Number.isSafeInteger(saved.acceptedGeneration)
        ? saved.acceptedGeneration
        : undefined,
    ipv6:
      saved.ipv6 === "follow" || saved.ipv6 === "block" ? saved.ipv6 : "direct",
    ports: {
      mixed: validPort(ports.mixed) ? ports.mixed : 2080,
      tproxy: validPort(ports.tproxy) ? ports.tproxy : 7893,
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
