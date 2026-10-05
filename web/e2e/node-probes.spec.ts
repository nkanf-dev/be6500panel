import { test, expect, type Locator, type Page } from "@playwright/test";
import {
  installNodeProbeBrowserFixture,
  expectedProbeStart,
  expectedProbeStop,
  probeNodeID,
  probeNodeLabel,
  PROBE_MEASURED_AT,
  PROBE_PATH,
  PROBE_REVISION,
  type ProbeWrite,
} from "./node-probe-browser-fixtures";
import {
  NODE_PROBE_TARGET,
  type NodeProbeRunInput,
} from "../src/modules/proxy/node-probe-contracts";

type Fixture = Awaited<ReturnType<typeof installNodeProbeBrowserFixture>>;
const install = (page: Page, baseURL?: string, stored = false) => {
  if (!baseURL)
    throw new Error("Node-probe acceptance requires the configured baseURL");
  return installNodeProbeBrowserFixture(page, baseURL, stored);
};
const input = (nodeIds: readonly string[], all = false): NodeProbeRunInput => ({
  all,
  nodeIds,
  revision: PROBE_REVISION,
});
const readCount = (fixture: Fixture, path: string) =>
  fixture.reads.filter((read) => read.split("?")[0] === path).length;
const expectWrites = (
  fixture: Fixture,
  expected: readonly ProbeWrite[] = [],
) => {
  expect(fixture.unexpected).toEqual([]);
  // The complete ledger includes raw bytes/body types; no ProxySelect/core/capture/config writes.
  expect(fixture.writes).toEqual(expected);
  expect(fixture.unconsumed()).toBeUndefined();
  expect(fixture.nodes().selectedNodeId).toBe(probeNodeID(1));
  // The compact gateway card observes canonical inactive capture and probe snapshots.
  expect(
    fixture.reads.filter(
      (read) =>
        /^\/api\/proxy\/(?:capture|node-probes)(?:[/?]|$)/.test(read) &&
        read !== "/api/proxy/capture" &&
        read !== PROBE_PATH,
    ),
  ).toEqual([]);
  expect(
    fixture.reads.filter((read) =>
      /\/api\/(?:proxy\/(?:request-traces|metrics|probe)(?:[/?]|$)|system\/services)/.test(
        read,
      ),
    ),
  ).toEqual([]);
};
const selectionState = async (page: Page, selector: Locator) => ({
  summaries: await selector
    .locator(".node-selector-node-summary")
    .allTextContents(),
  config: await selector.locator(".node-selector-config").evaluate((node) =>
    Array.from(
      node.querySelectorAll<HTMLInputElement | HTMLSelectElement>(
        "input, select",
      ),
      (control) => ({
        value: control.value,
        checked: "checked" in control ? control.checked : null,
      }),
    ),
  ),
  radios: await selector
    .getByRole("radio")
    .evaluateAll((nodes) =>
      nodes.map((node) => node.getAttribute("aria-checked")),
    ),
  savedPreference: await page.evaluate(() => {
    const value = JSON.parse(
      sessionStorage.getItem("be6500panel.proxy.node-view") ?? "{}",
    );
    return {
      selectedId: value.selectedId,
      ipv6: value.ipv6,
      ports: value.ports,
      acceptedGeneration: value.acceptedGeneration,
    };
  }),
});
const expectPage = async (selector: Locator, number: number) => {
  await expect(selector).toHaveCount(1);
  const cards = selector.locator(".node-selector-card");
  await expect(cards).toHaveCount(20);
  await expect(selector.locator(".node-probe-badge")).toHaveCount(20);
  await expect(cards.locator(".node-selector-name strong")).toHaveText(
    Array.from({ length: 20 }, (_, index) =>
      probeNodeLabel((number - 1) * 20 + index + 1),
    ),
  );
  await expect(
    selector.locator(".node-selector-controls [role=status]"),
  ).toContainText(
    `220 个匹配 / 共 220 个节点 · 本页 ${(number - 1) * 20 + 1}–${number * 20} · 每页 20 个`,
  );
};
const expectSelected = async (selector: Locator) => {
  const summary = selector.getByRole("region", {
    name: "节点选择与保存",
    exact: true,
  });
  await expect(summary).toContainText("与当前配置一致");
  await expect(summary).not.toContainText("尚未保存");
  await expect(
    summary.locator(".node-selector-node-summary").first(),
  ).toContainText(probeNodeLabel(1));
  await expect(
    summary.locator(".node-selector-node-summary").last(),
  ).toContainText(probeNodeLabel(1));
  await expect(
    summary.getByRole("button", { name: "保存配置", exact: true }),
  ).toBeEnabled();
};
const refresh = async (controls: Locator, fixture: Fixture) => {
  const before = readCount(fixture, PROBE_PATH);
  const button = controls.getByRole("button", {
    name: "刷新测速状态",
    exact: true,
  });
  await expect(button).toBeEnabled();
  await button.click();
  await expect
    .poll(() => readCount(fixture, PROBE_PATH))
    .toBeGreaterThan(before);
  await expect(button).toBeEnabled();
};

// Native Playwright, synthetic server readback only. Backend tests own real fixed-target/TLS claims.
// Pure fixture steps, not real-time probe delays, bound both cases to20 seconds.
test.describe("one-click node probes · isolated220-node browser acceptance", () => {
  test.setTimeout(20_000);
  test.beforeEach(async ({ page }) => {
    await page.emulateMedia({ reducedMotion: "reduce" });
  });

  test("single, exact20 current page and all220 jobs require explicit actions; progress and Stop never select or apply", async ({
    page,
    baseURL,
  }) => {
    const fixture = await install(page, baseURL);
    await page.goto("/#/proxy");
    await page
      .locator("summary")
      .filter({ hasText: /^更换节点$/ })
      .click();
    const selector = page.locator(".node-selector");
    const controls = selector.getByRole("region", {
      name: "节点延迟测速",
      exact: true,
    });
    const pager = selector
      .getByRole("navigation", { name: "节点分页", exact: true })
      .getByRole("combobox");
    const all = controls.getByRole("button", {
      name: "测速全部 (220)",
      exact: true,
    });
    await expect(controls).toHaveCount(1);
    await expect(all).toBeEnabled();
    await expectSelected(selector);
    await expectPage(selector, 1);
    expect(readCount(fixture, PROBE_PATH)).toBeGreaterThan(0);
    expect(fixture.snapshot().job).toBeUndefined();
    expect(fixture.admissions).toEqual([]);
    expectWrites(fixture); // Mount GET cannot start any job.
    const baseline = await selectionState(page, selector);
    const configReads = readCount(fixture, "/api/runtime/config");
    for (const number of [2, 11, 1]) {
      await pager.selectOption(String(number));
      await expectPage(selector, number);
    }
    const ledger: ProbeWrite[] = [];
    const single = input([probeNodeID(2)]);
    fixture.armStart(single);
    const badge = selector.getByRole("button", {
      name: `测速节点 ${probeNodeLabel(2)}：-- ms`,
      exact: true,
    });
    await badge.focus();
    await page.keyboard.press("Enter"); // Badge activation must not submit its surrounding form.
    ledger.push(expectedProbeStart(single));
    const stop = controls.getByRole("button", {
      name: "停止测速",
      exact: true,
    });
    await expect(stop).toBeEnabled();
    await expect(controls.locator(".node-probe-progress")).toHaveText(
      "正在测速 0/1…",
    );
    expectWrites(fixture, ledger);
    await expect(
      selector.getByLabel(`选择节点 ${probeNodeLabel(1)}`, { exact: true }),
    ).toHaveAttribute("aria-pressed", "true");
    await expect(
      selector.getByLabel(`选择节点 ${probeNodeLabel(2)}`, { exact: true }),
    ).toHaveAttribute("aria-pressed", "false");
    fixture.step(1);
    await refresh(controls, fixture);
    const measured = selector.getByRole("button", {
      name: `测速节点 ${probeNodeLabel(2)}：185 ms`,
      exact: true,
    });
    await expect(measured).toBeEnabled();
    await expect(measured).toHaveAttribute(
      "title",
      new RegExp(PROBE_MEASURED_AT.replace(/\./g, "\\.")),
    );
    expect(await selectionState(page, selector)).toEqual(baseline);

    await pager.selectOption("2");
    await expectPage(selector, 2);
    const pageIds = Array.from({ length: 20 }, (_, index) =>
      probeNodeID(index + 21),
    );
    const currentPage = input(pageIds);
    fixture.armStart(currentPage);
    await controls
      .getByRole("button", { name: "测速当前页 (20)", exact: true })
      .click();
    ledger.push(expectedProbeStart(currentPage));
    await expect(stop).toBeEnabled();
    await expect(controls.locator(".node-probe-progress")).toHaveText(
      "正在测速 0/20…",
    );
    expect(fixture.admissions[1]?.nodeIds).toEqual(pageIds);
    expectWrites(fixture, ledger);
    fixture.step(2);
    await refresh(controls, fixture);
    await expect(controls.locator(".node-probe-progress")).toHaveText(
      "正在测速 2/20…",
    );
    await expect(
      selector.getByRole("button", {
        name: `测速节点 ${probeNodeLabel(21)}：185 ms`,
        exact: true,
      }),
    ).toHaveClass(/node-probe-badge-medium/);
    await expect(
      selector.getByRole("button", {
        name: `测速节点 ${probeNodeLabel(22)}：超时`,
        exact: true,
      }),
    ).toHaveClass(/node-probe-badge-failure/);
    await expectPage(selector, 2);
    fixture.step(20);
    await refresh(controls, fixture);
    await expect(all).toBeEnabled();
    expect(await selectionState(page, selector)).toEqual(baseline);

    const everything = input([], true);
    fixture.armStart(everything);
    await all.click();
    ledger.push(expectedProbeStart(everything));
    await expect(stop).toBeEnabled();
    await expect(controls.locator(".node-probe-progress")).toHaveText(
      "正在测速 0/220…",
    );
    expect(fixture.admissions[2]).toEqual({
      input: everything,
      nodeIds: Array.from({ length: 220 }, (_, index) =>
        probeNodeID(index + 1),
      ),
    });
    fixture.step(45);
    await refresh(controls, fixture);
    await expect(controls.locator(".node-probe-progress")).toHaveText(
      "正在测速 45/220…",
    );
    expectWrites(fixture, ledger); // Passive progress reads never POST or DELETE.
    fixture.armStop();
    await stop.click();
    ledger.push(expectedProbeStop);
    await expect(all).toBeEnabled();
    await expect(controls).toContainText(
      "测速已停止。未完成的节点不会显示为成功。",
    );
    expect(
      fixture
        .snapshot()
        .results.filter((result) => result.status === "cancelled"),
    ).toHaveLength(175);
    await pager.selectOption("11");
    await expectPage(selector, 11);
    const cancelled = selector.getByRole("button", {
      name: `测速节点 ${probeNodeLabel(220)}：-- ms`,
      exact: true,
    });
    await expect(cancelled).toHaveAttribute("title", /测速已取消/);
    await expect(cancelled).not.toHaveText("0 ms");
    await expectSelected(selector);
    expect(await selectionState(page, selector)).toEqual(baseline);
    expect(readCount(fixture, "/api/runtime/config")).toBe(configReads);
    expectWrites(fixture, ledger); // Exactly3 POST +1 DELETE, and nothing else.
  });

  test("stored latency/time and failure badges survive passive GET without remount; a new node revision hides old results without auto jobs", async ({
    page,
    baseURL,
  }) => {
    const fixture = await install(page, baseURL, true);
    await page.goto("/#/proxy");
    await page
      .locator("summary")
      .filter({ hasText: /^更换节点$/ })
      .click();
    const selector = page.locator(".node-selector");
    const controls = selector.getByRole("region", {
      name: "节点延迟测速",
      exact: true,
    });
    const refreshButton = controls.getByRole("button", {
      name: "刷新测速状态",
      exact: true,
    });
    const measured = selector.getByRole("button", {
      name: `测速节点 ${probeNodeLabel(2)}：185 ms`,
      exact: true,
    });
    const timeout = selector.getByRole("button", {
      name: `测速节点 ${probeNodeLabel(3)}：超时`,
      exact: true,
    });
    const unreachable = selector.getByRole("button", {
      name: `测速节点 ${probeNodeLabel(4)}：不可达`,
      exact: true,
    });
    await expect(measured).toBeEnabled();
    await expectSelected(selector);
    await expectPage(selector, 1);
    await expect(measured).toHaveClass(/node-probe-badge-medium/);
    await expect(measured).toHaveAttribute("title", /测试时间：/);
    expect(await measured.getAttribute("title")).toContain(PROBE_MEASURED_AT);
    expect(await measured.getAttribute("title")).toContain(NODE_PROBE_TARGET);
    await expect(timeout).toHaveClass(/node-probe-badge-failure/);
    await expect(timeout).toHaveAttribute("title", /node_timeout/);
    await expect(unreachable).toHaveClass(/node-probe-badge-failure/);
    await expect(unreachable).toHaveAttribute("title", /node_unreachable/);
    await expect(
      selector.getByRole("button", {
        name: `测速节点 ${probeNodeLabel(1)}：-- ms`,
        exact: true,
      }),
    ).toBeEnabled();
    for (const failure of [timeout, unreachable])
      await expect(failure).not.toHaveText("0 ms");
    const baseline = await selectionState(page, selector);
    const controlNode = await controls.elementHandle();
    const refreshNode = await refreshButton.elementHandle();
    const measuredNode = await measured.elementHandle();
    if (!controlNode || !refreshNode || !measuredNode)
      throw new Error("Probe controls must be mounted");
    // No heavy chart is mounted for node badges; stable controls are the relevant refresh surface.
    await expect(selector.locator("canvas")).toHaveCount(0);
    await refresh(controls, fixture);
    expect(
      await controls.evaluate(
        (node, previous) => node === previous,
        controlNode,
      ),
    ).toBe(true);
    expect(
      await refreshButton.evaluate(
        (node, previous) => node === previous,
        refreshNode,
      ),
    ).toBe(true);
    expect(
      await measured.evaluate(
        (node, previous) => node === previous,
        measuredNode,
      ),
    ).toBe(true);
    await expect(controls).not.toContainText("正在读取节点测速状态");
    await expectPage(selector, 1);
    expect(await selectionState(page, selector)).toEqual(baseline);
    expect(fixture.admissions).toEqual([]);
    expectWrites(fixture);

    const nodeReads = readCount(fixture, "/api/proxy/nodes");
    fixture.setNodesRevision("fixture-node-probes-revision-2");
    // This GET-only action is in the separate advanced disclosure, not the picker.
    await page
      .locator("summary")
      .filter({ hasText: /^高级设置与诊断$/ })
      .click();
    await page
      .getByRole("button", { name: "刷新代理状态", exact: true })
      .click();
    await expect
      .poll(() => readCount(fixture, "/api/proxy/nodes"))
      .toBeGreaterThan(nodeReads);
    await expect(controls).toContainText("订阅节点已变化，旧测速结果已隐藏。");
    for (const number of [2, 3, 4]) {
      const hidden = selector.getByRole("button", {
        name: `测速节点 ${probeNodeLabel(number)}：-- ms`,
        exact: true,
      });
      await expect(hidden).toBeDisabled();
      await expect(hidden).toHaveClass(/node-probe-badge-unknown/);
      await expect(hidden).toHaveAttribute("title", /尚未测速/);
    }
    await expect(
      selector.locator(".node-probe-badge-medium, .node-probe-badge-failure"),
    ).toHaveCount(0);
    await expect(
      controls.getByRole("button", { name: "测速当前页 (20)", exact: true }),
    ).toBeDisabled();
    await expect(
      controls.getByRole("button", { name: "测速全部 (220)", exact: true }),
    ).toBeDisabled();
    await refresh(controls, fixture);
    await expectPage(selector, 1);
    await expectSelected(selector);
    expect(fixture.snapshot().revision).toBe(PROBE_REVISION); // Old source truly remains old.
    expect(fixture.admissions).toEqual([]);
    expect(await selectionState(page, selector)).toEqual(baseline);
    expectWrites(fixture); // Revision changes and every refresh remain GET-only.
    await controlNode.dispose();
    await refreshNode.dispose();
    await measuredNode.dispose();
  });
});
