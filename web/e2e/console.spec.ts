import { test, expect } from "@playwright/test";

const navigate = async (page: import("@playwright/test").Page, id: string) => {
  await page.locator(`.desktop-sidebar a[href="#/${id}"]`).click();
  await expect(page).toHaveURL(new RegExp(`/#/${id}$`));
};

test("console observations, charts, command palette and split plan", async ({
  page,
}) => {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.goto("/");
  await expect(page.getByRole("heading", { level: 1 })).toBeVisible();
  await expect(page.getByText("Demo", { exact: true }).first()).toBeVisible();
  await expect(page.locator(".connection-live")).toBeVisible();
  await expect(page.locator("canvas").first()).toBeVisible();
  await page.getByRole("button", { name: "切换主题" }).click();
  await page.getByRole("menuitem", { name: "深色" }).click();
  await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
  await page.screenshot({
    path: "test-results/overview-dark.png",
    fullPage: true,
  });
  await page.getByRole("button", { name: "切换主题" }).click();
  await page.getByRole("menuitem", { name: "浅色" }).click();
  await expect(page.locator("html")).toHaveAttribute("data-theme", "light");
  await page.screenshot({
    path: "test-results/overview-light.png",
    fullPage: true,
  });

  await page.keyboard.press("ControlOrMeta+k");
  await expect(page.getByRole("dialog")).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(page.getByRole("dialog")).toHaveCount(0);

  await navigate(page, "network");
  await expect(page.locator("table tbody tr").first()).toBeVisible();

  await navigate(page, "proxy");
  await page.getByRole("tab", { name: "计划预览" }).click();
  const proxyResponse = page.waitForResponse(
    (response) =>
      response.url().endsWith("/api/proxy/plan") &&
      response.request().method() === "POST",
  );
  await page.getByRole("button", { name: "校验并生成计划" }).click();
  const response = await proxyResponse;
  expect(response.status()).toBe(200);
  const plan = await response.json();
  expect(plan.readOnly).toBe(true);
  expect(plan.canApply).toBe(false);
  expect(plan.steps.length).toBeGreaterThan(0);
  await expect(page.getByText(plan.summary, { exact: true })).toBeVisible();
  await page.getByRole("tab", { name: "诊断", exact: true }).click();
  await expect(page.getByText("订阅诊断", { exact: true })).toBeVisible();
  await page.screenshot({
    path: "test-results/proxy-diagnostics.png",
    fullPage: true,
  });
  expect(errors).toEqual([]);
});

test("FRPC keeps unconfigured runtime disabled in demo", async ({ page }) => {
  await page.goto("/#/frpc");
  await expect(page.getByRole("heading", { level: 1 })).toBeVisible();
  await expect(
    page.getByRole("button", { name: "校验并保存 frpc 配置" }),
  ).toBeDisabled();
  await expect(page.getByPlaceholder("frps.example.com")).toBeVisible();
  await page.getByRole("tab", { name: "原生配置" }).click();
  await expect(
    page.getByText("运行管理未启用", { exact: false }).first(),
  ).toBeVisible();
});

test("mobile navigation and layout stay within viewport", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto("/");
  await page.getByRole("button", { name: "打开导航" }).click();
  await expect(page.getByRole("dialog")).toBeVisible();
  await page.getByRole("dialog").locator('a[href="#/system"]').click();
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await expect(page).toHaveURL(/#\/system$/);
  const width = await page.evaluate(() => ({
    content: document.documentElement.scrollWidth,
    viewport: window.innerWidth,
  }));
  expect(width.content).toBeLessThanOrEqual(width.viewport + 1);
  await page.screenshot({
    path: "test-results/mobile-system.png",
    fullPage: true,
  });
});

test("diagnostics shows bounded structured logs and filters by code", async ({
  page,
}) => {
  await page.goto("/");
  await page.getByRole("button", { name: "诊断", exact: true }).click();
  await expect(page).toHaveURL(/#\/system\?tab=diagnostics$/);
  await expect(page.getByRole("tab", { name: "诊断与日志" })).toHaveAttribute(
    "aria-selected",
    "true",
  );
  await expect(page.locator(".logs-table tbody tr").first()).toBeVisible();
  await page.getByLabel("筛选日志", { exact: true }).fill("server_started");
  await expect(page.locator(".logs-table tbody tr")).toHaveCount(1);
  await expect(page.locator(".logs-table")).toContainText("server_started");
  await page.screenshot({
    path: "test-results/diagnostics.png",
    fullPage: true,
  });
});
