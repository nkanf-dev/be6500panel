import { Schema } from "effect";
import { request } from "./api";

export const TerminalOpenResponseSchema = Schema.Struct({
  id: Schema.String,
  rows: Schema.Number,
  cols: Schema.Number,
  term: Schema.String,
  offset: Schema.Number,
  idleTimeoutSeconds: Schema.Number,
});

export const TerminalOutputResponseSchema = Schema.Struct({
  id: Schema.String,
  data: Schema.String, // standard padded base64
  offset: Schema.Number,
  nextOffset: Schema.Number,
  truncated: Schema.Boolean,
  state: Schema.Literal("open", "closing", "exited"),
  exitCode: Schema.NullOr(Schema.Number),
});

export const TerminalInputResponseSchema = Schema.Struct({
  accepted: Schema.Number,
});

export const TerminalResizeResponseSchema = Schema.Struct({
  rows: Schema.Number,
  cols: Schema.Number,
});

export const TerminalCloseResponseSchema = Schema.Struct({
  state: Schema.Literal("closing", "closed"),
});

export type TerminalOpenResponse = typeof TerminalOpenResponseSchema.Type;
export type TerminalOutputResponse = typeof TerminalOutputResponseSchema.Type;
export type TerminalInputResponse = typeof TerminalInputResponseSchema.Type;
export type TerminalResizeResponse = typeof TerminalResizeResponseSchema.Type;
export type TerminalCloseResponse = typeof TerminalCloseResponseSchema.Type;

export function bytesToBase64(bytes: Uint8Array): string {
  let binary = "";
  for (let i = 0; i < bytes.byteLength; i++) {
    binary += String.fromCharCode(bytes[i]);
  }
  return btoa(binary);
}

export function base64ToBytes(base64: string): Uint8Array {
  const binary = atob(base64);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) {
    bytes[i] = binary.charCodeAt(i);
  }
  return bytes;
}

export function textToBase64(text: string): string {
  return bytesToBase64(new TextEncoder().encode(text));
}

export function closeTerminalBeacon(id: string): boolean {
  if (typeof navigator !== "undefined" && navigator.sendBeacon) {
    const blob = new Blob([JSON.stringify({ id })], {
      type: "application/json",
    });
    return navigator.sendBeacon("/api/terminal/close", blob);
  }
  return false;
}

export const terminalApi = {
  open: (options: { rows?: number; cols?: number } = {}) =>
    request("/terminal/open", TerminalOpenResponseSchema, {
      method: "POST",
      body: options,
    }),

  output: (id: string, offset: number) =>
    request(
      `/terminal/output?id=${encodeURIComponent(id)}&offset=${offset}`,
      TerminalOutputResponseSchema,
    ),

  input: (id: string, dataBase64: string) =>
    request("/terminal/input", TerminalInputResponseSchema, {
      method: "POST",
      body: { id, data: dataBase64 },
    }),

  resize: (id: string, rows: number, cols: number) =>
    request("/terminal/resize", TerminalResizeResponseSchema, {
      method: "POST",
      body: { id, rows, cols },
    }),

  close: (id: string) =>
    request(
      `/terminal/close?id=${encodeURIComponent(id)}`,
      TerminalCloseResponseSchema,
      {
        method: "DELETE",
      },
    ),
};
