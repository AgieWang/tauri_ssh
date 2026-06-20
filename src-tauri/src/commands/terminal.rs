use crate::error::CommandError;
use crate::models::{
    CreateAuditLogInput, TerminalCommandInput, TerminalCommandResult, TerminalSessionCloseInput,
    TerminalSessionResizeInput, TerminalSessionStartInput, TerminalSessionStartResult,
    TerminalSessionWriteInput,
};
use crate::services::audit::AuditService;
use crate::services::terminal::TerminalService;
use crate::state::AppState;

#[tauri::command]
pub async fn execute_terminal_command(
    state: tauri::State<'_, AppState>,
    input: TerminalCommandInput,
) -> Result<TerminalCommandResult, CommandError> {
    let audit_input = input.clone();
    match TerminalService::execute(&state.db, input).await {
        Ok(result) => {
            let _ = AuditService::create(
                &state.db,
                CreateAuditLogInput {
                    actor: "local-user".into(),
                    source: "terminal".into(),
                    server_alias: result.server_alias.clone(),
                    action: "terminal_execute".into(),
                    risk: if result.blocked {
                        "blocked"
                    } else {
                        "readonly"
                    }
                    .into(),
                    result: if result.blocked {
                        "已禁止".into()
                    } else if result.exit_status == 0 {
                        "成功".into()
                    } else {
                        "失败".into()
                    },
                    summary: format!("执行终端命令：{}", redact_command_summary(&result.command)),
                    detail_json: Some(
                        serde_json::json!({
                            "serverAlias": result.server_alias,
                            "command": redact_command_summary(&result.command),
                            "exitStatus": result.exit_status,
                            "durationMs": result.duration_ms,
                            "blocked": result.blocked,
                            "stdoutBytes": result.stdout.len(),
                            "stderrBytes": result.stderr.len(),
                            "message": result.message
                        })
                        .to_string(),
                    ),
                    request_id: None,
                    approval_id: None,
                },
            );
            Ok(result)
        }
        Err(error) => {
            let message = error.to_string();
            let _ = AuditService::create(
                &state.db,
                CreateAuditLogInput {
                    actor: "local-user".into(),
                    source: "terminal".into(),
                    server_alias: audit_input.server_alias,
                    action: "terminal_execute".into(),
                    risk: "readonly".into(),
                    result: "失败".into(),
                    summary: format!(
                        "终端命令执行失败：{}",
                        redact_command_summary(&audit_input.command)
                    ),
                    detail_json: Some(
                        serde_json::json!({
                            "command": redact_command_summary(&audit_input.command),
                            "error": message
                        })
                        .to_string(),
                    ),
                    request_id: None,
                    approval_id: None,
                },
            );
            Err(error.into())
        }
    }
}

#[tauri::command]
pub fn start_terminal_session(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    input: TerminalSessionStartInput,
) -> Result<TerminalSessionStartResult, CommandError> {
    let audit_input = input.clone();
    match TerminalService::start_session(&state.db, &state.terminal_sessions, app, input) {
        Ok(result) => {
            let _ = AuditService::create(
                &state.db,
                CreateAuditLogInput {
                    actor: "local-user".into(),
                    source: "terminal".into(),
                    server_alias: audit_input.server_alias,
                    action: "terminal_session_start".into(),
                    risk: "readonly".into(),
                    result: "成功".into(),
                    summary: format!("打开终端会话：{}", result.session_id),
                    detail_json: Some(
                        serde_json::json!({
                            "sessionId": result.session_id,
                            "cols": audit_input.cols,
                            "rows": audit_input.rows
                        })
                        .to_string(),
                    ),
                    request_id: None,
                    approval_id: None,
                },
            );
            Ok(result)
        }
        Err(error) => {
            let message = error.to_string();
            let _ = AuditService::create(
                &state.db,
                CreateAuditLogInput {
                    actor: "local-user".into(),
                    source: "terminal".into(),
                    server_alias: audit_input.server_alias,
                    action: "terminal_session_start".into(),
                    risk: "readonly".into(),
                    result: "失败".into(),
                    summary: "打开终端会话失败".into(),
                    detail_json: Some(serde_json::json!({ "error": message }).to_string()),
                    request_id: None,
                    approval_id: None,
                },
            );
            Err(error.into())
        }
    }
}

#[tauri::command]
pub fn write_terminal_session(
    state: tauri::State<'_, AppState>,
    input: TerminalSessionWriteInput,
) -> Result<(), CommandError> {
    state
        .terminal_sessions
        .write(&input.session_id, input.data)
        .map_err(|e| e.into())
}

#[tauri::command]
pub fn resize_terminal_session(
    state: tauri::State<'_, AppState>,
    input: TerminalSessionResizeInput,
) -> Result<(), CommandError> {
    state
        .terminal_sessions
        .resize(&input.session_id, input.cols, input.rows)
        .map_err(|e| e.into())
}

#[tauri::command]
pub fn close_terminal_session(
    state: tauri::State<'_, AppState>,
    input: TerminalSessionCloseInput,
) -> Result<(), CommandError> {
    let session_id = input.session_id.clone();
    let result = state
        .terminal_sessions
        .close(&input.session_id)
        .map_err(CommandError::from);
    let _ = AuditService::create(
        &state.db,
        CreateAuditLogInput {
            actor: "local-user".into(),
            source: "terminal".into(),
            server_alias: "".into(),
            action: "terminal_session_close".into(),
            risk: "readonly".into(),
            result: if result.is_ok() { "成功" } else { "失败" }.into(),
            summary: format!("关闭终端会话：{}", session_id),
            detail_json: Some(
                serde_json::json!({
                    "sessionId": session_id,
                    "error": result.as_ref().err().map(|error| error.message.clone())
                })
                .to_string(),
            ),
            request_id: None,
            approval_id: None,
        },
    );
    result
}

fn redact_command_summary(command: &str) -> String {
    let mut text = command.trim().to_string();
    for marker in [
        "password=",
        "passwd=",
        "token=",
        "apikey=",
        "api_key=",
        "secret=",
    ] {
        let lower = text.to_lowercase();
        if let Some(index) = lower.find(marker) {
            let end = text[index..]
                .find(char::is_whitespace)
                .map(|offset| index + offset)
                .unwrap_or(text.len());
            text.replace_range(index..end, &format!("{}[REDACTED]", marker));
        }
    }
    if text.chars().count() > 500 {
        let mut truncated = text.chars().take(500).collect::<String>();
        truncated.push_str("...");
        truncated
    } else {
        text
    }
}
