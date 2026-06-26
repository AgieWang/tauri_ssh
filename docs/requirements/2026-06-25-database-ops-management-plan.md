# 数据库管理运维功能实施方案

**版本**: v0.1 草案
**日期**: 2026-06-25
**项目**: Tauri SSH
**目标模块**: 数据库管理 / 数据库运维 / Database Ops

---

## 1. 背景与目标

Tauri SSH 目前已经具备服务器资产、凭据保险库、SSH 终端、SFTP、日志监听、AI Provider、MCP Server、审批队列和审计日志等能力。数据库管理运维功能应在这些基础能力之上扩展，而不是做成一个独立的裸数据库客户端。

目标是让用户可以在同一个桌面应用中完成数据库连接管理、查询、表结构查看、数据浏览、备份导出、运维诊断和受控变更，同时让 AI 与 MCP Agent 可以在权限策略约束下读取数据库状态、解释 SQL、生成诊断建议，但不能绕过审批执行高风险操作。

一句话定位：

> 一个继承服务器资产、凭据保险库、AI 权限、审批队列和审计日志的受控数据库运维工作台。

---

## 2. 产品原则

1. **本地优先**
   - 数据库连接配置、查询历史、收藏 SQL、审计索引保存在本机 SQLite。
   - 数据库密码、连接串密钥等敏感字段只进入后端加密存储，不返回前端明文。

2. **连接受控**
   - 支持直连，也支持通过已配置 SSH 服务器建立隧道连接。
   - 推荐首版优先支持 SSH 隧道，复用服务器资产和凭据能力。

3. **默认只读**
   - 首版默认只开放 SELECT / SHOW / DESCRIBE / EXPLAIN 等只读能力。
   - DDL、DML、事务提交、批量删除、TRUNCATE、DROP 等必须进入审批或直接禁止。

4. **AI 只建议不越权**
   - AI 可解释 SQL、生成查询建议、解释执行计划和错误。
   - AI 触发的写操作必须走审批队列，且必须记录完整审计。

5. **MCP 与 UI 同权**
   - UI 点击、终端 AI、MCP 工具调用必须复用同一套 Service 层策略、审批和审计。
   - MCP 工具不得返回数据库连接密码、SSH 凭据或完整敏感数据。

---

## 3. 功能范围

### 3.1 V0.1 必做

- 数据库连接配置 CRUD。
- 连接类型首版支持：
  - MySQL / MariaDB
  - PostgreSQL
  - Redis
- 连接方式：
  - 直连：host、port、database、user、password_ref
  - SSH 隧道：选择现有 SSH 服务器作为跳板
- 凭据来源：
  - 直接密码加密存储
  - 凭据保险库引用
- 连接测试。
- 数据库/Schema/表/视图列表浏览。
- 表结构查看：字段、类型、是否为空、默认值、主键、索引。
- 只读 SQL 查询控制台。
- 查询结果表格展示、分页、复制、导出 CSV。
- 查询结果默认单页 500 行，必须支持分页加载。
- 查询历史、收藏 SQL。
- Redis 只读浏览：
  - DB 列表
  - key scan
  - key 类型识别
  - value 只读查看
  - TTL 查看
- SQL 风险识别：
  - 只读允许
  - 变更按数据库安全级别进入全部审批或二次确认执行
  - 高危禁止
- 审计日志：
  - 连接测试
  - 查询执行
  - 导出
  - AI 解释
  - 审批通过后的变更执行
- AI 辅助：
  - 解释 SQL
  - 优化 SQL 建议
  - 解释执行计划
  - 根据错误输出给出修复建议
- MCP 第一批数据库工具：
  - `db_list_connections`
  - `db_schema_overview`
  - `db_read_query`
  - `db_explain_query`

### 3.2 V0.2 应做

- MongoDB 管理：库/集合浏览、find 只读查询、索引查看。
- 数据库备份：
  - MySQL `mysqldump`
  - PostgreSQL `pg_dump`
  - 备份任务记录
- 表数据编辑，默认审批。
- SQL 批量执行，默认审批。
- 慢查询分析。
- 连接池和长会话管理。
- 多标签 SQL 编辑器。
- SQL 模板库。
- MCP 第二批工具：
  - `db_create_approval`
  - `db_execute_approved`
  - `db_export_query`
  - `db_backup_request`

### 3.3 暂不做

- 不做完整 Navicat/DataGrip 替代品。
- 不做数据库用户权限管理的全量 UI。
- 不做自动绕过堡垒机或数据库审计系统的连接方式。
- 不在前端保存或展示数据库密码明文。
- 不允许 AI/MCP 直接读取完整大表或导出敏感库。

---

## 4. 信息架构与菜单规划

建议在左侧菜单新增一级或二级入口：

```text
运维
├── 终端 + AI
├── 日志监听
├── SFTP 文件
└── 数据库管理
```

数据库管理页面内部使用 Tabs：

1. **连接**
   - 连接列表
   - 新建/编辑连接
   - 连接测试
   - SSH 隧道选择

2. **对象浏览**
   - 数据库 / Schema 树
   - 表 / 视图 / 索引
   - 字段结构

3. **SQL 控制台**
   - SQL 编辑器
   - 查询结果
   - 执行计划
   - 查询历史
   - 收藏 SQL

4. **运维诊断**
   - 连接状态
   - 版本信息
   - 进程 / 会话列表
   - 库大小 / 表大小
   - 慢查询入口

5. **备份与导出**
   - 查询结果导出
   - 备份任务
   - 下载记录

---

## 5. 技术方案

### 5.1 推荐方案：Rust 后端统一代理数据库连接

所有数据库连接、查询、导出、备份、隧道建立都在 Rust 后端完成。React 前端只负责 UI、表单、结果展示，不直接连接数据库。

调用链：

```text
React 页面
  -> src/lib/api/databaseOps.ts
  -> Tauri Command
  -> services/database_ops.rs
  -> database/mod.rs 本地配置读写
  -> db_runtime/* 远程数据库连接与查询
  -> approval/audit service
```

优点：

- 敏感连接信息不会暴露到 WebView。
- 可以统一接入审批、审计、脱敏、限流。
- MCP 工具与 UI 能共享同一套 Service。
- 跨平台行为更可控。

缺点：

- Rust 侧数据库驱动和 SSH 隧道管理复杂度高。
- 需要设计查询取消、超时、分页和大结果集限制。

结论：采用此方案。

### 5.2 Rust 依赖建议

首版优先选择成熟 Rust crate：

| 类型 | crate | 用途 |
| --- | --- | --- |
| MySQL | `mysql_async` 或 `sqlx` mysql feature | MySQL 查询 |
| PostgreSQL | `tokio-postgres` 或 `sqlx` postgres feature | PostgreSQL 查询 |
| Redis | `redis` | Redis 连接、DB/key/value/TTL 只读浏览 |
| SSH 隧道 | 复用现有 `ssh2` 能力或实现本地 TCP forward | 通过 SSH 服务器访问内网 DB |
| CSV 导出 | `csv` | 查询结果导出 |
| SQL 解析 | `sqlparser` | SQL 风险识别 |
| 脱敏 | 现有工具函数 + 新增规则 | 查询结果和审计脱敏 |

建议首版优先用数据库专用驱动，而不是通过远程 shell 调 `mysql` / `psql` 命令。只有备份场景可调用远端命令，但必须受审批和审计控制。

---

## 6. 本地 SQLite 表设计

当前项目 `SCHEMA_VERSION = 10`，数据库管理建议从 v11 开始迁移。

### 6.1 `database_connections`

保存数据库连接元信息，不保存明文密码。

```sql
CREATE TABLE IF NOT EXISTS database_connections (
    key                   TEXT PRIMARY KEY,
    name                  TEXT NOT NULL,
    group_name            TEXT NOT NULL DEFAULT '',
    db_type               TEXT NOT NULL,
    host                  TEXT NOT NULL,
    port                  INTEGER NOT NULL,
    database_name         TEXT NOT NULL DEFAULT '',
    username              TEXT NOT NULL DEFAULT '',
    auth_type             TEXT NOT NULL DEFAULT 'password_ref',
    auth_ref              TEXT NOT NULL DEFAULT '',
    password_nonce        TEXT DEFAULT NULL,
    password_ciphertext   TEXT DEFAULT NULL,
    ssh_tunnel_enabled    INTEGER NOT NULL DEFAULT 0,
    ssh_server_alias      TEXT NOT NULL DEFAULT '',
    ssl_mode              TEXT NOT NULL DEFAULT 'prefer',
    connect_timeout_secs  INTEGER NOT NULL DEFAULT 10,
    query_timeout_secs    INTEGER NOT NULL DEFAULT 30,
    readonly_by_default   INTEGER NOT NULL DEFAULT 1,
    security_mode         TEXT NOT NULL DEFAULT 'approval_all',
    ai_policy             TEXT NOT NULL DEFAULT 'L2',
    page_size             INTEGER NOT NULL DEFAULT 500,
    status                TEXT NOT NULL DEFAULT 'unknown',
    enabled               INTEGER NOT NULL DEFAULT 1,
    last_connected_at     TEXT DEFAULT NULL,
    created_at            TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
    updated_at            TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
    deleted_at            TEXT DEFAULT NULL
);

CREATE INDEX IF NOT EXISTS idx_database_connections_group ON database_connections(group_name);
CREATE INDEX IF NOT EXISTS idx_database_connections_type ON database_connections(db_type);
CREATE INDEX IF NOT EXISTS idx_database_connections_status ON database_connections(status);
CREATE INDEX IF NOT EXISTS idx_database_connections_ssh_server ON database_connections(ssh_server_alias);
```

字段说明：

| 字段 | 说明 |
| --- | --- |
| `key` | 连接唯一标识 |
| `db_type` | `mysql` / `postgres` / `redis` / `mongodb` |
| `auth_type` | `password_ref` / `direct_password` / `none` |
| `auth_ref` | 凭据保险库 key |
| `password_*` | 直接密码加密字段 |
| `ssh_tunnel_enabled` | 是否通过 SSH 隧道 |
| `ssh_server_alias` | 复用现有服务器资产 |
| `readonly_by_default` | 是否默认只读 |
| `security_mode` | 数据库安全级别：`approval_all` 全部变更审批 / `confirm_execute` 用户二次确认后执行 |
| `ai_policy` | 复用服务器 AI 权限级别 |
| `page_size` | 默认单页行数，首版默认 500 |

### 6.2 `database_query_history`

记录 SQL 查询历史，敏感内容可做摘要存储。

```sql
CREATE TABLE IF NOT EXISTS database_query_history (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    connection_key      TEXT NOT NULL,
    db_type             TEXT NOT NULL,
    database_name       TEXT NOT NULL DEFAULT '',
    sql_text            TEXT NOT NULL,
    sql_fingerprint     TEXT NOT NULL DEFAULT '',
    risk                TEXT NOT NULL DEFAULT 'readonly',
    result              TEXT NOT NULL DEFAULT '',
    rows_affected       INTEGER DEFAULT NULL,
    rows_returned       INTEGER DEFAULT NULL,
    duration_ms         INTEGER DEFAULT NULL,
    error_message       TEXT NOT NULL DEFAULT '',
    source              TEXT NOT NULL DEFAULT 'ui',
    approval_id         INTEGER DEFAULT NULL,
    created_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
    deleted_at          TEXT DEFAULT NULL
);

CREATE INDEX IF NOT EXISTS idx_database_query_history_connection ON database_query_history(connection_key);
CREATE INDEX IF NOT EXISTS idx_database_query_history_created ON database_query_history(created_at);
CREATE INDEX IF NOT EXISTS idx_database_query_history_risk ON database_query_history(risk);
```

### 6.3 `database_saved_queries`

收藏 SQL 和模板。

```sql
CREATE TABLE IF NOT EXISTS database_saved_queries (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    name            TEXT NOT NULL,
    connection_key  TEXT NOT NULL DEFAULT '',
    db_type         TEXT NOT NULL DEFAULT '',
    sql_text        TEXT NOT NULL,
    description     TEXT NOT NULL DEFAULT '',
    tags            TEXT NOT NULL DEFAULT '[]',
    created_at      TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
    updated_at      TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
    deleted_at      TEXT DEFAULT NULL
);
```

### 6.4 `database_export_tasks`

记录导出和备份任务。

```sql
CREATE TABLE IF NOT EXISTS database_export_tasks (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    connection_key  TEXT NOT NULL,
    task_type       TEXT NOT NULL,
    status          TEXT NOT NULL DEFAULT 'pending',
    resource        TEXT NOT NULL DEFAULT '',
    local_path      TEXT NOT NULL DEFAULT '',
    file_name       TEXT NOT NULL DEFAULT '',
    rows_exported   INTEGER DEFAULT NULL,
    bytes_written   INTEGER DEFAULT NULL,
    error_message   TEXT NOT NULL DEFAULT '',
    approval_id     INTEGER DEFAULT NULL,
    created_at      TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
    updated_at      TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
    deleted_at      TEXT DEFAULT NULL
);
```

---

## 7. Rust 模块设计

### 7.1 新增文件

```text
src-tauri/src/commands/database_ops.rs
src-tauri/src/services/database_ops.rs
src-tauri/src/db_runtime/mod.rs
src-tauri/src/db_runtime/mysql.rs
src-tauri/src/db_runtime/postgres.rs
src-tauri/src/db_runtime/redis.rs
src-tauri/src/db_runtime/tunnel.rs
src-tauri/src/db_runtime/sql_policy.rs
```

说明：

- `commands/database_ops.rs`：IPC 入口，只做参数承接和错误转换。
- `services/database_ops.rs`：业务编排，负责权限、审批、审计、查询历史。
- `db_runtime/*`：真实远程数据库连接和查询执行。
- `db_runtime/redis.rs`：Redis 只读浏览能力，不提供 FLUSH、CONFIG、SHUTDOWN 等高危命令入口。
- `database/mod.rs`：只负责本地 SQLite 元数据 CRUD。
- `database/schema.rs`：v11 迁移。

### 7.2 新增模型

建议添加到 `src-tauri/src/models/mod.rs`：

```rust
DatabaseConnection
UpsertDatabaseConnectionInput
DatabaseConnectionTestResult
DatabaseSchemaNode
DatabaseTableColumn
DatabaseTableIndex
DatabaseQueryInput
DatabaseQueryResult
DatabaseQueryHistory
DatabaseSavedQuery
DatabaseExportTask
DatabaseSqlRiskAssessment
RedisKeySummary
RedisValuePreview
```

所有结构体统一：

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
```

### 7.3 Command 清单

| Command | 说明 | 风险 |
| --- | --- | --- |
| `list_database_connections` | 连接列表 | 只读 |
| `upsert_database_connection` | 新增/编辑连接 | 配置变更 |
| `delete_database_connection` | 删除连接，软删除 | 配置变更 |
| `test_database_connection` | 测试连接 | 只读 |
| `list_database_schemas` | 库/Schema 列表 | 只读 |
| `list_database_tables` | 表/视图列表 | 只读 |
| `describe_database_table` | 表结构、索引 | 只读 |
| `run_database_query` | 执行 SQL | 按 SQL 风险 |
| `explain_database_query` | 执行计划 | 只读 |
| `list_redis_databases` | Redis DB 列表和 key 数量摘要 | 只读 |
| `scan_redis_keys` | Redis key scan，支持 pattern 和 cursor | 只读 |
| `get_redis_value_preview` | Redis value 预览和 TTL | 只读 |
| `list_database_query_history` | 查询历史 | 只读 |
| `save_database_query` | 收藏 SQL | 配置变更 |
| `export_database_query_result` | 导出查询结果 | 只读或审批 |

### 7.4 Service 层流程

以 `run_database_query` 为例：

```text
1. 读取 database_connections
2. 校验连接 enabled / deleted_at
3. 解析 SQL，生成 DatabaseSqlRiskAssessment
4. 根据连接 ai_policy + SQL 风险判断：
   - readonly: 可执行
   - review: 创建 approval_requests，返回等待审批
   - blocked: 直接拒绝
5. 建立数据库连接：
   - 直连
   - SSH 隧道
6. 执行查询，应用 timeout / limit
7. 结果脱敏和截断
8. 写 database_query_history
9. 写 audit_logs
10. 返回 DatabaseQueryResult
```

---

## 8. SQL 风险策略

### 8.1 风险等级

| 等级 | SQL 类型 | 默认处理 |
| --- | --- | --- |
| `readonly` | SELECT / SHOW / DESCRIBE / EXPLAIN | 允许 |
| `review` | INSERT / UPDATE / DELETE / CREATE / ALTER / DROP INDEX | 需审批 |
| `blocked` | DROP DATABASE / TRUNCATE / DELETE 无 WHERE / UPDATE 无 WHERE / GRANT / REVOKE | 禁止或强审批 |
| `unknown` | 无法解析、多语句混杂 | 默认审批 |

### 8.2 关键规则

- 禁止多语句默认执行，除非进入“审批后的脚本执行模式”。
- `SELECT *` 默认允许，但结果集按单页 500 行分页返回。
- 查询结果超过阈值时自动截断，并提示用户加 LIMIT。
- `DELETE` / `UPDATE` 无 WHERE 默认禁止。
- `DROP DATABASE`、`TRUNCATE`、`GRANT`、`REVOKE` 首版直接禁止。
- 数据库连接必须配置安全级别：
  - `approval_all`：DDL/DML/导出超阈值/备份全部创建审批请求，通过后执行。
  - `confirm_execute`：UI 用户手动触发的中风险 DDL/DML 可二次确认后执行；AI/MCP 触发仍必须进入审批。
- DDL/DML 不允许静默执行；即使是 `confirm_execute`，也必须弹出 SQL、风险、影响范围和目标连接信息。
- 审批通过后只允许执行审批时记录的 SQL fingerprint，防止审批后篡改 SQL。
- Redis 只开放 `SCAN`、`TYPE`、`TTL`、`GET`、`HGETALL`、`LRANGE`、`SMEMBERS`、`ZRANGE` 等只读查看能力。
- Redis `FLUSHALL`、`FLUSHDB`、`CONFIG`、`SHUTDOWN`、`SCRIPT`、`EVAL`、`KEYS *` 首版直接禁止。

---

## 9. 前端页面设计

### 9.1 连接列表

组件：

- `Card`
- `Table`
- `Tag`
- `Button`
- `Drawer`
- `Form`
- `Select`
- `Input.Password`

字段：

- 名称
- 分组
- 类型
- 地址
- 认证方式
- SSH 隧道
- 状态
- 最近连接
- 操作：连接测试 / 打开 / 编辑 / 删除

### 9.2 连接表单

表单分区：

1. 基础信息
   - 名称
   - 分组
   - 类型
   - Host
   - Port
   - 默认库名

2. 认证
   - 认证方式：密码引用 / 直接密码 / 无认证
   - 凭据引用下拉框
   - 直接密码输入框

3. SSH 隧道
   - 是否启用
   - SSH 服务器下拉框
   - 本地端口自动分配，不建议用户手填

4. 安全策略
   - 默认只读
   - 数据库安全级别：全部审批 / 二次确认执行
   - AI 权限级别：复用服务器 AI 权限级别枚举
   - 查询超时
   - 最大返回行数

### 9.3 SQL 控制台

布局：

```text
左侧：连接与对象树
右侧上：SQL 编辑器
右侧中：执行按钮 / 解释 / 格式化 / 收藏
右侧下：结果 Tabs
  - 结果表格
  - 执行计划
  - 消息
  - 历史
```

编辑器建议：

- 使用 Monaco Editor，复用 SFTP 文本编辑器的复杂编辑器能力。
- 支持 SQL 语法高亮。
- 支持快捷键：
  - `Cmd/Ctrl + Enter`: 执行选中 SQL 或当前 SQL
  - `Cmd/Ctrl + S`: 收藏
  - `Cmd/Ctrl + Shift + F`: 格式化

### 9.4 查询结果

结果表格要求：

- 默认单页 500 行，支持上一页 / 下一页 / 跳页分页查询。
- 支持列宽拖动。
- 支持复制单元格、复制整行。
- 支持导出 CSV。
- 大字段默认折叠，点击弹窗查看。
- 敏感字段名命中 `password`、`secret`、`token`、`key`、`credential` 时默认脱敏。

---

## 10. AI 能力设计

### 10.1 AI 可做

- 解释 SQL 作用。
- 判断 SQL 风险。
- 根据错误信息生成修复建议。
- 根据表结构生成 SELECT 查询。
- 解释 EXPLAIN 执行计划。
- 给出索引优化建议。
- 总结查询结果。

### 10.2 AI 不可直接做

- 直接执行 DDL/DML。
- 读取或输出连接密码。
- 导出大批量敏感数据。
- 绕过审批执行高危 SQL。

### 10.3 AI 权限级别

数据库管理不单独发明新的 AI 权限模型，复用服务器管理中已经存在的 AI 权限级别。数据库连接只保存当前连接选用的 `ai_policy` 值，Service 层统一解释该值：

- 禁用：AI 不可读取 schema、不可生成或执行 SQL。
- 只读：AI 只能解释 SQL、schema、错误和执行计划，不能触发查询执行。
- 低风险自动：AI 可触发只读元数据读取和安全 SELECT，结果限行、脱敏并审计。
- 中风险审核：AI 发起的变更、导出和备份全部进入审批。
- 高风险严格：AI 只能生成建议，不自动触发数据库访问。

### 10.4 AI 上下文输入

允许传给 AI：

- SQL 文本。
- 表结构摘要。
- EXPLAIN 输出。
- 错误信息。
- 已脱敏的查询结果样本。

禁止传给 AI：

- 连接密码。
- SSH 私钥。
- 未脱敏的敏感字段值。
- 超过阈值的大量业务数据。

---

## 11. MCP 工具设计

### 11.1 第一批工具

| 工具 | 说明 | 权限 |
| --- | --- | --- |
| `db_list_connections` | 返回可见连接摘要，不含凭据 | 只读 |
| `db_schema_overview` | 返回库、表、字段摘要 | 只读 |
| `db_read_query` | 执行只读 SQL | 策略允许 |
| `db_explain_query` | 返回执行计划 | 只读 |
| `db_redis_scan` | 扫描 Redis key，返回分页摘要 | 只读 |
| `db_redis_get` | 读取 Redis value 预览和 TTL | 只读 |

### 11.2 第二批工具

| 工具 | 说明 | 权限 |
| --- | --- | --- |
| `db_create_approval` | 为变更 SQL 创建审批 | 需审批 |
| `db_execute_approved` | 执行审批通过的 SQL | 审批后 |
| `db_export_query` | 导出查询结果 | 按数据量审批 |
| `db_backup_request` | 创建备份审批请求 | 需审批 |

### 11.3 MCP 返回约束

- 返回行数默认不超过 100 行。
- 返回字段默认脱敏。
- 所有工具返回结构化错误码。
- 所有 MCP 调用写审计日志。
- 连接信息只返回名称、类型、库名、host 掩码，不返回密码。

---

## 12. 审批与审计

### 12.1 审批场景

必须进入审批：

- DDL/DML。
- 导出超过阈值的数据。
- 备份数据库。
- AI/MCP 发起的任何写操作。
- 连接配置中修改 host、port、username、凭据引用。

直接禁止：

- DROP DATABASE。
- TRUNCATE。
- DELETE 无 WHERE。
- UPDATE 无 WHERE。
- GRANT / REVOKE。
- 多库批量破坏性语句。

### 12.2 审计字段

审计日志 `audit_logs` 建议记录：

| 字段 | 值 |
| --- | --- |
| `source` | `database_ui` / `database_ai` / `database_mcp` |
| `server_alias` | SSH 隧道服务器 alias，可为空 |
| `action` | `db_query` / `db_explain` / `db_export` / `db_change` |
| `risk` | `readonly` / `review` / `blocked` |
| `result` | `success` / `failed` / `blocked` / `approval_required` |
| `summary` | SQL 摘要和结果摘要 |
| `detail_json` | connectionKey、dbType、database、sqlFingerprint、durationMs、rows |
| `approval_id` | 关联审批 |

---

## 13. 安全设计

### 13.1 凭据安全

- 直接密码使用与 SSH/AI Provider 相同的加密方案。
- 凭据保险库引用只保存 key。
- 前端只看到 `hasPassword`、`passwordMasked`、`authRef`。
- 审计日志不记录密码、连接串明文。

### 13.2 查询安全

- 后端重新校验所有前端输入，不信任前端风险判断。
- SQL 使用 `sqlparser` 解析，解析失败默认审批。
- 查询强制 timeout。
- 查询结果强制 limit。
- 导出前估算行数和敏感字段。

### 13.3 隧道安全

- SSH 隧道只绑定 `127.0.0.1`。
- 本地端口自动分配，关闭连接后释放。
- 隧道会话生命周期绑定数据库查询任务。
- 失败时清理 tunnel handle。

### 13.4 下载目录与系统设置

数据库导出、查询结果 CSV、备份文件需要纳入系统设置：

- 新增系统设置项：数据库默认下载目录。
- macOS 默认值：用户 `Downloads` 目录。
- Windows 默认值：用户 `Downloads` 目录。
- 如果默认目录不可用，回退到应用数据目录下的 `database-downloads`。
- 导出前允许用户临时选择保存目录，但不改变系统默认值。
- 审计日志只记录文件名、任务 ID、文件大小和保存目录摘要，不记录敏感连接串。

---

## 14. 实施里程碑

### M1：基础数据模型与连接管理

- [ ] 迁移 v10 -> v11，新增 `database_connections`、`database_query_history`、`database_saved_queries`、`database_export_tasks`。
- [ ] Rust models 定义。
- [ ] Database 层 CRUD。
- [ ] Service 层连接配置校验。
- [ ] 系统设置新增数据库默认下载目录，默认读取 macOS / Windows 用户 Downloads 目录。
- [ ] Command 注册。
- [ ] 前端 API 与类型定义。
- [ ] 连接列表和编辑 Drawer。

验收：

- 能新增、编辑、删除数据库连接。
- 密码不返回前端明文。
- 连接列表刷新正常。

### M2：连接测试与 Schema 浏览

- [ ] MySQL 连接测试。
- [ ] PostgreSQL 连接测试。
- [ ] Redis 连接测试。
- [ ] SSH 隧道连接测试。
- [ ] 数据库/Schema/表/字段列表。
- [ ] Redis DB/key/value/TTL 只读浏览。
- [ ] 对象树 UI。

验收：

- 能通过直连或 SSH 隧道读取库和表结构。
- 连接失败能显示可理解错误。
- 测试操作写入审计日志。

### M3：只读 SQL 控制台

- [ ] SQL 编辑器。
- [ ] 只读 SQL 风险识别。
- [ ] 查询执行。
- [ ] 查询结果表格和 500 行分页。
- [ ] 查询历史。
- [ ] CSV 导出。

验收：

- SELECT/SHOW/DESCRIBE/EXPLAIN 可执行。
- DML/DDL 不会直接执行。
- 查询结果默认单页 500 行并支持分页。
- 查询历史可追溯。

### M4：审批与受控变更

- [ ] SQL 风险策略完善。
- [ ] 数据库安全级别支持全部审批和二次确认执行。
- [ ] DML/DDL 按连接安全级别创建审批请求或弹出二次确认。
- [ ] 审批通过后执行 SQL fingerprint 匹配。
- [ ] 执行结果写审计。
- [ ] UI 展示审批等待状态。

验收：

- AI/MCP 发起写 SQL 必须进审批。
- 危险 SQL 被禁止并给出原因。
- 审批通过后只执行原 SQL。

### M5：AI 辅助

- [ ] SQL 解释。
- [ ] 执行计划解释。
- [ ] 错误解释。
- [ ] 查询结果摘要。
- [ ] 敏感字段脱敏后传 AI。

验收：

- AI 不接触密码。
- AI 回复不自动执行写操作。
- AI 操作写审计。

### M6：MCP 数据库工具

- [ ] `db_list_connections`
- [ ] `db_schema_overview`
- [ ] `db_read_query`
- [ ] `db_explain_query`
- [ ] `db_redis_scan`
- [ ] `db_redis_get`
- [ ] MCP 调用接入审计。

验收：

- MCP 能读取连接摘要和 schema。
- MCP 只读查询受分页和行数限制。
- MCP Redis 工具只返回 key/value 预览，不执行写命令。
- MCP 不返回凭据。

---

## 15. 前端文件规划

```text
src/types/databaseOps.ts
src/lib/api/databaseOps.ts
src/pages/prototype/index.tsx
```

如果页面继续变大，建议从 `prototype/index.tsx` 拆出：

```text
src/pages/database/index.tsx
src/pages/database/components/ConnectionDrawer.tsx
src/pages/database/components/SchemaTree.tsx
src/pages/database/components/SqlConsole.tsx
src/pages/database/components/QueryResultTable.tsx
```

路由：

```text
/database
```

侧边栏：

```text
运维 -> 数据库管理
```

---

## 16. 后端文件规划

```text
src-tauri/src/commands/database_ops.rs
src-tauri/src/services/database_ops.rs
src-tauri/src/db_runtime/mod.rs
src-tauri/src/db_runtime/mysql.rs
src-tauri/src/db_runtime/postgres.rs
src-tauri/src/db_runtime/redis.rs
src-tauri/src/db_runtime/tunnel.rs
src-tauri/src/db_runtime/sql_policy.rs
```

需要更新：

```text
src-tauri/src/commands/mod.rs
src-tauri/src/services/mod.rs
src-tauri/src/lib.rs
src-tauri/src/models/mod.rs
src-tauri/src/database/mod.rs
src-tauri/src/database/schema.rs
src/types/index.ts
src/lib/api/index.ts
src/Router.tsx
src/components/layout/Sidebar.tsx
```

---

## 17. 验收标准

### 功能验收

- 能创建 MySQL / PostgreSQL 连接。
- 能创建 Redis 连接。
- 能通过 SSH 隧道访问内网数据库。
- 能通过数据库直连访问可达数据库。
- 能浏览库、表、字段、索引。
- 能浏览 Redis DB、key、value 预览和 TTL。
- 能执行只读 SQL 并展示结果。
- 查询结果默认单页 500 行，支持分页。
- 能导出查询结果 CSV。
- 能查看查询历史和收藏 SQL。
- DML/DDL 按连接安全级别进入全部审批或二次确认执行。
- 高危 SQL 直接禁止或进入审批。
- MCP 只读工具可用。
- AI 能解释 SQL 和执行计划。

### 安全验收

- 前端不出现数据库密码明文。
- 审计不记录密码明文。
- MCP 不返回凭据。
- 查询结果默认脱敏敏感字段。
- 超大结果集被限制。
- 写 SQL 审批通过前不会执行。
- `confirm_execute` 模式下，UI 用户二次确认前不会执行写 SQL。
- AI/MCP 发起写 SQL 不走二次确认，必须进入审批。
- 审批后执行 SQL fingerprint 不可变。

### 质量验收

- `pnpm build` 通过。
- `cargo check` 通过。
- 新增 Rust Command 均注册。
- 新增 capabilities 只按需添加。
- macOS / Windows 均可连接和查询。
- UI 暗色主题可读。

---

## 18. 风险与对策

| 风险 | 影响 | 对策 |
| --- | --- | --- |
| 数据库驱动差异大 | 查询和元数据兼容复杂 | 首版覆盖 MySQL/PostgreSQL/Redis，但按驱动能力拆分接口 |
| SSH 隧道不稳定 | 内网数据库连接失败 | tunnel 生命周期绑定任务，失败自动清理 |
| 大结果集卡 UI | WebView 卡顿 | 后端 limit + 前端虚拟表格 + 导出任务异步 |
| AI 泄露敏感数据 | 安全事故 | 字段脱敏、行数限制、禁止传凭据 |
| 审批后 SQL 被篡改 | 越权执行 | SQL fingerprint 绑定审批 |
| MCP 被滥用查询数据 | 数据泄露 | 默认只读、限行、脱敏、审计、连接级授权 |
| 跨平台依赖问题 | Windows/macOS 构建失败 | 优先纯 Rust crate，CI 双平台验证 |

---

## 19. 推荐实施顺序

优先级建议：

1. 先做连接配置 CRUD 和连接测试。
2. 再做 Schema 浏览。
3. 再做只读 SQL 控制台。
4. 再接入审批和受控写操作。
5. 再做 AI 解释。
6. 最后暴露 MCP 工具。

原因：

- 连接和 Schema 是所有后续功能基础。
- 只读查询风险低，能最快形成可用闭环。
- 写操作、AI 和 MCP 都必须依赖成熟策略和审计。

---

## 20. 已确认决策

1. V0.1 同时支持 MySQL 和 PostgreSQL。
2. Redis 进入首版，提供只读浏览能力。
3. 数据库直连和 SSH 隧道两种连接方式必须都支持。
4. 查询结果默认单页 500 行，支持分页加载。
5. 数据库安全级别可配置，首版支持 `approval_all` 全部审批和 `confirm_execute` 二次确认执行。
6. AI 权限级别复用服务器 AI 权限级别，不单独设计一套数据库 AI 权限。
7. 导出和备份默认目录纳入系统设置，默认使用 macOS / Windows 用户 Downloads 目录。
