import { invoke } from "@tauri-apps/api/core";

import type { ApiError } from "$lib/api/types";

export function normalizeApiError(e: unknown): ApiError {
  if (typeof e === "object" && e !== null) {
    const maybe = e as Record<string, unknown>;
    if (typeof maybe.code === "string" && typeof maybe.message === "string") {
      return {
        code: maybe.code,
        message: maybe.message,
        details: maybe.details,
      };
    }
  }

  if (typeof e === "string") {
    return { code: "unknown", message: e };
  }

  try {
    return { code: "unknown", message: JSON.stringify(e) };
  } catch {
    return { code: "unknown", message: String(e) };
  }
}

export async function invokeApi<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return (await invoke(command, args ?? {})) as T;
  } catch (e) {
    throw normalizeApiError(e);
  }
}
