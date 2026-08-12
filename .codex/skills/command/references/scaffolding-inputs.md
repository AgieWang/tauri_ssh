# `/command` 输入与文件决策

## 1. 输入清单

从需求和现有代码推断，只有风险较高或存在多个不兼容答案时才询问：

| 输入 | 示例 | 决策影响 |
|---|---|---|
| 功能和模块 | 读取配置 / `config` | 文件位置与命名 |
| Command 名 | `read_config` | Rust 注册与 invoke |
| 入参 | `path: String` | serde/TS payload 与安全校验 |
| 返回 | `ConfigFile` | Rust/TS 类型 |
| 工作负载 | 文件 IO | sync/async |
| 注入对象 | AppState/AppHandle | Rust 签名和生命周期 |
| 持久化 | SQLite | 是否需要 Database/Service |
| 系统能力 | dialog/fs/shell | 插件、Builder、Capabilities |

## 2. 类型判断

| 信号 | 默认判断 | 还需核对 |
|---|---|---|
| 纯计算/格式化 | 同步 | 数据量是否很大 |
| 网络/异步文件/等待 | async | SSRF、超时、取消 |
| SQLite/rusqlite | 当前 DB 模式 | 是否需要事务/阻塞隔离 |
| App 数据目录 | AppHandle | path API 和权限 |
| 当前窗口操作 | Window/WebviewWindow | 多窗口 scope |
| 长批处理 | async + 进度或 Channel | task id、取消、并发 |
| 子进程 | async 或 spawn_blocking | allowlist、审计、Windows 防弹窗 |

## 3. 强制读取的真实代码

按需读取存在的文件，不假设旧模板路径仍有效：

- `src-tauri/src/commands/mod.rs` 与目标模块；
- `src-tauri/src/services/mod.rs` 与同类 Service；
- `src-tauri/src/models/`、`src-tauri/src/error.rs`、`src-tauri/src/state.rs`；
- `src-tauri/src/lib.rs`、`src-tauri/Cargo.toml`；
- `src/lib/api/client.ts`、同类业务 API、`src/lib/api/index.ts`；
- `src/types/` 对应类型；
- 涉及插件时读取 `src-tauri/capabilities/*.json` 和 `tauri.conf.json`。

用 `rg` 搜索函数名、invoke 字符串和类型名，确认无冲突。

## 4. 文件决策

| 文件 | 何时修改 |
|---|---|
| `commands/<domain>.rs` | 必需：Command 定义 |
| `commands/mod.rs` | 新模块或缺少导出 |
| `lib.rs` | 必需：handler 注册 |
| `models/<domain>.rs` | 有共享/结构化输入输出 |
| `services/<domain>.rs` | 有业务不变量、组合或复用 |
| `database/<domain>.rs` | 有 SQL/SQLite 数据访问 |
| `src/types/<domain>.ts` | 有结构化 JSON 契约 |
| `src/lib/api/<domain>.ts` | 必需：业务 API 封装 |
| API/type index | 新模块需要统一导出 |
| Cargo/package | 确认缺少依赖且现有依赖不能满足 |
| capabilities | 使用插件 permission/scope |

纯计算 Command 不要生成空 Service/Database；SQLite Command 不要在 Command 或 Service 内直接写 SQL。

## 5. 范围升级或降级

- 发现需要多个 Command、完整 UI、路由和状态管理：停止 `/command` 扩张，改为显式 `dev` 或领域 Skill 组合。
- 仅修改已有契约字段：使用 `api-development`，不重新脚手架。
- 仅高级 async/State/Channel：使用 `tauri-commands`。
- 仅 UI：使用 `ui-frontend`。
