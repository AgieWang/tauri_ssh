---
name: database-ops
description: |
  用于 Tauri Rust 后端通过 rusqlite 设计、迁移、查询和验证本地 SQLite 数据库。

  触发场景：
  - 修改 src-tauri/src/database/ 下的 SQLite DAO 或查询映射
  - 新增或升级 PRAGMA user_version Schema 迁移
  - 设计本地 SQLite 表、索引、事务、软删除或并发访问
  - 核验数据库 DDL、真实数据格式、迁移兼容性或查询结果

  不应触发：远程 MySQL/PostgreSQL 的普通业务查询、前端状态持久化、业务字段“状态/数据”讨论。

  触发词：rusqlite、SQLite、PRAGMA user_version、schema migration、SQLite DAO、SQLite transaction、本地数据库迁移
---

# Tauri 本地 SQLite 操作

## 适用边界

本 Skill 只处理 `src-tauri/src/database/` 中由 `rusqlite` 管理的本地 SQLite。远程数据库连接、业务数据排查或数据库工作台功能，不因为出现“SQL、表、数据、查询”就自动使用本 Skill；它们仍须遵守项目的数据库访问与凭据规则。

本项目固定分层：

```text
Command（IPC 参数与返回）→ Service（业务规则）→ Database（rusqlite/SQL）
```

- Command 不直接写 SQL。
- Database 返回 `Result<T, AppError>`，锁失败必须显式转换，禁止 `unwrap()`。
- SQL 值必须参数化；只有经过白名单校验的表名、列名等标识符才能拼接。
- 数据库路径通过 Tauri path API 解析，不硬编码用户目录。

## 开始前必须核验

1. 阅读 `src-tauri/src/database/mod.rs`、`schema.rs` 及同领域 DAO，确认真实连接、当前 `SCHEMA_VERSION`、软删除和时间字段约定。
2. DDL 或数据格式会影响实现时，不凭示例推断：读取项目配置和 schema；需要核对外部数据源时，按项目规则通过 Tauri SSH MCP 做只读查询。
3. 明确旧库版本、目标版本、数据量、唯一约束、空值、时间格式和回滚/恢复路径。
4. 先确认工作区已有数据库改动，避免覆盖其他会话的迁移编号或未提交 SQL。

## 实施流程

### Schema 迁移

- 新迁移从当前版本严格递增，不复用、重排或删除历史迁移。
- 每一步只完成一个明确版本跃迁，并在成功后更新 `user_version`。
- 多条相互依赖的 DDL/DML 使用事务；升级中断后必须可再次启动或明确失败恢复方式。
- 迁移前检查 SQLite 版本能力；不能假设生产旧库支持新语法。
- 详细模式见 [schema-migrations.md](references/schema-migrations.md)。

### DAO 与事务

- 查询显式列名，按列顺序或列名稳定映射；`Option<T>` 必须对应可空列。
- 遵循目标表现有软删除语义；不能把“所有表都软删除”当作通用规则。
- 连接使用项目现有 `Mutex<Connection>`、WAL 和 `busy_timeout` 约定。
- 详情见 [dao-and-transactions.md](references/dao-and-transactions.md)。

### 真实验证

- 至少覆盖新库初始化、当前库升级、相关旧版本升级和失败回滚/重试。
- 对新增约束、索引和查询执行真实数据格式核验；性能任务还需真实 `EXPLAIN` 与耗时证据。
- 验证清单见 [database-verification.md](references/database-verification.md)。

## 高风险数据规则（不得下沉或省略）

1. 未获得明确授权，不执行删除、清空、覆盖、不可逆迁移或远程写入。
2. 写操作前先用只读查询解析精确连接、库、表、条件和影响行数；禁止使用模糊目标、宽泛 glob 或未验证环境变量。
3. 结构变化必须有升级路径和恢复策略；不能以“本机新库可运行”替代旧库升级验证。
4. 不输出数据库密码、连接串或凭据；Git、服务器与数据库访问优先使用 Tauri SSH MCP。
5. 不用拼接用户输入生成 SQL；动态标识符采用固定白名单。
6. 软删除、审计字段和时间格式必须以目标表现有 DDL/DAO 为准，不凭通用模板强加。

发现现有 DAO 吞错、拼接 SQL 或迁移不原子时，单独记录并在本次授权范围内修正或报告，不把风险继续复制到新代码。

## 完成条件

- 分层、错误传播、参数化查询和迁移版本链正确。
- DDL 与 Rust/TypeScript 字段类型已按真实来源对齐。
- 相关迁移/DAO 测试、`cargo fmt`、聚焦 `cargo test` 或 `cargo check` 通过。
- 若改变真实查询行为，已核对结果、索引/计划和边界数据。
- UTF-8 无 BOM，`git diff --check` 通过，未触碰任务外文件。
