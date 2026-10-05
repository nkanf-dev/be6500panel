import { test, expect, type Locator, type Page } from "@playwright/test";
import {
  installScaleFixture,
  scaleDeviceMAC,
  scaleDeviceName,
  scaleNodeID,
  scaleNodeLabel,
} from "./maturity-scale-fixtures";

type Fixture = Awaited<ReturnType<typeof installScaleFixture>>;
const install = (page: Page, baseURL?: string) => {
  if (!baseURL)
    throw new Error("Scale fixture requires the configured baseURL");
  return installScaleFixture(page, baseURL);
};
const activityReads = (fixture: Fixture) =>
  fixture.reads
    .filter((read) => read.startsWith("/api/devices/activity?"))
    .map((read) => new URL(read, "http://fixture.example.test"));
const hasActivityRead = (fixture: Fixture, search: string, limit: string) =>
  activityReads(fixture).some(
    (url) =>
      url.searchParams.get("search")?.toUpperCase() === search.toUpperCase() &&
      url.searchParams.get("limit") === limit &&
      url.searchParams.get("maxPoints") === "288",
  );
const expectReadOnly = (fixture: Fixture) => {
  expect(fixture.unexpected).toEqual([]);
  expect(fixture.writes).toEqual([]);
  expect(fixture.browserUBus).toEqual([]);
  // The gateway card now observes these exact passive snapshots, never query-driven probes.
  expect(
    fixture.reads.filter(
      (read) =>
        /^\/api\/proxy\/(?:capture|node-probes)(?:[/?]|$)/.test(read) &&
        read !== "/api/proxy/capture" &&
        read !== "/api/proxy/node-probes",
    ),
  ).toEqual([]);
  expect(
    fixture.reads.some((read) =>
      /\/api\/(?:proxy\/(?:request-traces|probe)(?:[/?]|$)|system\/services)/.test(
        read,
      ),
    ),
  ).toBe(false);
  // Check every ledger entry in one assertion; do not create hundreds of trace-only steps.
  expect(
    activityReads(fixture)
      .filter((url) => {
        const limit = Number(url.searchParams.get("limit"));
        const points = Number(url.searchParams.get("maxPoints"));
        return (
          !Number.isInteger(limit) ||
          limit < 1 ||
          limit > 64 ||
          !Number.isInteger(points) ||
          points < 1 ||
          points > 288 ||
          Array.from(url.searchParams.get("search") ?? "").length > 64
        );
      })
      .map((url) => url.pathname + url.search),
  ).toEqual([]);
  expect(
    fixture.activityResponses.filter(
      (response) =>
        response.deviceCount !== 128 ||
        response.returned > 64 ||
        response.points > 288,
    ),
  ).toEqual([]);
};
const expectBaseline = async (
  detail: Locator,
  title: string,
  deviceNumbers: readonly number[],
  fixture: Fixture,
) => {
  const trend = detail.getByRole("region", { name: title, exact: true });
  await expect(trend).toContainText("缺测与重置保持空隙，不补零");
  // The chart host must enter the viewport before its lazy EChart initializes.
  await trend.locator(".viz-canvas").scrollIntoViewIfNeeded();
  await expect(trend.locator("canvas")).toHaveCount(1);
  const table = trend.locator(".viz-table-details");
  const rows = table.locator("tbody tr");
  const total = 288 * deviceNumbers.length;
  const pages = Math.ceil(total / 100);
  await expect(table.locator("summary")).toContainText(`${total} 条`);
  await expect(rows).toHaveCount(0); // Closed heavy tables must not mount hidden history rows.
  await table.locator("summary").click();
  const pager = table.getByRole("navigation", {
    name: `${title} 数据表分页`,
    exact: true,
  });
  // A previous single-device/table journey may retain its page across same-component updates.
  for (
    let previous = 0;
    previous < pages &&
    (await pager
      .getByLabel(`${title} 数据表上一页`, { exact: true })
      .isEnabled());
    previous += 1
  )
    await pager.getByLabel(`${title} 数据表上一页`, { exact: true }).click();
  // Display values use decimal byte units, while rates in this fixture stay below 1 KB/s.
  // Keep this expected projection independent of the product formatter.
  const byteText = (bytes: number | null) =>
    bytes === null
      ? "—"
      : bytes < 1000
        ? `${bytes} B`
        : `${Number((bytes / 1000).toFixed(2))} KB`;
  const expected = deviceNumbers.flatMap((number) =>
    fixture.allDevices[number - 1].samples.map((sample) => [
      scaleDeviceName(number),
      scaleDeviceMAC(number),
      byteText(sample.rxBytes),
      byteText(sample.txBytes),
      String(sample.coverageSeconds),
      sample.rxBytes === null || sample.coverageSeconds === 0
        ? "—"
        : `${sample.rxBytes / sample.coverageSeconds} B/s`,
      sample.txBytes === null || sample.coverageSeconds === 0
        ? "—"
        : `${sample.txBytes / sample.coverageSeconds} B/s`,
    ]),
  );
  const observed: string[][] = [];
  for (let pageIndex = 0; pageIndex < pages; pageIndex += 1) {
    await expect(pager).toContainText(
      `第 ${pageIndex + 1} / ${pages} 页 · 共 ${total} 条`,
    );
    await expect(rows).toHaveCount(Math.min(100, total - pageIndex * 100));
    observed.push(
      ...(await rows.evaluateAll((elements) =>
        elements.map(
          (row) =>
            Array.from(
              row.querySelectorAll("th, td"),
              (cell) => cell.textContent?.trim() ?? "",
            ).filter((_, index) => index !== 2), // Localized timestamp presentation is not counter data.
        ),
      )),
    );
    if (pageIndex < pages - 1)
      await pager.getByLabel(`${title} 数据表下一页`, { exact: true }).click();
  }
  // Every displayed record is the source record. This includes all8 null first buckets.
  expect(observed).toEqual(expected);
  await expect(
    pager.getByLabel(`${title} 数据表下一页`, { exact: true }),
  ).toBeDisabled();
  await pager.getByLabel(`${title} 数据表上一页`, { exact: true }).click();
  await expect(rows).toHaveCount(100);
  await pager.getByLabel(`${title} 数据表末页`, { exact: true }).click();
  await expect(rows).toHaveCount(total - (pages - 1) * 100);
  await expect(rows.last()).toContainText(
    scaleDeviceMAC(deviceNumbers.at(-1)!),
  );
  await table.locator("summary").click();
  await expect(rows).toHaveCount(0);
};

// Synthetic scale acceptance, not live throughput/latency measurements. Three focused cases.
// API routes are intercepted at the complete configured origin, including candidate port5573.
test.describe("modern scale · 220 nodes / 128 retained trafficd identities", () => {
  test.setTimeout(35_000);
  test.beforeEach(async ({ page }) => {
    await page.emulateMedia({ reducedMotion: "reduce" });
  });

  test("220 public VLESS nodes: five bounded pages, favorites, label-country/search and unsaved selection", async ({
    page,
    baseURL,
  }) => {
    const fixture = await install(page, baseURL);
    await page.goto("/#/proxy");
    // The picker mounts only after the user opens its disclosure.
    await page
      .locator("summary")
      .filter({ hasText: /^更换节点$/ })
      .click();
    const selector = page.locator(".node-selector");
    const cards = selector.locator(".node-selector-card");
    const summary = selector.getByRole("region", {
      name: "节点选择与保存",
      exact: true,
    });
    const status = selector.locator(".node-selector-controls [role=status]");
    const pager = selector.getByRole("navigation", {
      name: "节点分页",
      exact: true,
    });
    const pageNumber = pager.getByRole("combobox");
    await expect(status).toContainText("220 个匹配 / 共 220 个节点");
    await expect(cards).toHaveCount(20);
    await expect(summary).toContainText("与当前配置一致");
    await expect(
      summary.getByRole("button", { name: "保存配置", exact: true }),
    ).toBeEnabled();
    await expect(
      selector.getByLabel(`选择节点 ${scaleNodeLabel(1)}`, { exact: true }),
    ).toHaveAttribute("aria-pressed", "true");
    // Actual page journeys retain truthful ranges and identities, not just arbitrary DOM counts.
    for (const pageIndex of [2, 3, 5, 11, 1]) {
      await pageNumber.selectOption(String(pageIndex));
      await expect(cards).toHaveCount(20);
      await expect(cards.first()).toContainText(
        scaleNodeLabel((pageIndex - 1) * 20 + 1),
      );
      await expect(cards.last()).toContainText(scaleNodeLabel(pageIndex * 20));
      await expect(status).toContainText(
        `本页 ${(pageIndex - 1) * 20 + 1}–${pageIndex * 20}`,
      );
      expect(await cards.count()).toBeLessThanOrEqual(20);
    }
    const regions = selector.getByRole("group", {
      name: "按名称地区标识筛选",
      exact: true,
    });
    await regions.getByRole("button", { name: "日本", exact: true }).click();
    await expect(status).toContainText("55 个匹配 / 共 220 个节点");
    await expect(cards).toHaveCount(20);
    for (const label of await cards
      .locator(".node-selector-name strong")
      .allTextContents())
      expect(label).toMatch(/^日本 /);
    await selector
      .getByRole("button", { name: "重置筛选", exact: true })
      .click();
    await selector.getByLabel("搜索节点", { exact: true }).fill("scale-220");
    await expect(cards).toHaveCount(1);
    await expect(status).toContainText("1 个匹配 / 共 220 个节点");
    await selector
      .getByLabel(`选择节点 ${scaleNodeLabel(220)}`, { exact: true })
      .click();
    await expect(summary).toContainText("已选择 · 尚未保存");
    await expect(
      summary.locator(".node-selector-node-summary").first(),
    ).toContainText(scaleNodeLabel(1));
    await expect(
      summary.locator(".node-selector-node-summary").last(),
    ).toContainText(scaleNodeLabel(220));
    const beforeStorage = await page.evaluate(() =>
      Object.fromEntries(Object.entries(localStorage)),
    );
    await selector
      .getByLabel(`收藏节点 ${scaleNodeLabel(220)}`, { exact: true })
      .click();
    await selector.getByLabel("仅收藏", { exact: true }).check();
    await selector
      .getByRole("button", { name: "清除搜索", exact: true })
      .click();
    await expect(cards).toHaveCount(1);
    await expect(cards.first()).toContainText(scaleNodeLabel(220));
    const afterStorage = await page.evaluate(() =>
      Object.fromEntries(Object.entries(localStorage)),
    );
    expect(
      JSON.parse(afterStorage["be6500panel.proxy.node-favorites"]),
    ).toEqual([scaleNodeID(220)]);
    for (const key of new Set([
      ...Object.keys(beforeStorage),
      ...Object.keys(afterStorage),
    ]))
      if (key !== "be6500panel.proxy.node-favorites")
        expect(afterStorage[key]).toEqual(beforeStorage[key]);
    await selector
      .getByRole("button", { name: "重置筛选", exact: true })
      .click();
    await expect(cards).toHaveCount(20);
    await expect(summary).toContainText(scaleNodeLabel(220));
    await summary
      .getByRole("button", { name: "定位已选节点", exact: true })
      .click();
    await expect(pageNumber).toHaveValue("11");
    await expect(cards).toHaveCount(20);
    await expect(
      selector.getByLabel(`选择节点 ${scaleNodeLabel(220)}`, { exact: true }),
    ).toHaveAttribute("aria-pressed", "true");
    expect(fixture.nodes.selectedNodeId).toBe(scaleNodeID(1));
    expectReadOnly(fixture);
  });

  test("128 source identities: bounded64 list/paging25 keeps selection; backend name/MAC search finds old device128 and exact detail", async ({
    page,
    baseURL,
  }) => {
    // Two complete 288-record paging journeys plus server-side name/MAC search.
    test.setTimeout(60_000);
    const fixture = await install(page, baseURL);
    await page.goto("/#/devices");
    const workspace = page.locator(".device-workspace");
    const inventory = workspace.locator(".device-inventory");
    const rows = inventory.locator(".device-list tbody tr");
    const search = workspace.getByLabel("搜索设备", { exact: true });
    await expect(rows).toHaveCount(25);
    await expect(
      inventory
        .locator(".device-source-note")
        .filter({ hasText: "流量列表当前显示" }),
    ).toContainText("64 / 128 个匹配设备");
    await expect(inventory.locator(".panel-header")).toContainText(
      "源统计 128",
    );
    await expect
      .poll(() =>
        fixture.activityResponses.some(
          (response) =>
            new URLSearchParams(response.query).get("limit") === "64" &&
            response.deviceCount === 128 &&
            response.matchedCount === 128 &&
            response.returned === 64,
        ),
      )
      .toBe(true);
    await inventory
      .getByLabel(`查看 ${scaleDeviceName(1)} 详情`, { exact: true })
      .click();
    const detail = page.getByRole("region", { name: "设备详情", exact: true });
    await expect(
      detail.getByRole("heading", { name: scaleDeviceName(1), exact: true }),
    ).toBeVisible();
    await expect
      .poll(() => hasActivityRead(fixture, scaleDeviceMAC(1), "1"))
      .toBe(true);
    await expectBaseline(detail, "设备流量趋势", [1], fixture);
    await workspace.getByLabel("设备下一页", { exact: true }).click();
    await expect(rows).toHaveCount(25);
    await expect(rows.first()).toContainText(scaleDeviceName(26));
    await expect(detail).toContainText(scaleDeviceMAC(1));
    await expect(workspace.locator(".device-pinned-selection")).toContainText(
      scaleDeviceName(1),
    );
    await workspace.getByLabel("设备下一页", { exact: true }).click();
    await expect(rows).toHaveCount(14);
    await expect(rows.last()).toContainText(scaleDeviceName(64));
    // Device128 is not in DHCP or the returned64. Main workspace search must reach the server.
    await search.fill(scaleDeviceName(128));
    await expect
      .poll(() => hasActivityRead(fixture, scaleDeviceName(128), "64"))
      .toBe(true);
    await expect(rows).toHaveCount(1);
    await expect(rows.first()).toContainText(scaleDeviceMAC(128));
    await expect(
      detail.getByRole("heading", { name: scaleDeviceName(1), exact: true }),
    ).toBeVisible();
    await search.fill(scaleDeviceMAC(128));
    await expect
      .poll(() => hasActivityRead(fixture, scaleDeviceMAC(128), "64"))
      .toBe(true);
    await expect(rows).toHaveCount(1);
    await expect(rows.first()).toContainText(scaleDeviceName(128));
    await inventory
      .getByLabel(`查看 ${scaleDeviceName(128)} 详情`, { exact: true })
      .click();
    await expect
      .poll(() => hasActivityRead(fixture, scaleDeviceMAC(128), "1"))
      .toBe(true);
    await expect(
      detail.getByRole("heading", { name: scaleDeviceName(128), exact: true }),
    ).toBeVisible();
    await expectBaseline(detail, "设备流量趋势", [128], fixture);
    await search.fill("");
    await expect(rows).toHaveCount(25);
    await expect(
      detail.getByRole("heading", { name: scaleDeviceName(128), exact: true }),
    ).toBeVisible();
    expectReadOnly(fixture);
  });

  test("128 source identities: paged 2-to-8 same-range comparison stays exact-MAC bounded and preserves null buckets", async ({
    page,
    baseURL,
  }) => {
    // This journey visits every row of 2x288 and 8x288 buckets twice.
    // Its per-case budget includes real 23-page user navigation, not API latency.
    test.setTimeout(90_000);
    const fixture = await install(page, baseURL);
    await page.goto("/#/devices");
    const workspace = page.locator(".device-workspace");
    const rows = workspace.locator(".device-list tbody tr");
    await expect(rows).toHaveCount(25);
    for (const number of [1, 2])
      await workspace
        .getByLabel(`对比 ${scaleDeviceName(number)}`, { exact: true })
        .check();
    await workspace
      .getByRole("button", { name: "对比设备", exact: true })
      .click();
    const compare = page.getByRole("region", {
      name: "设备对比详情",
      exact: true,
    });
    await expect(
      compare.getByRole("heading", { name: "设备对比 · 2 个", exact: true }),
    ).toBeVisible();
    for (const number of [1, 2])
      await expect
        .poll(() => hasActivityRead(fixture, scaleDeviceMAC(number), "1"))
        .toBe(true);
    await expectBaseline(compare, "设备流量对比", [1, 2], fixture);
    await workspace.getByLabel("设备下一页", { exact: true }).click();
    await expect(rows).toHaveCount(25);
    await expect(rows.first()).toContainText(scaleDeviceName(26));
    for (const number of [26, 27, 28, 29, 30, 31])
      await workspace
        .getByLabel(`对比 ${scaleDeviceName(number)}`, { exact: true })
        .check();
    await expect(workspace.locator(".device-compare-toolbar")).toContainText(
      "已选择 8 / 8 个设备",
    );
    await expect(
      workspace.getByLabel(`对比 ${scaleDeviceName(32)}`, { exact: true }),
    ).toBeDisabled();
    await expect(
      compare.getByRole("heading", { name: "设备对比 · 8 个", exact: true }),
    ).toBeVisible();
    for (const number of [1, 2, 26, 27, 28, 29, 30, 31])
      await expect
        .poll(() => hasActivityRead(fixture, scaleDeviceMAC(number), "1"))
        .toBe(true);
    // Source summary has one row per selected MAC, not a false128-row decoded response.
    await expect(
      compare.locator(".device-chart-stack > .table-scroll tbody tr"),
    ).toHaveCount(8);
    await workspace
      .getByLabel("搜索设备", { exact: true })
      .fill("scale-no-match");
    await expect(rows).toHaveCount(0);
    await expect(
      compare.getByRole("heading", { name: "设备对比 · 8 个", exact: true }),
    ).toBeVisible();
    await compare.getByRole("button", { name: "7 天", exact: true }).click();
    await expect
      .poll(
        () =>
          activityReads(fixture).filter(
            (url) =>
              url.searchParams.get("range") === "7d" &&
              url.searchParams.get("limit") === "1",
          ).length,
      )
      .toBeGreaterThanOrEqual(8);
    await expect(
      compare.locator(".device-chart-stack > .table-scroll tbody tr"),
    ).toHaveCount(8);
    await expectBaseline(
      compare,
      "设备流量对比",
      [1, 2, 26, 27, 28, 29, 30, 31],
      fixture,
    );
    const selectedQueries = activityReads(fixture).filter(
      (url) => url.searchParams.get("limit") === "1",
    );
    expect(
      new Set(selectedQueries.map((url) => url.searchParams.get("search")))
        .size,
    ).toBe(8);
    expectReadOnly(fixture);
  });
});
