import { test, expect, type Page } from "@playwright/test";
import {
  fixtureAcceptedToml,
  fixtureMAC,
  fixtureName,
  fixtureRawBackup,
  fixtureToken,
  installMaturityFixture,
} from "./maturity-fixtures";

type Fixture = Awaited<ReturnType<typeof installMaturityFixture>>;
const apiFixture = (page: Page, baseURL?: string) => {
  if (!baseURL)
    throw new Error(
      "Maturity fixtures require the configured Playwright baseURL",
    );
  return installMaturityFixture(page, baseURL);
};
const navigate = async (page: Page, id: string) => {
  const desktop = page.locator(`.desktop-sidebar a[href="#/${id}"]`);
  if (await desktop.isVisible()) await desktop.click();
  else {
    await page.getByRole("button", { name: "打开导航" }).click();
    const navigation = page.getByRole("dialog");
    await navigation.locator(`a[href="#/${id}"]`).click();
    await expect(navigation).toHaveCount(0);
  }
  await expect(page).toHaveURL(new RegExp(`/#/${id}$`));
};
const withinViewport = async (page: Page) => {
  const size = await page.evaluate(() => ({
    width: document.documentElement.scrollWidth,
    viewport: innerWidth,
  }));
  expect(size.width).toBeLessThanOrEqual(size.viewport + 1);
};
/** Exact total write ledger, not merely a count of the expected endpoint. */
const expectWrites = (fixture: Fixture, paths: readonly string[] = []) => {
  expect(fixture.unexpected).toEqual([]);
  expect(fixture.writes.map(({ method, path }) => `${method} ${path}`)).toEqual(
    paths.map((path) => `POST ${path}`),
  );
};
const readCount = (fixture: Fixture, path: string) =>
  fixture.reads.filter((read) => read.split("?")[0] === path).length;

// These test host-mode UI with synthetic decoded source responses. They are not live measurements.
// No waiting for real probes, polling clocks, screenshots or approximate ECharts canvas clicks.
test.describe("maturity workflows · synthetic source fixture", () => {
  test.setTimeout(15_000);
  test.beforeEach(async ({ page }) => {
    await page.emulateMedia({ reducedMotion: "reduce" });
  });

  test("ordinary homepage registers live-wrapper heatmap and diagnostic controls; mounts and refreshes only GET", async ({
    page,
    baseURL,
  }) => {
    const fixture = await apiFixture(page, baseURL);
    await page.goto("/");
    const heatmap = page.locator('[data-widget-id="deviceActivity"]');
    const diagnostic = page.locator('[data-widget-id="networkDiagnostics"]');
    await expect(
      heatmap.getByRole("heading", { name: "设备活跃热力图", exact: true }),
    ).toBeVisible();
    // EChart intentionally initializes only when its chart host enters the viewport.
    await heatmap.locator(".viz-canvas").scrollIntoViewIfNeeded();
    await expect(heatmap.locator("canvas")).toHaveCount(1);
    await expect(
      diagnostic.getByRole("button", { name: "发起诊断测试", exact: true }),
    ).toBeEnabled();
    await expect(
      diagnostic.getByLabel("诊断目标", { exact: true }),
    ).toHaveValue("google204");
    expect(readCount(fixture, "/api/devices/activity")).toBeGreaterThan(0);
    expect(readCount(fixture, "/api/proxy/request-traces")).toBeGreaterThan(0);
    expectWrites(fixture);

    // Accessible table uses the same source buckets as the heatmap. Missing is not zero.
    const buckets = heatmap.getByRole("region", {
      name: "设备活跃热力图",
      exact: true,
    });
    await buckets.locator("summary").click();
    const rows = buckets.locator("tbody tr");
    await expect(rows).toHaveCount(4);
    await expect(rows.nth(0).locator("th, td").nth(3)).toHaveText("—");
    await expect(rows.nth(0).locator("th, td").nth(4)).toHaveText("—");
    await expect(rows.nth(0)).toContainText("未采样");
    await expect(rows.nth(1).locator("th, td").nth(3)).toHaveText("1200");
    await expect(rows.nth(1)).toContainText("部分采样");
    await heatmap.getByLabel("字节方向", { exact: true }).selectOption("rx");
    const before = readCount(fixture, "/api/proxy/request-traces");
    await diagnostic
      .getByRole("button", { name: "刷新诊断记录", exact: true })
      .click();
    await expect
      .poll(() => readCount(fixture, "/api/proxy/request-traces"))
      .toBeGreaterThan(before);
    await expect(
      diagnostic.getByRole("button", { name: "发起诊断测试", exact: true }),
    ).toBeEnabled();
    expectWrites(fixture);
  });

  test("mobile table-to-MAC detail and keyboard annotation save read back aliases across homepage and devices", async ({
    page,
    baseURL,
  }) => {
    const fixture = await apiFixture(page, baseURL);
    await page.setViewportSize({ width: 320, height: 844 });
    await page.goto("/");
    const heatmap = page.locator('[data-widget-id="deviceActivity"]');
    await expect(
      heatmap.getByRole("button", {
        name: `查看设备 ${fixtureName.laptop}`,
        exact: true,
      }),
    ).toBeVisible();
    await withinViewport(page);
    // Stable accessible equivalent of selecting a heatmap cell; unit tests cover cell tuple mapping.
    await heatmap
      .getByRole("button", {
        name: `查看设备 ${fixtureName.laptop}`,
        exact: true,
      })
      .click();
    await expect(page).toHaveURL(/#\/devices$/);
    const detail = page.getByRole("region", { name: "设备详情", exact: true });
    await expect(detail).toContainText(fixtureMAC.laptop);
    await expect(
      detail.getByRole("heading", { name: fixtureName.laptop, exact: true }),
    ).toBeVisible();
    await withinViewport(page);
    await page.setViewportSize({ width: 390, height: 844 });
    await withinViewport(page);
    await detail
      .getByRole("button", { name: "编辑设备备注", exact: true })
      .focus();
    await page.keyboard.press("Enter");
    const modal = page.getByRole("dialog", {
      name: "编辑设备备注",
      exact: true,
    });
    await expect(
      modal.getByLabel("设备备注名称", { exact: true }),
    ).toBeFocused();
    await page.setViewportSize({ width: 320, height: 844 });
    await withinViewport(page);
    await page.setViewportSize({ width: 390, height: 844 });
    await withinViewport(page);
    await modal
      .getByLabel("设备备注名称", { exact: true })
      .fill("fixture 客厅电脑");
    await modal
      .getByLabel("详细备注", { exact: true })
      .fill("fixture-only keyboard edited note");
    await modal.getByLabel("设备标签", { exact: true }).fill("fixture, 办公");
    expectWrites(fixture);
    fixture.armWrite("/api/devices/annotations");
    await modal.getByRole("button", { name: "保存备注", exact: true }).focus();
    await page.keyboard.press("Enter");
    await expect(modal).toHaveCount(0);
    await expect(
      detail.getByRole("heading", { name: "fixture 客厅电脑", exact: true }),
    ).toBeVisible();
    expect(fixture.writes[0]?.body).toEqual({
      mac: fixtureMAC.laptop,
      label: "fixture 客厅电脑",
      note: "fixture-only keyboard edited note",
      tags: ["fixture", "办公"],
      expectedRevision: 3,
    });
    await navigate(page, "overview");
    await expect(
      page.locator('[data-widget-id="deviceActivity"]').getByRole("button", {
        name: "查看设备 fixture 客厅电脑",
        exact: true,
      }),
    ).toBeVisible();
    await navigate(page, "devices");
    await expect(
      page
        .locator(".device-list")
        .getByRole("button", { name: "fixture 客厅电脑", exact: true }),
    ).toBeVisible();
    // A full reload proves aliases are server readback, not only retained React state.
    const annotationReads = readCount(fixture, "/api/devices/annotations");
    await page.reload();
    await expect(
      page
        .locator(".device-list")
        .getByRole("button", { name: "fixture 客厅电脑", exact: true }),
    ).toBeVisible();
    expect(readCount(fixture, "/api/devices/annotations")).toBeGreaterThan(
      annotationReads,
    );
    await withinViewport(page);
    expectWrites(fixture, ["/api/devices/annotations"]);
  });

  test("two-MAC comparison keeps current-IP conflicts separate and does not attribute the shared core connection", async ({
    page,
    baseURL,
  }) => {
    const fixture = await apiFixture(page, baseURL);
    fixture.setConflicting();
    await page.goto("/#/devices");
    const inventory = page.locator(".device-list");
    await expect(inventory.locator("tbody tr")).toHaveCount(2);
    await expect(inventory.locator("tbody")).toContainText(fixtureMAC.laptop);
    await expect(inventory.locator("tbody")).toContainText(fixtureMAC.tv);
    await inventory
      .getByRole("checkbox", {
        name: `对比 ${fixtureName.laptop}`,
        exact: true,
      })
      .check();
    await inventory
      .getByRole("checkbox", { name: `对比 ${fixtureName.tv}`, exact: true })
      .check();
    await page.getByRole("button", { name: "对比设备", exact: true }).click();
    const comparison = page.getByRole("region", {
      name: "设备对比详情",
      exact: true,
    });
    await expect(
      comparison.getByRole("heading", { name: "设备对比 · 2 个", exact: true }),
    ).toBeVisible();
    const summary = comparison.getByRole("table", {
      name: "所选设备 · 同一时间范围数据汇总",
      exact: true,
    });
    await expect(summary.locator("tbody tr")).toHaveCount(2);
    await expect(summary.locator("tbody")).toContainText(fixtureMAC.laptop);
    await expect(summary.locator("tbody")).toContainText(fixtureMAC.tv);
    await expect(
      comparison.getByText("1 个地址归属不唯一的核心连接未关联", {
        exact: true,
      }),
    ).toBeVisible();
    await expect(
      comparison.getByText("0 个匹配的已观测连接", { exact: true }),
    ).toBeVisible();
    await expect(
      comparison.getByText("fixture-origin.example.test", { exact: true }),
    ).toHaveCount(0);
    await comparison
      .getByRole("button", { name: "30 分钟", exact: true })
      .click();
    await expect(comparison).toContainText("120 秒桶");
    await expect
      .poll(
        () =>
          fixture.reads.filter(
            (path) =>
              path.startsWith("/api/devices/activity?") &&
              new URL(path, "http://fixture.test").searchParams.get("range") ===
                "30m",
          ).length,
      )
      .toBeGreaterThan(0);
    expectWrites(fixture);
  });

  test("explicit diagnostic runs one POST; proxy DNS and CONNECT remain null and failed TLS stays partial", async ({
    page,
    baseURL,
  }) => {
    const fixture = await apiFixture(page, baseURL);
    await page.goto("/#/proxy");
    await page.getByRole("tab", { name: "网络诊断", exact: true }).click();
    const run = page.getByRole("button", { name: "发起诊断测试", exact: true });
    await expect(run).toBeEnabled();
    await page.getByLabel("诊断链路", { exact: true }).selectOption("proxy");
    expectWrites(fixture);
    fixture.armWrite("/api/proxy/request-traces");
    await run.click();
    await expect(run).toBeEnabled();
    await page
      .locator("summary")
      .filter({ hasText: "查看详细阶段耗时" })
      .click();
    const table = page.getByRole("table", {
      name: "网络诊断瀑布图 · 主动诊断记录",
      exact: true,
    });
    await expect(table.locator("tbody tr")).toHaveCount(6);
    const dns = table.locator("tbody tr").filter({
      has: page.getByRole("cell", { name: "DNS 解析", exact: true }),
    });
    const connect = table.locator("tbody tr").filter({
      has: page.getByRole("cell", { name: "CONNECT 隧道", exact: true }),
    });
    const tls = table.locator("tbody tr").filter({
      has: page.getByRole("cell", { name: "目标服务 TLS 握手", exact: true }),
    });
    for (const row of [dns, connect]) {
      await expect(row.locator("td").nth(3)).toHaveText("未观测");
      for (const column of [4, 5, 6])
        await expect(row.locator("td").nth(column)).toHaveText("未知");
    }
    await expect(dns).toContainText("代理侧目标 DNS 未暴露，本地不可观测");
    await expect(connect).toContainText("HTTP 追踪未提供 CONNECT 阶段时间");
    await expect(tls.locator("td").nth(3)).toHaveText("已开始，未完成");
    await expect(tls.locator("td").nth(4)).toHaveText("25");
    await expect(tls.locator("td").nth(5)).toHaveText("未知");
    await expect(tls.locator("td").nth(6)).toHaveText("未知");
    await expect(tls).toContainText("fixture_tls_failure");
    expect(fixture.writes[0]?.body).toEqual({
      targetId: "google204",
      route: "proxy",
    });
    expect(
      fixture.failedTrace.phases
        .filter((phase) => !phase.observed)
        .every((phase) => phase.durationMs === null),
    ).toBe(true);
    await page.getByLabel("筛选结果", { exact: true }).selectOption("failed");
    await page
      .getByRole("button", { name: "刷新诊断记录", exact: true })
      .click();
    await expect(run).toBeEnabled();
    expectWrites(fixture, ["/api/proxy/request-traces"]);
  });

  test("global pending recovery survives page navigation; explicit confirm and restore each send one POST", async ({
    page,
    baseURL,
  }) => {
    const fixture = await apiFixture(page, baseURL);
    fixture.setPending("fixture-confirm", 8);
    await page.goto("/");
    const banner = page.getByRole("region", {
      name: "全局配置恢复",
      exact: true,
    });
    await expect(banner).toHaveAttribute("data-phase", "pending");
    await expect(
      banner.getByRole("timer", { name: "自动恢复剩余时间", exact: true }),
    ).toContainText(/\d+s/);
    await navigate(page, "devices");
    await expect(banner).toHaveAttribute("data-phase", "pending");
    await navigate(page, "frpc");
    await expect(banner).toHaveAttribute("data-phase", "pending");
    expectWrites(fixture);
    fixture.armWrite("/api/configuration/confirm");
    await banner.getByRole("button", { name: "确认生效", exact: true }).click();
    await expect(banner).toContainText("更改已生效");
    expect(fixture.writes[0]?.body).toEqual({ id: "fixture-confirm" });
    await navigate(page, "system");
    await expect(banner).toContainText("更改已生效");
    fixture.setPending("fixture-restore", 9);
    await banner
      .getByRole("button", { name: "刷新全局配置状态", exact: true })
      .click();
    await expect(banner).toHaveAttribute("data-phase", "pending");
    expectWrites(fixture, ["/api/configuration/confirm"]);
    fixture.armWrite("/api/configuration/rollback");
    await banner
      .getByRole("button", { name: "恢复上一配置", exact: true })
      .click();
    await expect(banner).toContainText("已恢复上一配置");
    expect(fixture.writes[1]?.body).toEqual({ id: "fixture-restore" });
    await expect(
      banner.getByRole("button", { name: "确认生效", exact: true }),
    ).toHaveCount(0);
    expectWrites(fixture, [
      "/api/configuration/confirm",
      "/api/configuration/rollback",
    ]);
  });

  test("protected rescue remains observation-only; dnsmasq requires impact confirmation before one POST", async ({
    page,
    baseURL,
  }) => {
    const fixture = await apiFixture(page, baseURL);
    await page.goto("/#/system");
    await page.getByRole("tab", { name: "服务管理", exact: true }).click();
    const rescue = page.getByRole("article", {
      name: "be6500-rescue · fixture-rescue",
      exact: true,
    });
    await expect(rescue).toContainText("独立救援通道 · 受保护 · 仅观察");
    await expect(rescue.getByRole("button")).toHaveCount(0);
    await page
      .getByRole("button", { name: "重载 dnsmasq", exact: true })
      .click();
    const dialog = page.getByRole("dialog", {
      name: "确认重载 dnsmasq",
      exact: true,
    });
    const confirm = dialog.getByRole("button", {
      name: "确认重载 dnsmasq",
      exact: true,
    });
    await expect(confirm).toBeDisabled();
    await expect(dialog).toContainText("fixture: DNS/DHCP can be interrupted");
    expectWrites(fixture);
    await dialog
      .getByRole("checkbox", {
        name: "我确认本次操作可能短暂中断 DNS/DHCP 服务",
        exact: true,
      })
      .check();
    await expect(confirm).toBeEnabled();
    fixture.armWrite("/api/system/services/action");
    await confirm.click();
    await expect(dialog).toHaveCount(0);
    await expect(
      page.getByText("命令已接受，不代表服务运行或健康；以下以实际观察为准。", {
        exact: true,
      }),
    ).toBeVisible();
    expect(fixture.writes[0]?.body).toEqual({
      service: "dnsmasq",
      action: "reload",
      confirmImpact: true,
    });
    await expect(rescue.getByRole("button")).toHaveCount(0);
    expectWrites(fixture, ["/api/system/services/action"]);
  });

  test("backup uploads original raw bytes, previews and stages once, then opens the real queue without Apply", async ({
    page,
    baseURL,
  }) => {
    const fixture = await apiFixture(page, baseURL);
    await page.setViewportSize({ width: 390, height: 844 });
    await page.goto("/#/system");
    await page.getByRole("tab", { name: "备份与导入", exact: true }).click();
    const upload = page.getByLabel("选择 JSON 备份文件", { exact: true });
    await expect(upload).toBeAttached();
    expectWrites(fixture);
    fixture.armWrite("/api/maintenance/import/preview");
    await upload.setInputFiles({
      name: "fixture-only-backup.json",
      mimeType: "application/json",
      buffer: Buffer.from(fixtureRawBackup, "utf8"),
    });
    const preview = page.getByRole("dialog", {
      name: "导入配置差异预览",
      exact: true,
    });
    await expect(preview).toBeVisible();
    await expect(
      preview.getByRole("checkbox", { name: "暂存网络", exact: true }),
    ).toBeChecked();
    await withinViewport(page);
    expect(fixture.writes[0]?.raw).toBe(fixtureRawBackup);
    expectWrites(fixture, ["/api/maintenance/import/preview"]);
    fixture.armWrite("/api/maintenance/import/stage");
    await preview
      .getByRole("button", { name: "暂存为草稿 (稍后应用) (1)", exact: true })
      .click();
    await expect(preview).toHaveCount(0);
    await expect(
      page.getByRole("status", { name: "导入暂存结果", exact: true }),
    ).toContainText("已暂存 1 个配置草稿，尚未应用。");
    expect(fixture.writes[1]?.body).toEqual({
      previewId: "fixture-preview",
      generation: 7,
      modules: ["network"],
      acknowledgeModelMismatch: false,
    });
    await page
      .getByRole("button", { name: "前往配置队列", exact: true })
      .click();
    await expect(
      page.getByRole("tab", { name: "配置变更", exact: true }),
    ).toHaveAttribute("aria-selected", "true");
    const queue = page.getByRole("table", {
      name: "配置检查结果",
      exact: true,
    });
    await expect(queue.locator("tbody tr")).toHaveCount(1);
    await expect(
      queue.getByRole("checkbox", { name: "选择网络草稿 1", exact: true }),
    ).toBeChecked();
    await expect(
      page.getByRole("button", { name: "应用已选更改 (1)", exact: true }),
    ).toBeEnabled();
    expect(readCount(fixture, "/api/configuration/drafts")).toBeGreaterThan(0);
    await withinViewport(page);
    expectWrites(fixture, [
      "/api/maintenance/import/preview",
      "/api/maintenance/import/stage",
    ]);
  });

  test("accepted FRPC TOML preserves token and unknown native options; dirty form survives navigation until explicit Save", async ({
    page,
    baseURL,
  }) => {
    const fixture = await apiFixture(page, baseURL);
    await page.setViewportSize({ width: 390, height: 844 });
    await page.goto("/#/frpc");
    const server = page.getByRole("textbox", {
      name: "服务器地址",
      exact: true,
    });
    await expect(server).toHaveValue("fixture-frps.example.test");
    await expect(
      page.getByRole("spinbutton", { name: "服务器端口", exact: true }),
    ).toHaveValue("7443");
    await expect(
      page.getByRole("combobox", { name: "传输协议", exact: true }),
    ).toContainText("QUIC");
    await expect(
      page.getByRole("checkbox", { name: /启用 TLS/ }),
    ).not.toBeChecked();
    await expect(
      page
        .getByRole("region", { name: "服务映射 1", exact: true })
        .getByRole("spinbutton", { name: "远程端口", exact: true }),
    ).toHaveValue("0");
    const token = page.getByLabel("认证令牌", { exact: true });
    await expect(token).toHaveValue("");
    await expect(token).toHaveAttribute(
      "placeholder",
      "已配置密钥（留空保持不变）",
    );
    await expect(
      page.getByRole("radio", { name: "保留已保存密钥", exact: true }),
    ).toBeChecked();
    await expect(
      page.getByLabel("frpc TOML 预览", { exact: true }),
    ).not.toContainText(fixtureToken);
    await withinViewport(page);
    await page
      .getByRole("spinbutton", { name: "服务器端口", exact: true })
      .fill("7001");
    await expect(
      page.getByText("未保存更改已在本次会话中保留", { exact: true }),
    ).toBeVisible();
    await page.getByRole("tab", { name: "原生配置", exact: true }).click();
    await page.getByRole("button", { name: "载入配置", exact: true }).click();
    await expect(page.getByLabel("原生配置内容", { exact: true })).toHaveValue(
      fixtureAcceptedToml,
    );
    await page.getByRole("tab", { name: "连接与映射", exact: true }).click();
    await expect(
      page.getByRole("spinbutton", { name: "服务器端口", exact: true }),
    ).toHaveValue("7001");
    await navigate(page, "devices");
    await navigate(page, "frpc");
    await expect(
      page.getByRole("spinbutton", { name: "服务器端口", exact: true }),
    ).toHaveValue("7001");
    await expect(
      page.getByText("未保存更改已在本次会话中保留", { exact: true }),
    ).toBeVisible();
    expectWrites(fixture);
    const readsBeforeSave = readCount(fixture, "/api/runtime/config");
    const save = page.getByRole("button", {
      name: "校验并保存 frpc 配置",
      exact: true,
    });
    await expect(save).toBeEnabled();
    fixture.armWrite("/api/runtime/configure");
    await save.click();
    await expect(
      page.getByText(
        "配置已保存。密钥输入已清空；再次编辑默认保留已保存密钥和未修改选项。",
        { exact: true },
      ),
    ).toBeVisible();
    const expectedToml = fixtureAcceptedToml.replace(
      "serverPort = 7443",
      "serverPort = 7001",
    );
    expect(fixture.writes[0]?.body).toEqual({
      service: "frpc",
      generation: 7,
      config: expectedToml,
    });
    expect(readCount(fixture, "/api/runtime/config")).toBeGreaterThan(
      readsBeforeSave,
    );
    await expect(page.getByLabel("认证令牌", { exact: true })).toHaveValue("");
    await navigate(page, "overview");
    await navigate(page, "frpc");
    await expect(
      page.getByRole("spinbutton", { name: "服务器端口", exact: true }),
    ).toHaveValue("7001");
    await expect(
      page.getByLabel("frpc TOML 预览", { exact: true }),
    ).toContainText('healthCheck.path = "/fixture-ready"');
    await expect(
      page.getByLabel("frpc TOML 预览", { exact: true }),
    ).not.toContainText(fixtureToken);
    await withinViewport(page);
    expectWrites(fixture, ["/api/runtime/configure"]);
  });
});
