-- 团队知识库重构的无敏感数据夹具。
-- 在已执行当前 schema 迁移的临时 SQLite 数据库上加载，覆盖目录、版本、来源、文档、片段、向量、关系、任务和代码快照。

INSERT INTO knowledge_projects (project_key, name, aliases_json, description, git_workspace_key, git_workspace_keys_json, default_branch, enabled, created_at, updated_at)
VALUES ('fixture-project', '夹具项目', '["fixture","示例项目"]', '不含个人数据或凭据的重构测试项目', 'fixture-workspace', '["fixture-workspace"]', 'main', 1, '2026-08-03T00:00:00Z', '2026-08-03T00:00:00Z');

INSERT INTO knowledge_releases (project_id, version, tag_name, branch, commit_sha, description, created_at, updated_at)
VALUES (1, 'v1.0.0', 'v1.0.0', 'main', '1111111111111111111111111111111111111111', '夹具发布版本', '2026-08-03T00:00:00Z', '2026-08-03T00:00:00Z');

INSERT INTO knowledge_sources (source_key, project_id, source_type, display_name, root_path, git_workspace_key, include_globs_json, exclude_globs_json, version_strategy, sync_mode, allow_remote_embedding, enabled, last_sync_status, created_at, updated_at)
VALUES ('fixture-source', 1, 'git', '夹具仓库', '/tmp/fixture-repository', 'fixture-workspace', '["**/*.md"]', '[".git/**"]', 'manual', 'incremental', 0, 1, 'idle', '2026-08-03T00:00:00Z', '2026-08-03T00:00:00Z');

INSERT INTO knowledge_documents (document_key, project_id, source_id, doc_type, title, logical_path, status, sensitivity, tags_json, allow_ai, allow_mcp, created_at, updated_at)
VALUES ('fixture-document', 1, 1, 'markdown', '夹具文档', 'docs/fixture.md', 'active', 'internal', '["夹具","版本"]', 1, 1, '2026-08-03T00:00:00Z', '2026-08-03T00:00:00Z');

INSERT INTO knowledge_document_versions (document_id, release_id, version_label, git_branch, commit_sha, source_path, mime_type, content, content_hash, parsed_meta_json, token_estimate, valid, created_at)
VALUES (1, 1, 'v1', 'main', '1111111111111111111111111111111111111111', 'docs/fixture.md', 'text/markdown', '# 夹具文档', 'fixture-content-hash', '{"parser":"markdown"}', 10, 1, '2026-08-03T00:00:00Z');

INSERT INTO knowledge_chunks (document_version_id, chunk_index, heading_path, content, content_hash, token_estimate, location_json, created_at, updated_at)
VALUES (1, 0, '夹具文档', '这是用于搜索和引用的夹具内容。', 'fixture-chunk-hash', 12, '{"path":"docs/fixture.md","startLine":1,"endLine":2}', '2026-08-03T00:00:00Z', '2026-08-03T00:00:00Z');

INSERT INTO knowledge_embedding_profiles (profile_key, name, mode, provider_key, model, model_revision, dimension, normalized, config_json, fingerprint, status, is_active, created_at, updated_at)
VALUES ('fixture-profile', '夹具本地模型', 'local', 'fixture', 'fixture-model', '1', 2, 1, '{"chunkStrategy":"heading"}', 'fixture-profile-fingerprint', 'active', 1, '2026-08-03T00:00:00Z', '2026-08-03T00:00:00Z');

INSERT INTO knowledge_chunk_embeddings (chunk_id, profile_id, dimension, content_hash, vector_blob, vector_norm, created_at)
VALUES (1, 1, 2, 'fixture-chunk-hash', X'00000000000000000000000000000000', 1.0, '2026-08-03T00:00:00Z');

INSERT INTO knowledge_relations (project_id, release_id, document_version_id, snapshot_id, sensitivity, scope_status, from_type, from_key, relation_type, to_type, to_key, evidence_json, confidence, confirmed, source, created_at, updated_at)
VALUES (1, 1, 1, 0, 'internal', 'scoped', 'document', 'fixture-document', 'references', 'code_element', 'fixture-symbol', '{"documentVersionId":1,"chunkId":1}', 1.0, 1, 'fixture', '2026-08-03T00:00:00Z', '2026-08-03T00:00:00Z');

INSERT INTO knowledge_jobs (job_key, job_type, source_id, status, progress_current, progress_total, message, checkpoint_json, started_at, finished_at)
VALUES ('fixture-job', 'sync', 1, 'completed', 1, 1, '夹具已完成', '{"fixture":true}', '2026-08-03T00:00:00Z', '2026-08-03T00:00:00Z');

INSERT INTO knowledge_code_snapshots (snapshot_key, source_id, project_id, release_id, snapshot_type, ref_name, commit_sha, captured_at, analyzer_version, status, created_at, updated_at)
VALUES ('fixture-snapshot', 1, 1, 1, 'git_commit', 'v1.0.0', '1111111111111111111111111111111111111111', '2026-08-03T00:00:00Z', 'fixture-analyzer-v1', 'analyzed', '2026-08-03T00:00:00Z', '2026-08-03T00:00:00Z');
