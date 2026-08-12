# SQLite Schema 迁移参考

仅在新增/修改本地 SQLite Schema 或核验旧库升级时读取。

## 项目模式

项目使用 `PRAGMA user_version` 和连续版本函数。实现前必须读取当前 `src-tauri/src/database/schema.rs`，不能照搬本文中的版本数字。

```rust
pub fn migrate(conn: &Connection) -> Result<(), AppError> {
    let mut version = get_version(conn)?;

    if version > SCHEMA_VERSION {
        return Err(AppError::Custom(format!(
            "数据库版本({version})高于应用支持的版本({SCHEMA_VERSION})"
        )));
    }

    while version < SCHEMA_VERSION {
        match version {
            0 => migrate_v0_to_v1(conn)?,
            1 => migrate_v1_to_v2(conn)?,
            _ => return Err(AppError::Custom(format!("未知数据库版本: {version}"))),
        }
        version = get_version(conn)?;
    }
    Ok(())
}
```

新增迁移时同时完成：

1. 将 `SCHEMA_VERSION` 递增 1。
2. 新增唯一的 `migrate_vN_to_vN+1`。
3. 在 `match` 中连接旧版本到新版本。
4. 迁移成功后才更新 `user_version`。
5. 添加从相关旧版本升级的测试，而不只测试空库。

## 事务边界

表重建、数据回填、索引切换等多个步骤必须在同一事务中完成：

```rust
fn migrate_v34_to_v35(conn: &mut Connection) -> Result<(), AppError> {
    let tx = conn.transaction()?;
    tx.execute_batch(
        "ALTER TABLE example ADD COLUMN source TEXT NOT NULL DEFAULT 'local';
         CREATE INDEX IF NOT EXISTS idx_example_source ON example(source);",
    )?;
    tx.pragma_update(None, "user_version", 35)?;
    tx.commit()?;
    Ok(())
}
```

如果项目连接接口只提供 `&Connection`，先阅读现有事务实现并沿用；不要为了套模板擅自改变全局连接类型。

## SQLite 兼容注意

- SQLite 的 `ALTER TABLE` 能力受运行版本影响。复杂字段变化通常需要“新表 → 拷贝 → 校验 → 换表”。
- `CREATE TABLE IF NOT EXISTS` 只能保证表名存在，不能证明列和约束正确。
- 新增 `NOT NULL` 列需为旧数据提供可验证默认值或分阶段回填。
- 创建唯一索引前先查询重复值和 `NULL` 语义。
- 外键行为需确认 `PRAGMA foreign_keys` 是否启用及删除策略。
- 索引字段顺序以真实查询谓词和排序为依据，不凭字段列表猜测。
- 历史迁移永久保留；已发布版本不可重写，否则旧库无法确定性升级。

## 表设计检查

- 主键、唯一键、自然键分别承担什么语义。
- 必填与可空是否和 Rust `Option<T>` 对齐。
- 时间字段格式、时区和默认表达式是否与现表一致。
- JSON 文本列是否需要版本、校验或兼容旧结构。
- 是否真的需要软删除；若目标表已有 `deleted_at`，查询和唯一约束是否正确处理。
- 索引是否服务现有读取路径，写放大是否可接受。

## 迁移测试矩阵

| 场景 | 断言 |
|---|---|
| 空库初始化 | 最终版本、全部表/列/索引存在 |
| 上一版本升级 | 数据保留、默认值正确、版本前进 |
| 受影响的更早版本升级 | 连续迁移无跳步 |
| 中途失败 | 不留下半迁移结构，或能安全重试 |
| 未来版本数据库 | 明确拒绝降级打开 |
| 大表回填 | 时间、锁和磁盘占用可接受 |

