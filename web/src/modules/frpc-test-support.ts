import { screen } from "@testing-library/react";
import type userEvent from "@testing-library/user-event";
import { beforeEach, vi } from "vitest";
import { clearFrpcFormSession } from "./frpc-session";
beforeEach(clearFrpcFormSession);
import type { RuntimeStatus } from "../lib/contracts";

export const frpcStatus: RuntimeStatus = {
  service: "frpc",
  state: "notconfigured",
  generation: 7,
  configured: false,
  artifactAvailable: true,
  rssBytes: 0,
  rssAvailable: false,
  desired: false,
  restarts: 0,
};
const respond = (body: unknown, code = 200) =>
  new Response(JSON.stringify(body), {
    status: code,
    headers: { "Content-Type": "application/json" },
  });
export function mockRuntime(
  options: {
    enabled?: boolean;
    missingStatus?: boolean;
    failure?: boolean;
    artifactAvailable?: boolean;
    config?: string;
    configGeneration?: () => number;
    configured?: boolean;
    observedStatus?: () => RuntimeStatus | undefined;
  } = {},
) {
  let currentStatus = {
    ...frpcStatus,
    configured: options.configured ?? false,
    artifactAvailable: options.artifactAvailable ?? true,
  };
  let currentConfig =
    options.config ??
    'serverAddr = "private.example.test"\nauth.token = "synthetic-stored-token"\n';
  const fetch = vi.fn((url: string, init?: RequestInit) => {
    if (url === "/api/runtime") {
      const observed = options.observedStatus
        ? options.observedStatus()
        : currentStatus;
      return Promise.resolve(
        respond({
          enabled: options.enabled ?? true,
          services: options.missingStatus || !observed ? [] : [observed],
        }),
      );
    }
    if (url === "/api/runtime/config?service=frpc")
      return Promise.resolve(
        respond({
          service: "frpc",
          generation:
            options.configGeneration?.() ??
            options.observedStatus?.()?.generation ??
            currentStatus.generation,
          config: currentConfig,
        }),
      );
    if (url === "/api/runtime/configure" && init?.method === "POST") {
      if (options.failure)
        return Promise.resolve(
          respond(
            {
              error: { code: "generation_conflict", message: "配置版本已变化" },
            },
            409,
          ),
        );
      currentConfig = JSON.parse(init.body as string).config;
      currentStatus = {
        ...currentStatus,
        state: "stopped",
        generation: 8,
        configured: true,
      };
      return Promise.resolve(respond(currentStatus));
    }
    return Promise.reject(new Error(`Unexpected test request: ${url}`));
  });
  vi.stubGlobal("fetch", fetch);
  return fetch;
}
export const commitButton = () =>
  screen.getByRole("button", { name: "校验并保存 frpc 配置" });
export async function setServer(user: ReturnType<typeof userEvent.setup>) {
  await user.type(
    screen.getByRole("textbox", { name: "服务器地址" }),
    "frps.example.test",
  );
}
