// 移动端 WebSocket 客户端骨架（自动重连 + 心跳）
//
// 用法：
//   const ws = createMobileWS({ path: "/v1/pty", onMessage: console.log });
//   ws.send(...); ws.close();
//
// 弱网/切后台 → 自动 ping/pong + 断线重连（指数退避，封顶 30s）。

import { getEndpoint } from "./api";

export interface MobileWSOptions {
  path: string;
  onOpen?: () => void;
  onMessage?: (data: string | ArrayBuffer) => void;
  onClose?: (ev: CloseEvent) => void;
  onError?: (ev: Event) => void;
  pingIntervalMs?: number; // 默认 25_000
  maxBackoffMs?: number;   // 默认 30_000
}

export interface MobileWS {
  send: (data: string | ArrayBufferLike | Blob | ArrayBufferView) => void;
  close: () => void;
  isOpen: () => boolean;
}

export function createMobileWS(opt: MobileWSOptions): MobileWS {
  const { path, onOpen, onMessage, onClose, onError } = opt;
  const pingIntervalMs = opt.pingIntervalMs ?? 25_000;
  const maxBackoffMs = opt.maxBackoffMs ?? 30_000;

  let socket: WebSocket | null = null;
  let pingTimer: ReturnType<typeof setInterval> | null = null;
  let reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  let attempt = 0;
  let manuallyClosed = false;

  function buildUrl(): string {
    const ep = getEndpoint();
    if (!ep.baseUrl) throw new Error("尚未配置桌面端地址");
    // baseUrl 可能是 http(s)://... — 替换协议为 ws(s)
    const wsBase = ep.baseUrl.replace(/^http/i, "ws").replace(/\/$/, "");
    const fullPath = path.startsWith("/") ? path : `/${path}`;
    // Token 通过 query 传（WebSocket 没有 Authorization header）
    const sep = fullPath.includes("?") ? "&" : "?";
    return ep.token ? `${wsBase}${fullPath}${sep}token=${encodeURIComponent(ep.token)}` : `${wsBase}${fullPath}`;
  }

  function connect() {
    try {
      socket = new WebSocket(buildUrl());
    } catch (e) {
      scheduleReconnect();
      return;
    }

    socket.onopen = () => {
      attempt = 0;
      onOpen?.();
      pingTimer = setInterval(() => {
        if (socket?.readyState === WebSocket.OPEN) socket.send("ping");
      }, pingIntervalMs);
    };

    socket.onmessage = (ev) => onMessage?.(ev.data);
    socket.onerror = (ev) => onError?.(ev);

    socket.onclose = (ev) => {
      cleanupTimers();
      onClose?.(ev);
      if (!manuallyClosed) scheduleReconnect();
    };
  }

  function scheduleReconnect() {
    cleanupTimers();
    const delay = Math.min(maxBackoffMs, 1000 * Math.pow(2, attempt));
    attempt += 1;
    reconnectTimer = setTimeout(connect, delay);
  }

  function cleanupTimers() {
    if (pingTimer) {
      clearInterval(pingTimer);
      pingTimer = null;
    }
    if (reconnectTimer) {
      clearTimeout(reconnectTimer);
      reconnectTimer = null;
    }
  }

  connect();

  return {
    send: (data) => {
      if (socket?.readyState === WebSocket.OPEN) socket.send(data);
    },
    close: () => {
      manuallyClosed = true;
      cleanupTimers();
      socket?.close();
    },
    isOpen: () => socket?.readyState === WebSocket.OPEN,
  };
}
