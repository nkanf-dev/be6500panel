import type { Page } from "@playwright/test";
import { Schema } from "effect";
import { installMaturityFixture } from "./maturity-fixtures";
import { readFileSync } from "node:fs";

const realFeatureCatalog = JSON.parse(
  readFileSync(new URL("./feature-catalog.json", import.meta.url), "utf-8"),
);
import {
  FeatureCatalogSchema,
  FeatureStateSchema,
  FeatureApplyResponseSchema,
  FeatureOperationEnvelopeSchema,
  type FeatureState,
} from "../src/lib/features-api";

export interface FeatureWriteRecord {
  method: string;
  path: string;
  body: Record<string, unknown>;
}

export async function installFeaturesFixture(page: Page, baseURL: string) {
  const baseFixture = await installMaturityFixture(page, baseURL);
  const origin = new URL(baseURL).origin;

  const featureWrites: FeatureWriteRecord[] = [];
  const featureReads: string[] = [];

  let currentGeneration = 10;
  let pendingPollCount = 0;
  let shouldReturnCanConfirm = false;
  let currentPendingOperation: Record<string, unknown> | undefined;

  // Real native project data shapes matching factory controllers
  const stateStore: Record<string, Record<string, unknown>> = {
    "network/wan_info": {
      info: {
        details: {
          wanType: "pppoe",
          usernameConfigured: true,
          passwordConfigured: true,
          wan_name: "WAN1",
          mtu: 1480,
        },
        ipv4: [{ ip: "10.0.0.2", mask: "255.255.255.0" }],
        gateWay: "10.0.0.1",
        dnsAddrs: "10.0.0.1",
      },
    },
    "network/port_service": {
      service: "iptv",
      interface: "wan",
      vlan_id: 100,
      enabled: "1",
    },
    "network/lan_info": {
      info: {
        ipv4: [{ ip: "192.168.31.1", mask: "255.255.255.0" }],
        status: 1,
        mac: "AA:BB:CC:DD:EE:00",
      },
      linkList: [null, 0, 1000],
    },
    "network/macbind_info": {
      list: [
        { ip: "192.168.31.50", mac: "AA:BB:CC:DD:EE:01", name: "MyLaptop" },
        { ip: "192.168.31.51", mac: "AA:BB:CC:DD:EE:02", name: "SmartTV" },
      ],
    },
    "wireless/wifi_detail_all": {
      info: [
        {
          wifiIndex: 1,
          ssid: "Xiaomi_BE6500_5G",
          pwdConfigured: true,
          channel: "44",
          bandwidth: "160",
          band: "5GHz",
        },
        {
          wifiIndex: 2,
          ssid: "Xiaomi_BE6500_2.4G",
          pwdConfigured: true,
          channel: "6",
          bandwidth: "40",
          band: "2.4GHz",
        },
      ],
    },
    "wireless/wifi_share_info": {
      wifiIndex: 3,
      on: "0",
      ssid: "Xiaomi_Guest",
      pwdConfigured: false,
    },
    "services/forwarding": {
      list: [
        { name: "SSH Service", srcport: "2222", destip: "192.168.31.50", destport: "22", proto: "1" },
        { name: "NAS Web", srcport: "8080", destip: "192.168.31.51", destport: "80", proto: "1" },
      ],
    },
    "services/led": {
      status: 1,
      timer_status: 0,
      time_open: "00:00",
      time_close: "07:00",
    },
  };

  await page.route(
    (url) => url.origin === origin && url.pathname.startsWith("/api/features"),
    async (route) => {
      const req = route.request();
      const url = new URL(req.url());
      const method = req.method();

      if (method === "GET") {
        featureReads.push(`${url.pathname}${url.search}`);

        if (url.pathname === "/api/features/catalog") {
          const catalogData = {
            ...realFeatureCatalog,
            ...(currentPendingOperation ? { pendingOperation: currentPendingOperation } : {}),
          };
          const body = Schema.decodeUnknownSync(FeatureCatalogSchema)(catalogData);
          await route.fulfill({
            status: 200,
            contentType: "application/json",
            body: JSON.stringify(body),
          });
          return;
        }

        if (url.pathname.match(/^\/api\/features\/[^/]+\/state$/)) {
          const domain = url.pathname.split("/")[3];
          const readId = url.searchParams.get("read") || "";
          const key = `${domain}/${readId}`;
          const data = stateStore[key] || {};

          const state: FeatureState = {
            available: true,
            readId,
            generation: currentGeneration,
            sampledAt: new Date().toISOString(),
            data,
          };
          const body = Schema.decodeUnknownSync(FeatureStateSchema)(state);
          await route.fulfill({
            status: 200,
            contentType: "application/json",
            body: JSON.stringify(body),
          });
          return;
        }

        if (url.pathname === "/api/features/operations") {
          pendingPollCount++;
          const opId = url.searchParams.get("id") || "op-test";
          if (opId === "op-lan-confirm") {
            const canConfirm = pendingPollCount >= 2;
            if (currentPendingOperation) {
              currentPendingOperation.canConfirm = canConfirm;
            }
            const res = {
              operation: {
                id: "op-lan-confirm",
                state: "pending",
                actionId: "set_lan_ip",
                domain: "network",
                generation: currentGeneration,
                canConfirm,
                reconnectAddress: "192.168.50.1",
                waitingFor: "等待新网段连通确认",
              },
            };
            const body = Schema.decodeUnknownSync(FeatureOperationEnvelopeSchema)(res);
            await route.fulfill({
              status: 200,
              contentType: "application/json",
              body: JSON.stringify(body),
            });
            return;
          }
          const state = pendingPollCount >= 2 ? "completed" : "pending";
          const res = {
            operation: {
              id: opId,
              state,
              actionId: "set_wan",
              domain: "network",
              generation: currentGeneration,
            },
          };
          const body = Schema.decodeUnknownSync(FeatureOperationEnvelopeSchema)(res);
          await route.fulfill({
            status: 200,
            contentType: "application/json",
            body: JSON.stringify(body),
          });
          return;
        }
      }

      if (method === "POST") {
        const body = (await req.postDataJSON()) as Record<string, unknown>;
        featureWrites.push({
          method: "POST",
          path: url.pathname,
          body,
        });

        if (url.pathname.endsWith("/apply")) {
          currentGeneration += 1;
          const actionId = String(body.actionId);
          const domain = url.pathname.split("/")[3];

          if (actionId === "set_lan_ip" && shouldReturnCanConfirm) {
            pendingPollCount = 0;
            currentPendingOperation = {
              id: "op-lan-confirm",
              state: "pending",
              actionId,
              domain,
              generation: currentGeneration,
              canConfirm: false,
              reconnectAddress: "192.168.50.1",
              waitingFor: "等待新网段连通确认",
            };
            const res = {
              operation: currentPendingOperation,
            };
            await route.fulfill({
              status: 200,
              contentType: "application/json",
              body: JSON.stringify(res),
            });
            return;
          }

          if (actionId === "set_wan") {
            pendingPollCount = 0;
            const res = {
              operation: {
                id: "op-wan-poll",
                state: "pending",
                actionId,
                domain,
                generation: currentGeneration,
              },
            };
            await route.fulfill({
              status: 200,
              contentType: "application/json",
              body: JSON.stringify(res),
            });
            return;
          }

          // Default immediate completed
          const res = {
            operation: {
              id: `op-${actionId}`,
              state: "completed",
              actionId,
              domain,
              generation: currentGeneration,
            },
          };
          const validated = Schema.decodeUnknownSync(FeatureApplyResponseSchema)(res);
          await route.fulfill({
            status: 200,
            contentType: "application/json",
            body: JSON.stringify(validated),
          });
          return;
        }

        if (url.pathname === "/api/features/confirm") {
          currentGeneration += 1;
          currentPendingOperation = undefined;
          const res = {
            operation: {
              id: String(body.id),
              state: "completed",
              actionId: "set_lan_ip",
              domain: "network",
              generation: currentGeneration,
            },
          };
          const bodyVal = Schema.decodeUnknownSync(FeatureOperationEnvelopeSchema)(res);
          await route.fulfill({
            status: 200,
            contentType: "application/json",
            body: JSON.stringify(bodyVal),
          });
          return;
        }
      }

      await route.fallback();
    },
  );

  return {
    ...baseFixture,
    featureWrites,
    featureReads,
    setShouldReturnCanConfirm: (value: boolean) => {
      shouldReturnCanConfirm = value;
    },
    setPendingOperation: (op?: Record<string, unknown>) => {
      currentPendingOperation = op;
    },
  };
}
