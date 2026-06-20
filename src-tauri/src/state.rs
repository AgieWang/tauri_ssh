use crate::database::Database;
use crate::services::terminal::TerminalSessionRegistry;

/// 应用全局状态，通过 tauri::State 注入到 Command 中
pub struct AppState {
    pub db: Database,
    pub terminal_sessions: TerminalSessionRegistry,
}

impl AppState {
    pub fn new(db: Database) -> Self {
        Self {
            db,
            terminal_sessions: TerminalSessionRegistry::default(),
        }
    }
}
