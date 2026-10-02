import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { PlanView } from "./plan-view";
const plan = {
  id: "synthetic",
  generation: 1,
  readOnly: true,
  canApply: false,
  summary: "Synthetic preview",
  steps: [
    { module: "network", action: "exclude", detail: "Proposed exclusions" },
  ],
  warnings: [],
};
describe("read-only plan status", () => {
  it("shows ordered proposals, not completed operations", () => {
    const { container } = render(<PlanView plan={plan} />);
    expect(screen.getByText("仅预览 · 未执行任何变更")).toBeInTheDocument();
    expect(screen.getByText("拟议步骤")).toBeInTheDocument();
    expect(container.querySelector(".plan-steps .text-success")).toBeNull();
    expect(screen.getByRole("button", { name: "应用计划" })).toBeDisabled();
  });
});
