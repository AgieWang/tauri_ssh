//! 新领域 Service 的迁移落点。旧 `KnowledgeService` 仍是唯一兼容门面，领域逻辑会
//! 按任务逐步迁入；本阶段不改变既有行为。

pub mod analysis;
pub mod catalog;
pub mod documents;
pub mod governance;
pub mod graph;
pub mod ingestion;
pub mod jobs;
pub mod qa;
pub mod qa_export;
pub mod search;
pub mod terminology;
pub mod upload_validation;
