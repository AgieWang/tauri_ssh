mod commands;
mod database;
mod dev_server;
mod error;
mod models;
mod remote;
mod services;
pub mod shared;
mod state;
mod tray;

use state::AppState;
use tauri::{Manager, WindowEvent};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // ─── 插件注册 ───────────────────────────────
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(
            tauri_plugin_log::Builder::default()
                // 开发环境日志更详细，生产环境只记录 Warn 及以上
                .level(if cfg!(debug_assertions) {
                    log::LevelFilter::Info
                } else {
                    log::LevelFilter::Warn
                })
                .build(),
        )
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        // ─── 应用初始化 ─────────────────────────────
        .setup(|app| {
            // 初始化数据库（存放在应用数据目录）
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;

            // 开发与生产使用同一 app_data_dir，但用不同文件名隔离数据
            // 避免开发环境污染生产环境的真实数据（DB / 同名锁文件等）
            let db_filename = if cfg!(debug_assertions) {
                "dev-app.db"
            } else {
                "app.db"
            };
            let db_path = data_dir.join(db_filename);
            let db_path_str = db_path.to_string_lossy().to_string();

            let db = database::Database::init(&db_path_str)
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;

            log::info!("数据库初始化完成: {}", db_path_str);

            if let Err(err) = services::ai_skill::AiSkillService::sync_builtin(app.handle(), &db) {
                log::warn!("内置 Skill 同步失败: {}", err);
            }

            // 注册全局状态
            app.manage(AppState::new(db));

            let startup_recovery_app = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let state = startup_recovery_app.state::<AppState>();
                if let Err(err) =
                    services::jenkins::JenkinsService::recover_unfinished_runs_on_startup(
                        &startup_recovery_app,
                        &state.db,
                    )
                    .await
                {
                    log::warn!("Jenkins 未完成构建启动恢复失败: {}", err);
                }
            });

            dev_server::start(app.handle().clone());

            // 初始化系统托盘
            tray::setup_tray(app)?;
            log::info!("系统托盘初始化完成");

            // 开发模式下给窗口标题加 [DEV] 后缀，避免与生产版本混淆
            #[cfg(debug_assertions)]
            if let Some(window) = app.get_webview_window("main") {
                if let Ok(current_title) = window.title() {
                    let _ = window.set_title(&format!("{} [DEV]", current_title));
                }
            }

            Ok(())
        })
        // ─── Command 注册 ───────────────────────────
        .invoke_handler(tauri::generate_handler![
            // 系统模块
            commands::system::greet,
            commands::system::get_system_info,
            // 配置模块
            commands::config::get_all_config,
            commands::config::get_config,
            commands::config::set_config,
            commands::config::delete_config,
            // 系统设置模块
            commands::system_settings::get_system_settings,
            commands::system_settings::update_system_settings,
            commands::system_settings::reset_system_settings,
            commands::system_settings::export_system_settings,
            commands::system_settings::get_ai_unrestricted_state,
            commands::system_settings::enable_ai_unrestricted_mode,
            commands::system_settings::disable_ai_unrestricted_mode,
            // AI Provider 模块
            commands::ai_provider::list_ai_providers,
            commands::ai_provider::upsert_ai_provider,
            commands::ai_provider::delete_ai_provider,
            commands::ai_provider::list_ai_provider_routes,
            commands::ai_provider::upsert_ai_provider_route,
            commands::ai_provider::test_ai_provider,
            commands::ai_provider::list_ai_provider_models,
            commands::ai_provider::ask_ai_provider,
            // AI Skill 管理模块
            commands::ai_skill::sync_builtin_ai_skills,
            commands::ai_skill::list_ai_skills,
            commands::ai_skill::upsert_ai_skill,
            commands::ai_skill::set_ai_skill_enabled,
            commands::ai_skill::copy_ai_skill,
            commands::ai_skill::delete_ai_skill,
            commands::ai_skill::restore_builtin_ai_skill,
            commands::ai_skill::test_ai_skill_trigger,
            commands::ai_skill::preview_ai_skill_prompt,
            commands::ai_skill::list_ai_experiences,
            commands::ai_skill::recall_ai_experiences,
            commands::ai_skill::upsert_ai_experience,
            commands::ai_skill::delete_ai_experience,
            commands::ai_skill::list_ai_runbooks,
            commands::ai_skill::upsert_ai_runbook,
            commands::ai_skill::run_ai_runbook,
            commands::ai_skill::delete_ai_runbook,
            // SSH 服务器模块
            commands::ssh_server::list_ssh_servers,
            commands::ssh_server::upsert_ssh_server,
            commands::ssh_server::delete_ssh_server,
            commands::ssh_server::import_ssh_config,
            commands::ssh_server::test_ssh_server,
            commands::ssh_server::test_ssh_server_connection,
            // 凭据保险库模块
            commands::credential_vault::list_credentials,
            commands::credential_vault::upsert_credential,
            commands::credential_vault::authorize_credential,
            commands::credential_vault::rotate_credential,
            commands::credential_vault::delete_credential,
            // 安全凭证模块
            commands::secure_credential::get_secure_credential_overview,
            commands::secure_credential::list_secure_credential_audit_logs,
            commands::secure_credential::get_secure_credential_policy_settings,
            commands::secure_credential::update_secure_credential_policy_settings,
            commands::secure_credential::list_secure_credentials,
            commands::secure_credential::upsert_secure_credential,
            commands::secure_credential::rotate_secure_credential,
            commands::secure_credential::set_secure_credential_enabled,
            commands::secure_credential::delete_secure_credential,
            commands::secure_credential::list_secure_credential_sessions,
            commands::secure_credential::create_secure_credential_session,
            commands::secure_credential::get_secure_credential_session_status,
            commands::secure_credential::revoke_secure_credential_session,
            commands::secure_credential::test_secure_credential_provider,
            commands::secure_credential::list_secure_credential_repositories,
            commands::secure_credential::secure_credential_git_readonly_request,
            commands::secure_credential::secure_credential_http_readonly_request,
            commands::secure_credential::secure_credential_http_write_request,
            commands::secure_credential::execute_secure_credential_git_write,
            // Git 工作区模块
            commands::git_workspace::list_git_workspaces,
            commands::git_workspace::upsert_git_workspace,
            commands::git_workspace::delete_git_workspace,
            commands::git_workspace::refresh_git_workspace,
            commands::git_workspace::get_git_workspace_detail,
            commands::git_workspace::scan_git_workspace_root,
            commands::git_workspace::start_git_workspace_root_scan,
            commands::git_workspace::get_git_workspace_scan_status,
            commands::git_workspace::ai_commit_git_workspace,
            commands::git_workspace::get_git_workspace_status,
            commands::git_workspace::get_git_workspace_diff,
            commands::git_workspace::stage_git_workspace_files,
            commands::git_workspace::commit_git_workspace,
            commands::git_workspace::pull_git_workspace,
            commands::git_workspace::push_git_workspace,
            commands::git_workspace::list_git_workspace_branches,
            commands::git_workspace::switch_git_workspace_branch,
            commands::git_workspace::merge_git_workspace_branch,
            // 代码审核模块
            commands::code_review::list_code_review_tasks,
            commands::code_review::get_code_review_task,
            commands::code_review::create_code_review_task,
            commands::code_review::create_code_review_batch_tasks,
            commands::code_review::prepare_code_review_diff,
            commands::code_review::run_code_review_ai,
            commands::code_review::merge_code_review_task,
            commands::code_review::push_code_review_task,
            commands::code_review::abort_code_review_merge,
            commands::code_review::cancel_code_review_task,
            commands::code_review::parse_code_review_batch,
            // 数据库管理模块
            commands::database_ops::list_database_connections,
            commands::database_ops::upsert_database_connection,
            commands::database_ops::delete_database_connection,
            commands::database_ops::test_database_connection,
            commands::database_ops::execute_database_readonly_query,
            commands::database_ops::list_database_names,
            commands::database_ops::list_database_schema,
            commands::database_ops::execute_database_sql,
            commands::database_ops::execute_database_sql_batch,
            commands::database_ops::export_database,
            commands::database_ops::scan_redis_keys,
            commands::database_ops::describe_redis_keys,
            commands::database_ops::list_redis_databases,
            commands::database_ops::list_redis_key_tree,
            commands::database_ops::get_redis_value_preview,
            // 自动部署模块
            commands::deployment::list_deployment_templates,
            commands::deployment::list_deployment_environment_profiles,
            commands::deployment::list_deployment_image_store_apps,
            commands::deployment::install_deployment_image_store_app,
            commands::deployment::detect_deployment_project,
            commands::deployment::list_deployment_targets,
            commands::deployment::upsert_deployment_target,
            commands::deployment::delete_deployment_target,
            commands::deployment::list_deployment_groups,
            commands::deployment::upsert_deployment_group,
            commands::deployment::delete_deployment_group,
            commands::deployment::create_deployment_dry_run,
            commands::deployment::execute_deployment_run,
            commands::deployment::list_deployment_runs,
            commands::deployment::get_deployment_run_detail,
            commands::deployment::create_deployment_rollback_dry_run,
            commands::deployment::execute_deployment_rollback,
            commands::deployment::ask_deployment_ai_advice,
            // Jenkins 构建运维模块
            commands::jenkins::list_jenkins_connections,
            commands::jenkins::upsert_jenkins_connection,
            commands::jenkins::delete_jenkins_connection,
            commands::jenkins::restore_jenkins_connection,
            commands::jenkins::duplicate_jenkins_connection,
            commands::jenkins::test_jenkins_connection,
            commands::jenkins::list_jenkins_jobs,
            commands::jenkins::get_jenkins_job_detail,
            commands::jenkins::set_jenkins_job_favorite,
            commands::jenkins::list_jenkins_builds,
            commands::jenkins::sync_unfinished_jenkins_runs,
            commands::jenkins::list_jenkins_parameters,
            commands::jenkins::list_jenkins_recent_parameter_values,
            commands::jenkins::forget_jenkins_recent_parameter_value,
            commands::jenkins::list_jenkins_parameter_templates,
            commands::jenkins::upsert_jenkins_parameter_template,
            commands::jenkins::delete_jenkins_parameter_template,
            commands::jenkins::verify_jenkins_parameter_definition_hash,
            commands::jenkins::inspect_jenkins_file_parameter,
            commands::jenkins::create_jenkins_build_trigger_approval,
            commands::jenkins::execute_jenkins_build_trigger_approved,
            commands::jenkins::trigger_jenkins_build_without_approval,
            commands::jenkins::create_jenkins_build_stop_approval,
            commands::jenkins::execute_jenkins_build_stop_approved,
            commands::jenkins::stop_jenkins_build_without_approval,
            commands::jenkins::get_jenkins_build_detail,
            commands::jenkins::read_jenkins_build_log,
            commands::jenkins::record_jenkins_log_copy_audit,
            commands::jenkins::generate_jenkins_failure_analysis,
            commands::jenkins::get_latest_jenkins_build_analysis,
            commands::jenkins::list_jenkins_artifacts,
            commands::jenkins::download_jenkins_artifact,
            commands::jenkins::cleanup_jenkins_artifact_local_file,
            commands::jenkins::create_jenkins_artifact_deployment_candidate,
            commands::jenkins::create_jenkins_build_deployment_dry_run,
            commands::jenkins::list_jenkins_queue,
            commands::jenkins::poll_jenkins_queue_item,
            // 资源监控模块
            commands::resource_monitor::list_resource_monitor_targets,
            commands::resource_monitor::upsert_resource_monitor_target,
            commands::resource_monitor::delete_resource_monitor_target,
            commands::resource_monitor::get_resource_monitor_overview,
            commands::resource_monitor::list_resource_metric_snapshots,
            commands::resource_monitor::collect_server_resource_snapshot,
            commands::resource_monitor::collect_database_resource_snapshot,
            commands::resource_monitor::collect_redis_resource_snapshot,
            commands::resource_monitor::collect_resource_snapshots_batch,
            commands::resource_monitor::list_mysql_slow_queries,
            commands::resource_monitor::kill_mysql_query,
            commands::resource_monitor::list_resource_alert_rules,
            commands::resource_monitor::upsert_resource_alert_rule,
            commands::resource_monitor::delete_resource_alert_rule,
            commands::resource_monitor::list_resource_alert_events,
            commands::resource_monitor::resolve_resource_alert_event,
            // MCP Server 模块
            commands::mcp::get_mcp_overview,
            commands::mcp::configure_mcp_client,
            // 审批队列模块
            commands::approval::list_approval_requests,
            commands::approval::create_approval_request,
            commands::approval::decide_approval_request,
            // 审计日志模块
            commands::audit::list_audit_logs,
            commands::audit::create_audit_log,
            commands::audit::export_audit_logs,
            // 堡垒机会话模块
            commands::jumpserver::list_jumpserver_sessions,
            commands::jumpserver::upsert_jumpserver_session,
            commands::jumpserver::open_jumpserver_session,
            commands::jumpserver::delete_jumpserver_session,
            // 终端模块
            commands::terminal::execute_terminal_command,
            commands::terminal::start_terminal_session,
            commands::terminal::write_terminal_session,
            commands::terminal::resize_terminal_session,
            commands::terminal::close_terminal_session,
            // SFTP 文件模块
            commands::sftp::sftp_list,
            commands::sftp::sftp_read_text,
            commands::sftp::sftp_write_text,
            commands::sftp::sftp_upload,
            commands::sftp::sftp_download,
            commands::sftp::sftp_create_directory,
            commands::sftp::sftp_create_file,
            commands::sftp::sftp_rename,
            commands::sftp::sftp_delete,
        ])
        // ─── 窗口事件处理 ─────────────────────────
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                // 点击关闭按钮时隐藏到托盘，而不是退出
                let _ = window.hide();
                api.prevent_close();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
