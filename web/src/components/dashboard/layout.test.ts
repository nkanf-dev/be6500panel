import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  DASHBOARD_STORAGE_KEY,
  defaultLayout,
  moveWidget,
  readLayout,
  saveLayout,
  widgetDefinitions,
} from "./layout";

beforeEach(() => window.localStorage.clear());
afterEach(() => vi.restoreAllMocks());

describe("browser-local dashboard layout", () => {
  it("starts with all registered widgets and independent default copies", () => {
    const first = defaultLayout();
    first.widgets[0].visible = false;
    expect(readLayout().layout.widgets.map((widget) => widget.id)).toEqual(
      widgetDefinitions.map((widget) => widget.id),
    );
    expect(readLayout().layout.widgets.every((widget) => widget.visible)).toBe(
      true,
    );
    expect(readLayout().notice).toBeUndefined();
  });
  it("persists the named visibility, order and size settings only", () => {
    const layout = moveWidget(defaultLayout(), "environment", -1);
    layout.name = "  路由器监控  ";
    layout.widgets[0].visible = false;
    layout.widgets[1].size = "full";
    expect(saveLayout(layout).ok).toBe(true);
    expect(readLayout().layout).toEqual({ ...layout, name: "路由器监控" });
    const stored = JSON.parse(
      window.localStorage.getItem(DASHBOARD_STORAGE_KEY)!,
    );
    expect(Object.keys(stored)).toEqual(["name", "widgets"]);
    expect(Object.keys(stored.widgets[0])).toEqual(["id", "visible", "size"]);
  });
  it.each([
    "{not-json",
    "null",
    JSON.stringify({ name: "", widgets: [] }),
    JSON.stringify({
      name: "bad",
      widgets: [{ id: "systemSummary", visible: "yes", size: "full" }],
    }),
    JSON.stringify({
      name: "bad",
      widgets: [{ id: "systemSummary", visible: true, size: "enormous" }],
    }),
    JSON.stringify({
      name: "bad",
      widgets: [
        { id: "systemSummary", visible: true, size: "full" },
        { id: "systemSummary", visible: true, size: "wide" },
      ],
    }),
    "x".repeat(16_385),
  ])("recovers corrupt or invalid state without crashing (%#)", (raw) => {
    window.localStorage.setItem(DASHBOARD_STORAGE_KEY, raw);
    expect(readLayout().layout).toEqual(defaultLayout());
    expect(readLayout().notice).toMatch(/布局/);
    // Reading is not a write. The user can explicitly save a replacement.
    expect(window.localStorage.getItem(DASHBOARD_STORAGE_KEY)).toBe(raw);
  });
  it("preserves known settings when widgets are added or removed", () => {
    window.localStorage.setItem(
      DASHBOARD_STORAGE_KEY,
      JSON.stringify({
        name: "Saved",
        widgets: [
          { id: "retired", visible: false, size: "full" },
          { id: "devices", visible: false, size: "wide" },
        ],
      }),
    );
    const layout = readLayout().layout;
    expect(layout.widgets[0]).toEqual({
      id: "devices",
      visible: false,
      size: "wide",
    });
    expect(layout.widgets).toHaveLength(widgetDefinitions.length);
    expect(layout.widgets.slice(1).every((widget) => widget.visible)).toBe(
      true,
    );
  });
  it("allows hiding every widget and rejects invalid names without overwriting", () => {
    const layout = defaultLayout();
    layout.widgets.forEach((widget) => {
      widget.visible = false;
    });
    expect(saveLayout(layout).ok).toBe(true);
    expect(readLayout().layout.widgets.every((widget) => !widget.visible)).toBe(
      true,
    );
    const previous = window.localStorage.getItem(DASHBOARD_STORAGE_KEY);
    expect(saveLayout({ ...layout, name: " " }).ok).toBe(false);
    expect(saveLayout({ ...layout, name: "x".repeat(65) }).ok).toBe(false);
    expect(window.localStorage.getItem(DASHBOARD_STORAGE_KEY)).toBe(previous);
  });
  it("reports unavailable storage and never claims a failed write succeeded", () => {
    vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new DOMException("Blocked", "SecurityError");
    });
    expect(readLayout().layout).toEqual(defaultLayout());
    expect(readLayout().notice).toContain("无法读取");
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new DOMException("Full", "QuotaExceededError");
    });
    expect(saveLayout(defaultLayout())).toEqual({
      ok: false,
      message: expect.stringContaining("无法保存"),
    });
  });
  it("moves by keyboard-friendly steps without crossing boundaries or mutating", () => {
    const original = defaultLayout();
    expect(moveWidget(original, "systemSummary", -1)).toBe(original);
    expect(moveWidget(original, "moduleStatus", 1)).toBe(original);
    const changed = moveWidget(original, "environment", -1);
    expect(changed.widgets.map((widget) => widget.id)).toEqual([
      "systemSummary",
      "trafficHistory",
      "deviceActivity",
      "environment",
      "networkDiagnostics",
      "devices",
      "proxy",
      "moduleStatus",
    ]);
    expect(original.widgets.map((widget) => widget.id)).toEqual(
      widgetDefinitions.map((widget) => widget.id),
    );
  });
});
