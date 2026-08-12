---
name: error-handler
description: |
  用于实现或重构 Rust 到 React 的错误建模、传播、展示与恢复边界。

  触发场景：
  - 需要新增或调整 `AppError` 及业务错误类型
  - 需要设计 Command、Service、Database 的错误传播
  - 需要统一前端 API 错误处理、用户提示或恢复动作
  - 需要实现 React ErrorBoundary 或全局异常边界

  触发词：AppError、thiserror、错误传播、错误建模、Result映射、ErrorBoundary、invoke错误处理、恢复策略
---

# Tauri 错误处理实现

## 能力边界

本 Skill 负责“如何实现错误传播和恢复”，不负责诊断一个已发生故障的根因。

- 运行故障排查优先使用 `bug-detective`。
- 所有权、借用、生命周期、trait 或 Send/Sync 编译语义使用 `rust-fundamentals`。
- 普通业务修改若未改变错误模型，不因出现 `Result` 或 `try-catch` 自动触发。
- 只有用户授权实现/重构时才修改代码。

## 目标模型

```text
Database/基础设施错误
  -> AppError（保留类型和上下文）
  -> Service（添加业务语义）
  -> Command（稳定 IPC 错误契约）
  -> src/lib/api（统一转换）
  -> 页面提示或 ErrorBoundary（可行动、不过度暴露）
```

## Rust 侧规则

1. 使用 `thiserror` 定义有限、语义明确的 `AppError` 变体。
2. 底层错误通过 `#[from]` 或显式 `map_err` 保留原因；不要在每层反复字符串化。
3. Database 层返回数据访问错误，Service 层增加业务上下文，Command 层只转换为稳定 IPC 契约。
4. 不对可能失败的业务路径使用 `unwrap()`、`expect()` 或 `panic!()`。
5. 锁中毒、子进程、网络、文件和序列化错误必须显式传播；日志携带上下文但不能泄露凭据。
6. 区分可重试、用户输入、权限、未找到、冲突和内部错误，避免全部归为 `Custom(String)`。
7. 对外错误信息稳定、可理解；内部诊断细节写日志，并做好脱敏。

## IPC 契约规则

- 当前项目以 `src-tauri/src/error.rs` 的 `CommandError { code, message }` 作为结构化 IPC 错误契约；新增 Command 必须返回 `Result<T, CommandError>`，不得退化为 `Result<T, String>`。
- `AppError -> CommandError` 在 Command 边界完成；保留的 `AppError -> String` 仅用于向后兼容，不作为新代码模板。
- Rust 的 `CommandError` 必须与 `src/lib/api/client.ts` 的同名接口保持 `code`、`message` 字段一致。
- 前端统一通过 `parseCommandError`、`getErrorCode` 和 `getErrorMessage` 识别错误，不在页面重复解析 JSON 或凭字符串内容判断业务分支。
- 不把数据库实现、绝对路径、SQL、token、密码或完整堆栈直接返回 UI。
- 若错误需要重试、重新认证或用户修正输入，契约应提供足够的机器可判定信息。
- API 封装集中处理通用转换，页面只处理与当前交互相关的恢复动作。

## React 侧规则

1. 所有 `invoke` 通过 `src/lib/api/` 封装，并显式处理失败。
2. 预期业务错误用就近反馈：`Form.Item`、`message`、`Alert`、空状态或重试按钮。
3. 渲染期未捕获异常由 ErrorBoundary 兜底；它不能替代异步 `try/catch`。
4. 提示必须说明用户能做什么，不直接展示未知对象或敏感底层信息。
5. 异步操作在 `finally` 恢复 loading；防止重复提交、组件卸载后更新和过期响应覆盖。

## 实现流程

### 1. 读取现状

- 阅读 `src-tauri/src/error.rs`、相邻 Command/Service/Database 和 `src/lib/api/`。
- 记录现有错误变体、转换边界、日志方式和页面反馈模式。
- 确认本次是新增错误类别、补上下文、统一契约还是恢复 UI。

### 2. 设计传播路径

- 定义错误产生层、需要保留的内部原因和对用户暴露的信息。
- 说明错误码/类别、可重试性、恢复动作和日志级别。
- 对跨层新增字段同步更新 Rust/TypeScript 类型与测试。

### 3. 最小实现

- 在最靠近错误源的位置转换一次，并沿层级传播。
- 保持 Command 薄、Service 语义化、Database 专注数据访问。
- 不借错误处理重构扩大业务逻辑范围。

### 4. 验证失败路径

- 对每个新类别至少验证成功、预期失败和未知内部失败。
- 断言错误类别/码和用户可见行为，而非只匹配易变的完整文本。
- 页面变更必须使用 Codex 内置浏览器或 Control Chrome 验证提示和恢复操作。

## 常见反模式

- `unwrap()` 把可恢复故障变成进程崩溃。
- `.ok()` 丢弃错误、空 `catch` 或返回空数组/零值/成功状态，造成静默失败。
- 每层都 `to_string()`，导致类型和上下文丢失。
- 对用户展示完整内部错误或凭据。
- ErrorBoundary 包办异步请求错误。
- 捕获所有错误后仅记录日志，却不向调用方传播或提供明确降级状态。

## 按需参考

需要 Rust、IPC 和 React 的代码模板时，读取 [references/error-propagation-patterns.md](references/error-propagation-patterns.md)。

## 完成条件

- 错误从源头到 UI 的类型、上下文和恢复行为清晰一致。
- 新 Command 使用 `Result<T, CommandError>`，前端使用 `getErrorCode/getErrorMessage`，没有契约退化。
- 没有 panic、`.ok()` 丢错、空 `catch`、敏感信息泄露或静默默认值。
- 成功和失败路径都有针对性测试。
- 前端失败状态经过真实浏览器验证。
- 修改已格式化，并通过相关检查、测试及 `git diff --check`。
