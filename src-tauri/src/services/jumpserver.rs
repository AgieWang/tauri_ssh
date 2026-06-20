use crate::database::Database;
use crate::error::AppError;
use crate::models::{
    CreateAuditLogInput, JumpServerOpenResult, JumpServerSession, UpsertJumpServerSessionInput,
};
use crate::services::audit::AuditService;

pub struct JumpServerService;

impl JumpServerService {
    pub fn list(db: &Database) -> Result<Vec<JumpServerSession>, AppError> {
        db.list_jumpserver_sessions()
    }

    pub fn upsert(
        db: &Database,
        mut input: UpsertJumpServerSessionInput,
    ) -> Result<JumpServerSession, AppError> {
        Self::normalize(&mut input);
        Self::validate(&input)?;
        db.upsert_jumpserver_session(&input)
    }

    pub fn open(db: &Database, key: &str) -> Result<JumpServerOpenResult, AppError> {
        let key = key.trim();
        if key.is_empty() {
            return Err(AppError::InvalidInput("会话 Key 不能为空".into()));
        }
        let session = db.mark_jumpserver_session_opened(key)?;
        if !session.enabled {
            return Err(AppError::InvalidInput("该堡垒机会话入口已禁用".into()));
        }
        let _ = AuditService::create(
            db,
            CreateAuditLogInput {
                actor: "local-user".into(),
                source: "jumpserver".into(),
                server_alias: session.asset_hint.clone(),
                action: "jumpserver_session_open".into(),
                risk: "blocked".into(),
                result: "建议-only".into(),
                summary: format!("打开堡垒机会话入口：{}", session.name),
                detail_json: Some(
                    serde_json::json!({
                        "sessionKey": session.key,
                        "endpoint": session.endpoint,
                        "protocol": session.protocol,
                        "aiMode": session.ai_mode,
                        "credentialExtracted": false
                    })
                    .to_string(),
                ),
                request_id: None,
                approval_id: None,
            },
        );
        Ok(JumpServerOpenResult {
            key: session.key,
            web_url: session.web_url,
            message: "已记录打开时间，请在浏览器或桌面窗口内继续完成 ISC/JumpServer 登录。".into(),
        })
    }

    pub fn delete(db: &Database, key: &str) -> Result<(), AppError> {
        let key = key.trim();
        if key.is_empty() {
            return Err(AppError::InvalidInput("会话 Key 不能为空".into()));
        }
        if !db.delete_jumpserver_session(key)? {
            return Err(AppError::NotFound(format!("堡垒机会话 '{}' 不存在", key)));
        }
        Ok(())
    }

    fn normalize(input: &mut UpsertJumpServerSessionInput) {
        input.key = input.key.trim().to_string();
        input.name = input.name.trim().to_string();
        input.endpoint = input.endpoint.trim().to_string();
        input.web_url = input.web_url.trim().to_string();
        input.session_ref = input.session_ref.trim().to_string();
        input.group_name = input.group_name.trim().to_string();
        input.account_hint = input.account_hint.trim().to_string();
        input.asset_hint = input.asset_hint.trim().to_string();
        input.protocol = input.protocol.trim().to_lowercase();
        input.ai_mode = input.ai_mode.trim().to_lowercase();
        input.status = input
            .status
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_lowercase);
        input.notes = input.notes.trim().to_string();
        if input.group_name.is_empty() {
            input.group_name = "堡垒机".into();
        }
        if input.protocol.is_empty() {
            input.protocol = "web_ssh".into();
        }
        if input.ai_mode.is_empty() {
            input.ai_mode = "suggest_only".into();
        }
    }

    fn validate(input: &UpsertJumpServerSessionInput) -> Result<(), AppError> {
        if input.key.is_empty() {
            return Err(AppError::InvalidInput("会话 Key 不能为空".into()));
        }
        if input.name.is_empty() {
            return Err(AppError::InvalidInput("会话名称不能为空".into()));
        }
        if input.endpoint.is_empty() {
            return Err(AppError::InvalidInput("堡垒机入口不能为空".into()));
        }
        if input.web_url.is_empty() {
            return Err(AppError::InvalidInput("Web SSH URL 不能为空".into()));
        }
        if !input.web_url.starts_with("http://") && !input.web_url.starts_with("https://") {
            return Err(AppError::InvalidInput(
                "Web SSH URL 必须以 http:// 或 https:// 开头".into(),
            ));
        }
        if !["web_ssh", "web_sftp", "jumpserver_asset"].contains(&input.protocol.as_str()) {
            return Err(AppError::InvalidInput("会话协议无效".into()));
        }
        if !["suggest_only", "disabled"].contains(&input.ai_mode.as_str()) {
            return Err(AppError::InvalidInput("AI 模式无效".into()));
        }
        if let Some(status) = input.status.as_deref() {
            if !["unknown", "available", "opened", "expired", "disabled"].contains(&status) {
                return Err(AppError::InvalidInput("会话状态无效".into()));
            }
        }
        Ok(())
    }
}
