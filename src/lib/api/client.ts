import { invoke } from "@tauri-apps/api/core";

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

/**
 * 类型安全的 invoke 封装
 * 直接 re-export，方便 API 模块统一引用
 */
export { invoke };
