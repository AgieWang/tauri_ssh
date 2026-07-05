use crate::database::Database;
use crate::error::AppError;
use crate::models::{AuditLog, AuditLogExportResult, CreateAuditLogInput, ListAuditLogsInput};

pub struct AuditService;

impl AuditService {
    pub fn list(db: &Database, mut input: ListAuditLogsInput) -> Result<Vec<AuditLog>, AppError> {
        Self::normalize_filter(&mut input);
        Self::validate_filter(&input)?;
        db.list_audit_logs(&input)
    }

    pub fn create(db: &Database, mut input: CreateAuditLogInput) -> Result<AuditLog, AppError> {
        Self::normalize_create(&mut input);
        Self::validate_create(&input)?;
        db.create_audit_log(&input)
    }

    pub fn upsert_by_request_action(
        db: &Database,
        mut input: CreateAuditLogInput,
    ) -> Result<AuditLog, AppError> {
        Self::normalize_create(&mut input);
        Self::validate_create(&input)?;
        db.upsert_audit_log_by_request_action(&input)
    }

    pub fn export(
        db: &Database,
        input: ListAuditLogsInput,
    ) -> Result<AuditLogExportResult, AppError> {
        let rows = Self::list(db, input)?;
        let content = serde_json::to_string_pretty(&rows)?;
        let _ = Self::create(
            db,
            CreateAuditLogInput {
                actor: "local-user".into(),
                source: "audit".into(),
                server_alias: "".into(),
                action: "audit_export".into(),
                risk: "readonly".into(),
                result: "成功".into(),
                summary: format!("导出脱敏审计日志 {} 条", rows.len()),
                detail_json: Some(serde_json::json!({ "count": rows.len() }).to_string()),
                request_id: None,
                approval_id: None,
            },
        );
        Ok(AuditLogExportResult {
            file_name: format!(
                "tauri-ssh-audit-{}.json",
                chrono::Local::now().format("%Y%m%d%H%M%S")
            ),
            count: rows.len(),
            content,
        })
    }

    fn normalize_filter(input: &mut ListAuditLogsInput) {
        input.actor = trim_option(input.actor.take());
        input.source = trim_option(input.source.take());
        input.server_alias = trim_option(input.server_alias.take());
        input.action = trim_option(input.action.take());
        input.risk = trim_option(input.risk.take());
        input.result = trim_option(input.result.take());
        input.keyword = trim_option(input.keyword.take());
        input.limit = Some(input.limit.unwrap_or(200).clamp(1, 5000));
    }

    fn validate_filter(input: &ListAuditLogsInput) -> Result<(), AppError> {
        if let Some(risk) = input.risk.as_deref() {
            Self::validate_risk(risk)?;
        }
        if let Some(limit) = input.limit {
            if !(1..=5000).contains(&limit) {
                return Err(AppError::InvalidInput(
                    "审计日志查询数量必须在 1-5000 之间".into(),
                ));
            }
        }
        Ok(())
    }

    fn normalize_create(input: &mut CreateAuditLogInput) {
        input.actor = input.actor.trim().to_string();
        input.source = input.source.trim().to_string();
        input.server_alias = input.server_alias.trim().to_string();
        input.action = input.action.trim().to_string();
        input.risk = input.risk.trim().to_string();
        input.result = input.result.trim().to_string();
        input.summary = input.summary.trim().to_string();
        input.detail_json = input
            .detail_json
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .or_else(|| Some("{}".into()));
        input.request_id = input
            .request_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
    }

    fn validate_create(input: &CreateAuditLogInput) -> Result<(), AppError> {
        if input.actor.is_empty() {
            return Err(AppError::InvalidInput("操作者不能为空".into()));
        }
        if input.source.is_empty() {
            return Err(AppError::InvalidInput("审计来源不能为空".into()));
        }
        if input.action.is_empty() {
            return Err(AppError::InvalidInput("审计动作不能为空".into()));
        }
        if input.result.is_empty() {
            return Err(AppError::InvalidInput("执行结果不能为空".into()));
        }
        if input.summary.is_empty() {
            return Err(AppError::InvalidInput("审计摘要不能为空".into()));
        }
        Self::validate_risk(&input.risk)?;
        if let Some(detail_json) = input.detail_json.as_deref() {
            serde_json::from_str::<serde_json::Value>(detail_json)
                .map_err(|_| AppError::InvalidInput("detailJson 必须是合法 JSON".into()))?;
        }
        Ok(())
    }

    fn validate_risk(risk: &str) -> Result<(), AppError> {
        if !["L0", "L1", "L2", "L3", "readonly", "blocked", "ai"].contains(&risk) {
            return Err(AppError::InvalidInput("审计风险级别无效".into()));
        }
        Ok(())
    }
}

fn trim_option(value: Option<String>) -> Option<String> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_string)
}
