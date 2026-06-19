// Tauri Mobile —— 极简 webview 容器入口
//
// 设计哲学：移动端是纯 webview 容器，所有业务逻辑（会话、数据、文件、终端）
// 都在桌面端的 axum 远程网关上（src-tauri/src/remote/）。本 Rust 进程只负责：
//   1. 启动 webview，加载打包好的 React SPA（../../dist-mobile）
//   2. 提供少量平台原生能力（log / os / opener，未来可加 biometric / 通知）
//
// 不引入任何业务 Command —— 所有数据通过 fetch / WebSocket 从桌面端拉取。
// 详见 .claude/skills/mobile-app-architecture/skill.md。

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_log::Builder::default().build())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_opener::init())
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
