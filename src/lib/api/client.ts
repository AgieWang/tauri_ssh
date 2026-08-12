import { invoke } from "@tauri-apps/api/core";

const DEFAULT_DEV_API_BASE_URL = "http://127.0.0.1:17321/dev-api";

/**
 * 浏览器验收可覆盖本地 API 端口，以便与正在运行的正式版并行；生产构建忽略该变量，
 * 且仅接受回环 HTTP 地址，避免将 Dev API 请求导向局域网或公网。
 */
export function resolveDevApiBaseUrl(value?: string): string {
  if (!value) {
    return DEFAULT_DEV_API_BASE_URL;
  }
  try {
    const url = new URL(value);
    const path = url.pathname.replace(/\/$/, "");
    if (
      url.protocol !== "http:" ||
      !["127.0.0.1", "localhost", "[::1]"].includes(url.hostname) ||
      path !== "/dev-api" ||
      url.search ||
      url.hash
    ) {
      return DEFAULT_DEV_API_BASE_URL;
    }
    return `${url.origin}${path}`;
  } catch {
    return DEFAULT_DEV_API_BASE_URL;
  }
}

export const devApiBaseUrl = resolveDevApiBaseUrl(
  import.meta.env.DEV
    ? import.meta.env.VITE_TAURI_SSH_DEV_API_BASE_URL
    : undefined,
);

/** 后端结构化错误响应 */
export interface CommandError {
  code: string;
  message: string;
}

/**
 * 解析后端 CommandError 结构化错误
 * 如果是 JSON 格式则解析为 CommandError，否则返回纯文本错误
 */
export function parseCommandError(error: unknown): CommandError {
  if (
    typeof error === "object" &&
    error !== null &&
    "code" in error &&
    "message" in error
  ) {
    return error as CommandError;
  }
  const raw = typeof error === "string" ? error : String(error);
  try {
    const parsed = JSON.parse(raw);
    if (parsed.code && parsed.message) {
      return parsed as CommandError;
    }
  } catch {
    // 非 JSON 格式，降级为通用错误
  }
  return { code: "UNKNOWN", message: raw };
}

/**
 * 获取错误消息（用于 UI 展示）
 */
export function getErrorMessage(error: unknown): string {
  return parseCommandError(error).message;
}

/**
 * 获取错误码（用于程序判断）
 */
export function getErrorCode(error: unknown): string {
  return parseCommandError(error).code;
}

export function hasTauriRuntime() {
  return "__TAURI_INTERNALS__" in window;
}

async function parseHttpError(response: Response): Promise<CommandError> {
  const text = await response.text();
  if (text) {
    try {
      const parsed = JSON.parse(text);
      if (parsed.code && parsed.message) {
        return parsed as CommandError;
      }
    } catch {
      return { code: `HTTP_${response.status}`, message: text };
    }
  }
  return {
    code: `HTTP_${response.status}`,
    message: response.statusText || `HTTP ${response.status}`,
  };
}

export async function devApiFetch<T>(
  path: string,
  options: RequestInit = {},
): Promise<T> {
  const response = await fetch(`${devApiBaseUrl}${path}`, {
    ...options,
    headers: {
      "Content-Type": "application/json",
      ...(options.headers ?? {}),
    },
  });
  if (!response.ok) {
    throw await parseHttpError(response);
  }
  if (response.status === 204) {
    return undefined as T;
  }
  return response.json() as Promise<T>;
}

/**
 * 类型安全的 invoke 封装
 * 直接 re-export，方便 API 模块统一引用
 */
export { invoke };
