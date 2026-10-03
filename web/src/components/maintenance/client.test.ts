import { afterEach, describe, expect, it, vi } from "vitest";
import { ApiError, runRequest } from "../../lib/api";
import {
  assertBackupSize,
  downloadBackup,
  maintenanceApi,
  readBackupFile,
  retainedDraftIds,
} from "./client";
import { BackupEnvelopeSchema, MAX_BACKUP_BYTES } from "./contracts";
import { Schema } from "effect";
import {
  respond,
  syntheticPreview,
  syntheticRawBackup,
  syntheticStage,
} from "./maintenance-fixture.test-data";

afterEach(() => vi.unstubAllGlobals());

describe("maintenance HTTP boundary", () => {
  it("rejects non-JSON, oversized and empty backup downloads", async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(
        new Response("<html>test</html>", {
          headers: { "Content-Type": "text/html" },
        }),
      )
      .mockResolvedValueOnce(
        new Response(new Uint8Array(MAX_BACKUP_BYTES + 1), {
          headers: { "Content-Type": "application/json" },
        }),
      )
      .mockResolvedValueOnce(
        new Response("", { headers: { "Content-Type": "application/json" } }),
      );
    vi.stubGlobal("fetch", fetchMock);
    await expect(
      runRequest(maintenanceApi.backup(["network"])),
    ).rejects.toMatchObject({ code: "invalid_response" });
    await expect(
      runRequest(maintenanceApi.backup(["network"])),
    ).rejects.toMatchObject({ code: "backup_too_large" });
    await expect(
      runRequest(maintenanceApi.backup(["network"])),
    ).rejects.toMatchObject({ code: "backup_empty" });
  });
  it("allows bounded long native Stage and never caches private requests", async () => {
    const timeout = vi.spyOn(AbortSignal, "timeout");
    const fetchMock = vi.fn(async () => respond(syntheticStage));
    vi.stubGlobal("fetch", fetchMock);
    await runRequest(
      maintenanceApi.stage({
        previewId: "test",
        generation: 7,
        modules: ["network"],
        acknowledgeModelMismatch: false,
      }),
    );
    expect(timeout).toHaveBeenCalledWith(90_000);
    expect(fetchMock).toHaveBeenCalledWith(
      "/api/maintenance/import/stage",
      expect.objectContaining({ cache: "no-store" }),
    );
  });

  it("exports selected scopes as an untouched downloadable JSON envelope", async () => {
    const raw = syntheticRawBackup;
    const fetchMock = vi.fn(
      async () =>
        new Response(raw, {
          headers: {
            "Content-Type": "application/json",
            "Content-Disposition":
              'attachment; filename="backup-test-v1-2026.json"',
          },
        }),
    );
    vi.stubGlobal("fetch", fetchMock);
    const file = await runRequest(
      maintenanceApi.backup(["network", "runtime.frpc"]),
    );
    expect(await file.blob.text()).toBe(raw);
    expect(file.filename).toBe("backup-test-v1-2026.json");
    expect(fetchMock).toHaveBeenCalledWith(
      "/api/maintenance/backup",
      expect.objectContaining({
        method: "POST",
        credentials: "same-origin",
        body: '{"scopes":["network","runtime.frpc"]}',
      }),
    );
    expect(() =>
      Schema.decodeUnknownSync(BackupEnvelopeSchema)(JSON.parse(raw)),
    ).not.toThrow();
  });
  it("uploads original whitespace, BOM and duplicate keys without reserialization", async () => {
    const raw =
      '\uFEFF{ "generation":7, "generation":8, "fake":"synthetic-test-token" }\r\n';
    const fetchMock = vi.fn(async () => respond(syntheticPreview));
    vi.stubGlobal("fetch", fetchMock);
    await expect(runRequest(maintenanceApi.preview(raw))).resolves.toEqual(
      syntheticPreview,
    );
    expect(fetchMock).toHaveBeenCalledWith(
      "/api/maintenance/import/preview",
      expect.objectContaining({
        method: "POST",
        body: raw,
        credentials: "same-origin",
        headers: {
          Accept: "application/json",
          "Content-Type": "application/json",
        },
      }),
    );
  });
  it("allows the server to detect malformed and duplicate-key JSON", async () => {
    const fetchMock = vi.fn(async () =>
      respond({ error: { code: "duplicate_key", message: "重复字段" } }, 400),
    );
    vi.stubGlobal("fetch", fetchMock);
    await expect(
      runRequest(maintenanceApi.preview('{"scopes":[],"scopes":[]}')),
    ).rejects.toMatchObject({ code: "duplicate_key", status: 400 });
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });
  it("checks encoded byte length, not JavaScript character count", async () => {
    const fetchMock = vi.fn();
    vi.stubGlobal("fetch", fetchMock);
    const raw = "测".repeat(Math.ceil(MAX_BACKUP_BYTES / 3));
    expect(raw.length).toBeLessThan(MAX_BACKUP_BYTES);
    await expect(runRequest(maintenanceApi.preview(raw))).rejects.toMatchObject(
      { code: "backup_too_large" },
    );
    expect(fetchMock).not.toHaveBeenCalled();
    expect(() => assertBackupSize(MAX_BACKUP_BYTES)).not.toThrow();
  });
  it("preflights actual file bytes and preserves UTF-8 BOM when decoding", async () => {
    const raw = '\uFEFF{  "fake":"synthetic-test-secret" }\r\n';
    expect(
      await readBackupFile(
        new File([raw], "synthetic.json"),
        new AbortController().signal,
      ),
    ).toBe(raw);
    await expect(
      readBackupFile(
        new File([new Uint8Array(MAX_BACKUP_BYTES + 1)], "large.json"),
        new AbortController().signal,
      ),
    ).rejects.toMatchObject({ code: "backup_too_large" });
    await expect(
      readBackupFile(new File([], "empty.json"), new AbortController().signal),
    ).rejects.toMatchObject({ code: "backup_empty" });
    await expect(
      readBackupFile(
        new File([new Uint8Array([0xff])], "invalid.json"),
        new AbortController().signal,
      ),
    ).rejects.toMatchObject({ code: "backup_encoding" });
  });
  it("rejects reads canceled before they start", async () => {
    const controller = new AbortController();
    controller.abort();
    await expect(
      readBackupFile(
        new File([syntheticRawBackup], "synthetic.json"),
        controller.signal,
      ),
    ).rejects.toMatchObject({ name: "AbortError" });
  });
  it("keeps Stage exact preview generation and decodes configuration drafts", async () => {
    const fetchMock = vi.fn(async () => respond(syntheticStage));
    vi.stubGlobal("fetch", fetchMock);
    const body = {
      previewId: syntheticPreview.id,
      generation: 7,
      modules: ["network"] as const,
      acknowledgeModelMismatch: true,
    };
    expect(await runRequest(maintenanceApi.stage(body))).toEqual(
      syntheticStage,
    );
    expect(fetchMock).toHaveBeenCalledWith(
      "/api/maintenance/import/stage",
      expect.objectContaining({ method: "POST", body: JSON.stringify(body) }),
    );
  });
  it("retains cleanup failure IDs from the top-level envelope, with no retry", async () => {
    const fetchMock = vi.fn(async () =>
      respond(
        {
          error: {
            code: "import_cleanup_failed",
            message: "清理草稿失败",
            causeCode: "generation_conflict",
          },
          retainedDraftIds: ["draft-test-retained", 123],
        },
        409,
      ),
    );
    vi.stubGlobal("fetch", fetchMock);
    const error = await runRequest(
      maintenanceApi.stage({
        previewId: "test",
        generation: 7,
        modules: ["network"],
        acknowledgeModelMismatch: false,
      }),
    ).catch((cause: unknown) => cause);
    expect(error).toBeInstanceOf(ApiError);
    expect(retainedDraftIds(error)).toEqual(["draft-test-retained"]);
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });
  it.each(["backup", "preview", "stage"] as const)(
    "propagates 401 unauthorized even with invalid JSON for %s",
    async (kind) => {
      const unauthorized = vi.fn();
      window.addEventListener("be6500panel:unauthorized", unauthorized);
      vi.stubGlobal(
        "fetch",
        vi.fn(async () => new Response("not JSON", { status: 401 })),
      );
      const effect =
        kind === "backup"
          ? maintenanceApi.backup(["network"])
          : kind === "preview"
            ? maintenanceApi.preview("{}")
            : maintenanceApi.stage({
                previewId: "test",
                generation: 7,
                modules: ["network"],
                acknowledgeModelMismatch: false,
              });
      await expect(runRequest<unknown>(effect)).rejects.toMatchObject({
        status: 401,
      });
      expect(unauthorized).toHaveBeenCalledTimes(1);
      window.removeEventListener("be6500panel:unauthorized", unauthorized);
    },
  );
  it("rejects invalid preview and Stage schemas without exposing raw private bodies", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => respond({ private: "synthetic-test-secret" })),
    );
    await expect(
      runRequest(maintenanceApi.preview("{}")),
    ).rejects.toMatchObject({ code: "invalid_response" });
    await expect(
      runRequest(
        maintenanceApi.stage({
          previewId: "test",
          generation: 7,
          modules: ["network"],
          acknowledgeModelMismatch: false,
        }),
      ),
    ).rejects.toMatchObject({ code: "invalid_response" });
  });
  it("discards previews by encoded ID, without uploading private data", async () => {
    const fetchMock = vi.fn(async () => new Response(null, { status: 204 }));
    vi.stubGlobal("fetch", fetchMock);
    await runRequest(maintenanceApi.discard("test / preview"));
    expect(fetchMock).toHaveBeenCalledWith(
      "/api/maintenance/import/preview?id=test%20%2F%20preview",
      expect.objectContaining({
        method: "DELETE",
        credentials: "same-origin",
        body: undefined,
      }),
    );
  });
  it("releases download URLs and never writes browser storage", () => {
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
    const storage = vi.spyOn(Storage.prototype, "setItem");
    const blob = new Blob([syntheticRawBackup]);
    downloadBackup(blob, "test-backup.json");
    expect(createObjectURL).toHaveBeenCalledWith(blob);
    expect(click).toHaveBeenCalledTimes(1);
    expect(revokeObjectURL).toHaveBeenCalledWith("blob:synthetic");
    expect(document.querySelector('a[download="test-backup.json"]')).toBeNull();
    expect(storage).not.toHaveBeenCalled();
  });
});
