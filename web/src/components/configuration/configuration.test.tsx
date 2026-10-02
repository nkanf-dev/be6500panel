import { afterEach, describe, expect, it, vi } from "vitest";
import { StrictMode } from "react";
import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import {
  clearConfigurationSession,
  ConfigurationEditor,
  ConfigurationWorkspace,
} from "./index";
import type { ConfigurationDraft } from "./contracts";

const source = "config interface 'lan'\n\toption ipaddr '192.0.2.1'\n";
const edited = "config interface 'lan'\n\toption ipaddr '192.0.2.2'\n";
const draft: ConfigurationDraft = {
  id: "draft-synthetic",
  module: "network",
  generation: 7,
  diff: "--- network\n+++ network\n@@ -1,2 +1,2 @@\n config interface 'lan'\n- option ipaddr '192.0.2.1'\n+ option ipaddr '192.0.2.2'",
  risks: [],
  valid: true,
  errors: [],
  createdAt: "2026-01-01T00:00:00Z",
};
const respond = (body: unknown, status = 200) =>
  new Response(JSON.stringify(body), {
    status,
    headers: { "Content-Type": "application/json" },
  });
function mockApi(
  options: {
    generation?: number;
    drafts?: readonly ConfigurationDraft[];
    documents?: readonly { module: string; content: string }[];
    risky?: boolean;
    enabled?: boolean;
    stageConflict?: boolean;
    pending?: { id: string; deadline: string };
  } = {},
) {
  let generation = options.generation ?? 7;
  let drafts = [...(options.drafts ?? [])];
  let pending = options.pending;
  const fetchMock = vi.fn(async (url: string, _init?: RequestInit) => {
    if (url === "/api/configuration/status")
      return respond({
        enabled: options.enabled ?? true,
        generation,
        ...(pending ? { pendingCommit: pending } : {}),
      });
    if (url === "/api/configuration")
      return respond({
        generation,
        documents: options.documents ?? [
          {
            module: "network",
            content:
              options.stageConflict && generation === 8
                ? source.replace("192.0.2.1", "192.0.2.9")
                : source,
          },
          {
            module: "wireless",
            content:
              "config wifi-iface 'synthetic'\n option key 'synthetic-test-key'\n",
          },
        ],
        ...(pending ? { pendingCommit: pending } : {}),
      });
    if (url === "/api/configuration/drafts") return respond({ drafts });
    if (url === "/api/configuration/stage") {
      if (options.stageConflict) {
        generation = 8;
        return respond(
          { error: { code: "generation_conflict", message: "配置版本已改变" } },
          409,
        );
      }
      const staged = {
        ...draft,
        risks: options.risky
          ? [{ code: "management_address", message: "管理地址发生变化" }]
          : [],
      };
      drafts.push(staged);
      return respond(staged);
    }
    if (url.startsWith("/api/configuration/drafts?id=")) {
      drafts = [];
      return respond({ deleted: true });
    }
    if (url === "/api/configuration/commit") {
      generation = 8;
      drafts = [];
      if (options.risky)
        pending = {
          id: "commit-synthetic",
          deadline: new Date(Date.now() + 120_000).toISOString(),
        };
      return respond({
        id: "commit-synthetic",
        generation,
        state: pending ? "pending_confirmation" : "committed",
        ...(pending ? { deadline: pending.deadline } : {}),
        changedModules: ["network"],
      });
    }
    if (
      url === "/api/configuration/confirm" ||
      url === "/api/configuration/rollback"
    ) {
      pending = undefined;
      return respond({
        id: "commit-synthetic",
        generation,
        state: url.endsWith("rollback") ? "rolled_back" : "committed",
        changedModules: ["network"],
      });
    }
    throw new Error(`Unexpected request: ${url}`);
  });
  vi.stubGlobal("fetch", fetchMock);
  return fetchMock;
}
function posts(fetchMock: ReturnType<typeof mockApi>, endpoint: string) {
  return fetchMock.mock.calls.filter(
    ([url, init]) =>
      url === `/api/configuration/${endpoint}` && init?.method === "POST",
  );
}
afterEach(() => {
  clearConfigurationSession();
  vi.unstubAllGlobals();
  vi.useRealTimers();
});

describe("FullControl configuration", () => {
  it("defaults to editable fields and Ctrl+Enter stages their changed buffer without committing or changing saved generation", async () => {
    const fetchMock = mockApi();
    render(<ConfigurationEditor module="network" />);
    const input = await screen.findByRole("textbox", {
      name: /\(ipaddr\)/,
    });
    expect(screen.getByRole("tab", { name: "字段编辑" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    expect(
      screen.queryByRole("textbox", { name: "network 原生配置" }),
    ).not.toBeInTheDocument();
    fireEvent.change(input, { target: { value: "192.0.2.2" } });
    expect(screen.getByText("未暂存")).toBeInTheDocument();
    fireEvent.keyDown(input, { key: "Enter", ctrlKey: true });
    await screen.findByText("已暂存");
    expect(posts(fetchMock, "stage")).toHaveLength(1);
    expect(JSON.parse(posts(fetchMock, "stage")[0][1]!.body as string)).toEqual(
      { module: "network", content: edited, generation: 7 },
    );
    expect(posts(fetchMock, "commit")).toHaveLength(0);
    expect(screen.getByText("已保存版本 g7")).toBeInTheDocument();
    expect(screen.getByTestId("configuration-diff")).toHaveTextContent(
      "192.0.2.2",
    );
  });
  it("shows generation conflicts without losing unsaved text or silently retrying Stage", async () => {
    const fetchMock = mockApi({ stageConflict: true });
    render(<ConfigurationEditor module="network" />);
    const input = await screen.findByRole("textbox", {
      name: /\(ipaddr\)/,
    });
    fireEvent.change(input, { target: { value: "192.0.2.2" } });
    fireEvent.click(screen.getByRole("button", { name: "暂存并校验" }));
    await screen.findByText(/generation_conflict/);
    expect(input).toHaveValue("192.0.2.2");
    expect(posts(fetchMock, "stage")).toHaveLength(1);
    await screen.findByText("编辑基线 g7 · 当前 g8");
    expect(screen.getByRole("button", { name: "暂存并校验" })).toBeDisabled();
  });
  it("commits a low-risk selected draft directly with no confirmation dialog", async () => {
    const fetchMock = mockApi({ drafts: [draft] });
    const user = userEvent.setup();
    render(<ConfigurationWorkspace />);
    await screen.findByRole("checkbox", { name: "选择草稿 draft-synthetic" });
    await user.click(
      screen.getByRole("button", { name: "Commit 已选草稿 (1)" }),
    );
    await screen.findByText(/已提交.*network/);
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(
      JSON.parse(posts(fetchMock, "commit")[0][1]!.body as string),
    ).toEqual({ draftIds: [draft.id], generation: 7, acknowledgeRisks: false });
  });
  it("acknowledges risks once and shows pending confirmation with explicit Confirm and Rollback", async () => {
    const risky = {
      ...draft,
      risks: [{ code: "management_address", message: "管理地址发生变化" }],
    };
    const fetchMock = mockApi({ drafts: [risky], risky: true });
    const user = userEvent.setup();
    render(<ConfigurationWorkspace />);
    await screen.findByRole("checkbox", { name: "选择草稿 draft-synthetic" });
    await user.click(
      screen.getByRole("button", { name: "Commit 已选草稿 (1)" }),
    );
    const dialog = screen.getByRole("dialog", { name: "确认高风险 Commit" });
    expect(
      within(dialog).getByText("network / option ipaddr"),
    ).toBeInTheDocument();
    expect(posts(fetchMock, "commit")).toHaveLength(0);
    await user.click(
      within(dialog).getByRole("button", { name: "确认风险并 Commit" }),
    );
    await screen.findByText("等待连接确认");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(
      JSON.parse(posts(fetchMock, "commit")[0][1]!.body as string)
        .acknowledgeRisks,
    ).toBe(true);
    expect(screen.getByRole("button", { name: "确认当前连接" })).toBeEnabled();
    await user.click(screen.getByRole("button", { name: "恢复先前配置" }));
    await screen.findByText(/已回滚/);
    expect(posts(fetchMock, "rollback")).toHaveLength(1);
  });
  it("blocks confirmation after deadline and never assumes the rollback is already complete", async () => {
    mockApi({
      pending: {
        id: "commit-synthetic",
        deadline: new Date(Date.now() - 1000).toISOString(),
      },
    });
    render(<ConfigurationWorkspace />);
    await screen.findByText("确认期限已到，正在核对回滚状态");
    expect(screen.getByRole("button", { name: "确认当前连接" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "刷新提交状态" })).toBeEnabled();
    expect(screen.queryByText(/已回滚/)).not.toBeInTheDocument();
  });
  it("does not request private documents when write support is disabled", async () => {
    const fetchMock = mockApi({ enabled: false });
    render(<ConfigurationEditor module="wireless" />);
    await screen.findByText("配置服务未启用");
    expect(
      fetchMock.mock.calls.some(([url]) => url === "/api/configuration"),
    ).toBe(false);
  });
  it("masks editable passwords by default and shares changes with advanced native editing", async () => {
    const fetchMock = mockApi();
    render(<ConfigurationEditor module="wireless" />);
    const key = await screen.findByLabelText(/\(key\)/);
    expect(key).toHaveAttribute("type", "password");
    expect(key).toHaveValue("synthetic-test-key");
    expect(
      screen.queryByRole("textbox", { name: "wireless 原生配置" }),
    ).not.toBeInTheDocument();
    fireEvent.change(key, { target: { value: "edited-test-key" } });
    fireEvent.click(screen.getByRole("tab", { name: "高级：原生编辑" }));
    const raw = screen.getByRole("textbox", { name: "wireless 原生配置" });
    expect(raw).toHaveValue(
      "config wifi-iface 'synthetic'\n option key 'edited-test-key'\n",
    );
    fireEvent.change(raw, {
      target: {
        value:
          "config wifi-iface 'synthetic'\n option key \"raw-test-key\" # retain comment\n",
      },
    });
    fireEvent.click(screen.getByRole("tab", { name: "字段编辑" }));
    expect(screen.getByLabelText(/\(key\)/)).toHaveValue("raw-test-key");
    fireEvent.click(screen.getByRole("button", { name: "暂存并校验" }));
    await screen.findByText("已暂存");
    expect(JSON.parse(posts(fetchMock, "stage")[0][1]!.body as string)).toEqual(
      {
        module: "wireless",
        generation: 7,
        content:
          "config wifi-iface 'synthetic'\n option key \"raw-test-key\" # retain comment\n",
      },
    );
  });
  it("edits booleans, selects, numeric and unknown fields while retaining vendor enum values until changed", async () => {
    const content =
      "# synthetic\nconfig interface 'lan'\n option proto 'vendor-auto' # preserve until edited\n option delegate 'yes'\n option mtu 1500 # numeric\n option vendor_flag 'router-value'\nconfig interface 'wan'\n option proto 'dhcp'\n";
    const fetchMock = mockApi({ documents: [{ module: "network", content }] });
    render(<ConfigurationEditor module="network" />);
    const proto = await screen.findByRole("combobox", { name: /\(proto\)/ });
    expect(proto).toHaveValue("vendor-auto");
    expect(
      screen.getByRole("option", { name: /vendor-auto.*当前值/ }),
    ).toBeInTheDocument();
    const boolean = screen.getByRole("checkbox", { name: /\(delegate\)/ });
    expect(boolean).toBeChecked();
    fireEvent.click(boolean);
    const mtu = screen.getByRole("spinbutton", { name: /\(mtu\)/ });
    fireEvent.change(mtu, { target: { value: "1400" } });
    fireEvent.change(screen.getByRole("textbox", { name: /\(vendor_flag\)/ }), {
      target: { value: "Alice's # router" },
    });
    fireEvent.click(screen.getByRole("tab", { name: "高级：原生编辑" }));
    expect(
      screen.getByRole("textbox", { name: "network 原生配置" }),
    ).toHaveValue(
      content
        .replace("'yes'", "'no'")
        .replace("1500", "1400")
        .replace("'router-value'", "'Alice'\\''s # router'"),
    );
    fireEvent.click(screen.getByRole("tab", { name: "字段编辑" }));
    expect(
      screen.getByRole("textbox", { name: /\(vendor_flag\)/ }),
    ).toHaveValue("Alice's # router");
    fireEvent.change(screen.getByRole("combobox", { name: /\(proto\)/ }), {
      target: { value: "static" },
    });
    fireEvent.click(screen.getByRole("button", { name: "暂存并校验" }));
    await screen.findByText("已暂存");
    expect(
      JSON.parse(posts(fetchMock, "stage")[0][1]!.body as string).content,
    ).toBe(
      content
        .replace("'vendor-auto'", "'static'")
        .replace("'yes'", "'no'")
        .replace("1500", "1400")
        .replace("'router-value'", "'Alice'\\''s # router'"),
    );
    expect(posts(fetchMock, "commit")).toHaveLength(0);
  });
  it("adds, edits and removes list entries, including removing the final entry and adding it again", async () => {
    const content =
      "config interface 'lan'\n list dns '192.0.2.53' # resolver comment\n option ipaddr '192.0.2.1'\n list dns \"192.0.2.54\"\nconfig interface 'wan'\n option proto 'dhcp'\n";
    const fetchMock = mockApi({ documents: [{ module: "network", content }] });
    render(<ConfigurationEditor module="network" />);
    await screen.findByRole("textbox", { name: /\(dns\) 1/ });
    fireEvent.click(screen.getByRole("button", { name: "移除 dns 1" }));
    expect(screen.getByRole("textbox", { name: /\(dns\) 1/ })).toHaveValue(
      "192.0.2.54",
    );
    fireEvent.click(screen.getByRole("button", { name: "移除 dns 1" }));
    expect(
      screen.queryByRole("textbox", { name: /\(dns\)/ }),
    ).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "添加 dns 值" }));
    fireEvent.change(screen.getByRole("textbox", { name: /\(dns\) 1/ }), {
      target: { value: "198.51.100.53" },
    });
    fireEvent.click(screen.getByRole("button", { name: "添加 dns 值" }));
    fireEvent.change(screen.getByRole("textbox", { name: /\(dns\) 2/ }), {
      target: { value: "198.51.100.54" },
    });
    fireEvent.click(screen.getByRole("button", { name: "暂存并校验" }));
    await screen.findByText("已暂存");
    const staged = JSON.parse(
      posts(fetchMock, "stage")[0][1]!.body as string,
    ).content;
    expect(staged).toBe(
      "config interface 'lan'\n # resolver comment\n option ipaddr '192.0.2.1'\n list dns '198.51.100.53'\n list dns '198.51.100.54'\nconfig interface 'wan'\n option proto 'dhcp'\n",
    );
  });
  it("adds standard and unknown settings without requiring native editing", async () => {
    const fetchMock = mockApi();
    render(<ConfigurationEditor module="network" />);
    const name = await screen.findByRole("combobox", { name: "新字段名称" });
    fireEvent.change(name, { target: { value: "proto" } });
    fireEvent.click(screen.getByRole("button", { name: "添加字段" }));
    fireEvent.change(screen.getByRole("combobox", { name: /\(proto\)/ }), {
      target: { value: "static" },
    });
    fireEvent.change(screen.getByRole("combobox", { name: "新字段名称" }), {
      target: { value: "__custom" },
    });
    fireEvent.change(screen.getByRole("textbox", { name: "新字段名称" }), {
      target: { value: "vendor_note" },
    });
    fireEvent.click(screen.getByRole("button", { name: "添加字段" }));
    fireEvent.change(screen.getByRole("textbox", { name: /\(vendor_note\)/ }), {
      target: { value: "editable unknown" },
    });
    fireEvent.click(screen.getByRole("button", { name: "暂存并校验" }));
    await screen.findByText("已暂存");
    expect(
      JSON.parse(posts(fetchMock, "stage")[0][1]!.body as string).content,
    ).toBe(
      source +
        "\toption proto 'static'\n\toption vendor_note 'editable unknown'\n",
    );
  });
  it("keeps nonstandard boolean and numeric values editable without coercing them on render", async () => {
    const content =
      "config interface 'lan'\n option delegate 'vendor-auto'\n option mtu 'auto'\n";
    const fetchMock = mockApi({ documents: [{ module: "network", content }] });
    render(<ConfigurationEditor module="network" />);
    expect(
      await screen.findByRole("textbox", { name: /\(delegate\)/ }),
    ).toHaveValue("vendor-auto");
    expect(screen.getByRole("textbox", { name: /\(mtu\)/ })).toHaveValue(
      "auto",
    );
    expect(screen.getByRole("button", { name: "暂存并校验" })).toBeDisabled();
    expect(posts(fetchMock, "stage")).toHaveLength(0);
    fireEvent.change(screen.getByRole("textbox", { name: /\(mtu\)/ }), {
      target: { value: "1400" },
    });
    expect(screen.getByRole("spinbutton", { name: /\(mtu\)/ })).toHaveValue(
      1400,
    );
  });
  it("shows multiline vendor values as editable text without losing line breaks", async () => {
    const content =
      "config interface 'lan'\n option vendor_note \"First\nSecond\" # multiline\n option mtu '1.'\n";
    const fetchMock = mockApi({ documents: [{ module: "network", content }] });
    render(<ConfigurationEditor module="network" />);
    const input = await screen.findByRole("textbox", {
      name: /\(vendor_note\)/,
    });
    expect(input.tagName).toBe("TEXTAREA");
    expect(input).toHaveValue("First\nSecond");
    expect(screen.getByRole("textbox", { name: /\(mtu\)/ })).toHaveValue("1.");
    fireEvent.change(input, { target: { value: "Next\nLine" } });
    fireEvent.click(screen.getByRole("button", { name: "暂存并校验" }));
    await screen.findByText("已暂存");
    expect(
      JSON.parse(posts(fetchMock, "stage")[0][1]!.body as string).content,
    ).toBe(content.replace("First\nSecond", "Next\nLine"));
  });
  it.each([
    {
      module: "dhcp",
      content: "config dnsmasq\n option port '53' # DNS port\n",
      role: "spinbutton",
      field: "port",
      value: "5353",
      expected: "'5353'",
    },
    {
      module: "firewall",
      content: "config defaults\n option input 'REJECT' # default policy\n",
      role: "combobox",
      field: "input",
      value: "DROP",
      expected: "'DROP'",
    },
    {
      module: "system",
      content: "config system\n option hostname 'synthetic' # device name\n",
      role: "textbox",
      field: "hostname",
      value: "edited-host",
      expected: "'edited-host'",
    },
    {
      module: "dropbear",
      content: "config dropbear\n option Port '22' # SSH port\n",
      role: "spinbutton",
      field: "Port",
      value: "2222",
      expected: "'2222'",
    },
  ] as const)(
    "stages $module field edits using section-specific controls",
    async ({ module, content, role, field, value, expected }) => {
      const fetchMock = mockApi({ documents: [{ module, content }] });
      render(<ConfigurationEditor module={module} />);
      const input = await screen.findByRole(role, {
        name: new RegExp(`\\(${field}\\)`),
      });
      fireEvent.change(input, { target: { value } });
      fireEvent.click(screen.getByRole("button", { name: "暂存并校验" }));
      await screen.findByText("已暂存");
      expect(
        JSON.parse(posts(fetchMock, "stage")[0][1]!.body as string),
      ).toEqual({
        module,
        generation: 7,
        content: content.replace(/'[^']*'(?= #)/, expected),
      });
    },
  );
  it("retains local edits when switching documents and refreshing", async () => {
    mockApi();
    render(<ConfigurationWorkspace />);
    const input = await screen.findByRole("textbox", {
      name: /\(ipaddr\)/,
    });
    fireEvent.change(input, { target: { value: "192.0.2.2" } });
    fireEvent.click(screen.getByRole("button", { name: "编辑 wireless" }));
    await screen.findByLabelText(/\(key\)/);
    fireEvent.click(screen.getByRole("button", { name: "刷新配置" }));
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "刷新配置" })).toBeEnabled(),
    );
    fireEvent.click(screen.getByRole("button", { name: "编辑 network" }));
    expect(screen.getByRole("textbox", { name: /\(ipaddr\)/ })).toHaveValue(
      "192.0.2.2",
    );
  });
  it("retains real field edits through page unmount/remount and stages the retained buffer", async () => {
    const fetchMock = mockApi();
    const page = render(<ConfigurationEditor module="network" />);
    const input = await screen.findByRole("textbox", { name: /\(ipaddr\)/ });
    fireEvent.change(input, { target: { value: "192.0.2.2" } });
    page.unmount();
    const otherPage = render(<ConfigurationEditor module="wireless" />);
    await screen.findByLabelText(/\(key\)/);
    otherPage.unmount();
    render(<ConfigurationWorkspace />);
    await screen.findByRole("textbox", { name: /\(ipaddr\)/ });
    expect(screen.getByRole("textbox", { name: /\(ipaddr\)/ })).toHaveValue(
      "192.0.2.2",
    );
    fireEvent.click(screen.getByRole("button", { name: "暂存并校验" }));
    await screen.findAllByText("已暂存");
    expect(JSON.parse(posts(fetchMock, "stage")[0][1]!.body as string)).toEqual(
      { module: "network", content: edited, generation: 7 },
    );
  });
  it("keeps a retained edit stale when saved content changed between page visits", async () => {
    mockApi();
    const page = render(<ConfigurationEditor module="network" />);
    const input = await screen.findByRole("textbox", { name: /\(ipaddr\)/ });
    fireEvent.change(input, { target: { value: "192.0.2.2" } });
    page.unmount();
    mockApi({
      generation: 8,
      documents: [
        {
          module: "network",
          content: source.replace("192.0.2.1", "192.0.2.9"),
        },
      ],
    });
    render(<ConfigurationEditor module="network" />);
    await screen.findByRole("textbox", { name: /\(ipaddr\)/ });
    await screen.findByText("编辑基线 g7 · 当前 g8");
    expect(screen.getByRole("button", { name: "暂存并校验" })).toBeDisabled();
    expect(screen.getByRole("textbox", { name: /\(ipaddr\)/ })).toHaveValue(
      "192.0.2.2",
    );
  });
  it("warns before closing a tab with private unsaved edits, but not after discarding them", async () => {
    mockApi();
    render(<ConfigurationEditor module="network" />);
    const input = await screen.findByRole("textbox", { name: /\(ipaddr\)/ });
    fireEvent.change(input, { target: { value: "192.0.2.2" } });
    const dirtyClose = new Event("beforeunload", { cancelable: true });
    window.dispatchEvent(dirtyClose);
    expect(dirtyClose.defaultPrevented).toBe(true);
    fireEvent.click(screen.getByRole("button", { name: "丢弃本地编辑" }));
    const cleanClose = new Event("beforeunload", { cancelable: true });
    window.dispatchEvent(cleanClose);
    expect(cleanClose.defaultPrevented).toBe(false);
  });
  it("clears private retained buffers on session expiration", async () => {
    mockApi();
    const page = render(<ConfigurationEditor module="network" />);
    const input = await screen.findByRole("textbox", { name: /\(ipaddr\)/ });
    fireEvent.change(input, { target: { value: "192.0.2.2" } });
    page.unmount();
    window.dispatchEvent(new Event("be6500panel:unauthorized"));
    render(<ConfigurationEditor module="network" />);
    expect(
      await screen.findByRole("textbox", { name: /\(ipaddr\)/ }),
    ).toHaveValue("192.0.2.1");
  });
  it("does not select invalid or stale drafts and shows their diagnostics and diff", async () => {
    const invalid = {
      ...draft,
      id: "invalid-synthetic",
      valid: false,
      errors: [{ code: "invalid_port", message: "端口超出范围" }],
    };
    const stale = { ...draft, id: "stale-synthetic", generation: 6 };
    mockApi({ drafts: [invalid, stale] });
    render(<ConfigurationWorkspace />);
    await screen.findByRole("checkbox", { name: "选择草稿 invalid-synthetic" });
    expect(
      screen.getByRole("checkbox", { name: "选择草稿 invalid-synthetic" }),
    ).toBeDisabled();
    expect(
      screen.getByRole("checkbox", { name: "选择草稿 stale-synthetic" }),
    ).toBeDisabled();
    expect(
      screen.getByRole("button", { name: "Commit 已选草稿 (0)" }),
    ).toBeDisabled();
    fireEvent.click(
      screen.getByRole("button", { name: "查看差异 invalid-synthetic" }),
    );
    expect(screen.getByText("invalid_port")).toBeInTheDocument();
    expect(screen.getByText("端口超出范围")).toBeInTheDocument();
  });
  it("selects only one draft per native document and deletes through the typed DELETE boundary", async () => {
    const older = { ...draft, id: "older-synthetic" };
    const fetchMock = mockApi({ drafts: [older, draft] });
    render(<ConfigurationWorkspace />);
    const checkbox = await screen.findByRole("checkbox", {
      name: "选择草稿 draft-synthetic",
    });
    expect(checkbox).toBeChecked();
    expect(
      screen.getByRole("checkbox", { name: "选择草稿 older-synthetic" }),
    ).not.toBeChecked();
    fireEvent.click(
      screen.getByRole("checkbox", { name: "选择草稿 older-synthetic" }),
    );
    expect(checkbox).not.toBeChecked();
    fireEvent.click(
      screen.getByRole("button", { name: "删除草稿 older-synthetic" }),
    );
    await waitFor(() =>
      expect(
        screen.queryByRole("checkbox", { name: "选择草稿 older-synthetic" }),
      ).not.toBeInTheDocument(),
    );
    expect(fetchMock).toHaveBeenCalledWith(
      "/api/configuration/drafts?id=older-synthetic",
      expect.objectContaining({ method: "DELETE", credentials: "same-origin" }),
    );
    expect(posts(fetchMock, "commit")).toHaveLength(0);
  });
  it("confirms a pending operation explicitly", async () => {
    const fetchMock = mockApi({
      pending: {
        id: "commit-synthetic",
        deadline: new Date(Date.now() + 120_000).toISOString(),
      },
    });
    render(<ConfigurationWorkspace />);
    const confirm = await screen.findByRole("button", { name: "确认当前连接" });
    fireEvent.click(confirm);
    await screen.findByText(/已提交.*network/);
    expect(posts(fetchMock, "confirm")).toHaveLength(1);
    expect(posts(fetchMock, "commit")).toHaveLength(0);
  });
  it("reconciles only pending status every 10s and cancels timers on unmount", async () => {
    vi.useFakeTimers({ toFake: ["setInterval", "clearInterval", "Date"] });
    const fetchMock = mockApi({
      pending: {
        id: "commit-synthetic",
        deadline: new Date(Date.now() + 120_000).toISOString(),
      },
    });
    const view = render(<ConfigurationWorkspace />);
    await screen.findByRole("button", { name: "确认当前连接" });
    await waitFor(() =>
      expect(
        screen.getByRole("button", { name: "刷新提交状态" }),
      ).toBeEnabled(),
    );
    const documentReads = fetchMock.mock.calls.filter(
      ([url]) => url === "/api/configuration",
    ).length;
    const draftReads = fetchMock.mock.calls.filter(
      ([url]) => url === "/api/configuration/drafts",
    ).length;
    const statusReads = fetchMock.mock.calls.filter(
      ([url]) => url === "/api/configuration/status",
    ).length;
    await act(async () => {
      await vi.advanceTimersByTimeAsync(10_001);
    });
    expect(
      fetchMock.mock.calls.filter(
        ([url]) => url === "/api/configuration/status",
      ),
    ).toHaveLength(statusReads + 1);
    expect(
      fetchMock.mock.calls.filter(([url]) => url === "/api/configuration"),
    ).toHaveLength(documentReads);
    expect(
      fetchMock.mock.calls.filter(
        ([url]) => url === "/api/configuration/drafts",
      ),
    ).toHaveLength(draftReads);
    const total = fetchMock.mock.calls.length;
    view.unmount();
    await act(async () => {
      await vi.advanceTimersByTimeAsync(20_000);
    });
    expect(fetchMock.mock.calls).toHaveLength(total);
  });
  it("initializes safely in React StrictMode after its first lifetime is aborted", async () => {
    mockApi();
    render(
      <StrictMode>
        <ConfigurationEditor module="network" />
      </StrictMode>,
    );
    await screen.findByRole("textbox", { name: /\(ipaddr\)/ });
    expect(screen.getByRole("button", { name: "刷新配置" })).toBeEnabled();
  });
  it("aborts in-flight authenticated reads on unmount", async () => {
    const signals: AbortSignal[] = [];
    vi.stubGlobal(
      "fetch",
      vi.fn((_url: string, init: RequestInit) => {
        signals.push(init.signal as AbortSignal);
        return new Promise<Response>((_resolve, reject) =>
          init.signal?.addEventListener("abort", () =>
            reject(new DOMException("Aborted", "AbortError")),
          ),
        );
      }),
    );
    const view = render(<ConfigurationWorkspace />);
    await waitFor(() => expect(signals.length).toBeGreaterThan(0));
    view.unmount();
    await waitFor(() =>
      expect(signals.every((signal) => signal.aborted)).toBe(true),
    );
  });
});
