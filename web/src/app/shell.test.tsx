import { fireEvent, render, screen } from "@testing-library/react";
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
const shellElement = () => (
  <Shell
    page="overview"
    navigate={vi.fn()}
    onLogout={vi.fn()}
    authRequired={false}
  >
    content
  </Shell>
);
const view = () => render(shellElement());
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

describe("navigation stays mounted during background updates", () => {
  it("preserves hovered/focused menu nodes, scroll and advanced disclosure on a context rerender", () => {
    state.health = { mode: "host", readOnly: false };
    const rendered = view();
    const nav = screen.getByRole("navigation", { name: "模块导航" });
    const item = nav.querySelector<HTMLAnchorElement>('a[href="#/network"]')!;
    expect(item).not.toBeNull();
    const advanced = screen.getByText("高级诊断").closest("details")!;
    advanced.open = true;
    nav.scrollTop = 37;
    item.focus();
    fireEvent.mouseEnter(item);
    // ConsoleProvider's router/system streams create new context values even
    // when the selected page does not change. This must not remount navigation.
    state.health = { mode: "host", readOnly: false };
    rendered.rerender(shellElement());
    expect(screen.getByRole("navigation", { name: "模块导航" })).toBe(nav);
    expect(nav.querySelector('a[href="#/network"]')).toBe(item);
    expect(item.isConnected).toBe(true);
    expect(document.activeElement).toBe(item);
    expect(nav.scrollTop).toBe(37);
    expect(screen.getByText("高级诊断").closest("details")).toBe(advanced);
    expect(advanced.open).toBe(true);
  });
});

it("updates selection and collapse in place without replacing navigation", () => {
  const rendered = view();
  const nav = screen.getByRole("navigation", { name: "模块导航" });
  const network = nav.querySelector<HTMLAnchorElement>('a[href="#/network"]')!;
  fireEvent.click(screen.getByRole("button", { name: "收起导航" }));
  expect(screen.getByRole("navigation", { name: "模块导航" })).toBe(nav);
  expect(nav.querySelector('a[href="#/network"]')).toBe(network);
  expect(network).toHaveAttribute("title", "网络");
  fireEvent.click(screen.getByRole("button", { name: "展开导航" }));
  expect(nav.querySelector('a[href="#/network"]')).toBe(network);
  expect(network).not.toHaveAttribute("title");
  rendered.rerender(
    <Shell
      page="network"
      navigate={vi.fn()}
      onLogout={vi.fn()}
      authRequired={false}
    >
      content
    </Shell>,
  );
  expect(nav.querySelector('a[href="#/network"]')).toBe(network);
  expect(network).toHaveAttribute("aria-current", "page");
});
