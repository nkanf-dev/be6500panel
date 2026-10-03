import { render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { Shell } from "./shell";

const state = vi.hoisted(() => ({
  health: undefined as undefined | { mode: string; readOnly: boolean },
}));
vi.mock("./console-context", () => ({
  useConsole: () => ({ ...state, connection: "connecting", refresh: vi.fn() }),
}));
vi.mock("./theme-menu", () => ({ ThemeMenu: () => null }));
vi.mock("./command-palette", () => ({ CommandPalette: () => null }));
afterEach(() => {
  state.health = undefined;
});
const view = () =>
  render(
    <Shell
      page="overview"
      navigate={vi.fn()}
      onLogout={vi.fn()}
      authRequired={false}
    >
      content
    </Shell>,
  );
describe("professional gateway control labels", () => {
  it("shows connection progress before the gateway responds", () => {
    view();
    expect(screen.getByText("正在连接网关")).toBeInTheDocument();
    expect(screen.queryByText("控制模式")).not.toBeInTheDocument();
    expect(screen.queryByText("Full control")).not.toBeInTheDocument();
  });
  it("shows the connected gateway without implementation-mode framing", () => {
    state.health = { mode: "host", readOnly: false };
    view();
    expect(screen.getAllByText("已连接网关")[0]).toBeInTheDocument();
    expect(screen.queryByText("Full control")).not.toBeInTheDocument();
    expect(screen.getByText("高级诊断").closest("details")).not.toHaveAttribute(
      "open",
    );
  });
  it("keeps connection identity independent of individual action capability", () => {
    state.health = { mode: "host", readOnly: true };
    view();
    expect(screen.getAllByText("已连接网关")).toHaveLength(2);
    expect(screen.queryByText("配置只读")).not.toBeInTheDocument();
  });
});
