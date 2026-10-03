import { Effect } from "effect";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { api } from "../../lib/api";
import { ProxyDiagnostics } from "./diagnostics";
import type { ProxyPolicySummary } from "./policy-contracts";
import { PolicyReview } from "./policy-review";

const summary: ProxyPolicySummary = {
  total: 26,
  supported: 1,
  omitted: 25,
  reasons: [
    {
      code: "unsupported-process-rule",
      count: 25,
      message: "process rules cannot classify forwarded LAN clients",
    },
  ],
  omittedRules: Array.from({ length: 25 }, (_, index) => ({
    index: index + 1,
    code: "unsupported-process-rule",
    message: "process rules cannot classify forwarded LAN clients",
  })),
  revision: "a".repeat(64),
};
beforeEach(() => {
  vi.spyOn(api, "logs").mockReturnValue(
    Effect.succeed({ entries: [], capacity: 100 }),
  );
});

describe("honest read-only policy review", () => {
  it("explains PROCESS-NAME gateway limits without claiming runtime activation or diagnostic hits", () => {
    render(<PolicyReview summary={summary} />);
    expect(
      screen.getByText("共 26 条规则 · 1 条可应用路由规则"),
    ).toBeInTheDocument();
    expect(
      screen.getByText(
        /PROCESS-NAME.*PROCESS-PATH.*路由器网关.*LAN 客户端.*进程.*不支持.*进程分流/,
      ),
    ).toBeInTheDocument();
    expect(screen.queryByText(/已生效|命中次数/)).not.toBeInTheDocument();
    expect(screen.queryByRole("checkbox")).not.toBeInTheDocument();
  });

  it("has a collapsed, bounded, paged omission preview with one-based display indices", async () => {
    const user = userEvent.setup();
    const view = render(<PolicyReview summary={summary} />);
    const toggle = screen.getByText("查看忽略规则明细");
    expect(toggle.closest("details")).not.toHaveAttribute("open");
    await user.click(toggle);
    const table = within(screen.getByRole("table", { name: "忽略规则明细" }));
    expect(table.getAllByRole("row")).toHaveLength(21);
    expect(table.getAllByRole("row")[1]).toHaveTextContent(/^2/);
    expect(
      screen.getByRole("button", { name: "上一页忽略规则" }),
    ).toBeDisabled();
    await user.click(screen.getByRole("button", { name: "下一页忽略规则" }));
    expect(table.getAllByRole("row")).toHaveLength(6);
    expect(table.getAllByRole("row")[1]).toHaveTextContent(/^22/);
    expect(
      screen.getByRole("button", { name: "下一页忽略规则" }),
    ).toBeDisabled();
    view.rerender(
      <PolicyReview summary={{ ...summary, revision: "b".repeat(64) }} />,
    );
    expect(
      screen.getByText("查看忽略规则明细").closest("details"),
    ).not.toHaveAttribute("open");
    await user.click(screen.getByText("查看忽略规则明细"));
    expect(
      within(screen.getByRole("table", { name: "忽略规则明细" })).getAllByRole(
        "row",
      )[1],
    ).toHaveTextContent(/^2/);
  });

  it("labels unknown legacy rule statistics without invented counts", () => {
    render(<PolicyReview />);
    expect(screen.getByText("路由规则统计未知")).toBeInTheDocument();
    expect(
      screen.queryByText(/共 \d+ 条规则|\d+ 条规则将忽略|\d+ 条可应用/),
    ).not.toBeInTheDocument();
    expect(screen.queryByRole("checkbox")).not.toBeInTheDocument();
  });

  it("keeps diagnostic multiplicity distinct from omissions and shows the summary only once without a checkbox", () => {
    render(
      <ProxyDiagnostics
        policySummary={summary}
        diagnostics={Array.from({ length: 3 }, () => ({
          scope: "rule",
          index: 1,
          code: "unsupported-process-rule",
          message: "process rules cannot classify forwarded LAN clients",
        }))}
      />,
    );
    expect(screen.getByText("3 条校验结果")).toBeInTheDocument();
    expect(screen.getAllByText("25 条规则将忽略")).toHaveLength(1);
    expect(screen.queryByText("3 条规则将忽略")).not.toBeInTheDocument();
    expect(screen.queryByRole("checkbox")).not.toBeInTheDocument();
    const review = screen.getByRole("region", { name: "路由规则审阅" });
    expect(
      within(review).queryByText(/已生效|命中次数/),
    ).not.toBeInTheDocument();
  });

  it("does not treat empty diagnostics as proof that all rules are supported", () => {
    render(<ProxyDiagnostics />);
    expect(screen.getByText("0 条校验结果")).toBeInTheDocument();
    expect(screen.getByText("路由规则统计未知")).toBeInTheDocument();
    expect(
      screen.queryByText(/0 条规则将忽略|0 条可应用/),
    ).not.toBeInTheDocument();
  });
});
