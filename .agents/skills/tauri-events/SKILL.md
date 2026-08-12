---
name: tauri-events
description: |
  用于实现 Tauri `emit`/`listen`、`Emitter`、`EventTarget`、窗口定向事件和 IPC 进度事件；仅对 Tauri 发布订阅通信触发。

  触发场景：
  - Rust 使用 `Emitter` 向 WebView 推送进度或状态
  - 前端使用 Tauri `listen`/`emit` 订阅或发送事件
  - 在多个 Tauri Window/WebviewWindow 之间定向通信
  - 设计事件载荷、生命周期、取消订阅和错误传播

  触发词：tauri::Emitter、@tauri-apps/api/event、emit_to、EventTarget、Tauri listen、窗口事件、IPC 进度事件、unlisten
---

# Tauri 事件通信

## 边界

本技能处理 Tauri 发布订阅事件。请求—响应接口和类型契约使用 `api-development`；Command 高级注入使用 `tauri-commands`；macOS/Windows 原生桌面通知使用 `notification-system`；普通 React state 更新、Ant Design message 和浏览器 DOM 事件不应触发。

## 强制规则

1. 先确认事件是否必要：需要返回值或明确失败语义时优先 Command；事件用于进度、广播和异步通知。
2. 事件名使用稳定、可检索的 kebab-case；载荷定义 Rust/TypeScript 对齐的明确类型，禁止 `any`。
3. Rust `emit` 失败必须传播、记录或按业务语义处理，禁止 `unwrap()` 和静默丢弃。
4. React 监听必须保存并调用 `unlisten`；处理组件卸载、重复订阅和异步竞态。
5. 默认使用最窄目标：明确窗口时用定向事件，只有确需所有监听者时才广播。
6. 载荷视为跨进程输入，涉及敏感信息或外部数据时同时应用 `security-permissions`。

## 执行流程

1. 定义方向、触发者、接收者、事件名、载荷、频率和结束/取消语义。
2. 对照现有事件模式与 TypeScript 类型，避免同义事件或不兼容载荷。
3. 实现发送、订阅、清理、错误与背压/节流；高频事件避免无界广播。
4. 测试一次、重复、取消、窗口关闭、发送失败和乱序场景。
5. 页面相关变更使用 Codex 内置浏览器或 Control Chrome 验证实际交互。

## 按需参考

需要 Rust/前端双向示例、窗口通信或进度模式时读取 [references/event-patterns.md](references/event-patterns.md)。不要为普通通知或状态更新加载该长参考。

## 完成条件

- 事件边界、类型、目标和生命周期明确。
- 无 `unwrap()`、泄漏监听器、重复订阅或敏感载荷泄露。
- Rust/TypeScript 检查、聚焦测试、构建及真实页面/窗口验证通过。
- UTF-8 无 BOM，`git diff --check` 通过。
