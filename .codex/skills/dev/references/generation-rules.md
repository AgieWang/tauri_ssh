# `/dev` 生成规则与验证

## 目录

1. Rust 规则
2. TypeScript/React 规则
3. 权限与桌面边界
4. 验证矩阵

## 1. Rust 规则

1. Command、Service、Database 分层；Command 仅 IPC/校验/错误转换。
2. Service 承载业务不变量、跨模块组合和事务语义。
3. Database 承载 SQL、row mapping 和连接/事务操作，不含业务判断。
4. 所有模型按当前模块组织并具备必要 serde derive；不强制把所有类型塞入单一 `models/mod.rs`。
5. 复用当前 `AppError`/`CommandError`，不得为单一功能另造不兼容错误协议。
6. 可失败操作使用 `?`/显式转换，禁止 `unwrap()` 和 `panic!()`。
7. 新 Command 标记 `#[tauri::command]`、公开导出并注册。
8. IO/等待使用 async；CPU/阻塞库按实测决定 `spawn_blocking`；禁止 `std::thread::sleep`。
9. SQL 参数化；锁失败传播错误；不持锁跨 await。
10. 新表/列/索引使用当前 `PRAGMA user_version` 迁移和可验证升级路径，不写临时 `init_database()` 分支。
11. 插件在 Builder 注册；AppState 在 setup/manage 中按现有模式初始化。
12. 子进程不用 shell 拼接，处理 allowlist、超时、退出码、脱敏和 Windows 防弹窗。

## 2. TypeScript / React 规则

1. 业务 invoke 封装在 `src/lib/api/<domain>.ts`，通过统一 index 导出。
2. 类型放 `src/types/` 的领域模块；不在组件重复定义后端契约。
3. 字段与实际 serde JSON 一致；`Option<T>` 通常是 `T | null`，i64/u64 核对安全整数。
4. `unknown` 错误由 `getErrorMessage`/`parseCommandError` 处理，禁止 `any` 和对象字符串拼接。
5. 函数组件 + Hooks；使用 `@/` 别名、当前 Ant Design 6 和 TailwindCSS 4/CSS Variables；组件 API 以锁定版本类型为准。
6. 局部状态优先 Hooks；明确跨组件共享才用 Zustand，不用 Context 复制全局 Store。
7. 页面显示 loading、empty、error、success；删除二次确认，表单有校验，重复提交被禁止。
8. 外部 API 默认经 Rust 代理；dev API fallback 仅复用当前受限实现，不能引入任意公网 fetch。
9. 文件系统不使用 Node 模块或硬编码路径；使用 Rust Command/Tauri API 和跨平台 path。
10. 页面组件按职责拆分；不要机械要求固定 200 行，但过大且多职责时必须拆分。

## 3. 权限与桌面边界

1. 使用插件 API 才添加最小 permission/scope；权限和窗口范围必须匹配调用方。
2. 新插件同时核对 Rust crate、前端 binding、Builder 注册和 Capabilities。
3. 不添加未使用权限，不用 `core:default` 等宽权限掩盖缺失 scope。
4. Tauri IPC 不是 REST 路由；不设计 Controller、RESTful URL、后台菜单 SQL或多租户层，除非仓库真实存在对应远程服务。
5. 桌面本地应用仍可能包含远程访问/移动 companion；以当前代码为准，不用“桌面应用”否定真实功能。
6. 跨平台路径使用 Tauri/std Path API，子进程和窗口行为按平台验证。
7. 凭据只能走 Safe Credentials/安全服务，日志、事件、命令行和前端状态不泄漏明文。

## 4. 验证矩阵

| 改动 | 最低验证 |
|---|---|
| Rust models/service/command | `cargo fmt --check`、聚焦 test、`cargo check` |
| Rust 安全/复杂逻辑 | 上述 + `cargo clippy`、安全审查 |
| SQLite schema/DAO | 迁移测试、旧库升级、查询/事务测试 |
| TS types/API/store | formatter、聚焦 test、`tsc --noEmit` |
| React page/router | 上述 + build + 内置浏览器/Chrome |
| Capabilities/plugin | JSON/配置校验 + 真实 Tauri 权限调用 |
| 文件/网络/子进程 | 成功、非法输入、超时/失败、平台边界 |

最终统一执行 `git diff --check`。格式化必须实际运行而非仅检查肉眼格式。

## 5. 浏览器验收

前端页面测试强制使用 Control Chrome with Codex 或 Codex 内置浏览器，优先内置浏览器。验证：

- 路由和入口可到达；
- 表单校验、加载、空态、错误和成功提示；
- API payload/response 与契约一致；
- 控制台无未处理 Promise、React 警告和权限错误；
- 主题和窗口尺寸下布局可用。

仅前端 mock 通过不等于真实 Tauri IPC 验收；需要分别标注浏览器层和桌面运行时证据。
