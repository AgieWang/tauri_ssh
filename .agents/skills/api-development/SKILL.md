---
name: api-development
description: |
  用于设计或修改 Tauri 前后端端到端 IPC 契约，确保 Rust Command、serde 模型、注册链、TypeScript 类型和前端 API 封装一致。

  触发场景：
  - 新增或修改会被 React 调用的 Tauri IPC 接口
  - 排查 Command 名称、参数、返回值或错误结构的前后端不一致
  - 一次改动跨越 Rust models/commands 与 TypeScript types/lib/api

  不应触发：仅实现 AppHandle/State/async/stream 等 Rust 高级 Command 细节；仅做 React 页面；普通 HTTP/REST API；显式 /command 脚手架工作流。

  强触发词：Tauri IPC 契约、前后端类型对齐、invoke 参数不匹配、Command not found、Rust TypeScript 对齐、serde IPC
---

# Tauri IPC 契约开发

## 职责边界

本 Skill 负责一条可调用链是否完整、名称与数据是否一致：

```text
React -> src/lib/api/* -> invoke -> #[tauri::command]
      -> Service -> Database（按需）
```

- `api-development`：端到端契约、注册、Rust/TypeScript 类型对齐。
- `tauri-commands`：State/AppHandle/Window、异步、进度、Channel 等 Rust 高级实现。
- `command`：用户显式请求 `/command` 或“使用 command 脚手架”时的生成流程。
- `ui-frontend`：页面、组件和真实浏览器验收。

不要把 Tauri IPC 描述成 HTTP 路由；前端不得直接裸写 `invoke()`，统一通过 `src/lib/api/` 的业务模块调用。

## 强制执行流程

1. 读取现有相似实现，不凭模板猜目录或返回错误类型：
   - `src-tauri/src/models/`
   - `src-tauri/src/commands/`、`services/`、`database/`
   - `src-tauri/src/lib.rs`
   - `src/types/`、`src/lib/api/`
2. 先写清契约表：Command 名、Rust 入参、JS payload、Rust 返回、TS 返回、错误结构。
3. 按项目现有三层模式实现：Command 仅负责 IPC 边界、参数校验、Service 调用与错误转换；SQL 只进入 Database 层。
4. 定义 serde 模型与 TypeScript 类型，逐字段核对名称、空值、枚举、时间和数字范围。
5. 完成双端注册：Rust 模块导出、`generate_handler![]`、TS API 模块和统一导出。
6. 前端通过业务 API 调用并处理 loading、成功、失败；错误文本使用 `getErrorMessage(error)`。
7. 按改动范围格式化并运行 Rust/TypeScript 检查；含页面时必须交给浏览器验收。

## 准确性门禁

- 不假设结构体字段会自动从 `snake_case` 变为 `camelCase`；以 serde 属性和实际序列化结果为准。
- Command 参数键遵循项目当前 Tauri 配置；默认按 Rust `snake_case` 与前端 `camelCase` 映射，并用测试或运行时调用确认。
- 不允许 `unwrap()`/`panic!()` 处理可失败操作；返回项目当前结构化错误（现有代码通常为 `CommandError`）。
- Command 必须标记 `#[tauri::command]`、公开导出并注册，否则前端会得到 `Command not found`。
- 新插件 API 必须同时检查 Builder 注册和 Capabilities；仅新增普通 Command 不虚构权限。
- 外部 HTTP、文件、凭据或数据库写操作必须继续加载对应安全/权限/数据库 Skill。

## 按需读取 References

- 修改参数、返回值、serde/TS 类型或错误协议：读取 [IPC 契约参考](references/ipc-contract.md)。
- 新增 Command、模块或前端 API 导出：读取 [注册链检查清单](references/registration-checklist.md)。
- 注入 State/AppHandle、异步或进度流：改读 `tauri-commands` 的对应 references。
- 需要生成单个 Command 全套文件：仅在用户显式请求时使用 `command`。

## 路由示例

| 请求 | 本 Skill | 组合 |
|---|---|---|
| “新增 `get_servers`，补齐 Rust/TS 类型和 API” | 必选 | 按需数据库/UI |
| “invoke 返回字段名与 Rust 不一致” | 必选 | `json-serialization`（复杂 serde 时） |
| “Command 里注入 AppHandle 并持续发进度” | 非主责 | `tauri-commands` + `tauri-events` |
| “只修改一个 React 表格” | 不选 | `ui-frontend` |
| “/command 生成一个读取文件命令” | 不选 | `command` 按需读取契约 reference，并组合文件/安全 Skill |

## 最小交付摘要

交付时写明 Command 名、入参/返回/错误、注册点、前端 API 方法以及验证证据；任何未运行的真实 IPC/浏览器检查必须明确标注，不能以编译成功替代。

## 完成条件

- [ ] 契约表与实现逐项一致，没有字段名或可空性猜测。
- [ ] Command -> Service -> Database 分层符合当前模块模式。
- [ ] Rust 注册链、TypeScript API 和统一导出全部存在。
- [ ] 前端没有组件内裸写 `invoke()`，错误被统一解析。
- [ ] 聚焦测试、类型检查、格式化和 `git diff --check` 通过。
- [ ] 涉及页面的调用已用 Codex 内置浏览器或 Chrome 验收。
