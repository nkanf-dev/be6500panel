import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ThemeProvider } from "../../theme";
import { TrafficHistoryPanel } from "./TrafficHistoryPanel";
import { loadTrafficHistory } from "../../lib/traffic-history-api";
import { downloadTrafficHistory } from "../../lib/traffic-history-format";
import { TRAFFIC_HISTORY_RANGES } from "../../lib/traffic-history-contracts";
import { historyFixture } from "./history-fixture.test-data";
vi.mock("../../lib/traffic-history-api", () => ({
  loadTrafficHistory: vi.fn(),
}));
vi.mock("../../lib/traffic-history-format", async (importOriginal) => ({
  ...(await importOriginal<
    typeof import("../../lib/traffic-history-format")
  >()),
  downloadTrafficHistory: vi.fn(),
}));
vi.mock("../visualizations/EChart", () => ({
  EChart: ({ label }: { label: string }) => (
    <div role="img" aria-label={label} />
  ),
}));
const load = vi.mocked(loadTrafficHistory);
const renderPanel = (demo = false, active = true) =>
  render(
    <ThemeProvider>
      <TrafficHistoryPanel demo={demo} active={active} />
    </ThemeProvider>,
  );
beforeEach(() => {
  load.mockReset();
  vi.mocked(downloadTrafficHistory).mockReset();
});
afterEach(() => vi.restoreAllMocks());

describe("durable history panel", () => {
  it("offers all real time windows even before the first measurement", async () => {
    const empty = {
      ...historyFixture(),
      samples: [],
      oldestAt: undefined,
      summary: { rxBytes: 0, txBytes: 0, coverageSeconds: 0 },
    };
    load.mockImplementation(async (range) => ({ ...empty, range }));
    renderPanel();
    expect(
      screen
        .getAllByRole("option")
        .map((option) => (option as HTMLOptionElement).value),
    ).toEqual([...TRAFFIC_HISTORY_RANGES]);
    await screen.findByText("此时间范围没有真实记录");
    expect(screen.getByText("尚无真实记录")).toBeInTheDocument();
    expect(screen.queryByText("演示数据")).not.toBeInTheDocument();
    expect(screen.queryByRole("img")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "导出 CSV" })).toBeDisabled();
    for (const range of TRAFFIC_HISTORY_RANGES) {
      fireEvent.change(screen.getByLabelText("时间范围"), {
        target: { value: range },
      });
      await waitFor(() =>
        expect(load).toHaveBeenLastCalledWith(range, expect.any(AbortSignal)),
      );
    }
  });
  it("does not present an all-uncovered bucket grid as measured zero traffic", async () => {
    const history = historyFixture();
    load.mockResolvedValue({
      ...history,
      samples: history.samples.map((point) => ({
        ...point,
        rx: 0,
        tx: 0,
        rxPeak: 0,
        txPeak: 0,
        rxBytes: 0,
        txBytes: 0,
        coverageSeconds: 0,
      })),
      oldestAt: undefined,
      summary: { rxBytes: 0, txBytes: 0, coverageSeconds: 0 },
    });
    renderPanel();
    await screen.findByText("此时间范围没有真实记录");
    expect(screen.queryByRole("img")).not.toBeInTheDocument();
    expect(
      screen.queryByRole("table", { hidden: true }),
    ).not.toBeInTheDocument();
    expect(screen.queryByText("0 B")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "导出 CSV" })).toBeDisabled();
  });
  it("shows exact counter totals, recording start, retention, persistence, coverage and resolution", async () => {
    load.mockResolvedValue(historyFixture());
    renderPanel();
    await screen.findByText("42.9 MiB");
    expect(screen.getByText("21.5 MiB")).toBeInTheDocument();
    expect(screen.getByText("1 分钟 · 3.3%")).toBeInTheDocument();
    expect(screen.getByText("30 秒")).toBeInTheDocument();
    expect(screen.getByText("400 天")).toBeInTheDocument();
    expect(screen.getByText("已持久化")).toBeInTheDocument();
    expect(screen.getByText("2026-10-02T17:00:00.000Z")).toBeInTheDocument();
    expect(screen.getByText(/不补齐启用前的历史/)).toBeInTheDocument();
    expect(screen.queryByLabelText("样本范围")).not.toBeInTheDocument();
    expect(
      screen.getByRole("region", { name: "WAN 流量历史" }),
    ).toBeInTheDocument();
    const table = screen.getByRole("table", { hidden: true });
    expect(within(table).getAllByRole("row", { hidden: true })).toHaveLength(4);
    fireEvent.click(screen.getByRole("button", { name: "导出 CSV" }));
    expect(downloadTrafficHistory).toHaveBeenCalledWith(historyFixture());
  });
  it("clears stale totals/table/export when selecting a different range", async () => {
    let resolve!: (value: ReturnType<typeof historyFixture>) => void;
    load.mockResolvedValueOnce(historyFixture()).mockImplementationOnce(
      () =>
        new Promise((yes) => {
          resolve = yes;
        }),
    );
    renderPanel();
    await screen.findByText("42.9 MiB");
    fireEvent.change(screen.getByLabelText("时间范围"), {
      target: { value: "1y" },
    });
    expect(screen.queryByText("42.9 MiB")).not.toBeInTheDocument();
    expect(
      screen.queryByRole("table", { hidden: true }),
    ).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "导出 CSV" })).toBeDisabled();
    expect(screen.getByRole("status")).toHaveTextContent("正在读取流量历史");
    await act(async () =>
      resolve({ ...historyFixture("1y"), resolutionSeconds: 21600 }),
    );
    expect(screen.getByText("6 小时")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "导出 CSV" }));
    expect(vi.mocked(downloadTrafficHistory).mock.calls[0][0].range).toBe("1y");
  });
  it("makes persistence failures visible instead of implying durable history", async () => {
    load.mockResolvedValue({
      ...historyFixture(),
      persistent: false,
      error: "permission denied: history path",
    });
    renderPanel();
    expect(await screen.findByText("未持久化")).toBeInTheDocument();
    expect(
      screen.getByText("permission denied: history path"),
    ).toBeInTheDocument();
    expect(screen.getByText(/重启后记录可能丢失/)).toBeInTheDocument();
  });
  it("shows disabled collection and never invents a year of records", async () => {
    load.mockResolvedValue({
      ...historyFixture(),
      enabled: false,
      samples: [],
      oldestAt: undefined,
    });
    renderPanel();
    await screen.findByText("服务器未启用流量记录");
    expect(screen.getByText(/记录未启用/)).toBeInTheDocument();
    expect(
      screen.queryByRole("table", { hidden: true }),
    ).not.toBeInTheDocument();
    expect(screen.queryByText("演示数据")).not.toBeInTheDocument();
  });
  it("reports read errors and keeps retry and the range selector usable", async () => {
    load
      .mockRejectedValueOnce(new Error("history unavailable"))
      .mockResolvedValueOnce(historyFixture());
    renderPanel();
    await screen.findByRole("alert");
    expect(screen.getByRole("status")).toHaveTextContent("流量历史读取失败");
    fireEvent.click(screen.getByRole("button", { name: "重试" }));
    await screen.findByText("42.9 MiB");
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });
  it("keeps fixed demo samples explicit and never queries or exports them as real history", () => {
    renderPanel(true);
    expect(load).not.toHaveBeenCalled();
    expect(screen.getByText("演示数据")).toBeInTheDocument();
    expect(screen.getByText(/不代表服务器历史记录/)).toBeInTheDocument();
    expect(screen.queryByText("已持久化")).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "导出 CSV" }),
    ).not.toBeInTheDocument();
    expect(screen.getByText("61 条")).toBeInTheDocument();
  });
});
