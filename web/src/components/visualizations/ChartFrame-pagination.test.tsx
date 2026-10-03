import {
  render,
  screen,
  fireEvent,
  waitFor,
  within,
} from "@testing-library/react";
import { describe, it, expect } from "vitest";
import { ChartFrame } from "./ChartFrame";
const rows = Array.from(
  { length: 576 },
  (_, i) => ["fixture-device", i === 0 ? "缺测" : i] as const,
);
const view = () =>
  render(
    <ChartFrame
      title="设备流量对比"
      subtitle="同源"
      unavailable="empty"
      demo={false}
      hasData
      source="trafficd"
      columns={["设备", "计数"]}
      rows={rows}
      lazyTable
      tablePageSize={100}
    />,
  );
describe("bounded chart data table", () => {
  it("does not mount collapsed rows and retains full source count in summary", async () => {
    view();
    expect(screen.getByText("576 条")).toBeInTheDocument();
    expect(
      screen.queryByRole("table", { hidden: true }),
    ).not.toBeInTheDocument();
    const summary = screen.getByText("查看数据表").closest("summary")!;
    fireEvent.click(summary);
    await waitFor(() => expect(screen.getByRole("table")).toBeInTheDocument());
    expect(within(screen.getByRole("table")).getAllByRole("row")).toHaveLength(
      101,
    );
    expect(screen.getByText("缺测")).toBeInTheDocument();
    expect(screen.getByText("第 1 / 6 页 · 共 576 条")).toBeInTheDocument();
    fireEvent.click(
      screen.getByRole("button", { name: "设备流量对比 数据表末页" }),
    );
    expect(within(screen.getByRole("table")).getAllByRole("row")).toHaveLength(
      77,
    );
    expect(screen.getByText("575")).toBeInTheDocument();
    fireEvent.click(summary);
    await waitFor(() =>
      expect(
        screen.queryByRole("table", { hidden: true }),
      ).not.toBeInTheDocument(),
    );
  });
  it("keeps one-page default tables backward compatible", () => {
    render(
      <ChartFrame
        title="small"
        subtitle="same"
        unavailable="empty"
        demo={false}
        hasData
        source="trafficd"
        columns={["设备", "计数"]}
        rows={rows.slice(0, 2)}
      />,
    );
    expect(screen.getByRole("table", { hidden: true })).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: /数据表下一页/ }),
    ).not.toBeInTheDocument();
  });
});
