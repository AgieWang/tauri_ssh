// 远程访问网关（移动端伴侣的桌面侧入口）
//
// 详细文档：.claude/skills/remote-gateway/skill.md
// 上层架构：.claude/skills/mobile-app-architecture/skill.md

pub mod auth;
pub mod config;
pub mod context;
pub mod failure_tracker;
pub mod handlers;
pub mod server;

pub use config::RemoteConfig;
pub use server::{RemoteServerState, RemoteStatus};
