//! 新知识平台 Command 的模块边界。Command 仅在领域 Service 与 DTO 都完成后注册，
//! 避免暴露没有持久化与授权实现的半成品 IPC。

pub mod analysis;
pub mod catalog;
pub mod documents;
pub mod governance;
pub mod graph;
pub mod ingestion;
pub mod jobs;
pub mod qa;
pub mod search;
pub mod terminology;
