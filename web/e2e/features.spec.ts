import { test, expect, type Page } from "@playwright/test";
import { installFeaturesFixture } from "./features-fixtures";

test.describe("feature management workflows · e2e", () => {
  test.setTimeout(25_000);

  test("1. navigates to network feature tab and renders domain panel", async ({ page, baseURL }) => {
    await installFeaturesFixture(page, baseURL!);
    await page.goto("/#/network");

    // Click 功能设置 tab
    await page.getByRole("tab", { name: "功能设置" }).click();
    await expect(page.getByRole("tablist", { name: /网络.*功能分组/ })).toBeVisible();
    await expect(page.getByRole("tab", { name: "WAN IPv4 / 上游 DNS" })).toBeVisible();
    await expect(page.getByRole("heading", { name: "WAN IPv4 / 上游 DNS", exact: true })).toBeVisible();
  });

  test("2. requires getter parameters and does not fetch until parameters are filled (port_service required service)", async ({ page, baseURL }) => {
    const fixture = await installFeaturesFixture(page, baseURL!);
    await page.goto("/#/network");
    await page.getByRole("tab", { name: "功能设置" }).click();

    // Switch to 端口角色 / IPTV / VLAN / LAG (requires service parameter)
    await page.getByRole("tab", { name: "端口角色 / IPTV / VLAN / LAG" }).click();

    // Verify 读取数据 is disabled before required parameter is selected
    const fetchBtn = page.getByRole("button", { name: "读取数据" });
    await expect(fetchBtn).toBeDisabled();

    // Select required parameter 端口服务 -> iptv
    await page.getByLabel("端口服务").selectOption("iptv");
    await expect(fetchBtn).toBeEnabled();

    // Click fetch
    await fetchBtn.click();
    await expect(page.getByText("已读取")).toBeVisible();
    await expect(page.getByText("100")).toBeVisible();

    const serviceReads = fixture.featureReads.filter((r) => r.includes("port_service"));
    expect(serviceReads.length).toBeGreaterThanOrEqual(1);
    expect(serviceReads[0]).toContain("service=iptv");
  });

  test("3. pre-fills action form from real nested LAN info (lan_info -> set_lan_ip)", async ({ page, baseURL }) => {
    await installFeaturesFixture(page, baseURL!);
    await page.goto("/#/network");
    await page.getByRole("tab", { name: "功能设置" }).click();

    // Switch to LAN 地址与链路
    await page.getByRole("tab", { name: "LAN 地址与链路" }).click();
    await expect(page.getByText("已读取")).toBeVisible();

    // Form field for LAN IPv4 and LAN 掩码 should be pre-filled from info.ipv4[0]
    const ipInput = page.getByLabel("LAN IPv4");
    await expect(ipInput).toHaveValue("192.168.31.1");
    const maskInput = page.getByLabel("LAN 掩码");
    await expect(maskInput).toHaveValue("255.255.255.0");
  });

  test("4. applies local impact actions immediately without impact confirmation popup (led_set)", async ({ page, baseURL }) => {
    const fixture = await installFeaturesFixture(page, baseURL!);
    await page.goto("/#/system");

    // Click 高级服务 tab
    await page.getByRole("tab", { name: "高级服务" }).click();
    await expect(page.getByRole("tab", { name: "状态灯与时段" })).toBeVisible();
    await page.getByRole("tab", { name: "状态灯与时段" }).click();

    await expect(page.getByText("已读取")).toBeVisible();

    // led_set action: change on to 0
    const onInput = page.getByLabel("指示灯开关");
    await onInput.fill("0");

    // Click 应用设置
    await page.getByRole("button", { name: "应用设置" }).click();

    // Local impact -> NO confirmation dialog
    await expect(page.getByText("确认执行操作？")).toHaveCount(0);
    await expect(page.getByText("配置已成功应用并生效")).toBeVisible();

    expect(fixture.featureWrites).toHaveLength(1);
    expect(fixture.featureWrites[0].body.actionId).toBe("led_set");
  });

  test("5. prompts single confirmation dialog for maintenance/network impact and sends acknowledgeImpact (set_lan_ip)", async ({ page, baseURL }) => {
    const fixture = await installFeaturesFixture(page, baseURL!);
    await page.goto("/#/network");
    await page.getByRole("tab", { name: "功能设置" }).click();
    await page.getByRole("tab", { name: "LAN 地址与链路" }).click();

    await expect(page.getByText("已读取")).toBeVisible();

    const maskInput = page.getByLabel("LAN 掩码");
    await maskInput.fill("255.255.0.0");

    // Click 应用设置
    await page.getByRole("button", { name: "应用设置" }).click();

    // Should show maintenance impact confirmation dialog
    const dialog = page.getByRole("dialog");
    await expect(dialog).toBeVisible();
    await expect(dialog.getByText("确认执行操作？")).toBeVisible();
    await expect(dialog.getByText(/重启后台服务或相关设备/)).toBeVisible();

    // Confirm
    await dialog.getByRole("button", { name: "确认继续" }).click();
    await expect(page.getByText("配置已成功应用并生效")).toBeVisible();

    expect(fixture.featureWrites).toHaveLength(1);
    expect(fixture.featureWrites[0].body.acknowledgeImpact).toBe(true);
    expect(fixture.featureWrites[0].body.generation).toBe(10);
  });

  test("6. handles pending poll until completed with success notification (set_wan)", async ({ page, baseURL }) => {
    const fixture = await installFeaturesFixture(page, baseURL!);
    await page.goto("/#/network");
    await page.getByRole("tab", { name: "功能设置" }).click();

    // WAN IPv4 / 上游 DNS (no required params, auto-loads)
    await expect(page.getByText("已读取")).toBeVisible();

    const wanForm = page
      .locator("form")
      .filter({ has: page.getByRole("heading", { name: "设置 WAN IPv4 / DNS", exact: true }) });
    await expect(wanForm).toBeVisible();

    // Select IPv4 连接方式 -> pppoe
    await wanForm.getByLabel("IPv4 连接方式").selectOption("pppoe");

    await wanForm.getByRole("button", { name: "应用设置" }).click();
    // Network impact confirm
    await page.getByRole("button", { name: "确认继续" }).click();

    // Polling resolves to completed
    await expect(page.getByText("配置已成功应用并生效")).toBeVisible();

    const pollRequests = fixture.featureReads.filter((r) => r.includes("/operations?id="));
    expect(pollRequests.length).toBeGreaterThanOrEqual(1);
  });

  test("7. renders connection confirmation banner with reconnect address and confirms on demand (set_lan_ip + canConfirm)", async ({ page, baseURL }) => {
    const fixture = await installFeaturesFixture(page, baseURL!);
    fixture.setShouldReturnCanConfirm(true);

    await page.goto("/#/network");
    await page.getByRole("tab", { name: "功能设置" }).click();
    await page.getByRole("tab", { name: "LAN 地址与链路" }).click();
    await expect(page.getByText("已读取")).toBeVisible();

    // Change LAN IP
    await page.getByLabel("LAN IPv4").fill("192.168.50.1");
    await page.getByRole("button", { name: "应用设置" }).click();
    await page.getByRole("button", { name: "确认继续" }).click();

    // Connection confirmation banner must appear with valid full URL retaining port
    const confirmRegion = page.getByRole("region", { name: /连接/ }).first();
    await expect(confirmRegion).toBeVisible();
    await expect(confirmRegion.getByText("配置已应用，等待连接确认")).toBeVisible();
    const reconnectLink = confirmRegion.getByRole("link", { name: /192\.168\.50\.1/ });
    await expect(reconnectLink).toBeVisible();
    await expect(reconnectLink).toHaveAttribute("href", /^http:\/\/192\.168\.50\.1(:\d+)?/);

    // Test page reload recovery from catalog.pendingOperation
    await page.reload();
    await page.getByRole("tab", { name: "功能设置" }).click();

    const recoveredRegion = page.getByRole("region", { name: /连接/ }).first();
    await expect(recoveredRegion).toBeVisible();
    await expect(recoveredRegion.getByText("配置已应用，等待连接确认")).toBeVisible();
    const recoveredLink = recoveredRegion.getByRole("link", { name: /192\.168\.50\.1/ });
    await expect(recoveredLink).toBeVisible();
    await expect(recoveredLink).toHaveAttribute("href", /^http:\/\/192\.168\.50\.1(:\d+)?/);

    // Click 确认连接正常 on recovered banner
    await recoveredRegion.getByRole("button", { name: "确认连接正常" }).click();

    const confirmWrites = fixture.featureWrites.filter((w) => w.path === "/api/features/confirm");
    expect(confirmWrites).toHaveLength(1);
  });

  test("8. omits untouched empty secrets and does not write secrets to localStorage (set_wan pppoePwd)", async ({ page, baseURL }) => {
    const fixture = await installFeaturesFixture(page, baseURL!);
    await page.goto("/#/network");
    await page.getByRole("tab", { name: "功能设置" }).click();
    await expect(page.getByText("已读取")).toBeVisible();

    const wanForm = page
      .locator("form")
      .filter({ has: page.getByRole("heading", { name: "设置 WAN IPv4 / DNS", exact: true }) });
    await expect(wanForm).toBeVisible();

    // PPPoE 密码 is configured (placeholder shows 已配置（留空保留）)
    const pwdInput = wanForm.getByLabel("PPPoE 密码");
    await expect(pwdInput).toHaveAttribute("placeholder", "已配置（留空保留）");

    // Modify PPPoE MRU (integer), leave password untouched
    await wanForm.getByLabel("PPPoE MRU").fill("1480");
    await wanForm.getByRole("button", { name: "应用设置" }).click();
    await page.getByRole("button", { name: "确认继续" }).click();

    await expect(page.getByText("配置已成功应用并生效")).toBeVisible();

    const lastWrite = fixture.featureWrites[fixture.featureWrites.length - 1];
    expect("pppoePwd" in lastWrite.body.input).toBe(false);

    // Verify localStorage has not stored any passwords
    const storageDump = await page.evaluate(() => JSON.stringify(window.localStorage));
    expect(storageDump).not.toContain("password");
    expect(storageDump).not.toContain("pppoePwd");
  });

  test("9. renders wireless features tab under wifi page and switches reads (wifi_detail_all & wifi_share_info)", async ({ page, baseURL }) => {
    await installFeaturesFixture(page, baseURL!);
    await page.goto("/#/wifi");

    // Click 功能设置 tab
    await page.getByRole("tab", { name: "功能设置" }).click();
    await expect(page.getByRole("tablist", { name: /无线.*功能分组/ })).toBeVisible();
    await expect(page.getByRole("tab", { name: "无线配置与频段能力" })).toBeVisible();
    await expect(page.getByRole("tab", { name: "访客配置与状态" })).toBeVisible();

    // Auto-loads 无线配置与频段能力
    await expect(page.getByText("已读取")).toBeVisible();
    await expect(page.getByText("Xiaomi_BE6500_5G")).toBeVisible();

    // Switch to 访客配置与状态
    await page.getByRole("tab", { name: "访客配置与状态" }).click();
    await expect(page.getByText("已读取")).toBeVisible();
    await expect(page.getByText("Xiaomi_Guest")).toBeVisible();
  });

  test("10. renders services features tab under system page and pre-fills from forwarding list", async ({ page, baseURL }) => {
    await installFeaturesFixture(page, baseURL!);
    await page.goto("/#/system");

    // Click 高级服务 tab
    await page.getByRole("tab", { name: "高级服务" }).click();
    await expect(page.getByRole("tablist", { name: /服务.*功能分组/ })).toBeVisible();
    await expect(page.getByRole("heading", { name: "端口转发", exact: true })).toBeVisible();
    await expect(page.getByText("SSH Service")).toBeVisible();
    await expect(page.getByText("192.168.31.50")).toBeVisible();

    // Click 选择编辑 on first forwarding rule in list
    const selectEditButtons = page.getByRole("button", { name: /选择编辑/ });
    await selectEditButtons.first().click();

    // Form for 添加单端口转发 (forward_add) pre-fills target IP and sport
    const forwardForm=page.locator("form").filter({has:page.getByRole("heading",{name:"添加单端口转发",exact:true})});
    await expect(forwardForm.getByLabel("目标 IPv4",{exact:true})).toHaveValue("192.168.31.50");
    await expect(forwardForm.getByLabel("外部端口",{exact:true})).toHaveValue("2222");
  });

  test("11. truthfully renders backend error in error state banner", async ({ page, baseURL }) => {
    await installFeaturesFixture(page, baseURL!);
    // Override apply to return error
    await page.route("**/api/features/network/apply", (route) => {
      return route.fulfill({
        status: 400,
        contentType: "application/json",
        body: JSON.stringify({
          error: { code: "invalid_lan_ip", message: "输入的 LAN 地址在保留网段内不可用" },
        }),
      });
    });

    await page.goto("/#/network");
    await page.getByRole("tab", { name: "功能设置" }).click();
    await page.getByRole("tab", { name: "LAN 地址与链路" }).click();
    await expect(page.getByText("已读取")).toBeVisible();

    await page.getByLabel("LAN IPv4").fill("999.999.999.999");
    await page.getByRole("button", { name: "应用设置" }).click();
    await page.getByRole("button", { name: "确认继续" }).click();

    // Truthful error must be visible
    await expect(page.getByText("输入的 LAN 地址在保留网段内不可用")).toBeVisible();
  });
});
