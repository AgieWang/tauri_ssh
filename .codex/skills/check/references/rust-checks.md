# Rust 检查

## 架构与注册

- Commands 只负责 IPC 参数、状态注入、调用 Service 和错误转换，不直接写 SQL 或承载长业务逻辑。
- Services 承载业务校验、事务编排和跨 DAO 协作。
- Database 层持有 rusqlite 查询、映射、事务和迁移。
- 新模块在 `mod.rs` 导出；每个 `#[tauri::command]` 在真实 `generate_handler![]` 注册。
- 前端调用名、参数命名和返回类型与 Rust 契约一致。

使用 `rg` 收集 Command 与注册列表，人工对照条件编译和模块路径，不能只凭简单 grep 数量判断。

## 错误与安全

- 可失败操作使用 `Result` 和项目当前统一错误类型，不在可恢复路径 `unwrap()`、`expect()` 或 `panic!()`。
- 锁中毒、IO、数据库和序列化错误保留上下文并可安全传给 IPC；不得泄露凭据。
- `unsafe` 块必须有紧邻的 `SAFETY:` 依据，并审查边界、生命周期和平台假设。
- SQL 使用参数绑定；动态表名、列名和排序字段只能来自白名单。
- 跨 IPC struct 具有所需 serde derive、字段命名、Option/枚举/时间格式和 TS 对齐。

测试代码或进程入口的 `expect` 不能被一刀切误报；需要结合“是否可恢复、是否会导致用户数据丢失”判断。

## 并发与阻塞

- 同步 Command 不执行长文件 IO、网络、sleep 或大量 CPU 工作。
- async Command 中也不能直接阻塞 Tokio executor；按项目模式使用异步 API 或 `spawn_blocking`。
- Mutex/RwLock 持有范围最小，不跨 `.await`；数据库事务正确提交或回滚。
- 取消、超时、重复调用和应用退出时的资源释放有明确行为。

## 建议验证

```bash
(cd src-tauri && cargo fmt --check)
(cd src-tauri && cargo test <focused-target>)
(cd src-tauri && cargo check)
(cd src-tauri && cargo clippy --all-targets --all-features -- -D warnings)
```

根据改动风险选择聚焦测试或更广范围；clippy 失败需区分本次新增与既有问题。依赖或 feature 变更还需检查 lockfile、目标平台和许可证/安全影响。

## 审查清单

- [ ] 分层与模块注册完整。
- [ ] 错误传播无不必要 panic，敏感信息不泄露。
- [ ] serde 与 TypeScript 契约一致。
- [ ] 阻塞、锁、事务和取消语义正确。
- [ ] 聚焦测试覆盖成功、错误和边界路径。
- [ ] fmt、check 通过；高风险变更 clippy 通过或有说明。
