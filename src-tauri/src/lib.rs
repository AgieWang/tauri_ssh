mod commands;
mod database;
#[cfg(debug_assertions)]
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

            #[cfg(debug_assertions)]
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
