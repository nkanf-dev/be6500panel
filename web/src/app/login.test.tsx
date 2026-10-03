import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { Login } from "./login";
vi.mock("./theme-menu", () => ({ ThemeMenu: () => null }));
describe("control-center login", () => {
  it("presents management tasks rather than a read-only or internal transport mode", () => {
    render(<Login onLogin={vi.fn()} />);
    expect(screen.getByRole("heading", { name: "登录控制中心" })).toBeInTheDocument();
    expect(screen.getByText("登录后管理设备、网络与服务")).toBeInTheDocument();
    expect(screen.queryByText(/只读|HttpOnly|cookie/)).not.toBeInTheDocument();
  });
});
