import { chromium } from "@playwright/test";
import fs from "node:fs/promises";

const url = process.env.BE6500PANEL_URL;
const password = process.env.BE6500PANEL_PASSWORD;
if (!url || !password)
  throw new Error("BE6500PANEL_URL and BE6500PANEL_PASSWORD are required");
const browser = await chromium.launch({
  executablePath: process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE,
});
try {
  const context = await browser.newContext({
    viewport: { width: 1440, height: 1000 },
  });
  const page = await context.newPage();
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.goto(url);
  await page.getByLabel("访问密码").fill(password);
  await page.getByRole("button", { name: /登录/ }).click();
  await page.locator(".connection-live").waitFor();
  const status = await context.request.get(
    new URL("/api/system", url).toString(),
  );
  if (!status.ok()) throw new Error(`system status ${status.status()}`);
  const body = await status.json();
  if (body.mode !== "host" || body.arch !== "arm")
    throw new Error("Live ARM host observation expected");
  if (await page.locator("canvas").count())
    throw new Error("Host mode must not show demo charts");
  await fs.mkdir("test-results", { recursive: true });
  await page.screenshot({
    path: "test-results/live-router-overview.png",
    fullPage: true,
  });
  await page.getByRole("button", { name: "诊断", exact: true }).click();
  await page.locator(".logs-table tbody tr").first().waitFor();
  await page.screenshot({
    path: "test-results/live-router-logs.png",
    fullPage: true,
  });
  if (errors.length) throw new Error(errors.join("\n"));
  console.log(
    JSON.stringify({
      success: true,
      mode: body.mode,
      architecture: body.arch,
      cpuCount: body.cpuCount,
      screenshotDirectory: "test-results",
    }),
  );
} finally {
  await browser.close();
}
