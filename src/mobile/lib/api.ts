// 移动端 API 客户端骨架
//
// 移动端不能 invoke()，所有桌面端能力通过 axum 远程网关 (HTTP/WS) 拉取。
// 网关骨架见 src-tauri/src/remote/，详细文档见 .claude/skills/remote-gateway/skill.md。

export interface RemoteEndpoint {
  baseUrl: string; // 例如 "http://192.168.1.100:7800/v1" 或反代后的 "https://example.com/v1"
  token: string | null; // 桌面端首次启动生成，由用户配到手机端
}

const STORAGE_KEY = "mobile.endpoint";

export function getEndpoint(): RemoteEndpoint {
  const raw = localStorage.getItem(STORAGE_KEY);
  if (raw) {
    try {
      return JSON.parse(raw) as RemoteEndpoint;
    } catch {
      // 解析失败时回退默认
    }
  }
  return { baseUrl: "", token: null };
}

export function setEndpoint(ep: RemoteEndpoint) {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(ep));
}

/**
 * 统一 fetch 封装 — 自动带 Bearer Token。
 * 失败抛 Error（含 HTTP 状态文本），由调用方决定如何展示。
 */
export async function apiFetch<T = unknown>(
  path: string,
  init: RequestInit = {},
): Promise<T> {
  const ep = getEndpoint();
  if (!ep.baseUrl) {
    throw new Error("尚未配置桌面端地址（请在设置中填写）");
  }

  const headers: Record<string, string> = {
    "Content-Type": "application/json",
    ...((init.headers as Record<string, string>) ?? {}),
  };
  if (ep.token) {
    headers["Authorization"] = `Bearer ${ep.token}`;
  }

  const url = `${ep.baseUrl.replace(/\/$/, "")}${path.startsWith("/") ? path : `/${path}`}`;
  const resp = await fetch(url, { ...init, headers });

  if (!resp.ok) {
    throw new Error(`HTTP ${resp.status} ${resp.statusText}`);
  }

  // 没有 body 的 204 直接返回 undefined
  if (resp.status === 204) return undefined as T;

  const ct = resp.headers.get("Content-Type") ?? "";
  return (
    ct.includes("application/json")
      ? await resp.json()
      : ((await resp.text()) as unknown)
  ) as T;
}

/** 健康探测 */
export function ping(): Promise<{ ok: boolean; version: string }> {
  return apiFetch("/ping");
}
