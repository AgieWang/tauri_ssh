use regex::Regex;
use rusqlite::{
    params, params_from_iter, types::Value, Connection, OptionalExtension, Transaction,
};

use crate::database::Database;
use crate::error::AppError;
use crate::models::knowledge_domain::search::KnowledgeCatalogSearchInput;
use crate::models::{KnowledgeCitation, KnowledgeSearchHit, KnowledgeSearchInput};

pub(crate) const DOMAIN: &str = "search";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct KnowledgeTitleIndexRecord {
    pub document_id: i64,
    pub normalized_title: String,
    pub current_version_id: i64,
}

/// 标题索引使用稳定的展示无关格式：忽略首尾及连续空白，兼容全角 ASCII，并统一大小写。
/// 标点不被删除，避免把不同的接口名或版本标识意外合并为同一个标题。
pub(crate) fn normalize_knowledge_title(value: &str) -> String {
    let mut normalized = String::new();
    let mut pending_space = false;
    for character in value.trim().chars() {
        let normalized_character = match character {
            '\u{3000}' => ' ',
            '\u{ff01}'..='\u{ff5e}' => (character as u32 - 0xfee0) as u8 as char,
            _ => character,
        };
        if normalized_character.is_whitespace() {
            pending_space = !normalized.is_empty();
            continue;
        }
        if pending_space {
            normalized.push(' ');
            pending_space = false;
        }
        normalized.extend(normalized_character.to_lowercase());
    }
    normalized
}

/// 当前正式版本切换、删除和恢复都经由此函数更新标题索引。索引不是事实来源；当文档
/// 不存在、已删除或传入的版本不再是当前版本时，必须移除旧条目，不能保留历史标题。
pub(crate) fn sync_knowledge_document_title_index(
    conn: &Connection,
    document_id: i64,
) -> Result<(), AppError> {
    let current = conn
        .query_row(
            "SELECT title, latest_version_id
             FROM knowledge_documents
             WHERE id = ?1 AND deleted_at IS NULL AND latest_version_id IS NOT NULL",
            [document_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
        )
        .optional()?;
    let Some((title, version_id)) = current else {
        conn.execute(
            "DELETE FROM knowledge_document_title_index WHERE document_id = ?1",
            [document_id],
        )?;
        return Ok(());
    };
    let normalized_title = normalize_knowledge_title(&title);
    if normalized_title.is_empty() {
        conn.execute(
            "DELETE FROM knowledge_document_title_index WHERE document_id = ?1",
            [document_id],
        )?;
        return Ok(());
    }
    conn.execute(
        "INSERT INTO knowledge_document_title_index
            (document_id, normalized_title, current_version_id, updated_at)
         VALUES (?1, ?2, ?3, datetime('now', 'localtime'))
         ON CONFLICT(document_id) DO UPDATE SET normalized_title = excluded.normalized_title,
             current_version_id = excluded.current_version_id, updated_at = excluded.updated_at",
        params![document_id, normalized_title, version_id],
    )?;
    Ok(())
}

/// 维护或迁移后可重建派生标题索引。仅当前、未删除的正式文档进入索引，草稿与历史版本
/// 不会被回填为标题搜索候选。清空和逐条回填置于同一个事务，避免异常中断留下半索引。
pub(crate) fn rebuild_knowledge_document_title_index(conn: &Connection) -> Result<i64, AppError> {
    let transaction = conn.unchecked_transaction()?;
    let count = rebuild_knowledge_document_title_index_in_transaction(&transaction)?;
    transaction.commit()?;
    Ok(count)
}

/// 当全文索引重建已经持有事务时复用该事务，避免 SQLite 不支持的嵌套事务。
pub(crate) fn rebuild_knowledge_document_title_index_in_transaction(
    transaction: &Transaction<'_>,
) -> Result<i64, AppError> {
    let records = {
        let mut statement = transaction.prepare(
            "SELECT id, latest_version_id
             FROM knowledge_documents
             WHERE deleted_at IS NULL AND latest_version_id IS NOT NULL
             ORDER BY id",
        )?;
        let records = statement
            .query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        records
    };
    transaction.execute("DELETE FROM knowledge_document_title_index", [])?;
    for (document_id, _) in &records {
        sync_knowledge_document_title_index(&transaction, *document_id)?;
    }
    i64::try_from(records.len()).map_err(|_| AppError::Custom("标题索引数量超出范围".to_string()))
}

/// 索引写路径已经在提交、上传、删除与恢复时同步；仅当旧库缺少条目或索引版本落后时才
/// 触发启动回填，避免每次打开应用都进行全量写入。
pub(crate) fn knowledge_document_title_index_needs_rebuild(
    conn: &Connection,
) -> Result<bool, AppError> {
    let mut statement = conn.prepare(
        "SELECT d.title, d.latest_version_id, title_index.normalized_title,
                title_index.current_version_id
         FROM knowledge_documents d
         LEFT JOIN knowledge_document_title_index title_index ON title_index.document_id = d.id
         WHERE d.deleted_at IS NULL AND d.latest_version_id IS NOT NULL",
    )?;
    let entries = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<i64>>(3)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(entries
        .into_iter()
        .any(|(title, version_id, indexed_title, indexed_version_id)| {
            let normalized_title = normalize_knowledge_title(&title);
            indexed_title.as_deref() != Some(normalized_title.as_str())
                || indexed_version_id != Some(version_id)
        }))
}

impl Database {
    /// 返回当前搜索范围的轻量快照指纹原料。它不保存正文，只覆盖会改变可见候选、当前
    /// 版本、片段、标题索引、来源启用状态及版本范围绑定的字段，供 Service 在翻页前检测
    /// 结果漂移。范围绑定单独聚合，避免其一对多关系放大文档/片段计数。
    pub(crate) fn get_knowledge_catalog_search_snapshot(
        &self,
        input: &KnowledgeCatalogSearchInput,
    ) -> Result<String, AppError> {
        let mut sql = String::from(
            "SELECT COUNT(*), COALESCE(MAX(v.id), 0), COALESCE(MAX(c.id), 0),
                    COALESCE(MAX(d.updated_at), ''), COALESCE(MAX(title_index.updated_at), ''),
                    COALESCE(MAX(s.updated_at), '')
             FROM knowledge_documents d
             JOIN knowledge_document_versions v ON v.id = d.latest_version_id
             LEFT JOIN knowledge_chunks c ON c.document_version_id = v.id
             LEFT JOIN knowledge_document_title_index title_index ON title_index.document_id = d.id
             LEFT JOIN knowledge_sources s ON s.id = d.source_id
             WHERE d.project_id = ?
               AND v.valid = 1 AND d.deleted_at IS NULL AND d.status = 'active'
               AND COALESCE(s.enabled, 1) = 1",
        );
        let mut values = vec![Value::Integer(input.project_id)];
        append_release_scope_filter(
            &mut sql,
            &mut values,
            "d",
            "v",
            &input.project_version_id.into_iter().collect::<Vec<_>>(),
        );
        append_catalog_repository_filter(&mut sql, &mut values, "v", &input.repository_binding_ids);
        append_text_in_filter(&mut sql, &mut values, "d.doc_type", &input.document_types);
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let base_snapshot = conn.query_row(&sql, params_from_iter(values.iter()), |row| {
            Ok(format!(
                "{}:{}:{}:{}:{}:{}",
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
            ))
        })?;
        let mut binding_sql = String::from(
            "SELECT COALESCE(GROUP_CONCAT(binding_state, '|'), '') FROM (
                 SELECT v.id || ':' || COALESCE(CAST(binding.id AS TEXT), '') || ':' ||
                        COALESCE(CAST(binding.release_id AS TEXT), '') || ':' ||
                        COALESCE(CAST(binding.repository_binding_id AS TEXT), '') || ':' ||
                        COALESCE(binding.cross_version_scope, '') AS binding_state
                 FROM knowledge_documents d
                 JOIN knowledge_document_versions v ON v.id = d.latest_version_id
                 LEFT JOIN knowledge_sources s ON s.id = d.source_id
                 LEFT JOIN knowledge_document_version_bindings binding
                   ON binding.document_version_id = v.id
                 WHERE d.project_id = ?
                   AND v.valid = 1 AND d.deleted_at IS NULL AND d.status = 'active'
                   AND COALESCE(s.enabled, 1) = 1",
        );
        let mut binding_values = vec![Value::Integer(input.project_id)];
        append_release_scope_filter(
            &mut binding_sql,
            &mut binding_values,
            "d",
            "v",
            &input.project_version_id.into_iter().collect::<Vec<_>>(),
        );
        append_catalog_repository_filter(
            &mut binding_sql,
            &mut binding_values,
            "v",
            &input.repository_binding_ids,
        );
        append_text_in_filter(
            &mut binding_sql,
            &mut binding_values,
            "d.doc_type",
            &input.document_types,
        );
        binding_sql.push_str(" ORDER BY v.id ASC, binding.id ASC)");
        let binding_snapshot: String = conn.query_row(
            &binding_sql,
            params_from_iter(binding_values.iter()),
            |row| row.get(0),
        )?;
        Ok(format!("{base_snapshot}:{binding_snapshot}"))
    }

    /// 标题与全文候选在同一 SQL 中按文档当前版本合并。使用 `(标题匹配级别, 文档 ID)`
    /// 作为稳定排序与游标键，正文命中只补充引用位置，避免两条召回通道重复显示同一文档。
    pub(crate) fn search_knowledge_catalog_page(
        &self,
        input: &KnowledgeCatalogSearchInput,
        search_terms: &[String],
        last_title_rank: Option<i64>,
        last_document_id: Option<i64>,
        limit: u32,
    ) -> Result<Vec<KnowledgeSearchHit>, AppError> {
        self.ensure_knowledge_fts_ready_for_search()?;
        let normalized_query = normalize_knowledge_title(&input.query);
        let fts_query = catalog_fts_query(search_terms);
        let normalized_title_terms = search_terms
            .iter()
            .map(|term| normalize_knowledge_title(term))
            .filter(|term| !term.is_empty())
            .collect::<Vec<_>>();
        if normalized_query.is_empty() || fts_query.is_empty() || normalized_title_terms.is_empty()
        {
            return Ok(Vec::new());
        }
        let exact = normalized_query.clone();
        let prefix = format!("{normalized_query}%");
        let contains = format!("%{normalized_query}%");
        let title_predicate = catalog_title_match_predicate(&normalized_title_terms);
        // 先在 CTE 中一次性解析 FTS 命中，并为每个版本选出第一段正文。不能在每个
        // 文档候选的关联子查询里重复执行 MATCH：术语扩展为多个别名后，真实项目中
        // 成千上万的命中会使同一 FTS 表被反复扫描，进而让搜索请求长期不返回。
        let mut sql = String::from(
            "WITH fts_matches AS (
                 SELECT matched_chunk.id AS chunk_id,
                        matched_chunk.document_version_id,
                        matched_chunk.chunk_index
                 FROM knowledge_chunks_fts
                 JOIN knowledge_chunks matched_chunk
                   ON matched_chunk.id = CAST(knowledge_chunks_fts.chunk_id AS INTEGER)
                 WHERE knowledge_chunks_fts MATCH ?
                   AND (json_extract(matched_chunk.location_json, '$.snapshotId') IS NULL OR EXISTS (
                       SELECT 1 FROM knowledge_code_snapshots report_snapshot
                       WHERE report_snapshot.id = CAST(json_extract(matched_chunk.location_json, '$.snapshotId') AS INTEGER)
                         AND report_snapshot.status = 'analyzed'
                   ))
             ),
             first_fts_chunk_indices AS (
                 SELECT document_version_id, MIN(chunk_index) AS chunk_index
                 FROM fts_matches
                 GROUP BY document_version_id
             ),
             first_fts_chunks AS (
                 SELECT fts_matches.document_version_id, MIN(fts_matches.chunk_id) AS chunk_id
                 FROM fts_matches
                 JOIN first_fts_chunk_indices first_index
                   ON first_index.document_version_id = fts_matches.document_version_id
                  AND first_index.chunk_index = fts_matches.chunk_index
                 GROUP BY fts_matches.document_version_id
             )
             SELECT d.id, d.project_id, d.title,
                    COALESCE(NULLIF(v.source_path, ''), d.logical_path),
                    v.id, v.release_id, v.commit_sha, title_index.normalized_title,
                    c.id, c.heading_path, c.content, c.location_json
             FROM knowledge_documents d
             JOIN knowledge_document_versions v ON v.id = d.latest_version_id
             LEFT JOIN knowledge_document_title_index title_index ON title_index.document_id = d.id
             LEFT JOIN knowledge_sources s ON s.id = d.source_id
             LEFT JOIN first_fts_chunks fts_hit ON fts_hit.document_version_id = v.id
             LEFT JOIN knowledge_chunks c ON c.id = fts_hit.chunk_id
             WHERE d.project_id = ?
               AND v.valid = 1 AND d.deleted_at IS NULL AND d.status = 'active'
               AND COALESCE(s.enabled, 1) = 1
               AND NOT EXISTS (
                   SELECT 1 FROM knowledge_code_files blocked_code_file
                   LEFT JOIN knowledge_code_snapshots blocked_snapshot
                     ON blocked_snapshot.id = blocked_code_file.snapshot_id
                   WHERE blocked_code_file.document_version_id = v.id
                     AND (blocked_code_file.status != 'active' OR blocked_snapshot.status != 'analyzed')
               )
               AND ((",
        );
        sql.push_str(&title_predicate);
        sql.push_str(") OR fts_hit.chunk_id IS NOT NULL)");
        let mut values = vec![
            Value::Text(fts_query.clone()),
            Value::Integer(input.project_id),
        ];
        values.extend(
            normalized_title_terms
                .iter()
                .map(|term| Value::Text(format!("%{term}%"))),
        );
        append_release_scope_filter(
            &mut sql,
            &mut values,
            "d",
            "v",
            &input.project_version_id.into_iter().collect::<Vec<_>>(),
        );
        append_catalog_repository_filter(&mut sql, &mut values, "v", &input.repository_binding_ids);
        append_text_in_filter(&mut sql, &mut values, "d.doc_type", &input.document_types);

        let rank_expression = "CASE WHEN title_index.normalized_title = ? THEN 0
                                    WHEN title_index.normalized_title LIKE ? THEN 1
                                    WHEN title_index.normalized_title LIKE ? THEN 2 ELSE 3 END";
        if let (Some(last_title_rank), Some(last_document_id)) = (last_title_rank, last_document_id)
        {
            sql.push_str(" AND ((");
            sql.push_str(rank_expression);
            sql.push_str(") > ? OR ((");
            sql.push_str(rank_expression);
            sql.push_str(") = ? AND d.id > ?))");
            values.extend([
                Value::Text(exact.clone()),
                Value::Text(prefix.clone()),
                Value::Text(contains.clone()),
                Value::Integer(last_title_rank),
                Value::Text(exact.clone()),
                Value::Text(prefix.clone()),
                Value::Text(contains.clone()),
                Value::Integer(last_title_rank),
                Value::Integer(last_document_id),
            ]);
        }
        sql.push_str(" ORDER BY ");
        sql.push_str(rank_expression);
        sql.push_str(", d.id ASC LIMIT ?");
        values.extend([
            Value::Text(exact),
            Value::Text(prefix),
            Value::Text(contains),
            Value::Integer(i64::from(limit.saturating_add(1))),
        ]);

        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let rows = conn
            .prepare(&sql)?
            .query_map(params_from_iter(values.iter()), |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Option<i64>>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, Option<i64>>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, Option<String>>(7)?,
                    row.get::<_, Option<i64>>(8)?,
                    row.get::<_, Option<String>>(9)?,
                    row.get::<_, Option<String>>(10)?,
                    row.get::<_, Option<String>>(11)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows
            .into_iter()
            .map(|row| catalog_search_hit_from_row(row, &normalized_query, &normalized_title_terms))
            .collect())
    }

    pub(crate) fn upsert_knowledge_document_title_index(
        &self,
        document_id: i64,
        normalized_title: &str,
        current_version_id: i64,
    ) -> Result<(), AppError> {
        if document_id <= 0 || current_version_id <= 0 || normalized_title.trim().is_empty() {
            return Err(AppError::InvalidInput(
                "标题索引缺少文档、版本或标题".to_string(),
            ));
        }
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        conn.execute(
            "INSERT INTO knowledge_document_title_index
                (document_id, normalized_title, current_version_id, updated_at)
             VALUES (?1, ?2, ?3, datetime('now', 'localtime'))
             ON CONFLICT(document_id) DO UPDATE SET normalized_title = excluded.normalized_title,
                 current_version_id = excluded.current_version_id, updated_at = excluded.updated_at",
            params![document_id, normalized_title.trim(), current_version_id],
        )?;
        Ok(())
    }

    /// 标题通道和全文通道共享相同的硬过滤条件。标题只指向当前正式版本的首个结构块，
    /// 让 UI 可以与全文命中按版本去重，同时仍能展示可追溯的段落位置。
    pub(crate) fn search_knowledge_document_title_hits(
        &self,
        input: &KnowledgeSearchInput,
    ) -> Result<Vec<KnowledgeSearchHit>, AppError> {
        if input.snapshot_id.is_some() {
            return Ok(Vec::new());
        }
        let query = normalize_knowledge_title(&input.query);
        if query.is_empty() {
            return Ok(Vec::new());
        }
        let exact = query.clone();
        let prefix = format!("{query}%");
        let project_aliases = self.knowledge_project_title_aliases(&query, &input.project_ids)?;
        let title_match_groups = knowledge_title_match_groups(&query, &project_aliases);
        let structured_title_match = title_match_groups.len() > 1;
        let title_predicate = title_match_groups
            .iter()
            .enumerate()
            .map(|(group_index, aliases)| {
                if structured_title_match && group_index == 1 {
                    format!(
                        "({})",
                        aliases
                            .iter()
                            .map(|_| title_version_token_predicate())
                            .collect::<Vec<_>>()
                            .join(" OR ")
                    )
                } else {
                    format!(
                        "({})",
                        aliases
                            .iter()
                            .map(|_| "title_index.normalized_title LIKE ? ESCAPE '\\'")
                            .collect::<Vec<_>>()
                            .join(" OR ")
                    )
                }
            })
            .collect::<Vec<_>>()
            .join(" AND ");
        let mut sql = String::from(
            "SELECT d.id, d.project_id, d.title,
                    COALESCE(NULLIF(v.source_path, ''), d.logical_path),
                    v.id, v.release_id, v.commit_sha,
                    c.id, c.heading_path, c.location_json,
                    title_index.normalized_title,
                    substr(COALESCE(c.content, v.content), 1, 400)
             FROM knowledge_document_title_index title_index
             JOIN knowledge_documents d ON d.id = title_index.document_id
             JOIN knowledge_document_versions v
               ON v.id = title_index.current_version_id AND d.latest_version_id = v.id
             LEFT JOIN knowledge_chunks c ON c.id = (
                SELECT current_chunk.id FROM knowledge_chunks current_chunk
                WHERE current_chunk.document_version_id = v.id
                ORDER BY current_chunk.chunk_index ASC, current_chunk.id ASC LIMIT 1
             )
             LEFT JOIN knowledge_sources s ON s.id = d.source_id
             WHERE (
        ",
        );
        sql.push_str(&title_predicate);
        sql.push_str(
            ")
               AND v.valid = 1 AND d.deleted_at IS NULL AND d.status = 'active'
               AND d.allow_ai = 1 AND COALESCE(s.enabled, 1) = 1",
        );
        let mut values = Vec::new();
        for (group_index, aliases) in title_match_groups.iter().enumerate() {
            for alias in aliases {
                if structured_title_match && group_index == 1 {
                    values.extend((0..4).map(|_| Value::Text(alias.clone())));
                } else {
                    values.push(Value::Text(format!("%{}%", escape_like_pattern(alias))));
                }
            }
        }
        append_in_filter(&mut sql, &mut values, "d.project_id", &input.project_ids);
        append_release_scope_filter(&mut sql, &mut values, "d", "v", &input.release_ids);
        append_in_filter(&mut sql, &mut values, "d.source_id", &input.source_ids);
        append_text_in_filter(&mut sql, &mut values, "d.doc_type", &input.document_types);
        append_text_in_filter(&mut sql, &mut values, "d.sensitivity", &input.sensitivities);
        sql.push_str(
            " ORDER BY CASE WHEN title_index.normalized_title = ? THEN 0
                              WHEN title_index.normalized_title LIKE ? THEN 1 ELSE 2 END,
                         d.id ASC
              LIMIT ?",
        );
        values.push(Value::Text(exact));
        values.push(Value::Text(prefix));
        values.push(Value::Integer(input.limit.unwrap_or(20).clamp(1, 100)));
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let rows = conn
            .prepare(&sql)?
            .query_map(params_from_iter(values.iter()), |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Option<i64>>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, Option<i64>>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, Option<i64>>(7)?,
                    row.get::<_, Option<String>>(8)?,
                    row.get::<_, Option<String>>(9)?,
                    row.get::<_, String>(10)?,
                    row.get::<_, String>(11)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows
            .into_iter()
            .map(|row| {
                let content = row.11;
                let location = row
                    .9
                    .as_deref()
                    .and_then(|value| serde_json::from_str::<serde_json::Value>(value).ok())
                    .unwrap_or_default();
                let score = if structured_title_match {
                    4.0
                } else if row.10 == query {
                    3.0
                } else if row.10.starts_with(&query) {
                    2.0
                } else {
                    1.0
                };
                KnowledgeSearchHit {
                    score,
                    channels: vec!["title".to_string()],
                    citation: KnowledgeCitation {
                        citation_key: match row.7 {
                            Some(chunk_id) => {
                                format!("document:{}:version:{}:chunk:{chunk_id}", row.0, row.4)
                            }
                            None => format!("document:{}:version:{}", row.0, row.4),
                        },
                        source_type: "knowledge_document".to_string(),
                        document_id: Some(row.0),
                        document_version_id: Some(row.4),
                        chunk_id: row.7,
                        project_id: row.1,
                        release_id: row.5,
                        title: row.2,
                        logical_path: row.3,
                        heading_path: row.8.unwrap_or_default(),
                        commit_sha: row.6,
                        external_key: String::new(),
                        snapshot_id: None,
                        symbol_key: String::new(),
                        start_line: location
                            .get("startLine")
                            .and_then(serde_json::Value::as_i64),
                        end_line: location.get("endLine").and_then(serde_json::Value::as_i64),
                        excerpt: content.chars().take(400).collect(),
                    },
                    content: input
                        .include_context
                        .unwrap_or(false)
                        .then_some(content)
                        .unwrap_or_default(),
                    diagnostics: serde_json::json!({
                        "titleMatch": true,
                        "highConfidenceTitleMatch": structured_title_match,
                        "titleMatchGroups": title_match_groups,
                    }),
                }
            })
            .collect())
    }

    fn knowledge_project_title_aliases(
        &self,
        normalized_query: &str,
        project_ids: &[i64],
    ) -> Result<Vec<String>, AppError> {
        if project_ids.is_empty() {
            return Ok(Vec::new());
        }
        let mut sql = String::from(
            "SELECT project_key, name, aliases_json FROM knowledge_projects
             WHERE enabled = 1 AND deleted_at IS NULL AND id IN (",
        );
        sql.push_str(
            &(0..project_ids.len())
                .map(|_| "?")
                .collect::<Vec<_>>()
                .join(", "),
        );
        sql.push(')');
        let values = project_ids
            .iter()
            .copied()
            .map(Value::Integer)
            .collect::<Vec<_>>();
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let rows = conn
            .prepare(&sql)?
            .query_map(params_from_iter(values.iter()), |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut aliases = rows
            .into_iter()
            .flat_map(|(project_key, name, aliases_json)| {
                let mut aliases =
                    serde_json::from_str::<Vec<String>>(&aliases_json).unwrap_or_default();
                aliases.push(project_key);
                aliases.push(name);
                aliases
            })
            .map(|alias| normalize_knowledge_title(&alias))
            .filter(|alias| alias.chars().count() >= 2 && normalized_query.contains(alias))
            .collect::<Vec<_>>();
        aliases.sort();
        aliases.dedup();
        aliases.sort_by_key(|alias| std::cmp::Reverse(alias.chars().count()));
        Ok(aliases)
    }

    pub(crate) fn search_knowledge_document_titles(
        &self,
        normalized_query: &str,
        limit: u32,
    ) -> Result<Vec<KnowledgeTitleIndexRecord>, AppError> {
        let query = normalized_query.trim();
        if query.is_empty() || limit == 0 {
            return Ok(Vec::new());
        }
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let exact = query.to_string();
        let prefix = format!("{query}%");
        let contains = format!("%{query}%");
        let mut statement = conn.prepare(
            "SELECT document_id, normalized_title, current_version_id
             FROM knowledge_document_title_index
             WHERE normalized_title LIKE ?3
             ORDER BY CASE WHEN normalized_title = ?1 THEN 0
                           WHEN normalized_title LIKE ?2 THEN 1 ELSE 2 END,
                      document_id ASC
             LIMIT ?4",
        )?;
        let matches = statement
            .query_map(params![exact, prefix, contains, i64::from(limit)], |row| {
                Ok(KnowledgeTitleIndexRecord {
                    document_id: row.get(0)?,
                    normalized_title: row.get(1)?,
                    current_version_id: row.get(2)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(AppError::from)?;
        Ok(matches)
    }
}

/// 版本化需求问题通常包含自然语言包装，不能要求整句话原样出现在标题中。只有同时
/// 识别出版本号和明确的文档意图时才启用分组匹配，避免把普通短查询过度拆分。版本组
/// 同时兼容 `v1.2.0`、`1.2.0` 与省略尾零的 `1.2`，其余数字不会被当成版本。
fn knowledge_title_match_groups(
    normalized_query: &str,
    project_aliases: &[String],
) -> Vec<Vec<String>> {
    let version_regex =
        Regex::new(r"(?i)v?\d+(?:\.\d+){1,3}(?:[-+][a-z0-9.-]+)?").expect("静态版本号正则必须有效");
    let Some(version_match) = version_regex.find(normalized_query) else {
        return vec![vec![normalized_query.to_string()]];
    };
    let intents = ["需求", "进度", "方案", "设计", "测试", "部署"]
        .into_iter()
        .filter(|intent| normalized_query.contains(intent))
        .map(str::to_string)
        .collect::<Vec<_>>();
    if intents.is_empty() {
        return vec![vec![normalized_query.to_string()]];
    }

    let subjects = if project_aliases.is_empty() {
        let mut subject = version_regex.replace_all(normalized_query, " ").to_string();
        for intent in &intents {
            subject = subject.replace(intent, " ");
        }
        for wrapper in [
            "包括哪些",
            "是什么",
            "有哪些",
            "有什么",
            "请帮我",
            "请问",
            "帮我",
            "查询",
            "查看",
            "关于",
            "相关",
            "版本的",
            "版本",
            "主要",
            "详细",
            "具体",
            "文档",
            "内容",
            "说明",
            "介绍",
            "了解",
            "想",
            "请",
            "我",
            "的",
        ] {
            subject = subject.replace(wrapper, " ");
        }
        let subject = subject
            .split(|character: char| {
                character.is_whitespace()
                    || matches!(
                        character,
                        '?' | '？' | ',' | '，' | '.' | '。' | ':' | '：' | '_' | '-' | '/'
                    )
            })
            .filter(|part| part.chars().count() >= 2)
            .collect::<Vec<_>>()
            .join("");
        vec![subject]
    } else {
        project_aliases.to_vec()
    };
    if subjects.iter().all(|subject| subject.chars().count() < 2) {
        return vec![vec![normalized_query.to_string()]];
    }

    let normalized_version = version_match
        .as_str()
        .trim_start_matches(['v', 'V'])
        .to_ascii_lowercase();
    let mut version_aliases = vec![normalized_version.clone()];
    let mut compact_version = normalized_version.clone();
    while compact_version.matches('.').count() >= 2 && compact_version.ends_with(".0") {
        compact_version.truncate(compact_version.len() - 2);
        version_aliases.push(compact_version.clone());
    }
    version_aliases.sort();
    version_aliases.dedup();

    vec![subjects, version_aliases, intents]
}

fn title_version_token_predicate() -> &'static str {
    "(title_index.normalized_title = ?
       OR title_index.normalized_title GLOB (? || '[^0-9.]*')
       OR title_index.normalized_title GLOB ('*[^0-9.]' || ?)
       OR title_index.normalized_title GLOB ('*[^0-9.]' || ? || '[^0-9.]*'))"
}

fn escape_like_pattern(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

fn append_in_filter(sql: &mut String, values: &mut Vec<Value>, column: &str, ids: &[i64]) {
    if ids.is_empty() {
        return;
    }
    sql.push_str(" AND ");
    sql.push_str(column);
    sql.push_str(" IN (");
    sql.push_str(&(0..ids.len()).map(|_| "?").collect::<Vec<_>>().join(", "));
    sql.push(')');
    values.extend(ids.iter().copied().map(Value::Integer));
}

fn append_catalog_repository_filter(
    sql: &mut String,
    values: &mut Vec<Value>,
    version_alias: &str,
    repository_binding_ids: &[i64],
) {
    if repository_binding_ids.is_empty() {
        return;
    }
    sql.push_str(" AND EXISTS (SELECT 1 FROM knowledge_document_version_bindings repository_scope");
    sql.push_str(" WHERE repository_scope.document_version_id = ");
    sql.push_str(version_alias);
    sql.push_str(".id AND repository_scope.repository_binding_id IN (");
    sql.push_str(
        &(0..repository_binding_ids.len())
            .map(|_| "?")
            .collect::<Vec<_>>()
            .join(", "),
    );
    sql.push_str("))");
    values.extend(repository_binding_ids.iter().copied().map(Value::Integer));
}

fn catalog_fts_query(search_terms: &[String]) -> String {
    let groups = search_terms
        .iter()
        .filter_map(|query| {
            let parts = query
                .split_whitespace()
                .map(|part| part.replace('"', ""))
                .filter(|part| !part.is_empty())
                .map(|part| format!("\"{part}\""))
                .collect::<Vec<_>>();
            (!parts.is_empty()).then(|| {
                if parts.len() == 1 {
                    parts.into_iter().next().expect("已确认单个 FTS 词")
                } else {
                    format!("({})", parts.join(" AND "))
                }
            })
        })
        .collect::<Vec<_>>();
    match groups.len() {
        0 => String::new(),
        1 => groups[0].clone(),
        _ => format!("({})", groups.join(" OR ")),
    }
}

fn catalog_title_match_predicate(normalized_terms: &[String]) -> String {
    normalized_terms
        .iter()
        .map(|_| "title_index.normalized_title LIKE ?")
        .collect::<Vec<_>>()
        .join(" OR ")
}

type CatalogSearchRow = (
    i64,
    Option<i64>,
    String,
    String,
    i64,
    Option<i64>,
    String,
    Option<String>,
    Option<i64>,
    Option<String>,
    Option<String>,
    Option<String>,
);

fn catalog_search_hit_from_row(
    row: CatalogSearchRow,
    normalized_query: &str,
    normalized_title_terms: &[String],
) -> KnowledgeSearchHit {
    let title_rank = match row.7.as_deref() {
        Some(title) if title == normalized_query => 0,
        Some(title) if title.starts_with(normalized_query) => 1,
        Some(title) if title.contains(normalized_query) => 2,
        _ => 3,
    };
    let location = row
        .11
        .as_deref()
        .and_then(|value| serde_json::from_str::<serde_json::Value>(value).ok())
        .unwrap_or_default();
    let snapshot_id = location
        .get("snapshotId")
        .and_then(serde_json::Value::as_i64);
    let mut channels = Vec::new();
    if row.7.as_deref().is_some_and(|title| {
        normalized_title_terms
            .iter()
            .any(|term| title.contains(term))
    }) {
        channels.push("title".to_string());
    }
    if row.8.is_some() {
        channels.push("fts".to_string());
    }
    let excerpt = row
        .10
        .as_deref()
        .unwrap_or_default()
        .chars()
        .take(400)
        .collect::<String>();
    KnowledgeSearchHit {
        score: match title_rank {
            0 => 3.0,
            1 => 2.0,
            2 => 1.0,
            _ => 0.0,
        },
        channels,
        citation: KnowledgeCitation {
            citation_key: match (snapshot_id, row.8) {
                (Some(snapshot_id), Some(chunk_id)) => {
                    format!("code:snapshot:{snapshot_id}:chunk:{chunk_id}")
                }
                (_, Some(chunk_id)) => {
                    format!("document:{}:version:{}:chunk:{chunk_id}", row.0, row.4)
                }
                _ => format!("document:{}:version:{}", row.0, row.4),
            },
            source_type: if snapshot_id.is_some() {
                "code_snapshot".to_string()
            } else {
                "knowledge_document".to_string()
            },
            document_id: Some(row.0),
            document_version_id: Some(row.4),
            chunk_id: row.8,
            project_id: row.1,
            release_id: row.5,
            title: row.2,
            logical_path: row.3,
            heading_path: row.9.unwrap_or_default(),
            commit_sha: row.6,
            external_key: String::new(),
            snapshot_id,
            symbol_key: location
                .get("symbolKey")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string(),
            start_line: location
                .get("startLine")
                .and_then(serde_json::Value::as_i64),
            end_line: location.get("endLine").and_then(serde_json::Value::as_i64),
            excerpt,
        },
        content: row.10.unwrap_or_default(),
        diagnostics: serde_json::json!({
            "sortSummary": match title_rank {
                0 => "标题精确匹配",
                1 => "标题前缀匹配",
                2 => "标题包含匹配",
                _ => "正文匹配",
            },
            "titleRank": title_rank,
        }),
    }
}

/// 生成项目内版本可见性谓词。全版本文档不归属单一发布版本，但只能随着其所属项目的
/// 发布版本出现；外层查询必须已经将 `document_alias` 与 `version_alias` 关联。
///
/// 这里刻意用相关 `EXISTS`，而非关联版本绑定表：一个版本即使存在多个绑定记录，也只
/// 会保留外层的一行，避免列表、全文和向量候选被放大。别名和 `requested_release_predicate`
/// 全部来自本模块内部固定 SQL，不接收外部输入。
pub(crate) fn release_scope_visibility_predicate(
    document_alias: &str,
    version_alias: &str,
    requested_release_predicate: &str,
) -> String {
    format!(
        "EXISTS (
            SELECT 1 FROM knowledge_releases requested_release
            WHERE {requested_release_predicate}
              AND requested_release.project_id = {document_alias}.project_id
              AND requested_release.deleted_at IS NULL
              AND (
                    {version_alias}.release_id = requested_release.id
                    OR EXISTS (
                        SELECT 1 FROM knowledge_document_version_bindings version_binding
                        WHERE version_binding.document_version_id = {version_alias}.id
                          AND version_binding.cross_version_scope = 'project_all_versions'
                    )
              )
        )"
    )
}

/// 与全文、向量通道保持同一版本可见性：项目范围文档不归属某一个发布版本，但在任意
/// 指定的同项目版本中都应可被检索到。
pub(crate) fn append_release_scope_filter(
    sql: &mut String,
    values: &mut Vec<Value>,
    document_alias: &str,
    version_alias: &str,
    release_ids: &[i64],
) {
    if release_ids.is_empty() {
        return;
    }
    let placeholders = (0..release_ids.len())
        .map(|_| "?")
        .collect::<Vec<_>>()
        .join(", ");
    let mut requested_release_predicate = String::from("requested_release.id IN (");
    requested_release_predicate.push_str(&placeholders);
    requested_release_predicate.push(')');
    sql.push_str(" AND ");
    sql.push_str(&release_scope_visibility_predicate(
        document_alias,
        version_alias,
        &requested_release_predicate,
    ));
    values.extend(release_ids.iter().copied().map(Value::Integer));
}

/// 默认检索只读取每篇文档的当前正式版本；当调用方明确指定项目版本时，则选择该版本
/// 可见的最新文档版本。这样历史问答不会被 `latest_version_id` 覆盖，也不会因同一文档
/// 的多次提交产生重复候选。
pub(crate) fn append_selected_document_version_filter(
    sql: &mut String,
    values: &mut Vec<Value>,
    document_alias: &str,
    version_alias: &str,
    release_ids: &[i64],
) {
    if release_ids.is_empty() {
        sql.push_str(" AND ");
        sql.push_str(version_alias);
        sql.push_str(".id = ");
        sql.push_str(document_alias);
        sql.push_str(".latest_version_id");
        return;
    }
    let placeholders = (0..release_ids.len())
        .map(|_| "?")
        .collect::<Vec<_>>()
        .join(", ");
    let requested_release_predicate = format!("requested_release.id IN ({placeholders})");
    let visibility = release_scope_visibility_predicate(
        document_alias,
        "candidate_version",
        &requested_release_predicate,
    );
    sql.push_str(" AND ");
    sql.push_str(version_alias);
    sql.push_str(".id = (SELECT MAX(candidate_version.id) FROM knowledge_document_versions candidate_version WHERE candidate_version.document_id = ");
    sql.push_str(document_alias);
    sql.push_str(".id AND candidate_version.valid = 1 AND ");
    sql.push_str(&visibility);
    sql.push(')');
    values.extend(release_ids.iter().copied().map(Value::Integer));
}

/// 仓库筛选必须依据版本绑定而非工作区路径推测。未绑定任何仓库的自定义文档仅在未选择
/// 仓库筛选时可见，避免用户选择单仓库后仍混入其他来源的证据。
pub(crate) fn append_repository_binding_filter(
    sql: &mut String,
    values: &mut Vec<Value>,
    version_alias: &str,
    repository_binding_ids: &[i64],
) {
    if repository_binding_ids.is_empty() {
        return;
    }
    sql.push_str(" AND EXISTS (SELECT 1 FROM knowledge_document_version_bindings repository_scope");
    sql.push_str(" WHERE repository_scope.document_version_id = ");
    sql.push_str(version_alias);
    sql.push_str(".id AND repository_scope.repository_binding_id IN (");
    sql.push_str(
        &(0..repository_binding_ids.len())
            .map(|_| "?")
            .collect::<Vec<_>>()
            .join(", "),
    );
    sql.push_str("))");
    values.extend(repository_binding_ids.iter().copied().map(Value::Integer));
}

fn append_text_in_filter(
    sql: &mut String,
    values: &mut Vec<Value>,
    column: &str,
    texts: &[String],
) {
    if texts.is_empty() {
        return;
    }
    sql.push_str(" AND ");
    sql.push_str(column);
    sql.push_str(" IN (");
    sql.push_str(&(0..texts.len()).map(|_| "?").collect::<Vec<_>>().join(", "));
    sql.push(')');
    values.extend(texts.iter().cloned().map(Value::Text));
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use rusqlite::Connection;

    use super::{
        knowledge_title_match_groups, normalize_knowledge_title, Database,
        KnowledgeCatalogSearchInput,
    };
    use crate::database::schema;
    use crate::models::{
        CreateKnowledgeDocumentVersionInput, KnowledgeChunkWriteInput,
        UpsertKnowledgeDocumentInput, UpsertKnowledgeProjectInput,
    };

    #[test]
    fn title_index_orders_exact_then_prefix_then_contains_matches(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let connection = Connection::open_in_memory()?;
        schema::migrate(&connection)?;
        let database = Database {
            conn: Mutex::new(connection),
        };
        database.upsert_knowledge_document_title_index(1, "用户接口", 10)?;
        database.upsert_knowledge_document_title_index(2, "用户接口说明", 11)?;
        database.upsert_knowledge_document_title_index(3, "管理用户接口", 12)?;
        let results = database.search_knowledge_document_titles("用户接口", 10)?;
        assert_eq!(
            results
                .iter()
                .map(|item| item.document_id)
                .collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
        Ok(())
    }

    #[test]
    fn normalizes_whitespace_full_width_ascii_and_case_without_erasing_punctuation() {
        assert_eq!(
            normalize_knowledge_title("  用户　ＡＰＩ  v1.0  "),
            "用户 api v1.0"
        );
        assert_eq!(normalize_knowledge_title("接口-A"), "接口-a");
    }

    #[test]
    fn versioned_requirement_questions_form_subject_version_and_intent_title_groups() {
        assert_eq!(
            knowledge_title_match_groups(
                "全业务工单 v1.2.0 版本的需求是什么",
                &["全业务工单".to_string()]
            ),
            vec![
                vec!["全业务工单".to_string()],
                vec!["1.2".to_string(), "1.2.0".to_string()],
                vec!["需求".to_string()],
            ]
        );
        assert_eq!(
            knowledge_title_match_groups("查询 2026 年需求", &[]),
            vec![vec!["查询 2026 年需求".to_string()]],
            "没有点分隔的普通数字不能被当成版本"
        );
        assert_eq!(
            knowledge_title_match_groups(
                "请帮我查看全业务工单 v1.2.0 的主要需求是什么",
                &["全业务工单".to_string()]
            )[0],
            vec!["全业务工单".to_string()]
        );
    }

    #[test]
    fn catalog_search_uses_keyset_pagination_and_deduplicates_title_and_full_text(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let connection = Connection::open_in_memory()?;
        schema::migrate(&connection)?;
        let database = Database {
            conn: Mutex::new(connection),
        };
        let project = database.upsert_knowledge_project(&UpsertKnowledgeProjectInput {
            id: None,
            project_key: "search-project".to_string(),
            name: "搜索项目".to_string(),
            aliases: Vec::new(),
            description: String::new(),
            git_workspace_keys: Vec::new(),
            git_workspace_key: String::new(),
            default_branch: String::new(),
            enabled: true,
        })?;
        database.ensure_knowledge_fts()?;
        for (key, title, content, allow_ai) in [
            ("exact", "订单接口", "订单接口用于创建订单。", true),
            (
                "body",
                "订单说明",
                "调用订单接口前需要校验客户状态。",
                false,
            ),
            ("third", "订单接口扩展", "扩展字段说明。", true),
        ] {
            let document = database.upsert_knowledge_document(&UpsertKnowledgeDocumentInput {
                id: None,
                document_key: key.to_string(),
                project_id: Some(project.id),
                source_id: None,
                doc_type: "markdown".to_string(),
                title: title.to_string(),
                logical_path: format!("docs/{key}.md"),
                sensitivity: "internal".to_string(),
                tags: Vec::new(),
                allow_ai,
                allow_mcp: false,
            })?;
            database.create_knowledge_document_version(
                &CreateKnowledgeDocumentVersionInput {
                    document_id: document.id,
                    release_id: None,
                    version_label: "v1".to_string(),
                    git_branch: String::new(),
                    commit_sha: String::new(),
                    source_path: format!("docs/{key}.md"),
                    mime_type: "text/markdown".to_string(),
                    content: content.to_string(),
                    content_hash: format!("hash-{key}"),
                    parsed_meta: serde_json::json!({}),
                    token_estimate: 10,
                },
                &[KnowledgeChunkWriteInput {
                    chunk_index: 0,
                    heading_path: title.to_string(),
                    content: content.to_string(),
                    content_hash: format!("chunk-{key}"),
                    location: serde_json::json!({"startLine": 1, "endLine": 1}),
                    token_estimate: 10,
                }],
            )?;
        }
        let input = KnowledgeCatalogSearchInput {
            project_id: project.id,
            project_version_id: None,
            query: "订单接口".to_string(),
            repository_binding_ids: Vec::new(),
            document_types: Vec::new(),
            cursor: None,
            limit: Some(1),
        };
        let first = database.search_knowledge_catalog_page(
            &input,
            &[input.query.clone()],
            None,
            None,
            1,
        )?;
        assert_eq!(first.len(), 2, "DAO 应多读取一条以判断是否仍有下一页");
        assert_eq!(first[0].citation.title, "订单接口");
        assert_eq!(first[0].channels, vec!["title", "fts"]);
        let rank = first[0].diagnostics["titleRank"].as_i64().expect("排序键");
        let second = database.search_knowledge_catalog_page(
            &input,
            &[input.query.clone()],
            Some(rank),
            first[0].citation.document_id,
            1,
        )?;
        assert_eq!(second[0].citation.title, "订单接口扩展");
        assert_ne!(
            first[0].citation.document_id,
            second[0].citation.document_id
        );
        // `allow_ai` 只限制内容能否发给 Provider；桌面本地搜索必须仍能检索这类
        // 已提交且在当前项目范围内的文档，不能把隐私选择误当作可见性开关。
        let local_only = database.search_knowledge_catalog_page(
            &KnowledgeCatalogSearchInput {
                query: "客户状态".to_string(),
                limit: Some(20),
                ..input.clone()
            },
            &["客户状态".to_string()],
            None,
            None,
            20,
        )?;
        assert_eq!(
            local_only
                .iter()
                .map(|hit| hit.citation.title.as_str())
                .collect::<Vec<_>>(),
            vec!["订单说明"],
            "关闭 AI 的文档仍须保留在本地标题和全文搜索中"
        );
        let mapped = database.search_knowledge_catalog_page(
            &KnowledgeCatalogSearchInput {
                query: "工单".to_string(),
                limit: Some(20),
                ..input.clone()
            },
            &["工单".to_string(), "订单接口".to_string()],
            None,
            None,
            20,
        )?;
        assert!(
            mapped.iter().any(|hit| hit.citation.title == "订单接口"),
            "扩展词应使标题仅包含代码或别名的文档进入本地检索结果"
        );
        let snapshot_before = database.get_knowledge_catalog_search_snapshot(&input)?;
        let first_version_id = first[0].citation.document_version_id.expect("文档版本 ID");
        {
            let connection = database.conn.lock().map_err(|error| error.to_string())?;
            connection.execute(
                "INSERT INTO knowledge_document_version_bindings
                    (document_version_id, release_id, repository_binding_id, cross_version_scope)
                 VALUES (?1, NULL, NULL, 'project_all_versions')",
                [first_version_id],
            )?;
        }
        assert_ne!(
            snapshot_before,
            database.get_knowledge_catalog_search_snapshot(&input)?
        );
        Ok(())
    }
}
