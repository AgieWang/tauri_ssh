---
name: tauri-window-management
description: |
  用于 Tauri Window/WebviewWindow、窗口 label、无边框标题栏、系统托盘和原生窗口生命周期；浏览器测试窗口不触发。

  触发场景：
  - 创建或控制 Tauri `Window`/`WebviewWindow`
  - 配置无边框窗口、拖拽区、自定义标题栏或透明窗口
  - 实现系统托盘、最小化到托盘、关闭拦截或窗口恢复
  - 处理多窗口 label、权限、事件和生命周期

  触发词：WebviewWindow、tauri::Window、窗口 label、data-tauri-drag-region、无边框标题栏、system tray、最小化到托盘、窗口生命周期
---

# Tauri 窗口与托盘

## 边界

本技能只处理 Tauri 原生窗口。Control Chrome、Codex 内置浏览器、浏览器 tab/window、React 弹窗 Modal 和普通 CSS 布局不应触发。窗口间事件通信使用 `tauri-events`；窗口 Capability 差异使用 `tauri-capabilities`。

## 强制规则

1. 先读取 `tauri.conf.json`、窗口创建代码、Capabilities、路由和现有 label 约定。
2. label 必须唯一且稳定；创建、查找、关闭与权限配置使用同一标识。
3. 无边框标题栏必须保留拖动、最小化、最大化、关闭、键盘与可访问性路径。
4. 窗口事件监听必须清理；关闭/隐藏/托盘恢复要区分应用退出语义，避免后台幽灵进程。
5. 新窗口只获得完成职责所需的最小 Capability，不继承主窗口全部权限。
6. 平台特性必须验证目标系统；不把单平台成功表述为全平台通过。
7. `dragDropEnabled` 必须按产品交互选择：页内 HTML5/Ant Design 拖拽受 Tauri 原生文件拖入拦截时可设为 `false` 并重启 dev；需要操作系统文件拖入时不得无条件关闭。

## 执行流程

1. 定义窗口角色、label、URL/路由、生命周期、父子关系和权限。
2. 选择配置创建或运行时创建，复用当前项目模式。
3. 实现窗口控制、事件清理、重复打开去重和错误反馈。
4. 验证创建、聚焦、最小化、最大化、关闭、恢复、拖放、多显示器和异常路径。
5. 前端界面变更必须使用 Codex 内置浏览器或 Control Chrome；原生窗口行为还需真实 Tauri 运行时验证。

## 按需参考

需要窗口 JSON、Rust/TypeScript 创建、无边框标题栏、控制 API 或托盘示例时读取 [references/window-patterns.md](references/window-patterns.md)。

## 完成条件

- label、路由、生命周期、事件和权限映射一致。
- 无重复窗口、监听泄漏、关闭语义错误或权限扩大。
- 类型检查、Rust 检查、聚焦测试、构建与真实窗口行为验证通过。
- UTF-8 无 BOM，`git diff --check` 通过。
