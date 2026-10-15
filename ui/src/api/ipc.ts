import { invoke } from "@tauri-apps/api/core";
import type { ErrorPayload } from "../bindings/ErrorPayload";

export class IpcError extends Error {
  readonly payload: ErrorPayload;
  constructor(payload: ErrorPayload) {
    super(payload.code);
    this.payload = payload;
  }
}

function isErrorPayload(value: unknown): value is ErrorPayload {
  if (typeof value !== "object" || value === null) return false;
  const v = value as Record<string, unknown>;
  return typeof v.code === "string" && typeof v.params === "object" && v.params !== null;
}

/** Rust から返ったエラーの形を実行時に確かめる。形が違えば app.unexpected にする。 */
export function toErrorPayload(error: unknown): ErrorPayload {
  if (error instanceof IpcError) return error.payload;
  if (isErrorPayload(error)) return error;
  return { code: "app.unexpected", params: {}, detail: String(error) };
}

export async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(cmd, args);
  } catch (error) {
    throw new IpcError(toErrorPayload(error));
  }
}
