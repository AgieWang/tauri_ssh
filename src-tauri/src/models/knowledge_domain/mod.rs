//! 新知识平台的领域 DTO。旧 `models::knowledge` 继续提供兼容外观；本目录只承载
//! 新流程的显式契约，避免继续向单体模型追加互不相关的字段。

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

#[allow(unused_imports)]
pub use analysis::*;
#[allow(unused_imports)]
pub use catalog::*;
#[allow(unused_imports)]
pub use documents::*;
#[allow(unused_imports)]
pub use governance::*;
#[allow(unused_imports)]
pub use graph::*;
#[allow(unused_imports)]
pub use ingestion::*;
#[allow(unused_imports)]
pub use jobs::*;
#[allow(unused_imports)]
pub use qa::*;
#[allow(unused_imports)]
pub use search::*;
#[allow(unused_imports)]
pub use terminology::*;

#[cfg(test)]
mod tests {
    use serde::de::DeserializeOwned;
    use serde_json::{json, Value};

    use super::{
        CommitKnowledgeDocumentDraftInput, CreateKnowledgeAnalysisDraftInput,
        KnowledgeCatalogSearchInput, KnowledgeDocumentDraftInput,
        KnowledgeDocumentVersionBindingInput, KnowledgeDomainJobRequest, KnowledgeFeatureFlag,
        KnowledgeGitRefType, KnowledgeGraphQueryInput, KnowledgeProjectVersionManifestInput,
        KnowledgeRepositoryBinding, KnowledgeRepositoryBindingInput, KnowledgeScopedQuestionInput,
        KnowledgeVersionStrategy, ProjectVersionRepositoryRefInput, RepositoryBindingInput,
        RestoreKnowledgeDocumentVersionToDraftInput, UploadKnowledgeAssetInput,
        UpsertKnowledgeProjectTermInput,
    };

    fn deserialize<T: DeserializeOwned>(value: Value) -> T {
        serde_json::from_value(value).expect("DTO 应按公开 JSON 契约解析")
    }

    #[test]
    fn repository_binding_uses_camel_case_and_safe_default_version_strategy() {
        let input: KnowledgeRepositoryBindingInput = serde_json::from_value(serde_json::json!({
            "projectId": 7,
            "repositories": [{ "workspaceKey": "workspace-a" }]
        }))
        .expect("缺少可选设置时仍可解析");
        assert_eq!(
            input.repositories[0].version_strategy,
            KnowledgeVersionStrategy::Manual
        );

        let json = serde_json::to_value(RepositoryBindingInput {
            workspace_key: "workspace-a".to_string(),
            alias: None,
            role: None,
            default_branch: None,
            version_strategy: KnowledgeVersionStrategy::TagOrBranch,
        })
        .expect("可序列化");
        assert!(json.get("workspaceKey").is_some());
        assert!(json.get("version_strategy").is_none());
    }

    #[test]
    fn project_term_input_uses_camel_case_and_preserves_optional_identifier() {
        let input: UpsertKnowledgeProjectTermInput = deserialize(json!({
            "id": null,
            "projectId": 8,
            "term": "工单",
            "aliases": ["WorkOrder", "work_order"],
            "confirmationNote": "负责人已确认"
        }));
        assert_eq!(input.id, None);
        assert_eq!(input.project_id, 8);
        assert_eq!(input.aliases, vec!["WorkOrder", "work_order"]);
        assert_eq!(input.created_by, None);
        let output = serde_json::to_value(input).expect("术语输入可序列化");
        assert!(output.get("projectId").is_some());
        assert!(output.get("confirmationNote").is_some());
        assert!(output.get("project_id").is_none());
    }

    #[test]
    fn all_domain_inputs_preserve_camel_case_defaults_nulls_and_identifier_boundaries() {
        let catalog: KnowledgeRepositoryBindingInput = deserialize(json!({
            "projectId": 9_223_372_036_854_775_807_i64,
            "repositories": [{ "workspaceKey": "repo-a" }]
        }));
        assert_eq!(catalog.project_id, i64::MAX);
        assert_eq!(catalog.repositories[0].alias, None);

        let manifest: KnowledgeProjectVersionManifestInput = deserialize(json!({
            "projectId": 1,
            "version": "v2.0.0",
            "repositories": [{ "repositoryBindingId": -9_223_372_036_854_775_808_i64, "refType": "tag", "refName": "v2.0.0" }]
        }));
        assert_eq!(manifest.repositories[0].repository_binding_id, i64::MIN);
        assert_eq!(manifest.repositories[0].ref_type, KnowledgeGitRefType::Tag);
        assert!(!manifest.repositories[0].excluded);

        let draft: KnowledgeDocumentDraftInput = deserialize(json!({
            "projectId": 1, "title": "需求", "content": "正文",
            "draftId": null, "documentId": null, "baseVersionId": null, "revision": null
        }));
        assert_eq!(draft.draft_id, None);
        assert_eq!(draft.document_id, None);
        assert_eq!(draft.doc_type, "markdown");
        assert_eq!(draft.editor_label, None);
        assert_eq!(draft.base_version_id, None);
        assert_eq!(draft.revision, None);

        let restore: RestoreKnowledgeDocumentVersionToDraftInput = deserialize(json!({
            "sourceVersionId": 8, "draftId": null, "revision": null, "editorLabel": null
        }));
        assert_eq!(restore.source_version_id, 8);
        assert_eq!(restore.draft_id, None);
        assert_eq!(restore.revision, None);
        let restore_json = serde_json::to_value(&restore).expect("恢复输入应按 camelCase 序列化");
        assert_eq!(restore_json["sourceVersionId"], 8);
        assert!(restore_json.get("source_version_id").is_none());

        let commit: CommitKnowledgeDocumentDraftInput = deserialize(json!({
            "draftId": 6, "revision": 2, "versionLabel": "初始版本",
            "projectVersionId": null, "commitMessage": null, "authorLabel": null
        }));
        assert_eq!(commit.draft_id, 6);
        assert_eq!(commit.revision, 2);
        assert_eq!(commit.project_version_id, None);

        let binding: KnowledgeDocumentVersionBindingInput = deserialize(json!({
            "documentVersionId": 2, "projectVersionId": null,
            "repositoryBindingId": null, "crossVersionScope": null
        }));
        assert_eq!(binding.project_version_id, None);
        assert_eq!(binding.repository_binding_id, None);
        assert_eq!(binding.cross_version_scope, None);

        let upload: UploadKnowledgeAssetInput = deserialize(json!({
            "projectId": 3, "projectVersionId": null, "fileHandle": "token", "displayName": null,
            "sourceFolderName": "退款原型"
        }));
        assert_eq!(upload.project_version_id, None);
        assert_eq!(upload.display_name, None);
        assert_eq!(upload.source_folder_name.as_deref(), Some("退款原型"));
        let upload_json = serde_json::to_value(&upload).expect("上传输入应按 camelCase 序列化");
        assert_eq!(upload_json["sourceFolderName"], "退款原型");
        assert!(upload_json.get("source_folder_name").is_none());

        let search: KnowledgeCatalogSearchInput = deserialize(json!({
            "projectId": 4, "projectVersionId": null, "query": "接口"
        }));
        assert!(search.repository_binding_ids.is_empty());
        assert_eq!(search.cursor, None);
        assert_eq!(search.limit, None);

        let graph: KnowledgeGraphQueryInput = deserialize(json!({
            "projectId": 5, "projectVersionId": 6, "rootEntityKey": "document:7"
        }));
        assert_eq!(graph.depth, 1);
        assert_eq!(graph.node_limit, 100);
        assert!(!graph.include_unconfirmed);

        let analysis: CreateKnowledgeAnalysisDraftInput = deserialize(json!({
            "projectId": 7, "projectVersionId": 8, "snapshotIds": []
        }));
        assert_eq!(analysis.provider_key, None);
        assert_eq!(analysis.template_key, None);

        let question: KnowledgeScopedQuestionInput = deserialize(json!({
            "projectId": 9, "projectVersionId": 10, "question": "如何部署？"
        }));
        assert!(question.repository_binding_ids.is_empty());
        assert!(question.conversation.is_empty());

        let job: KnowledgeDomainJobRequest = deserialize(json!({
            "jobType": "graph", "idempotencyKey": "graph:9:10"
        }));
        assert_eq!(job.project_id, None);
        assert_eq!(job.project_version_id, None);
        assert_eq!(job.payload_ref, None);

        let flag: KnowledgeFeatureFlag = deserialize(json!({
            "feature": "catalog", "projectId": null, "enabled": true
        }));
        assert_eq!(flag.project_id, None);

        let timestamp: KnowledgeRepositoryBinding = deserialize(json!({
            "id": 1, "projectId": 2, "workspaceKey": "repo-a", "alias": "服务 A",
            "repositoryRole": "service", "defaultBranch": "main", "versionStrategy": "branch",
            "enabled": true, "deletedAt": "9999-12-31T23:59:59.999Z"
        }));
        assert_eq!(
            timestamp.deleted_at.as_deref(),
            Some("9999-12-31T23:59:59.999Z")
        );
    }

    #[test]
    fn required_fields_and_unknown_enums_are_rejected() {
        let missing_project = serde_json::from_value::<KnowledgeCatalogSearchInput>(json!({
            "query": "接口"
        }));
        assert!(missing_project.is_err());

        let unknown_strategy = serde_json::from_value::<RepositoryBindingInput>(json!({
            "workspaceKey": "repo-a", "versionStrategy": "latest"
        }));
        assert!(unknown_strategy.is_err());

        let unknown_ref = serde_json::from_value::<ProjectVersionRepositoryRefInput>(json!({
            "repositoryBindingId": 1, "refType": "pull_request", "refName": "42"
        }));
        assert!(unknown_ref.is_err());

        let unknown_job = serde_json::from_value::<KnowledgeDomainJobRequest>(json!({
            "jobType": "shell", "idempotencyKey": "unsafe"
        }));
        assert!(unknown_job.is_err());
    }
}
