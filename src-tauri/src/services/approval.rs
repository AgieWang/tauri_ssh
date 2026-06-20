use crate::database::Database;
use crate::error::AppError;
use crate::models::{
    ApprovalRequest, CreateApprovalRequestInput, DecideApprovalRequestInput,
    ListApprovalRequestsInput,
};

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
        db.create_approval_request(&input)
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
}
