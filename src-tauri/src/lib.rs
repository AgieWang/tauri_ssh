mod commands;
mod database;
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

            // 注册全局状态
            app.manage(AppState::new(db));

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
