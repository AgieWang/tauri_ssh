use std::time::Instant;

use serde_json::json;

use crate::database::Database;
use crate::error::AppError;
use crate::models::{
    CollectResourceBatchInput, CollectResourceBatchResult, CreateAuditLogInput, DatabaseConnection,
    KillMysqlQueryInput, KillMysqlQueryResult, ListResourceAlertEventsInput,
    ListResourceAlertRulesInput, MysqlSlowQuery, MysqlSlowQueryListInput, ResourceAlertEvent,
    ResourceAlertRule, ResourceMetricSnapshot, ResourceMonitorOverview, ResourceMonitorTarget,
    ResourceSnapshotListInput, UpsertResourceAlertRuleInput, UpsertResourceMonitorTargetInput,
};
use crate::services::audit::AuditService;
use crate::services::database_ops::DatabaseOpsService;
use crate::services::terminal::TerminalService;

pub struct ResourceMonitorService;

impl ResourceMonitorService {
    pub fn list_targets(db: &Database) -> Result<Vec<ResourceMonitorTarget>, AppError> {
        Self::sync_default_targets(db)?;
        let mut targets = db.list_resource_monitor_targets()?;
        for target in &mut targets {
            target.latest_snapshot =
                db.get_latest_resource_metric_snapshot(&target.target_type, &target.target_key)?;
        }
        Ok(targets)
    }

    pub fn upsert_target(
        db: &Database,
        input: UpsertResourceMonitorTargetInput,
    ) -> Result<ResourceMonitorTarget, AppError> {
        Self::validate_target(&input.target_type, &input.target_key)?;
        let fallback_name = input
            .display_name
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(&input.target_key)
            .to_string();
        db.upsert_resource_monitor_target(&input, &fallback_name)
    }

    pub fn delete_target(
        db: &Database,
        target_type: &str,
        target_key: &str,
    ) -> Result<(), AppError> {
        Self::validate_target(target_type, target_key)?;
        if !db.delete_resource_monitor_target(target_type, target_key)? {
            return Err(AppError::NotFound("资源监控目标不存在".into()));
        }
        Ok(())
    }

    pub fn overview(db: &Database) -> Result<ResourceMonitorOverview, AppError> {
        let targets = Self::list_targets(db)?;
        let total_targets = targets.len() as i64;
        let enabled_targets = targets.iter().filter(|item| item.enabled).count() as i64;
        let healthy_targets = targets
            .iter()
            .filter(|item| item.last_status == "healthy")
            .count() as i64;
        let warning_targets = targets
            .iter()
            .filter(|item| item.last_status == "warning")
            .count() as i64;
        let failed_targets = targets
            .iter()
            .filter(|item| item.last_status == "failed")
            .count() as i64;
        let open_alerts = db.count_open_resource_alert_events()?;
        let latest_collected_at = targets
            .iter()
            .filter_map(|item| item.last_collected_at.clone())
            .max();
        Ok(ResourceMonitorOverview {
            total_targets,
            enabled_targets,
            healthy_targets,
            warning_targets,
            failed_targets,
            open_alerts,
            latest_collected_at,
        })
    }

    pub fn list_snapshots(
        db: &Database,
        input: ResourceSnapshotListInput,
    ) -> Result<Vec<ResourceMetricSnapshot>, AppError> {
        db.list_resource_metric_snapshots(&input)
    }

    pub fn list_alert_rules(
        db: &Database,
        input: ListResourceAlertRulesInput,
    ) -> Result<Vec<ResourceAlertRule>, AppError> {
        db.list_resource_alert_rules(&input)
    }

    pub fn upsert_alert_rule(
        db: &Database,
        input: UpsertResourceAlertRuleInput,
    ) -> Result<ResourceAlertRule, AppError> {
        Self::validate_alert_rule(&input)?;
        db.upsert_resource_alert_rule(&input)
    }

    pub fn delete_alert_rule(db: &Database, id: i64) -> Result<(), AppError> {
        if !db.delete_resource_alert_rule(id)? {
            return Err(AppError::NotFound(format!("告警规则 '{}' 不存在", id)));
        }
        Ok(())
    }

    pub fn list_alert_events(
        db: &Database,
        input: ListResourceAlertEventsInput,
    ) -> Result<Vec<ResourceAlertEvent>, AppError> {
        db.list_resource_alert_events(&input)
    }

    pub fn resolve_alert_event(db: &Database, id: i64) -> Result<(), AppError> {
        db.resolve_resource_alert_event(id)
    }

    pub async fn collect_server(
        db: &Database,
        alias: &str,
    ) -> Result<ResourceMetricSnapshot, AppError> {
        let alias = alias.trim();
        if alias.is_empty() {
            return Err(AppError::InvalidInput("服务器别名不能为空".into()));
        }
        let server = db
            .get_ssh_server(alias)?
            .ok_or_else(|| AppError::NotFound(format!("服务器 '{}' 不存在", alias)))?;
        db.ensure_resource_monitor_target("server", alias, &server.alias)?;

        let started = Instant::now();
        let command = Self::server_collect_command();
        let result = TerminalService::execute(
            db,
            crate::models::TerminalCommandInput {
                server_alias: alias.to_string(),
                command,
                timeout_secs: Some(12),
                initiated_by_ai: None,
            },
        )
        .await;

        let duration_ms = started.elapsed().as_millis() as i64;
        let snapshot = match result {
            Ok(output) if !output.blocked && output.exit_status == 0 => {
                let metrics = parse_server_metrics(&output.stdout);
                let summary = build_server_summary(&metrics);
                let status = status_from_summary(&summary);
                db.save_resource_metric_snapshot(
                    "server",
                    alias,
                    status,
                    duration_ms,
                    &summary,
                    &metrics,
                    None,
                )?
            }
            Ok(output) => {
                let message = if output.stderr.trim().is_empty() {
                    output.message
                } else {
                    output.stderr
                };
                Self::save_failed_snapshot(db, "server", alias, duration_ms, &message)?
            }
            Err(error) => {
                Self::save_failed_snapshot(db, "server", alias, duration_ms, &error.to_string())?
            }
        };

        let _ = AuditService::create(
            db,
            CreateAuditLogInput {
                actor: "local-user".into(),
                source: "resource_monitor".into(),
                server_alias: alias.to_string(),
                action: "resource.collect.server".into(),
                risk: "readonly".into(),
                result: if snapshot.status == "failed" {
                    "失败".into()
                } else {
                    "成功".into()
                },
                summary: format!("采集服务器资源状态：{}", alias),
                detail_json: Some(
                    json!({
                        "snapshotId": snapshot.id,
                        "status": snapshot.status,
                        "durationMs": snapshot.duration_ms,
                        "error": snapshot.error
                    })
                    .to_string(),
                ),
                request_id: None,
                approval_id: None,
            },
        );
        Self::evaluate_alerts(db, &snapshot)?;
        Ok(snapshot)
    }

    pub async fn collect_database(
        db: &Database,
        connection_key: &str,
    ) -> Result<ResourceMetricSnapshot, AppError> {
        let connection_key = connection_key.trim();
        if connection_key.is_empty() {
            return Err(AppError::InvalidInput("数据库连接 Key 不能为空".into()));
        }
        let row = db
            .get_database_connection_secret_row(connection_key)?
            .ok_or_else(|| AppError::NotFound(format!("数据库连接 '{}' 不存在", connection_key)))?;
        let connection = row.connection;
        if connection.db_type == "redis" {
            return Self::collect_redis(db, connection_key).await;
        }
        db.ensure_resource_monitor_target(&connection.db_type, connection_key, &connection.name)?;

        let started = Instant::now();
        let snapshot = if connection.connection_mode != "direct" {
            Self::save_failed_snapshot(
                db,
                &connection.db_type,
                connection_key,
                started.elapsed().as_millis() as i64,
                "SSH 隧道数据库资源采集会在隧道连接模块接入后启用",
            )?
        } else {
            let password = DatabaseOpsService::resolve_connection_password(
                db,
                &connection,
                row.password_nonce.as_deref(),
                row.password_ciphertext.as_deref(),
            )?;
            let result = match connection.db_type.as_str() {
                "mysql" => collect_mysql_metrics(&connection, password.as_deref()).await,
                "postgresql" => collect_postgres_metrics(&connection, password.as_deref()).await,
                _ => Err(AppError::InvalidInput("数据库类型无效".into())),
            };
            let duration_ms = started.elapsed().as_millis() as i64;
            match result {
                Ok((summary, metrics)) => {
                    let status = status_from_database_summary(&summary);
                    db.save_resource_metric_snapshot(
                        &connection.db_type,
                        connection_key,
                        status,
                        duration_ms,
                        &summary,
                        &metrics,
                        None,
                    )?
                }
                Err(error) => Self::save_failed_snapshot(
                    db,
                    &connection.db_type,
                    connection_key,
                    duration_ms,
                    &error.to_string(),
                )?,
            }
        };

        Self::audit_collect(
            db,
            "resource.collect.database",
            "",
            &connection.db_type,
            connection_key,
            &snapshot,
        );
        Self::evaluate_alerts(db, &snapshot)?;
        Ok(snapshot)
    }

    pub async fn list_mysql_slow_queries(
        db: &Database,
        input: MysqlSlowQueryListInput,
    ) -> Result<Vec<MysqlSlowQuery>, AppError> {
        let (connection, password) =
            Self::resolve_direct_mysql_connection(db, &input.connection_key)?;
        let min_elapsed_secs = input.min_elapsed_secs.unwrap_or(5).clamp(0, 86_400);
        let limit = input.limit.unwrap_or(100).clamp(1, 200);
        let rows =
            list_mysql_slow_queries(&connection, password.as_deref(), min_elapsed_secs, limit)
                .await?;
        let _ = AuditService::create(
            db,
            CreateAuditLogInput {
                actor: "local-user".into(),
                source: "resource_monitor".into(),
                server_alias: String::new(),
                action: "resource.mysql.slow_queries.list".into(),
                risk: "readonly".into(),
                result: "成功".into(),
                summary: format!("查看 MySQL 慢查询：{}", input.connection_key),
                detail_json: Some(
                    json!({
                        "connectionKey": input.connection_key,
                        "minElapsedSecs": min_elapsed_secs,
                        "count": rows.len()
                    })
                    .to_string(),
                ),
                request_id: None,
                approval_id: None,
            },
        );
        Ok(rows)
    }

    pub async fn kill_mysql_query(
        db: &Database,
        input: KillMysqlQueryInput,
    ) -> Result<KillMysqlQueryResult, AppError> {
        if input.process_id <= 0 {
            return Err(AppError::InvalidInput("MySQL 线程 ID 无效".into()));
        }
        let (connection, password) =
            Self::resolve_direct_mysql_connection(db, &input.connection_key)?;
        kill_mysql_query(&connection, password.as_deref(), input.process_id).await?;
        let result = KillMysqlQueryResult {
            process_id: input.process_id,
            killed: true,
            message: format!("已发送 KILL QUERY {}", input.process_id),
        };
        let _ = AuditService::create(
            db,
            CreateAuditLogInput {
                actor: "local-user".into(),
                source: "resource_monitor".into(),
                server_alias: String::new(),
                action: "resource.mysql.query.kill".into(),
                risk: "write".into(),
                result: "成功".into(),
                summary: format!(
                    "终止 MySQL 查询：{} / {}",
                    input.connection_key, input.process_id
                ),
                detail_json: Some(
                    json!({
                        "connectionKey": input.connection_key,
                        "processId": input.process_id
                    })
                    .to_string(),
                ),
                request_id: None,
                approval_id: None,
            },
        );
        Ok(result)
    }

    pub async fn collect_redis(
        db: &Database,
        connection_key: &str,
    ) -> Result<ResourceMetricSnapshot, AppError> {
        let connection_key = connection_key.trim();
        if connection_key.is_empty() {
            return Err(AppError::InvalidInput("Redis 连接 Key 不能为空".into()));
        }
        let row = db
            .get_database_connection_secret_row(connection_key)?
            .ok_or_else(|| AppError::NotFound(format!("Redis 连接 '{}' 不存在", connection_key)))?;
        let connection = row.connection;
        if connection.db_type != "redis" {
            return Err(AppError::InvalidInput("该连接不是 Redis 类型".into()));
        }
        db.ensure_resource_monitor_target("redis", connection_key, &connection.name)?;
        let started = Instant::now();
        let snapshot = if connection.connection_mode != "direct" {
            Self::save_failed_snapshot(
                db,
                "redis",
                connection_key,
                started.elapsed().as_millis() as i64,
                "SSH 隧道 Redis 资源采集会在隧道连接模块接入后启用",
            )?
        } else {
            let password = DatabaseOpsService::resolve_connection_password(
                db,
                &connection,
                row.password_nonce.as_deref(),
                row.password_ciphertext.as_deref(),
            )?;
            let result = collect_redis_metrics(&connection, password.as_deref()).await;
            let duration_ms = started.elapsed().as_millis() as i64;
            match result {
                Ok((summary, metrics)) => {
                    let status = status_from_redis_summary(&summary);
                    db.save_resource_metric_snapshot(
                        "redis",
                        connection_key,
                        status,
                        duration_ms,
                        &summary,
                        &metrics,
                        None,
                    )?
                }
                Err(error) => Self::save_failed_snapshot(
                    db,
                    "redis",
                    connection_key,
                    duration_ms,
                    &error.to_string(),
                )?,
            }
        };

        Self::audit_collect(
            db,
            "resource.collect.redis",
            "",
            "redis",
            connection_key,
            &snapshot,
        );
        Self::evaluate_alerts(db, &snapshot)?;
        Ok(snapshot)
    }

    pub async fn collect_batch(
        db: &Database,
        input: CollectResourceBatchInput,
    ) -> Result<CollectResourceBatchResult, AppError> {
        let targets = Self::list_targets(db)?;
        let target_type = input.target_type.as_deref().map(str::trim).unwrap_or("");
        let only_enabled = input.only_enabled.unwrap_or(true);
        let mut snapshots = Vec::new();
        let mut total = 0;
        let mut success = 0;
        let mut failed = 0;

        for target in targets {
            if only_enabled && !target.enabled {
                continue;
            }
            if !target_type.is_empty() && target.target_type != target_type {
                continue;
            }
            total += 1;
            let snapshot = match target.target_type.as_str() {
                "server" => Self::collect_server(db, &target.target_key).await?,
                "mysql" | "postgresql" => Self::collect_database(db, &target.target_key).await?,
                "redis" => Self::collect_redis(db, &target.target_key).await?,
                _ => continue,
            };
            if snapshot.status == "failed" {
                failed += 1;
            } else {
                success += 1;
            }
            snapshots.push(snapshot);
        }

        Ok(CollectResourceBatchResult {
            total,
            success,
            failed,
            snapshots,
        })
    }

    fn sync_default_targets(db: &Database) -> Result<(), AppError> {
        for server in db.list_ssh_servers()? {
            if server.enabled && server.source != "jumpserver" && server.status != "web" {
                db.ensure_resource_monitor_target("server", &server.alias, &server.alias)?;
            }
        }
        for connection in db.list_database_connections()? {
            if connection.enabled {
                db.ensure_resource_monitor_target(
                    &connection.db_type,
                    &connection.key,
                    &connection.name,
                )?;
            }
        }
        Ok(())
    }

    fn validate_target(target_type: &str, target_key: &str) -> Result<(), AppError> {
        if !["server", "mysql", "postgresql", "redis"].contains(&target_type) {
            return Err(AppError::InvalidInput("资源类型无效".into()));
        }
        if target_key.trim().is_empty() {
            return Err(AppError::InvalidInput("资源 Key 不能为空".into()));
        }
        Ok(())
    }

    fn resolve_direct_mysql_connection(
        db: &Database,
        connection_key: &str,
    ) -> Result<(DatabaseConnection, Option<String>), AppError> {
        let connection_key = connection_key.trim();
        if connection_key.is_empty() {
            return Err(AppError::InvalidInput("MySQL 连接 Key 不能为空".into()));
        }
        let row = db
            .get_database_connection_secret_row(connection_key)?
            .ok_or_else(|| AppError::NotFound(format!("MySQL 连接 '{}' 不存在", connection_key)))?;
        let connection = row.connection;
        if connection.db_type != "mysql" {
            return Err(AppError::InvalidInput("该资源不是 MySQL 连接".into()));
        }
        if connection.connection_mode != "direct" {
            return Err(AppError::InvalidInput(
                "SSH 隧道 MySQL 慢查询查看会在隧道连接模块接入后启用".into(),
            ));
        }
        let password = DatabaseOpsService::resolve_connection_password(
            db,
            &connection,
            row.password_nonce.as_deref(),
            row.password_ciphertext.as_deref(),
        )?;
        Ok((connection, password))
    }

    fn save_failed_snapshot(
        db: &Database,
        target_type: &str,
        target_key: &str,
        duration_ms: i64,
        message: &str,
    ) -> Result<ResourceMetricSnapshot, AppError> {
        let summary = json!({
            "statusText": "采集失败",
            "error": message,
        });
        let metrics = json!({
            "error": message,
        });
        db.save_resource_metric_snapshot(
            target_type,
            target_key,
            "failed",
            duration_ms,
            &summary,
            &metrics,
            Some(message),
        )
    }

    fn audit_collect(
        db: &Database,
        action: &str,
        server_alias: &str,
        target_type: &str,
        target_key: &str,
        snapshot: &ResourceMetricSnapshot,
    ) {
        let _ = AuditService::create(
            db,
            CreateAuditLogInput {
                actor: "local-user".into(),
                source: "resource_monitor".into(),
                server_alias: server_alias.into(),
                action: action.into(),
                risk: "readonly".into(),
                result: if snapshot.status == "failed" {
                    "失败".into()
                } else {
                    "成功".into()
                },
                summary: format!("采集资源状态：{} / {}", target_type, target_key),
                detail_json: Some(
                    json!({
                        "targetType": target_type,
                        "targetKey": target_key,
                        "snapshotId": snapshot.id,
                        "status": snapshot.status,
                        "durationMs": snapshot.duration_ms,
                        "error": snapshot.error
                    })
                    .to_string(),
                ),
                request_id: None,
                approval_id: None,
            },
        );
    }

    fn evaluate_alerts(db: &Database, snapshot: &ResourceMetricSnapshot) -> Result<(), AppError> {
        let rules = db.list_resource_alert_rules(&ListResourceAlertRulesInput {
            target_type: Some(snapshot.target_type.clone()),
            target_key: Some(snapshot.target_key.clone()),
            enabled: Some(true),
        })?;
        for rule in rules {
            if rule.target_key != "*" && rule.target_key != snapshot.target_key {
                continue;
            }
            let Some(value) = metric_value(&snapshot.summary, &rule.metric_key) else {
                continue;
            };
            if compare_metric(value, &rule.operator, rule.threshold_value) {
                let message = format!(
                    "{} {} {}，当前值 {:.2}，阈值 {:.2}",
                    snapshot.target_key,
                    rule.metric_key,
                    rule.operator,
                    value,
                    rule.threshold_value
                );
                let event = db.open_or_update_resource_alert_event(
                    &rule,
                    &snapshot.target_type,
                    &snapshot.target_key,
                    value,
                    &message,
                    snapshot.id,
                )?;
                let _ = AuditService::create(
                    db,
                    CreateAuditLogInput {
                        actor: "system".into(),
                        source: "resource_monitor".into(),
                        server_alias: if snapshot.target_type == "server" {
                            snapshot.target_key.clone()
                        } else {
                            String::new()
                        },
                        action: "resource.alert.open".into(),
                        risk: "readonly".into(),
                        result: "成功".into(),
                        summary: format!("资源阈值告警：{}", message),
                        detail_json: Some(
                            json!({
                                "eventId": event.id,
                                "ruleId": rule.id,
                                "targetType": snapshot.target_type,
                                "targetKey": snapshot.target_key,
                                "metricKey": rule.metric_key,
                                "metricValue": value,
                                "thresholdValue": rule.threshold_value,
                                "severity": rule.severity
                            })
                            .to_string(),
                        ),
                        request_id: None,
                        approval_id: None,
                    },
                );
            } else {
                db.auto_resolve_resource_alert_event(
                    rule.id,
                    &snapshot.target_type,
                    &snapshot.target_key,
                )?;
            }
        }
        Ok(())
    }

    fn validate_alert_rule(input: &UpsertResourceAlertRuleInput) -> Result<(), AppError> {
        if !["server", "mysql", "postgresql", "redis"].contains(&input.target_type.as_str()) {
            return Err(AppError::InvalidInput("告警资源类型无效".into()));
        }
        if input.metric_key.trim().is_empty() {
            return Err(AppError::InvalidInput("告警指标不能为空".into()));
        }
        if ![">", ">=", "<", "<=", "=="].contains(&input.operator.as_str()) {
            return Err(AppError::InvalidInput("告警操作符无效".into()));
        }
        if !["info", "warning", "critical"].contains(&input.severity.as_str()) {
            return Err(AppError::InvalidInput("告警级别无效".into()));
        }
        Ok(())
    }

    fn server_collect_command() -> String {
        r#"printf '__CPU1__\n'; head -n 1 /proc/stat 2>/dev/null; printf '__NET1__\n'; cat /proc/net/dev 2>/dev/null; printf '__DISK1__\n'; cat /proc/diskstats 2>/dev/null; sleep 1; printf '__CPU2__\n'; head -n 1 /proc/stat 2>/dev/null; printf '__NET2__\n'; cat /proc/net/dev 2>/dev/null; printf '__DISK2__\n'; cat /proc/diskstats 2>/dev/null; printf '__LOAD__\n'; cat /proc/loadavg 2>/dev/null; printf '__MEM__\n'; cat /proc/meminfo 2>/dev/null; printf '__DF__\n'; df -P -B1 2>/dev/null; printf '__PS_CPU__\n'; ps -eo pid,comm,pcpu,pmem,rss --sort=-pcpu 2>/dev/null | head -n 11; printf '__PS_MEM__\n'; ps -eo pid,comm,pcpu,pmem,rss --sort=-pmem 2>/dev/null | head -n 11; printf '__PORTS__\n'; (ss -tuln 2>/dev/null || netstat -tuln 2>/dev/null) | head -n 30; printf '__UNAME__\n'; uname -a 2>/dev/null; printf '__UPTIME__\n'; uptime -p 2>/dev/null || uptime 2>/dev/null"#.into()
    }
}

fn parse_server_metrics(output: &str) -> serde_json::Value {
    let cpu1 = section(output, "__CPU1__");
    let cpu2 = section(output, "__CPU2__");
    let net1 = section(output, "__NET1__");
    let net2 = section(output, "__NET2__");
    let disk1 = section(output, "__DISK1__");
    let disk2 = section(output, "__DISK2__");

    json!({
        "cpu": {
            "usagePercent": cpu_usage(cpu1, cpu2),
            "load": parse_load(section(output, "__LOAD__")),
        },
        "memory": parse_meminfo(section(output, "__MEM__")),
        "disk": {
            "filesystems": parse_df(section(output, "__DF__")),
            "io": disk_io_per_sec(disk1, disk2),
        },
        "network": network_per_sec(net1, net2),
        "processes": {
            "topCpu": parse_ps(section(output, "__PS_CPU__")),
            "topMemory": parse_ps(section(output, "__PS_MEM__")),
        },
        "ports": parse_ports(section(output, "__PORTS__")),
        "system": {
            "uname": section(output, "__UNAME__").trim(),
            "uptime": section(output, "__UPTIME__").trim(),
        }
    })
}

async fn collect_mysql_metrics(
    connection: &DatabaseConnection,
    password: Option<&str>,
) -> Result<(serde_json::Value, serde_json::Value), AppError> {
    use sqlx::{mysql::MySqlPoolOptions, Row};

    let url = DatabaseOpsService::mysql_url(connection, password);
    let pool = MySqlPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(std::time::Duration::from_secs(5))
        .connect(&url)
        .await
        .map_err(|error| AppError::Custom(format!("MySQL 资源采集连接失败: {}", error)))?;

    let version = sqlx::query_scalar::<_, String>("SELECT VERSION()")
        .fetch_one(&pool)
        .await
        .unwrap_or_default();
    let status_rows = sqlx::query(
        "SHOW GLOBAL STATUS WHERE Variable_name IN (
            'Threads_connected','Threads_running','Questions','Slow_queries',
            'Connections','Com_select','Com_insert','Com_update','Com_delete',
            'Innodb_buffer_pool_read_requests','Innodb_buffer_pool_reads'
        )",
    )
    .fetch_all(&pool)
    .await
    .unwrap_or_default();
    let mut status = serde_json::Map::new();
    for row in status_rows {
        let key = row.try_get::<String, _>(0).unwrap_or_default();
        let value = row.try_get::<String, _>(1).unwrap_or_default();
        status.insert(key, json!(value.parse::<f64>().unwrap_or(0.0)));
    }

    let max_connections = sqlx::query("SHOW VARIABLES LIKE 'max_connections'")
        .fetch_optional(&pool)
        .await
        .ok()
        .flatten()
        .and_then(|row| row.try_get::<String, _>(1).ok())
        .and_then(|value| value.parse::<f64>().ok());
    let configured_database = connection.database_name.trim();
    let (database_size, table_count, size_scope) = if configured_database.is_empty() {
        let database_size = sqlx::query_scalar::<_, i64>(
            "SELECT CAST(COALESCE(SUM(COALESCE(data_length, 0) + COALESCE(index_length, 0)), 0) AS SIGNED)
             FROM information_schema.tables
             WHERE table_schema NOT IN ('mysql','information_schema','performance_schema','sys')",
        )
        .fetch_one(&pool)
        .await
        .unwrap_or(0) as f64;
        let table_count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*)
             FROM information_schema.tables
             WHERE table_schema NOT IN ('mysql','information_schema','performance_schema','sys')",
        )
        .fetch_one(&pool)
        .await
        .unwrap_or(0);
        (database_size, table_count, "all_non_system".to_string())
    } else {
        let database_size = sqlx::query_scalar::<_, i64>(
            "SELECT CAST(COALESCE(SUM(COALESCE(data_length, 0) + COALESCE(index_length, 0)), 0) AS SIGNED)
             FROM information_schema.tables
             WHERE table_schema = ?",
        )
        .bind(configured_database)
        .fetch_one(&pool)
        .await
        .unwrap_or(0) as f64;
        let table_count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*)
             FROM information_schema.tables
             WHERE table_schema = ?",
        )
        .bind(configured_database)
        .fetch_one(&pool)
        .await
        .unwrap_or(0);
        (database_size, table_count, configured_database.to_string())
    };
    let threads_connected = status
        .get("Threads_connected")
        .and_then(|value| value.as_f64())
        .unwrap_or(0.0);
    let connection_usage = max_connections.and_then(|max| percent(threads_connected, max));
    let buffer_reads = status
        .get("Innodb_buffer_pool_reads")
        .and_then(|value| value.as_f64())
        .unwrap_or(0.0);
    let buffer_requests = status
        .get("Innodb_buffer_pool_read_requests")
        .and_then(|value| value.as_f64())
        .unwrap_or(0.0);
    let buffer_hit = if buffer_requests > 0.0 {
        Some(((buffer_requests - buffer_reads) / buffer_requests * 100.0).clamp(0.0, 100.0))
    } else {
        None
    };
    let slow_query_threshold_secs = mysql_slow_query_threshold_secs(&pool).await;
    let current_slow_queries =
        count_current_mysql_slow_queries(&pool, slow_query_threshold_secs).await;
    pool.close().await;

    let summary = json!({
        "connectionUsagePercent": connection_usage,
        "activeConnections": threads_connected,
        "maxConnections": max_connections,
        "databaseSizeBytes": database_size,
        "databaseSizeScope": size_scope,
        "tableCount": table_count,
        "cacheHitPercent": buffer_hit,
        "slowQueries": current_slow_queries,
        "cumulativeSlowQueries": status.get("Slow_queries").and_then(|value| value.as_f64()),
        "slowQueryThresholdSecs": slow_query_threshold_secs,
        "statusText": "采集成功",
    });
    let metrics = json!({
        "engine": "mysql",
        "version": version,
        "status": status,
        "databaseSizeScope": size_scope,
        "summary": summary,
    });
    Ok((summary, metrics))
}

async fn list_mysql_slow_queries(
    connection: &DatabaseConnection,
    password: Option<&str>,
    min_elapsed_secs: i64,
    limit: i64,
) -> Result<Vec<MysqlSlowQuery>, AppError> {
    use sqlx::{mysql::MySqlPoolOptions, Row};

    let url = DatabaseOpsService::mysql_url(connection, password);
    let pool = MySqlPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(std::time::Duration::from_secs(5))
        .connect(&url)
        .await
        .map_err(|error| AppError::Custom(format!("MySQL 慢查询连接失败: {}", error)))?;

    let rows = sqlx::query(
        "SELECT ID, USER, HOST, DB, COMMAND, TIME, STATE, INFO
         FROM information_schema.PROCESSLIST
         WHERE COMMAND IN ('Query','Execute')
           AND TIME >= ?
           AND ID <> CONNECTION_ID()
           AND INFO IS NOT NULL
         ORDER BY TIME DESC
         LIMIT ?",
    )
    .bind(min_elapsed_secs)
    .bind(limit)
    .fetch_all(&pool)
    .await
    .map_err(|error| AppError::Custom(format!("读取 MySQL 慢查询失败: {}", error)))?;
    pool.close().await;

    Ok(rows
        .into_iter()
        .map(|row| MysqlSlowQuery {
            process_id: row.try_get::<i64, _>("ID").unwrap_or_default(),
            user: row.try_get::<String, _>("USER").unwrap_or_default(),
            host: row.try_get::<String, _>("HOST").unwrap_or_default(),
            database: row.try_get::<Option<String>, _>("DB").unwrap_or(None),
            command: row.try_get::<String, _>("COMMAND").unwrap_or_default(),
            elapsed_secs: row.try_get::<i64, _>("TIME").unwrap_or_default(),
            state: row.try_get::<Option<String>, _>("STATE").unwrap_or(None),
            info: row.try_get::<Option<String>, _>("INFO").unwrap_or(None),
        })
        .collect())
}

async fn mysql_slow_query_threshold_secs(pool: &sqlx::MySqlPool) -> i64 {
    let value = sqlx::query_scalar::<_, f64>("SELECT @@long_query_time")
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .unwrap_or(5.0);
    value.ceil().max(0.0) as i64
}

async fn count_current_mysql_slow_queries(pool: &sqlx::MySqlPool, min_elapsed_secs: i64) -> f64 {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*)
         FROM information_schema.PROCESSLIST
         WHERE COMMAND IN ('Query','Execute')
           AND TIME >= ?
           AND ID <> CONNECTION_ID()
           AND INFO IS NOT NULL",
    )
    .bind(min_elapsed_secs)
    .fetch_one(pool)
    .await
    .unwrap_or(0) as f64
}

async fn kill_mysql_query(
    connection: &DatabaseConnection,
    password: Option<&str>,
    process_id: i64,
) -> Result<(), AppError> {
    use sqlx::mysql::MySqlPoolOptions;

    let url = DatabaseOpsService::mysql_url(connection, password);
    let pool = MySqlPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(std::time::Duration::from_secs(5))
        .connect(&url)
        .await
        .map_err(|error| AppError::Custom(format!("MySQL 终止查询连接失败: {}", error)))?;

    // MySQL 不支持把 KILL QUERY 的线程 ID 作为普通参数绑定；调用方已拒绝非正整数。
    let kill_sql = format!("KILL QUERY {}", process_id);
    sqlx::query(sqlx::AssertSqlSafe(kill_sql))
        .execute(&pool)
        .await
        .map_err(|error| AppError::Custom(format!("终止 MySQL 查询失败: {}", error)))?;
    pool.close().await;
    Ok(())
}

async fn collect_postgres_metrics(
    connection: &DatabaseConnection,
    password: Option<&str>,
) -> Result<(serde_json::Value, serde_json::Value), AppError> {
    use sqlx::{postgres::PgPoolOptions, Row};

    let url = DatabaseOpsService::postgres_url(connection, password);
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(std::time::Duration::from_secs(5))
        .connect(&url)
        .await
        .map_err(|error| AppError::Custom(format!("PostgreSQL 资源采集连接失败: {}", error)))?;

    let version = sqlx::query_scalar::<_, String>("SELECT version()")
        .fetch_one(&pool)
        .await
        .unwrap_or_default();
    let active_connections = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM pg_stat_activity")
        .fetch_one(&pool)
        .await
        .unwrap_or(0);
    let database_size = sqlx::query_scalar::<_, i64>("SELECT pg_database_size(current_database())")
        .fetch_one(&pool)
        .await
        .unwrap_or(0);
    let table_count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*)
         FROM information_schema.tables
         WHERE table_schema NOT IN ('pg_catalog','information_schema')",
    )
    .fetch_one(&pool)
    .await
    .unwrap_or(0);
    let lock_count =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM pg_locks WHERE NOT granted")
            .fetch_one(&pool)
            .await
            .unwrap_or(0);
    let stat = sqlx::query(
        "SELECT blks_hit, blks_read, xact_commit, xact_rollback, deadlocks
         FROM pg_stat_database
         WHERE datname = current_database()",
    )
    .fetch_optional(&pool)
    .await
    .ok()
    .flatten();
    let (blks_hit, blks_read, xact_commit, xact_rollback, deadlocks) = stat
        .as_ref()
        .map(|row| {
            (
                row.try_get::<i64, _>("blks_hit").unwrap_or(0),
                row.try_get::<i64, _>("blks_read").unwrap_or(0),
                row.try_get::<i64, _>("xact_commit").unwrap_or(0),
                row.try_get::<i64, _>("xact_rollback").unwrap_or(0),
                row.try_get::<i64, _>("deadlocks").unwrap_or(0),
            )
        })
        .unwrap_or((0, 0, 0, 0, 0));
    let cache_hit = percent(blks_hit as f64, (blks_hit + blks_read) as f64);
    pool.close().await;

    let summary = json!({
        "activeConnections": active_connections,
        "databaseSizeBytes": database_size,
        "tableCount": table_count,
        "lockWaits": lock_count,
        "cacheHitPercent": cache_hit,
        "deadlocks": deadlocks,
        "statusText": "采集成功",
    });
    let metrics = json!({
        "engine": "postgresql",
        "version": version,
        "activity": {
            "activeConnections": active_connections,
            "lockWaits": lock_count,
        },
        "database": {
            "sizeBytes": database_size,
            "tableCount": table_count,
            "blksHit": blks_hit,
            "blksRead": blks_read,
            "xactCommit": xact_commit,
            "xactRollback": xact_rollback,
            "deadlocks": deadlocks,
        },
        "summary": summary,
    });
    Ok((summary, metrics))
}

async fn collect_redis_metrics(
    connection: &DatabaseConnection,
    password: Option<&str>,
) -> Result<(serde_json::Value, serde_json::Value), AppError> {
    let url = DatabaseOpsService::redis_url(connection, password);
    let client = redis::Client::open(url)
        .map_err(|error| AppError::Custom(format!("Redis URL 无效: {}", error)))?;
    let mut conn = client
        .get_connection()
        .map_err(|error| AppError::Custom(format!("Redis 资源采集连接失败: {}", error)))?;
    let info: String = redis::cmd("INFO")
        .query(&mut conn)
        .map_err(|error| AppError::Custom(format!("读取 Redis INFO 失败: {}", error)))?;
    let current_dbsize: i64 = redis::cmd("DBSIZE").query(&mut conn).unwrap_or(0);
    let slowlog_len: i64 = redis::cmd("SLOWLOG")
        .arg("LEN")
        .query(&mut conn)
        .unwrap_or(0);
    let info_map = parse_redis_info(&info);
    let keyspace = parse_redis_keyspace(&info_map);
    let total_keys = keyspace
        .values()
        .filter_map(|item| item.get("keys").and_then(|value| value.as_i64()))
        .sum::<i64>();
    let used_memory = redis_info_f64(&info_map, "used_memory");
    let maxmemory = redis_info_f64(&info_map, "maxmemory");
    let hits = redis_info_f64(&info_map, "keyspace_hits").unwrap_or(0.0);
    let misses = redis_info_f64(&info_map, "keyspace_misses").unwrap_or(0.0);
    let hit_percent = percent(hits, hits + misses);
    let memory_usage = maxmemory.and_then(|max| {
        if max > 0.0 {
            percent(used_memory.unwrap_or(0.0), max)
        } else {
            None
        }
    });
    let summary = json!({
        "usedMemoryBytes": used_memory,
        "maxMemoryBytes": maxmemory,
        "memoryUsagePercent": memory_usage,
        "connectedClients": redis_info_f64(&info_map, "connected_clients"),
        "keyCount": total_keys,
        "currentDbKeyCount": current_dbsize,
        "hitPercent": hit_percent,
        "expiredKeys": redis_info_f64(&info_map, "expired_keys"),
        "evictedKeys": redis_info_f64(&info_map, "evicted_keys"),
        "slowlogLen": slowlog_len,
        "statusText": "采集成功",
    });
    let metrics = json!({
        "engine": "redis",
        "info": info_map,
        "keyspace": keyspace,
        "dbsize": current_dbsize,
        "slowlogLen": slowlog_len,
        "summary": summary,
    });
    Ok((summary, metrics))
}

fn status_from_database_summary(summary: &serde_json::Value) -> &'static str {
    let conn = value_f64(summary, &["connectionUsagePercent"]).unwrap_or(0.0);
    let cache = value_f64(summary, &["cacheHitPercent"]).unwrap_or(100.0);
    let locks = value_f64(summary, &["lockWaits"]).unwrap_or(0.0);
    if conn >= 90.0 || cache < 80.0 || locks > 0.0 {
        "warning"
    } else {
        "healthy"
    }
}

fn status_from_redis_summary(summary: &serde_json::Value) -> &'static str {
    let memory = value_f64(summary, &["memoryUsagePercent"]).unwrap_or(0.0);
    let evicted = value_f64(summary, &["evictedKeys"]).unwrap_or(0.0);
    if memory >= 90.0 || evicted > 0.0 {
        "warning"
    } else {
        "healthy"
    }
}

fn parse_redis_info(info: &str) -> serde_json::Map<String, serde_json::Value> {
    let mut map = serde_json::Map::new();
    for line in info.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        if let Ok(number) = value.parse::<f64>() {
            map.insert(key.to_string(), json!(number));
        } else {
            map.insert(key.to_string(), json!(value));
        }
    }
    map
}

fn redis_info_f64(info: &serde_json::Map<String, serde_json::Value>, key: &str) -> Option<f64> {
    info.get(key).and_then(|value| value.as_f64())
}

fn parse_redis_keyspace(
    info: &serde_json::Map<String, serde_json::Value>,
) -> serde_json::Map<String, serde_json::Value> {
    let mut keyspace = serde_json::Map::new();
    for (key, value) in info {
        if !key.starts_with("db") {
            continue;
        }
        let Some(raw) = value.as_str() else {
            continue;
        };
        let mut item = serde_json::Map::new();
        for pair in raw.split(',') {
            let Some((name, number)) = pair.split_once('=') else {
                continue;
            };
            if let Ok(parsed) = number.parse::<i64>() {
                item.insert(name.to_string(), json!(parsed));
            }
        }
        keyspace.insert(key.clone(), serde_json::Value::Object(item));
    }
    keyspace
}

fn build_server_summary(metrics: &serde_json::Value) -> serde_json::Value {
    let cpu = value_f64(metrics, &["cpu", "usagePercent"]);
    let memory = value_f64(metrics, &["memory", "usagePercent"]);
    let swap = value_f64(metrics, &["memory", "swapUsagePercent"]);
    let disk = metrics
        .pointer("/disk/filesystems")
        .and_then(|value| value.as_array())
        .and_then(|items| {
            items
                .iter()
                .filter_map(|item| item.get("usagePercent").and_then(|v| v.as_f64()))
                .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
        });
    json!({
        "cpuUsagePercent": cpu,
        "memoryUsagePercent": memory,
        "swapUsagePercent": swap,
        "diskUsagePercent": disk,
        "networkRxBytesPerSec": value_f64(metrics, &["network", "rxBytesPerSec"]),
        "networkTxBytesPerSec": value_f64(metrics, &["network", "txBytesPerSec"]),
        "diskReadBytesPerSec": value_f64(metrics, &["disk", "io", "readBytesPerSec"]),
        "diskWriteBytesPerSec": value_f64(metrics, &["disk", "io", "writeBytesPerSec"]),
        "statusText": "采集成功",
    })
}

fn status_from_summary(summary: &serde_json::Value) -> &'static str {
    let cpu = value_f64(summary, &["cpuUsagePercent"]).unwrap_or(0.0);
    let mem = value_f64(summary, &["memoryUsagePercent"]).unwrap_or(0.0);
    let disk = value_f64(summary, &["diskUsagePercent"]).unwrap_or(0.0);
    if cpu >= 90.0 || mem >= 90.0 || disk >= 90.0 {
        "warning"
    } else {
        "healthy"
    }
}

fn section<'a>(output: &'a str, marker: &str) -> &'a str {
    let Some(start) = output.find(marker) else {
        return "";
    };
    let rest = &output[start + marker.len()..];
    let end = rest.find("\n__").unwrap_or(rest.len());
    rest[..end].trim_matches('\n')
}

fn cpu_usage(first: &str, second: &str) -> Option<f64> {
    let a = parse_cpu_line(first)?;
    let b = parse_cpu_line(second)?;
    let total_delta = b.0.saturating_sub(a.0) as f64;
    let idle_delta = b.1.saturating_sub(a.1) as f64;
    if total_delta <= 0.0 {
        return None;
    }
    Some(((total_delta - idle_delta) / total_delta * 100.0).clamp(0.0, 100.0))
}

fn parse_cpu_line(line: &str) -> Option<(u64, u64)> {
    let values = line
        .split_whitespace()
        .skip(1)
        .filter_map(|item| item.parse::<u64>().ok())
        .collect::<Vec<_>>();
    if values.len() < 5 {
        return None;
    }
    let idle = values.get(3).copied().unwrap_or(0) + values.get(4).copied().unwrap_or(0);
    let total = values.iter().sum();
    Some((total, idle))
}

fn parse_load(value: &str) -> serde_json::Value {
    let parts = value.split_whitespace().collect::<Vec<_>>();
    json!({
        "one": parts.first().and_then(|v| v.parse::<f64>().ok()),
        "five": parts.get(1).and_then(|v| v.parse::<f64>().ok()),
        "fifteen": parts.get(2).and_then(|v| v.parse::<f64>().ok()),
    })
}

fn parse_meminfo(value: &str) -> serde_json::Value {
    let read_kb = |key: &str| -> f64 {
        value
            .lines()
            .find(|line| line.starts_with(key))
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|item| item.parse::<f64>().ok())
            .unwrap_or(0.0)
    };
    let total = read_kb("MemTotal:") * 1024.0;
    let available = read_kb("MemAvailable:") * 1024.0;
    let swap_total = read_kb("SwapTotal:") * 1024.0;
    let swap_free = read_kb("SwapFree:") * 1024.0;
    let used = (total - available).max(0.0);
    let swap_used = (swap_total - swap_free).max(0.0);
    json!({
        "totalBytes": total,
        "availableBytes": available,
        "usedBytes": used,
        "usagePercent": percent(used, total),
        "swapTotalBytes": swap_total,
        "swapUsedBytes": swap_used,
        "swapUsagePercent": percent(swap_used, swap_total),
    })
}

fn parse_df(value: &str) -> Vec<serde_json::Value> {
    value
        .lines()
        .skip(1)
        .filter_map(|line| {
            let parts = line.split_whitespace().collect::<Vec<_>>();
            if parts.len() < 6 {
                return None;
            }
            let total = parts.get(1)?.parse::<f64>().ok()?;
            let used = parts.get(2)?.parse::<f64>().ok()?;
            Some(json!({
                "filesystem": parts[0],
                "mount": parts[5],
                "totalBytes": total,
                "usedBytes": used,
                "availableBytes": parts.get(3).and_then(|v| v.parse::<f64>().ok()),
                "usagePercent": percent(used, total),
            }))
        })
        .collect()
}

fn network_per_sec(first: &str, second: &str) -> serde_json::Value {
    let a = network_totals(first);
    let b = network_totals(second);
    json!({
        "rxBytesPerSec": b.0.saturating_sub(a.0),
        "txBytesPerSec": b.1.saturating_sub(a.1),
    })
}

fn network_totals(value: &str) -> (u64, u64) {
    let mut rx = 0;
    let mut tx = 0;
    for line in value.lines().filter(|line| line.contains(':')) {
        let Some((_, data)) = line.split_once(':') else {
            continue;
        };
        let parts = data.split_whitespace().collect::<Vec<_>>();
        rx += parts
            .first()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(0);
        tx += parts
            .get(8)
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(0);
    }
    (rx, tx)
}

fn disk_io_per_sec(first: &str, second: &str) -> serde_json::Value {
    let a = disk_totals(first);
    let b = disk_totals(second);
    json!({
        "readBytesPerSec": b.0.saturating_sub(a.0),
        "writeBytesPerSec": b.1.saturating_sub(a.1),
    })
}

fn disk_totals(value: &str) -> (u64, u64) {
    let mut read_sectors = 0;
    let mut write_sectors = 0;
    for line in value.lines() {
        let parts = line.split_whitespace().collect::<Vec<_>>();
        if parts.len() < 10 {
            continue;
        }
        read_sectors += parts
            .get(5)
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(0);
        write_sectors += parts
            .get(9)
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(0);
    }
    (read_sectors * 512, write_sectors * 512)
}

fn parse_ps(value: &str) -> Vec<serde_json::Value> {
    value
        .lines()
        .skip(1)
        .filter_map(|line| {
            let parts = line.split_whitespace().collect::<Vec<_>>();
            if parts.len() < 5 {
                return None;
            }
            Some(json!({
                "pid": parts[0],
                "command": parts[1],
                "cpuPercent": parts[2].parse::<f64>().ok(),
                "memoryPercent": parts[3].parse::<f64>().ok(),
                "rssKb": parts[4].parse::<f64>().ok(),
            }))
        })
        .collect()
}

fn parse_ports(value: &str) -> Vec<serde_json::Value> {
    value
        .lines()
        .skip(1)
        .take(20)
        .map(|line| json!({ "raw": line.trim() }))
        .collect()
}

fn percent(used: f64, total: f64) -> Option<f64> {
    if total <= 0.0 {
        return None;
    }
    Some((used / total * 100.0).clamp(0.0, 100.0))
}

fn value_f64(value: &serde_json::Value, path: &[&str]) -> Option<f64> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }
    current.as_f64()
}

fn metric_value(summary: &serde_json::Value, metric_key: &str) -> Option<f64> {
    let mut current = summary;
    for key in metric_key.split('.') {
        current = current.get(key)?;
    }
    current.as_f64()
}

fn compare_metric(value: f64, operator: &str, threshold: f64) -> bool {
    match operator {
        ">" => value > threshold,
        ">=" => value >= threshold,
        "<" => value < threshold,
        "<=" => value <= threshold,
        "==" => (value - threshold).abs() < f64::EPSILON,
        _ => false,
    }
}
