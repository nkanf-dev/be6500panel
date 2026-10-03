import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ReactNode } from "react";
import { App } from "./app";
import { ThemeProvider } from "../theme";
const renderApp = () =>
  render(
    <ThemeProvider>
      <App />
    </ThemeProvider>,
  );
import { jsonResponse } from "../modules/production-fixtures.test-data";

const cleanup = vi.hoisted(() => ({
  configuration: vi.fn(),
  runtime: vi.fn(),
}));
vi.mock("../components/configuration", () => ({
  clearConfigurationSession: cleanup.configuration,
}));
vi.mock("../modules/runtime/editor-session", () => ({
  clearRuntimeEditorSession: cleanup.runtime,
}));
vi.mock("./console-context", () => ({
  ConsoleProvider: ({ children }: { children: ReactNode }) => <>{children}</>,
}));
vi.mock("./shell", () => ({
  Shell: ({
    children,
    navigate,
    onLogout,
    recoveryBanner,
  }: {
    children: ReactNode;
    navigate: (id: string) => void;
    onLogout: () => void;
    recoveryBanner?: ReactNode;
  }) => (
    <>
      {recoveryBanner}
      <button onClick={() => navigate("system")}>系统页面</button>
      <button onClick={onLogout}>退出登录</button>
      {children}
    </>
  ),
}));
vi.mock("./pages", () => ({
  ModulePage: ({ page }: { page: string }) => (
    <p data-testid="current-page">{page}</p>
  ),
}));
const operation = {
  id: "pending-test",
  state: "pending_confirmation",
  phase: "pending",
  generation: 4,
  changedModules: ["network"],
  deadline: new Date(Date.now() + 60_000).toISOString(),
  canConfirm: true,
  canRollback: true,
};
let authenticated: boolean;
let fetchMock: ReturnType<typeof vi.fn>;
beforeEach(() => {
  authenticated = true;
  window.location.hash = "/overview";
  window.scrollTo = vi.fn();
  vi.clearAllMocks();
  fetchMock = vi.fn((url: string) => {
    if (url === "/api/session" || url === "/api/session/logout") {
      if (url.endsWith("logout")) authenticated = false;
      return Promise.resolve(
        jsonResponse({ authenticated, authRequired: true }),
      );
    }
    if (url === "/api/configuration/status")
      return Promise.resolve(
        jsonResponse({ enabled: true, generation: 4, operation }),
      );
    throw new Error(`Unexpected request: ${url}`);
  });
  vi.stubGlobal("fetch", fetchMock);
});
afterEach(() => vi.unstubAllGlobals());

describe("authenticated global recovery wiring", () => {
  it("mounts one persistent banner across page changes without replaying any mutation", async () => {
    const user = userEvent.setup();
    renderApp();
    const banner = await screen.findByRole("region", { name: "全局配置恢复" });
    expect(
      screen.getAllByRole("region", { name: "全局配置恢复" }),
    ).toHaveLength(1);
    expect(screen.getByRole("button", { name: "确认生效" })).toBeEnabled();
    await user.click(screen.getByRole("button", { name: "系统页面" }));
    await waitFor(() =>
      expect(screen.getByTestId("current-page")).toHaveTextContent("system"),
    );
    expect(screen.getByRole("region", { name: "全局配置恢复" })).toBe(banner);
    expect(
      fetchMock.mock.calls.filter(
        ([url]) => url === "/api/configuration/status",
      ),
    ).toHaveLength(1);
    expect(
      fetchMock.mock.calls.every(([, init]) => !init || init.method === "GET"),
    ).toBe(true);
  });
  it("clears private sessions and unmounts recovery on explicit logout", async () => {
    const user = userEvent.setup();
    renderApp();
    await screen.findByRole("region", { name: "全局配置恢复" });
    await user.click(screen.getByRole("button", { name: "退出登录" }));
    await screen.findByRole("heading", { name: "登录控制中心" });
    expect(
      screen.queryByRole("region", { name: "全局配置恢复" }),
    ).not.toBeInTheDocument();
    expect(cleanup.configuration).toHaveBeenCalledOnce();
    expect(cleanup.runtime).toHaveBeenCalledOnce();
  });
  it("clears recovery on 401 and does not mount it before authentication", async () => {
    const { unmount } = renderApp();
    await screen.findByRole("region", { name: "全局配置恢复" });
    act(() => window.dispatchEvent(new Event("be6500panel:unauthorized")));
    await screen.findByRole("heading", { name: "登录控制中心" });
    expect(
      screen.queryByRole("region", { name: "全局配置恢复" }),
    ).not.toBeInTheDocument();
    expect(cleanup.configuration).toHaveBeenCalledOnce();
    expect(cleanup.runtime).toHaveBeenCalledOnce();
    unmount();
    authenticated = false;
    fetchMock.mockClear();
    renderApp();
    await screen.findByRole("heading", { name: "登录控制中心" });
    expect(
      fetchMock.mock.calls.some(([url]) => url === "/api/configuration/status"),
    ).toBe(false);
  });
});
