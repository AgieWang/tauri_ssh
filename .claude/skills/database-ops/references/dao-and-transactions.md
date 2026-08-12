# rusqlite DAO 与事务参考

仅在实现查询、写入、事务、连接并发或行映射时读取。

## 连接初始化

沿用项目真实实现。当前常见模式如下，但参数必须以源码为准：

```rust
pub struct Database {
    conn: Mutex<Connection>,
}

impl Database {
    pub fn init(db_path: &str) -> Result<Self, AppError> {
        let conn = Connection::open(db_path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        schema::migrate(&conn)?;
        Ok(Self { conn: Mutex::new(conn) })
    }
}
```

`busy_timeout` 只能缓解短时锁冲突，不能替代短事务、正确锁粒度和写入队列设计。

## 查询映射

```rust
pub fn list_configs(&self) -> Result<Vec<AppConfig>, AppError> {
    let conn = self
        .conn
        .lock()
        .map_err(|error| AppError::Custom(error.to_string()))?;
    let mut stmt = conn.prepare(
        "SELECT key, value FROM app_config
         WHERE deleted_at IS NULL ORDER BY key",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(AppConfig {
            key: row.get(0)?,
            value: row.get(1)?,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(AppError::from)
}
```

- 使用显式列名，避免 `SELECT *` 在迁移后改变映射。
- 列顺序、Rust 类型、可空性必须与真实 DDL 对齐。
- 单条可缺失查询使用 `OptionalExtension::optional()`，不要把所有错误 `.ok()` 后误判成未找到。
- 大结果集使用分页/游标或流式处理，不一次性加载全部数据。

```rust
use rusqlite::OptionalExtension;

let value = stmt
    .query_row([key], |row| row.get::<_, String>(0))
    .optional()?;
```

只有 `QueryReturnedNoRows` 转为 `None`；连接、类型映射和 SQL 错误继续向上传播。

## 参数化写入与 Upsert

```rust
conn.execute(
    "INSERT INTO app_config (key, value, updated_at)
     VALUES (?1, ?2, datetime('now', 'localtime'))
     ON CONFLICT(key) DO UPDATE SET
       value = excluded.value,
       updated_at = excluded.updated_at,
       deleted_at = NULL",
    params![key, value],
)?;
```

- 值全部使用占位符。
- 表名、列名不能使用值占位符；若确需动态标识符，映射到代码白名单。
- Upsert 的冲突键必须与真实唯一约束一致。
- 返回 `affected_rows` 时明确 0 行是幂等、未找到还是失败。

## 软删除

软删除是表级约定，不是 SQLite 的自动能力：

```rust
let affected = conn.execute(
    "UPDATE app_config
     SET deleted_at = datetime('now', 'localtime')
     WHERE key = ?1 AND deleted_at IS NULL",
    [key],
)?;
```

若目标表存在软删除：

- 默认读取明确过滤 `deleted_at IS NULL`。
- 恢复时明确处理唯一键冲突。
- 物理删除使用语义明显的专用方法，并按破坏性操作审批。
- 唯一索引是否允许删除后重建同自然键，必须按真实 DDL 验证。

目标表没有软删除约定时，不擅自新增 `deleted_at`。

## 事务与锁

- 事务包含保持一致性所需的最小 SQL 集合，不把网络、文件或用户等待放进事务。
- 获取 `Mutex<Connection>` 后不跨 `.await` 持锁。
- 批量写入用事务和参数化 statement，按数据量控制 batch。
- 事务失败必须传播原始上下文；禁止只记录日志后返回成功。
- 需要“读后写”一致性时，在同一事务内完成并验证影响行数。

## 三层调用

Database 层负责 SQL，Service 层负责业务校验和事务编排边界，Command 层仅做 IPC：

```rust
#[tauri::command]
pub fn list_configs(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<AppConfig>, CommandError> {
    ConfigService::list(&state.db).map_err(Into::into)
}
```

实际 Command 错误签名以当前 `src-tauri/src/error.rs` 和同类 Command 为准。
