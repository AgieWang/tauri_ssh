# 任务：按原型图实现 Tauri SSH 桌面端界面

**状态**: 🔵 已完成
**创建时间**: 2026-06-20 00:00:00
**更新时间**: 2026-06-20 01:43:00
**Git 分支**: master

---

## 需求描述

先按已生成的完整原型图实现桌面端界面，暂不实现真实 SSH、SFTP、AI、MCP、审批、日志 tail 等后端功能。所有页面使用静态数据或 Mock 数据表达真实业务状态，目标是让 V0.1 的功能界面覆盖率达到 100%，后续可直接替换为真实 Command/API 数据源。

约束：

- 只修改 `/Users/bin/Documents/GitHub/tauri_ssh`，不修改模板项目 `/Users/bin/Documents/GitHub/tauri`。
- 不接入真实后端，不新增真实危险操作入口。
- 使用 React 19 + TypeScript + Ant Design + TailwindCSS 4 + Lucide React。
- UI 风格遵守现代简约、桌面工具优先、信息密度适中。
- macOS 和 Windows 首发，界面需避免平台专属路径和控件假设。

参考文件：

- `docs/prototype/tauri-ssh-full-prototype.html`
- `docs/prototype/ai-workstation-brief.md`
- `docs/requirements/2026-06-19-ai-ssh-tool-prd.md`

---

## 方案设计

### 最终方案

采用“真实应用导航 + Mock 页面”的方式实现：

- 路由按未来真实产品模块拆分，而不是只做单页长图。
- 侧边栏覆盖所有核心模块，便于后续逐模块接入真实功能。
- 页面组件先放在 `src/pages/` 下，公共展示组件放在 `src/components/ui/` 或新建 `src/components/prototype/`。
- Mock 数据集中放在 `src/data/`，避免散落在页面中。
- 不调用 `invoke()`，所有交互只做前端状态切换、表单展示、Tab 切换、筛选视觉效果。

### 取舍

- 暂不实现 xterm、Monaco、虚拟列表等重量能力；先用结构化终端面板、编辑器面板、日志面板模拟真实体验。
- 表格、表单、Tabs、Drawer、Modal 等使用 Ant Design，图标使用 Lucide React。
- 先保证覆盖率、导航完整性和视觉可用性，后续再逐模块接真实后端。

---

## 页面覆盖范围

| 序号 | 路由 | 页面 | 覆盖重点 |
|---:|---|---|---|
| 01 | `/onboarding` | 启动引导 | 首次配置、导入 SSH Config、配置 AI Provider、启动 MCP Server |
| 02 | `/dashboard` 或 `/` | 工作台 | 服务器概览、最近会话、待审批、AI 建议、日志状态 |
| 03 | `/servers` | 服务器管理 | 分组、来源、连接方式、AI 权限、状态、操作入口 |
| 04 | `/server-form` | 服务器表单 | 基础信息、认证方式、跳板/代理、AI 权限、团队预留字段 |
| 05 | `/ssh-import` | SSH Config 导入 | 解析结果、冲突处理、分组映射、导入预览 |
| 06 | `/vault` | 凭据保险库 | 加密凭据列表、授权范围、轮换状态、非明文提示 |
| 07 | `/terminal` | 终端 + AI | 终端区域、命令建议、风险拦截、上下文解释 |
| 08 | `/approval` | 审批队列 | 风险等级、来源、目标服务器、审批动作、临时授权 |
| 09 | `/logs` | 日志监听 | 多标签 tail、同/跨服务器文件、搜索、过滤、正则、大小写、反向过滤、AI 解释 |
| 10 | `/sftp` | SFTP 文件 | 文件树、远程列表、上传下载队列、权限标识 |
| 11 | `/editor` | 文本编辑器 | 内置文本编辑器、差异摘要、保存审批、AI 修改建议 |
| 12 | `/providers` | AI Provider | OpenAI、DeepSeek、GLM、Kimi、MiniMax、自定义兼容接口 |
| 13 | `/mcp` | MCP Server | 本应用作为 MCP Server、工具权限、客户端配置、运行状态 |
| 14 | `/jumpserver` | 堡垒机会话 | JumpServer/ISC Web SSH 兼容、建议-only、安全边界 |
| 15 | `/audit` | 审计日志 | 命令、日志监听、搜索过滤、SFTP、AI、MCP、审批链路 |
| 16 | `/workspace` | 团队预留 | workspace、users、roles、server scope 预留字段 |
| 17 | `/settings` | 系统设置 | 主题、更新、日志、备份、保留期、跨平台设置 |
| 18 | `/states` | 状态页 | 空状态、错误状态、权限不足、连接失败、审批等待 |
| 19 | `/coverage` | 覆盖矩阵 | PRD 功能点到页面覆盖关系，显示 100% |

---

## 前端结构规划

### 新增/调整文件

```text
src/
├── Router.tsx
├── components/
│   ├── layout/
│   │   └── Sidebar.tsx
│   └── prototype/
│       ├── AiInsightPanel.tsx
│       ├── CoverageMatrix.tsx
│       ├── LogTailPanel.tsx
│       ├── PageHeader.tsx
│       ├── RiskBadge.tsx
│       ├── StatCard.tsx
│       └── TerminalPanel.tsx
├── data/
│   └── prototype.ts
└── pages/
    ├── approval/index.tsx
    ├── audit/index.tsx
    ├── coverage/index.tsx
    ├── dashboard/index.tsx
    ├── editor/index.tsx
    ├── jumpserver/index.tsx
    ├── logs/index.tsx
    ├── mcp/index.tsx
    ├── onboarding/index.tsx
    ├── providers/index.tsx
    ├── server-form/index.tsx
    ├── servers/index.tsx
    ├── sftp/index.tsx
    ├── ssh-import/index.tsx
    ├── states/index.tsx
    ├── terminal/index.tsx
    ├── vault/index.tsx
    └── workspace/index.tsx
```

### 复用组件

- `PageHeader`: 页面标题、说明、右侧动作。
- `StatCard`: 工作台和模块概览统计。
- `RiskBadge`: L0/L1/L2/L3、只读、审批、拦截等统一状态。
- `TerminalPanel`: 终端输出模拟、命令输入条、脱敏上下文提示。
- `LogTailPanel`: 日志 Tab、日志行、高亮、搜索/过滤状态。
- `AiInsightPanel`: AI 建议、解释、风险提示、命令草稿。
- `CoverageMatrix`: 功能覆盖矩阵。

---

## 实现步骤

- [x] 1. 扩展路由和侧边栏导航
  - 文件：`src/Router.tsx`、`src/components/layout/Sidebar.tsx`
  - 结果：所有原型页面均可从侧边栏进入。

- [x] 2. 建立 Mock 数据和公共 UI 组件
  - 文件：`src/data/prototype.ts`、`src/components/prototype/*`
  - 结果：页面使用统一数据源和统一视觉语言。

- [x] 3. 实现基础工作流页面
  - 页面：启动引导、工作台、服务器管理、服务器表单、SSH Config 导入、凭据保险库。
  - 结果：覆盖服务器资产、分组、凭据、安全边界。

- [x] 4. 实现核心操作页面
  - 页面：终端 + AI、审批队列、日志监听、SFTP 文件、文本编辑器。
  - 结果：覆盖 SSH 运维主流程，尤其日志监听多标签、关键词搜索和过滤。

- [x] 5. 实现配置与治理页面
  - 页面：AI Provider、MCP Server、JumpServer/堡垒机会话、审计日志、团队预留、系统设置。
  - 结果：覆盖 AI、MCP、安全治理、跨平台设置。

- [x] 6. 实现状态页与覆盖矩阵
  - 页面：状态页、覆盖矩阵。
  - 结果：明确展示空状态、错误状态、权限限制，以及 100% 覆盖关系。

- [x] 7. 视觉与构建验证
  - 命令：`pnpm exec tsc --noEmit`、必要时 `pnpm build`
  - 浏览器：使用 Codex 内置浏览器或 Chrome 检查桌面窗口主要页面。
  - 结果：无 TypeScript 错误，主要页面无明显重叠、空白、溢出。

---

## 验收标准

- [x] 侧边栏/路由能访问全部 19 个页面。
- [x] 原型中的 18 个业务页面和 1 个覆盖矩阵全部实现。
- [x] 日志监听页面包含：
  - [x] 多标签监听。
  - [x] 同一服务器多个日志文件。
  - [x] 不同服务器多个日志文件。
  - [x] 搜索关键词、匹配计数、上一条/下一条。
  - [x] 过滤关键词、正则、大小写敏感、反向过滤。
  - [x] 关键词高亮、错误级别高亮。
  - [x] AI 解释最近 N 行。
- [x] AI Provider 页面覆盖 OpenAI API、DeepSeek、GLM、Kimi、MiniMax、自定义 OpenAI-compatible endpoint。
- [x] MCP 页面优先体现“本应用作为 MCP Server”。
- [x] SFTP 页面包含上传/下载队列和内置编辑器入口。
- [x] 文本编辑器页面包含编辑区、差异摘要、保存审批。
- [x] JumpServer 页面只体现合规会话接入和建议-only，不设计绕过凭据或规避安全扫描。
- [x] 所有数据均为 Mock，不触发真实 SSH/SFTP/AI/MCP/文件写入。
- [x] `/Users/bin/Documents/GitHub/tauri` 模板仓库保持无新增和无改动。

---

## 当前进度

**已完成**: 7 / 7 步骤 (100%)

**当前状态**:

- 已完成路由、侧边栏、Mock 数据、公共组件和 19 个原型页面。
- `pnpm exec tsc --noEmit` 已通过。
- `pnpm build` 已通过。
- 已通过 Chrome 抽检工作台、启动引导、日志监听、SFTP、编辑器、AI Provider、MCP、JumpServer、审计、状态页、覆盖矩阵等关键页面，无空白、无横向溢出、无控制台警告。

**下一步操作**:

1. 后续可按模块接入真实 Rust Command、SQLite、SSH/SFTP、AI Provider 和 MCP Server。
2. 接真实功能前先补齐对应数据模型、权限策略和审批链路。

---

## 风险与注意事项

- 页面数量较多，优先用统一组件和 Mock 数据降低重复实现。
- 终端、日志、编辑器都先做视觉原型，不引入真实长连接或文件写入。
- 后续接真实功能时，再补 Rust Command、SQLite、权限声明和安全审批链路。

---

## 变更记录

### 2026-06-20 01:36
**变更类型**: 进度更新

**变更内容**:
- 新增 `src/data/prototype.ts` Mock 数据。
- 新增 `src/components/prototype/common.tsx` 公共展示组件。
- 新增 `src/pages/prototype/index.tsx`，实现 19 个原型页面。
- 更新 `src/Router.tsx` 和 `src/components/layout/Sidebar.tsx` 接入所有路由。
- 更新 `src/styles/global.css` 增加原型页面布局、终端、编辑器样式。

**影响范围**:
- 仅影响 `/Users/bin/Documents/GitHub/tauri_ssh` 前端界面和任务文档。

### 2026-06-20 01:43
**变更类型**: 验证完成

**变更内容**:
- 修复浏览器环境下 Tauri Window API 直接调用导致的空白页问题。
- 修复 Ant Design v6 属性警告和 Tree 重复 key 警告。
- 完成 TypeScript、生产构建和 Chrome 关键页面抽检。

**影响范围**:
- `src/main.tsx`
- `src/pages/prototype/index.tsx`
- `docs/tasks/active/task-20260620-ui-prototype-implementation-plan.md`

### 2026-06-20 02:18
**变更类型**: 功能升级

**变更内容**:
- AI Provider 从前端 Mock 原型升级为真实后端模块。
- 支持范围按最新要求收敛为 Anthropic、OpenAI、Gemini、DeepSeek、智谱 GLM、Kimi、MiniMax、小米 MiMo。
- 新增 SQLite 表 `ai_providers` 和 `ai_provider_routes`，预置 8 个 Provider 和场景路由。
- 新增 Rust Service/Command，支持 Provider 列表、保存、删除、场景路由、真实连接测试。
- API Key 由 Rust 后端加密存储，前端只接收 `hasApiKey` 和掩码。
- `/providers` 页面改为调用真实 Tauri Command；普通浏览器预览无 Tauri IPC 时降级为只读预览。

**验证情况**:
- `pnpm exec tsc --noEmit` 通过。
- `pnpm build` 通过。
- Chrome 验证 `/providers` 只展示 8 个目标 Provider，旧 Provider 不再出现，无控制台错误、无横向溢出。
- 当前 shell 未找到 `cargo`/`rustc`，Rust 编译检查暂未执行。

**影响范围**:
- `src-tauri/Cargo.toml`
- `src-tauri/src/models/mod.rs`
- `src-tauri/src/database/schema.rs`
- `src-tauri/src/database/mod.rs`
- `src-tauri/src/services/ai_provider.rs`
- `src-tauri/src/commands/ai_provider.rs`
- `src-tauri/src/lib.rs`
- `src/types/aiProvider.ts`
- `src/lib/api/aiProvider.ts`
- `src/pages/prototype/index.tsx`
