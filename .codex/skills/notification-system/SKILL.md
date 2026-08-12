---
name: notification-system
description: |
  用于通过 tauri-plugin-notification 发送原生桌面通知。

  触发场景：
  - 发送 macOS、Windows 或 Linux 系统通知
  - 后台任务在应用不聚焦时提醒用户
  - 配置通知权限、频率、点击行为和 Capabilities

  不应触发：Ant Design message/notification、页面 Toast、表单反馈、Tauri emit/listen 事件。

  触发词：tauri-plugin-notification、系统通知、桌面通知、原生通知、notification permission、sendNotification、NotificationExt
---

# Tauri 原生系统通知

## 适用边界

本 Skill 只处理系统通知中心可见的通知。页面反馈使用 Ant Design，由 `ui-frontend` 处理；进程/窗口通信使用 `tauri-events`。

| 需求 | 归属 |
|---|---|
| 保存成功、表单错误、页面提示 | `ui-frontend` + Ant Design message/notification |
| Rust 向前端推送进度 | `tauri-events` |
| 应用退到后台后提醒任务完成 | 本 Skill |
| 定时业务提醒且显示在系统通知中心 | 本 Skill |

## 强制流程

1. 仅对重要、用户可理解且低频的事件使用原生通知。
2. 读取当前插件注册和 `capabilities/*.json`，使用最小 `notification` 权限。
3. 发送前检查权限；只在清晰的用户操作中请求，并处理拒绝/不可用。
4. 锁屏正文不得包含密码、令牌、连接串、敏感路径或业务敏感信息。
5. 为重复任务设置去重、节流或聚合规则，避免通知风暴。
6. 通知失败不能阻断核心业务；记录错误并提供应用内反馈。

插件注册、权限检查、TypeScript/Rust 发送模式见 [native-notification-patterns.md](references/native-notification-patterns.md)。

## 平台要求

- 浏览器不能证明原生通知可用；需在目标桌面系统验证。
- 点击或深链只允许白名单目标，并验证未启动、后台和前台状态。
- 通知遵循 i18n；测试不得向真实用户无限发送。

## 不应触发示例

- “保存后显示一条 antd message.success”。
- “页面右上角显示 notification.open”。
- “Rust emit 进度给 React”。

## 完成条件

- 插件注册、Capabilities、权限申请与拒绝路径完整。
- 频率、去重、敏感内容和平台差异已处理。
- 桌面环境已验证允许、拒绝、前台、后台、失败和页面降级。
- Rust/TypeScript 格式化、聚焦测试与 `git diff --check` 通过。
