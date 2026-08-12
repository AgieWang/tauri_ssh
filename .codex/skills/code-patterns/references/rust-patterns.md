# Rust 模式

## 分层

```text
commands/ -> services/ -> database/
                 |
              shared/
```

- Command：IPC 参数、`State`/`AppHandle` 注入、调用 Service、错误边界。
- Service：业务规则、事务边界、跨 DAO 编排和外部能力协调。
- Database：rusqlite SQL、行映射、迁移和持久化细节。
- Models：IPC/数据库模型；按项目当前分域方式组织。
- Shared：跨领域基础能力；只有变化原因一致时才抽取。

Service 是否必须存在取决于业务复杂度，但 Command 不能直接混入长 SQL 或不可测试的业务流程。

## 错误模式

- 内部层使用项目当前统一错误类型并保留 source/context。
- IPC 边界转换为结构化 CommandError 或项目现有可序列化错误。
- 不用 `String` 拼接取代错误分类，也不把凭据、SQL 参数或远端响应原样暴露给前端。
- 可恢复路径不 `unwrap`/`panic`；仅在进程启动的不可恢复不变量处按现有模式处理。

## 数据库与事务

- SQL 使用参数绑定；动态标识符白名单化。
- 锁范围最小，错误显式转换，不跨 await 持有同步锁。
- 多步写操作在 Database/Service 明确事务，失败完整回滚。
- `Option`、not found、affected rows 与软删除语义按真实表契约处理，不通用化假设所有表都有 `deleted_at`。
- SQLite WAL、busy timeout 和连接策略以当前数据库初始化实现为准。

## 模块与注册

新增端到端能力时检查：

1. module 声明与 re-export。
2. model、database、service、command 的引用方向。
3. `generate_handler![]` 注册。
4. TypeScript types 与 API 封装。
5. 测试和 feature/平台条件。

模块命名采用当前仓库约定，通常 Rust 文件和函数为 snake_case，类型为 PascalCase，常量为 SCREAMING_SNAKE_CASE。

## 共享抽象判断

适合抽取：

- 两处以上重复，且业务规则和变化原因相同。
- 需要统一错误、重试、审计或资源释放。
- 可以定义窄接口并用测试固定行为。

不适合抽取：

- 只是代码形状相似但领域语义不同。
- 为未来假设提前创建 trait/泛型层。
- 抽取后需要大量布尔参数或分支恢复原差异。

## 验证

```bash
(cd src-tauri && cargo fmt --check)
(cd src-tauri && cargo test <focused-target>)
(cd src-tauri && cargo check)
```

高风险并发、unsafe、依赖或公共 API 重构追加 clippy、更多测试和运行时验证。
