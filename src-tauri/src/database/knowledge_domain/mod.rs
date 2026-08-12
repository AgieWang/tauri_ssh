//! 新知识平台的 DAO 迁移落点。旧 `database::knowledge` 保持兼容；每个领域只在
//! 迁移、完整性校验和 Service 都具备后接管写路径。

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
