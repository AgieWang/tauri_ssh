# React 与 TypeScript 检查

## API 与类型

- Tauri `invoke` 统一封装在 `src/lib/api/`，组件不裸调用。
- API 参数、返回、错误结构与 Rust Command 对齐；不得用 `any` 掩盖不确定类型。
- 项目内部导入遵循当前 `@/` 别名约定；同目录相对导入是否允许以现有代码为准，不机械禁止所有相对路径。
- 错误提示保留用户可理解信息，日志不输出 Token、密码或私钥。

## React 与状态

- 使用函数组件和 Hooks；ErrorBoundary 等项目明确例外按现有模式处理。
- effect 的依赖、取消、竞态和 cleanup 正确；Tauri `listen/once` 在卸载时执行 unlisten。
- 组件局部状态使用 Hooks，全局共享状态按项目 Zustand store；后端持久数据不重复塞入 UI store。
- 异步请求具备 loading、empty、error 和重复触发行为，卸载后不更新失效状态。
- Ant Design 用于交互组件，Tailwind 用于布局，CSS Variables/antd token 用于主题；语义 HTML 和无障碍属性不能因组件库规则被误报。

## 环境边界

- WebView 不直接导入 Node `fs/path/http/child_process`，系统能力通过 Tauri API 或 Rust Command。
- 文件路径使用 Tauri path API 或后端解析，不硬编码个人绝对路径。
- 调试日志、临时开关和 mock 数据不进入生产路径。

## 建议验证

按项目现有脚本选择：

```bash
pnpm exec prettier --check <changed-files>
npx tsc --noEmit
pnpm vitest run <focused-target>
pnpm build
```

页面、组件或样式变更还必须使用 Codex 内置浏览器或 Control Chrome：

1. 打开真实开发地址和目标路由。
2. 验证主要操作、加载/空/错误态和键盘交互。
3. 检查控制台、网络/IPC 请求和布局溢出。
4. 保存必要截图或明确交互证据。

## 审查清单

- [ ] API 封装和 TS/Rust 类型一致。
- [ ] 无 `any`、未处理 Promise 或敏感日志。
- [ ] effect cleanup、依赖和竞态正确。
- [ ] 状态层级、UI 组件、主题和可访问性符合项目模式。
- [ ] 格式、类型、聚焦测试、构建已执行。
- [ ] 页面经强制浏览器验收且控制台无新增错误。
