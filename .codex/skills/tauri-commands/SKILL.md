---
name: tauri-commands
description: |
  用于实现 Tauri 2 Rust Command 的高级模式，包括 State/AppHandle/Window 注入、async、长任务进度、事件或 Channel 流式返回及子进程调用。

  触发场景：
  - Command 需要注入 AppHandle、Window、WebviewWindow 或 State
  - Command 包含异步 IO、长任务、取消、进度或流式数据
  - Rust IPC 入口需要安全执行系统子进程或共享后台状态

  不应触发：仅做普通端到端 IPC 类型对齐；仅新增简单 CRUD Command；普通 React 页面；用户未显式请求的 /command 脚手架。

  强触发词：State AppHandle Command、Window 注入 Command、async tauri command、Command 进度回报、Channel stream、长任务 IPC、CREATE_NO_WINDOW
---

# Tauri Command 高级实现

## 职责边界

本 Skill 只负责 Rust Command 的高级实现机制。端到端名称、serde/TypeScript 契约和注册链使用 `api-development`；Tauri `emit/listen` 的事件语义使用 `tauri-events`；显式生成单个 Command 使用 `command`。

Command 必须保持薄层：接收和校验 IPC 参数、获取框架注入对象、调用 Service、转换可序列化错误。业务逻辑放 Service，SQL 放 Database。

## 决策流程

1. 读取目标模块和一个相似 Command，确认当前错误类型、State 结构、运行时和模块导出模式。
2. 判断工作负载：
   - 纯计算且很短：同步 Command。
   - 文件、网络、子进程或等待：async Command，避免阻塞 IPC 线程。
   - 共享资源：注入 `State<'_, AppState>`，按现有锁策略访问。
   - 应用路径/全局能力：注入 `AppHandle`。
   - 当前窗口能力：注入 `Window`/`WebviewWindow`，不要从前端传伪窗口标识替代框架注入。
   - 多次进度/结果：事件或 Channel；设计事件名、payload、清理和并发隔离。
3. 将可复用或耗时逻辑下沉 Service；Command 内只保留边界代码。
4. 为参数、错误、取消、并发和平台差异补测试或最小复现。
5. 完成 `commands/mod.rs` 导出与 `generate_handler![]` 注册，并与前端调用联调。

## 不可下沉的关键规则

- 禁止对可失败操作使用 `unwrap()` 或在 Command 中 `panic!()`；使用项目现有 `AppError`/`CommandError` 向前端返回稳定错误。
- 禁止 `std::thread::sleep`、同步网络请求或长时间同步磁盘 IO 阻塞 IPC；使用 async API、`tokio::time::sleep`，必要时 `spawn_blocking`。
- 不要持有 `MutexGuard` 跨 `.await`；先复制必要数据或重构锁边界。
- 所有 Command 都要 `#[tauri::command]`、公开导出并注册；事件调用需要引入正确的 `Emitter` trait。
- 事件监听必须在成功、失败和组件卸载路径清理；并发任务不得共享无法区分的全局事件流。
- 启动子进程时必须校验程序和参数，禁止拼接 shell 字符串；敏感值不得出现在命令行、日志或错误中。
- Windows GUI 包启动 `std::process::Command` 或 `tokio::process::Command` 时必须设置 `CREATE_NO_WINDOW`，避免弹出控制台窗口。
- 插件、窗口、文件或 shell 能力必须继续检查 Capabilities 与安全边界；Command 本身不是绕过权限的通道。

## 按需读取 References

- State/AppHandle/Window、async、阻塞任务和锁：读取 [注入与异步模式](references/injection-and-async.md)。
- 进度、事件、多任务隔离、Channel/stream 和监听清理：读取 [进度与流式模式](references/progress-and-streaming.md)。
- 模块化、错误、批量参数、子进程及完整代码：读取 [完整实现示例](references/complete-examples.md)。
- 如果主要问题是 Rust/TS 参数或返回类型不一致，停止扩展本 Skill，使用 `api-development`。

## 路由示例

| 请求 | 本 Skill | 其他 Skill |
|---|---|---|
| “State 中取数据库后异步执行任务” | 必选 | 数据库/错误按需 |
| “用 AppHandle 获取 app data dir” | 必选 | 涉及文件功能再加 `file-storage` |
| “用事件实时展示批处理进度” | 必选 | `tauri-events` + `ui-frontend` |
| “新增普通查询 Command 并补 TS API” | 通常不选 | `api-development` |
| “invoke 字段名不匹配” | 不选 | `api-development`/`json-serialization` |

## 交付证据

至少说明同步/异步选择、注入对象、锁与取消策略、错误协议、注册点和实际执行的 Rust 检查。含事件/页面时额外说明监听清理与浏览器/Tauri 运行时验证。

不要只报告 `cargo check`；涉及并发、取消、子进程或事件时必须给出对应行为测试。无法运行桌面环境时明确剩余运行时风险。

所有示例都必须按当前依赖版本编译校正。

## 验证出口

- [ ] Command 是薄 IPC 包装，业务与 SQL 未泄漏到入口。
- [ ] async/阻塞、锁和取消策略与负载匹配。
- [ ] 注入对象、事件 payload 和前端清理路径正确。
- [ ] 子进程调用使用参数数组、脱敏日志和 Windows 防弹窗设置。
- [ ] 模块导出、handler 注册、API 契约和权限全部闭环。
- [ ] `cargo fmt --check`、聚焦测试、`cargo check`/`clippy` 与 `git diff --check` 通过。
