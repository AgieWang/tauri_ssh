---
name: check
description: |
  编排 Tauri SSH 项目的显式代码规范与交付检查工作流，按实际变更选择 Rust、React/TypeScript、Tauri 配置、安全和运行时验证。仅在用户明确运行 `/check`、要求完整规范检查或交付门禁报告时使用；普通代码审查、单个报错排查或实现任务的常规验证不触发。
---

# 全栈检查编排

## 目标

根据实际变更文件和风险生成最小但完整的检查矩阵，执行格式化、静态检查、聚焦测试、构建和必要的真实运行验收。检查结果必须有命令、文件与运行时证据，不以关键词匹配或单次构建替代完整验证。

## 触发与范围

这是显式工作流：

- `/check`：检查当前相关变更的全栈门禁。
- `/check rust`：聚焦 Rust，但仍补充被 Rust 契约影响的前端或配置检查。
- `/check react`：聚焦 React/TypeScript；页面变更仍必须浏览器验收。
- 用户明确要求“完整规范检查/交付门禁报告”时按同样流程执行。

普通“帮我看一下代码”、单个编译错误或实现后的常规测试不自动加载本 Skill；分别使用代码审查、构建诊断或验证矩阵即可。

## 强制原则

- 先读取仓库约束、`git status -s`、当前分支和实际 diff，保护其他会话未提交工作。
- 检查标准来自当前参考代码、配置和项目规则，不机械套用过期示例。
- 按变更文件触发验证，不依赖路由是否选中某个领域 Skill。
- 不把格式化、类型检查、测试、构建、数据库、页面或安全验收互相替代。
- 不在检查任务中默认修复代码；用户只要求检查时输出发现和证据。
- 不执行 stash、reset、跨分支 checkout、全量暂存、kill 端口或外部写入。
- 所有文本检查 UTF-8 无 BOM、乱码和 `git diff --check`。

## 执行流程

### 1. 解析变更

1. 获取 `git status -s`、`git diff --name-only` 和相关已暂存差异。
2. 将文件映射到 `.codex/tests/skill-routing/expected-matrix.json`；若矩阵缺失，按下列引用保守选择。
3. 识别跨层契约：Rust Command、TypeScript API/types、数据库 schema、Capabilities、插件、页面和发布配置。
4. 区分本次相关变更与其他会话 WIP，只检查相关文件；全量检查也不得修改无关文件。

### 2. 静态规范审查

按领域读取：

- Rust： [rust-checks.md](references/rust-checks.md)
- React/TypeScript： [frontend-checks.md](references/frontend-checks.md)
- Tauri 配置、Capabilities、插件： [tauri-config-checks.md](references/tauri-config-checks.md)

同时核对真实架构模式：Command → Service → Database、统一错误边界、IPC 注册、TS 类型对齐、`src/lib/api/` 封装、权限最小化和资源清理。

### 3. 执行验证矩阵

最小基线：

| 变更 | 必须验证 |
|---|---|
| `.rs` | `cargo fmt --check`、聚焦测试、`cargo check`；高风险再 `cargo clippy` |
| `.ts/.tsx` | 项目格式化、`tsc --noEmit`、聚焦 Vitest、`pnpm build` |
| 页面/组件/样式 | 前端基线 + Codex 内置浏览器或 Control Chrome |
| schema/database | 迁移测试、旧库升级、新库初始化、真实数据格式确认 |
| capabilities/plugin | JSON、注册、最小权限和真实运行时能力 |
| Cargo/package/lock | 安装一致性、类型/编译、测试、跨平台与安全影响 |
| updater/release | 版本、签名、产物、manifest、匿名端点、回滚 |
| 所有文本 | UTF-8、无乱码、`git diff --check` |

若某项因环境不可运行，记录准确阻塞和替代证据，不写成通过。

### 4. 运行时验收

- 页面变更强制使用 Codex 内置浏览器或 Control Chrome，检查关键交互、加载/空/错误态、控制台与请求。
- 数据库 DDL 或数据格式读取真实配置，并通过 Tauri SSH MCP 查询或验证迁移。
- Git、服务器、凭据和远端操作优先使用 Tauri SSH MCP，检查过程中不执行未授权写入。
- 不直接 kill 已有开发进程；复用或选择无冲突环境。

### 5. 输出报告

结论优先，按严重程度输出：

1. 阻断问题：导致错误、安全缺陷、数据损坏或运行失败。
2. 警告：当前可运行但存在明确回归风险。
3. 已通过门禁：命令、范围和关键结果。
4. 未验证项：原因、影响和建议的下一步。

每个发现包含文件、紧凑行号、证据、影响和可执行建议。不要把风格偏好冒充严重错误，也不要声称无问题却省略实际未运行的检查。

## 引用索引

- [rust-checks.md](references/rust-checks.md)：Rust 分层、错误、注册、阻塞、unsafe、serde、fmt/clippy/test。
- [frontend-checks.md](references/frontend-checks.md)：API 封装、类型、React、状态、监听、格式、测试、构建和浏览器。
- [tauri-config-checks.md](references/tauri-config-checks.md)：配置、Capabilities、插件、CSP、identifier 和运行时权限。

## 完成条件

- 实际变更均映射到验证动作，跨层影响没有遗漏。
- 严重发现有可复现证据，误报已通过上下文复核排除。
- 所需格式、类型、测试、构建和运行时验收已运行或明确标为未验证。
- 页面、数据库、安全和发布类变更满足各自真实验收门禁。
- `git diff --check`、UTF-8 和变更范围检查通过。
