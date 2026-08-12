---
name: code-patterns
description: |
  分析或重构 Tauri SSH 代码，使其符合项目已有设计模式、模块边界和编码约定。仅在用户明确询问最佳实践、设计模式、代码规范重构或跨模块一致性时使用；普通功能实现、单个 Bug 修复、数据库/UI/Command 专项开发不触发。
---

# 代码模式与规范重构

## 目标

从当前仓库的真实相邻实现提炼模式，评估候选代码是否符合项目边界，并给出最小、安全、可验证的重构。领域 Skill 负责具体实现细节；本 Skill 负责跨代码的模式选择和一致性判断。

## 激活边界

使用本 Skill：

- 用户明确要求最佳实践、设计模式或编码规范说明。
- 用户要求按项目模式重构、统一多个模块或消除结构性重复。
- 需要判断逻辑应位于 Command、Service、Database、API、Store、Hook 还是页面。

不使用本 Skill：

- 普通新增功能或局部 Bug 修复。
- SQLite、Rust Command、React UI、状态或错误处理的专项实现；使用对应领域 Skill。
- 仅格式化、lint、测试或代码审查；使用检查/审查流程。

## 强制原则

- 先读当前代码树和至少 2 个相邻实现，不以旧教程或目录假设替代仓库事实。
- 模式必须解决当前问题；不要为“更优雅”引入无业务价值的抽象。
- 保持 Tauri 双进程边界和 Rust Command → Service → Database 分层。
- 前端 IPC 统一经 `src/lib/api/`，Rust/TypeScript 类型对齐；系统能力不直接放入 WebView。
- 保留已有业务注释、错误语义、事务和权限边界。
- 重构应小步、行为等价、可回滚；先有聚焦测试或行为基线。
- 页面重构必须使用 Codex 内置浏览器或 Control Chrome 验收。
- 数据库、权限、凭据、发布和远端操作仍遵循对应高风险 Skill，不由通用模式覆盖。

## 工作流

### 1. 建立事实地图

1. 使用 `rg --files` 和 `rg` 定位入口、调用者、数据模型、错误类型、注册点和测试。
2. 阅读目标代码、相邻模块和公共基础设施，例如 `src-tauri/src/shared/`、`src/lib/api/client.ts`、分域 API/types/store。
3. 记录项目当前模式及其变体，区分“明确约束”“主流惯例”“历史遗留”。
4. 明确重构目标：职责、重复、可测性、错误传播、类型安全或性能。

### 2. 选择模式

使用最小可行抽象：

- 只有一处使用且逻辑简单：保持局部，不提前抽公共层。
- 多处重复且变化原因一致：抽取共享函数、Service、Hook 或 API client。
- 跨 IPC：先定义契约和错误，再安排 Rust/TS 两侧。
- 业务数据持久化：Database/Service；UI 全局状态：Zustand；局部交互：Hooks。
- 横切能力：优先复用 `shared/`、API client 或统一错误类型，避免复制。

Rust 细节读取 [rust-patterns.md](references/rust-patterns.md)，React/TypeScript 细节读取 [react-patterns.md](references/react-patterns.md)。

### 3. 评估替代方案

对每个候选简要比较：

- 与现有模式一致性。
- 复杂度和新增抽象数量。
- 错误、事务、并发、取消与权限影响。
- 类型/序列化兼容性。
- 测试和迁移成本。

优先选择能减少状态与分支、保持边界清晰、能由当前测试验证的方案。若两种模式在仓库并存，以目标模块最新且经过验证的实现为准，并说明选择依据。

### 4. 实施重构

1. 先补或确认行为测试。
2. 每次只移动一个职责，保持编译和测试可运行。
3. 更新模块导出、Command 注册、API/types/store re-export 等全链路入口。
4. 删除重复仅限确认无引用后；不触碰其他会话 WIP。
5. 对公开类型、数据库 schema、Capabilities 或持久化格式变更执行兼容性检查。

### 5. 验证

根据实际文件运行格式化、类型检查、聚焦测试、构建和 `git diff --check`。Rust 运行 fmt/test/check，前端运行 format/tsc/Vitest/build；页面再做强制浏览器验收。

确认：行为不变或符合新需求、错误码和消息兼容、序列化字段不漂移、权限未扩大、无无效抽象和死代码。

## 引用索引

- [rust-patterns.md](references/rust-patterns.md)：Rust 分层、错误、共享逻辑、事务与模块组织模式。
- [react-patterns.md](references/react-patterns.md)：React 组件、API client、Hook、Zustand、类型和样式模式。

专项细节按需读取 `api-development`、`tauri-commands`、`database-ops`、`error-handler`、`ui-frontend`、`store-management` 或 `theme-system`，不要一次加载全部。

## 完成条件

- 选择的模式有当前仓库证据，而非只引用通用最佳实践。
- 重构减少了明确问题，没有增加无用抽象或扩大权限。
- 分层、IPC、类型、错误、事务和状态边界保持完整。
- 相关格式、测试、编译、构建与运行时验证通过。
- 页面用指定浏览器验收，UTF-8 与 `git diff --check` 通过。
