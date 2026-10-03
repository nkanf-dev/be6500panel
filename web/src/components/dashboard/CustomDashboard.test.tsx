import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { CustomDashboard } from "./CustomDashboard";
import {
  DASHBOARD_STORAGE_KEY,
  defaultLayout,
  widgetDefinitions,
} from "./layout";

vi.mock("./widgets", () => ({
  DashboardWidget: ({
    id,
    navigate,
  }: {
    id: string;
    navigate: (id: string) => void;
  }) => (
    <section>
      <p>Observed {id}</p>
      <button onClick={() => navigate("system")}>Open {id}</button>
    </section>
  ),
}));
const visibleIds = () =>
  within(screen.getByRole("list", { name: "仪表盘组件" }))
    .getAllByRole("listitem")
    .map((element) => element.dataset.widgetId);
const beginEditing = async (user: ReturnType<typeof userEvent.setup>) =>
  user.click(screen.getByRole("button", { name: "编辑布局" }));
beforeEach(() => window.localStorage.clear());
afterEach(() => vi.restoreAllMocks());

describe("custom dashboard layout flows", () => {
  it("previews visibility, order and width through keyboard controls; persists only on Save", async () => {
    const user = userEvent.setup();
    const first = render(<CustomDashboard navigate={vi.fn()} />);
    expect(visibleIds()).toEqual(widgetDefinitions.map((widget) => widget.id));
    await beginEditing(user);
    expect(screen.getByRole("textbox", { name: "布局名称" })).toHaveFocus();
    const firstMove = screen.getByRole("button", { name: "上移系统摘要" });
    expect(firstMove).toBeDisabled();
    const toggle = screen.getByRole("checkbox", { name: "显示运行环境" });
    toggle.focus();
    await user.keyboard(" ");
    expect(visibleIds()).not.toContain("environment");
    const move = screen.getByRole("button", { name: "上移WAN 历史" });
    move.focus();
    await user.keyboard("{Enter}");
    expect(visibleIds()[0]).toBe("trafficHistory");
    await user.selectOptions(
      screen.getByRole("combobox", { name: "WAN 历史宽度" }),
      "full",
    );
    expect(screen.getByRole("listitem", { name: "WAN 历史" })).toHaveAttribute(
      "data-widget-size",
      "full",
    );
    await user.clear(screen.getByRole("textbox", { name: "布局名称" }));
    await user.type(
      screen.getByRole("textbox", { name: "布局名称" }),
      "家中路由器",
    );
    expect(window.localStorage.getItem(DASHBOARD_STORAGE_KEY)).toBeNull();
    await user.click(screen.getByRole("button", { name: "保存布局" }));
    expect(
      screen.queryByRole("region", { name: "布局编辑器" }),
    ).not.toBeInTheDocument();
    expect(screen.getByText("家中路由器")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "编辑布局" })).toHaveFocus();
    expect(
      JSON.parse(window.localStorage.getItem(DASHBOARD_STORAGE_KEY)!)
        .widgets[0],
    ).toEqual({ id: "trafficHistory", visible: true, size: "full" });
    first.unmount();
    render(<CustomDashboard navigate={vi.fn()} />);
    expect(visibleIds()[0]).toBe("trafficHistory");
    expect(visibleIds()).not.toContain("environment");
    expect(screen.getByRole("listitem", { name: "WAN 历史" })).toHaveAttribute(
      "data-widget-size",
      "full",
    );
    expect(screen.getByText("家中路由器")).toBeInTheDocument();
  });
  it("cancels previews and reset previews without changing saved layout", async () => {
    const user = userEvent.setup();
    const stored = defaultLayout();
    stored.name = "Saved";
    stored.widgets.find((widget) => widget.id === "environment")!.visible =
      false;
    window.localStorage.setItem(DASHBOARD_STORAGE_KEY, JSON.stringify(stored));
    render(<CustomDashboard navigate={vi.fn()} />);
    await beginEditing(user);
    await user.click(screen.getByRole("checkbox", { name: "显示设备观察" }));
    await user.selectOptions(
      screen.getByRole("combobox", { name: "系统摘要宽度" }),
      "compact",
    );
    await user.click(screen.getByRole("button", { name: "下移系统摘要" }));
    await user.click(screen.getByRole("button", { name: "取消" }));
    expect(screen.getByRole("button", { name: "编辑布局" })).toHaveFocus();
    expect(visibleIds()[0]).toBe("systemSummary");
    expect(visibleIds()).toContain("devices");
    expect(screen.getByRole("listitem", { name: "系统摘要" })).toHaveAttribute(
      "data-widget-size",
      "full",
    );
    expect(
      JSON.parse(window.localStorage.getItem(DASHBOARD_STORAGE_KEY)!),
    ).toEqual(stored);
    await beginEditing(user);
    await user.click(screen.getByRole("button", { name: "恢复默认" }));
    expect(visibleIds()).toContain("environment");
    expect(screen.getByRole("textbox", { name: "布局名称" })).toHaveValue(
      "我的仪表盘",
    );
    await user.click(screen.getByRole("button", { name: "取消" }));
    expect(visibleIds()).not.toContain("environment");
    expect(screen.getByText("Saved")).toBeInTheDocument();
    expect(
      JSON.parse(window.localStorage.getItem(DASHBOARD_STORAGE_KEY)!),
    ).toEqual(stored);
    await beginEditing(user);
    await user.click(screen.getByRole("button", { name: "恢复默认" }));
    await user.click(screen.getByRole("button", { name: "保存布局" }));
    expect(
      JSON.parse(window.localStorage.getItem(DASHBOARD_STORAGE_KEY)!),
    ).toEqual(defaultLayout());
  });
  it("shows the empty layout and restores hidden widgets through editing", async () => {
    const user = userEvent.setup();
    const layout = defaultLayout();
    layout.widgets.forEach((widget) => {
      widget.visible = false;
    });
    window.localStorage.setItem(DASHBOARD_STORAGE_KEY, JSON.stringify(layout));
    render(<CustomDashboard navigate={vi.fn()} />);
    expect(screen.getByText("所有组件已隐藏")).toBeInTheDocument();
    expect(
      screen.queryByRole("list", { name: "仪表盘组件" }),
    ).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "选择组件" }));
    await user.click(screen.getByRole("checkbox", { name: "显示系统摘要" }));
    expect(visibleIds()).toEqual(["systemSummary"]);
    await user.click(screen.getByRole("button", { name: "保存布局" }));
    expect(screen.queryByText("所有组件已隐藏")).not.toBeInTheDocument();
  });
  it("explains corrupt saved state and repairs it with an explicit save", async () => {
    const user = userEvent.setup();
    window.localStorage.setItem(DASHBOARD_STORAGE_KEY, "{broken");
    render(<CustomDashboard navigate={vi.fn()} />);
    expect(screen.getByText(/保存的布局已损坏/)).toBeInTheDocument();
    expect(visibleIds()).toHaveLength(widgetDefinitions.length);
    await beginEditing(user);
    await user.click(screen.getByRole("button", { name: "保存布局" }));
    expect(screen.queryByText(/保存的布局已损坏/)).not.toBeInTheDocument();
    expect(
      JSON.parse(window.localStorage.getItem(DASHBOARD_STORAGE_KEY)!),
    ).toEqual(defaultLayout());
  });
  it("does not close edits or claim success after a blocked save", async () => {
    const user = userEvent.setup();
    render(<CustomDashboard navigate={vi.fn()} />);
    await beginEditing(user);
    await user.click(screen.getByRole("checkbox", { name: "显示设备观察" }));
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new DOMException("Blocked", "QuotaExceededError");
    });
    await user.click(screen.getByRole("button", { name: "保存布局" }));
    expect(screen.getByRole("alert")).toHaveTextContent("无法保存到浏览器");
    expect(
      screen.getByRole("region", { name: "布局编辑器" }),
    ).toBeInTheDocument();
    expect(visibleIds()).not.toContain("devices");
    expect(window.localStorage.getItem(DASHBOARD_STORAGE_KEY)).toBeNull();
    await user.click(screen.getByRole("button", { name: "取消" }));
    expect(visibleIds()).toContain("devices");
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });
  it("requires a layout name and retains access to module navigation", async () => {
    const user = userEvent.setup();
    const navigate = vi.fn();
    render(<CustomDashboard navigate={navigate} />);
    await user.click(
      screen.getByRole("button", { name: "Open systemSummary" }),
    );
    expect(navigate).toHaveBeenCalledWith("system");
    await beginEditing(user);
    await user.clear(screen.getByRole("textbox", { name: "布局名称" }));
    await user.click(screen.getByRole("button", { name: "保存布局" }));
    expect(screen.getByRole("alert")).toHaveTextContent("布局名称");
    expect(
      screen.getByRole("region", { name: "布局编辑器" }),
    ).toBeInTheDocument();
    expect(window.localStorage.getItem(DASHBOARD_STORAGE_KEY)).toBeNull();
  });
});
