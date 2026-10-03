import { test, expect } from "@playwright/test";
import { installMaturityFixture } from "./maturity-fixtures";

test("routine data refresh keeps the same chart canvas and visible content", async ({
  page,
  baseURL,
}) => {
  if (!baseURL) throw new Error("candidate baseURL required");
  const fixture = await installMaturityFixture(page, baseURL);
  await page.emulateMedia({ reducedMotion: "reduce" });
  await page.goto("/#/overview");
  const heatmap = page.locator('[data-widget-id="deviceActivity"]');
  const host = heatmap.locator(".viz-canvas").first();
  await host.scrollIntoViewIfNeeded();
  const canvas = host.locator("canvas");
  await expect(canvas).toHaveCount(1);
  await canvas.evaluate((node) => {
    (window as any).__stableCanvas = node;
  });
  for (let i = 0; i < 3; i++) {
    const before = fixture.reads.filter((path: string) =>
      path.startsWith("/api/devices/activity"),
    ).length;
    await heatmap
      .getByRole("button", { name: "刷新数据", exact: true })
      .click();
    await expect
      .poll(
        () =>
          fixture.reads.filter((path: string) =>
            path.startsWith("/api/devices/activity"),
          ).length,
      )
      .toBeGreaterThan(before);
    await expect(canvas).toHaveCount(1);
    expect(
      await canvas.evaluate((node) => node === (window as any).__stableCanvas),
    ).toBe(true);
    await expect(heatmap.getByText("图表加载中", { exact: true })).toHaveCount(
      0,
    );
    await expect(heatmap.getByText("刷新中", { exact: true })).toHaveCount(0);
  }
  expect(fixture.unexpected).toEqual([]);
  expect(fixture.writes).toEqual([]);
});
