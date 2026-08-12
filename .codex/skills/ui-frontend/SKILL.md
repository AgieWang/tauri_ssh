---
name: ui-frontend
description: |
  用于实现或修改 tauri_ssh 的 React 页面和可交互组件，覆盖 Ant Design、TailwindCSS、路由、表单、表格、弹窗、可访问性、前端状态与浏览器验收。

  触发场景：
  - 新增或修改 React 页面、组件、路由或桌面布局
  - 实现 Ant Design Form/Table/Modal/Drawer 等交互
  - 修复页面 loading、空态、错误提示、响应式或可访问性问题

  不应触发：只改主题令牌/暗亮模式；仅 Rust Command；原生系统通知；普通 Markdown 页面或无 UI 的 TypeScript 工具。

  强触发词：React 页面、React 组件、Ant Design 表单、Ant Design 表格、Modal Drawer、前端交互、页面布局、浏览器验收
---

# React 前端 UI 开发

## 技术与边界

使用当前仓库的 React 19、TypeScript、Ant Design 6、TailwindCSS 4、React Router 7 和 Zustand 模式。版本必须以 `package.json` 和锁文件为准，升级后同步校正 API。

- 页面/组件交互由本 Skill 负责。
- IPC 契约使用 `api-development`；组件不得裸写 `invoke()`。
- 主题、暗亮模式、设计令牌和 `antdTheme` 使用 `theme-system`。
- 原生桌面通知使用 `notification-system`；页面内 antd `message` 属于本 Skill。

## 强制执行流程

1. 读取目标页面、一个相似页面、布局/路由、对应 API/类型和样式令牌，不按旧模板猜目录。
2. 明确用户流程、数据来源、loading/empty/error/success、权限和验收动作。
3. 设计组件职责与状态归属：局部状态用 Hooks，真正跨组件共享才用 Zustand。
4. 优先复用 Ant Design 组件；Tailwind 负责布局/间距，CSS Variables/antd token 负责主题颜色。
5. API 调用通过 `src/lib/api/<domain>.ts`；`catch` 接收 `unknown`，使用 `getErrorMessage(error)`。
6. 实现键盘操作、label、焦点、禁用/加载态、危险操作确认和可读反馈。
7. 格式化并运行聚焦测试、TypeScript 检查和构建。
8. **强制使用 Codex 内置浏览器或 Control Chrome with Codex 验收页面**；不能只以编译或截图代替交互。

## 核心规则

- 使用函数组件 + Hooks 和 `@/` 路径别名；禁止 `any`。
- 不在组件中从 `@tauri-apps/api/core` 直接调用 invoke；API/类型按业务模块拆分并统一导出。
- 数据请求必须处理竞态、重复提交和卸载后的状态更新；所有异步路径恢复 loading。
- 表单使用可见 label、明确校验和提交错误；删除/覆盖操作使用确认对话框。
- Table 提供稳定 `rowKey`、loading/empty，数据量大时考虑分页/虚拟化而非一次渲染全部。
- 使用 Ant Design `message`/`Alert`/`Result` 呈现错误，不用 `window.alert()`；外部打开使用 Tauri opener/窗口 API。
- 不硬编码主题颜色，不使用 Tailwind `dark:` 建立第二套主题；使用 `var(--token)` 或 Ant Design token。
- 桌面窗口尺寸下避免内容被截断；滚动区域归属明确，不能笼统禁止页面级滚动。
- 设置页是否用 Drawer、独立页面或现有模式，以当前产品结构和相似实现为准，不机械套旧模板。

## 按需读取 References

- Table/Form/Modal、CRUD 和错误状态：读取 [表格表单弹窗模式](references/table-form-modal.md)。
- 设置 Drawer、Tabs、Zustand/Store 和桌面布局：读取 [设置与布局模式](references/settings-and-layout.md)。
- 任何页面变更交付前：必须读取 [浏览器验收](references/browser-acceptance.md)。
- 涉及主题 token：再读取 `theme-system`；涉及端到端 IPC：再读取 `api-development`。

## 路由示例

| 请求 | 本 Skill | 组合/排除 |
|---|---|---|
| “新增 SSH 连接表单并调用已有 API” | 必选 | API 契约变更时加 `api-development` |
| “antd message 显示保存失败” | 必选 | 不选原生 `notification-system` |
| “修改 variables.css 的暗色品牌色” | 不选 | `theme-system` |
| “Rust Command 注入 State” | 不选 | `tauri-commands` |
| “Control Chrome 验收当前页面” | 必选 | 读取浏览器验收 reference |

## 交付证据

交付中区分静态检查、浏览器交互和真实 Tauri IPC 三类证据；截图只能证明当时画面，不能替代表单操作、控制台检查、失败态或原生权限验证。

页面被隐藏、路由不可达或依赖 mock 数据时，不得写成“验收通过”；应准确记录可达性、数据来源和未覆盖路径。

验收中不得展示或记录真实凭据。

## 完成条件

- [ ] 组件职责和状态归属清晰，API/类型未散落或重复。
- [ ] loading、empty、error、success、校验和危险确认完整。
- [ ] 键盘、焦点、label、对比度和缩放/窗口尺寸可用。
- [ ] 格式化、聚焦测试、`tsc --noEmit`、build 和 `git diff --check` 通过。
- [ ] 内置浏览器或 Chrome 完成真实交互与控制台验收，并记录证据和 Tauri 运行时限制。
