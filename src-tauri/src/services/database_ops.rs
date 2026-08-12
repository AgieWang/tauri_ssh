use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use base64::{engine::general_purpose, Engine as _};
use rand::RngCore;
use sha2::{Digest, Sha256};
use tokio::net::TcpStream;
use tokio::time::{timeout, Duration};

use crate::database::Database;
use crate::error::AppError;
use crate::models::{
    DatabaseCellUpdateInput, DatabaseCellUpdateResult, DatabaseColumnSchema, DatabaseConnection,
    DatabaseConnectionTestResult, DatabaseEditableQueryMeta, DatabaseExportInput,
    DatabaseExportResult, DatabaseIndexSchema, DatabaseNameListInput, DatabaseNameListResult,
    DatabaseQueryInput, DatabaseQueryResult, DatabaseSchemaInput, DatabaseSchemaResult,
    DatabaseTableSchema, RedisDatabaseInfo, RedisDatabaseListInput, RedisDatabaseListResult,
    RedisDescribeKeysInput, RedisKeyEntry, RedisKeyTreeInput, RedisKeyTreeResult, RedisScanInput,
    RedisScanResult, RedisValuePreview, RedisValuePreviewInput, UpsertDatabaseConnectionInput,
};
use crate::services::credential_vault::CredentialVaultService;
use crate::services::system_settings::SystemSettingsService;

pub struct DatabaseOpsService;

const DATABASE_PASSWORD_SECRET_SEED_KEY: &str = "database_connection_password_secret_seed";

impl DatabaseOpsService {
    pub fn list_connections(db: &Database) -> Result<Vec<DatabaseConnection>, AppError> {
        db.list_database_connections()
    }

    pub fn upsert_connection(
        db: &Database,
        input: UpsertDatabaseConnectionInput,
    ) -> Result<DatabaseConnection, AppError> {
        Self::validate_connection(&input)?;
        let encrypted_password = match input
            .password
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            Some(password) => {
                let (nonce, ciphertext) = Self::encrypt_password(db, password)?;
                Some((nonce, ciphertext))
            }
            None => None,
        };
        let encrypted_ref = encrypted_password
            .as_ref()
            .map(|(nonce, ciphertext)| (nonce.as_str(), ciphertext.as_str()));
        db.upsert_database_connection(&input, encrypted_ref, input.clear_password.unwrap_or(false))
    }

    pub fn delete_connection(db: &Database, key: &str) -> Result<(), AppError> {
        if key.trim().is_empty() {
            return Err(AppError::InvalidInput("数据库连接 Key 不能为空".into()));
        }
        if !db.delete_database_connection(key)? {
            return Err(AppError::NotFound(format!("数据库连接 '{}' 不存在", key)));
        }
        Ok(())
    }

    pub async fn test_connection(
        db: &Database,
        key: &str,
    ) -> Result<DatabaseConnectionTestResult, AppError> {
        let connection = db
            .get_database_connection(key)?
            .ok_or_else(|| AppError::NotFound(format!("数据库连接 '{}' 不存在", key)))?;
        if !connection.enabled {
            return Err(AppError::InvalidInput("数据库连接已禁用".into()));
        }
        let result =
            Self::test_tcp_endpoint(&connection.key, &connection.host, connection.port).await?;
        if result.ok {
            db.update_database_connection_status(&connection.key, "online", true)?;
        } else if result.message == "TCP 连接超时" {
            db.update_database_connection_status(&connection.key, "degraded", false)?;
        } else {
            db.update_database_connection_status(&connection.key, "offline", false)?;
        }
        Ok(result)
    }

    pub async fn execute_readonly_query(
        db: &Database,
        input: DatabaseQueryInput,
    ) -> Result<DatabaseQueryResult, AppError> {
        Self::execute_sql(db, input).await
    }

    pub async fn list_database_names(
        db: &Database,
        input: DatabaseNameListInput,
    ) -> Result<DatabaseNameListResult, AppError> {
        let connection = db
            .get_database_connection_secret_row(&input.connection_key)?
            .ok_or_else(|| {
                AppError::NotFound(format!("数据库连接 '{}' 不存在", input.connection_key))
            })?;
        let connection_info = connection.connection;
        if connection_info.db_type == "redis" {
            return Err(AppError::InvalidInput("Redis 不支持 SQL 数据库选择".into()));
        }
        if connection_info.connection_mode != "direct" {
            return Err(AppError::InvalidInput(
                "SSH 隧道数据库选择会在隧道模块接入后启用".into(),
            ));
        }
        let password = Self::resolve_connection_password(
            db,
            &connection_info,
            connection.password_nonce.as_deref(),
            connection.password_ciphertext.as_deref(),
        )?;
        let databases = match connection_info.db_type.as_str() {
            "mysql" => {
                let url = Self::mysql_url(&connection_info, password.as_deref());
                Self::list_mysql_databases(&url).await?
            }
            "postgresql" => {
                let url = Self::postgres_url(&connection_info, password.as_deref());
                Self::list_postgres_databases(&url).await?
            }
            _ => return Err(AppError::InvalidInput("数据库类型无效".into())),
        };
        Ok(DatabaseNameListResult {
            connection_key: input.connection_key,
            databases,
            current: if connection_info.database_name.trim().is_empty() {
                None
            } else {
                Some(connection_info.database_name)
            },
        })
    }

    pub async fn list_database_schema(
        db: &Database,
        input: DatabaseSchemaInput,
    ) -> Result<DatabaseSchemaResult, AppError> {
        let connection = db
            .get_database_connection_secret_row(&input.connection_key)?
            .ok_or_else(|| {
                AppError::NotFound(format!("数据库连接 '{}' 不存在", input.connection_key))
            })?;
        let mut connection_info = connection.connection;
        if connection_info.db_type == "redis" {
            return Err(AppError::InvalidInput("Redis 不支持 SQL 结构补全".into()));
        }
        if connection_info.connection_mode != "direct" {
            return Err(AppError::InvalidInput(
                "SSH 隧道结构补全会在隧道模块接入后启用".into(),
            ));
        }
        if let Some(database_name) = input
            .database_name
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            connection_info.database_name = database_name.to_string();
        }
        let password = Self::resolve_connection_password(
            db,
            &connection_info,
            connection.password_nonce.as_deref(),
            connection.password_ciphertext.as_deref(),
        )?;
        let tables = match connection_info.db_type.as_str() {
            "mysql" => {
                let database_name = connection_info.database_name.trim();
                if database_name.is_empty() {
                    return Err(AppError::InvalidInput("请先选择数据库".into()));
                }
                let url = Self::mysql_url(&connection_info, password.as_deref());
                Self::list_mysql_schema(&url, database_name).await?
            }
            "postgresql" => {
                let url = Self::postgres_url(&connection_info, password.as_deref());
                Self::list_postgres_schema(&url).await?
            }
            _ => return Err(AppError::InvalidInput("数据库类型无效".into())),
        };
        Ok(DatabaseSchemaResult {
            connection_key: input.connection_key,
            database_name: if connection_info.database_name.trim().is_empty() {
                None
            } else {
                Some(connection_info.database_name)
            },
            tables,
        })
    }

    pub async fn execute_sql(
        db: &Database,
        input: DatabaseQueryInput,
    ) -> Result<DatabaseQueryResult, AppError> {
        let connection = db
            .get_database_connection_secret_row(&input.connection_key)?
            .ok_or_else(|| {
                AppError::NotFound(format!("数据库连接 '{}' 不存在", input.connection_key))
            })?;
        let mut connection_info = connection.connection;
        if connection_info.db_type == "redis" {
            return Err(AppError::InvalidInput(
                "Redis 请使用 Redis 浏览工具，不支持 SQL 查询".into(),
            ));
        }
        if connection_info.connection_mode != "direct" {
            return Err(AppError::InvalidInput(
                "SSH 隧道查询会在隧道模块接入后启用".into(),
            ));
        }
        let normalized_sql = Self::normalize_single_sql(&input.sql)?;
        if let Some(database_name) = input
            .database_name
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            connection_info.database_name = database_name.to_string();
        }
        let is_query = Self::is_result_sql(&normalized_sql);
        let page = input.page.unwrap_or(1).max(1);
        let page_size = input
            .page_size
            .unwrap_or(connection_info.page_size)
            .clamp(1, 500);
        let password = Self::resolve_connection_password(
            db,
            &connection_info,
            connection.password_nonce.as_deref(),
            connection.password_ciphertext.as_deref(),
        )?;
        let started = Instant::now();
        let statement_type = Self::statement_type(&normalized_sql);
        if is_query {
            let offset = (page - 1) * page_size;
            let can_wrap_for_pagination = Self::supports_subquery_pagination(&normalized_sql);
            let paged_sql = if can_wrap_for_pagination {
                format!(
                    "SELECT * FROM ({}) AS tauri_ssh_query_result LIMIT {} OFFSET {}",
                    normalized_sql,
                    page_size + 1,
                    offset
                )
            } else {
                normalized_sql.clone()
            };
            let (columns, column_types, mut rows) = match connection_info.db_type.as_str() {
                "mysql" => {
                    let url = Self::mysql_url(&connection_info, password.as_deref());
                    Self::query_mysql(&url, &paged_sql).await?
                }
                "postgresql" => {
                    let url = Self::postgres_url(&connection_info, password.as_deref());
                    Self::query_postgres(&url, &paged_sql).await?
                }
                _ => return Err(AppError::InvalidInput("数据库类型无效".into())),
            };
            if !can_wrap_for_pagination && offset > 0 {
                rows = rows
                    .into_iter()
                    .skip(offset as usize)
                    .take((page_size + 1) as usize)
                    .collect();
            }
            let truncated = rows.len() as i64 > page_size;
            if truncated {
                rows.truncate(page_size as usize);
            }
            let row_count = rows.len() as i64;
            let editable = Self::build_editable_query_meta(
                &connection_info,
                password.as_deref(),
                &normalized_sql,
                &columns,
            )
            .await;
            return Ok(DatabaseQueryResult {
                columns,
                column_types,
                editable: Some(editable),
                row_count,
                rows_affected: 0,
                rows,
                page,
                page_size,
                duration_ms: started.elapsed().as_millis() as i64,
                truncated,
                statement_type,
                status: "success".into(),
                message: format!("查询成功，返回 {} 行", row_count),
            });
        }
        let rows_affected = match connection_info.db_type.as_str() {
            "mysql" => {
                let url = Self::mysql_url(&connection_info, password.as_deref());
                Self::execute_mysql(&url, &normalized_sql).await?
            }
            "postgresql" => {
                let url = Self::postgres_url(&connection_info, password.as_deref());
                Self::execute_postgres(&url, &normalized_sql).await?
            }
            _ => return Err(AppError::InvalidInput("数据库类型无效".into())),
        };
        Ok(DatabaseQueryResult {
            columns: vec![],
            column_types: vec![],
            editable: None,
            row_count: 0,
            rows_affected: rows_affected as i64,
            rows: vec![],
            page,
            page_size,
            duration_ms: started.elapsed().as_millis() as i64,
            truncated: false,
            statement_type,
            status: "success".into(),
            message: format!("执行成功，影响 {} 行", rows_affected),
        })
    }

    pub async fn execute_sql_batch(
        db: &Database,
        input: DatabaseQueryInput,
    ) -> Result<Vec<DatabaseQueryResult>, AppError> {
        let statements = Self::split_sql_statements(&input.sql)?;
        let page = input.page.unwrap_or(1).max(1);
        let page_size = input.page_size.unwrap_or(500).clamp(1, 500);
        let mut results = Vec::with_capacity(statements.len());

        for statement in statements {
            let statement_type = Self::statement_type(&statement);
            let started = Instant::now();
            let item_input = DatabaseQueryInput {
                connection_key: input.connection_key.clone(),
                database_name: input.database_name.clone(),
                sql: statement.clone(),
                page: input.page,
                page_size: input.page_size,
            };

            match Self::execute_sql(db, item_input).await {
                Ok(result) => results.push(result),
                Err(error) => {
                    results.push(DatabaseQueryResult {
                        columns: vec![],
                        column_types: vec![],
                        editable: None,
                        row_count: 0,
                        rows_affected: 0,
                        rows: vec![],
                        page,
                        page_size,
                        duration_ms: started.elapsed().as_millis() as i64,
                        truncated: false,
                        statement_type,
                        status: "error".into(),
                        message: error.to_string(),
                    });
                    break;
                }
            }
        }

        Ok(results)
    }

    pub async fn update_query_result_cell(
        db: &Database,
        input: DatabaseCellUpdateInput,
    ) -> Result<DatabaseCellUpdateResult, AppError> {
        let connection = db
            .get_database_connection_secret_row(&input.connection_key)?
            .ok_or_else(|| {
                AppError::NotFound(format!("数据库连接 '{}' 不存在", input.connection_key))
            })?;
        let mut connection_info = connection.connection;
        if connection_info.db_type == "redis" {
            return Err(AppError::InvalidInput("Redis 不支持表格单元格更新".into()));
        }
        if connection_info.connection_mode != "direct" {
            return Err(AppError::InvalidInput(
                "SSH 隧道单元格更新会在隧道模块接入后启用".into(),
            ));
        }
        if connection_info.security_mode == "approval_all" {
            return Err(AppError::InvalidInput(
                "当前连接安全级别为全部审批，暂不支持直接编辑查询结果".into(),
            ));
        }
        if let Some(database_name) = input
            .database_name
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            connection_info.database_name = database_name.to_string();
        }
        let password = Self::resolve_connection_password(
            db,
            &connection_info,
            connection.password_nonce.as_deref(),
            connection.password_ciphertext.as_deref(),
        )?;
        let tables = Self::list_schema_by_connection(&connection_info, password.as_deref()).await?;
        let table =
            Self::find_table_schema(&tables, input.table_schema.as_deref(), &input.table_name)
                .ok_or_else(|| AppError::InvalidInput("目标表不存在或不可编辑".into()))?;
        if table.object_type.to_ascii_uppercase().contains("VIEW") {
            return Err(AppError::InvalidInput("视图结果暂不支持直接编辑".into()));
        }
        let primary_key_columns = Self::primary_key_columns(table)
            .ok_or_else(|| AppError::InvalidInput("目标表缺少主键或唯一键，无法安全更新".into()))?;
        let column = table
            .column_details
            .iter()
            .find(|item| item.name == input.column_name)
            .ok_or_else(|| AppError::InvalidInput("目标字段不存在".into()))?;
        if primary_key_columns.contains(&input.column_name) {
            return Err(AppError::InvalidInput(
                "主键字段不能在结果表格中直接编辑".into(),
            ));
        }
        let primary_key = input.primary_key.as_object().ok_or_else(|| {
            AppError::InvalidInput("主键条件格式无效，必须是字段到值的对象".into())
        })?;
        for primary_column in &primary_key_columns {
            if !primary_key.contains_key(primary_column) {
                return Err(AppError::InvalidInput(format!(
                    "缺少主键字段 '{}'，无法安全更新",
                    primary_column
                )));
            }
        }

        let rows_affected = match connection_info.db_type.as_str() {
            "mysql" => {
                let url = Self::mysql_url(&connection_info, password.as_deref());
                Self::update_mysql_cell(&url, table, &primary_key_columns, &input, column).await?
            }
            "postgresql" => {
                let url = Self::postgres_url(&connection_info, password.as_deref());
                Self::update_postgres_cell(&url, table, &primary_key_columns, &input, column)
                    .await?
            }
            _ => return Err(AppError::InvalidInput("数据库类型无效".into())),
        };

        if rows_affected == 0 {
            return Ok(DatabaseCellUpdateResult {
                updated: false,
                rows_affected: 0,
                message: "未更新任何行，数据可能已被其他操作修改，请重新查询后再编辑".into(),
                value: input.old_value,
            });
        }
        Ok(DatabaseCellUpdateResult {
            updated: true,
            rows_affected: rows_affected as i64,
            message: "单元格已更新".into(),
            value: input.new_value,
        })
    }

    pub async fn export_database(
        db: &Database,
        input: DatabaseExportInput,
    ) -> Result<DatabaseExportResult, AppError> {
        let connection = db
            .get_database_connection_secret_row(&input.connection_key)?
            .ok_or_else(|| {
                AppError::NotFound(format!("数据库连接 '{}' 不存在", input.connection_key))
            })?;
        let mut connection_info = connection.connection;
        if connection_info.db_type == "redis" {
            return Err(AppError::InvalidInput(
                "Redis 导出请使用 Redis 浏览页按 Key 导出".into(),
            ));
        }
        if connection_info.connection_mode != "direct" {
            return Err(AppError::InvalidInput(
                "SSH 隧道数据库导出会在隧道模块接入后启用".into(),
            ));
        }
        if let Some(database_name) = input
            .database_name
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            connection_info.database_name = database_name.to_string();
        }
        if connection_info.database_name.trim().is_empty() {
            return Err(AppError::InvalidInput("请先选择数据库".into()));
        }
        let password = Self::resolve_connection_password(
            db,
            &connection_info,
            connection.password_nonce.as_deref(),
            connection.password_ciphertext.as_deref(),
        )?;
        let mode = input.mode.trim();
        let output_dir = Self::database_export_dir(db)?;
        let timestamp = chrono::Local::now().format("%Y%m%d%H%M%S");
        match mode {
            "table_csv" | "query_csv" => {
                let max_rows = input.max_rows.unwrap_or(100_000).clamp(1, 1_000_000);
                let sql = if mode == "table_csv" {
                    let table_name = input
                        .table_name
                        .as_deref()
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .ok_or_else(|| AppError::InvalidInput("请选择要导出的数据表".into()))?;
                    format!(
                        "SELECT * FROM {} LIMIT {}",
                        Self::export_table_name(table_name, connection_info.db_type.as_str()),
                        max_rows
                    )
                } else {
                    let sql = input
                        .sql
                        .as_deref()
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .ok_or_else(|| {
                            AppError::InvalidInput("请输入要导出的 SELECT SQL".into())
                        })?;
                    if !Self::is_result_sql(sql) {
                        return Err(AppError::InvalidInput("CSV 导出只支持查询类 SQL".into()));
                    }
                    let normalized = sql.trim_end_matches(';').trim();
                    if Self::supports_subquery_pagination(normalized) {
                        format!(
                            "SELECT * FROM ({}) AS tauri_ssh_export_result LIMIT {}",
                            normalized, max_rows
                        )
                    } else {
                        normalized.to_string()
                    }
                };
                let (columns, _, rows) =
                    Self::query_by_connection(&connection_info, password.as_deref(), &sql).await?;
                let content = Self::rows_to_csv(&columns, &rows);
                let base = if mode == "table_csv" {
                    input.table_name.as_deref().unwrap_or("table")
                } else {
                    "query"
                };
                let file_name = format!(
                    "{}-{}-{}.csv",
                    Self::safe_file_part(&connection_info.database_name),
                    Self::safe_file_part(base),
                    timestamp
                );
                let file_path = output_dir.join(&file_name);
                fs::write(&file_path, content)?;
                Ok(DatabaseExportResult {
                    file_name,
                    file_path: file_path.to_string_lossy().into_owned(),
                    row_count: rows.len() as i64,
                    table_count: 0,
                    mode: mode.into(),
                    message: format!("CSV 已导出 {} 行", rows.len()),
                })
            }
            "sql_backup" => {
                let include_data = input.include_data.unwrap_or(true);
                let schema = Self::list_database_schema(
                    db,
                    DatabaseSchemaInput {
                        connection_key: input.connection_key.clone(),
                        database_name: Some(connection_info.database_name.clone()),
                    },
                )
                .await?;
                let selected_table = input
                    .table_name
                    .as_deref()
                    .map(str::trim)
                    .filter(|v| !v.is_empty());
                let tables = schema
                    .tables
                    .into_iter()
                    .filter(|table| {
                        selected_table
                            .map(|name| name == table.name)
                            .unwrap_or(true)
                    })
                    .collect::<Vec<_>>();
                if tables.is_empty() {
                    return Err(AppError::InvalidInput("未找到可备份的数据表".into()));
                }
                let mut content = String::new();
                content.push_str(&format!(
                    "-- Tauri SSH database backup\n-- database: {}\n-- generated at: {}\n\n",
                    connection_info.database_name,
                    chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
                ));
                let mut row_count = 0_i64;
                for table in &tables {
                    content.push_str(
                        &Self::create_table_backup_sql(
                            &connection_info,
                            password.as_deref(),
                            table,
                        )
                        .await?,
                    );
                    if include_data && !table.object_type.to_uppercase().contains("VIEW") {
                        let sql = format!(
                            "SELECT * FROM {}",
                            Self::export_table_name(&table.name, connection_info.db_type.as_str())
                        );
                        let (columns, _, rows) =
                            Self::query_by_connection(&connection_info, password.as_deref(), &sql)
                                .await?;
                        row_count += rows.len() as i64;
                        content.push_str(&Self::insert_backup_sql(
                            &table.name,
                            &columns,
                            &rows,
                            connection_info.db_type.as_str(),
                        ));
                    }
                    content.push('\n');
                }
                let scope = selected_table.unwrap_or("database");
                let file_name = format!(
                    "{}-{}-{}.sql",
                    Self::safe_file_part(&connection_info.database_name),
                    Self::safe_file_part(scope),
                    timestamp
                );
                let file_path = output_dir.join(&file_name);
                fs::write(&file_path, content)?;
                Ok(DatabaseExportResult {
                    file_name,
                    file_path: file_path.to_string_lossy().into_owned(),
                    row_count,
                    table_count: tables.len() as i64,
                    mode: mode.into(),
                    message: format!("SQL 备份已导出 {} 张表，{} 行数据", tables.len(), row_count),
                })
            }
            _ => Err(AppError::InvalidInput("导出模式无效".into())),
        }
    }

    pub async fn scan_redis_keys(
        db: &Database,
        input: RedisScanInput,
    ) -> Result<RedisScanResult, AppError> {
        let connection = db
            .get_database_connection_secret_row(&input.connection_key)?
            .ok_or_else(|| {
                AppError::NotFound(format!("Redis 连接 '{}' 不存在", input.connection_key))
            })?;
        let mut connection_info = connection.connection;
        if connection_info.db_type != "redis" {
            return Err(AppError::InvalidInput("当前连接不是 Redis".into()));
        }
        if connection_info.connection_mode != "direct" {
            return Err(AppError::InvalidInput(
                "SSH 隧道 Redis 浏览会在隧道模块接入后启用".into(),
            ));
        }
        if let Some(database_name) = input
            .database_name
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            connection_info.database_name = database_name.to_string();
        }
        let password = Self::resolve_connection_password(
            db,
            &connection_info,
            connection.password_nonce.as_deref(),
            connection.password_ciphertext.as_deref(),
        )?;
        let url = Self::redis_url(&connection_info, password.as_deref());
        let client = redis::Client::open(url)
            .map_err(|error| AppError::Custom(format!("Redis URL 无效: {}", error)))?;
        let mut conn = client
            .get_multiplexed_async_connection()
            .await
            .map_err(|error| AppError::Custom(format!("连接 Redis 失败: {}", error)))?;
        let cursor = input.cursor.unwrap_or(0);
        let count = input.count.unwrap_or(100).clamp(1, 500) as usize;
        let pattern = input.pattern.unwrap_or_else(|| "*".into());
        let (next_cursor, keys): (u64, Vec<String>) = redis::cmd("SCAN")
            .arg(cursor)
            .arg("MATCH")
            .arg(pattern)
            .arg("COUNT")
            .arg(count)
            .query_async(&mut conn)
            .await
            .map_err(|error| AppError::Custom(format!("扫描 Redis Key 失败: {}", error)))?;
        let mut entries = Vec::with_capacity(keys.len());
        for key in keys {
            let key_type: String = redis::cmd("TYPE")
                .arg(&key)
                .query_async(&mut conn)
                .await
                .unwrap_or_else(|_| "unknown".into());
            let ttl: i64 = redis::cmd("TTL")
                .arg(&key)
                .query_async(&mut conn)
                .await
                .unwrap_or(-2);
            entries.push(RedisKeyEntry { key, key_type, ttl });
        }
        Ok(RedisScanResult {
            cursor: next_cursor,
            keys: entries,
        })
    }

    pub async fn describe_redis_keys(
        db: &Database,
        input: RedisDescribeKeysInput,
    ) -> Result<RedisScanResult, AppError> {
        let connection = db
            .get_database_connection_secret_row(&input.connection_key)?
            .ok_or_else(|| {
                AppError::NotFound(format!("Redis 连接 '{}' 不存在", input.connection_key))
            })?;
        let mut connection_info = connection.connection;
        if connection_info.db_type != "redis" {
            return Err(AppError::InvalidInput("当前连接不是 Redis".into()));
        }
        if connection_info.connection_mode != "direct" {
            return Err(AppError::InvalidInput(
                "SSH 隧道 Redis 浏览会在隧道模块接入后启用".into(),
            ));
        }
        if let Some(database_name) = input
            .database_name
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            connection_info.database_name = database_name.to_string();
        }
        let password = Self::resolve_connection_password(
            db,
            &connection_info,
            connection.password_nonce.as_deref(),
            connection.password_ciphertext.as_deref(),
        )?;
        let url = Self::redis_url(&connection_info, password.as_deref());
        let client = redis::Client::open(url)
            .map_err(|error| AppError::Custom(format!("Redis URL 无效: {}", error)))?;
        let mut conn = client
            .get_multiplexed_async_connection()
            .await
            .map_err(|error| AppError::Custom(format!("连接 Redis 失败: {}", error)))?;

        // 树节点已经持有 Key 快照；这里只补齐当前页的类型和 TTL，避免再次 SCAN 造成数量与列表不一致。
        let mut entries = Vec::with_capacity(input.keys.len().min(500));
        for key in input.keys.into_iter().take(500) {
            let key_type: String = redis::cmd("TYPE")
                .arg(&key)
                .query_async(&mut conn)
                .await
                .unwrap_or_else(|_| "unknown".into());
            let ttl: i64 = redis::cmd("TTL")
                .arg(&key)
                .query_async(&mut conn)
                .await
                .unwrap_or(-2);
            entries.push(RedisKeyEntry { key, key_type, ttl });
        }

        Ok(RedisScanResult {
            cursor: 0,
            keys: entries,
        })
    }

    pub async fn list_redis_databases(
        db: &Database,
        input: RedisDatabaseListInput,
    ) -> Result<RedisDatabaseListResult, AppError> {
        let connection = db
            .get_database_connection_secret_row(&input.connection_key)?
            .ok_or_else(|| {
                AppError::NotFound(format!("Redis 连接 '{}' 不存在", input.connection_key))
            })?;
        let connection_info = connection.connection;
        if connection_info.db_type != "redis" {
            return Err(AppError::InvalidInput("当前连接不是 Redis".into()));
        }
        if connection_info.connection_mode != "direct" {
            return Err(AppError::InvalidInput(
                "SSH 隧道 Redis 浏览会在隧道模块接入后启用".into(),
            ));
        }
        let password = Self::resolve_connection_password(
            db,
            &connection_info,
            connection.password_nonce.as_deref(),
            connection.password_ciphertext.as_deref(),
        )?;
        let url = Self::redis_url(&connection_info, password.as_deref());
        let client = redis::Client::open(url)
            .map_err(|error| AppError::Custom(format!("Redis URL 无效: {}", error)))?;
        let mut conn = client
            .get_multiplexed_async_connection()
            .await
            .map_err(|error| AppError::Custom(format!("连接 Redis 失败: {}", error)))?;
        let database_count = redis::cmd("CONFIG")
            .arg("GET")
            .arg("databases")
            .query_async::<Vec<String>>(&mut conn)
            .await
            .ok()
            .and_then(|values| values.get(1).and_then(|value| value.parse::<u8>().ok()))
            .unwrap_or(16)
            .clamp(1, 64);
        let current = connection_info
            .database_name
            .trim()
            .parse::<u8>()
            .unwrap_or(0)
            .min(database_count.saturating_sub(1));
        let mut databases = Vec::with_capacity(database_count as usize);
        for index in 0..database_count {
            let _: () = redis::cmd("SELECT")
                .arg(index)
                .query_async(&mut conn)
                .await
                .map_err(|error| {
                    AppError::Custom(format!("选择 Redis DB {} 失败: {}", index, error))
                })?;
            let key_count: i64 =
                redis::cmd("DBSIZE")
                    .query_async(&mut conn)
                    .await
                    .map_err(|error| {
                        AppError::Custom(format!("读取 Redis DB {} Key 数失败: {}", index, error))
                    })?;
            databases.push(RedisDatabaseInfo {
                name: index.to_string(),
                index,
                key_count,
            });
        }
        Ok(RedisDatabaseListResult {
            connection_key: input.connection_key,
            current: current.to_string(),
            databases,
        })
    }

    pub async fn list_redis_key_tree(
        db: &Database,
        input: RedisKeyTreeInput,
    ) -> Result<RedisKeyTreeResult, AppError> {
        let connection = db
            .get_database_connection_secret_row(&input.connection_key)?
            .ok_or_else(|| {
                AppError::NotFound(format!("Redis 连接 '{}' 不存在", input.connection_key))
            })?;
        let mut connection_info = connection.connection;
        if connection_info.db_type != "redis" {
            return Err(AppError::InvalidInput("当前连接不是 Redis".into()));
        }
        if connection_info.connection_mode != "direct" {
            return Err(AppError::InvalidInput(
                "SSH 隧道 Redis Key 树会在隧道模块接入后启用".into(),
            ));
        }
        if let Some(database_name) = input
            .database_name
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            connection_info.database_name = database_name.to_string();
        }
        let password = Self::resolve_connection_password(
            db,
            &connection_info,
            connection.password_nonce.as_deref(),
            connection.password_ciphertext.as_deref(),
        )?;
        let url = Self::redis_url(&connection_info, password.as_deref());
        let client = redis::Client::open(url)
            .map_err(|error| AppError::Custom(format!("Redis URL 无效: {}", error)))?;
        let mut conn = client
            .get_multiplexed_async_connection()
            .await
            .map_err(|error| AppError::Custom(format!("连接 Redis 失败: {}", error)))?;
        let pattern = input
            .pattern
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("*")
            .to_string();
        let limit = input.limit.unwrap_or(20_000).clamp(100, 20_000) as usize;
        let mut cursor = 0_u64;
        let mut keys = Vec::new();

        loop {
            let (next_cursor, mut batch): (u64, Vec<String>) = redis::cmd("SCAN")
                .arg(cursor)
                .arg("MATCH")
                .arg(&pattern)
                .arg("COUNT")
                .arg(1000)
                .query_async(&mut conn)
                .await
                .map_err(|error| AppError::Custom(format!("扫描 Redis Key 树失败: {}", error)))?;
            cursor = next_cursor;
            keys.append(&mut batch);
            if cursor == 0 || keys.len() >= limit {
                break;
            }
        }

        let truncated = cursor != 0 || keys.len() > limit;
        keys.truncate(limit);
        keys.sort();

        Ok(RedisKeyTreeResult {
            connection_key: input.connection_key,
            database_name: if connection_info.database_name.trim().is_empty() {
                None
            } else {
                Some(connection_info.database_name)
            },
            pattern,
            total_scanned: keys.len() as i64,
            truncated,
            keys,
        })
    }

    pub async fn get_redis_value_preview(
        db: &Database,
        input: RedisValuePreviewInput,
    ) -> Result<RedisValuePreview, AppError> {
        if input.key.trim().is_empty() {
            return Err(AppError::InvalidInput("Redis Key 不能为空".into()));
        }
        let connection = db
            .get_database_connection_secret_row(&input.connection_key)?
            .ok_or_else(|| {
                AppError::NotFound(format!("Redis 连接 '{}' 不存在", input.connection_key))
            })?;
        let mut connection_info = connection.connection;
        if connection_info.db_type != "redis" {
            return Err(AppError::InvalidInput("当前连接不是 Redis".into()));
        }
        if let Some(database_name) = input
            .database_name
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            connection_info.database_name = database_name.to_string();
        }
        let password = Self::resolve_connection_password(
            db,
            &connection_info,
            connection.password_nonce.as_deref(),
            connection.password_ciphertext.as_deref(),
        )?;
        let url = Self::redis_url(&connection_info, password.as_deref());
        let client = redis::Client::open(url)
            .map_err(|error| AppError::Custom(format!("Redis URL 无效: {}", error)))?;
        let mut conn = client
            .get_multiplexed_async_connection()
            .await
            .map_err(|error| AppError::Custom(format!("连接 Redis 失败: {}", error)))?;
        let key_type: String = redis::cmd("TYPE")
            .arg(&input.key)
            .query_async(&mut conn)
            .await
            .map_err(|error| AppError::Custom(format!("读取 Redis Key 类型失败: {}", error)))?;
        let ttl: i64 = redis::cmd("TTL")
            .arg(&input.key)
            .query_async(&mut conn)
            .await
            .unwrap_or(-2);
        let preview = match key_type.as_str() {
            "string" => {
                let value: String = redis::cmd("GET")
                    .arg(&input.key)
                    .query_async(&mut conn)
                    .await
                    .unwrap_or_default();
                serde_json::Value::String(value)
            }
            "list" => {
                let values: Vec<String> = redis::cmd("LRANGE")
                    .arg(&input.key)
                    .arg(0)
                    .arg(99)
                    .query_async(&mut conn)
                    .await
                    .unwrap_or_default();
                serde_json::json!(values)
            }
            "set" => {
                let values: Vec<String> = redis::cmd("SSCAN")
                    .arg(&input.key)
                    .arg(0)
                    .arg("COUNT")
                    .arg(100)
                    .query_async::<(u64, Vec<String>)>(&mut conn)
                    .await
                    .map(|(_, values)| values)
                    .unwrap_or_default();
                serde_json::json!(values)
            }
            "zset" => {
                let values: Vec<String> = redis::cmd("ZRANGE")
                    .arg(&input.key)
                    .arg(0)
                    .arg(99)
                    .arg("WITHSCORES")
                    .query_async(&mut conn)
                    .await
                    .unwrap_or_default();
                serde_json::json!(values)
            }
            "hash" => {
                let values: Vec<(String, String)> = redis::cmd("HSCAN")
                    .arg(&input.key)
                    .arg(0)
                    .arg("COUNT")
                    .arg(100)
                    .query_async::<(u64, Vec<(String, String)>)>(&mut conn)
                    .await
                    .map(|(_, values)| values)
                    .unwrap_or_default();
                serde_json::json!(values)
            }
            _ => serde_json::json!({ "message": "暂不支持预览该 Redis 类型" }),
        };
        Ok(RedisValuePreview {
            key: input.key,
            key_type,
            ttl,
            preview,
        })
    }

    pub async fn execute_redis_write_command(
        db: &Database,
        connection_key: &str,
        database_name: Option<String>,
        command: &str,
        args: Vec<String>,
    ) -> Result<serde_json::Value, AppError> {
        let command = command.trim().to_uppercase();
        if command.is_empty() {
            return Err(AppError::InvalidInput("Redis 命令不能为空".into()));
        }
        let allowed = ["SET", "DEL", "EXPIRE", "HSET", "HDEL"];
        if !allowed.contains(&command.as_str()) {
            return Err(AppError::InvalidInput(format!(
                "Redis 批准执行仅支持 {}，当前命令为 {}",
                allowed.join("/"),
                command
            )));
        }
        let connection = db
            .get_database_connection_secret_row(connection_key)?
            .ok_or_else(|| AppError::NotFound(format!("Redis 连接 '{}' 不存在", connection_key)))?;
        let mut connection_info = connection.connection;
        if connection_info.db_type != "redis" {
            return Err(AppError::InvalidInput("当前连接不是 Redis".into()));
        }
        if connection_info.connection_mode != "direct" {
            return Err(AppError::InvalidInput(
                "SSH 隧道 Redis 写入会在隧道模块接入后启用".into(),
            ));
        }
        if let Some(database_name) = database_name
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            connection_info.database_name = database_name.to_string();
        }
        let expected_args = match command.as_str() {
            "SET" => 2,
            "DEL" => 1,
            "EXPIRE" => 2,
            "HSET" => 3,
            "HDEL" => 2,
            _ => unreachable!(),
        };
        if args.len() < expected_args {
            return Err(AppError::InvalidInput(format!(
                "Redis {} 至少需要 {} 个参数",
                command, expected_args
            )));
        }
        let password = Self::resolve_connection_password(
            db,
            &connection_info,
            connection.password_nonce.as_deref(),
            connection.password_ciphertext.as_deref(),
        )?;
        let url = Self::redis_url(&connection_info, password.as_deref());
        let client = redis::Client::open(url)
            .map_err(|error| AppError::Custom(format!("Redis URL 无效: {}", error)))?;
        let mut conn = client
            .get_multiplexed_async_connection()
            .await
            .map_err(|error| AppError::Custom(format!("连接 Redis 失败: {}", error)))?;
        let mut redis_command = redis::cmd(command.as_str());
        for arg in &args {
            redis_command.arg(arg);
        }
        let value: redis::Value = redis_command
            .query_async(&mut conn)
            .await
            .map_err(|error| AppError::Custom(format!("执行 Redis 命令失败: {}", error)))?;
        Ok(serde_json::json!({
            "connectionKey": connection_key,
            "databaseName": connection_info.database_name,
            "command": command,
            "argsCount": args.len(),
            "result": format!("{:?}", value)
        }))
    }

    pub async fn create_redis_acl_user(
        db: &Database,
        connection_key: &str,
        database_name: Option<String>,
        username: &str,
        password: &str,
    ) -> Result<(), AppError> {
        if username.trim().is_empty() {
            return Err(AppError::InvalidInput("Redis ACL 用户名不能为空".into()));
        }
        if password.trim().is_empty() {
            return Err(AppError::InvalidInput("Redis ACL 密码不能为空".into()));
        }
        let connection = db
            .get_database_connection_secret_row(connection_key)?
            .ok_or_else(|| AppError::NotFound(format!("Redis 连接 '{}' 不存在", connection_key)))?;
        let mut connection_info = connection.connection;
        if connection_info.db_type != "redis" {
            return Err(AppError::InvalidInput("当前连接不是 Redis".into()));
        }
        if connection_info.connection_mode != "direct" {
            return Err(AppError::InvalidInput(
                "SSH 隧道 Redis ACL 创建会在隧道模块接入后启用".into(),
            ));
        }
        if let Some(database_name) = database_name
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            connection_info.database_name = database_name.to_string();
        }
        let password_ref = Self::resolve_connection_password(
            db,
            &connection_info,
            connection.password_nonce.as_deref(),
            connection.password_ciphertext.as_deref(),
        )?;
        let url = Self::redis_url(&connection_info, password_ref.as_deref());
        let client = redis::Client::open(url)
            .map_err(|error| AppError::Custom(format!("Redis URL 无效: {}", error)))?;
        let mut conn = client
            .get_multiplexed_async_connection()
            .await
            .map_err(|error| AppError::Custom(format!("连接 Redis 失败: {}", error)))?;
        let _: redis::Value = redis::cmd("ACL")
            .arg("SETUSER")
            .arg(username)
            .arg("on")
            .arg(format!(">{}", password))
            .arg("~*")
            .arg("+@all")
            .query_async(&mut conn)
            .await
            .map_err(|error| AppError::Custom(format!("创建 Redis ACL 用户失败: {}", error)))?;
        Ok(())
    }

    fn database_export_dir(db: &Database) -> Result<PathBuf, AppError> {
        let settings = SystemSettingsService::get(db)?;
        let configured = settings.database_download_dir.trim();
        let dir = if configured.is_empty() || configured.starts_with("应用数据目录") {
            Self::default_download_dir()
        } else {
            PathBuf::from(configured)
        };
        fs::create_dir_all(&dir)?;
        Ok(dir)
    }

    fn default_download_dir() -> PathBuf {
        let home = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .unwrap_or_default();
        if home.trim().is_empty() {
            return PathBuf::from(".");
        }
        Path::new(&home).join("Downloads")
    }

    async fn query_by_connection(
        connection: &DatabaseConnection,
        password: Option<&str>,
        sql: &str,
    ) -> Result<(Vec<String>, Vec<String>, Vec<serde_json::Value>), AppError> {
        match connection.db_type.as_str() {
            "mysql" => {
                let url = Self::mysql_url(connection, password);
                Self::query_mysql(&url, sql).await
            }
            "postgresql" => {
                let url = Self::postgres_url(connection, password);
                Self::query_postgres(&url, sql).await
            }
            _ => Err(AppError::InvalidInput("数据库类型无效".into())),
        }
    }

    async fn list_schema_by_connection(
        connection: &DatabaseConnection,
        password: Option<&str>,
    ) -> Result<Vec<DatabaseTableSchema>, AppError> {
        match connection.db_type.as_str() {
            "mysql" => {
                let database_name = connection.database_name.trim();
                if database_name.is_empty() {
                    return Err(AppError::InvalidInput("请先选择数据库".into()));
                }
                let url = Self::mysql_url(connection, password);
                Self::list_mysql_schema(&url, database_name).await
            }
            "postgresql" => {
                let url = Self::postgres_url(connection, password);
                Self::list_postgres_schema(&url).await
            }
            _ => Err(AppError::InvalidInput("数据库类型无效".into())),
        }
    }

    async fn build_editable_query_meta(
        connection: &DatabaseConnection,
        password: Option<&str>,
        sql: &str,
        result_columns: &[String],
    ) -> DatabaseEditableQueryMeta {
        let Some(target) = Self::parse_simple_select_table(sql) else {
            return Self::disabled_editable_meta("仅简单单表 SELECT 查询支持直接编辑结果");
        };
        let Ok(tables) = Self::list_schema_by_connection(connection, password).await else {
            return Self::disabled_editable_meta("无法读取表结构，结果暂不可编辑");
        };
        let Some(table) = Self::find_table_schema(&tables, target.schema.as_deref(), &target.table)
        else {
            return Self::disabled_editable_meta("查询目标表不存在，结果暂不可编辑");
        };
        if table.object_type.to_ascii_uppercase().contains("VIEW") {
            return Self::disabled_editable_meta("视图结果暂不支持直接编辑");
        }
        let table_columns = table
            .columns
            .iter()
            .cloned()
            .collect::<std::collections::BTreeSet<_>>();
        let primary_key_columns = match Self::primary_key_columns(table) {
            Some(columns) => columns,
            None => return Self::disabled_editable_meta("目标表缺少主键或唯一键，无法安全编辑"),
        };
        let mut readonly_columns = Vec::new();
        let mut editable_columns = Vec::new();
        for column in result_columns {
            if !table_columns.contains(column) || primary_key_columns.contains(column) {
                readonly_columns.push(column.clone());
            } else {
                editable_columns.push(column.clone());
            }
        }
        let missing_primary_key = primary_key_columns
            .iter()
            .any(|column| !result_columns.contains(column));
        let mut enabled = !editable_columns.is_empty() && !missing_primary_key;
        let mut reason = if missing_primary_key {
            "查询结果缺少主键/唯一键字段，无法定位要更新的行".to_string()
        } else if editable_columns.is_empty() {
            "查询结果没有可编辑的真实表字段".to_string()
        } else {
            "可编辑".to_string()
        };
        if connection.security_mode == "approval_all" {
            enabled = false;
            reason = "当前连接安全级别为全部审批，暂不支持直接编辑查询结果".into();
        }
        DatabaseEditableQueryMeta {
            enabled,
            reason,
            table_name: Some(table.name.clone()),
            table_schema: table.schema_name.clone().or(target.schema),
            primary_key_columns,
            editable_columns,
            readonly_columns,
        }
    }

    fn disabled_editable_meta(reason: &str) -> DatabaseEditableQueryMeta {
        DatabaseEditableQueryMeta {
            enabled: false,
            reason: reason.into(),
            table_name: None,
            table_schema: None,
            primary_key_columns: vec![],
            editable_columns: vec![],
            readonly_columns: vec![],
        }
    }

    fn find_table_schema<'a>(
        tables: &'a [DatabaseTableSchema],
        schema_name: Option<&str>,
        table_name: &str,
    ) -> Option<&'a DatabaseTableSchema> {
        tables.iter().find(|table| {
            table.name == table_name
                && schema_name
                    .map(|schema| table.schema_name.as_deref().unwrap_or("") == schema)
                    .unwrap_or(true)
        })
    }

    fn primary_key_columns(table: &DatabaseTableSchema) -> Option<Vec<String>> {
        table
            .indexes
            .iter()
            .find(|index| {
                index.name.eq_ignore_ascii_case("PRIMARY") || index.name.ends_with("_pkey")
            })
            .or_else(|| table.indexes.iter().find(|index| index.unique))
            .map(|index| index.columns.clone())
            .filter(|columns| !columns.is_empty())
    }

    async fn update_mysql_cell(
        url: &str,
        table: &DatabaseTableSchema,
        primary_key_columns: &[String],
        input: &DatabaseCellUpdateInput,
        column: &DatabaseColumnSchema,
    ) -> Result<u64, AppError> {
        use sqlx::mysql::MySqlPoolOptions;

        let pool = MySqlPoolOptions::new()
            .max_connections(1)
            .acquire_timeout(Duration::from_secs(5))
            .connect(url)
            .await
            .map_err(|error| AppError::Custom(format!("连接 MySQL 失败: {}", error)))?;
        let pk_object = input.primary_key.as_object().ok_or_else(|| {
            AppError::InvalidInput("主键条件格式无效，必须是字段到值的对象".into())
        })?;
        let mut where_parts = primary_key_columns
            .iter()
            .map(|column| format!("{} <=> ?", Self::export_table_name(column, "mysql")))
            .collect::<Vec<_>>();
        where_parts.push(format!(
            "{} <=> ?",
            Self::export_table_name(&input.column_name, "mysql")
        ));
        let sql = format!(
            "UPDATE {} SET {} = ? WHERE {}",
            Self::qualified_table_name(table.schema_name.as_deref(), &table.name, "mysql"),
            Self::export_table_name(&input.column_name, "mysql"),
            where_parts.join(" AND ")
        );
        // 标识符来自已校验的表结构与转义函数，值仍全部通过绑定参数传递。
        let mut query = sqlx::query(sqlx::AssertSqlSafe(sql)).bind(Self::json_to_db_string(
            &input.new_value,
            &column.data_type,
            "mysql",
        ));
        for primary_column in primary_key_columns {
            query = query.bind(Self::json_to_db_string(
                pk_object
                    .get(primary_column)
                    .unwrap_or(&serde_json::Value::Null),
                "",
                "mysql",
            ));
        }
        query = query.bind(Self::json_to_db_string(
            &input.old_value,
            &column.data_type,
            "mysql",
        ));
        let result = query
            .execute(&pool)
            .await
            .map_err(|error| AppError::Custom(format!("更新 MySQL 单元格失败: {}", error)))?;
        Ok(result.rows_affected())
    }

    async fn update_postgres_cell(
        url: &str,
        table: &DatabaseTableSchema,
        primary_key_columns: &[String],
        input: &DatabaseCellUpdateInput,
        column: &DatabaseColumnSchema,
    ) -> Result<u64, AppError> {
        use sqlx::postgres::PgPoolOptions;

        let pool = PgPoolOptions::new()
            .max_connections(1)
            .acquire_timeout(Duration::from_secs(5))
            .connect(url)
            .await
            .map_err(|error| AppError::Custom(format!("连接 PostgreSQL 失败: {}", error)))?;
        let pk_object = input.primary_key.as_object().ok_or_else(|| {
            AppError::InvalidInput("主键条件格式无效，必须是字段到值的对象".into())
        })?;
        let mut next_param = 2;
        let mut where_parts = Vec::new();
        for primary_column in primary_key_columns {
            let primary_type = table
                .column_details
                .iter()
                .find(|item| item.name == *primary_column)
                .map(Self::postgres_cast_type)
                .unwrap_or_else(|| "text".into());
            where_parts.push(format!(
                "{} IS NOT DISTINCT FROM ${}::{}",
                Self::export_table_name(primary_column, "postgresql"),
                next_param,
                primary_type
            ));
            next_param += 1;
        }
        let old_value_param = next_param;
        let column_type = Self::postgres_cast_type(column);
        where_parts.push(format!(
            "{} IS NOT DISTINCT FROM ${}::{}",
            Self::export_table_name(&input.column_name, "postgresql"),
            old_value_param,
            column_type
        ));
        let sql = format!(
            "UPDATE {} SET {} = $1::{} WHERE {}",
            Self::qualified_table_name(table.schema_name.as_deref(), &table.name, "postgresql"),
            Self::export_table_name(&input.column_name, "postgresql"),
            column_type,
            where_parts.join(" AND ")
        );
        // 标识符来自已校验的表结构与转义函数，值仍全部通过绑定参数传递。
        let mut query = sqlx::query(sqlx::AssertSqlSafe(sql)).bind(Self::json_to_db_string(
            &input.new_value,
            &column.data_type,
            "postgresql",
        ));
        for primary_column in primary_key_columns {
            let primary_type = table
                .column_details
                .iter()
                .find(|item| item.name == *primary_column)
                .map(|item| item.data_type.as_str())
                .unwrap_or("");
            query = query.bind(Self::json_to_db_string(
                pk_object
                    .get(primary_column)
                    .unwrap_or(&serde_json::Value::Null),
                primary_type,
                "postgresql",
            ));
        }
        query = query.bind(Self::json_to_db_string(
            &input.old_value,
            &column.data_type,
            "postgresql",
        ));
        let result = query
            .execute(&pool)
            .await
            .map_err(|error| AppError::Custom(format!("更新 PostgreSQL 单元格失败: {}", error)))?;
        Ok(result.rows_affected())
    }

    async fn create_table_backup_sql(
        connection: &DatabaseConnection,
        password: Option<&str>,
        table: &DatabaseTableSchema,
    ) -> Result<String, AppError> {
        if connection.db_type == "mysql" {
            let sql = format!(
                "SHOW CREATE TABLE {}",
                Self::export_table_name(&table.name, "mysql")
            );
            let (_, _, rows) = Self::query_by_connection(connection, password, &sql).await?;
            let create_sql = rows
                .first()
                .and_then(|row| row.as_object())
                .and_then(|object| {
                    object
                        .get("Create Table")
                        .or_else(|| object.get("Create View"))
                })
                .and_then(|value| value.as_str())
                .ok_or_else(|| AppError::Custom(format!("读取 {} 的建表语句失败", table.name)))?;
            return Ok(format!(
                "DROP TABLE IF EXISTS {};\n{};\n\n",
                Self::export_table_name(&table.name, "mysql"),
                create_sql
            ));
        }

        let columns = table
            .column_details
            .iter()
            .map(|column| {
                let mut parts = vec![
                    Self::export_table_name(&column.name, "postgresql"),
                    if column.column_type.is_empty() {
                        column.data_type.clone()
                    } else {
                        column.column_type.clone()
                    },
                ];
                if !column.nullable {
                    parts.push("NOT NULL".into());
                }
                if let Some(default_value) = &column.default_value {
                    parts.push(format!("DEFAULT {}", default_value));
                }
                parts.join(" ")
            })
            .collect::<Vec<_>>();
        let primary = table
            .indexes
            .iter()
            .find(|index| index.name == "PRIMARY" || index.name.ends_with("_pkey"))
            .filter(|index| !index.columns.is_empty())
            .map(|index| {
                format!(
                    ",\n  PRIMARY KEY ({})",
                    index
                        .columns
                        .iter()
                        .map(|name| Self::export_table_name(name, "postgresql"))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            })
            .unwrap_or_default();
        Ok(format!(
            "DROP TABLE IF EXISTS {};\nCREATE TABLE {} (\n  {}{}\n);\n\n",
            Self::export_table_name(&table.name, "postgresql"),
            Self::export_table_name(&table.name, "postgresql"),
            columns.join(",\n  "),
            primary
        ))
    }

    fn insert_backup_sql(
        table_name: &str,
        columns: &[String],
        rows: &[serde_json::Value],
        db_type: &str,
    ) -> String {
        if columns.is_empty() || rows.is_empty() {
            return String::new();
        }
        let column_sql = columns
            .iter()
            .map(|column| Self::export_table_name(column, db_type))
            .collect::<Vec<_>>()
            .join(", ");
        let mut sql = String::new();
        for row in rows {
            let values = columns
                .iter()
                .map(|column| {
                    row.as_object()
                        .and_then(|object| object.get(column))
                        .map(Self::json_to_sql_literal)
                        .unwrap_or_else(|| "NULL".into())
                })
                .collect::<Vec<_>>()
                .join(", ");
            sql.push_str(&format!(
                "INSERT INTO {} ({}) VALUES ({});\n",
                Self::export_table_name(table_name, db_type),
                column_sql,
                values
            ));
        }
        sql.push('\n');
        sql
    }

    fn rows_to_csv(columns: &[String], rows: &[serde_json::Value]) -> String {
        let mut content = String::new();
        content.push_str(
            &columns
                .iter()
                .map(|column| Self::csv_cell(&serde_json::Value::String(column.clone())))
                .collect::<Vec<_>>()
                .join(","),
        );
        content.push('\n');
        for row in rows {
            let line = columns
                .iter()
                .map(|column| {
                    row.as_object()
                        .and_then(|object| object.get(column))
                        .map(Self::csv_cell)
                        .unwrap_or_default()
                })
                .collect::<Vec<_>>()
                .join(",");
            content.push_str(&line);
            content.push('\n');
        }
        content
    }

    fn csv_cell(value: &serde_json::Value) -> String {
        let raw = match value {
            serde_json::Value::Null => String::new(),
            serde_json::Value::String(value) => value.clone(),
            _ => value.to_string(),
        };
        if raw.contains(',') || raw.contains('"') || raw.contains('\n') || raw.contains('\r') {
            format!("\"{}\"", raw.replace('"', "\"\""))
        } else {
            raw
        }
    }

    fn json_to_sql_literal(value: &serde_json::Value) -> String {
        match value {
            serde_json::Value::Null => "NULL".into(),
            serde_json::Value::Bool(value) => {
                if *value {
                    "1".into()
                } else {
                    "0".into()
                }
            }
            serde_json::Value::Number(value) => value.to_string(),
            serde_json::Value::String(value) => Self::sql_literal(value),
            _ => Self::sql_literal(&value.to_string()),
        }
    }

    fn sql_literal(value: &str) -> String {
        format!("'{}'", value.replace('\'', "''"))
    }

    fn export_table_name(value: &str, db_type: &str) -> String {
        let quote = if db_type == "mysql" { "`" } else { "\"" };
        let escaped = value.replace(quote, &format!("{}{}", quote, quote));
        format!("{}{}{}", quote, escaped, quote)
    }

    fn qualified_table_name(schema: Option<&str>, table: &str, db_type: &str) -> String {
        match schema.map(str::trim).filter(|value| !value.is_empty()) {
            Some(schema) => format!(
                "{}.{}",
                Self::export_table_name(schema, db_type),
                Self::export_table_name(table, db_type)
            ),
            None => Self::export_table_name(table, db_type),
        }
    }

    fn json_to_db_string(
        value: &serde_json::Value,
        data_type: &str,
        db_type: &str,
    ) -> Option<String> {
        match value {
            serde_json::Value::Null => None,
            serde_json::Value::Bool(value) => {
                if db_type == "postgresql" && Self::is_boolean_type(data_type) {
                    Some(value.to_string())
                } else if *value {
                    Some("1".into())
                } else {
                    Some("0".into())
                }
            }
            serde_json::Value::Number(value) => Some(value.to_string()),
            serde_json::Value::String(value) => Some(value.clone()),
            _ => Some(value.to_string()),
        }
    }

    fn is_boolean_type(data_type: &str) -> bool {
        matches!(
            data_type.to_ascii_lowercase().as_str(),
            "bool" | "boolean" | "tinyint(1)"
        )
    }

    fn postgres_cast_type(column: &DatabaseColumnSchema) -> String {
        let candidate = if column.column_type.trim().is_empty() {
            column.data_type.trim()
        } else {
            column.column_type.trim()
        };
        if candidate.is_empty()
            || candidate.eq_ignore_ascii_case("USER-DEFINED")
            || !candidate.chars().all(|ch| {
                ch.is_ascii_alphanumeric() || matches!(ch, '_' | ' ' | '(' | ')' | ',' | '[' | ']')
            })
        {
            return "text".into();
        }
        candidate.to_ascii_lowercase()
    }

    fn parse_simple_select_table(sql: &str) -> Option<EditableSelectTarget> {
        let normalized = sql.trim().trim_end_matches(';').trim();
        let lower = normalized.to_ascii_lowercase();
        if !lower.starts_with("select ") {
            return None;
        }
        for forbidden in [
            " join ",
            " group by ",
            " having ",
            " union ",
            " intersect ",
            " except ",
            " distinct ",
            " from (",
            " with ",
        ] {
            if lower.contains(forbidden) {
                return None;
            }
        }
        let from_index = find_keyword_outside_quotes(normalized, "from")?;
        let after_from = normalized[from_index + 4..].trim_start();
        let (table_token, _) = read_identifier_path(after_from)?;
        let parts = split_identifier_path(&table_token)?;
        let table = parts.last()?.clone();
        let schema = if parts.len() >= 2 {
            Some(parts[parts.len() - 2].clone())
        } else {
            None
        };
        Some(EditableSelectTarget { schema, table })
    }

    fn safe_file_part(value: &str) -> String {
        let cleaned = value
            .chars()
            .map(|ch| {
                if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                    ch
                } else {
                    '_'
                }
            })
            .collect::<String>();
        if cleaned.trim_matches('_').is_empty() {
            "export".into()
        } else {
            cleaned
        }
    }

    fn validate_connection(input: &UpsertDatabaseConnectionInput) -> Result<(), AppError> {
        if input.key.trim().is_empty() {
            return Err(AppError::InvalidInput("连接 Key 不能为空".into()));
        }
        if input.name.trim().is_empty() {
            return Err(AppError::InvalidInput("连接名称不能为空".into()));
        }
        if !["mysql", "postgresql", "redis"].contains(&input.db_type.as_str()) {
            return Err(AppError::InvalidInput("数据库类型无效".into()));
        }
        if !["direct", "ssh_tunnel"].contains(&input.connection_mode.as_str()) {
            return Err(AppError::InvalidInput("连接方式无效".into()));
        }
        if input.connection_mode == "ssh_tunnel" && input.ssh_server_alias.trim().is_empty() {
            return Err(AppError::InvalidInput(
                "SSH 隧道连接必须选择跳板服务器".into(),
            ));
        }
        if input.host.trim().is_empty() {
            return Err(AppError::InvalidInput("主机地址不能为空".into()));
        }
        if !(1..=65535).contains(&input.port) {
            return Err(AppError::InvalidInput("端口必须在 1-65535 之间".into()));
        }
        if !["direct_password", "credential_ref"].contains(&input.auth_type.as_str()) {
            return Err(AppError::InvalidInput("认证方式无效".into()));
        }
        if input.auth_type == "credential_ref" && input.credential_ref.trim().is_empty() {
            return Err(AppError::InvalidInput("请选择凭据引用".into()));
        }
        if !["approval_all", "confirm_execute"].contains(&input.security_mode.as_str()) {
            return Err(AppError::InvalidInput("数据库安全级别无效".into()));
        }
        if !["readonly", "L1", "L2", "L3", "blocked"].contains(&input.ai_policy.as_str()) {
            return Err(AppError::InvalidInput("AI 权限级别无效".into()));
        }
        if !(1..=500).contains(&input.page_size) {
            return Err(AppError::InvalidInput("单页行数必须在 1-500 之间".into()));
        }
        Ok(())
    }

    fn normalize_single_sql(sql: &str) -> Result<String, AppError> {
        let normalized = sql.trim();
        if normalized.is_empty() {
            return Err(AppError::InvalidInput("SQL 不能为空".into()));
        }
        let lower = normalized.to_lowercase();
        if lower.matches(';').count() > usize::from(normalized.ends_with(';')) {
            return Err(AppError::InvalidInput("不允许一次执行多条 SQL".into()));
        }
        Ok(normalized.trim_end_matches(';').trim().to_string())
    }

    fn split_sql_statements(sql: &str) -> Result<Vec<String>, AppError> {
        let mut statements = Vec::new();
        let mut current = String::new();
        let mut chars = sql.chars().peekable();
        let mut in_single_quote = false;
        let mut in_double_quote = false;
        let mut in_backtick = false;
        let mut in_line_comment = false;
        let mut in_block_comment = false;

        while let Some(ch) = chars.next() {
            let next = chars.peek().copied();

            if in_line_comment {
                current.push(ch);
                if ch == '\n' {
                    in_line_comment = false;
                }
                continue;
            }

            if in_block_comment {
                current.push(ch);
                if ch == '*' && next == Some('/') {
                    current.push('/');
                    chars.next();
                    in_block_comment = false;
                }
                continue;
            }

            if (in_single_quote || in_double_quote) && ch == '\\' {
                current.push(ch);
                if let Some(escaped) = chars.next() {
                    current.push(escaped);
                }
                continue;
            }

            if !in_single_quote && !in_double_quote && !in_backtick {
                if ch == '-' && next == Some('-') {
                    current.push(ch);
                    current.push('-');
                    chars.next();
                    in_line_comment = true;
                    continue;
                }
                if ch == '#' {
                    current.push(ch);
                    in_line_comment = true;
                    continue;
                }
                if ch == '/' && next == Some('*') {
                    current.push(ch);
                    current.push('*');
                    chars.next();
                    in_block_comment = true;
                    continue;
                }
            }

            match ch {
                '\'' if !in_double_quote && !in_backtick => {
                    current.push(ch);
                    if in_single_quote && next == Some('\'') {
                        current.push('\'');
                        chars.next();
                    } else {
                        in_single_quote = !in_single_quote;
                    }
                }
                '"' if !in_single_quote && !in_backtick => {
                    current.push(ch);
                    in_double_quote = !in_double_quote;
                }
                '`' if !in_single_quote && !in_double_quote => {
                    current.push(ch);
                    in_backtick = !in_backtick;
                }
                ';' if !in_single_quote && !in_double_quote && !in_backtick => {
                    let statement = current.trim();
                    if !statement.is_empty() {
                        statements.push(statement.to_string());
                    }
                    current.clear();
                }
                _ => current.push(ch),
            }
        }

        if in_single_quote || in_double_quote || in_backtick || in_block_comment {
            return Err(AppError::InvalidInput(
                "SQL 语句存在未闭合的引号或注释".into(),
            ));
        }

        let statement = current.trim();
        if !statement.is_empty() {
            statements.push(statement.to_string());
        }

        if statements.is_empty() {
            return Err(AppError::InvalidInput("SQL 不能为空".into()));
        }

        Ok(statements)
    }

    fn statement_type(sql: &str) -> String {
        sql.trim()
            .to_lowercase()
            .split(|ch: char| ch.is_whitespace() || ch == '(')
            .find(|item| !item.is_empty())
            .unwrap_or("sql")
            .to_string()
    }

    fn is_result_sql(sql: &str) -> bool {
        matches!(
            Self::statement_type(sql).as_str(),
            "select" | "show" | "describe" | "desc" | "explain" | "with"
        )
    }

    fn supports_subquery_pagination(sql: &str) -> bool {
        matches!(Self::statement_type(sql).as_str(), "select" | "with")
    }

    async fn query_mysql(
        url: &str,
        sql: &str,
    ) -> Result<(Vec<String>, Vec<String>, Vec<serde_json::Value>), AppError> {
        use sqlx::{
            mysql::MySqlPoolOptions, Column, Executor, Row, SqlSafeStr, Statement, TypeInfo,
        };

        let pool = MySqlPoolOptions::new()
            .max_connections(1)
            .acquire_timeout(Duration::from_secs(5))
            .connect(url)
            .await
            .map_err(|error| AppError::Custom(format!("连接 MySQL 失败: {}", error)))?;
        // SQL 由数据库工作台的单语句校验或内部固定查询提供；SQLx 0.9 要求显式确认。
        let mut connection = pool
            .acquire()
            .await
            .map_err(|error| AppError::Custom(format!("获取 MySQL 查询连接失败: {}", error)))?;
        let statement = (&mut *connection)
            .prepare(sqlx::AssertSqlSafe(sql).into_sql_str())
            .await
            .map_err(|error| AppError::Custom(format!("读取 MySQL 查询列信息失败: {}", error)))?;
        let columns = statement
            .columns()
            .iter()
            .map(|column| column.name().to_string())
            .collect::<Vec<_>>();
        let column_types = statement
            .columns()
            .iter()
            .map(|column| column.type_info().name().to_string())
            .collect::<Vec<_>>();
        let rows = statement
            .query()
            .fetch_all(&mut *connection)
            .await
            .map_err(|error| AppError::Custom(format!("执行 MySQL 查询失败: {}", error)))?;
        let data = rows
            .iter()
            .map(|row| {
                let mut item = serde_json::Map::new();
                for (index, column) in row.columns().iter().enumerate() {
                    item.insert(column.name().to_string(), mysql_cell_to_json(row, index));
                }
                serde_json::Value::Object(item)
            })
            .collect::<Vec<_>>();
        Ok((columns, column_types, data))
    }

    async fn execute_mysql(url: &str, sql: &str) -> Result<u64, AppError> {
        use sqlx::mysql::MySqlPoolOptions;

        let pool = MySqlPoolOptions::new()
            .max_connections(1)
            .acquire_timeout(Duration::from_secs(5))
            .connect(url)
            .await
            .map_err(|error| AppError::Custom(format!("连接 MySQL 失败: {}", error)))?;
        // 写入语句先经过单语句校验；调用者明确选择的 SQL 是工作台的受控能力。
        let result = sqlx::query(sqlx::AssertSqlSafe(sql))
            .execute(&pool)
            .await
            .map_err(|error| AppError::Custom(format!("执行 MySQL 语句失败: {}", error)))?;
        Ok(result.rows_affected())
    }

    async fn list_mysql_databases(url: &str) -> Result<Vec<String>, AppError> {
        let (_, _, rows) = Self::query_mysql(url, "SHOW DATABASES").await?;
        let mut databases = Vec::with_capacity(rows.len());
        for row in rows {
            if let Some(value) = row.as_object().and_then(|item| item.values().next()) {
                if let Some(name) = value.as_str() {
                    databases.push(name.to_string());
                }
            }
        }
        Ok(databases)
    }

    async fn list_mysql_schema(
        url: &str,
        database_name: &str,
    ) -> Result<Vec<DatabaseTableSchema>, AppError> {
        use sqlx::{mysql::MySqlPoolOptions, Row};
        use std::collections::BTreeMap;

        let pool = MySqlPoolOptions::new()
            .max_connections(1)
            .acquire_timeout(Duration::from_secs(5))
            .connect(url)
            .await
            .map_err(|error| AppError::Custom(format!("连接 MySQL 失败: {}", error)))?;
        let table_rows = sqlx::query(
            "SELECT CAST(TABLE_NAME AS CHAR) AS table_name, CAST(TABLE_TYPE AS CHAR) AS table_type
             FROM information_schema.TABLES
             WHERE TABLE_SCHEMA = ?
             ORDER BY TABLE_NAME",
        )
        .bind(database_name)
        .fetch_all(&pool)
        .await
        .map_err(|error| AppError::Custom(format!("读取 MySQL 对象失败: {}", error)))?;
        let column_rows = sqlx::query(
            "SELECT CAST(TABLE_NAME AS CHAR) AS table_name, CAST(COLUMN_NAME AS CHAR) AS column_name,
                    CAST(DATA_TYPE AS CHAR) AS data_type, CAST(COLUMN_TYPE AS CHAR) AS column_type,
                    CAST(IS_NULLABLE AS CHAR) AS is_nullable, CAST(COLUMN_DEFAULT AS CHAR) AS column_default,
                    CAST(EXTRA AS CHAR) AS extra, CAST(ORDINAL_POSITION AS SIGNED) AS ordinal_position
             FROM information_schema.COLUMNS
             WHERE TABLE_SCHEMA = ?
             ORDER BY TABLE_NAME, ORDINAL_POSITION",
        )
        .bind(database_name)
        .fetch_all(&pool)
        .await
        .map_err(|error| AppError::Custom(format!("读取 MySQL 字段失败: {}", error)))?;
        let index_rows = sqlx::query(
            "SELECT CAST(TABLE_NAME AS CHAR) AS table_name, CAST(INDEX_NAME AS CHAR) AS index_name,
                    CAST(COLUMN_NAME AS CHAR) AS column_name, CAST(NON_UNIQUE AS SIGNED) AS non_unique,
                    CAST(SEQ_IN_INDEX AS SIGNED) AS seq_in_index
             FROM information_schema.STATISTICS
             WHERE TABLE_SCHEMA = ?
             ORDER BY TABLE_NAME, INDEX_NAME, SEQ_IN_INDEX",
        )
        .bind(database_name)
        .fetch_all(&pool)
        .await
        .map_err(|error| AppError::Custom(format!("读取 MySQL 索引失败: {}", error)))?;

        let mut table_map: BTreeMap<String, Vec<DatabaseColumnSchema>> = BTreeMap::new();
        let mut type_map: BTreeMap<String, String> = BTreeMap::new();
        let mut index_map: BTreeMap<String, BTreeMap<String, DatabaseIndexSchema>> =
            BTreeMap::new();
        for row in table_rows {
            let table_name: String = row.try_get(0).unwrap_or_default();
            let table_type: String = row.try_get(1).unwrap_or_default();
            if !table_name.is_empty() {
                type_map.insert(table_name.clone(), table_type);
                table_map.entry(table_name).or_default();
            }
        }
        for row in column_rows {
            let table_name: String = row.try_get(0).unwrap_or_default();
            let column_name: String = row.try_get(1).unwrap_or_default();
            let data_type: String = row.try_get(2).unwrap_or_default();
            let column_type: String = row.try_get(3).unwrap_or_else(|_| data_type.clone());
            let is_nullable: String = row.try_get(4).unwrap_or_default();
            let default_value: Option<String> = row.try_get(5).ok();
            let extra: String = row.try_get(6).unwrap_or_default();
            let ordinal_position: i64 = row.try_get(7).unwrap_or(0);
            if !table_name.is_empty() && !column_name.is_empty() {
                table_map
                    .entry(table_name)
                    .or_default()
                    .push(DatabaseColumnSchema {
                        name: column_name,
                        data_type,
                        column_type,
                        nullable: is_nullable.eq_ignore_ascii_case("YES"),
                        default_value,
                        extra,
                        ordinal_position,
                    });
            }
        }
        for row in index_rows {
            let table_name: String = row.try_get(0).unwrap_or_default();
            let index_name: String = row.try_get(1).unwrap_or_default();
            let column_name: String = row.try_get(2).unwrap_or_default();
            let non_unique: i64 = row.try_get(3).unwrap_or(1);
            if table_name.is_empty() || index_name.is_empty() || column_name.is_empty() {
                continue;
            }
            let indexes = index_map.entry(table_name).or_default();
            let index = indexes
                .entry(index_name.clone())
                .or_insert_with(|| DatabaseIndexSchema {
                    name: index_name,
                    columns: vec![],
                    unique: non_unique == 0,
                });
            index.columns.push(column_name);
        }

        Ok(table_map
            .into_iter()
            .map(|(name, column_details)| {
                let columns = column_details
                    .iter()
                    .map(|column| column.name.clone())
                    .collect::<Vec<_>>();
                DatabaseTableSchema {
                    object_type: type_map
                        .get(&name)
                        .cloned()
                        .unwrap_or_else(|| "BASE TABLE".into()),
                    indexes: index_map
                        .remove(&name)
                        .map(|items| items.into_values().collect())
                        .unwrap_or_default(),
                    schema_name: None,
                    name,
                    columns,
                    column_details,
                }
            })
            .collect())
    }

    async fn query_postgres(
        url: &str,
        sql: &str,
    ) -> Result<(Vec<String>, Vec<String>, Vec<serde_json::Value>), AppError> {
        use sqlx::{
            postgres::PgPoolOptions, Column, Executor, Row, SqlSafeStr, Statement, TypeInfo,
        };

        let pool = PgPoolOptions::new()
            .max_connections(1)
            .acquire_timeout(Duration::from_secs(5))
            .connect(url)
            .await
            .map_err(|error| AppError::Custom(format!("连接 PostgreSQL 失败: {}", error)))?;
        // SQL 由数据库工作台的单语句校验或内部固定查询提供；SQLx 0.9 要求显式确认。
        let mut connection = pool.acquire().await.map_err(|error| {
            AppError::Custom(format!("获取 PostgreSQL 查询连接失败: {}", error))
        })?;
        let statement = (&mut *connection)
            .prepare(sqlx::AssertSqlSafe(sql).into_sql_str())
            .await
            .map_err(|error| {
                AppError::Custom(format!("读取 PostgreSQL 查询列信息失败: {}", error))
            })?;
        let columns = statement
            .columns()
            .iter()
            .map(|column| column.name().to_string())
            .collect::<Vec<_>>();
        let column_types = statement
            .columns()
            .iter()
            .map(|column| column.type_info().name().to_string())
            .collect::<Vec<_>>();
        let rows = statement
            .query()
            .fetch_all(&mut *connection)
            .await
            .map_err(|error| AppError::Custom(format!("执行 PostgreSQL 查询失败: {}", error)))?;
        let data = rows
            .iter()
            .map(|row| {
                let mut item = serde_json::Map::new();
                for (index, column) in row.columns().iter().enumerate() {
                    item.insert(column.name().to_string(), postgres_cell_to_json(row, index));
                }
                serde_json::Value::Object(item)
            })
            .collect::<Vec<_>>();
        Ok((columns, column_types, data))
    }

    async fn execute_postgres(url: &str, sql: &str) -> Result<u64, AppError> {
        use sqlx::postgres::PgPoolOptions;

        let pool = PgPoolOptions::new()
            .max_connections(1)
            .acquire_timeout(Duration::from_secs(5))
            .connect(url)
            .await
            .map_err(|error| AppError::Custom(format!("连接 PostgreSQL 失败: {}", error)))?;
        // 写入语句先经过单语句校验；调用者明确选择的 SQL 是工作台的受控能力。
        let result = sqlx::query(sqlx::AssertSqlSafe(sql))
            .execute(&pool)
            .await
            .map_err(|error| AppError::Custom(format!("执行 PostgreSQL 语句失败: {}", error)))?;
        Ok(result.rows_affected())
    }

    async fn list_postgres_databases(url: &str) -> Result<Vec<String>, AppError> {
        let (_, _, rows) = Self::query_postgres(
            url,
            "SELECT datname FROM pg_database WHERE datallowconn = true ORDER BY datname",
        )
        .await?;
        Ok(rows
            .into_iter()
            .filter_map(|row| {
                row.as_object()
                    .and_then(|item| item.get("datname"))
                    .and_then(|value| value.as_str())
                    .map(ToString::to_string)
            })
            .collect())
    }

    async fn list_postgres_schema(url: &str) -> Result<Vec<DatabaseTableSchema>, AppError> {
        use sqlx::{postgres::PgPoolOptions, Row};
        use std::collections::BTreeMap;

        let pool = PgPoolOptions::new()
            .max_connections(1)
            .acquire_timeout(Duration::from_secs(5))
            .connect(url)
            .await
            .map_err(|error| AppError::Custom(format!("连接 PostgreSQL 失败: {}", error)))?;
        let table_rows = sqlx::query(
            "SELECT table_schema, table_name, table_type
             FROM information_schema.tables
             WHERE table_schema NOT IN ('pg_catalog', 'information_schema')
             ORDER BY table_schema, table_name",
        )
        .fetch_all(&pool)
        .await
        .map_err(|error| AppError::Custom(format!("读取 PostgreSQL 对象失败: {}", error)))?;
        let column_rows = sqlx::query(
            "SELECT table_schema, table_name, column_name, data_type, udt_name,
                    is_nullable, column_default, ordinal_position
             FROM information_schema.columns
             WHERE table_schema NOT IN ('pg_catalog', 'information_schema')
             ORDER BY table_schema, table_name, ordinal_position",
        )
        .fetch_all(&pool)
        .await
        .map_err(|error| AppError::Custom(format!("读取 PostgreSQL 字段失败: {}", error)))?;
        let index_rows = sqlx::query(
            "SELECT schemaname, tablename, indexname, indexdef
             FROM pg_indexes
             WHERE schemaname NOT IN ('pg_catalog', 'information_schema')
             ORDER BY schemaname, tablename, indexname",
        )
        .fetch_all(&pool)
        .await
        .map_err(|error| AppError::Custom(format!("读取 PostgreSQL 索引失败: {}", error)))?;

        let mut table_map: BTreeMap<(String, String), Vec<DatabaseColumnSchema>> = BTreeMap::new();
        let mut type_map: BTreeMap<(String, String), String> = BTreeMap::new();
        let mut index_map: BTreeMap<(String, String), Vec<DatabaseIndexSchema>> = BTreeMap::new();
        for row in table_rows {
            let schema_name: String = row.try_get("table_schema").unwrap_or_default();
            let table_name: String = row.try_get("table_name").unwrap_or_default();
            let table_type: String = row.try_get("table_type").unwrap_or_default();
            if !schema_name.is_empty() && !table_name.is_empty() {
                let key = (schema_name, table_name);
                type_map.insert(key.clone(), table_type);
                table_map.entry(key).or_default();
            }
        }
        for row in column_rows {
            let schema_name: String = row.try_get("table_schema").unwrap_or_default();
            let table_name: String = row.try_get("table_name").unwrap_or_default();
            let column_name: String = row.try_get("column_name").unwrap_or_default();
            let data_type: String = row.try_get("data_type").unwrap_or_default();
            let udt_name: String = row
                .try_get("udt_name")
                .unwrap_or_else(|_| data_type.clone());
            let is_nullable: String = row.try_get("is_nullable").unwrap_or_default();
            let default_value: Option<String> = row.try_get("column_default").ok();
            let ordinal_position: i64 = row.try_get("ordinal_position").unwrap_or(0);
            if !schema_name.is_empty() && !table_name.is_empty() && !column_name.is_empty() {
                table_map
                    .entry((schema_name, table_name))
                    .or_default()
                    .push(DatabaseColumnSchema {
                        name: column_name,
                        data_type: data_type.clone(),
                        column_type: if data_type.is_empty() {
                            udt_name
                        } else {
                            data_type
                        },
                        nullable: is_nullable.eq_ignore_ascii_case("YES"),
                        default_value,
                        extra: String::new(),
                        ordinal_position,
                    });
            }
        }
        for row in index_rows {
            let schema_name: String = row.try_get("schemaname").unwrap_or_default();
            let table_name: String = row.try_get("tablename").unwrap_or_default();
            let index_name: String = row.try_get("indexname").unwrap_or_default();
            let index_def: String = row.try_get("indexdef").unwrap_or_default();
            if schema_name.is_empty() || table_name.is_empty() || index_name.is_empty() {
                continue;
            }
            index_map
                .entry((schema_name, table_name))
                .or_default()
                .push(DatabaseIndexSchema {
                    name: index_name,
                    columns: parse_postgres_index_columns(&index_def),
                    unique: index_def
                        .to_ascii_uppercase()
                        .starts_with("CREATE UNIQUE INDEX"),
                });
        }

        Ok(table_map
            .into_iter()
            .map(|((schema_name, name), column_details)| {
                let columns = column_details
                    .iter()
                    .map(|column| column.name.clone())
                    .collect::<Vec<_>>();
                DatabaseTableSchema {
                    object_type: type_map
                        .get(&(schema_name.clone(), name.clone()))
                        .cloned()
                        .unwrap_or_else(|| "BASE TABLE".into()),
                    indexes: index_map
                        .remove(&(schema_name.clone(), name.clone()))
                        .unwrap_or_default(),
                    schema_name: Some(schema_name),
                    name,
                    columns,
                    column_details,
                }
            })
            .collect())
    }

    pub(crate) fn mysql_url(connection: &DatabaseConnection, password: Option<&str>) -> String {
        let username = percent_encode(&connection.username);
        let password = password.map(percent_encode).unwrap_or_default();
        let auth = if username.is_empty() {
            String::new()
        } else {
            format!("{}:{}@", username, password)
        };
        format!(
            // 禁用 mysql-rsa 后，MySQL 8 的完整认证必须走 TLS；强制 TLS，禁止连接失败时回退明文。
            "mysql://{}{}:{}/{}?ssl-mode=REQUIRED",
            auth,
            connection.host,
            connection.port,
            percent_encode(&connection.database_name)
        )
    }

    pub(crate) fn postgres_url(connection: &DatabaseConnection, password: Option<&str>) -> String {
        let username = percent_encode(&connection.username);
        let password = password.map(percent_encode).unwrap_or_default();
        let auth = if username.is_empty() {
            String::new()
        } else {
            format!("{}:{}@", username, password)
        };
        format!(
            "postgres://{}{}:{}/{}",
            auth,
            connection.host,
            connection.port,
            percent_encode(&connection.database_name)
        )
    }

    pub(crate) fn redis_url(connection: &DatabaseConnection, password: Option<&str>) -> String {
        let db = connection.database_name.trim().parse::<u8>().unwrap_or(0);
        let username = connection.username.trim();
        match (
            username.is_empty(),
            password.filter(|value| !value.is_empty()),
        ) {
            (false, Some(password)) => format!(
                "redis://{}:{}@{}:{}/{}",
                percent_encode(username),
                percent_encode(password),
                connection.host,
                connection.port,
                db
            ),
            (true, Some(password)) => format!(
                "redis://:{}@{}:{}/{}",
                percent_encode(password),
                connection.host,
                connection.port,
                db
            ),
            _ => format!("redis://{}:{}/{}", connection.host, connection.port, db),
        }
    }

    fn decrypt_optional_password(
        db: &Database,
        nonce: Option<&str>,
        ciphertext: Option<&str>,
    ) -> Result<Option<String>, AppError> {
        match (nonce, ciphertext) {
            (Some(nonce), Some(ciphertext)) => {
                let seed = Self::get_or_create_secret_seed(db)?;
                let key = Sha256::digest(seed.as_bytes());
                let cipher = Aes256Gcm::new_from_slice(&key).map_err(|error| {
                    AppError::Custom(format!("初始化数据库密码解密器失败: {}", error))
                })?;
                let nonce_bytes = general_purpose::STANDARD.decode(nonce).map_err(|error| {
                    AppError::Custom(format!("数据库密码 nonce 无效: {}", error))
                })?;
                let ciphertext_bytes = general_purpose::STANDARD
                    .decode(ciphertext)
                    .map_err(|error| AppError::Custom(format!("数据库密码密文无效: {}", error)))?;
                let plaintext = cipher
                    .decrypt(Nonce::from_slice(&nonce_bytes), ciphertext_bytes.as_ref())
                    .map_err(|error| AppError::Custom(format!("解密数据库密码失败: {}", error)))?;
                let password = String::from_utf8(plaintext).map_err(|error| {
                    AppError::Custom(format!("数据库密码不是有效 UTF-8: {}", error))
                })?;
                Ok(Some(password))
            }
            _ => Ok(None),
        }
    }

    pub(crate) fn resolve_connection_password(
        db: &Database,
        connection: &DatabaseConnection,
        nonce: Option<&str>,
        ciphertext: Option<&str>,
    ) -> Result<Option<String>, AppError> {
        match connection.auth_type.as_str() {
            "direct_password" => {
                let password = Self::decrypt_optional_password(db, nonce, ciphertext)?;
                if password.is_none() {
                    return Err(AppError::InvalidInput(format!(
                        "数据库连接 '{}' 使用直接密码认证，但未保存密码。请编辑连接并填写密码后再执行。",
                        connection.name
                    )));
                }
                Ok(password)
            }
            "credential_ref" => {
                let secret = CredentialVaultService::get_secret(db, &connection.credential_ref)?;
                Ok(Some(secret))
            }
            _ => Err(AppError::InvalidInput("认证方式无效".into())),
        }
    }

    async fn test_tcp_endpoint(
        connection_key: &str,
        host: &str,
        port: i64,
    ) -> Result<DatabaseConnectionTestResult, AppError> {
        let endpoint = format!("{}:{}", host.trim(), port);
        let started = Instant::now();
        let result = timeout(Duration::from_secs(3), TcpStream::connect(&endpoint)).await;
        let latency_ms = started.elapsed().as_millis() as i64;

        match result {
            Ok(Ok(_stream)) => Ok(DatabaseConnectionTestResult {
                ok: true,
                connection_key: connection_key.to_string(),
                endpoint,
                latency_ms,
                message: "TCP 连接成功".into(),
            }),
            Ok(Err(error)) => Ok(DatabaseConnectionTestResult {
                ok: false,
                connection_key: connection_key.to_string(),
                endpoint,
                latency_ms,
                message: format!("TCP 连接失败: {}", error),
            }),
            Err(_) => Ok(DatabaseConnectionTestResult {
                ok: false,
                connection_key: connection_key.to_string(),
                endpoint,
                latency_ms,
                message: "TCP 连接超时".into(),
            }),
        }
    }

    fn encrypt_password(db: &Database, password: &str) -> Result<(String, String), AppError> {
        let seed = Self::get_or_create_secret_seed(db)?;
        let key = Sha256::digest(seed.as_bytes());
        let cipher = Aes256Gcm::new_from_slice(&key)
            .map_err(|error| AppError::Custom(format!("初始化数据库密码加密器失败: {}", error)))?;
        let mut nonce_bytes = [0_u8; 12];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ciphertext = cipher
            .encrypt(nonce, password.as_bytes())
            .map_err(|error| AppError::Custom(format!("加密数据库密码失败: {}", error)))?;
        Ok((
            general_purpose::STANDARD.encode(nonce_bytes),
            general_purpose::STANDARD.encode(ciphertext),
        ))
    }

    fn get_or_create_secret_seed(db: &Database) -> Result<String, AppError> {
        if let Some(seed) = db.get_config(DATABASE_PASSWORD_SECRET_SEED_KEY)? {
            return Ok(seed);
        }
        let mut bytes = [0_u8; 32];
        rand::thread_rng().fill_bytes(&mut bytes);
        let seed = general_purpose::STANDARD.encode(bytes);
        db.set_config(DATABASE_PASSWORD_SECRET_SEED_KEY, &seed)?;
        Ok(seed)
    }
}

fn mysql_cell_to_json(row: &sqlx::mysql::MySqlRow, index: usize) -> serde_json::Value {
    use sqlx::{Row, ValueRef};
    if row
        .try_get_raw(index)
        .map(|value| value.is_null())
        .unwrap_or(true)
    {
        return serde_json::Value::Null;
    }
    if let Ok(value) = row.try_get::<sqlx::types::Json<serde_json::Value>, _>(index) {
        return value.0;
    }
    if let Ok(value) = row.try_get::<String, _>(index) {
        return serde_json::Value::String(value);
    }
    if let Ok(value) = row.try_get::<Vec<u8>, _>(index) {
        return String::from_utf8(value)
            .map(serde_json::Value::String)
            .unwrap_or_else(|_| serde_json::Value::String("<binary>".into()));
    }
    if let Ok(value) = row.try_get::<i64, _>(index) {
        return safe_json_i64(value);
    }
    if let Ok(value) = row.try_get::<u64, _>(index) {
        return safe_json_u64(value);
    }
    if let Ok(value) = row.try_get::<f64, _>(index) {
        return serde_json::json!(value);
    }
    if let Ok(value) = row.try_get::<bool, _>(index) {
        return serde_json::json!(value);
    }
    if let Ok(value) = row.try_get::<chrono::NaiveDateTime, _>(index) {
        return serde_json::Value::String(value.to_string());
    }
    if let Ok(value) = row.try_get::<chrono::NaiveDate, _>(index) {
        return serde_json::Value::String(value.to_string());
    }
    if let Ok(value) = row.try_get::<chrono::NaiveTime, _>(index) {
        return serde_json::Value::String(value.to_string());
    }
    serde_json::Value::String("<unprintable>".into())
}

fn postgres_cell_to_json(row: &sqlx::postgres::PgRow, index: usize) -> serde_json::Value {
    use sqlx::{Row, ValueRef};
    if row
        .try_get_raw(index)
        .map(|value| value.is_null())
        .unwrap_or(true)
    {
        return serde_json::Value::Null;
    }
    if let Ok(value) = row.try_get::<serde_json::Value, _>(index) {
        return value;
    }
    if let Ok(value) = row.try_get::<String, _>(index) {
        return serde_json::Value::String(value);
    }
    if let Ok(value) = row.try_get::<Vec<u8>, _>(index) {
        return String::from_utf8(value)
            .map(serde_json::Value::String)
            .unwrap_or_else(|_| serde_json::Value::String("<binary>".into()));
    }
    if let Ok(value) = row.try_get::<i64, _>(index) {
        return safe_json_i64(value);
    }
    if let Ok(value) = row.try_get::<i32, _>(index) {
        return serde_json::json!(value);
    }
    if let Ok(value) = row.try_get::<f64, _>(index) {
        return serde_json::json!(value);
    }
    if let Ok(value) = row.try_get::<bool, _>(index) {
        return serde_json::json!(value);
    }
    if let Ok(value) = row.try_get::<chrono::NaiveDateTime, _>(index) {
        return serde_json::Value::String(value.to_string());
    }
    if let Ok(value) = row.try_get::<chrono::NaiveDate, _>(index) {
        return serde_json::Value::String(value.to_string());
    }
    if let Ok(value) = row.try_get::<chrono::NaiveTime, _>(index) {
        return serde_json::Value::String(value.to_string());
    }
    serde_json::Value::String("<unprintable>".into())
}

fn safe_json_i64(value: i64) -> serde_json::Value {
    const JS_MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_991;
    const JS_MIN_SAFE_INTEGER: i64 = -9_007_199_254_740_991;
    if (JS_MIN_SAFE_INTEGER..=JS_MAX_SAFE_INTEGER).contains(&value) {
        serde_json::json!(value)
    } else {
        serde_json::Value::String(value.to_string())
    }
}

fn safe_json_u64(value: u64) -> serde_json::Value {
    const JS_MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;
    if value <= JS_MAX_SAFE_INTEGER {
        serde_json::json!(value)
    } else {
        serde_json::Value::String(value.to_string())
    }
}

struct EditableSelectTarget {
    schema: Option<String>,
    table: String,
}

fn find_keyword_outside_quotes(sql: &str, keyword: &str) -> Option<usize> {
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut in_backtick = false;
    let bytes = sql.as_bytes();
    let keyword_bytes = keyword.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        let ch = bytes[index] as char;
        match ch {
            '\'' if !in_double_quote && !in_backtick => in_single_quote = !in_single_quote,
            '"' if !in_single_quote && !in_backtick => in_double_quote = !in_double_quote,
            '`' if !in_single_quote && !in_double_quote => in_backtick = !in_backtick,
            _ => {}
        }
        if !in_single_quote
            && !in_double_quote
            && !in_backtick
            && index + keyword_bytes.len() <= bytes.len()
            && bytes[index..index + keyword_bytes.len()].eq_ignore_ascii_case(keyword_bytes)
        {
            let before_ok = index == 0
                || !(bytes[index - 1] as char).is_ascii_alphanumeric() && bytes[index - 1] != b'_';
            let after_index = index + keyword_bytes.len();
            let after_ok = after_index >= bytes.len()
                || !(bytes[after_index] as char).is_ascii_alphanumeric()
                    && bytes[after_index] != b'_';
            if before_ok && after_ok {
                return Some(index);
            }
        }
        index += 1;
    }
    None
}

fn read_identifier_path(input: &str) -> Option<(String, usize)> {
    let mut token = String::new();
    let mut consumed = 0;
    let mut chars = input.char_indices().peekable();
    while let Some((index, ch)) = chars.next() {
        if ch.is_whitespace() {
            if token.is_empty() {
                consumed = index + ch.len_utf8();
                continue;
            }
            break;
        }
        if matches!(ch, '`' | '"') {
            token.push(ch);
            let quote = ch;
            let mut closed = false;
            while let Some((_, quoted)) = chars.next() {
                token.push(quoted);
                if quoted == quote {
                    closed = true;
                    break;
                }
            }
            if !closed {
                return None;
            }
            consumed = index + ch.len_utf8();
            continue;
        }
        if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '.' | '$') {
            token.push(ch);
            consumed = index + ch.len_utf8();
            continue;
        }
        break;
    }
    if token.trim().is_empty() {
        None
    } else {
        Some((token, consumed))
    }
}

fn split_identifier_path(token: &str) -> Option<Vec<String>> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut chars = token.chars().peekable();
    while let Some(ch) = chars.next() {
        if matches!(ch, '`' | '"') {
            let quote = ch;
            let mut quoted = String::new();
            let mut closed = false;
            while let Some(inner) = chars.next() {
                if inner == quote {
                    closed = true;
                    break;
                }
                quoted.push(inner);
            }
            if !closed {
                return None;
            }
            current.push_str(&quoted);
            continue;
        }
        if ch == '.' {
            if current.trim().is_empty() {
                return None;
            }
            parts.push(current.trim().to_string());
            current.clear();
            continue;
        }
        current.push(ch);
    }
    if current.trim().is_empty() {
        return None;
    }
    parts.push(current.trim().to_string());
    Some(parts)
}

fn percent_encode(value: &str) -> String {
    value
        .bytes()
        .flat_map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                vec![byte as char]
            }
            _ => format!("%{:02X}", byte).chars().collect::<Vec<_>>(),
        })
        .collect()
}

fn parse_postgres_index_columns(index_def: &str) -> Vec<String> {
    index_def
        .rsplit_once('(')
        .and_then(|(_, right)| right.rsplit_once(')').map(|(inside, _)| inside))
        .map(|inside| {
            inside
                .split(',')
                .map(|item| {
                    item.trim()
                        .trim_matches('"')
                        .split_whitespace()
                        .next()
                        .unwrap_or_default()
                        .trim_matches('"')
                        .to_string()
                })
                .filter(|item| !item.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::mysql::{MySqlConnectOptions, MySqlSslMode};

    #[test]
    fn mysql_url_requires_tls_without_rsa_auth_fallback() {
        let connection = DatabaseConnection {
            key: "mysql-test".into(),
            name: "MySQL 测试连接".into(),
            group_name: "默认分组".into(),
            db_type: "mysql".into(),
            connection_mode: "direct".into(),
            host: "mysql.example.test".into(),
            port: 3306,
            database_name: "业务库".into(),
            username: "reader".into(),
            auth_type: "direct_password".into(),
            credential_ref: String::new(),
            password_masked: None,
            has_password: true,
            ssh_server_alias: String::new(),
            security_mode: "approval_all".into(),
            ai_policy: "L2".into(),
            page_size: 100,
            status: "unknown".into(),
            enabled: true,
            last_connected_at: None,
            notes: String::new(),
            updated_at: String::new(),
        };

        let url = DatabaseOpsService::mysql_url(&connection, Some("password"));
        let options: MySqlConnectOptions = url.parse().expect("MySQL URL 应可被 SQLx 解析");

        assert!(matches!(options.get_ssl_mode(), MySqlSslMode::Required));
    }
}
