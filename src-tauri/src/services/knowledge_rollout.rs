use crate::database::Database;
use crate::error::AppError;
/// 团队知识库始终可用。保留这个兼容边界，避免各领域 Service 重复实现访问策略，
/// 同时让已有数据库中遗留的发布阶段配置不再影响用户操作。
pub struct KnowledgeRolloutService;

impl KnowledgeRolloutService {
    /// 保持既有调用点的统一边界，但不再存在可配置的知识库启停状态。
    pub fn require(_db: &Database, _feature: &str) -> Result<(), AppError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::KnowledgeRolloutService;
    use crate::database::Database;

    #[test]
    fn legacy_disabled_stage_does_not_block_knowledge_features() {
        let db = Database::init(":memory:").expect("test database");
        db.set_config("knowledge.rollout.stage", "disabled")
            .expect("set disabled rollout stage");
        assert!(KnowledgeRolloutService::require(&db, "catalog").is_ok());
        assert!(KnowledgeRolloutService::require(&db, "hybrid_rag").is_ok());
        assert!(KnowledgeRolloutService::require(&db, "local_embedding").is_ok());
    }
}
