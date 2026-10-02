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
describe("truthful workspace control labels", () => {
  it("does not infer control permission before health is known", () => {
    view();
    expect(screen.getByText("权限读取中")).toBeInTheDocument();
    expect(screen.queryByText("控制模式")).not.toBeInTheDocument();
    expect(screen.queryByText("Full control")).not.toBeInTheDocument();
  });
  it("names the enabled configuration capability without claiming full router parity", () => {
    state.health = { mode: "host", readOnly: false };
    view();
    expect(screen.getByText("配置控制已启用")).toBeInTheDocument();
    expect(screen.queryByText("Full control")).not.toBeInTheDocument();
  });
  it("keeps read-only deployment explicit", () => {
    state.health = { mode: "host", readOnly: true };
    view();
    expect(screen.getByText("配置只读")).toBeInTheDocument();
  });
});
