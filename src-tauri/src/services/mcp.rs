use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::Local;
use serde_json::{json, Map, Value};

use crate::error::AppError;
use crate::models::{
    ConfigureMcpClientInput, ConfigureMcpClientResult, McpClientConfig, McpManualSnippet,
    McpOverview, McpServerStatus, McpToolPermission,
};

const MCP_SERVER_KEY: &str = "tauri-ssh";
const MCP_HTTP_URL: &str = "http://127.0.0.1:17321/mcp";

struct McpClientTemplate {
    key: &'static str,
    name: &'static str,
    vendor: &'static str,
    description: &'static str,
    scope: &'static str,
    transport: &'static str,
    relative_config_path: &'static str,
}

pub struct McpService;

impl McpService {
    pub fn overview() -> Result<McpOverview, AppError> {
        let status = Self::status();
        Ok(McpOverview {
            clients: Self::clients()?,
            tools: Self::tools(),
            snippets: Self::manual_snippets(&status),
            status,
        })
    }

    pub fn status() -> McpServerStatus {
        McpServerStatus {
            server_name: MCP_SERVER_KEY.into(),
            streamable_http_url: MCP_HTTP_URL.into(),
            // 用 mcp-remote 作为 stdio bridge，可兼容只支持 stdio 的 Agent 客户端。
            stdio_command: "npx".into(),
            stdio_args: vec!["-y".into(), "mcp-remote".into(), MCP_HTTP_URL.into()],
            local_only: true,
            http_reachable: tcp_reachable("127.0.0.1:17321"),
            platform: std::env::consts::OS.into(),
            notes: vec![
                "端点仅监听 127.0.0.1，不直接暴露公网。".into(),
                "仅覆盖客户端配置中的 tauri-ssh 条目，写入前会备份原配置文件。".into(),
                "仅支持本地 Streamable HTTP；stdio 客户端通过 mcp-remote 桥接。".into(),
            ],
        }
    }

    pub fn clients() -> Result<Vec<McpClientConfig>, AppError> {
        client_templates()
            .iter()
            .map(|template| client_from_template(template))
            .collect()
    }

    pub fn tools() -> Vec<McpToolPermission> {
        let mut tools = vec![
            McpToolPermission {
                tool: "mcp_status".into(),
                policy: "只读".into(),
                audit: "记录调用来源".into(),
            },
            McpToolPermission {
                tool: "ssh_servers_list".into(),
                policy: "只读，返回脱敏服务器元数据".into(),
                audit: "记录客户端与数量".into(),
            },
            McpToolPermission {
                tool: "ssh_server_detail".into(),
                policy: "只读，返回单台服务器脱敏详情".into(),
                audit: "记录服务器别名".into(),
            },
            McpToolPermission {
                tool: "ssh_test_connection".into(),
                policy: "只读连通性探测".into(),
                audit: "记录服务器别名和测试结果".into(),
            },
            McpToolPermission {
                tool: "terminal_execute_readonly".into(),
                policy: "只允许命令白名单内的只读命令".into(),
                audit: "记录命令、退出码和耗时".into(),
            },
            McpToolPermission {
                tool: "sftp_list".into(),
                policy: "只读目录浏览".into(),
                audit: "记录服务器和路径".into(),
            },
            McpToolPermission {
                tool: "sftp_read_text".into(),
                policy: "只读文本读取，限制大小".into(),
                audit: "记录服务器、路径和截断状态".into(),
            },
            McpToolPermission {
                tool: "log_tail_snapshot".into(),
                policy: "只读 tail 快照，限制行数".into(),
                audit: "记录服务器、日志路径和行数".into(),
            },
            McpToolPermission {
                tool: "log_search".into(),
                policy: "只读日志关键词搜索".into(),
                audit: "记录服务器、日志路径和匹配数".into(),
            },
            McpToolPermission {
                tool: "deployment_templates_list".into(),
                policy: "只读，返回内置部署配方和适用场景".into(),
                audit: "记录客户端和配方数量".into(),
            },
            McpToolPermission {
                tool: "deployment_targets_list".into(),
                policy: "只读，返回部署目标脱敏配置，不返回凭证明文".into(),
                audit: "记录筛选条件和目标数量".into(),
            },
            McpToolPermission {
                tool: "deployment_groups_list".into(),
                policy: "只读，返回部署组和目标顺序".into(),
                audit: "记录客户端和部署组数量".into(),
            },
            McpToolPermission {
                tool: "deployment_runs_list".into(),
                policy: "只读，返回部署运行记录".into(),
                audit: "记录目标/部署组/状态筛选和数量".into(),
            },
            McpToolPermission {
                tool: "deployment_detect_project".into(),
                policy: "受控检测本地目录或 Git 来源，不执行远程部署命令".into(),
                audit: "记录来源类型、路径或仓库地址和候选数量".into(),
            },
            McpToolPermission {
                tool: "deployment_dry_run".into(),
                policy: "只生成部署计划和风险说明，不执行部署".into(),
                audit: "记录 target/group、planId、风险和阶段数".into(),
            },
            McpToolPermission {
                tool: "deployment_run".into(),
                policy: "必须先 dry-run 并传入 planId；高风险阶段进入审批队列".into(),
                audit: "记录 target/group、planId、runId、审批 ID 和结果".into(),
            },
            McpToolPermission {
                tool: "deployment_run_status".into(),
                policy: "只读查询部署运行状态".into(),
                audit: "记录 runId 和步骤状态".into(),
            },
            McpToolPermission {
                tool: "deployment_run_logs".into(),
                policy: "只读查询部署步骤日志预览".into(),
                audit: "记录 runId、stepKey 和日志条数".into(),
            },
            McpToolPermission {
                tool: "deployment_rollback_dry_run".into(),
                policy: "只生成回滚计划，不执行回滚".into(),
                audit: "记录 targetKey、planId 和风险".into(),
            },
            McpToolPermission {
                tool: "deployment_rollback_run".into(),
                policy: "必须先 rollback dry-run 并传入 planId；高风险阶段进入审批队列".into(),
                audit: "记录 targetKey、planId、runId、审批 ID 和结果".into(),
            },
            McpToolPermission {
                tool: "deployment_ai_advice".into(),
                policy: "基于 dry-run 计划生成部署建议和风险解释，不执行部署".into(),
                audit: "记录 target/group、Provider、模型和耗时".into(),
            },
            McpToolPermission {
                tool: "ai_providers_list".into(),
                policy: "只读，返回 Provider 状态不含密钥".into(),
                audit: "记录客户端与数量".into(),
            },
            McpToolPermission {
                tool: "secure_credentials_list".into(),
                policy: "只读，返回安全凭证脱敏元数据".into(),
                audit: "记录客户端、筛选条件和数量".into(),
            },
            McpToolPermission {
                tool: "secure_session_create".into(),
                policy: "仅为 allow_mcp=true 的凭证创建短期会话，不返回密钥".into(),
                audit: "记录调用方、凭证 Key、授权范围和过期时间".into(),
            },
            McpToolPermission {
                tool: "secure_session_status".into(),
                policy: "只读校验短期会话状态".into(),
                audit: "记录 sessionId 和有效性".into(),
            },
            McpToolPermission {
                tool: "secure_session_revoke".into(),
                policy: "吊销短期会话，不触碰原始凭据".into(),
                audit: "记录 sessionId 和吊销结果".into(),
            },
            McpToolPermission {
                tool: "secure_provider_test".into(),
                policy: "通过短期 session 测试 Provider 连接，响应脱敏".into(),
                audit: "记录 session、Provider 和测试结果".into(),
            },
            McpToolPermission {
                tool: "secure_git_repositories_list".into(),
                policy: "只读读取 GitHub/GitLab/GitCode/Gitee 仓库列表，不返回 Token".into(),
                audit: "记录 sessionId、Provider 和仓库数量".into(),
            },
            McpToolPermission {
                tool: "secure_git_readonly_request".into(),
                policy: "只读读取 repo/branch/file/commit/PR/MR/issue/tag/release 等 Git 资源，不返回 Token".into(),
                audit: "记录 sessionId、资源类型、仓库和状态码".into(),
            },
            McpToolPermission {
                tool: "secure_http_readonly_request".into(),
                policy: "仅允许 HTTP API GET 相对路径请求，响应脱敏".into(),
                audit: "记录 sessionId、path、状态码和截断状态".into(),
            },
            McpToolPermission {
                tool: "secure_git_write_controlled".into(),
                policy: "Git 写操作只创建审批请求，固化 requestHash".into(),
                audit: "记录仓库、操作、requestHash 和审批 ID".into(),
            },
            McpToolPermission {
                tool: "secure_git_write_approved".into(),
                policy: "仅执行 approved 且 requestHash 匹配的 Git 写操作；高风险操作受策略页开关限制".into(),
                audit: "记录审批 ID、仓库、操作和执行结果".into(),
            },
            McpToolPermission {
                tool: "secure_http_write_controlled".into(),
                policy: "HTTP API 非 GET 请求只创建审批请求，固化 requestHash".into(),
                audit: "记录 method、path、requestHash 和审批 ID".into(),
            },
            McpToolPermission {
                tool: "secure_http_write_approved".into(),
                policy: "仅执行 approved 且 requestHash 匹配的 HTTP API 非 GET 请求".into(),
                audit: "记录审批 ID、method、path 和执行结果".into(),
            },
            McpToolPermission {
                tool: "database_connections_list".into(),
                policy: "只读，返回数据库连接脱敏元数据".into(),
                audit: "记录客户端与连接数量".into(),
            },
            McpToolPermission {
                tool: "database_connection_test".into(),
                policy: "只读连通性探测，不返回凭据".into(),
                audit: "记录连接 Key 和测试结果".into(),
            },
            McpToolPermission {
                tool: "database_names_list".into(),
                policy: "只读读取数据库列表".into(),
                audit: "记录连接 Key 和数据库数量".into(),
            },
            McpToolPermission {
                tool: "database_schema_list".into(),
                policy: "只读读取表、字段、视图和索引元数据".into(),
                audit: "记录连接 Key、数据库名和对象数量".into(),
            },
            McpToolPermission {
                tool: "database_sql_query_readonly".into(),
                policy: "仅允许 SELECT/SHOW/DESC/EXPLAIN/WITH 等只读 SQL".into(),
                audit: "记录连接 Key、SQL 摘要、行数和耗时".into(),
            },
            McpToolPermission {
                tool: "database_sql_execute_controlled".into(),
                policy: "只读 SQL 自动执行；写入/DDL SQL 创建审批；禁止 SQL 直接拒绝".into(),
                audit: "记录连接 Key、SQL 摘要、审批 ID 或执行结果".into(),
            },
            McpToolPermission {
                tool: "database_sql_execute_approved".into(),
                policy: "仅执行 approved 且 SQL 与连接匹配的审批请求".into(),
                audit: "记录审批 ID、连接 Key、SQL 类型和执行结果".into(),
            },
            McpToolPermission {
                tool: "database_export_create".into(),
                policy: "按系统默认下载目录导出 CSV/备份文件，不返回凭据".into(),
                audit: "记录连接 Key、导出模式、输出路径和行数".into(),
            },
            McpToolPermission {
                tool: "redis_databases_list".into(),
                policy: "只读读取 Redis DB 编号和 Key 数".into(),
                audit: "记录连接 Key 和 DB 数量".into(),
            },
            McpToolPermission {
                tool: "redis_key_tree".into(),
                policy: "只读扫描 Redis Key，限制返回数量".into(),
                audit: "记录连接 Key、DB、模式和扫描数量".into(),
            },
            McpToolPermission {
                tool: "redis_key_value_preview".into(),
                policy: "只读预览 Redis Key 类型、TTL 和 Value".into(),
                audit: "记录连接 Key、DB 和 Key 名".into(),
            },
            McpToolPermission {
                tool: "redis_command_controlled".into(),
                policy: "只读命令自动执行；写入命令创建审批；危险管理命令直接拒绝".into(),
                audit: "记录连接 Key、命令摘要、审批 ID 或执行结果".into(),
            },
            McpToolPermission {
                tool: "redis_command_approved".into(),
                policy: "仅执行 approved 且命令与连接匹配的 Redis 写入命令".into(),
                audit: "记录审批 ID、连接 Key、命令和执行结果".into(),
            },
            McpToolPermission {
                tool: "ai_skills_list".into(),
                policy: "只读返回 Skill 元数据，不返回正文".into(),
                audit: "记录过滤条件和数量".into(),
            },
            McpToolPermission {
                tool: "ai_skill_detail".into(),
                policy: "只允许读取 allow_mcp=true 的 Skill 正文".into(),
                audit: "记录 Skill Key".into(),
            },
            McpToolPermission {
                tool: "ai_skill_trigger_test".into(),
                policy: "只读测试 MCP 场景 Skill 命中".into(),
                audit: "记录问题摘要和命中数量".into(),
            },
            McpToolPermission {
                tool: "ai_prompt_preview".into(),
                policy: "只读预览 MCP 场景提示词片段".into(),
                audit: "记录问题摘要和注入 Skill 数量".into(),
            },
            McpToolPermission {
                tool: "ai_experiences_list".into(),
                policy: "只读返回经验库条目和 Markdown 路径".into(),
                audit: "记录过滤条件和数量".into(),
            },
            McpToolPermission {
                tool: "ai_experience_upsert_controlled".into(),
                policy: "写入本地 Markdown 经验库，来源标记为 MCP".into(),
                audit: "记录经验标题、Key、标签和 Markdown 路径".into(),
            },
            McpToolPermission {
                tool: "ai_runbooks_list".into(),
                policy: "只读返回 Runbook 元数据，不返回步骤正文".into(),
                audit: "记录过滤条件和数量".into(),
            },
            McpToolPermission {
                tool: "ai_runbook_detail".into(),
                policy: "只允许读取 allow_mcp=true 的 Runbook 步骤详情".into(),
                audit: "记录 Runbook Key".into(),
            },
            McpToolPermission {
                tool: "recall_experience".into(),
                policy: "只读，按场景和问题召回本地经验库 Markdown 摘要".into(),
                audit: "记录作用域、问题摘要和命中数量".into(),
            },
            McpToolPermission {
                tool: "run_runbook".into(),
                policy:
                    "仅允许 allow_mcp=true 的 Runbook；只读步骤自动执行，写入/高风险步骤创建审批"
                        .into(),
                audit: "记录 Runbook、请求方、步骤结果和审批 ID".into(),
            },
            McpToolPermission {
                tool: "ai_runbook_run".into(),
                policy:
                    "run_runbook 的等价别名；仅允许 allow_mcp=true 的 Runbook，危险步骤按审批策略处理"
                        .into(),
                audit: "记录 Runbook、请求方、步骤结果和审批 ID".into(),
            },
            McpToolPermission {
                tool: "ai_skill_enable_controlled".into(),
                policy: "只创建启用/禁用 Skill 的审批请求，不直接改状态".into(),
                audit: "记录 Skill Key、目标状态和审批 ID".into(),
            },
            McpToolPermission {
                tool: "ai_skill_enable_approved".into(),
                policy: "仅执行 approved 且 Skill Key、目标状态匹配的启停变更".into(),
                audit: "记录审批 ID、Skill Key 和目标状态".into(),
            },
            McpToolPermission {
                tool: "ai_skill_copy_controlled".into(),
                policy: "复制 Skill 为用户副本，便于修改后再启用".into(),
                audit: "记录来源 Skill Key 和新副本 Key".into(),
            },
            McpToolPermission {
                tool: "approval_requests_list".into(),
                policy: "只读，查看本地审批队列状态".into(),
                audit: "记录客户端、状态过滤和数量".into(),
            },
            McpToolPermission {
                tool: "approval_request_create".into(),
                policy: "仅创建 pending 审批请求，不执行真实远程动作".into(),
                audit: "记录请求方、服务器、动作和风险级别".into(),
            },
            McpToolPermission {
                tool: "ai_policy_evaluate_command".into(),
                policy: "只读，按服务器 AI 权限级别返回策略决策".into(),
                audit: "记录服务器、命令摘要、风险和决策".into(),
            },
            McpToolPermission {
                tool: "terminal_execute_controlled".into(),
                policy: "只读自动执行；需审核动作只创建审批；禁止动作直接拒绝".into(),
                audit: "记录策略决策、审批 ID 或执行结果".into(),
            },
            McpToolPermission {
                tool: "terminal_execute_approved".into(),
                policy: "仅执行 approved 且命令匹配的审批请求".into(),
                audit: "记录审批 ID、服务器、命令和退出码".into(),
            },
            McpToolPermission {
                tool: "sftp_write_text_controlled".into(),
                policy: "仅创建远程文本写入审批，审批中记录内容 SHA-256".into(),
                audit: "记录服务器、路径、内容哈希和审批 ID".into(),
            },
            McpToolPermission {
                tool: "sftp_write_text_approved".into(),
                policy: "仅允许 approved 且路径、内容哈希匹配的文本写入".into(),
                audit: "记录审批 ID、服务器、路径和字节数".into(),
            },
            McpToolPermission {
                tool: "sftp_create_directory_controlled".into(),
                policy: "仅创建远程目录创建审批，不直接写远端".into(),
                audit: "记录服务器、路径和审批 ID".into(),
            },
            McpToolPermission {
                tool: "sftp_create_directory_approved".into(),
                policy: "仅允许 approved 且路径匹配的目录创建".into(),
                audit: "记录审批 ID、服务器和路径".into(),
            },
            McpToolPermission {
                tool: "sftp_create_file_controlled".into(),
                policy: "仅创建远程文件创建审批，审批中记录初始内容 SHA-256".into(),
                audit: "记录服务器、路径、内容哈希和审批 ID".into(),
            },
            McpToolPermission {
                tool: "sftp_create_file_approved".into(),
                policy: "仅允许 approved 且路径、内容哈希匹配的文件创建".into(),
                audit: "记录审批 ID、服务器、路径和字节数".into(),
            },
            McpToolPermission {
                tool: "sftp_rename_controlled".into(),
                policy: "仅创建远程路径重命名审批，不直接改远端".into(),
                audit: "记录服务器、源路径、目标路径和审批 ID".into(),
            },
            McpToolPermission {
                tool: "sftp_rename_approved".into(),
                policy: "仅允许 approved 且源路径、目标路径匹配的重命名".into(),
                audit: "记录审批 ID、服务器、源路径和目标路径".into(),
            },
            McpToolPermission {
                tool: "sftp_delete_controlled".into(),
                policy: "仅创建远程文件/空目录删除审批，不直接删除".into(),
                audit: "记录服务器、路径、类型和审批 ID".into(),
            },
            McpToolPermission {
                tool: "sftp_delete_approved".into(),
                policy: "仅允许 approved 且路径、类型匹配的文件/空目录删除".into(),
                audit: "记录审批 ID、服务器、路径和类型".into(),
            },
            McpToolPermission {
                tool: "server_groups_list".into(),
                policy: "只读，返回服务器分组统计".into(),
                audit: "记录客户端和分组数量".into(),
            },
            McpToolPermission {
                tool: "server_group_inventory".into(),
                policy: "只读，返回指定分组脱敏连接资料".into(),
                audit: "记录分组名和服务器数量".into(),
            },
            McpToolPermission {
                tool: "ssh_connection_profile".into(),
                policy: "只读，返回单台服务器连接资料，不含凭据明文".into(),
                audit: "记录服务器别名".into(),
            },
            McpToolPermission {
                tool: "ssh_connection_profiles".into(),
                policy: "只读，批量返回脱敏连接资料".into(),
                audit: "记录筛选条件和返回数量".into(),
            },
            McpToolPermission {
                tool: "ssh_command_generate".into(),
                policy: "只生成命令模板，不嵌入密码或 token".into(),
                audit: "记录服务器别名和是否包含远程命令".into(),
            },
            McpToolPermission {
                tool: "openssh_config_generate".into(),
                policy: "生成不含密钥内容和密码的 OpenSSH Config 片段".into(),
                audit: "记录服务器/分组筛选和生成数量".into(),
            },
            McpToolPermission {
                tool: "credential_access_request_create".into(),
                policy: "仅创建凭据访问审批请求，不返回凭据明文".into(),
                audit: "记录服务器、凭据引用和请求方".into(),
            },
            McpToolPermission {
                tool: "credential_access_status".into(),
                policy: "只读审批状态；即使通过也不返回凭据明文".into(),
                audit: "记录审批 ID 和状态".into(),
            },
        ];
        tools.extend(secure_credential_semantic_tool_permissions());
        tools
    }

    pub fn configure(input: ConfigureMcpClientInput) -> Result<ConfigureMcpClientResult, AppError> {
        let client_key = input.client_key.trim();
        if client_key.is_empty() {
            return Err(AppError::InvalidInput("客户端 Key 不能为空".into()));
        }
        let template = client_templates()
            .into_iter()
            .find(|item| item.key == client_key)
            .ok_or_else(|| AppError::NotFound(format!("不支持的 MCP 客户端：{}", client_key)))?;
        let config_path = resolve_config_path(template.relative_config_path)?;
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let backup_path = backup_file(&config_path)?;
        let status = Self::status();
        let transport = input.transport.as_deref().unwrap_or(template.transport);
        let snippet = match template.key {
            "codex" => write_codex_config(&config_path, &status)?,
            "zed" => write_zed_config(&config_path, &status)?,
            "opencode" => write_opencode_config(&config_path, &status)?,
            _ => write_generic_mcp_json(&config_path, &status, transport)?,
        };
        let client = client_from_template(&template)?;
        Ok(ConfigureMcpClientResult {
            client,
            config_path: config_path.to_string_lossy().into_owned(),
            backup_path: backup_path.map(|path| path.to_string_lossy().into_owned()),
            message: format!("{} 已接入 {}", template.name, MCP_SERVER_KEY),
            snippet,
        })
    }

    fn manual_snippets(status: &McpServerStatus) -> Vec<McpManualSnippet> {
        vec![
            McpManualSnippet {
                title: "stdio 接入（Claude Code / Cursor / Cline / OpenCode 通用）".into(),
                transport: "stdio".into(),
                content: serde_json::to_string_pretty(&json!({
                    "mcpServers": {
                        MCP_SERVER_KEY: stdio_config(status)
                    }
                }))
                .unwrap_or_default(),
            },
            McpManualSnippet {
                title: "Streamable HTTP 接入（支持 HTTP MCP 的客户端）".into(),
                transport: "http".into(),
                content: serde_json::to_string_pretty(&json!({
                    "mcpServers": {
                        MCP_SERVER_KEY: {
                            "url": status.streamable_http_url
                        }
                    }
                }))
                .unwrap_or_default(),
            },
            McpManualSnippet {
                title: "Codex config.toml 片段".into(),
                transport: "http".into(),
                content: codex_block(status),
            },
        ]
    }
}

fn secure_credential_semantic_tool_permissions() -> Vec<McpToolPermission> {
    let mut tools = Vec::new();
    for tool in ["secure_credential_detail", "secure_credential_audit_list"] {
        tools.push(McpToolPermission {
            tool: tool.into(),
            policy: "只读，返回安全凭证脱敏详情或审计记录".into(),
            audit: "记录筛选条件和返回数量".into(),
        });
    }
    for tool in [
        "github_repos_list",
        "github_repo_detail",
        "github_branches_list",
        "github_file_read",
        "github_commits_list",
        "github_pull_requests_list",
        "github_issues_list",
        "github_releases_list",
        "github_tags_list",
        "gitlab_projects_list",
        "gitlab_project_detail",
        "gitlab_branches_list",
        "gitlab_file_read",
        "gitlab_commits_list",
        "gitlab_issues_list",
        "gitlab_merge_requests_list",
        "gitlab_releases_list",
        "gitlab_tags_list",
        "gitcode_repos_list",
        "gitcode_repo_detail",
        "gitcode_branches_list",
        "gitcode_file_read",
        "gitcode_commits_list",
        "gitcode_merge_requests_list",
        "gitee_repos_list",
        "gitee_repo_detail",
        "gitee_branches_list",
        "gitee_file_read",
        "gitee_commits_list",
        "gitee_pull_requests_list",
        "gitee_issues_list",
        "gitee_releases_list",
        "gitee_tags_list",
        "http_api_request_readonly",
    ] {
        tools.push(McpToolPermission {
            tool: tool.into(),
            policy: "只读，必须提供 sessionId，后端代调用 Provider，不返回 Token".into(),
            audit: "记录 sessionId、Provider、资源类型和调用结果".into(),
        });
    }
    for tool in [
        "github_issue_create_controlled",
        "github_branch_create_controlled",
        "github_file_commit_controlled",
        "github_pull_request_create_controlled",
        "github_pull_request_update_controlled",
        "github_pull_request_merge_controlled",
        "github_tag_create_controlled",
        "github_release_create_controlled",
        "github_workflow_dispatch_controlled",
        "gitlab_issue_create_controlled",
        "gitlab_branch_create_controlled",
        "gitlab_file_commit_controlled",
        "gitlab_merge_request_create_controlled",
        "gitlab_merge_request_update_controlled",
        "gitlab_merge_request_merge_controlled",
        "gitlab_tag_create_controlled",
        "gitlab_release_create_controlled",
        "gitlab_pipeline_trigger_controlled",
        "gitcode_issue_create_controlled",
        "gitcode_branch_create_controlled",
        "gitcode_file_commit_controlled",
        "gitcode_merge_request_create_controlled",
        "gitcode_merge_request_merge_controlled",
        "gitcode_tag_create_controlled",
        "gitcode_release_create_controlled",
        "gitee_issue_create_controlled",
        "gitee_branch_create_controlled",
        "gitee_file_commit_controlled",
        "gitee_pull_request_create_controlled",
        "gitee_pull_request_update_controlled",
        "gitee_pull_request_merge_controlled",
        "gitee_tag_create_controlled",
        "gitee_release_create_controlled",
        "http_api_request_controlled",
        "secure_credential_rotate_request",
    ] {
        tools.push(McpToolPermission {
            tool: tool.into(),
            policy: "受控写操作，只创建审批请求，不直接执行真实写入".into(),
            audit: "记录 payload requestHash、审批 ID 和资源摘要".into(),
        });
    }
    for tool in [
        "github_branch_delete_controlled",
        "github_tag_delete_controlled",
        "github_release_delete_controlled",
        "github_ref_update_controlled",
        "github_repository_settings_update_controlled",
        "gitlab_branch_delete_controlled",
        "gitlab_tag_delete_controlled",
        "gitlab_release_delete_controlled",
        "gitlab_project_settings_update_controlled",
        "gitcode_branch_delete_controlled",
        "gitcode_tag_delete_controlled",
        "gitcode_release_delete_controlled",
        "gitcode_repository_settings_update_controlled",
        "gitee_branch_delete_controlled",
        "gitee_tag_delete_controlled",
        "gitee_release_delete_controlled",
        "gitee_repository_settings_update_controlled",
    ] {
        tools.push(McpToolPermission {
            tool: tool.into(),
            policy: "高风险仓库操作，只创建审批请求；approved 后仍受策略页开关限制".into(),
            audit: "记录高风险动作、requestHash、审批 ID 和策略拒绝原因".into(),
        });
    }
    tools
}

fn client_templates() -> Vec<McpClientTemplate> {
    vec![
        McpClientTemplate {
            key: "claude-code",
            name: "Claude Code",
            vendor: "Anthropic",
            description: "Anthropic 官方 CLI，写入用户级 ~/.claude.json。",
            scope: "user",
            transport: "stdio",
            relative_config_path: ".claude.json",
        },
        McpClientTemplate {
            key: "codex",
            name: "Codex",
            vendor: "OpenAI",
            description: "Codex CLI / Desktop，写入 ~/.codex/config.toml。",
            scope: "user",
            transport: "http",
            relative_config_path: ".codex/config.toml",
        },
        McpClientTemplate {
            key: "cursor",
            name: "Cursor",
            vendor: "Anysphere",
            description: "AI 编辑器，写入 ~/.cursor/mcp.json。",
            scope: "user",
            transport: "stdio",
            relative_config_path: ".cursor/mcp.json",
        },
        McpClientTemplate {
            key: "cline",
            name: "Cline",
            vendor: "Cline",
            description: "VS Code AI 编程插件，写入 ~/.cline/mcp.json。",
            scope: "user",
            transport: "stdio",
            relative_config_path: ".cline/mcp.json",
        },
        McpClientTemplate {
            key: "zed",
            name: "Zed Editor",
            vendor: "Zed",
            description: "Rust 高性能编辑器，写入 ~/.config/zed/settings.json。",
            scope: "user",
            transport: "stdio",
            relative_config_path: ".config/zed/settings.json",
        },
        McpClientTemplate {
            key: "opencode",
            name: "OpenCode",
            vendor: "SST",
            description: "开源多模型 CLI，写入 ~/.config/opencode/opencode.json。",
            scope: "user",
            transport: "stdio",
            relative_config_path: ".config/opencode/opencode.json",
        },
        McpClientTemplate {
            key: "continue",
            name: "Continue",
            vendor: "Continue",
            description: "VS Code / JetBrains AI 编程助手，写入 ~/.continue/config.json。",
            scope: "user",
            transport: "stdio",
            relative_config_path: ".continue/config.json",
        },
        McpClientTemplate {
            key: "windsurf",
            name: "Windsurf",
            vendor: "Codeium",
            description: "Windsurf AI 编辑器，写入 ~/.codeium/windsurf/mcp_config.json。",
            scope: "user",
            transport: "stdio",
            relative_config_path: ".codeium/windsurf/mcp_config.json",
        },
        McpClientTemplate {
            key: "roo-code",
            name: "Roo Code",
            vendor: "Roo Code",
            description: "VS Code Agent 插件，写入 ~/.roo/mcp.json。",
            scope: "user",
            transport: "stdio",
            relative_config_path: ".roo/mcp.json",
        },
        McpClientTemplate {
            key: "qwen-code",
            name: "Qwen Code",
            vendor: "Alibaba",
            description: "通义系 CLI / Agent 工具，写入 ~/.qwen/mcp.json。",
            scope: "user",
            transport: "stdio",
            relative_config_path: ".qwen/mcp.json",
        },
    ]
}

fn client_from_template(template: &McpClientTemplate) -> Result<McpClientConfig, AppError> {
    let path = resolve_config_path(template.relative_config_path)?;
    let configured = is_configured(template.key, &path);
    let installed = path.exists() || path.parent().is_some_and(Path::exists);
    let last_configured_at = if configured {
        path.metadata()
            .and_then(|metadata| metadata.modified())
            .ok()
            .map(|modified| {
                let datetime: chrono::DateTime<Local> = modified.into();
                datetime.format("%Y-%m-%d %H:%M:%S").to_string()
            })
    } else {
        None
    };
    Ok(McpClientConfig {
        key: template.key.into(),
        name: template.name.into(),
        vendor: template.vendor.into(),
        description: template.description.into(),
        config_path: path.to_string_lossy().into_owned(),
        scope: template.scope.into(),
        transport: template.transport.into(),
        status: if configured {
            "configured".into()
        } else if installed {
            "available".into()
        } else {
            "not_found".into()
        },
        installed,
        configured,
        last_configured_at,
        notes: vec!["写入前自动备份；仅更新 tauri-ssh 条目。".into()],
    })
}

fn resolve_config_path(relative: &str) -> Result<PathBuf, AppError> {
    Ok(home_dir()?.join(relative))
}

fn home_dir() -> Result<PathBuf, AppError> {
    if let Some(home) = std::env::var_os("HOME") {
        return Ok(PathBuf::from(home));
    }
    if let Some(home) = std::env::var_os("USERPROFILE") {
        return Ok(PathBuf::from(home));
    }
    Err(AppError::Custom("无法识别用户 Home 目录".into()))
}

fn tcp_reachable(addr: &str) -> bool {
    addr.parse()
        .ok()
        .and_then(|socket_addr| {
            std::net::TcpStream::connect_timeout(&socket_addr, Duration::from_millis(250)).ok()
        })
        .is_some()
}

fn backup_file(path: &Path) -> Result<Option<PathBuf>, AppError> {
    if !path.exists() {
        return Ok(None);
    }
    let backup = path.with_extension(format!(
        "{}.bak.{}",
        path.extension()
            .and_then(|item| item.to_str())
            .unwrap_or("config"),
        Local::now().format("%Y%m%d%H%M%S")
    ));
    fs::copy(path, &backup)?;
    Ok(Some(backup))
}

fn read_json_object(path: &Path) -> Result<Value, AppError> {
    if !path.exists() {
        return Ok(json!({}));
    }
    let content = fs::read_to_string(path)?;
    if content.trim().is_empty() {
        return Ok(json!({}));
    }
    serde_json::from_str(&content).map_err(Into::into)
}

fn write_json_pretty(path: &Path, value: &Value) -> Result<String, AppError> {
    let content = serde_json::to_string_pretty(value)?;
    fs::write(path, format!("{}\n", content))?;
    Ok(content)
}

fn ensure_object_field<'a>(value: &'a mut Value, key: &str) -> &'a mut Map<String, Value> {
    if !value.is_object() {
        *value = json!({});
    }
    let object = value.as_object_mut().expect("json object");
    object.entry(key.to_string()).or_insert_with(|| json!({}));
    object
        .get_mut(key)
        .and_then(Value::as_object_mut)
        .expect("json object field")
}

fn stdio_config(status: &McpServerStatus) -> Value {
    json!({
        "command": status.stdio_command,
        "args": status.stdio_args,
        "env": {
            "TAURI_SSH_MCP_HTTP": status.streamable_http_url
        }
    })
}

fn write_generic_mcp_json(
    path: &Path,
    status: &McpServerStatus,
    transport: &str,
) -> Result<String, AppError> {
    let mut value = read_json_object(path)?;
    let servers = ensure_object_field(&mut value, "mcpServers");
    let config = if transport == "http" {
        json!({ "url": status.streamable_http_url })
    } else {
        stdio_config(status)
    };
    servers.insert(MCP_SERVER_KEY.into(), config);
    write_json_pretty(path, &value)
}

fn write_zed_config(path: &Path, status: &McpServerStatus) -> Result<String, AppError> {
    let mut value = read_json_object(path)?;
    let servers = ensure_object_field(&mut value, "context_servers");
    servers.insert(
        MCP_SERVER_KEY.into(),
        json!({
            "command": {
                "path": status.stdio_command,
                "args": status.stdio_args,
                "env": {
                    "TAURI_SSH_MCP_HTTP": status.streamable_http_url
                }
            }
        }),
    );
    write_json_pretty(path, &value)
}

fn write_opencode_config(path: &Path, status: &McpServerStatus) -> Result<String, AppError> {
    let mut value = read_json_object(path)?;
    let servers = ensure_object_field(&mut value, "mcp");
    servers.insert(
        MCP_SERVER_KEY.into(),
        json!({
            "type": "local",
            "command": status.stdio_command,
            "args": status.stdio_args,
            "enabled": true
        }),
    );
    write_json_pretty(path, &value)
}

fn write_codex_config(path: &Path, status: &McpServerStatus) -> Result<String, AppError> {
    let existing = if path.exists() {
        fs::read_to_string(path)?
    } else {
        String::new()
    };
    let cleaned = remove_codex_block(&existing);
    let block = codex_block(status);
    let content = if cleaned.trim().is_empty() {
        format!("{}\n", block)
    } else {
        format!("{}\n\n{}\n", cleaned.trim_end(), block)
    };
    fs::write(path, &content)?;
    Ok(content)
}

fn codex_block(status: &McpServerStatus) -> String {
    format!(
        "[mcp_servers.{MCP_SERVER_KEY}]\nenabled = true\nurl = \"{}\"\n",
        status.streamable_http_url
    )
}

fn remove_codex_block(content: &str) -> String {
    let mut output = Vec::new();
    let mut skipping = false;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed == format!("[mcp_servers.{MCP_SERVER_KEY}]") {
            skipping = true;
            continue;
        }
        if skipping && trimmed.starts_with('[') && trimmed.ends_with(']') {
            skipping = false;
        }
        if !skipping {
            output.push(line);
        }
    }
    output.join("\n")
}

fn is_configured(client_key: &str, path: &Path) -> bool {
    if !path.exists() {
        return false;
    }
    let Ok(content) = fs::read_to_string(path) else {
        return false;
    };
    if client_key == "codex" {
        return content.contains(&format!("[mcp_servers.{MCP_SERVER_KEY}]"));
    }
    content.contains(MCP_SERVER_KEY)
}
