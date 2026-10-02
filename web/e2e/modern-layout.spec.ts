import { test, expect, type Page } from "@playwright/test";

const withinViewport = async (page: Page) => {
  const dimensions = await page.evaluate(() => ({
    document: document.documentElement.scrollWidth,
    viewport: window.innerWidth,
  }));
  expect(dimensions.document).toBeLessThanOrEqual(dimensions.viewport + 1);
};

for (const theme of ["light", "dark"] as const) {
  test(`${theme}: desktop and mobile modules contain their own overflow`, async ({
    page,
  }) => {
    await page.addInitScript(
      (mode) => localStorage.setItem("be6500panel.theme", mode),
      theme,
    );
    await page.emulateMedia({ reducedMotion: "reduce" });
    for (const width of [1440, 768, 390]) {
      await page.setViewportSize({ width, height: 844 });
      for (const module of [
        "overview",
        "system",
        "network",
        "proxy",
        "frpc",
        "wifi",
        "dns",
      ]) {
        await page.goto(`/#/${module}`);
        await expect(page.getByRole("heading", { level: 1 })).toBeVisible();
        await expect(page.locator(".page-stack").first()).toBeVisible();
        await withinViewport(page);
      }
    }
  });

  test(`${theme}: text and status tokens retain accessible contrast`, async ({
    page,
  }) => {
    await page.addInitScript(
      (mode) => localStorage.setItem("be6500panel.theme", mode),
      theme,
    );
    await page.goto("/");
    await expect(page.locator("html")).toHaveAttribute("data-theme", theme);
    const contrast = await page.evaluate(() => {
      const styles = getComputedStyle(document.documentElement);
      const canvas = document.createElement("canvas");
      canvas.width = canvas.height = 1;
      const context = canvas.getContext("2d")!;
      const luminance = (token: string) => {
        context.clearRect(0, 0, 1, 1);
        context.fillStyle = styles.getPropertyValue(token).trim();
        context.fillRect(0, 0, 1, 1);
        const [r, g, b] = [...context.getImageData(0, 0, 1, 1).data].map(
          (value) => {
            const channel = value / 255;
            return channel <= 0.04045
              ? channel / 12.92
              : ((channel + 0.055) / 1.055) ** 2.4;
          },
        );
        return 0.2126 * r + 0.7152 * g + 0.0722 * b;
      };
      const ratio = (foreground: string, background: string) => {
        const a = luminance(foreground);
        const b = luminance(background);
        return (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05);
      };
      return {
        foreground: ratio("--foreground", "--card"),
        muted: ratio("--muted-foreground", "--background"),
        primary: ratio("--primary", "--card"),
        button: ratio("--primary-foreground", "--primary"),
        accent: ratio("--accent-foreground", "--accent"),
        success: ratio("--success", "--card"),
        warning: ratio("--warning", "--card"),
        destructive: ratio("--destructive", "--card"),
      };
    });
    for (const [token, ratio] of Object.entries(contrast)) {
      expect(ratio, `${theme} ${token} contrast`).toBeGreaterThanOrEqual(4.5);
    }
  });
}

test("390px proxy tabs scroll locally and keep every view reachable", async ({
  page,
}) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.emulateMedia({ reducedMotion: "reduce" });
  await page.goto("/#/proxy");
  const tabs = page.getByRole("tablist", { name: "代理视图" });
  await expect(tabs).toBeVisible();
  const dimensions = await tabs.evaluate((element) => ({
    client: element.clientWidth,
    scroll: element.scrollWidth,
    overflow: getComputedStyle(element).overflowX,
  }));
  expect(dimensions.scroll).toBeGreaterThan(dimensions.client);
  expect(dimensions.overflow).toBe("auto");
  const lastTab = page.getByRole("tab", { name: "计划预览" });
  await lastTab.focus();
  await page.keyboard.press("Enter");
  await expect(lastTab).toHaveAttribute("aria-selected", "true");
  await expect(
    page.getByRole("button", { name: "校验并生成计划" }),
  ).toBeVisible();
  await withinViewport(page);
  const focus = await lastTab.evaluate((element) => ({
    width: getComputedStyle(element).outlineWidth,
    style: getComputedStyle(element).outlineStyle,
    offset: getComputedStyle(element).outlineOffset,
  }));
  expect(focus.width).toBe("2px");
  expect(focus.style).toBe("solid");
  expect(focus.offset).toBe("-2px");
});

test("mobile form inputs and navigation have usable touch targets", async ({
  page,
}) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto("/#/frpc");
  const input = page.getByPlaceholder("frps.example.com");
  await expect(input).toBeVisible();
  const metrics = await input.evaluate((element) => ({
    height: element.getBoundingClientRect().height,
    font: getComputedStyle(element).fontSize,
  }));
  expect(metrics.height).toBeGreaterThanOrEqual(44);
  expect(metrics.font).toBe("16px");
  const menu = page.getByRole("button", { name: "打开导航" });
  expect((await menu.boundingBox())?.height).toBeGreaterThanOrEqual(44);
  await menu.click();
  const dialog = page.getByRole("dialog");
  await expect(dialog).toBeVisible();
  await dialog.locator('a[href="#/proxy"]').click();
  await expect(dialog).toHaveCount(0);
  await withinViewport(page);
});

test("short screens keep module navigation scrollable and footer reachable", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1440, height: 600 });
  await page.goto("/");
  const navigation = page.locator(".desktop-sidebar .sidebar-nav");
  await expect(navigation).toBeVisible();
  const size = await navigation.evaluate((element) => ({
    client: element.clientHeight,
    scroll: element.scrollHeight,
    overflow: getComputedStyle(element).overflowY,
  }));
  expect(size.scroll).toBeGreaterThan(size.client);
  expect(size.overflow).toBe("auto");
  await page.locator('.desktop-sidebar a[href="#/frpc"]').click();
  await expect(page.getByRole("heading", { level: 1 })).toHaveText("frpc");
  const footer = page.locator(".desktop-sidebar .sidebar-footer");
  expect(
    (await footer.boundingBox())!.y + (await footer.boundingBox())!.height,
  ).toBeLessThanOrEqual(600);
});

test("reduced motion stops the loading spinner", async ({ page }) => {
  await page.emulateMedia({ reducedMotion: "reduce" });
  await page.goto("/");
  await expect(page.getByRole("heading", { level: 1 })).toBeVisible();
  const motion = await page.evaluate(() => {
    const spinner = document.createElement("span");
    spinner.className = "spin";
    document.body.append(spinner);
    const styles = getComputedStyle(spinner);
    const result = {
      duration: styles.animationDuration,
      count: styles.animationIterationCount,
    };
    spinner.remove();
    return result;
  });
  expect(motion.duration).toBe("1e-05s");
  expect(motion.count).toBe("1");
});

test("long service mapping names wrap without pushing delete controls off-screen", async ({
  page,
}) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto("/#/frpc");
  const firstMapping = page.getByRole("region", {
    name: "服务映射 1",
    exact: true,
  });
  await expect(firstMapping).toBeVisible();
  await firstMapping
    .locator("input")
    .first()
    .fill("a-very-long-service-name-without-spaces-that-still-keeps-controls");
  for (let count = 0; count < 8; count += 1) {
    await page.getByRole("button", { name: "添加", exact: true }).click();
  }
  await expect(
    page.getByRole("region", { name: "服务映射 9", exact: true }),
  ).toBeVisible();
  await withinViewport(page);
  const remove = firstMapping.getByRole("button", { name: "删除映射 1" });
  const bounds = await remove.boundingBox();
  expect(bounds!.x + bounds!.width).toBeLessThanOrEqual(390);
  await page.getByRole("button", { name: "删除映射 9", exact: true }).click();
  await expect(
    page.getByRole("region", { name: "服务映射 9", exact: true }),
  ).toHaveCount(0);
});
