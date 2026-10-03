import { Effect, Schema } from "effect";
import { ApiError } from "../../lib/api";
import {
  ImportPreviewSchema,
  ImportStageSchema,
  MAX_BACKUP_BYTES,
  type BackupScope,
  type ImportStageInput,
} from "./contracts";

const asApiError = (cause: unknown) =>
  cause instanceof ApiError
    ? cause
    : new ApiError({
        code: "network_error",
        message: cause instanceof Error ? cause.message : "无法连接控制服务",
      });

/** Raw upload and download must not pass through JSON.stringify request bodies. */
async function rawRequest(
  path: string,
  body: string | undefined,
  signal: AbortSignal,
  method: "POST" | "DELETE" = "POST",
  timeoutMs = 30_000,
): Promise<Response> {
  const response = await fetch(`/api${path}`, {
    method,
    credentials: "same-origin",
    cache: "no-store",
    headers: { Accept: "application/json", "Content-Type": "application/json" },
    body,
    signal: AbortSignal.any([signal, AbortSignal.timeout(timeoutMs)]),
  });
  if (!response.ok) {
    // Session expiry must propagate even if an upstream proxy sends invalid JSON.
    if (response.status === 401)
      window.dispatchEvent(new Event("be6500panel:unauthorized"));
    let envelope: {
      error?: { code?: unknown; message?: unknown };
      retainedDraftIds?: unknown;
    } = {};
    try {
      const value: unknown = await response.json();
      if (typeof value === "object" && value !== null) envelope = value;
    } catch {
      // Keep the HTTP status and unauthorized behavior, without retaining the body.
    }
    const error = new ApiError({
      status: response.status,
      code:
        typeof envelope.error?.code === "string"
          ? envelope.error.code
          : "http_error",
      message:
        typeof envelope.error?.message === "string"
          ? envelope.error.message
          : `请求失败（HTTP ${response.status}）`,
    });
    if (Array.isArray(envelope.retainedDraftIds)) {
      Object.assign(error, {
        retainedDraftIds: envelope.retainedDraftIds.filter(
          (id): id is string => typeof id === "string",
        ),
      });
    }
    throw error;
  }
  return response;
}

function backupFilename(disposition: string | null) {
  const name = disposition?.match(/filename="([^"\\]+)"/i)?.[1];
  if (name && /^[a-zA-Z0-9._-]+\.json$/.test(name)) return name;
  return `be6500panel-backup-${new Date().toISOString().replace(/[:.]/g, "-")}.json`;
}

export const maintenanceApi = {
  backup: (scopes: readonly BackupScope[]) =>
    Effect.tryPromise({
      try: async (signal) => {
        const response = await rawRequest(
          "/maintenance/backup",
          JSON.stringify({ scopes }),
          signal,
        );
        if (
          response.headers
            .get("Content-Type")
            ?.split(";")[0]
            .trim()
            .toLowerCase() !== "application/json"
        )
          throw new ApiError({
            code: "invalid_response",
            message: "备份响应不是 JSON 文件",
          });
        const blob = await response.blob();
        assertBackupSize(blob.size);
        return {
          blob,
          filename: backupFilename(response.headers.get("Content-Disposition")),
        };
      },
      catch: asApiError,
    }),
  preview: (rawText: string) =>
    Effect.tryPromise({
      try: async (signal) => {
        assertBackupSize(new TextEncoder().encode(rawText).byteLength);
        const response = await rawRequest(
          "/maintenance/import/preview",
          rawText,
          signal,
        );
        try {
          return Schema.decodeUnknownSync(ImportPreviewSchema)(
            await response.json(),
          );
        } catch {
          throw new ApiError({
            code: "invalid_response",
            message: "导入预览响应与 API 合同不符，请检查服务版本",
          });
        }
      },
      catch: asApiError,
    }),
  // This boundary keeps cleanup IDs which the shared request ApiError omits.
  stage: (body: ImportStageInput) =>
    Effect.tryPromise({
      try: async (signal) => {
        const response = await rawRequest(
          "/maintenance/import/stage",
          JSON.stringify(body),
          signal,
          "POST",
          90_000,
        );
        try {
          return Schema.decodeUnknownSync(ImportStageSchema)(
            await response.json(),
          );
        } catch {
          throw new ApiError({
            code: "invalid_response",
            message: "暂存响应与 API 合同不符，请到配置队列核对结果",
          });
        }
      },
      catch: asApiError,
    }),
  discard: (id: string) =>
    Effect.tryPromise({
      try: async (signal) => {
        await rawRequest(
          `/maintenance/import/preview?id=${encodeURIComponent(id)}`,
          undefined,
          signal,
          "DELETE",
        );
      },
      catch: asApiError,
    }),
};

export function assertBackupSize(bytes: number) {
  if (bytes > MAX_BACKUP_BYTES)
    throw new ApiError({
      code: "backup_too_large",
      message: "备份文件不能超过 2 MiB",
    });
  if (bytes === 0)
    throw new ApiError({
      code: "backup_empty",
      message: "请选择非空 JSON 备份文件",
    });
}

/** Read bytes before decoding. Preserve BOM, whitespace and duplicate JSON keys. */
export async function readBackupFile(file: File, signal: AbortSignal) {
  assertBackupSize(file.size);
  const bytes = await new Promise<ArrayBuffer>((resolve, reject) => {
    const reader = new FileReader();
    const abort = () => {
      reader.abort();
      reject(new DOMException("Aborted", "AbortError"));
    };
    const clear = () => signal.removeEventListener("abort", abort);
    reader.onload = () => {
      clear();
      if (reader.result instanceof ArrayBuffer) resolve(reader.result);
      else
        reject(
          new ApiError({
            code: "backup_read_failed",
            message: "无法读取备份文件",
          }),
        );
    };
    reader.onerror = () => {
      clear();
      reject(
        new ApiError({
          code: "backup_read_failed",
          message: "无法读取备份文件",
        }),
      );
    };
    reader.onabort = () => {
      clear();
      reject(new DOMException("Aborted", "AbortError"));
    };
    if (signal.aborted) return abort();
    signal.addEventListener("abort", abort, { once: true });
    reader.readAsArrayBuffer(file);
  });
  assertBackupSize(bytes.byteLength);
  if (signal.aborted) throw new DOMException("Aborted", "AbortError");
  try {
    return new TextDecoder("utf-8", { fatal: true, ignoreBOM: true }).decode(
      bytes,
    );
  } catch {
    throw new ApiError({
      code: "backup_encoding",
      message: "备份文件必须是 UTF-8 JSON",
    });
  }
}

/** A manual export only. No browser storage or cached copies; release the URL. */
export function downloadBackup(blob: Blob, filename: string) {
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  try {
    anchor.href = url;
    anchor.download = filename;
    anchor.hidden = true;
    document.body.append(anchor);
    anchor.click();
  } finally {
    anchor.remove();
    URL.revokeObjectURL(url);
  }
}

export function retainedDraftIds(error: unknown): readonly string[] {
  if (!(error instanceof ApiError)) return [];
  const ids = (error as ApiError & { retainedDraftIds?: unknown })
    .retainedDraftIds;
  return Array.isArray(ids)
    ? ids.filter((id): id is string => typeof id === "string")
    : [];
}
