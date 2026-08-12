# 前端浏览器强制验收

## 1. 工具顺序

所有前端页面测试必须使用以下任一工具，优先顺序：

1. Codex 内置浏览器（`browser:control-in-app-browser`）。
2. Control Chrome with Codex（`chrome:control-chrome`）。

若二者不可用，明确记录阻塞，不能用“构建通过”宣称页面已验收。Playwright 只在用户要求或既定 E2E 流程需要时补充，不能替代项目规定的浏览器工具。

通常访问当前开发服务；仓库默认常见地址为 `http://localhost:1422`，但先读取运行输出确认端口，禁止凭文档杀端口或启动重复 dev server。

## 2. 验收前

- 确认当前服务和端口属于本任务，遵守多会话避让，禁止 kill 其他会话进程。
- 读取路由入口、登录/前置数据和 Tauri runtime 限制。
- 准备最小非敏感测试数据；不得在截图或控制台暴露凭据。
- 列出本次变更对应的用户动作和预期结果。

## 3. 必测路径

### 页面与布局

- 从真实导航进入页面，刷新/HashRouter 路径可恢复。
- 常用和最小窗口尺寸无关键内容遮挡、不可达滚动或弹窗溢出。
- 暗色/亮色（若受影响）对比度和颜色无硬编码异常。

### 交互

- 表单必填、格式、边界值和服务端错误。
- loading 时防重复提交，成功后状态更新正确。
- Table 空态、分页/筛选（如有）、编辑与删除确认。
- Modal/Drawer 打开、关闭、Esc、焦点和取消路径。
- 含输入/多步操作的 Modal/Drawer 点击遮罩不会误关；纯查看浮层仍符合预期。
- 键盘 Tab 顺序、Enter/Space 激活、仅图标按钮的可访问名称。

### API 与错误

- 请求 payload 与 IPC 契约一致，成功结果正确渲染。
- 后端失败展示 `getErrorMessage` 的可读内容，不出现 `[object Object]`。
- 无未处理 Promise、React key/hook 警告、资源 404、CSP/Capabilities 错误。
- 本地 asset/PDF/HTML iframe 在目标 WebView2 与严格 CSP 下可以加载；若平台可能静默拦截，提供系统应用打开兜底，不能只依赖 iframe `onError`。

## 4. Tauri 与浏览器边界

纯浏览器可能使用 dev API fallback 或缺少 `__TAURI_INTERNALS__`。分别记录：

- 浏览器层验证了布局和 Web 交互；
- Tauri runtime 层是否实际调用 Command/插件；
- 未验证的原生窗口、文件、通知、shell 或 Capabilities 行为。

不要把 HTTP 200、mock 数据或截图当作真实 IPC/生产验收。

## 5. 证据记录

交付时简洁记录：

- 使用的浏览器工具、地址和页面；
- 执行的关键动作及结果；
- 控制台是否干净；
- 必要截图/日志路径（不得含敏感数据）；
- Tauri 原生行为是否另行验证；
- 环境阻塞和剩余风险。
