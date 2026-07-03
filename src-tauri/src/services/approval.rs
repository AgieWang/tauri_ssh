use crate::database::Database;
use crate::error::AppError;
use crate::models::{
    ApprovalRequest, CreateApprovalRequestInput, DecideApprovalRequestInput,
    ListApprovalRequestsInput,
};
use crate::services::system_settings::SystemSettingsService;

pub struct ApprovalService;

impl ApprovalService {
    pub fn list(
        db: &Database,
        input: ListApprovalRequestsInput,
    ) -> Result<Vec<ApprovalRequest>, AppError> {
        Self::validate_list(&input)?;
        db.list_approval_requests(&input)
    }

    pub fn create(
        db: &Database,
        mut input: CreateApprovalRequestInput,
    ) -> Result<ApprovalRequest, AppError> {
        Self::normalize_create(&mut input);
        Self::validate_create(&input)?;
        let approval = db.create_approval_request(&input)?;
        if Self::should_auto_approve(db, &approval)? {
            return db.decide_approval_request(&DecideApprovalRequestInput {
                id: approval.id,
                decision: "approved".into(),
                note: "AI 临时放行已开启，系统自动确认；危险命令黑名单仍不放行。".into(),
                decided_by: "ai-unrestricted".into(),
            });
        }
        Ok(approval)
    }

    pub fn decide(
        db: &Database,
        mut input: DecideApprovalRequestInput,
    ) -> Result<ApprovalRequest, AppError> {
        input.decision = input.decision.trim().to_lowercase();
        input.note = input.note.trim().to_string();
        input.decided_by = input.decided_by.trim().to_string();
        Self::validate_decide(&input)?;
        db.decide_approval_request(&input)
    }

    fn validate_list(input: &ListApprovalRequestsInput) -> Result<(), AppError> {
        if let Some(status) = input.status.as_deref() {
            if ![
                "all",
                "pending",
                "approved",
                "rejected",
                "cancelled",
                "expired",
            ]
            .contains(&status)
            {
                return Err(AppError::InvalidInput("审批状态过滤条件无效".into()));
            }
        }
        Ok(())
    }

    fn normalize_create(input: &mut CreateApprovalRequestInput) {
        input.source = input.source.trim().to_lowercase();
        input.requester = input.requester.trim().to_string();
        input.server_alias = input.server_alias.trim().to_string();
        input.action = input.action.trim().to_lowercase();
        input.risk = input.risk.trim().to_string();
        input.command = input.command.trim().to_string();
        input.resource = input.resource.trim().to_string();
        input.reason = input.reason.trim().to_string();
        input.summary = input.summary.trim().to_string();
        input.payload_json = input
            .payload_json
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        input.expires_at = input
            .expires_at
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
    }

    fn validate_create(input: &CreateApprovalRequestInput) -> Result<(), AppError> {
        if input.source.is_empty() {
            return Err(AppError::InvalidInput("审批来源不能为空".into()));
        }
        if input.action.is_empty() {
            return Err(AppError::InvalidInput("审批动作不能为空".into()));
        }
        if input.command.is_empty() && input.resource.is_empty() {
            return Err(AppError::InvalidInput(
                "命令或资源路径至少需要填写一个".into(),
            ));
        }
        if !["readonly", "L1", "L2", "L3", "review", "high", "blocked"]
            .contains(&input.risk.as_str())
        {
            return Err(AppError::InvalidInput("审批风险级别无效".into()));
        }
        if let Some(payload_json) = input.payload_json.as_deref() {
            serde_json::from_str::<serde_json::Value>(payload_json)
                .map_err(|_| AppError::InvalidInput("payloadJson 必须是合法 JSON".into()))?;
        }
        Ok(())
    }

    fn validate_decide(input: &DecideApprovalRequestInput) -> Result<(), AppError> {
        if input.id <= 0 {
            return Err(AppError::InvalidInput("审批 ID 无效".into()));
        }
        if !["approved", "rejected", "cancelled"].contains(&input.decision.as_str()) {
            return Err(AppError::InvalidInput("审批决策无效".into()));
        }
        if input.decided_by.is_empty() {
            return Err(AppError::InvalidInput("决策人不能为空".into()));
        }
        Ok(())
    }

    fn should_auto_approve(db: &Database, approval: &ApprovalRequest) -> Result<bool, AppError> {
        if approval.risk == "blocked" {
            return Ok(false);
        }
        if !SystemSettingsService::get_ai_unrestricted_state(db)?.active {
            return Ok(false);
        }
        // AI 放行只绕过人工审批，不绕过危险命令黑名单。
        if SystemSettingsService::dangerous_command_match(db, &approval.command)?.is_some() {
            return Ok(false);
        }
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{CreateApprovalRequestInput, EnableAiUnrestrictedInput};

    fn approval_input(command: &str) -> CreateApprovalRequestInput {
        CreateApprovalRequestInput {
            source: "mcp".into(),
            requester: "test".into(),
            server_alias: String::new(),
            action: "database_execute".into(),
            risk: "L3".into(),
            command: command.into(),
            resource: "test-db".into(),
            reason: "test approval".into(),
            summary: "test approval".into(),
            payload_json: Some("{}".into()),
            expires_at: None,
        }
    }

    #[test]
    fn create_auto_approves_when_ai_unrestricted_is_active() {
        let db = Database::init(":memory:").expect("init db");
        SystemSettingsService::enable_ai_unrestricted_mode(
            &db,
            EnableAiUnrestrictedInput { minutes: Some(30) },
        )
        .expect("enable ai unrestricted");

        let approval = ApprovalService::create(
            &db,
            approval_input("UPDATE tmp_org_no_list SET org_no = org_no WHERE 1 = 0"),
        )
        .expect("create approval");

        assert_eq!(approval.status, "approved");
        assert_eq!(approval.decided_by, "ai-unrestricted");
        assert!(approval
            .decision_note
            .contains("AI 临时放行已开启，系统自动确认"));
    }

    #[test]
    fn create_keeps_dangerous_blacklist_command_pending() {
        let db = Database::init(":memory:").expect("init db");
        SystemSettingsService::enable_ai_unrestricted_mode(
            &db,
            EnableAiUnrestrictedInput { minutes: Some(30) },
        )
        .expect("enable ai unrestricted");

        let approval =
            ApprovalService::create(&db, approval_input("rm -rf /")).expect("create approval");

        assert_eq!(approval.status, "pending");
        assert!(approval.decided_by.is_empty());
    }
}
