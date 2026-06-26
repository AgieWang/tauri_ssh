use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, Query, State,
    },
    http::{HeaderValue, Method, StatusCode},
    response::IntoResponse,
    routing::{delete, get, post},
    Json, Router,
};
use tower_http::cors::CorsLayer;

use crate::error::{AppError, CommandError};
use crate::models::{
    AiExperienceRecallInput, AiProviderAskInput, AiProviderModelListInput,
    AiSkillPromptPreviewInput, AiSkillTriggerInput, CreateApprovalRequestInput,
    CreateAuditLogInput, DecideApprovalRequestInput, ListAiSkillsInput, ListApprovalRequestsInput,
    RunAiRunbookInput, UpdateSystemSettingsInput, UpsertAiExperienceInput, UpsertAiProviderInput,
    UpsertAiProviderRouteInput, UpsertAiRunbookInput, UpsertAiSkillInput,
    UpsertDatabaseConnectionInput, UpsertJumpServerSessionInput,
};
use crate::services::ai_provider::AiProviderService;
use crate::services::ai_skill::AiSkillService;
use crate::services::approval::ApprovalService;
use crate::services::audit::AuditService;
use crate::services::credential_vault::CredentialVaultService;
use crate::services::database_ops::DatabaseOpsService;
use crate::services::jumpserver::JumpServerService;
use crate::services::mcp::McpService;
use crate::services::sftp::SftpService;
use crate::services::ssh_server::SshServerService;
use crate::services::system_settings::SystemSettingsService;
use crate::services::terminal::{TerminalPtyCommand, TerminalService};
use crate::state::AppState;
use tauri::Manager;

const DEV_API_ADDR: &str = "127.0.0.1:17321";

#[derive(Clone)]
struct DevApiState {
    app_handle: tauri::AppHandle,
}

type DevApiResult<T> = Result<Json<T>, DevApiError>;

struct DevApiError(CommandError);

impl IntoResponse for DevApiError {
    fn into_response(self) -> axum::response::Response {
        let status = match self.0.code.as_str() {
            "NOT_FOUND" => StatusCode::NOT_FOUND,
            "INVALID_INPUT" => StatusCode::BAD_REQUEST,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        (status, Json(self.0)).into_response()
    }
}

impl From<AppError> for DevApiError {
    fn from(error: AppError) -> Self {
        Self(error.into())
    }
}

pub fn start(app_handle: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        if let Err(error) = serve(app_handle).await {
            log::warn!("Dev HTTP API 启动失败: {}", error);
        }
    });
}

async fn serve(app_handle: tauri::AppHandle) -> Result<(), String> {
    let origin = HeaderValue::from_static("http://localhost:1422");
    let cors = CorsLayer::new()
        .allow_origin(origin)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers(tower_http::cors::Any);

    let state = DevApiState { app_handle };
    let app = Router::new()
        .route("/dev-api/health", get(health))
        .route("/mcp", get(mcp_server_info).post(mcp_rpc))
        .route("/dev-api/system-settings", get(get_system_settings))
        .route("/dev-api/system-settings", post(update_system_settings))
        .route(
            "/dev-api/system-settings/reset",
            post(reset_system_settings),
        )
        .route(
            "/dev-api/system-settings/export",
            post(export_system_settings),
        )
        .route("/dev-api/mcp/overview", get(get_mcp_overview))
        .route("/dev-api/mcp/configure", post(configure_mcp_client))
        .route("/dev-api/approvals", get(list_approval_requests))
        .route("/dev-api/approvals", post(create_approval_request))
        .route("/dev-api/approvals/decide", post(decide_approval_request))
        .route("/dev-api/audit-logs", get(list_audit_logs))
        .route("/dev-api/audit-logs", post(create_audit_log))
        .route("/dev-api/audit-logs/export", post(export_audit_logs))
        .route(
            "/dev-api/jumpserver-sessions",
            get(list_jumpserver_sessions),
        )
        .route(
            "/dev-api/jumpserver-sessions",
            post(upsert_jumpserver_session),
        )
        .route(
            "/dev-api/jumpserver-sessions/:key/open",
            post(open_jumpserver_session),
        )
        .route(
            "/dev-api/jumpserver-sessions/:key",
            delete(delete_jumpserver_session),
        )
        .route("/dev-api/ai-providers", get(list_ai_providers))
        .route("/dev-api/ai-providers", post(upsert_ai_provider))
        .route("/dev-api/ai-providers/:key", delete(delete_ai_provider))
        .route("/dev-api/ai-providers/routes", get(list_ai_provider_routes))
        .route(
            "/dev-api/ai-providers/routes",
            post(upsert_ai_provider_route),
        )
        .route("/dev-api/ai-providers/:key/test", post(test_ai_provider))
        .route(
            "/dev-api/ai-providers/models",
            post(list_ai_provider_models),
        )
        .route("/dev-api/ai-providers/ask", post(ask_ai_provider))
        .route("/dev-api/ai-skills/sync", post(sync_builtin_ai_skills))
        .route("/dev-api/ai-skills", post(list_ai_skills))
        .route("/dev-api/ai-skills/upsert", post(upsert_ai_skill))
        .route("/dev-api/ai-skills/:id/enabled", post(set_ai_skill_enabled))
        .route("/dev-api/ai-skills/:id/copy", post(copy_ai_skill))
        .route(
            "/dev-api/ai-skills/:id/restore",
            post(restore_builtin_ai_skill),
        )
        .route("/dev-api/ai-skills/:id", delete(delete_ai_skill))
        .route("/dev-api/ai-skills/trigger", post(test_ai_skill_trigger))
        .route("/dev-api/ai-skills/preview", post(preview_ai_skill_prompt))
        .route("/dev-api/ai-experiences", get(list_ai_experiences))
        .route("/dev-api/ai-experiences", post(upsert_ai_experience))
        .route(
            "/dev-api/ai-experiences/recall",
            post(recall_ai_experiences),
        )
        .route("/dev-api/ai-experiences/:id", delete(delete_ai_experience))
        .route("/dev-api/ai-runbooks", get(list_ai_runbooks))
        .route("/dev-api/ai-runbooks", post(upsert_ai_runbook))
        .route("/dev-api/ai-runbooks/run", post(run_ai_runbook))
        .route("/dev-api/ai-runbooks/:id", delete(delete_ai_runbook))
        .route("/dev-api/ssh-servers", get(list_ssh_servers))
        .route("/dev-api/ssh-servers", post(upsert_ssh_server))
        .route("/dev-api/ssh-servers/import", post(import_ssh_config))
        .route(
            "/dev-api/ssh-servers/test-connection",
            post(test_ssh_server_connection),
        )
        .route("/dev-api/ssh-servers/:alias", delete(delete_ssh_server))
        .route("/dev-api/ssh-servers/:alias/test", post(test_ssh_server))
        .route("/dev-api/credentials", get(list_credentials))
        .route("/dev-api/credentials", post(upsert_credential))
        .route("/dev-api/credentials/authorize", post(authorize_credential))
        .route("/dev-api/credentials/rotate", post(rotate_credential))
        .route("/dev-api/credentials/:key", delete(delete_credential))
        .route(
            "/dev-api/database/connections",
            get(list_database_connections),
        )
        .route(
            "/dev-api/database/connections",
            post(upsert_database_connection),
        )
        .route(
            "/dev-api/database/connections/:key",
            delete(delete_database_connection),
        )
        .route(
            "/dev-api/database/connections/:key/test",
            post(test_database_connection),
        )
        .route(
            "/dev-api/database/query",
            post(execute_database_readonly_query),
        )
        .route("/dev-api/database/names", post(list_database_names))
        .route("/dev-api/database/schema", post(list_database_schema))
        .route("/dev-api/database/sql", post(execute_database_sql))
        .route(
            "/dev-api/database/sql/batch",
            post(execute_database_sql_batch),
        )
        .route("/dev-api/database/export", post(export_database))
        .route("/dev-api/database/redis/scan", post(scan_redis_keys))
        .route(
            "/dev-api/database/redis/describe",
            post(describe_redis_keys),
        )
        .route(
            "/dev-api/database/redis/databases",
            post(list_redis_databases),
        )
        .route(
            "/dev-api/database/redis/key-tree",
            post(list_redis_key_tree),
        )
        .route(
            "/dev-api/database/redis/value",
            post(get_redis_value_preview),
        )
        .route("/dev-api/terminal/execute", post(execute_terminal_command))
        .route("/dev-api/terminal/ws", get(terminal_websocket))
        .route("/dev-api/sftp/list", post(sftp_list))
        .route("/dev-api/sftp/read-text", post(sftp_read_text))
        .route("/dev-api/sftp/write-text", post(sftp_write_text))
        .route("/dev-api/sftp/upload", post(sftp_upload))
        .route("/dev-api/sftp/download", post(sftp_download))
        .route(
            "/dev-api/sftp/create-directory",
            post(sftp_create_directory),
        )
        .route("/dev-api/sftp/create-file", post(sftp_create_file))
        .route("/dev-api/sftp/rename", post(sftp_rename))
        .route("/dev-api/sftp/delete", post(sftp_delete))
        .layer(cors)
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(DEV_API_ADDR)
        .await
        .map_err(|error| error.to_string())?;

    log::info!("Dev HTTP API 已启动: http://{}", DEV_API_ADDR);
    axum::serve(listener, app)
        .await
        .map_err(|error| error.to_string())
}

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "ok": true }))
}

async fn get_system_settings(
    State(ctx): State<DevApiState>,
) -> DevApiResult<crate::models::SystemSettings> {
    let state = app_state(&ctx);
    Ok(Json(SystemSettingsService::get(&state.db)?))
}

async fn update_system_settings(
    State(ctx): State<DevApiState>,
    Json(input): Json<UpdateSystemSettingsInput>,
) -> DevApiResult<crate::models::SystemSettings> {
    let state = app_state(&ctx);
    Ok(Json(SystemSettingsService::update(&state.db, input)?))
}

async fn reset_system_settings(
    State(ctx): State<DevApiState>,
) -> DevApiResult<crate::models::SystemSettings> {
    let state = app_state(&ctx);
    Ok(Json(SystemSettingsService::reset(&state.db)?))
}

async fn export_system_settings(
    State(ctx): State<DevApiState>,
) -> DevApiResult<crate::models::SystemSettingsExportResult> {
    let state = app_state(&ctx);
    Ok(Json(SystemSettingsService::export(&state.db)?))
}

async fn get_mcp_overview() -> DevApiResult<crate::models::McpOverview> {
    Ok(Json(McpService::overview()?))
}

async fn configure_mcp_client(
    Json(input): Json<crate::models::ConfigureMcpClientInput>,
) -> DevApiResult<crate::models::ConfigureMcpClientResult> {
    Ok(Json(McpService::configure(input)?))
}

async fn list_approval_requests(
    State(ctx): State<DevApiState>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> DevApiResult<Vec<crate::models::ApprovalRequest>> {
    let state = app_state(&ctx);
    let input = ListApprovalRequestsInput {
        status: params.get("status").cloned(),
        limit: params
            .get("limit")
            .and_then(|value| value.parse::<i64>().ok()),
    };
    Ok(Json(ApprovalService::list(&state.db, input)?))
}

async fn create_approval_request(
    State(ctx): State<DevApiState>,
    Json(input): Json<CreateApprovalRequestInput>,
) -> DevApiResult<crate::models::ApprovalRequest> {
    let state = app_state(&ctx);
    Ok(Json(ApprovalService::create(&state.db, input)?))
}

async fn decide_approval_request(
    State(ctx): State<DevApiState>,
    Json(input): Json<DecideApprovalRequestInput>,
) -> DevApiResult<crate::models::ApprovalRequest> {
    let state = app_state(&ctx);
    Ok(Json(ApprovalService::decide(&state.db, input)?))
}

async fn list_audit_logs(
    State(ctx): State<DevApiState>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> DevApiResult<Vec<crate::models::AuditLog>> {
    let state = app_state(&ctx);
    let input = crate::models::ListAuditLogsInput {
        actor: params.get("actor").cloned(),
        source: params.get("source").cloned(),
        server_alias: params.get("serverAlias").cloned(),
        action: params.get("action").cloned(),
        risk: params.get("risk").cloned(),
        result: params.get("result").cloned(),
        keyword: params.get("keyword").cloned(),
        limit: params
            .get("limit")
            .and_then(|value| value.parse::<i64>().ok()),
    };
    Ok(Json(AuditService::list(&state.db, input)?))
}

async fn create_audit_log(
    State(ctx): State<DevApiState>,
    Json(input): Json<CreateAuditLogInput>,
) -> DevApiResult<crate::models::AuditLog> {
    let state = app_state(&ctx);
    Ok(Json(AuditService::create(&state.db, input)?))
}

async fn export_audit_logs(
    State(ctx): State<DevApiState>,
    Json(input): Json<crate::models::ListAuditLogsInput>,
) -> DevApiResult<crate::models::AuditLogExportResult> {
    let state = app_state(&ctx);
    Ok(Json(AuditService::export(&state.db, input)?))
}

async fn list_jumpserver_sessions(
    State(ctx): State<DevApiState>,
) -> DevApiResult<Vec<crate::models::JumpServerSession>> {
    let state = app_state(&ctx);
    Ok(Json(JumpServerService::list(&state.db)?))
}

async fn upsert_jumpserver_session(
    State(ctx): State<DevApiState>,
    Json(input): Json<UpsertJumpServerSessionInput>,
) -> DevApiResult<crate::models::JumpServerSession> {
    let state = app_state(&ctx);
    Ok(Json(JumpServerService::upsert(&state.db, input)?))
}

async fn open_jumpserver_session(
    State(ctx): State<DevApiState>,
    Path(key): Path<String>,
) -> DevApiResult<crate::models::JumpServerOpenResult> {
    let state = app_state(&ctx);
    Ok(Json(JumpServerService::open(&state.db, &key)?))
}

async fn delete_jumpserver_session(
    State(ctx): State<DevApiState>,
    Path(key): Path<String>,
) -> Result<StatusCode, DevApiError> {
    let state = app_state(&ctx);
    JumpServerService::delete(&state.db, &key)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn mcp_server_info() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "name": "tauri-ssh",
        "protocol": "mcp",
        "transport": "streamable-http",
        "localOnly": true,
        "methods": ["initialize", "tools/list", "tools/call"]
    }))
}

async fn mcp_rpc(
    State(ctx): State<DevApiState>,
    Json(payload): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    Json(handle_mcp_rpc(&ctx, payload).await)
}

async fn handle_mcp_rpc(ctx: &DevApiState, payload: serde_json::Value) -> serde_json::Value {
    let id = payload
        .get("id")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let method = payload
        .get("method")
        .and_then(|value| value.as_str())
        .unwrap_or_default();
    let result = match method {
        "initialize" => serde_json::json!({
            "protocolVersion": "2025-03-26",
            "capabilities": {
                "tools": {}
            },
            "serverInfo": {
                "name": "tauri-ssh",
                "version": env!("CARGO_PKG_VERSION")
            }
        }),
        "tools/list" => serde_json::json!({ "tools": mcp_tool_schemas() }),
        "tools/call" => {
            let params = payload.get("params").cloned().unwrap_or_default();
            let tool_name = params
                .get("name")
                .and_then(|value| value.as_str())
                .unwrap_or_default();
            let arguments = params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| serde_json::json!({}));
            match call_mcp_tool(ctx, tool_name, arguments).await {
                Ok(value) => serde_json::json!({
                    "content": [
                        {
                            "type": "text",
                            "text": serde_json::to_string_pretty(&value).unwrap_or_else(|_| "{}".into())
                        }
                    ]
                }),
                Err(error) => {
                    return serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "error": {
                            "code": -32000,
                            "message": error.to_string()
                        }
                    });
                }
            }
        }
        _ => {
            return serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": {
                    "code": -32601,
                    "message": format!("Unsupported MCP method: {}", method)
                }
            });
        }
    };
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result
    })
}

fn mcp_tool_schemas() -> Vec<serde_json::Value> {
    vec![
        serde_json::json!({
            "name": "mcp_status",
            "description": "读取 Tauri SSH MCP Server 本地端点状态。",
            "inputSchema": { "type": "object", "properties": {} }
        }),
        serde_json::json!({
            "name": "ssh_servers_list",
            "description": "列出已配置 SSH 服务器的脱敏元数据。",
            "inputSchema": { "type": "object", "properties": {} }
        }),
        serde_json::json!({
            "name": "ssh_server_detail",
            "description": "查询单台 SSH 服务器的脱敏配置详情。",
            "inputSchema": {
                "type": "object",
                "properties": { "alias": { "type": "string" } },
                "required": ["alias"]
            }
        }),
        serde_json::json!({
            "name": "ssh_test_connection",
            "description": "测试指定 SSH 服务器连通性。",
            "inputSchema": {
                "type": "object",
                "properties": { "alias": { "type": "string" } },
                "required": ["alias"]
            }
        }),
        serde_json::json!({
            "name": "terminal_execute_readonly",
            "description": "在指定服务器执行只读命令，拒绝写入和危险命令。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "serverAlias": { "type": "string" },
                    "command": { "type": "string" },
                    "timeoutSecs": { "type": "number" }
                },
                "required": ["serverAlias", "command"]
            }
        }),
        serde_json::json!({
            "name": "sftp_list",
            "description": "列出指定服务器远程目录。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "serverAlias": { "type": "string" },
                    "path": { "type": "string" }
                },
                "required": ["serverAlias", "path"]
            }
        }),
        serde_json::json!({
            "name": "sftp_read_text",
            "description": "读取指定服务器远程 UTF-8 文本文件，默认限制 1MB。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "serverAlias": { "type": "string" },
                    "path": { "type": "string" },
                    "maxBytes": { "type": "number" }
                },
                "required": ["serverAlias", "path"]
            }
        }),
        serde_json::json!({
            "name": "log_tail_snapshot",
            "description": "获取远程日志文件最近 N 行快照。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "serverAlias": { "type": "string" },
                    "path": { "type": "string" },
                    "lineCount": { "type": "number" }
                },
                "required": ["serverAlias", "path"]
            }
        }),
        serde_json::json!({
            "name": "log_search",
            "description": "在远程日志最近 N 行中按关键词搜索。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "serverAlias": { "type": "string" },
                    "path": { "type": "string" },
                    "keyword": { "type": "string" },
                    "lineCount": { "type": "number" },
                    "caseSensitive": { "type": "boolean" }
                },
                "required": ["serverAlias", "path", "keyword"]
            }
        }),
        serde_json::json!({
            "name": "ai_providers_list",
            "description": "列出已配置 AI Provider 状态，不返回 API Key。",
            "inputSchema": { "type": "object", "properties": {} }
        }),
        serde_json::json!({
            "name": "recall_experience",
            "description": "按问题和场景召回 Tauri SSH 本地经验库，返回 Markdown 文件路径、摘要和匹配词。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "prompt": { "type": "string" },
                    "scope": { "type": "string", "enum": ["all", "global", "terminal", "sql", "logs", "sftp", "mcp", "jumpserver"] },
                    "limit": { "type": "number" }
                },
                "required": ["prompt"]
            }
        }),
        serde_json::json!({
            "name": "run_runbook",
            "description": "执行已保存 Runbook。只读步骤自动执行，需审批步骤创建审批请求。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id": { "type": "number" },
                    "runbookKey": { "type": "string" },
                    "serverAlias": { "type": "string" },
                    "databaseConnectionKey": { "type": "string" },
                    "databaseName": { "type": "string" },
                    "requester": { "type": "string" },
                    "dryRun": { "type": "boolean" }
                }
            }
        }),
        serde_json::json!({
            "name": "approval_requests_list",
            "description": "列出本地审批队列，供 Agent 查看 pending 或近期审批状态。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "status": { "type": "string", "enum": ["all", "pending", "approved", "rejected", "cancelled", "expired"] },
                    "limit": { "type": "number" }
                }
            }
        }),
        serde_json::json!({
            "name": "approval_request_create",
            "description": "为需要人工确认的远程命令、SFTP 写入等动作创建本地审批请求。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "source": { "type": "string" },
                    "requester": { "type": "string" },
                    "serverAlias": { "type": "string" },
                    "action": { "type": "string" },
                    "risk": { "type": "string" },
                    "command": { "type": "string" },
                    "resource": { "type": "string" },
                    "reason": { "type": "string" },
                    "summary": { "type": "string" },
                    "payloadJson": { "type": "string" },
                    "expiresAt": { "type": "string" }
                },
                "required": ["source", "action", "risk"]
            }
        }),
        serde_json::json!({
            "name": "ai_policy_evaluate_command",
            "description": "按服务器 AI 权限级别评估命令风险，返回自动执行、需审批或禁止。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "serverAlias": { "type": "string" },
                    "command": { "type": "string" }
                },
                "required": ["serverAlias", "command"]
            }
        }),
        serde_json::json!({
            "name": "terminal_execute_controlled",
            "description": "按服务器 AI 权限级别执行命令：只读自动执行，需审批则创建审批请求，禁止则拒绝。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "serverAlias": { "type": "string" },
                    "command": { "type": "string" },
                    "requester": { "type": "string" },
                    "reason": { "type": "string" },
                    "timeoutSecs": { "type": "number" }
                },
                "required": ["serverAlias", "command"]
            }
        }),
        serde_json::json!({
            "name": "terminal_execute_approved",
            "description": "执行已由本机用户批准的远程命令，approvalId 必须对应 approved 审批请求。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "approvalId": { "type": "number" },
                    "serverAlias": { "type": "string" },
                    "command": { "type": "string" },
                    "timeoutSecs": { "type": "number" }
                },
                "required": ["approvalId", "serverAlias", "command"]
            }
        }),
        serde_json::json!({
            "name": "sftp_write_text_approved",
            "description": "写入已批准的远程文本文件。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "approvalId": { "type": "number" },
                    "serverAlias": { "type": "string" },
                    "path": { "type": "string" },
                    "content": { "type": "string" }
                },
                "required": ["approvalId", "serverAlias", "path", "content"]
            }
        }),
        serde_json::json!({
            "name": "sftp_create_directory_approved",
            "description": "创建已批准的远程目录。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "approvalId": { "type": "number" },
                    "serverAlias": { "type": "string" },
                    "path": { "type": "string" }
                },
                "required": ["approvalId", "serverAlias", "path"]
            }
        }),
        serde_json::json!({
            "name": "sftp_create_file_approved",
            "description": "创建已批准的远程文件。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "approvalId": { "type": "number" },
                    "serverAlias": { "type": "string" },
                    "path": { "type": "string" },
                    "content": { "type": "string" }
                },
                "required": ["approvalId", "serverAlias", "path"]
            }
        }),
        serde_json::json!({
            "name": "sftp_rename_approved",
            "description": "重命名已批准的远程路径。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "approvalId": { "type": "number" },
                    "serverAlias": { "type": "string" },
                    "fromPath": { "type": "string" },
                    "toPath": { "type": "string" }
                },
                "required": ["approvalId", "serverAlias", "fromPath", "toPath"]
            }
        }),
        serde_json::json!({
            "name": "sftp_delete_approved",
            "description": "删除已批准的远程文件或空目录。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "approvalId": { "type": "number" },
                    "serverAlias": { "type": "string" },
                    "path": { "type": "string" },
                    "fileType": { "type": "string", "enum": ["file", "directory"] }
                },
                "required": ["approvalId", "serverAlias", "path", "fileType"]
            }
        }),
        serde_json::json!({
            "name": "server_groups_list",
            "description": "列出服务器分组及数量统计。",
            "inputSchema": { "type": "object", "properties": {} }
        }),
        serde_json::json!({
            "name": "server_group_inventory",
            "description": "读取指定分组下的服务器连接元数据，不返回凭据明文。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "groupName": { "type": "string" },
                    "includeDisabled": { "type": "boolean" }
                },
                "required": ["groupName"]
            }
        }),
        serde_json::json!({
            "name": "ssh_connection_profile",
            "description": "读取单台服务器 SSH 连接资料，不返回密码、私钥或 token 明文。",
            "inputSchema": {
                "type": "object",
                "properties": { "serverAlias": { "type": "string" } },
                "required": ["serverAlias"]
            }
        }),
        serde_json::json!({
            "name": "ssh_connection_profiles",
            "description": "批量读取 SSH 连接资料，可按分组过滤，不返回凭据明文。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "groupName": { "type": "string" },
                    "includeDisabled": { "type": "boolean" },
                    "limit": { "type": "number" }
                }
            }
        }),
        serde_json::json!({
            "name": "ssh_command_generate",
            "description": "为指定服务器生成可复制的 ssh 命令模板；密码类认证不会嵌入密码。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "serverAlias": { "type": "string" },
                    "command": { "type": "string" }
                },
                "required": ["serverAlias"]
            }
        }),
        serde_json::json!({
            "name": "openssh_config_generate",
            "description": "生成 OpenSSH Config 片段，便于 Agent 或用户复用 ssh/scp/sftp。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "serverAlias": { "type": "string" },
                    "groupName": { "type": "string" },
                    "includeDisabled": { "type": "boolean" }
                }
            }
        }),
        serde_json::json!({
            "name": "credential_access_request_create",
            "description": "为凭据访问创建审批请求；不会返回凭据明文。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "serverAlias": { "type": "string" },
                    "credentialKey": { "type": "string" },
                    "requester": { "type": "string" },
                    "reason": { "type": "string" }
                },
                "required": ["serverAlias", "reason"]
            }
        }),
        serde_json::json!({
            "name": "credential_access_status",
            "description": "查看凭据访问审批状态，只返回状态和脱敏引用，不返回凭据明文。",
            "inputSchema": {
                "type": "object",
                "properties": { "approvalId": { "type": "number" } },
                "required": ["approvalId"]
            }
        }),
    ]
}

async fn call_mcp_tool(
    ctx: &DevApiState,
    tool_name: &str,
    arguments: serde_json::Value,
) -> Result<serde_json::Value, AppError> {
    let started = std::time::Instant::now();
    let audit_arguments = arguments.clone();
    let result = call_mcp_tool_inner(ctx, tool_name, arguments).await;
    audit_mcp_tool_call(
        ctx,
        tool_name,
        &audit_arguments,
        started.elapsed().as_millis() as i64,
        &result,
    );
    result
}

async fn call_mcp_tool_inner(
    ctx: &DevApiState,
    tool_name: &str,
    arguments: serde_json::Value,
) -> Result<serde_json::Value, AppError> {
    match tool_name {
        "mcp_status" => Ok(serde_json::to_value(McpService::status())?),
        "ssh_servers_list" => {
            let state = app_state(ctx);
            let servers = SshServerService::list(&state.db)?
                .into_iter()
                .map(|server| {
                    serde_json::json!({
                        "alias": server.alias,
                        "groupName": server.group_name,
                        "host": server.host,
                        "port": server.port,
                        "username": server.username,
                        "aiPolicy": server.ai_policy,
                        "status": server.status,
                        "enabled": server.enabled
                    })
                })
                .collect::<Vec<_>>();
            Ok(serde_json::json!({ "servers": servers }))
        }
        "ssh_server_detail" => {
            let alias = required_string(&arguments, "alias")?;
            let state = app_state(ctx);
            let server = SshServerService::list(&state.db)?
                .into_iter()
                .find(|item| item.alias == alias)
                .ok_or_else(|| AppError::NotFound(format!("服务器 '{}' 不存在", alias)))?;
            Ok(serde_json::json!({
                "alias": server.alias,
                "groupName": server.group_name,
                "host": server.host,
                "port": server.port,
                "username": server.username,
                "source": server.source,
                "authType": server.auth_type,
                "authRef": server.auth_ref,
                "identityFile": server.identity_file,
                "hasPassword": server.has_password,
                "proxyJump": server.proxy_jump,
                "aiPolicy": server.ai_policy,
                "status": server.status,
                "enabled": server.enabled,
                "lastConnectedAt": server.last_connected_at,
                "updatedAt": server.updated_at
            }))
        }
        "ssh_test_connection" => {
            let alias = required_string(&arguments, "alias")?;
            let state = app_state(ctx);
            Ok(serde_json::to_value(
                SshServerService::test(&state.db, &alias).await?,
            )?)
        }
        "terminal_execute_readonly" => {
            let server_alias = required_string(&arguments, "serverAlias")?;
            let command = required_string(&arguments, "command")?;
            validate_readonly_command(&command)?;
            let timeout_secs = optional_u64(&arguments, "timeoutSecs").or(Some(30));
            let state = app_state(ctx);
            let result = TerminalService::execute(
                &state.db,
                crate::models::TerminalCommandInput {
                    server_alias,
                    command,
                    timeout_secs,
                },
            )
            .await?;
            Ok(serde_json::to_value(result)?)
        }
        "sftp_list" => {
            let state = app_state(ctx);
            let result = SftpService::list(
                &state.db,
                crate::models::SftpListInput {
                    server_alias: required_string(&arguments, "serverAlias")?,
                    path: required_string(&arguments, "path")?,
                },
            )?;
            Ok(serde_json::to_value(result)?)
        }
        "sftp_read_text" => {
            let state = app_state(ctx);
            let result = SftpService::read_text(
                &state.db,
                crate::models::SftpReadTextInput {
                    server_alias: required_string(&arguments, "serverAlias")?,
                    path: required_string(&arguments, "path")?,
                    max_bytes: optional_u64(&arguments, "maxBytes"),
                },
            )?;
            Ok(serde_json::to_value(result)?)
        }
        "log_tail_snapshot" => {
            let server_alias = required_string(&arguments, "serverAlias")?;
            let path = required_string(&arguments, "path")?;
            let line_count = optional_u64(&arguments, "lineCount")
                .unwrap_or(200)
                .clamp(20, 5000);
            let command = format!("tail -n {} {}", line_count, shell_quote(&path));
            let state = app_state(ctx);
            let result = TerminalService::execute(
                &state.db,
                crate::models::TerminalCommandInput {
                    server_alias,
                    command,
                    timeout_secs: Some(20),
                },
            )
            .await?;
            Ok(serde_json::json!({
                "path": path,
                "lineCount": line_count,
                "exitStatus": result.exit_status,
                "stdout": result.stdout,
                "stderr": result.stderr,
                "durationMs": result.duration_ms
            }))
        }
        "log_search" => {
            let server_alias = required_string(&arguments, "serverAlias")?;
            let path = required_string(&arguments, "path")?;
            let keyword = required_string(&arguments, "keyword")?;
            let line_count = optional_u64(&arguments, "lineCount")
                .unwrap_or(500)
                .clamp(20, 5000);
            let case_sensitive = optional_bool(&arguments, "caseSensitive").unwrap_or(false);
            let command = format!("tail -n {} {}", line_count, shell_quote(&path));
            let state = app_state(ctx);
            let result = TerminalService::execute(
                &state.db,
                crate::models::TerminalCommandInput {
                    server_alias,
                    command,
                    timeout_secs: Some(20),
                },
            )
            .await?;
            let needle = if case_sensitive {
                keyword.clone()
            } else {
                keyword.to_lowercase()
            };
            let matches = result
                .stdout
                .lines()
                .filter(|line| {
                    let haystack = if case_sensitive {
                        (*line).to_string()
                    } else {
                        line.to_lowercase()
                    };
                    haystack.contains(&needle)
                })
                .map(|line| line.to_string())
                .collect::<Vec<_>>();
            Ok(serde_json::json!({
                "path": path,
                "keyword": keyword,
                "lineCount": line_count,
                "caseSensitive": case_sensitive,
                "matchCount": matches.len(),
                "matches": matches,
                "stderr": result.stderr,
                "exitStatus": result.exit_status
            }))
        }
        "ai_providers_list" => {
            let state = app_state(ctx);
            let providers = AiProviderService::list(&state.db)?
                .into_iter()
                .map(|provider| {
                    serde_json::json!({
                        "key": provider.key,
                        "name": provider.name,
                        "defaultModel": provider.default_model,
                        "status": provider.status,
                        "enabled": provider.enabled,
                        "hasApiKey": provider.has_api_key
                    })
                })
                .collect::<Vec<_>>();
            Ok(serde_json::json!({ "providers": providers }))
        }
        "recall_experience" => {
            let state = app_state(ctx);
            let matches = AiSkillService::recall_experiences(
                &state.db,
                AiExperienceRecallInput {
                    prompt: required_string(&arguments, "prompt")?,
                    scope: optional_string(&arguments, "scope"),
                    limit: optional_i64(&arguments, "limit"),
                },
            )?;
            let items = matches
                .into_iter()
                .map(|item| {
                    serde_json::json!({
                        "title": item.experience.title,
                        "experienceKey": item.experience.experience_key,
                        "scenario": item.experience.scenario,
                        "tags": item.experience.tags,
                        "matchedWords": item.matched_words,
                        "score": item.score,
                        "summary": item.summary,
                        "markdownPath": item.experience.markdown_path,
                        "updatedAt": item.experience.updated_at
                    })
                })
                .collect::<Vec<_>>();
            let count = items.len();
            Ok(serde_json::json!({
                "matches": items,
                "count": count
            }))
        }
        "run_runbook" => {
            let state = app_state(ctx);
            let input = RunAiRunbookInput {
                id: optional_i64(&arguments, "id"),
                runbook_key: optional_string(&arguments, "runbookKey"),
                server_alias: optional_string(&arguments, "serverAlias"),
                database_connection_key: optional_string(&arguments, "databaseConnectionKey"),
                database_name: optional_string(&arguments, "databaseName"),
                requester: optional_string(&arguments, "requester")
                    .or_else(|| Some("mcp-client".into())),
                dry_run: optional_bool(&arguments, "dryRun"),
            };
            let runbook = AiSkillService::resolve_runbook(&state.db, &input)?;
            if !runbook.allow_mcp {
                return Err(AppError::InvalidInput(format!(
                    "Runbook '{}' 未开启 MCP 调用",
                    runbook.name
                )));
            }
            Ok(serde_json::to_value(
                AiSkillService::run_runbook(&state.db, input).await?,
            )?)
        }
        "approval_requests_list" => {
            let state = app_state(ctx);
            let input = ListApprovalRequestsInput {
                status: optional_string(&arguments, "status"),
                limit: optional_i64(&arguments, "limit"),
            };
            Ok(serde_json::to_value(ApprovalService::list(
                &state.db, input,
            )?)?)
        }
        "approval_request_create" => {
            let state = app_state(ctx);
            let input = CreateApprovalRequestInput {
                source: required_string(&arguments, "source")?,
                requester: optional_string(&arguments, "requester")
                    .unwrap_or_else(|| "mcp-client".into()),
                server_alias: optional_string(&arguments, "serverAlias").unwrap_or_default(),
                action: required_string(&arguments, "action")?,
                risk: required_string(&arguments, "risk")?,
                command: optional_string(&arguments, "command").unwrap_or_default(),
                resource: optional_string(&arguments, "resource").unwrap_or_default(),
                reason: optional_string(&arguments, "reason").unwrap_or_default(),
                summary: optional_string(&arguments, "summary").unwrap_or_default(),
                payload_json: optional_string(&arguments, "payloadJson"),
                expires_at: optional_string(&arguments, "expiresAt"),
            };
            Ok(serde_json::to_value(ApprovalService::create(
                &state.db, input,
            )?)?)
        }
        "ai_policy_evaluate_command" => {
            let state = app_state(ctx);
            let server_alias = required_string(&arguments, "serverAlias")?;
            let command = required_string(&arguments, "command")?;
            let evaluation = evaluate_command_policy(&state.db, &server_alias, &command)?;
            Ok(evaluation.to_json())
        }
        "terminal_execute_controlled" => {
            let state = app_state(ctx);
            let server_alias = required_string(&arguments, "serverAlias")?;
            let command = required_string(&arguments, "command")?;
            let timeout_secs = optional_u64(&arguments, "timeoutSecs").or(Some(30));
            let requester =
                optional_string(&arguments, "requester").unwrap_or_else(|| "mcp-client".into());
            let reason = optional_string(&arguments, "reason")
                .unwrap_or_else(|| "MCP Agent 请求执行远程命令".into());
            let evaluation = evaluate_command_policy(&state.db, &server_alias, &command)?;
            match evaluation.action.as_str() {
                "auto" => {
                    let result = TerminalService::execute(
                        &state.db,
                        crate::models::TerminalCommandInput {
                            server_alias,
                            command,
                            timeout_secs,
                        },
                    )
                    .await?;
                    Ok(serde_json::json!({
                        "action": "executed",
                        "evaluation": evaluation.to_json(),
                        "result": result
                    }))
                }
                "review" => {
                    let approval = ApprovalService::create(
                        &state.db,
                        CreateApprovalRequestInput {
                            source: "mcp".into(),
                            requester,
                            server_alias: server_alias.clone(),
                            action: "terminal_execute".into(),
                            risk: evaluation.risk.clone(),
                            command: command.clone(),
                            resource: String::new(),
                            reason,
                            summary: format!("执行远程命令：{}", command),
                            payload_json: Some(
                                serde_json::json!({
                                    "tool": "terminal_execute_controlled",
                                    "serverAlias": server_alias,
                                    "command": command,
                                    "timeoutSecs": timeout_secs
                                })
                                .to_string(),
                            ),
                            expires_at: None,
                        },
                    )?;
                    Ok(serde_json::json!({
                        "action": "approval_required",
                        "evaluation": evaluation.to_json(),
                        "approval": approval
                    }))
                }
                _ => Ok(serde_json::json!({
                    "action": "blocked",
                    "evaluation": evaluation.to_json(),
                    "message": evaluation.reason
                })),
            }
        }
        "terminal_execute_approved" => {
            let state = app_state(ctx);
            let approval_id = required_i64(&arguments, "approvalId")?;
            let server_alias = required_string(&arguments, "serverAlias")?;
            let command = required_string(&arguments, "command")?;
            require_approved_request(
                &state.db,
                approval_id,
                "terminal_execute",
                &server_alias,
                Some(&command),
                None,
            )?;
            let result = TerminalService::execute(
                &state.db,
                crate::models::TerminalCommandInput {
                    server_alias,
                    command,
                    timeout_secs: optional_u64(&arguments, "timeoutSecs").or(Some(30)),
                },
            )
            .await?;
            Ok(serde_json::to_value(result)?)
        }
        "sftp_write_text_approved" => {
            let state = app_state(ctx);
            let approval_id = required_i64(&arguments, "approvalId")?;
            let server_alias = required_string(&arguments, "serverAlias")?;
            let path = required_string(&arguments, "path")?;
            require_approved_request(
                &state.db,
                approval_id,
                "sftp_write_text",
                &server_alias,
                None,
                Some(&path),
            )?;
            let result = SftpService::write_text(
                &state.db,
                crate::models::SftpWriteTextInput {
                    server_alias,
                    path,
                    content: required_string(&arguments, "content")?,
                },
            )?;
            Ok(serde_json::to_value(result)?)
        }
        "sftp_create_directory_approved" => {
            let state = app_state(ctx);
            let approval_id = required_i64(&arguments, "approvalId")?;
            let server_alias = required_string(&arguments, "serverAlias")?;
            let path = required_string(&arguments, "path")?;
            require_approved_request(
                &state.db,
                approval_id,
                "sftp_create_directory",
                &server_alias,
                None,
                Some(&path),
            )?;
            let result = SftpService::create_directory(
                &state.db,
                crate::models::SftpCreateDirectoryInput { server_alias, path },
            )?;
            Ok(serde_json::to_value(result)?)
        }
        "sftp_create_file_approved" => {
            let state = app_state(ctx);
            let approval_id = required_i64(&arguments, "approvalId")?;
            let server_alias = required_string(&arguments, "serverAlias")?;
            let path = required_string(&arguments, "path")?;
            require_approved_request(
                &state.db,
                approval_id,
                "sftp_create_file",
                &server_alias,
                None,
                Some(&path),
            )?;
            let result = SftpService::create_file(
                &state.db,
                crate::models::SftpCreateFileInput {
                    server_alias,
                    path,
                    content: optional_string(&arguments, "content"),
                },
            )?;
            Ok(serde_json::to_value(result)?)
        }
        "sftp_rename_approved" => {
            let state = app_state(ctx);
            let approval_id = required_i64(&arguments, "approvalId")?;
            let server_alias = required_string(&arguments, "serverAlias")?;
            let from_path = required_string(&arguments, "fromPath")?;
            let to_path = required_string(&arguments, "toPath")?;
            require_approved_request(
                &state.db,
                approval_id,
                "sftp_rename",
                &server_alias,
                None,
                Some(&from_path),
            )?;
            let result = SftpService::rename(
                &state.db,
                crate::models::SftpRenameInput {
                    server_alias,
                    from_path,
                    to_path,
                },
            )?;
            Ok(serde_json::to_value(result)?)
        }
        "sftp_delete_approved" => {
            let state = app_state(ctx);
            let approval_id = required_i64(&arguments, "approvalId")?;
            let server_alias = required_string(&arguments, "serverAlias")?;
            let path = required_string(&arguments, "path")?;
            require_approved_request(
                &state.db,
                approval_id,
                "sftp_delete",
                &server_alias,
                None,
                Some(&path),
            )?;
            let result = SftpService::delete(
                &state.db,
                crate::models::SftpDeleteInput {
                    server_alias,
                    path,
                    file_type: required_string(&arguments, "fileType")?,
                },
            )?;
            Ok(serde_json::to_value(result)?)
        }
        "server_groups_list" => {
            let state = app_state(ctx);
            let mut groups = std::collections::BTreeMap::<String, (usize, usize)>::new();
            for server in SshServerService::list(&state.db)? {
                let entry = groups.entry(server.group_name).or_insert((0, 0));
                entry.0 += 1;
                if server.enabled {
                    entry.1 += 1;
                }
            }
            Ok(serde_json::json!({
                "groups": groups.into_iter().map(|(group_name, (total, enabled))| {
                    serde_json::json!({
                        "groupName": group_name,
                        "total": total,
                        "enabled": enabled,
                        "disabled": total.saturating_sub(enabled)
                    })
                }).collect::<Vec<_>>()
            }))
        }
        "server_group_inventory" => {
            let state = app_state(ctx);
            let group_name = required_string(&arguments, "groupName")?;
            let include_disabled = optional_bool(&arguments, "includeDisabled").unwrap_or(false);
            let servers = SshServerService::list(&state.db)?
                .into_iter()
                .filter(|server| server.group_name == group_name)
                .filter(|server| include_disabled || server.enabled)
                .map(|server| connection_profile_json(&server))
                .collect::<Vec<_>>();
            Ok(serde_json::json!({
                "groupName": group_name,
                "count": servers.len(),
                "servers": servers
            }))
        }
        "ssh_connection_profile" => {
            let state = app_state(ctx);
            let server = find_server(&state.db, &required_string(&arguments, "serverAlias")?)?;
            Ok(connection_profile_json(&server))
        }
        "ssh_connection_profiles" => {
            let state = app_state(ctx);
            let group_name = optional_string(&arguments, "groupName");
            let include_disabled = optional_bool(&arguments, "includeDisabled").unwrap_or(false);
            let limit = optional_i64(&arguments, "limit")
                .unwrap_or(100)
                .clamp(1, 500) as usize;
            let profiles = SshServerService::list(&state.db)?
                .into_iter()
                .filter(|server| include_disabled || server.enabled)
                .filter(|server| {
                    group_name
                        .as_deref()
                        .map(|value| server.group_name == value)
                        .unwrap_or(true)
                })
                .take(limit)
                .map(|server| connection_profile_json(&server))
                .collect::<Vec<_>>();
            Ok(serde_json::json!({ "profiles": profiles }))
        }
        "ssh_command_generate" => {
            let state = app_state(ctx);
            let server = find_server(&state.db, &required_string(&arguments, "serverAlias")?)?;
            let remote_command = optional_string(&arguments, "command");
            Ok(serde_json::json!({
                "serverAlias": server.alias,
                "command": build_ssh_command(&server, remote_command.as_deref()),
                "credentialMode": credential_mode_note(&server),
                "notes": [
                    "不会在命令中嵌入密码或 token。",
                    "密码类服务器建议通过 terminal_execute_controlled / terminal_execute_approved 由应用内执行。",
                    "key 认证仅引用本机 IdentityFile 路径，不读取私钥内容。"
                ]
            }))
        }
        "openssh_config_generate" => {
            let state = app_state(ctx);
            let server_alias = optional_string(&arguments, "serverAlias");
            let group_name = optional_string(&arguments, "groupName");
            let include_disabled = optional_bool(&arguments, "includeDisabled").unwrap_or(false);
            let servers = SshServerService::list(&state.db)?
                .into_iter()
                .filter(|server| include_disabled || server.enabled)
                .filter(|server| {
                    server_alias
                        .as_deref()
                        .map(|value| server.alias == value)
                        .unwrap_or(true)
                })
                .filter(|server| {
                    group_name
                        .as_deref()
                        .map(|value| server.group_name == value)
                        .unwrap_or(true)
                })
                .collect::<Vec<_>>();
            if servers.is_empty() {
                return Err(AppError::NotFound("没有匹配的服务器".into()));
            }
            let config = servers
                .iter()
                .map(openssh_config_block)
                .collect::<Vec<_>>()
                .join("\n\n");
            Ok(serde_json::json!({
                "count": servers.len(),
                "config": config,
                "notes": [
                    "片段不包含密码、token 或私钥内容。",
                    "IdentityFile 只在服务器配置中已有 key 路径时输出。",
                    "密码类认证需要用户本机 SSH 客户端交互输入，或通过应用内审批执行。"
                ]
            }))
        }
        "credential_access_request_create" => {
            let state = app_state(ctx);
            let server_alias = required_string(&arguments, "serverAlias")?;
            let reason = required_string(&arguments, "reason")?;
            let server = find_server(&state.db, &server_alias)?;
            let credential_key = optional_string(&arguments, "credentialKey")
                .or_else(|| credential_reference_key(&server))
                .unwrap_or_else(|| server.alias.clone());
            let approval = ApprovalService::create(
                &state.db,
                CreateApprovalRequestInput {
                    source: "mcp".into(),
                    requester: optional_string(&arguments, "requester")
                        .unwrap_or_else(|| "mcp-client".into()),
                    server_alias: server.alias.clone(),
                    action: "credential_access".into(),
                    risk: "L3".into(),
                    command: String::new(),
                    resource: credential_key.clone(),
                    reason,
                    summary: format!("申请访问服务器 {} 的凭据引用", server.alias),
                    payload_json: Some(
                        serde_json::json!({
                            "tool": "credential_access_request_create",
                            "serverAlias": server.alias,
                            "credentialKey": credential_key,
                            "authType": server.auth_type
                        })
                        .to_string(),
                    ),
                    expires_at: None,
                },
            )?;
            Ok(serde_json::json!({
                "approval": approval,
                "credentialDisclosure": "pending_approval",
                "message": "已创建凭据访问审批请求；MCP 不会直接返回凭据明文。"
            }))
        }
        "credential_access_status" => {
            let state = app_state(ctx);
            let approval_id = required_i64(&arguments, "approvalId")?;
            let approval = state
                .db
                .get_approval_request(approval_id)?
                .ok_or_else(|| AppError::NotFound(format!("审批请求 '{}' 不存在", approval_id)))?;
            if approval.action != "credential_access" {
                return Err(AppError::InvalidInput("该审批不是凭据访问请求".into()));
            }
            Ok(serde_json::json!({
                "approvalId": approval.id,
                "status": approval.status,
                "serverAlias": approval.server_alias,
                "credentialRef": approval.resource,
                "approved": approval.status == "approved",
                "credentialDisclosure": "never_return_plaintext",
                "message": if approval.status == "approved" {
                    "审批已通过。出于安全边界，MCP 仍不返回凭据明文；请使用应用内执行工具或本机已配置的 SSH 凭据。"
                } else {
                    "审批尚未通过，不能访问凭据。"
                }
            }))
        }
        _ => Err(AppError::InvalidInput(format!(
            "不支持的 MCP 工具：{}",
            tool_name
        ))),
    }
}

fn audit_mcp_tool_call(
    ctx: &DevApiState,
    tool_name: &str,
    arguments: &serde_json::Value,
    duration_ms: i64,
    result: &Result<serde_json::Value, AppError>,
) {
    let state = app_state(ctx);
    let success = result.is_ok();
    let server_alias = optional_string(arguments, "serverAlias")
        .or_else(|| optional_string(arguments, "alias"))
        .unwrap_or_default();
    let risk = mcp_tool_risk(tool_name);
    let detail = serde_json::json!({
        "tool": tool_name,
        "serverAlias": server_alias,
        "durationMs": duration_ms,
        "arguments": sanitize_mcp_arguments(arguments),
        "outcome": mcp_result_summary(result)
    });
    let _ = AuditService::create(
        &state.db,
        CreateAuditLogInput {
            actor: "mcp-client".into(),
            source: "mcp".into(),
            server_alias,
            action: format!("mcp_tool:{}", tool_name),
            risk: risk.into(),
            result: if success { "成功" } else { "失败" }.into(),
            summary: if success {
                format!("MCP 工具调用成功：{}", tool_name)
            } else {
                format!("MCP 工具调用失败：{}", tool_name)
            },
            detail_json: Some(detail.to_string()),
            request_id: None,
            approval_id: approval_id_from_result(result),
        },
    );
}

fn mcp_tool_risk(tool_name: &str) -> &'static str {
    if tool_name.contains("delete")
        || tool_name.contains("write")
        || tool_name.contains("rename")
        || tool_name.contains("credential_access")
        || tool_name == "terminal_execute_approved"
    {
        "L3"
    } else if tool_name.contains("controlled")
        || tool_name.contains("approval_request_create")
        || tool_name == "run_runbook"
        || tool_name.contains("create_file")
        || tool_name.contains("create_directory")
    {
        "L2"
    } else {
        "readonly"
    }
}

fn sanitize_mcp_arguments(arguments: &serde_json::Value) -> serde_json::Value {
    let Some(map) = arguments.as_object() else {
        return serde_json::json!({});
    };
    let sanitized = map
        .iter()
        .map(|(key, value)| {
            let lower = key.to_lowercase();
            let value = if lower.contains("password")
                || lower.contains("secret")
                || lower.contains("token")
                || lower.contains("apikey")
                || lower.contains("api_key")
                || lower == "content"
            {
                serde_json::json!("[REDACTED]")
            } else if lower == "command" {
                serde_json::json!(redact_audit_text(value.as_str().unwrap_or_default(), 500))
            } else if value.is_string() {
                serde_json::json!(redact_audit_text(value.as_str().unwrap_or_default(), 500))
            } else {
                value.clone()
            };
            (key.clone(), value)
        })
        .collect::<serde_json::Map<_, _>>();
    serde_json::Value::Object(sanitized)
}

fn mcp_result_summary(result: &Result<serde_json::Value, AppError>) -> serde_json::Value {
    match result {
        Ok(value) => serde_json::json!({
            "ok": true,
            "action": value.get("action").and_then(|item| item.as_str()),
            "exitStatus": value.get("exitStatus").and_then(|item| item.as_i64()),
            "approvalId": approval_id_from_value(value),
            "keys": value.as_object().map(|map| map.keys().cloned().collect::<Vec<_>>()).unwrap_or_default()
        }),
        Err(error) => serde_json::json!({
            "ok": false,
            "error": error.to_string()
        }),
    }
}

fn approval_id_from_result(result: &Result<serde_json::Value, AppError>) -> Option<i64> {
    result.as_ref().ok().and_then(approval_id_from_value)
}

fn approval_id_from_value(value: &serde_json::Value) -> Option<i64> {
    value
        .get("approval")
        .and_then(|approval| approval.get("id"))
        .and_then(|id| id.as_i64())
        .or_else(|| value.get("approvalId").and_then(|id| id.as_i64()))
}

fn redact_audit_text(value: &str, max_chars: usize) -> String {
    let mut text = value.trim().to_string();
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
    if text.chars().count() > max_chars {
        let mut truncated = text.chars().take(max_chars).collect::<String>();
        truncated.push_str("...");
        truncated
    } else {
        text
    }
}

fn required_string(arguments: &serde_json::Value, key: &str) -> Result<String, AppError> {
    arguments
        .get(key)
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| AppError::InvalidInput(format!("参数 '{}' 不能为空", key)))
}

fn required_i64(arguments: &serde_json::Value, key: &str) -> Result<i64, AppError> {
    optional_i64(arguments, key)
        .filter(|value| *value > 0)
        .ok_or_else(|| AppError::InvalidInput(format!("参数 '{}' 必须是正整数", key)))
}

fn optional_u64(arguments: &serde_json::Value, key: &str) -> Option<u64> {
    arguments.get(key).and_then(|value| {
        value
            .as_u64()
            .or_else(|| value.as_str().and_then(|text| text.parse::<u64>().ok()))
    })
}

fn optional_i64(arguments: &serde_json::Value, key: &str) -> Option<i64> {
    arguments.get(key).and_then(|value| {
        value
            .as_i64()
            .or_else(|| value.as_str().and_then(|text| text.parse::<i64>().ok()))
    })
}

fn optional_string(arguments: &serde_json::Value, key: &str) -> Option<String> {
    arguments
        .get(key)
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn optional_bool(arguments: &serde_json::Value, key: &str) -> Option<bool> {
    arguments.get(key).and_then(|value| {
        value
            .as_bool()
            .or_else(|| value.as_str().and_then(|text| text.parse::<bool>().ok()))
    })
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn find_server(
    db: &crate::database::Database,
    server_alias: &str,
) -> Result<crate::models::SshServer, AppError> {
    db.get_ssh_server(server_alias)?
        .ok_or_else(|| AppError::NotFound(format!("服务器 '{}' 不存在", server_alias)))
}

fn connection_profile_json(server: &crate::models::SshServer) -> serde_json::Value {
    serde_json::json!({
        "alias": server.alias,
        "groupName": server.group_name,
        "host": server.host,
        "port": server.port,
        "username": server.username,
        "source": server.source,
        "authType": server.auth_type,
        "authRef": safe_auth_ref(server),
        "identityFile": safe_identity_file(server),
        "proxyJump": server.proxy_jump,
        "aiPolicy": server.ai_policy,
        "status": server.status,
        "enabled": server.enabled,
        "lastConnectedAt": server.last_connected_at,
        "credentialMode": credential_mode_note(server),
        "credentialDisclosure": "redacted"
    })
}

fn safe_auth_ref(server: &crate::models::SshServer) -> String {
    match server.auth_type.as_str() {
        "key" => server.auth_ref.clone(),
        "password_ref" => server.auth_ref.clone(),
        "direct_password" if server.has_password => "stored:direct_password".into(),
        _ => String::new(),
    }
}

fn safe_identity_file(server: &crate::models::SshServer) -> String {
    if server.auth_type == "key" {
        server.identity_file.clone()
    } else {
        String::new()
    }
}

fn credential_reference_key(server: &crate::models::SshServer) -> Option<String> {
    if server.auth_type == "password_ref" {
        let key = server
            .auth_ref
            .strip_prefix("vault:")
            .unwrap_or(server.auth_ref.as_str())
            .trim();
        if !key.is_empty() {
            return Some(key.to_string());
        }
    }
    None
}

fn credential_mode_note(server: &crate::models::SshServer) -> String {
    match server.auth_type.as_str() {
        "key" => {
            if safe_identity_file(server).is_empty() {
                "key_reference_without_identity_file".into()
            } else {
                "identity_file_reference_only".into()
            }
        }
        "password_ref" => "vault_password_reference_not_disclosed".into(),
        "direct_password" => "stored_password_not_disclosed".into(),
        "session_reference" => "jumpserver_session_reference_not_direct_ssh".into(),
        _ => "unsupported_or_unknown_auth".into(),
    }
}

fn build_ssh_command(server: &crate::models::SshServer, remote_command: Option<&str>) -> String {
    let mut parts = vec!["ssh".to_string()];
    if server.port != 22 {
        parts.push("-p".into());
        parts.push(server.port.to_string());
    }
    if !server.proxy_jump.trim().is_empty() {
        parts.push("-J".into());
        parts.push(shell_arg(server.proxy_jump.trim()));
    }
    let identity_file = safe_identity_file(server);
    if !identity_file.trim().is_empty() {
        parts.push("-i".into());
        parts.push(shell_arg(identity_file.trim()));
    }
    parts.push(format!(
        "{}@{}",
        shell_arg(&server.username),
        shell_arg(&server.host)
    ));
    if let Some(command) = remote_command
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        parts.push(shell_quote(command));
    }
    parts.join(" ")
}

fn openssh_config_block(server: &crate::models::SshServer) -> String {
    let mut lines = vec![
        format!("Host {}", sanitize_host_alias(&server.alias)),
        format!("  HostName {}", server.host),
        format!("  Port {}", server.port),
        format!("  User {}", server.username),
    ];
    let identity_file = safe_identity_file(server);
    if !identity_file.trim().is_empty() {
        lines.push(format!("  IdentityFile {}", identity_file));
    }
    if !server.proxy_jump.trim().is_empty() {
        lines.push(format!("  ProxyJump {}", server.proxy_jump.trim()));
    }
    lines.push("  # Generated by Tauri SSH MCP; credential material is not included.".into());
    lines.join("\n")
}

fn sanitize_host_alias(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.' {
                ch
            } else {
                '-'
            }
        })
        .collect()
}

fn shell_arg(value: &str) -> String {
    if value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | '/' | ':' | '@'))
    {
        value.to_string()
    } else {
        shell_quote(value)
    }
}

struct CommandPolicyEvaluation {
    server_alias: String,
    ai_policy: String,
    command: String,
    risk: String,
    action: String,
    reason: String,
}

impl CommandPolicyEvaluation {
    fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "serverAlias": self.server_alias,
            "aiPolicy": self.ai_policy,
            "command": self.command,
            "risk": self.risk,
            "action": self.action,
            "reason": self.reason
        })
    }
}

fn evaluate_command_policy(
    db: &crate::database::Database,
    server_alias: &str,
    command: &str,
) -> Result<CommandPolicyEvaluation, AppError> {
    let server = db
        .get_ssh_server(server_alias)?
        .ok_or_else(|| AppError::NotFound(format!("服务器 '{}' 不存在", server_alias)))?;
    let (risk, command_reason) = classify_command_risk(command)?;
    let (action, reason) = decide_policy_action(&server.ai_policy, &risk, &command_reason);
    Ok(CommandPolicyEvaluation {
        server_alias: server.alias,
        ai_policy: server.ai_policy,
        command: command.trim().to_string(),
        risk,
        action,
        reason,
    })
}

fn classify_command_risk(command: &str) -> Result<(String, String), AppError> {
    let normalized = command.trim();
    if normalized.is_empty() {
        return Err(AppError::InvalidInput("命令不能为空".into()));
    }
    let lower = normalized.to_lowercase();
    let blocked_patterns = [
        ("rm -rf /", "包含根目录强制递归删除"),
        ("mkfs", "涉及磁盘格式化"),
        ("fdisk", "涉及磁盘分区"),
        ("parted", "涉及磁盘分区"),
        (":(){:|:&};:", "命中 fork bomb"),
        ("shutdown", "涉及主机关机"),
        ("poweroff", "涉及主机关机"),
        ("halt", "涉及主机关机"),
        ("drop table", "涉及数据库删除结构"),
        ("truncate table", "涉及数据库清空表"),
    ];
    if blocked_patterns
        .iter()
        .any(|(pattern, _)| lower.contains(pattern))
    {
        let reason = blocked_patterns
            .iter()
            .find(|(pattern, _)| lower.contains(pattern))
            .map(|(_, reason)| *reason)
            .unwrap_or("命中禁止策略");
        return Ok(("blocked".into(), reason.into()));
    }
    if (lower.contains("curl") || lower.contains("wget"))
        && (lower.contains("| sh") || lower.contains("| bash") || lower.contains("| zsh"))
    {
        return Ok(("blocked".into(), "包含下载后直接执行脚本".into()));
    }
    if lower.contains("dd if=") && lower.contains(" of=") {
        return Ok(("blocked".into(), "涉及块级写入".into()));
    }

    let high_patterns = [
        ("rm ", "包含删除命令"),
        (" reboot", "涉及主机重启"),
        ("systemctl restart", "涉及服务重启"),
        ("systemctl stop", "涉及服务停止"),
        ("systemctl start", "涉及服务启动"),
        ("service ", "涉及服务控制"),
        ("kill ", "涉及终止进程"),
        ("pkill ", "涉及终止进程"),
        ("chmod ", "涉及权限变更"),
        ("chown ", "涉及属主变更"),
        ("useradd ", "涉及账号变更"),
        ("userdel ", "涉及账号变更"),
        ("passwd ", "涉及密码变更"),
        (" apt install", "涉及软件安装"),
        (" apt-get install", "涉及软件安装"),
        (" yum install", "涉及软件安装"),
        (" dnf install", "涉及软件安装"),
        (" npm install", "涉及软件安装"),
        (" pnpm install", "涉及软件安装"),
        (" docker stop", "涉及容器变更"),
        (" docker restart", "涉及容器变更"),
        (" docker rm", "涉及容器删除"),
        (" kubectl apply", "涉及 Kubernetes 变更"),
        (" kubectl delete", "涉及 Kubernetes 删除"),
        (" kubectl exec", "涉及 Kubernetes 远程执行"),
        ("sed -i", "包含原地修改文件"),
        (" tee ", "包含写文件管道"),
        (" delete from ", "涉及数据库删除数据"),
        (" update ", "可能涉及数据库更新"),
        (" insert into ", "涉及数据库写入"),
        (" alter table ", "涉及数据库结构变更"),
    ];
    let padded = format!(" {} ", lower);
    if lower.contains(">>") || lower.contains("> ") {
        return Ok(("high".into(), "包含输出重定向写入".into()));
    }
    if let Some((_, reason)) = high_patterns
        .iter()
        .find(|(pattern, _)| padded.contains(pattern) || lower.contains(pattern.trim()))
    {
        return Ok(("high".into(), (*reason).into()));
    }

    let review_patterns = [
        ("sudo ", "包含 sudo，可能触发提权或交互式密码"),
        ("ssh ", "包含二次 SSH 跳转"),
        ("scp ", "涉及文件传输"),
        ("rsync ", "涉及文件同步"),
        ("find ", "find 命令需要额外审核"),
    ];
    let review_reason = review_patterns
        .iter()
        .find(|(pattern, _)| padded.contains(pattern))
        .map(|(_, reason)| (*reason).to_string());

    let segments = split_shell_segments(normalized);
    let all_readonly =
        !segments.is_empty() && segments.iter().all(|segment| is_readonly_segment(segment));
    if all_readonly {
        return Ok(("readonly".into(), "只读查询命令".into()));
    }
    Ok((
        "review".into(),
        review_reason.unwrap_or_else(|| "不在只读命令白名单内".into()),
    ))
}

fn split_shell_segments(command: &str) -> Vec<&str> {
    command
        .split(['|', ';'])
        .flat_map(|part| part.split("&&"))
        .flat_map(|part| part.split("||"))
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect()
}

fn is_readonly_segment(segment: &str) -> bool {
    let lower = segment.to_lowercase();
    if lower.contains('>') || lower.contains("sed -i") || lower.contains("tee ") {
        return false;
    }
    let token = first_command_token(&lower);
    let readonly = [
        "awk",
        "cat",
        "date",
        "df",
        "dmesg",
        "du",
        "env",
        "file",
        "free",
        "grep",
        "head",
        "hostname",
        "id",
        "ip",
        "journalctl",
        "last",
        "lastb",
        "ls",
        "netstat",
        "pgrep",
        "printenv",
        "ps",
        "pwd",
        "rg",
        "sed",
        "sort",
        "ss",
        "stat",
        "tail",
        "top",
        "uname",
        "uniq",
        "uptime",
        "vmstat",
        "wc",
        "which",
        "who",
        "whoami",
        "whereis",
    ];
    if readonly.contains(&token.as_str()) {
        return true;
    }
    if token == "systemctl" {
        return lower.contains("systemctl status")
            || lower.contains("systemctl show")
            || lower.contains("systemctl list")
            || lower.contains("systemctl is-active")
            || lower.contains("systemctl is-enabled");
    }
    if token == "docker" {
        return lower.contains("docker ps")
            || lower.contains("docker images")
            || lower.contains("docker logs")
            || lower.contains("docker inspect")
            || lower.contains("docker stats")
            || lower.contains("docker version")
            || lower.contains("docker info");
    }
    if token == "kubectl" {
        return lower.contains("kubectl get")
            || lower.contains("kubectl describe")
            || lower.contains("kubectl logs")
            || lower.contains("kubectl top")
            || lower.contains("kubectl version");
    }
    false
}

fn first_command_token(segment: &str) -> String {
    let tokens = segment.split_whitespace().collect::<Vec<_>>();
    let mut index = 0usize;
    while let Some(token) = tokens.get(index) {
        match *token {
            "sudo" | "env" | "command" | "builtin" | "time" => index += 1,
            "timeout" => index += 2,
            _ => break,
        }
    }
    tokens
        .get(index)
        .copied()
        .unwrap_or_default()
        .trim_matches(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_' && ch != '-')
        .to_string()
}

fn decide_policy_action(policy: &str, risk: &str, command_reason: &str) -> (String, String) {
    let policy_label = match policy {
        "readonly" => "只读",
        "L1" => "低风险",
        "L2" => "中风险",
        "L3" => "高风险",
        "blocked" => "禁用",
        _ => policy,
    };
    if policy == "blocked" {
        return ("blocked".into(), "当前服务器 AI 权限为禁用".into());
    }
    if risk == "blocked" {
        return (
            "blocked".into(),
            format!("命令命中绝对禁止策略：{}", command_reason),
        );
    }
    if risk == "readonly" {
        return (
            "auto".into(),
            "当前服务器 AI 权限允许自动执行只读命令".into(),
        );
    }
    if risk == "review" {
        if policy == "L2" || policy == "L3" {
            return (
                "review".into(),
                format!(
                    "当前服务器 AI 权限为 {}，常规非只读命令需要用户审核",
                    policy_label
                ),
            );
        }
        return (
            "blocked".into(),
            format!(
                "当前服务器 AI 权限为 {}，不允许 AI 执行非只读命令",
                policy_label
            ),
        );
    }
    if policy == "L3" {
        return (
            "review".into(),
            format!(
                "当前服务器 AI 权限为 {}，高风险命令必须用户强确认",
                policy_label
            ),
        );
    }
    (
        "blocked".into(),
        format!(
            "当前服务器 AI 权限为 {}，不允许 AI 执行高风险命令",
            policy_label
        ),
    )
}

fn require_approved_request(
    db: &crate::database::Database,
    approval_id: i64,
    expected_action: &str,
    server_alias: &str,
    command: Option<&str>,
    resource: Option<&str>,
) -> Result<(), AppError> {
    let approval = db
        .get_approval_request(approval_id)?
        .ok_or_else(|| AppError::NotFound(format!("审批请求 '{}' 不存在", approval_id)))?;
    if approval.status != "approved" {
        return Err(AppError::InvalidInput(format!(
            "审批请求当前状态为 '{}'，不能执行",
            approval.status
        )));
    }
    if approval.action != expected_action {
        return Err(AppError::InvalidInput(format!(
            "审批动作不匹配：期望 '{}'，实际 '{}'",
            expected_action, approval.action
        )));
    }
    if !approval.server_alias.is_empty() && approval.server_alias != server_alias {
        return Err(AppError::InvalidInput(
            "审批服务器与执行服务器不一致".into(),
        ));
    }
    if let Some(command) = command {
        if !approval.command.is_empty() && approval.command.trim() != command.trim() {
            return Err(AppError::InvalidInput("审批命令与执行命令不一致".into()));
        }
    }
    if let Some(resource) = resource {
        if !approval.resource.is_empty() && approval.resource.trim() != resource.trim() {
            return Err(AppError::InvalidInput("审批资源与执行资源不一致".into()));
        }
    }
    Ok(())
}

fn validate_readonly_command(command: &str) -> Result<(), AppError> {
    let normalized = command.trim();
    if normalized.is_empty() {
        return Err(AppError::InvalidInput("命令不能为空".into()));
    }
    let lower = normalized.to_lowercase();
    let blocked_patterns = [
        ">",
        ">>",
        "&&",
        "||",
        ";",
        "`",
        "$(",
        " rm ",
        "rm -",
        "mv ",
        "cp ",
        "chmod ",
        "chown ",
        "sudo ",
        "su ",
        "kill ",
        "pkill ",
        "reboot",
        "shutdown",
        "systemctl restart",
        "systemctl stop",
        "service ",
        "docker rm",
        "docker stop",
        "kubectl delete",
        "truncate ",
        "mkfs",
        "dd ",
        "tee ",
        "sed -i",
        "perl -pi",
        "npm uninstall",
        "nvm uninstall",
    ];
    if blocked_patterns
        .iter()
        .any(|pattern| lower.contains(pattern))
    {
        return Err(AppError::InvalidInput(
            "该工具只允许只读命令，当前命令包含写入/控制/危险片段".into(),
        ));
    }
    let allowed = [
        "ls",
        "pwd",
        "whoami",
        "id",
        "hostname",
        "uname",
        "date",
        "uptime",
        "df",
        "du",
        "free",
        "top",
        "ps",
        "pgrep",
        "netstat",
        "ss",
        "ip",
        "ifconfig",
        "cat",
        "head",
        "tail",
        "grep",
        "egrep",
        "fgrep",
        "rg",
        "awk",
        "wc",
        "sort",
        "uniq",
        "find",
        "stat",
        "file",
        "env",
        "printenv",
        "which",
        "whereis",
        "systemctl",
        "journalctl",
        "docker",
        "kubectl",
        "curl",
    ];
    let first = normalized
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .trim_matches(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_' && ch != '-');
    if !allowed.contains(&first) {
        return Err(AppError::InvalidInput(format!(
            "只读命令白名单不包含 '{}'",
            first
        )));
    }
    if (first == "systemctl"
        && !lower.starts_with("systemctl status")
        && !lower.starts_with("systemctl list"))
        || (first == "docker"
            && !lower.starts_with("docker ps")
            && !lower.starts_with("docker logs")
            && !lower.starts_with("docker inspect"))
        || (first == "kubectl"
            && !lower.starts_with("kubectl get")
            && !lower.starts_with("kubectl describe")
            && !lower.starts_with("kubectl logs"))
    {
        return Err(AppError::InvalidInput("该子命令不属于只读范围".into()));
    }
    Ok(())
}

fn app_state(ctx: &DevApiState) -> tauri::State<'_, AppState> {
    ctx.app_handle.state::<AppState>()
}

async fn list_ai_providers(
    State(ctx): State<DevApiState>,
) -> DevApiResult<Vec<crate::models::AiProvider>> {
    let state = app_state(&ctx);
    Ok(Json(AiProviderService::list(&state.db)?))
}

async fn upsert_ai_provider(
    State(ctx): State<DevApiState>,
    Json(input): Json<UpsertAiProviderInput>,
) -> DevApiResult<crate::models::AiProvider> {
    let state = app_state(&ctx);
    Ok(Json(AiProviderService::upsert(&state.db, input)?))
}

async fn delete_ai_provider(
    State(ctx): State<DevApiState>,
    Path(key): Path<String>,
) -> Result<StatusCode, DevApiError> {
    let state = app_state(&ctx);
    AiProviderService::delete(&state.db, &key)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_ai_provider_routes(
    State(ctx): State<DevApiState>,
) -> DevApiResult<Vec<crate::models::AiProviderRoute>> {
    let state = app_state(&ctx);
    Ok(Json(AiProviderService::list_routes(&state.db)?))
}

async fn upsert_ai_provider_route(
    State(ctx): State<DevApiState>,
    Json(input): Json<UpsertAiProviderRouteInput>,
) -> DevApiResult<crate::models::AiProviderRoute> {
    let state = app_state(&ctx);
    Ok(Json(AiProviderService::upsert_route(&state.db, input)?))
}

async fn test_ai_provider(
    State(ctx): State<DevApiState>,
    Path(key): Path<String>,
) -> DevApiResult<crate::models::AiProviderTestResult> {
    let state = app_state(&ctx);
    Ok(Json(AiProviderService::test(&state.db, &key).await?))
}

async fn list_ai_provider_models(
    State(ctx): State<DevApiState>,
    Json(input): Json<AiProviderModelListInput>,
) -> DevApiResult<crate::models::AiProviderModelListResult> {
    let state = app_state(&ctx);
    Ok(Json(
        AiProviderService::list_models(&state.db, input).await?,
    ))
}

async fn ask_ai_provider(
    State(ctx): State<DevApiState>,
    Json(input): Json<AiProviderAskInput>,
) -> DevApiResult<crate::models::AiProviderAskResult> {
    let state = app_state(&ctx);
    Ok(Json(AiProviderService::ask(&state.db, input).await?))
}

async fn sync_builtin_ai_skills(
    State(ctx): State<DevApiState>,
) -> DevApiResult<crate::models::SyncBuiltinAiSkillsResult> {
    let state = app_state(&ctx);
    Ok(Json(AiSkillService::sync_builtin(
        &ctx.app_handle,
        &state.db,
    )?))
}

async fn list_ai_skills(
    State(ctx): State<DevApiState>,
    Json(input): Json<ListAiSkillsInput>,
) -> DevApiResult<crate::models::ListAiSkillsResult> {
    let state = app_state(&ctx);
    Ok(Json(AiSkillService::list(&state.db, input)?))
}

async fn upsert_ai_skill(
    State(ctx): State<DevApiState>,
    Json(input): Json<UpsertAiSkillInput>,
) -> DevApiResult<crate::models::AiSkill> {
    let state = app_state(&ctx);
    Ok(Json(AiSkillService::upsert(&state.db, input)?))
}

async fn set_ai_skill_enabled(
    State(ctx): State<DevApiState>,
    Path(id): Path<i64>,
    Json(payload): Json<serde_json::Value>,
) -> DevApiResult<crate::models::AiSkill> {
    let state = app_state(&ctx);
    let enabled = payload
        .get("enabled")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(true);
    Ok(Json(AiSkillService::set_enabled(&state.db, id, enabled)?))
}

async fn copy_ai_skill(
    State(ctx): State<DevApiState>,
    Path(id): Path<i64>,
) -> DevApiResult<crate::models::AiSkill> {
    let state = app_state(&ctx);
    Ok(Json(AiSkillService::copy_skill(&state.db, id)?))
}

async fn restore_builtin_ai_skill(
    State(ctx): State<DevApiState>,
    Path(id): Path<i64>,
) -> DevApiResult<crate::models::AiSkill> {
    let state = app_state(&ctx);
    Ok(Json(AiSkillService::restore_builtin(&state.db, id)?))
}

async fn delete_ai_skill(
    State(ctx): State<DevApiState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, DevApiError> {
    let state = app_state(&ctx);
    AiSkillService::delete(&state.db, id)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn test_ai_skill_trigger(
    State(ctx): State<DevApiState>,
    Json(input): Json<AiSkillTriggerInput>,
) -> DevApiResult<crate::models::AiSkillTriggerResult> {
    let state = app_state(&ctx);
    Ok(Json(AiSkillService::test_trigger(&state.db, input)?))
}

async fn preview_ai_skill_prompt(
    State(ctx): State<DevApiState>,
    Json(input): Json<AiSkillPromptPreviewInput>,
) -> DevApiResult<crate::models::AiSkillPromptPreviewResult> {
    let state = app_state(&ctx);
    Ok(Json(AiSkillService::prompt_preview(&state.db, input)?))
}

async fn list_ai_experiences(
    State(ctx): State<DevApiState>,
    Query(query): Query<std::collections::HashMap<String, String>>,
) -> DevApiResult<Vec<crate::models::AiExperience>> {
    let state = app_state(&ctx);
    Ok(Json(AiSkillService::list_experiences(
        &state.db,
        query.get("keyword").cloned(),
    )?))
}

async fn recall_ai_experiences(
    State(ctx): State<DevApiState>,
    Json(input): Json<AiExperienceRecallInput>,
) -> DevApiResult<Vec<crate::models::AiExperienceMatch>> {
    let state = app_state(&ctx);
    Ok(Json(AiSkillService::recall_experiences(&state.db, input)?))
}

async fn upsert_ai_experience(
    State(ctx): State<DevApiState>,
    Json(input): Json<UpsertAiExperienceInput>,
) -> DevApiResult<crate::models::AiExperience> {
    let state = app_state(&ctx);
    Ok(Json(AiSkillService::upsert_experience(
        &ctx.app_handle,
        &state.db,
        input,
    )?))
}

async fn delete_ai_experience(
    State(ctx): State<DevApiState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, DevApiError> {
    let state = app_state(&ctx);
    AiSkillService::delete_experience(&state.db, id)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_ai_runbooks(
    State(ctx): State<DevApiState>,
    Query(query): Query<std::collections::HashMap<String, String>>,
) -> DevApiResult<Vec<crate::models::AiRunbook>> {
    let state = app_state(&ctx);
    Ok(Json(AiSkillService::list_runbooks(
        &state.db,
        query.get("keyword").cloned(),
    )?))
}

async fn upsert_ai_runbook(
    State(ctx): State<DevApiState>,
    Json(input): Json<UpsertAiRunbookInput>,
) -> DevApiResult<crate::models::AiRunbook> {
    let state = app_state(&ctx);
    Ok(Json(AiSkillService::upsert_runbook(&state.db, input)?))
}

async fn run_ai_runbook(
    State(ctx): State<DevApiState>,
    Json(input): Json<RunAiRunbookInput>,
) -> DevApiResult<crate::models::AiRunbookRunResult> {
    let state = app_state(&ctx);
    Ok(Json(AiSkillService::run_runbook(&state.db, input).await?))
}

async fn delete_ai_runbook(
    State(ctx): State<DevApiState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, DevApiError> {
    let state = app_state(&ctx);
    AiSkillService::delete_runbook(&state.db, id)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_ssh_servers(
    State(ctx): State<DevApiState>,
) -> DevApiResult<Vec<crate::models::SshServer>> {
    let state = app_state(&ctx);
    Ok(Json(SshServerService::list(&state.db)?))
}

async fn upsert_ssh_server(
    State(ctx): State<DevApiState>,
    Json(input): Json<crate::models::UpsertSshServerInput>,
) -> DevApiResult<crate::models::SshServer> {
    let state = app_state(&ctx);
    Ok(Json(SshServerService::upsert(&state.db, input)?))
}

async fn delete_ssh_server(
    State(ctx): State<DevApiState>,
    Path(alias): Path<String>,
) -> Result<StatusCode, DevApiError> {
    let state = app_state(&ctx);
    SshServerService::delete(&state.db, &alias)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn import_ssh_config(
    State(ctx): State<DevApiState>,
    Json(path): Json<Option<String>>,
) -> DevApiResult<crate::models::SshConfigImportResult> {
    let state = app_state(&ctx);
    Ok(Json(SshServerService::import_ssh_config(&state.db, path)?))
}

async fn test_ssh_server(
    State(ctx): State<DevApiState>,
    Path(alias): Path<String>,
) -> DevApiResult<crate::models::SshServerTestResult> {
    let state = app_state(&ctx);
    Ok(Json(SshServerService::test(&state.db, &alias).await?))
}

async fn test_ssh_server_connection(
    Json(input): Json<crate::models::SshServerConnectionTestInput>,
) -> DevApiResult<crate::models::SshServerTestResult> {
    Ok(Json(SshServerService::test_connection(input).await?))
}

async fn list_credentials(
    State(ctx): State<DevApiState>,
) -> DevApiResult<Vec<crate::models::CredentialVaultItem>> {
    let state = app_state(&ctx);
    Ok(Json(CredentialVaultService::list(&state.db)?))
}

async fn upsert_credential(
    State(ctx): State<DevApiState>,
    Json(input): Json<crate::models::UpsertCredentialInput>,
) -> DevApiResult<crate::models::CredentialVaultItem> {
    let state = app_state(&ctx);
    Ok(Json(CredentialVaultService::upsert(&state.db, input)?))
}

async fn authorize_credential(
    State(ctx): State<DevApiState>,
    Json(input): Json<crate::models::AuthorizeCredentialInput>,
) -> DevApiResult<crate::models::CredentialVaultItem> {
    let state = app_state(&ctx);
    Ok(Json(CredentialVaultService::authorize(&state.db, input)?))
}

async fn rotate_credential(
    State(ctx): State<DevApiState>,
    Json(input): Json<crate::models::RotateCredentialInput>,
) -> DevApiResult<crate::models::CredentialVaultItem> {
    let state = app_state(&ctx);
    Ok(Json(CredentialVaultService::rotate(&state.db, input)?))
}

async fn delete_credential(
    State(ctx): State<DevApiState>,
    Path(key): Path<String>,
) -> Result<StatusCode, DevApiError> {
    let state = app_state(&ctx);
    CredentialVaultService::delete(&state.db, &key)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_database_connections(
    State(ctx): State<DevApiState>,
) -> DevApiResult<Vec<crate::models::DatabaseConnection>> {
    let state = app_state(&ctx);
    Ok(Json(DatabaseOpsService::list_connections(&state.db)?))
}

async fn upsert_database_connection(
    State(ctx): State<DevApiState>,
    Json(input): Json<UpsertDatabaseConnectionInput>,
) -> DevApiResult<crate::models::DatabaseConnection> {
    let state = app_state(&ctx);
    Ok(Json(DatabaseOpsService::upsert_connection(
        &state.db, input,
    )?))
}

async fn delete_database_connection(
    State(ctx): State<DevApiState>,
    Path(key): Path<String>,
) -> Result<StatusCode, DevApiError> {
    let state = app_state(&ctx);
    DatabaseOpsService::delete_connection(&state.db, &key)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn test_database_connection(
    State(ctx): State<DevApiState>,
    Path(key): Path<String>,
) -> DevApiResult<crate::models::DatabaseConnectionTestResult> {
    let state = app_state(&ctx);
    Ok(Json(
        DatabaseOpsService::test_connection(&state.db, &key).await?,
    ))
}

async fn execute_database_readonly_query(
    State(ctx): State<DevApiState>,
    Json(input): Json<crate::models::DatabaseQueryInput>,
) -> DevApiResult<crate::models::DatabaseQueryResult> {
    let state = app_state(&ctx);
    Ok(Json(
        DatabaseOpsService::execute_readonly_query(&state.db, input).await?,
    ))
}

async fn list_database_names(
    State(ctx): State<DevApiState>,
    Json(input): Json<crate::models::DatabaseNameListInput>,
) -> DevApiResult<crate::models::DatabaseNameListResult> {
    let state = app_state(&ctx);
    Ok(Json(
        DatabaseOpsService::list_database_names(&state.db, input).await?,
    ))
}

async fn list_database_schema(
    State(ctx): State<DevApiState>,
    Json(input): Json<crate::models::DatabaseSchemaInput>,
) -> DevApiResult<crate::models::DatabaseSchemaResult> {
    let state = app_state(&ctx);
    Ok(Json(
        DatabaseOpsService::list_database_schema(&state.db, input).await?,
    ))
}

async fn execute_database_sql(
    State(ctx): State<DevApiState>,
    Json(input): Json<crate::models::DatabaseQueryInput>,
) -> DevApiResult<crate::models::DatabaseQueryResult> {
    let state = app_state(&ctx);
    Ok(Json(
        DatabaseOpsService::execute_sql(&state.db, input).await?,
    ))
}

async fn execute_database_sql_batch(
    State(ctx): State<DevApiState>,
    Json(input): Json<crate::models::DatabaseQueryInput>,
) -> DevApiResult<Vec<crate::models::DatabaseQueryResult>> {
    let state = app_state(&ctx);
    Ok(Json(
        DatabaseOpsService::execute_sql_batch(&state.db, input).await?,
    ))
}

async fn export_database(
    State(ctx): State<DevApiState>,
    Json(input): Json<crate::models::DatabaseExportInput>,
) -> DevApiResult<crate::models::DatabaseExportResult> {
    let state = app_state(&ctx);
    Ok(Json(
        DatabaseOpsService::export_database(&state.db, input).await?,
    ))
}

async fn scan_redis_keys(
    State(ctx): State<DevApiState>,
    Json(input): Json<crate::models::RedisScanInput>,
) -> DevApiResult<crate::models::RedisScanResult> {
    let state = app_state(&ctx);
    Ok(Json(
        DatabaseOpsService::scan_redis_keys(&state.db, input).await?,
    ))
}

async fn describe_redis_keys(
    State(ctx): State<DevApiState>,
    Json(input): Json<crate::models::RedisDescribeKeysInput>,
) -> DevApiResult<crate::models::RedisScanResult> {
    let state = app_state(&ctx);
    Ok(Json(
        DatabaseOpsService::describe_redis_keys(&state.db, input).await?,
    ))
}

async fn list_redis_databases(
    State(ctx): State<DevApiState>,
    Json(input): Json<crate::models::RedisDatabaseListInput>,
) -> DevApiResult<crate::models::RedisDatabaseListResult> {
    let state = app_state(&ctx);
    Ok(Json(
        DatabaseOpsService::list_redis_databases(&state.db, input).await?,
    ))
}

async fn list_redis_key_tree(
    State(ctx): State<DevApiState>,
    Json(input): Json<crate::models::RedisKeyTreeInput>,
) -> DevApiResult<crate::models::RedisKeyTreeResult> {
    let state = app_state(&ctx);
    Ok(Json(
        DatabaseOpsService::list_redis_key_tree(&state.db, input).await?,
    ))
}

async fn get_redis_value_preview(
    State(ctx): State<DevApiState>,
    Json(input): Json<crate::models::RedisValuePreviewInput>,
) -> DevApiResult<crate::models::RedisValuePreview> {
    let state = app_state(&ctx);
    Ok(Json(
        DatabaseOpsService::get_redis_value_preview(&state.db, input).await?,
    ))
}

async fn execute_terminal_command(
    State(ctx): State<DevApiState>,
    Json(input): Json<crate::models::TerminalCommandInput>,
) -> DevApiResult<crate::models::TerminalCommandResult> {
    let state = app_state(&ctx);
    let audit_input = input.clone();
    match TerminalService::execute(&state.db, input).await {
        Ok(result) => {
            audit_terminal_command_result(&state.db, &result);
            Ok(Json(result))
        }
        Err(error) => {
            audit_terminal_command_error(&state.db, &audit_input, &error.to_string());
            Err(error.into())
        }
    }
}

async fn sftp_list(
    State(ctx): State<DevApiState>,
    Json(input): Json<crate::models::SftpListInput>,
) -> DevApiResult<crate::models::SftpListResult> {
    let state = app_state(&ctx);
    Ok(Json(SftpService::list(&state.db, input)?))
}

async fn sftp_read_text(
    State(ctx): State<DevApiState>,
    Json(input): Json<crate::models::SftpReadTextInput>,
) -> DevApiResult<crate::models::SftpReadTextResult> {
    let state = app_state(&ctx);
    Ok(Json(SftpService::read_text(&state.db, input)?))
}

async fn sftp_write_text(
    State(ctx): State<DevApiState>,
    Json(input): Json<crate::models::SftpWriteTextInput>,
) -> DevApiResult<crate::models::SftpOperationResult> {
    let state = app_state(&ctx);
    Ok(Json(SftpService::write_text(&state.db, input)?))
}

async fn sftp_upload(
    State(ctx): State<DevApiState>,
    Json(input): Json<crate::models::SftpTransferPathInput>,
) -> DevApiResult<crate::models::SftpOperationResult> {
    let state = app_state(&ctx);
    Ok(Json(SftpService::upload(&state.db, input)?))
}

async fn sftp_download(
    State(ctx): State<DevApiState>,
    Json(input): Json<crate::models::SftpTransferPathInput>,
) -> DevApiResult<crate::models::SftpOperationResult> {
    let state = app_state(&ctx);
    Ok(Json(SftpService::download(&state.db, input)?))
}

async fn sftp_create_directory(
    State(ctx): State<DevApiState>,
    Json(input): Json<crate::models::SftpCreateDirectoryInput>,
) -> DevApiResult<crate::models::SftpOperationResult> {
    let state = app_state(&ctx);
    Ok(Json(SftpService::create_directory(&state.db, input)?))
}

async fn sftp_create_file(
    State(ctx): State<DevApiState>,
    Json(input): Json<crate::models::SftpCreateFileInput>,
) -> DevApiResult<crate::models::SftpOperationResult> {
    let state = app_state(&ctx);
    Ok(Json(SftpService::create_file(&state.db, input)?))
}

async fn sftp_rename(
    State(ctx): State<DevApiState>,
    Json(input): Json<crate::models::SftpRenameInput>,
) -> DevApiResult<crate::models::SftpOperationResult> {
    let state = app_state(&ctx);
    Ok(Json(SftpService::rename(&state.db, input)?))
}

async fn sftp_delete(
    State(ctx): State<DevApiState>,
    Json(input): Json<crate::models::SftpDeleteInput>,
) -> DevApiResult<crate::models::SftpOperationResult> {
    let state = app_state(&ctx);
    Ok(Json(SftpService::delete(&state.db, input)?))
}

async fn terminal_websocket(
    State(ctx): State<DevApiState>,
    Query(params): Query<std::collections::HashMap<String, String>>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_terminal_websocket(ctx, params, socket))
}

async fn handle_terminal_websocket(
    ctx: DevApiState,
    params: std::collections::HashMap<String, String>,
    mut socket: WebSocket,
) {
    let server_alias = params.get("serverAlias").cloned().unwrap_or_default();
    let cols = params
        .get("cols")
        .and_then(|value| value.parse::<u32>().ok());
    let rows = params
        .get("rows")
        .and_then(|value| value.parse::<u32>().ok());
    let (event_tx, mut event_rx) =
        tokio::sync::mpsc::unbounded_channel::<crate::models::TerminalSessionEvent>();
    let started = {
        let state = app_state(&ctx);
        TerminalService::start_raw_session(
            &state.db,
            crate::models::TerminalSessionStartInput {
                server_alias: server_alias.clone(),
                cols,
                rows,
            },
            move |event| {
                let _ = event_tx.send(event);
            },
            None,
        )
    };

    let (session_id, handle) = match started {
        Ok(value) => {
            let state = app_state(&ctx);
            let _ = AuditService::create(
                &state.db,
                CreateAuditLogInput {
                    actor: "local-user".into(),
                    source: "terminal".into(),
                    server_alias: server_alias.clone(),
                    action: "terminal_session_start".into(),
                    risk: "readonly".into(),
                    result: "成功".into(),
                    summary: format!("打开 Dev WebSocket 终端会话：{}", value.0),
                    detail_json: Some(
                        serde_json::json!({
                            "sessionId": value.0,
                            "transport": "dev-websocket",
                            "cols": cols,
                            "rows": rows
                        })
                        .to_string(),
                    ),
                    request_id: None,
                    approval_id: None,
                },
            );
            value
        }
        Err(error) => {
            let state = app_state(&ctx);
            let _ = AuditService::create(
                &state.db,
                CreateAuditLogInput {
                    actor: "local-user".into(),
                    source: "terminal".into(),
                    server_alias: server_alias.clone(),
                    action: "terminal_session_start".into(),
                    risk: "readonly".into(),
                    result: "失败".into(),
                    summary: "打开 Dev WebSocket 终端会话失败".into(),
                    detail_json: Some(
                        serde_json::json!({
                            "transport": "dev-websocket",
                            "error": error.to_string()
                        })
                        .to_string(),
                    ),
                    request_id: None,
                    approval_id: None,
                },
            );
            let payload = crate::models::TerminalSessionEvent {
                session_id: String::new(),
                kind: "error".into(),
                data: None,
                message: Some(error.to_string()),
            };
            let _ = socket
                .send(Message::Text(
                    serde_json::to_string(&payload).unwrap_or_else(|_| "{}".into()),
                ))
                .await;
            let _ = socket.close().await;
            return;
        }
    };

    let hello = crate::models::TerminalSessionEvent {
        session_id: session_id.clone(),
        kind: "status".into(),
        data: None,
        message: Some("Dev WebSocket 终端通道已建立".into()),
    };
    let _ = socket
        .send(Message::Text(
            serde_json::to_string(&hello).unwrap_or_else(|_| "{}".into()),
        ))
        .await;

    loop {
        tokio::select! {
            maybe_event = event_rx.recv() => {
                let Some(event) = maybe_event else { break };
                let should_close = event.kind == "exit" || event.kind == "error";
                let payload = serde_json::to_string(&event).unwrap_or_else(|_| "{}".into());
                if socket.send(Message::Text(payload)).await.is_err() {
                    break;
                }
                if should_close {
                    break;
                }
            }
            maybe_message = socket.recv() => {
                let Some(Ok(message)) = maybe_message else { break };
                match message {
                    Message::Text(text) => {
                        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) {
                            match value.get("type").and_then(|item| item.as_str()) {
                                Some("data") => {
                                    if let Some(data) = value.get("data").and_then(|item| item.as_str()) {
                                        let _ = handle.send(TerminalPtyCommand::Data(data.to_string()));
                                    }
                                }
                                Some("resize") => {
                                    let cols = value.get("cols").and_then(|item| item.as_u64()).unwrap_or(100) as u32;
                                    let rows = value.get("rows").and_then(|item| item.as_u64()).unwrap_or(30) as u32;
                                    let _ = handle.send(TerminalPtyCommand::Resize(cols, rows));
                                }
                                Some("close") => {
                                    let _ = handle.send(TerminalPtyCommand::Close);
                                    break;
                                }
                                _ => {}
                            }
                        }
                    }
                    Message::Binary(bytes) => {
                        let data = String::from_utf8_lossy(&bytes).to_string();
                        let _ = handle.send(TerminalPtyCommand::Data(data));
                    }
                    Message::Close(_) => break,
                    _ => {}
                }
            }
        }
    }
    let _ = handle.send(TerminalPtyCommand::Close);
    let state = app_state(&ctx);
    let _ = AuditService::create(
        &state.db,
        CreateAuditLogInput {
            actor: "local-user".into(),
            source: "terminal".into(),
            server_alias,
            action: "terminal_session_close".into(),
            risk: "readonly".into(),
            result: "成功".into(),
            summary: format!("关闭 Dev WebSocket 终端会话：{}", session_id),
            detail_json: Some(
                serde_json::json!({
                    "sessionId": session_id,
                    "transport": "dev-websocket"
                })
                .to_string(),
            ),
            request_id: None,
            approval_id: None,
        },
    );
}

fn audit_terminal_command_result(
    db: &crate::database::Database,
    result: &crate::models::TerminalCommandResult,
) {
    let _ = AuditService::create(
        db,
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
                "已禁止"
            } else if result.exit_status == 0 {
                "成功"
            } else {
                "失败"
            }
            .into(),
            summary: format!("执行终端命令：{}", redact_audit_text(&result.command, 500)),
            detail_json: Some(
                serde_json::json!({
                    "serverAlias": result.server_alias,
                    "command": redact_audit_text(&result.command, 500),
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
}

fn audit_terminal_command_error(
    db: &crate::database::Database,
    input: &crate::models::TerminalCommandInput,
    error: &str,
) {
    let _ = AuditService::create(
        db,
        CreateAuditLogInput {
            actor: "local-user".into(),
            source: "terminal".into(),
            server_alias: input.server_alias.clone(),
            action: "terminal_execute".into(),
            risk: "readonly".into(),
            result: "失败".into(),
            summary: format!(
                "终端命令执行失败：{}",
                redact_audit_text(&input.command, 500)
            ),
            detail_json: Some(
                serde_json::json!({
                    "command": redact_audit_text(&input.command, 500),
                    "error": error
                })
                .to_string(),
            ),
            request_id: None,
            approval_id: None,
        },
    );
}
