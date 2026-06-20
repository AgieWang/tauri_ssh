import React from "react";
import ReactDOM from "react-dom/client";
import { getCurrentWindow, type Window } from "@tauri-apps/api/window";
import App from "./App";
import "./styles/global.css";

// ─── 窗口显示兜底机制 ────────────────────────
// WebView2 初始化可能阻塞事件循环，导致窗口不可见
// 使用多层兜底确保窗口最终可见（参考 tauri-cc 经验）
function getAppWindow(): Window | null {
  try {
    return getCurrentWindow();
  } catch {
    return null;
  }
}

const appWindow = getAppWindow();

// 立即尝试显示
appWindow?.show().catch(() => {});

// 500ms 重试
setTimeout(() => {
  appWindow?.show().catch(() => {});
}, 500);

// 5s 最终兜底
setTimeout(() => {
  appWindow?.show().catch(() => {});
}, 5000);

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
