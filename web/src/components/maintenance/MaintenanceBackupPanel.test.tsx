import { StrictMode } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { MaintenanceBackupPanel } from "./index";
import { MAX_BACKUP_BYTES, type ImportPreview } from "./contracts";
import {
  respond,
  syntheticChange,
  syntheticPreview,
  syntheticRawBackup,
  syntheticStage,
} from "./maintenance-fixture.test-data";

function mockApi(
  options: {
    preview?: ImportPreview;
    stageError?: unknown;
    stageStatus?: number;
    previewStatus?: number;
  } = {},
) {
  const fetchMock = vi.fn(async (url: string, _init?: RequestInit) => {
    if (url === "/api/maintenance/backup")
      return new Response(syntheticRawBackup, {
        headers: {
          "Content-Type": "application/json",
          "Content-Disposition":
            'attachment; filename="synthetic-test-backup.json"',
        },
      });
    if (url === "/api/maintenance/import/preview")
      return respond(
        options.preview ?? syntheticPreview,
        options.previewStatus ?? 200,
      );
    if (url === "/api/maintenance/import/stage")
      return respond(
        options.stageError ?? syntheticStage,
        options.stageStatus ?? 200,
      );
    if (url.startsWith("/api/maintenance/import/preview?id="))
      return new Response(null, { status: 204 });
    throw new Error(`Unexpected request: ${url}`);
  });
  vi.stubGlobal("fetch", fetchMock);
  return fetchMock;
}
function chooseFile(raw = syntheticRawBackup) {
  const input = screen.getByLabelText("选择 JSON 备份文件");
  fireEvent.change(input, {
    target: {
      files: [
        new File([raw], "synthetic-test.json", { type: "application/json" }),
      ],
    },
  });
  return input;
}
const stageButton = () => screen.getByRole("button", { name: /暂存为草稿/ });
const stages = (fetchMock: ReturnType<typeof mockApi>) =>
  fetchMock.mock.calls.filter(
    ([url]) => url === "/api/maintenance/import/stage",
  );
const previews = (fetchMock: ReturnType<typeof mockApi>) =>
  fetchMock.mock.calls.filter(
    ([url]) => url === "/api/maintenance/import/preview",
  );

function fakeDownload() {
  const createObjectURL = vi.fn(() => "blob:synthetic");
  const revokeObjectURL = vi.fn();
  vi.stubGlobal(
    "URL",
    class extends URL {
      static createObjectURL = createObjectURL;
      static revokeObjectURL = revokeObjectURL;
    },
  );
  const click = vi
    .spyOn(HTMLAnchorElement.prototype, "click")
    .mockImplementation(() => {});
  return { click, createObjectURL, revokeObjectURL };
}

afterEach(() => {
  vi.unstubAllGlobals();
  vi.useRealTimers();
});

describe("MaintenanceBackupPanel", () => {
  it("states runtime validation was not performed instead of claiming invalid runtime config", async () => {
    mockApi({
      preview: {
        ...syntheticPreview,
        changes: [
          {
            ...syntheticChange,
            module: "runtime.frpc",
            diff: "",
            valid: false,
            stageable: false,
            errors: [
              {
                code: "runtime_validation_deferred",
                message: "Synthetic runtime validation has not been performed.",
              },
            ],
          },
        ],
      },
    });
    render(<MaintenanceBackupPanel />);
    chooseFile();
    await screen.findByRole("dialog");
    fireEvent.click(screen.getByText("查看FRPC 运行配置差异与检查结果"));
    expect(screen.getByText("本次未进行运行配置原生校验。")).toBeVisible();
    expect(
      screen.getByRole("region", { name: "FRPC 运行配置校验说明" }),
    ).toHaveTextContent("runtime_validation_deferred");
    expect(screen.queryByText("检查未通过")).not.toBeInTheDocument();
    expect(
      screen.getByRole("checkbox", { name: "暂存FRPC 运行配置" }),
    ).toBeDisabled();
  });

  it("defaults to six native scopes, opts runtime in explicitly, and manually exports without storage", async () => {
    const fetchMock = mockApi();
    const { click, revokeObjectURL } = fakeDownload();
    const setItem = vi.spyOn(Storage.prototype, "setItem");
    render(<MaintenanceBackupPanel />);
    expect(
      screen.getByRole("checkbox", { name: "备份FRPC 运行配置" }),
    ).not.toBeChecked();
    expect(
      screen.getByRole("checkbox", { name: "备份sing-box 运行配置" }),
    ).not.toBeChecked();
    expect(screen.getByRole("checkbox", { name: "备份网络" })).toBeChecked();
    expect(
      screen.getByText(/备份包含 Wi-Fi 密钥、Token 和私有配置/),
    ).toBeInTheDocument();
    expect(
      screen.getByText(/passwd、shadow 与 SSH 密钥文件不包含/),
    ).toBeInTheDocument();
    expect(fetchMock).not.toHaveBeenCalled();
    fireEvent.click(
      screen.getByRole("checkbox", { name: "备份FRPC 运行配置" }),
    );
    fireEvent.click(screen.getByRole("button", { name: "导出配置备份" }));
    await screen.findByText(/已请求下载备份/);
    expect(JSON.parse(fetchMock.mock.calls[0][1]!.body as string)).toEqual({
      scopes: [
        "network",
        "wireless",
        "dhcp",
        "firewall",
        "system",
        "dropbear",
        "runtime.frpc",
      ],
    });
    expect(click).toHaveBeenCalledTimes(1);
    expect(revokeObjectURL).toHaveBeenCalledWith("blob:synthetic");
    expect(setItem).not.toHaveBeenCalled();
    expect(stages(fetchMock)).toHaveLength(0);
  });
  it("disables export when no scope is selected", () => {
    mockApi();
    render(<MaintenanceBackupPanel />);
    screen
      .getAllByRole("checkbox")
      .filter((checkbox) => (checkbox as HTMLInputElement).checked)
      .forEach((checkbox) => fireEvent.click(checkbox));
    expect(screen.getByRole("button", { name: "导出配置备份" })).toBeDisabled();
  });
  it("uploads original raw file and previews exact generation, diagnostics and diff without side effects", async () => {
    const fetchMock = mockApi();
    const { click, createObjectURL } = fakeDownload();
    const setItem = vi.spyOn(Storage.prototype, "setItem");
    render(<MaintenanceBackupPanel />);
    const raw =
      '\uFEFF{  "fake":"synthetic-test-secret", "generation":7, "generation":8 }\r\n';
    const input = chooseFile(raw);
    const dialog = await screen.findByRole("dialog", {
      name: "导入配置差异预览",
    });
    expect(input).toHaveValue("");
    expect(previews(fetchMock)[0][1]?.body).toBe(raw);
    expect(dialog).toHaveTextContent("0 项新增，1 项修改，0 项删除");
    expect(
      within(dialog).getByText("预览配置版本").nextSibling,
    ).toHaveTextContent("7");
    expect(dialog).toHaveTextContent("备份包含私有测试配置");
    fireEvent.click(within(dialog).getByText("查看网络差异与检查结果"));
    expect(within(dialog).getByLabelText("网络配置差异")).toHaveTextContent(
      "192.0.2.2",
    );
    expect(dialog).toHaveTextContent("请一并核对相关防火墙草稿");
    expect(dialog).toHaveTextContent("管理地址可能改变");
    expect(click).not.toHaveBeenCalled();
    expect(createObjectURL).not.toHaveBeenCalled();
    expect(setItem).not.toHaveBeenCalled();
    expect(stages(fetchMock)).toHaveLength(0);
    expect(
      screen.queryByRole("button", { name: /立即应用/ }),
    ).not.toBeInTheDocument();
  });
  it("stages selected valid native modules only and directs to existing configuration queue", async () => {
    const preview: ImportPreview = {
      ...syntheticPreview,
      changes: [
        syntheticChange,
        { ...syntheticChange, module: "runtime.frpc", stageable: false },
        { ...syntheticChange, module: "runtime.sing-box", stageable: true },
        { ...syntheticChange, module: "wireless", kind: "unchanged" },
        {
          ...syntheticChange,
          module: "firewall",
          valid: false,
          errors: [{ code: "fake_invalid", message: "测试配置语法无效" }],
        },
      ],
    };
    const fetchMock = mockApi({ preview });
    const onOpenConfiguration = vi.fn();
    render(
      <MaintenanceBackupPanel onOpenConfiguration={onOpenConfiguration} />,
    );
    chooseFile();
    await screen.findByRole("dialog");
    expect(screen.getByRole("checkbox", { name: "暂存网络" })).toBeChecked();
    expect(
      screen.getByRole("checkbox", { name: "暂存FRPC 运行配置" }),
    ).toBeDisabled();
    expect(
      screen.getByRole("checkbox", { name: "暂存sing-box 运行配置" }),
    ).toBeDisabled();
    expect(screen.getByRole("checkbox", { name: "暂存无线" })).toBeDisabled();
    expect(screen.getByRole("checkbox", { name: "暂存防火墙" })).toBeDisabled();
    expect(
      screen.getAllByText("本次仅预览，运行配置恢复需在对应服务中确认。"),
    ).toHaveLength(2);
    fireEvent.click(stageButton());
    await screen.findByRole("status", { name: "导入暂存结果" });
    expect(JSON.parse(stages(fetchMock)[0][1]!.body as string)).toEqual({
      previewId: "preview-synthetic",
      generation: 7,
      modules: ["network"],
      acknowledgeModelMismatch: false,
    });
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(
      screen.getByText(/已暂存 1 个配置草稿，尚未应用/),
    ).toBeInTheDocument();
    expect(screen.getByText("应用前仍需核对依赖与风险")).toBeInTheDocument();
    expect(onOpenConfiguration).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "前往配置队列" }));
    expect(onOpenConfiguration).toHaveBeenCalledTimes(1);
    expect(
      fetchMock.mock.calls.every(
        ([url]) =>
          !url.includes("/commit") &&
          !url.includes("/reboot") &&
          !url.includes("/runtime/start"),
      ),
    ).toBe(true);
  });
  it("requires explicit model mismatch acknowledgement before Stage", async () => {
    const fetchMock = mockApi({
      preview: {
        ...syntheticPreview,
        currentModel: "synthetic-router-b",
        modelMismatch: true,
      },
    });
    render(<MaintenanceBackupPanel />);
    chooseFile();
    await screen.findByRole("dialog");
    expect(
      screen.getByText(
        /设备型号不一致：synthetic-router-a → synthetic-router-b/,
      ),
    ).toBeInTheDocument();
    expect(stageButton()).toBeDisabled();
    fireEvent.click(
      screen.getByRole("checkbox", { name: /我已核对设备型号差异/ }),
    );
    expect(stageButton()).toBeEnabled();
    fireEvent.click(stageButton());
    await screen.findByRole("status", { name: "导入暂存结果" });
    expect(
      JSON.parse(stages(fetchMock)[0][1]!.body as string)
        .acknowledgeModelMismatch,
    ).toBe(true);
  });
  it("does not stage unselected groups and shows uncompared runtime truthfully", async () => {
    mockApi({
      preview: {
        ...syntheticPreview,
        summary: { ...syntheticPreview.summary, uncompared: 1 },
        changes: [
          syntheticChange,
          {
            ...syntheticChange,
            module: "runtime.frpc",
            kind: "uncompared",
            beforeDigest: "",
            stageable: false,
          },
        ],
      },
    });
    render(<MaintenanceBackupPanel />);
    chooseFile();
    await screen.findByRole("dialog");
    expect(screen.getByText("无法比较")).toBeInTheDocument();
    expect(screen.getByText(/1 项无法比较/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("checkbox", { name: "暂存网络" }));
    expect(stageButton()).toBeDisabled();
  });
  it("shows native validation failures and does not let them be staged", async () => {
    mockApi({
      preview: {
        ...syntheticPreview,
        changes: [
          {
            ...syntheticChange,
            valid: false,
            stageable: false,
            errors: [
              { code: "invalid_synthetic_config", message: "测试配置语法无效" },
            ],
          },
        ],
      },
    });
    render(<MaintenanceBackupPanel />);
    chooseFile();
    await screen.findByRole("dialog");
    expect(screen.getByRole("checkbox", { name: "暂存网络" })).toBeDisabled();
    expect(stageButton()).toBeDisabled();
    fireEvent.click(screen.getByText("查看网络差异与检查结果"));
    expect(screen.getByText("测试配置语法无效")).toBeVisible();
    expect(screen.getByText("invalid_synthetic_config")).toBeVisible();
  });
  it("rejects oversized file preflight and does not upload it", async () => {
    const fetchMock = mockApi();
    render(<MaintenanceBackupPanel />);
    fireEvent.change(screen.getByLabelText("选择 JSON 备份文件"), {
      target: {
        files: [new File([new Uint8Array(MAX_BACKUP_BYTES + 1)], "large.json")],
      },
    });
    await screen.findByText(/backup_too_large/);
    expect(fetchMock).not.toHaveBeenCalled();
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });
  it("prevents Stage on expired preview and asks for a new upload", async () => {
    const fetchMock = mockApi({
      preview: { ...syntheticPreview, expiresAt: "2000-01-01T00:00:00Z" },
    });
    render(<MaintenanceBackupPanel />);
    chooseFile();
    await screen.findByText("预览已过期，请放弃后重新上传备份文件。");
    expect(stageButton()).toBeDisabled();
    expect(stages(fetchMock)).toHaveLength(0);
  });
  it("shows generation conflict and does not silently retry uncertain Stage", async () => {
    const fetchMock = mockApi({
      stageError: {
        error: { code: "generation_conflict", message: "配置版本已改变" },
      },
      stageStatus: 409,
    });
    render(<MaintenanceBackupPanel />);
    chooseFile();
    await screen.findByRole("dialog");
    fireEvent.click(stageButton());
    await screen.findByText(/generation_conflict/);
    expect(stageButton()).toBeDisabled();
    expect(stages(fetchMock)).toHaveLength(1);
    expect(
      screen.queryByRole("status", { name: "导入暂存结果" }),
    ).not.toBeInTheDocument();
  });
  it("shows retained cleanup drafts and offers queue instead of silent success", async () => {
    const fetchMock = mockApi({
      stageError: {
        error: { code: "import_cleanup_failed", message: "清理草稿失败" },
        retainedDraftIds: ["synthetic-retained"],
      },
      stageStatus: 500,
    });
    const onOpenConfiguration = vi.fn();
    render(
      <MaintenanceBackupPanel onOpenConfiguration={onOpenConfiguration} />,
    );
    chooseFile();
    await screen.findByRole("dialog");
    fireEvent.click(stageButton());
    await screen.findByText(/import_cleanup_failed/);
    expect(screen.getByText(/队列中可能保留 1 个草稿/)).toBeInTheDocument();
    expect(
      screen.queryByRole("status", { name: "导入暂存结果" }),
    ).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "前往配置队列" }));
    expect(onOpenConfiguration).toHaveBeenCalledTimes(1);
    expect(stages(fetchMock)).toHaveLength(1);
  });
  it("discards preview with DELETE and clears private diff state", async () => {
    const fetchMock = mockApi();
    render(<MaintenanceBackupPanel />);
    chooseFile();
    await screen.findByRole("dialog");
    fireEvent.click(screen.getByRole("button", { name: "放弃" }));
    await waitFor(() =>
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument(),
    );
    await waitFor(() =>
      expect(
        fetchMock.mock.calls.some(
          ([url, init]) =>
            url === "/api/maintenance/import/preview?id=preview-synthetic" &&
            init?.method === "DELETE",
        ),
      ).toBe(true),
    );
    expect(screen.queryByLabelText("网络配置差异")).not.toBeInTheDocument();
    expect(stages(fetchMock)).toHaveLength(0);
  });
  it.each(["be6500panel:unauthorized", "be6500panel:logout"])(
    "clears private preview on %s",
    async (eventName) => {
      mockApi();
      render(<MaintenanceBackupPanel />);
      chooseFile();
      await screen.findByRole("dialog");
      act(() => window.dispatchEvent(new Event(eventName)));
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
      expect(screen.queryByLabelText("网络配置差异")).not.toBeInTheDocument();
    },
  );
  it.each(["unmount", "be6500panel:unauthorized", "be6500panel:logout"])(
    "cancels in-flight upload on %s and ignores a late response",
    async (action) => {
      let resolve: ((value: Response) => void) | undefined;
      let signal: AbortSignal | undefined;
      vi.stubGlobal(
        "fetch",
        vi.fn((_url: string, init: RequestInit) => {
          signal = init.signal as AbortSignal;
          return new Promise<Response>((done) => {
            resolve = done;
          });
        }),
      );
      const view = render(<MaintenanceBackupPanel />);
      chooseFile();
      await waitFor(() => expect(signal).toBeDefined());
      if (action === "unmount") view.unmount();
      else act(() => window.dispatchEvent(new Event(action)));
      await waitFor(() => expect(signal?.aborted).toBe(true));
      await act(async () => {
        resolve?.(respond(syntheticPreview));
      });
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    },
  );
  it("aborts download on unmount and never saves a late response", async () => {
    const { click } = fakeDownload();
    let signal: AbortSignal | undefined;
    let resolve: ((value: Response) => void) | undefined;
    vi.stubGlobal(
      "fetch",
      vi.fn((_url: string, init: RequestInit) => {
        signal = init.signal as AbortSignal;
        return new Promise<Response>((done) => {
          resolve = done;
        });
      }),
    );
    const view = render(<MaintenanceBackupPanel />);
    fireEvent.click(screen.getByRole("button", { name: "导出配置备份" }));
    await waitFor(() => expect(signal).toBeDefined());
    view.unmount();
    await waitFor(() => expect(signal?.aborted).toBe(true));
    await act(async () => {
      resolve?.(new Response(syntheticRawBackup));
    });
    expect(click).not.toHaveBeenCalled();
  });
  it("blocks repeated Stage clicks and aborts Stage on session expiry", async () => {
    const signals: AbortSignal[] = [];
    const fetchMock = vi.fn(async (url: string, init: RequestInit) => {
      if (url.endsWith("/preview")) return respond(syntheticPreview);
      signals.push(init.signal as AbortSignal);
      return await new Promise<Response>(() => {});
    });
    vi.stubGlobal("fetch", fetchMock);
    render(<MaintenanceBackupPanel />);
    chooseFile();
    await screen.findByRole("dialog");
    const button = stageButton();
    fireEvent.click(button);
    fireEvent.click(button);
    await waitFor(() => expect(signals).toHaveLength(1));
    expect(screen.getByRole("button", { name: "放弃" })).toBeDisabled();
    act(() => window.dispatchEvent(new Event("be6500panel:unauthorized")));
    await waitFor(() => expect(signals[0].aborted).toBe(true));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("does not call omitted runtime text diff no changes or treat unknown size as zero", async () => {
    mockApi({
      preview: {
        ...syntheticPreview,
        summary: { ...syntheticPreview.summary, uncompared: 1 },
        changes: [
          {
            ...syntheticChange,
            module: "runtime.frpc",
            kind: "uncompared",
            beforeBytes: 0,
            beforeDigest: "",
            diff: "",
            stageable: false,
          },
        ],
      },
    });
    render(<MaintenanceBackupPanel />);
    chooseFile();
    await screen.findByRole("dialog");
    fireEvent.click(screen.getByText("查看FRPC 运行配置差异与检查结果"));
    expect(screen.getByText(/未知（未读取当前配置）/)).toBeVisible();
    expect(screen.getByLabelText("FRPC 运行配置配置差异")).toHaveTextContent(
      "运行配置本次以摘要和大小比较；恢复需到对应服务确认。",
    );
    expect(screen.queryByText("无配置差异")).not.toBeInTheDocument();
    expect(stageButton()).toBeDisabled();
  });
  it("discards the old server preview when a new file is picked", async () => {
    const fetchMock = mockApi();
    render(<MaintenanceBackupPanel />);
    chooseFile();
    await screen.findByRole("dialog");
    chooseFile('{"fake":"replacement-test"}');
    await screen.findByRole("dialog");
    await waitFor(() => expect(previews(fetchMock)).toHaveLength(2));
    expect(
      fetchMock.mock.calls.some(
        ([url, init]) =>
          url === "/api/maintenance/import/preview?id=preview-synthetic" &&
          init?.method === "DELETE",
      ),
    ).toBe(true);
  });
  it("opens real existing ConfigurationWorkspace when no navigation callback was supplied", async () => {
    const fetchMock = mockApi();
    fetchMock.mockImplementation(async (url: string) => {
      if (url === "/api/maintenance/import/preview")
        return respond(syntheticPreview);
      if (url === "/api/maintenance/import/stage")
        return respond(syntheticStage);
      if (url === "/api/configuration/status")
        return respond({ enabled: true, generation: 7 });
      if (url === "/api/configuration")
        return respond({ generation: 7, documents: [] });
      if (url === "/api/configuration/drafts")
        return respond({ drafts: syntheticStage.drafts });
      throw new Error(`Unexpected request ${url}`);
    });
    render(<MaintenanceBackupPanel />);
    chooseFile();
    await screen.findByRole("dialog");
    fireEvent.click(stageButton());
    await screen.findByRole("status", { name: "导入暂存结果" });
    fireEvent.click(screen.getByRole("button", { name: "前往配置队列" }));
    await screen.findByRole("checkbox", { name: "选择网络草稿 1" });
    expect(screen.getByText(syntheticStage.drafts[0].id)).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "返回备份与导入" }),
    ).toBeInTheDocument();
    expect(
      fetchMock.mock.calls.some(([url]) => url === "/api/configuration/commit"),
    ).toBe(false);
    fireEvent.click(screen.getByRole("button", { name: "返回备份与导入" }));
    expect(
      screen.getByRole("button", { name: "导出配置备份" }),
    ).toBeInTheDocument();
  });

  it("has no automatic private reads and works in StrictMode", async () => {
    const fetchMock = mockApi();
    render(
      <StrictMode>
        <MaintenanceBackupPanel />
      </StrictMode>,
    );
    expect(fetchMock).not.toHaveBeenCalled();
    chooseFile();
    await screen.findByRole("dialog");
    expect(previews(fetchMock)).toHaveLength(1);
  });
});
