import { render, screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { DeviceProxyDetails } from "./device-proxy-details";
import type { WorkspaceDevice } from "./device-model";
import type { ProxyMetrics } from "../proxy/telemetry-api";
import { routerSnapshot } from "../production-fixtures.test-data";

const time = new Date().toISOString();
const mac = "02:00:00:00:00:20";
const device: WorkspaceDevice = {
  mac,
  hostname: "laptop",
  addresses: ["192.0.2.20"],
  currentAddresses: ["192.0.2.20"],
  currentAddressSampledAt: time,
  leases: [],
};
const capability = { available: true, reason: "" };
const metrics: ProxyMetrics = {
  state: "ready",
  reason: "",
  source: "sing-box",
  sampledAt: time,
  activeConnections: 800,
  truncated: true,
  totals: { uploadBytes: 999999, downloadBytes: 999999 },
  traffic: [],
  probes: [],
  capabilities: {
    connections: capability,
    traffic: capability,
    routing: capability,
    latency: capability,
    requestPhases: capability,
  },
  connections: [
    {
      id: "selected",
      startedAt: time,
      ageMs: 0,
      network: "tcp",
      sourceIP: "192.0.2.20",
      sourcePort: 1234,
      destinationIP: "198.51.100.1",
      destinationPort: 443,
      host: "device.example.test",
      uploadBytes: 200,
      downloadBytes: 100,
      outbound: "proxy",
      ruleId: "r1",
      rule: "domain",
    },
    {
      id: "unmatched",
      startedAt: time,
      ageMs: 0,
      network: "tcp",
      sourceIP: "192.0.2.99",
      sourcePort: 1234,
      destinationIP: "198.51.100.2",
      destinationPort: 443,
      host: "unmatched.example.test",
      uploadBytes: 1234,
      downloadBytes: 5678,
      outbound: "direct",
      ruleId: "",
      rule: "",
    },
  ],
};
describe("device core connection detail", () => {
  it("renders only matching observed core rows with source age and actual outbound, never global totals", () => {
    render(
      <DeviceProxyDetails
        devices={[device]}
        inventory={[device]}
        metrics={metrics}
        snapshot={{ ...routerSnapshot, sampledAt: time }}
      />,
    );
    expect(screen.getByText("1 个匹配的已观测连接")).toBeInTheDocument();
    expect(screen.getByText("代理出口 1")).toBeInTheDocument();
    expect(screen.getByText("直连出口 0")).toBeInTheDocument();
    expect(screen.getByText("device.example.test")).toBeInTheDocument();
    expect(
      screen.queryByText("unmatched.example.test"),
    ).not.toBeInTheDocument();
    expect(screen.getByText(/源数据年龄：0 秒/)).toBeInTheDocument();
    expect(screen.getByText(/核心列表已截断/)).toBeInTheDocument();
    expect(screen.queryByText("800")).not.toBeInTheDocument();
    expect(screen.queryByText("999999")).not.toBeInTheDocument();
    const reference = screen.getByRole("table", {
      name: "内核当前默认路由 · 非逐设备分流命中",
    });
    expect(within(reference).getByText("wan-test")).toBeInTheDocument();
  });
  it("hides stale/failing core observations and routes with concrete refresh action", () => {
    const old = new Date(Date.now() - 60_000).toISOString();
    render(
      <DeviceProxyDetails
        devices={[device]}
        inventory={[device]}
        metrics={{ ...metrics, sampledAt: old }}
        error={new Error("controller timeout")}
        snapshot={{ ...routerSnapshot, sampledAt: old }}
        onRefresh={() => {}}
      />,
    );
    expect(screen.queryByText("device.example.test")).not.toBeInTheDocument();
    expect(screen.getByText("等待有效的设备核心连接采样")).toBeInTheDocument();
    expect(screen.getByRole("alert")).toHaveTextContent("controller timeout");
    expect(
      screen.getByRole("button", { name: "刷新采样" }),
    ).toBeInTheDocument();
    expect(screen.queryByText("wan-test")).not.toBeInTheDocument();
  });
  it("does not attribute ambiguous current addresses", () => {
    const other = { ...device, mac: "02:00:00:00:00:21" };
    render(
      <DeviceProxyDetails
        devices={[device]}
        inventory={[device, other]}
        metrics={metrics}
      />,
    );
    expect(screen.getByText(/1 个地址归属不唯一/)).toBeInTheDocument();
    expect(screen.queryByText("device.example.test")).not.toBeInTheDocument();
    expect(screen.getByText("当前没有匹配的核心连接")).toBeInTheDocument();
  });
});
