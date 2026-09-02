mod commands;
mod database;
pub(crate) mod dev_server;
mod error;
mod models;
mod remote;
mod services;
pub mod shared;
mod state;
mod tray;

use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use state::AppState;
use tauri::{webview::PageLoadEvent, Manager, WindowEvent};

const STARTUP_WINDOW_FALLBACK_TIMEOUT: Duration = Duration::from_secs(8);

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // 正常首屏完成和超时兜底共享一次性展示状态，避免用户主动隐藏后被定时任务重新打开。
    let startup_window_shown = Arc::new(AtomicBool::new(false));
    let page_load_window_shown = Arc::clone(&startup_window_shown);

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
        // 主窗口初始隐藏，等 HTML、样式和首屏脚本加载完成后再显示，避免暴露 WebView 白底。
        // 即使 React 执行失败，index.html 内联的启动占位也会随 Finished 事件显示。
        .on_page_load(move |webview, payload| {
            if webview.label() == "main"
                && matches!(payload.event(), PageLoadEvent::Finished)
                && page_load_window_shown
                    .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                    .is_ok()
            {
                match webview.window().show() {
                    Ok(()) => log::info!("主窗口首屏加载完成并已显示"),
                    Err(error) => {
                        page_load_window_shown.store(false, Ordering::Release);
                        log::warn!("主窗口首屏加载后显示失败: {}", error);
                    }
                }
            }
        })
        // ─── 应用初始化 ─────────────────────────────
        .setup(move |app| {
            let setup_started = Instant::now();

            // 页面加载异常时不能让初始隐藏的窗口永久不可见。正常启动由 on_page_load 立即显示，
            // 这里的有界兜底只在 WebView 未进入 Finished 时触发，show 调用本身是幂等的。
            let startup_window_app = app.handle().clone();
            let fallback_window_shown = Arc::clone(&startup_window_shown);
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(STARTUP_WINDOW_FALLBACK_TIMEOUT).await;
                if fallback_window_shown
                    .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                    .is_ok()
                {
                    if let Some(window) = startup_window_app.get_webview_window("main") {
                        match window.show() {
                            Ok(()) => log::warn!("主窗口首屏加载超时，已执行可见性兜底"),
                            Err(error) => {
                                fallback_window_shown.store(false, Ordering::Release);
                                log::warn!("主窗口超时兜底显示失败: {}", error);
                            }
                        }
                    } else {
                        fallback_window_shown.store(false, Ordering::Release);
                    }
                }
            });

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

            let database_started = Instant::now();
            let db = database::Database::init(&db_path_str)
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;

            log::info!(
                "数据库初始化完成: {}，耗时 {} ms",
                db_path_str,
                database_started.elapsed().as_millis()
            );

            let recovery_started = Instant::now();
            match services::knowledge::KnowledgeService::recover_interrupted_jobs(&db) {
                Ok(count) if count > 0 => {
                    log::info!("已恢复 {} 个中断的知识任务，等待用户重试", count);
                }
                Ok(_) => {}
                Err(error) => log::warn!("知识任务启动恢复失败: {}", error),
            }
            match services::knowledge_domain::analysis::KnowledgeAnalysisService::recover_interrupted_state(&db) {
                Ok(count) if count > 0 => {
                    log::info!("已恢复 {} 个中断的 AI 分析状态", count);
                }
                Ok(_) => {}
                Err(error) => log::warn!("AI 分析启动恢复失败: {}", error),
            }
            log::info!(
                "启动状态恢复检查完成，耗时 {} ms",
                recovery_started.elapsed().as_millis()
            );

            // 内置 Skill 是业务调用的基础数据，必须在 AppState 对外可用前完成同步，
            // 避免首次安装或资源升级时读到空集合、旧版本或部分更新状态。
            let skill_sync_started = Instant::now();
            if let Err(error) =
                services::ai_skill::AiSkillService::sync_builtin(app.handle(), &db)
            {
                log::warn!("内置 Skill 同步失败: {}", error);
            }
            log::info!(
                "内置 Skill 同步完成，耗时 {} ms",
                skill_sync_started.elapsed().as_millis()
            );

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

            // Dev API 仅用于浏览器开发验收，不能随正式安装包启动；仅监听回环也不能阻止
            // 同机恶意进程复用已配置的 AI Provider 或调用本地写接口。
            #[cfg(debug_assertions)]
            dev_server::start(app.handle().clone());

            // 初始化系统托盘
            let tray_started = Instant::now();
            tray::setup_tray(app)?;
            log::info!(
                "系统托盘初始化完成，耗时 {} ms",
                tray_started.elapsed().as_millis()
            );

            // 开发模式下给窗口标题加 [DEV] 后缀，避免与生产版本混淆
            #[cfg(debug_assertions)]
            if let Some(window) = app.get_webview_window("main") {
                if let Ok(current_title) = window.title() {
                    let _ = window.set_title(&format!("{} [DEV]", current_title));
                }
            }

            log::info!(
                "应用同步初始化完成，总耗时 {} ms",
                setup_started.elapsed().as_millis()
            );
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
            commands::git_workspace::start_git_provider_repositories_clone,
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
            // 团队知识库模块
            commands::knowledge::analyze_knowledge_query,
            commands::knowledge::search_knowledge_fts,
            commands::knowledge::search_knowledge_hybrid,
            commands::knowledge::preview_knowledge_rag_context,
            commands::knowledge::ask_knowledge,
            commands::knowledge::run_fixed_knowledge_retrieval_evaluation,
            commands::knowledge::build_knowledge_local_embedding_batch,
            commands::knowledge::build_knowledge_remote_embedding_batch,
            commands::knowledge::get_knowledge_remote_embedding_enabled,
            commands::knowledge::upsert_knowledge_relation,
            commands::knowledge::list_knowledge_relations,
            commands::knowledge::confirm_knowledge_relation,
            commands::knowledge::import_knowledge_document_relations,
            commands::knowledge::import_knowledge_commit_relations,
            commands::knowledge_domain::catalog::list_knowledge_project_repository_bindings,
            commands::knowledge_domain::catalog::replace_knowledge_project_repository_bindings,
            commands::knowledge_domain::catalog::unlink_knowledge_project_repository_binding,
            commands::knowledge_domain::catalog::inspect_knowledge_project_repository_binding,
            commands::knowledge_domain::catalog::create_knowledge_project_version_manifest,
            commands::knowledge_domain::catalog::get_knowledge_project_version_manifest,
            commands::knowledge_domain::catalog::get_knowledge_project_version_completeness,
            commands::knowledge_domain::catalog::start_knowledge_project_version_backfill,
            commands::knowledge_domain::search::search_knowledge_catalog,
            commands::knowledge_domain::terminology::list_knowledge_project_terms,
            commands::knowledge_domain::terminology::upsert_knowledge_project_term,
            commands::knowledge_domain::terminology::delete_knowledge_project_term,
            commands::knowledge_domain::qa::ask_knowledge_scoped_question,
            commands::knowledge_domain::qa::list_knowledge_qa_sessions,
            commands::knowledge_domain::qa::get_knowledge_qa_session,
            commands::knowledge_domain::qa::persist_knowledge_qa_round,
            commands::knowledge_domain::qa::delete_knowledge_qa_session,
            commands::knowledge_domain::qa::save_knowledge_qa_markdown,
            commands::knowledge_domain::analysis::list_knowledge_analysis_code_sources,
            commands::knowledge_domain::analysis::list_knowledge_analysis_code_snapshots,
            commands::knowledge_domain::analysis::capture_knowledge_analysis_git_snapshot,
            commands::knowledge_domain::analysis::analyze_knowledge_analysis_snapshot,
            commands::knowledge_domain::analysis::generate_knowledge_analysis_documents,
            commands::knowledge_domain::analysis::create_knowledge_analysis_ai_draft,
            commands::knowledge_domain::analysis::confirm_knowledge_analysis_ai_draft,
            commands::knowledge_domain::graph::build_knowledge_project_graph,
            commands::knowledge_domain::graph::query_knowledge_project_graph,
            commands::knowledge_domain::documents::save_knowledge_document_draft,
            commands::knowledge_domain::documents::commit_knowledge_document_draft,
            commands::knowledge_domain::documents::restore_knowledge_document_version_to_draft,
            commands::knowledge_domain::documents::list_deleted_knowledge_documents,
            commands::knowledge_domain::documents::preview_knowledge_document_deletion,
            commands::knowledge_domain::documents::restore_knowledge_document,
            commands::knowledge_domain::documents::get_knowledge_document_image_preview,
            commands::knowledge_domain::ingestion::prepare_knowledge_upload_file,
            commands::knowledge_domain::ingestion::prepare_knowledge_upload_directory,
            commands::knowledge_domain::ingestion::create_knowledge_document_upload,
            commands::knowledge_domain::ingestion::create_knowledge_document_upload_batch,
            commands::knowledge::list_knowledge_projects,
            commands::knowledge::list_zentao_connections,
            commands::knowledge::upsert_zentao_connection,
            commands::knowledge::delete_zentao_connection,
            commands::knowledge::probe_zentao_connection,
            commands::knowledge::discover_zentao_remote_scopes,
            commands::knowledge::upsert_zentao_project_mapping,
            commands::knowledge::list_zentao_project_mappings,
            commands::knowledge::sync_zentao_mapping,
            commands::knowledge::generate_zentao_fact_documents,
            commands::knowledge::generate_zentao_ai_summary,
            commands::knowledge::import_knowledge_ai_experiences,
            commands::knowledge::upsert_knowledge_project,
            commands::knowledge::delete_knowledge_project,
            commands::knowledge::list_knowledge_releases,
            commands::knowledge::upsert_knowledge_release,
            commands::knowledge::delete_knowledge_release,
            commands::knowledge::discover_knowledge_git_refs,
            commands::knowledge::capture_knowledge_git_snapshot,
            commands::knowledge::capture_knowledge_dirty_worktree_snapshot,
            commands::knowledge::capture_knowledge_local_directory_snapshot,
            commands::knowledge::analyze_knowledge_code_snapshot,
            commands::knowledge::generate_knowledge_code_documents,
            commands::knowledge::search_knowledge_code_symbols,
            commands::knowledge::list_knowledge_code_files,
            commands::knowledge::get_knowledge_code_file_content,
            commands::knowledge::get_knowledge_code_call_graph,
            commands::knowledge::compare_knowledge_code_snapshots,
            commands::knowledge::analyze_knowledge_code_impact,
            commands::knowledge::list_knowledge_code_snapshots,
            commands::knowledge::list_knowledge_sources,
            commands::knowledge::list_knowledge_code_sources,
            commands::knowledge::upsert_knowledge_source,
            commands::knowledge::upsert_knowledge_sources_atomically,
            commands::knowledge::upsert_knowledge_code_source,
            commands::knowledge::delete_knowledge_source,
            commands::knowledge::preview_knowledge_source_scope,
            commands::knowledge::preview_knowledge_code_source_scope,
            commands::knowledge::sync_knowledge_git_source,
            commands::knowledge::sync_knowledge_local_source,
            commands::knowledge::sync_knowledge_experience_source,
            commands::knowledge::start_knowledge_source_sync,
            commands::knowledge::get_knowledge_job,
            commands::knowledge::list_knowledge_jobs,
            commands::knowledge::cancel_knowledge_job,
            commands::knowledge::retry_knowledge_job,
            commands::knowledge::list_knowledge_documents,
            commands::knowledge::get_knowledge_document_detail,
            commands::knowledge::list_knowledge_document_versions,
            commands::knowledge::list_knowledge_document_chunks,
            commands::knowledge::compare_knowledge_document_versions,
            commands::knowledge::get_knowledge_citation_detail,
            commands::knowledge::preview_knowledge_parse_and_chunk,
            commands::knowledge::parse_and_index_knowledge_document_version,
            commands::knowledge::calculate_knowledge_embedding_fingerprint,
            commands::knowledge::list_knowledge_embedding_profiles,
            commands::knowledge::upsert_knowledge_embedding_profile,
            commands::knowledge::get_knowledge_local_embedding_runtime_status,
            commands::knowledge::import_knowledge_local_embedding_model,
            commands::knowledge::download_knowledge_local_embedding_model,
            commands::knowledge::generate_knowledge_local_embeddings,
            commands::knowledge::test_knowledge_local_embedding_profile,
            commands::knowledge::test_knowledge_remote_embedding_profile,
            commands::knowledge::remove_knowledge_local_embedding_model,
            commands::knowledge::estimate_knowledge_embedding_rebuild,
            commands::knowledge::begin_knowledge_embedding_profile_rebuild,
            commands::knowledge::validate_knowledge_embedding_profile_rebuild,
            commands::knowledge::complete_knowledge_embedding_profile_rebuild,
            commands::knowledge::activate_knowledge_embedding_profile_rebuild,
            commands::knowledge::rollback_knowledge_embedding_profile_rebuild,
            commands::knowledge::retire_knowledge_embedding_profile_rebuild,
            commands::knowledge::search_active_knowledge_vectors,
            commands::knowledge::upsert_knowledge_document,
            commands::knowledge::delete_knowledge_document,
            commands::knowledge::ensure_knowledge_fts,
            commands::knowledge::rebuild_knowledge_fts,
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
            commands::database_ops::get_database_table_detail,
            commands::database_ops::execute_database_sql,
            commands::database_ops::execute_database_sql_batch,
            commands::database_ops::update_database_query_result_cell,
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
