import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ModulePage } from "./pages";
vi.mock("./device-page", () => ({
  DevicePage: () => <p>设备详情与对比工作区</p>,
}));
describe("device route", () => {
  it("dispatches devices to detailed workspace, not the old narrow unavailable table", async () => {
    render(<ModulePage page="devices" navigate={vi.fn()} />);
    expect(await screen.findByText("设备详情与对比工作区")).toBeInTheDocument();
  });
});
