use crate::database::Database;
use crate::services::knowledge_domain::documents::KnowledgeUploadGrantRegistry;
use crate::services::terminal::TerminalSessionRegistry;

/// 应用全局状态，通过 tauri::State 注入到 Command 中
pub struct AppState {
    pub db: Database,
    pub terminal_sessions: TerminalSessionRegistry,
    /// 一次性上传句柄只驻留内存，应用退出后自然失效，不能成为持久化路径入口。
    pub knowledge_upload_grants: KnowledgeUploadGrantRegistry,
}

impl AppState {
    pub fn new(db: Database) -> Self {
        Self {
            db,
            terminal_sessions: TerminalSessionRegistry::default(),
            knowledge_upload_grants: KnowledgeUploadGrantRegistry::default(),
        }
    }
}
