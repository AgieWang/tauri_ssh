use std::collections::{BTreeMap, HashSet};
use std::path::Path;

use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::database::{
    knowledge::{KnowledgeEmbeddingRebuildCandidate, KnowledgeVectorCandidate},
    Database,
};
use crate::error::AppError;
use crate::models::{
    AiProviderEmbeddingInput, BuildKnowledgeEmbeddingBatchInput, CreateAuditLogInput,
    CreateKnowledgeJobInput, EstimateKnowledgeEmbeddingRebuildInput,
    GenerateKnowledgeLocalEmbeddingsInput, KnowledgeCitation, KnowledgeEmbeddingBatchResult,
    KnowledgeEmbeddingFingerprintInput, KnowledgeEmbeddingIndexAvailability,
    KnowledgeEmbeddingIndexValidation, KnowledgeEmbeddingLifecycleResult,
    KnowledgeEmbeddingProfile, KnowledgeEmbeddingProfileTestResult,
    KnowledgeEmbeddingRebuildEstimate, KnowledgeRemoteRebuildSourceEstimate, KnowledgeSearchHit,
    KnowledgeVectorSearchInput, UpsertKnowledgeEmbeddingProfileInput,
};
use crate::services::ai_provider::AiProviderService;
use crate::services::audit::AuditService;
use crate::services::knowledge_local_embedding::KnowledgeLocalEmbeddingService;
use crate::services::knowledge_policy::{detect_sensitive_content, KnowledgePolicyService};
use crate::services::knowledge_retrieval::KnowledgeRetrievalService;
use crate::services::knowledge_rollout::KnowledgeRolloutService;

pub struct KnowledgeEmbeddingService;

/// 当前受控远程模型的单条输入上限为 512 token。中文和代码的 token 密度可能接近
/// 每字符一个 token，因此按保守字符窗口切片；再合并为原始全文分块的一条向量，避免
/// 为兼容远程模型而改写正在使用的蓝绿索引分块。
const REMOTE_EMBEDDING_SAFE_SEGMENT_CHARS: usize = 400;
const REMOTE_EMBEDDING_SEGMENTS_PER_REQUEST: usize = 8;
const REMOTE_EMBEDDING_MAX_PREFIX_CHARS: usize = 64;

/// Profile 生命周期审计仅记录可公开的 Profile 状态与完整性计数；模型请求、文档
/// 内容、远程端点和凭据均不得进入审计明细。
fn audit_embedding_lifecycle(
    db: &Database,
    action: &str,
    summary: &str,
    result: &KnowledgeEmbeddingLifecycleResult,
) {
    let _ = AuditService::create(
        db,
        CreateAuditLogInput {
            actor: "local-user".to_string(),
            source: "knowledge".to_string(),
            server_alias: String::new(),
            action: action.to_string(),
            risk: "L2".to_string(),
            result: "成功".to_string(),
            summary: summary.to_string(),
            detail_json: Some(
                serde_json::json!({
                    "profileId": result.profile.id,
                    "profileKey": result.profile.profile_key,
                    "mode": result.profile.mode,
                    "status": result.profile.status,
                    "active": result.profile.is_active,
                    "expectedChunks": result.validation.expected_chunks,
                "indexedChunks": result.validation.indexed_chunks,
                    "complete": result.validation.complete,
                })
                .to_string(),
            ),
            request_id: None,
            approval_id: None,
        },
    );
}

/// 只推进已安全落盘或明确跳过的片段检查点。Provider/模型错误前尚未写入的片段不得
/// 提前越过，否则重试会错误跳过缺失向量并在完整性校验前制造不可恢复的缺口。
fn embedding_batch_checkpoint(
    profile_id: i64,
    last_chunk_id: i64,
    processed: i64,
    embedded: i64,
    skipped: i64,
    blocked: i64,
) -> serde_json::Value {
    serde_json::json!({
        "profileId": profile_id,
        "lastChunkId": last_chunk_id,
        "processed": processed,
        "embedded": embedded,
        "skipped": skipped,
        "blocked": blocked,
    })
}

fn with_embedding_prefix(prefix: &str, text: &str) -> String {
    if prefix.is_empty() || text.starts_with(prefix) {
        text.to_string()
    } else {
        format!("{prefix}{text}")
    }
}

/// Profile 保存的维度来自短文本探测的实际响应，是后续响应校验条件而非请求参数。
/// 很多 OpenAI-compatible Embedding 服务（包括固定 384 维的本地模型）不接受
/// `dimensions` 字段；构建与问句检索必须沿用探测时的请求形状。
fn remote_embedding_request(
    profile: &KnowledgeEmbeddingProfile,
    inputs: Vec<String>,
) -> AiProviderEmbeddingInput {
    AiProviderEmbeddingInput {
        provider_key: profile.provider_key.clone(),
        model: Some(profile.model.clone()),
        inputs,
        dimensions: None,
    }
}

fn split_remote_embedding_segments(
    document_prefix: &str,
    content: &str,
) -> Result<Vec<String>, AppError> {
    let prefix_chars = document_prefix.chars().count();
    if prefix_chars > REMOTE_EMBEDDING_MAX_PREFIX_CHARS
        || prefix_chars >= REMOTE_EMBEDDING_SAFE_SEGMENT_CHARS
    {
        return Err(AppError::InvalidInput(
            "远程向量化文本前缀过长，无法满足模型输入限制".to_string(),
        ));
    }
    let content_limit = REMOTE_EMBEDDING_SAFE_SEGMENT_CHARS - prefix_chars;
    let characters = content.chars().collect::<Vec<_>>();
    let segments = characters
        .chunks(content_limit)
        .map(|segment| segment.iter().collect::<String>())
        .filter(|segment| !segment.trim().is_empty())
        .map(|segment| with_embedding_prefix(document_prefix, &segment))
        .collect::<Vec<_>>();
    Ok(segments)
}

fn ensure_remote_embedding_job_not_cancelled(db: &Database, job_id: i64) -> Result<(), AppError> {
    if db.is_knowledge_job_cancel_requested(job_id)? {
        return Err(AppError::InvalidInput("远程向量构建任务已取消".to_string()));
    }
    Ok(())
}

fn merge_remote_embedding_segments(
    vectors: Vec<Vec<f32>>,
    dimension: i64,
) -> Result<Vec<f32>, AppError> {
    let expected_dimension = usize::try_from(dimension)
        .map_err(|_| AppError::InvalidInput("远程向量维度超出范围".to_string()))?;
    if expected_dimension == 0
        || vectors.is_empty()
        || vectors
            .iter()
            .any(|vector| vector.len() != expected_dimension)
    {
        return Err(AppError::InvalidInput(
            "远程向量化返回维度或数量与向量化方案不一致".to_string(),
        ));
    }
    let mut merged = vec![0_f64; expected_dimension];
    for vector in &vectors {
        for (index, value) in vector.iter().enumerate() {
            if !value.is_finite() {
                return Err(AppError::InvalidInput("远程向量化返回非有限数".to_string()));
            }
            merged[index] += f64::from(*value);
        }
    }
    let vector_count = vectors.len() as f64;
    for value in &mut merged {
        *value /= vector_count;
    }
    let norm = merged.iter().map(|value| value * value).sum::<f64>().sqrt();
    if !norm.is_finite() || norm <= 0.0 {
        return Err(AppError::InvalidInput(
            "远程向量化合并结果为零向量".to_string(),
        ));
    }
    Ok(merged
        .into_iter()
        .map(|value| (value / norm) as f32)
        .collect())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CanonicalEmbeddingFingerprint<'a> {
    mode: &'a str,
    provider_protocol: &'a str,
    endpoint_identity: &'a str,
    provider_key: &'a str,
    model: &'a str,
    model_revision: &'a str,
    dimension: i64,
    normalized: bool,
    query_prefix: &'a str,
    document_prefix: &'a str,
    chunk_strategy_id: &'a str,
    normalization_version: &'a str,
}

impl KnowledgeEmbeddingService {
    /// 远程向量能力默认可用；是否允许发送具体正文仍由来源授权、敏感级别和内容
    /// 安全检查共同决定。保留该方法是为了兼容已有 Command/API 调用方，但不再读取
    /// 或写入一个可关闭远程向量的全局开关。
    pub fn remote_embedding_enabled(_db: &Database) -> Result<bool, AppError> {
        Ok(true)
    }

    /// 用当前活动 Profile 生成问句向量。没有活动索引时返回 `None`，调用方可以继续
    /// 使用 FTS 证据；一旦存在活动 Profile，运行时、权限或维度异常必须明确报错，不能
    /// 悄悄改用另一套模型或远程服务。
    pub async fn generate_active_query_embedding(
        db: &Database,
        app_data_dir: &Path,
        question: &str,
    ) -> Result<Option<Vec<f32>>, AppError> {
        KnowledgeRolloutService::require(db, "hybrid_rag")?;
        let question = question.trim();
        if question.is_empty() {
            return Err(AppError::InvalidInput("知识问答问题不能为空".to_string()));
        }
        let Some(profile) = db.get_active_knowledge_embedding_profile()? else {
            return Ok(None);
        };
        if profile.dimension <= 0 {
            return Err(AppError::InvalidInput(
                "当前活动向量化方案缺少有效维度，请重新构建并激活索引".to_string(),
            ));
        }
        Self::require_profile_rollout(db, profile.id)?;
        let query_prefix = profile
            .config
            .get("queryPrefix")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("query: ")
            .to_string();
        let vector = match profile.mode.as_str() {
            "local" => {
                let model_key = profile
                    .config
                    .get("modelKey")
                    .and_then(serde_json::Value::as_str)
                    .filter(|value| !value.trim().is_empty())
                    .unwrap_or(&profile.model)
                    .to_string();
                let mut vectors = KnowledgeLocalEmbeddingService::generate_embeddings(
                    app_data_dir,
                    GenerateKnowledgeLocalEmbeddingsInput {
                        model_key,
                        texts: vec![question.to_string()],
                        prefix: query_prefix,
                        batch_size: Some(1),
                    },
                    || false,
                )?;
                vectors
                    .pop()
                    .ok_or_else(|| AppError::Custom("本地向量化未返回问句向量".to_string()))?
            }
            "remote" => {
                if profile.provider_key.trim().is_empty() {
                    return Err(AppError::InvalidInput(
                        "活动远程向量化方案未配置 AI Provider".to_string(),
                    ));
                }
                let response = AiProviderService::embed_with_preflight(
                    db,
                    remote_embedding_request(
                        &profile,
                        vec![with_embedding_prefix(&query_prefix, question)],
                    ),
                    || Ok(()),
                )
                .await?;
                if response.dimension != profile.dimension || response.vectors.len() != 1 {
                    return Err(AppError::InvalidInput(
                        "远程向量化问句返回维度或数量与活动方案不一致".to_string(),
                    ));
                }
                response
                    .vectors
                    .into_iter()
                    .next()
                    .ok_or_else(|| AppError::Custom("远程向量化未返回问句向量".to_string()))?
            }
            _ => {
                return Err(AppError::InvalidInput(
                    "当前活动向量化方案模式无效".to_string(),
                ));
            }
        };
        if i64::try_from(vector.len()).ok() != Some(profile.dimension)
            || vector.iter().any(|value| !value.is_finite())
        {
            return Err(AppError::InvalidInput(
                "问句向量与活动方案维度不一致或包含无效数值".to_string(),
            ));
        }
        Ok(Some(vector))
    }

    fn require_profile_rollout(db: &Database, profile_id: i64) -> Result<(), AppError> {
        let profile = db
            .get_knowledge_embedding_profile_by_id(profile_id)?
            .ok_or_else(|| AppError::NotFound(format!("向量化方案不存在: {profile_id}")))?;
        KnowledgeRolloutService::require(
            db,
            if profile.mode == "remote" {
                "hybrid_rag"
            } else {
                "local_embedding"
            },
        )
    }

    /// Profile 配置只能在草稿阶段写入；构建后的 Profile 保持不可变，避免 UI 修改导致
    /// 已存向量空间与其指纹不一致。
    pub fn list_profiles(db: &Database) -> Result<Vec<KnowledgeEmbeddingProfile>, AppError> {
        // Profile 列表同时承载远程模式；本地模型关闭时仍应允许查看和管理远程 Profile。
        KnowledgeRolloutService::require(db, "catalog")?;
        db.list_knowledge_embedding_profiles()
    }

    pub fn upsert_profile(
        db: &Database,
        input: UpsertKnowledgeEmbeddingProfileInput,
    ) -> Result<KnowledgeEmbeddingProfile, AppError> {
        KnowledgeRolloutService::require(
            db,
            if input.mode == "remote" {
                "hybrid_rag"
            } else {
                "local_embedding"
            },
        )?;
        if input.profile_key.trim().is_empty() || input.name.trim().is_empty() {
            return Err(AppError::InvalidInput(
                "向量化方案标识和名称不能为空".to_string(),
            ));
        }
        if !matches!(input.mode.as_str(), "local" | "remote") {
            return Err(AppError::InvalidInput(
                "向量化模式仅支持本地或远程".to_string(),
            ));
        }
        if input.model.trim().is_empty() {
            return Err(AppError::InvalidInput("向量化模型不能为空".to_string()));
        }
        let profile = db.upsert_knowledge_embedding_profile(&input)?;
        let _ = AuditService::create(
            db,
            CreateAuditLogInput {
                actor: "local-user".to_string(),
                source: "knowledge".to_string(),
                server_alias: String::new(),
                action: "knowledge_embedding_profile_upsert".to_string(),
                risk: "L1".to_string(),
                result: "成功".to_string(),
                summary: "保存知识向量化方案配置".to_string(),
                detail_json: Some(
                    serde_json::json!({
                        "profileId": profile.id,
                        "profileKey": profile.profile_key,
                        "mode": profile.mode,
                        "status": profile.status,
                    })
                    .to_string(),
                ),
                request_id: None,
                approval_id: None,
            },
        );
        Ok(profile)
    }

    /// 执行一个有上限的本地向量构建批次。每个写入都使用 (chunk_id, profile_id,
    /// content_hash) 幂等约束；应用关闭后可用同一 job_key 从持久化检查点继续。
    pub fn build_local_embedding_batch(
        db: &Database,
        app_data_dir: &Path,
        input: BuildKnowledgeEmbeddingBatchInput,
    ) -> Result<KnowledgeEmbeddingBatchResult, AppError> {
        KnowledgeRolloutService::require(db, "local_embedding")?;
        if input.profile_id <= 0 {
            return Err(AppError::InvalidInput(
                "向量化方案 ID 必须为正数".to_string(),
            ));
        }
        let profile = db
            .get_knowledge_embedding_profile_by_id(input.profile_id)?
            .ok_or_else(|| AppError::NotFound(format!("向量化方案不存在: {}", input.profile_id)))?;
        if profile.mode != "local" || profile.status != "building" || profile.is_active {
            return Err(AppError::InvalidInput(
                "仅允许为非活动的 local building Profile 执行向量构建".to_string(),
            ));
        }
        if profile.dimension <= 0 {
            return Err(AppError::InvalidInput(
                "local Profile 尚未通过真实短文本探测并保存实际维度".to_string(),
            ));
        }
        let job_key = normalized_embedding_job_key(input.job_key.as_deref(), profile.id)?;
        let job = match db.get_knowledge_job(&job_key)? {
            Some(job) => {
                if job.profile_id != Some(profile.id) || job.job_type != "embedding_build" {
                    return Err(AppError::InvalidInput(
                        "向量化任务标识已被其他任务占用".to_string(),
                    ));
                }
                if job.status == "running" {
                    // 运行中的任务只能由已经持有它的执行器推进；第二个调用不能借用同一
                    // checkpoint 并发写向量。应用重启后的任务由恢复流程先转为 interrupted。
                    return Err(AppError::InvalidInput(
                        "本地向量构建任务正在运行，请等待、取消或在恢复后重试".to_string(),
                    ));
                } else {
                    let queued = if job.status == "queued" {
                        job
                    } else if job.status == "completed" {
                        db.restart_completed_knowledge_embedding_job(job.id, profile.id)?
                    } else {
                        db.restart_knowledge_job(job.id)?
                    };
                    db.mark_knowledge_job_running(
                        queued.id,
                        "embedding",
                        "继续本地向量构建",
                        &queued.checkpoint,
                    )?
                }
            }
            None => {
                let checkpoint = serde_json::json!({
                    "profileId": profile.id,
                    "lastChunkId": 0,
                    "processed": 0,
                    "embedded": 0,
                    "skipped": 0,
                    "blocked": 0,
                });
                let created = db.create_knowledge_job(&CreateKnowledgeJobInput {
                    job_key: job_key.clone(),
                    job_type: "embedding_build".to_string(),
                    source_id: None,
                    profile_id: Some(profile.id),
                    message: "等待本地向量构建".to_string(),
                    checkpoint: checkpoint.clone(),
                })?;
                db.mark_knowledge_job_running(
                    created.id,
                    "embedding",
                    "开始本地向量构建",
                    &checkpoint,
                )?
            }
        };
        let mut candidates = Vec::new();
        if let Err(error) =
            db.visit_knowledge_embedding_rebuild_candidates(profile.id, true, |candidate| {
                candidates.push(candidate);
                Ok(())
            })
        {
            finish_embedding_job_error(db, job.id, "failed", &job.checkpoint, &error);
            return Err(error);
        }
        let total_chunks = i64::try_from(candidates.len())
            .map_err(|_| AppError::Custom("知识片段数量超出范围".to_string()))?;
        let checkpoint = job.checkpoint.clone();
        let last_chunk_id = checkpoint
            .get("lastChunkId")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(0);
        let mut processed = checkpoint
            .get("processed")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(0);
        let mut embedded = checkpoint
            .get("embedded")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(0);
        let mut skipped = checkpoint
            .get("skipped")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(0);
        let mut blocked = checkpoint
            .get("blocked")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(0);
        let batch_size = input.batch_size.unwrap_or(16).clamp(1, 64) as usize;
        let selected = candidates
            .into_iter()
            .filter(|candidate| candidate.chunk_id > last_chunk_id)
            .take(batch_size)
            .collect::<Vec<_>>();
        let model_key = profile
            .config
            .get("modelKey")
            .and_then(serde_json::Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(&profile.model)
            .to_string();
        let document_prefix = profile
            .config
            .get("documentPrefix")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("passage: ")
            .to_string();
        let mut pending = Vec::new();
        let mut latest_chunk_id = last_chunk_id;
        for candidate in selected {
            if candidate.existing_embedding_content_hash.as_deref()
                == Some(candidate.content_hash.as_str())
            {
                skipped += 1;
                processed += 1;
                latest_chunk_id = candidate.chunk_id;
            } else if candidate.content.trim().is_empty() {
                blocked += 1;
                processed += 1;
                latest_chunk_id = candidate.chunk_id;
            } else {
                pending.push(candidate);
            }
        }
        if db.is_knowledge_job_cancel_requested(job.id)? {
            let error = AppError::InvalidInput("本地向量构建任务已取消".to_string());
            finish_embedding_job_error(
                db,
                job.id,
                "cancelled",
                &embedding_batch_checkpoint(
                    profile.id,
                    latest_chunk_id,
                    processed,
                    embedded,
                    skipped,
                    blocked,
                ),
                &error,
            );
            return Err(error);
        }
        if !pending.is_empty() {
            let texts = pending
                .iter()
                .map(|candidate| candidate.content.clone())
                .collect::<Vec<_>>();
            let vectors = match KnowledgeLocalEmbeddingService::generate_embeddings(
                app_data_dir,
                crate::models::GenerateKnowledgeLocalEmbeddingsInput {
                    model_key,
                    texts,
                    prefix: document_prefix,
                    batch_size: Some(batch_size as i64),
                },
                || {
                    db.get_knowledge_job_by_id(job.id)
                        .ok()
                        .flatten()
                        .is_some_and(|value| value.cancel_requested)
                },
            ) {
                Ok(vectors) => vectors,
                Err(error) => {
                    let status = if db
                        .is_knowledge_job_cancel_requested(job.id)
                        .unwrap_or(false)
                    {
                        "cancelled"
                    } else {
                        "failed"
                    };
                    finish_embedding_job_error(
                        db,
                        job.id,
                        status,
                        &embedding_batch_checkpoint(
                            profile.id,
                            latest_chunk_id,
                            processed,
                            embedded,
                            skipped,
                            blocked,
                        ),
                        &error,
                    );
                    return Err(error);
                }
            };
            if vectors.len() != pending.len() {
                let error = AppError::Custom("本地向量化返回数量与输入不一致".to_string());
                finish_embedding_job_error(
                    db,
                    job.id,
                    "failed",
                    &embedding_batch_checkpoint(
                        profile.id,
                        latest_chunk_id,
                        processed,
                        embedded,
                        skipped,
                        blocked,
                    ),
                    &error,
                );
                return Err(error);
            }
            for (candidate, vector) in pending.into_iter().zip(vectors) {
                if db.is_knowledge_job_cancel_requested(job.id)? {
                    let error = AppError::InvalidInput("本地向量构建任务已取消".to_string());
                    finish_embedding_job_error(
                        db,
                        job.id,
                        "cancelled",
                        &embedding_batch_checkpoint(
                            profile.id,
                            latest_chunk_id,
                            processed,
                            embedded,
                            skipped,
                            blocked,
                        ),
                        &error,
                    );
                    return Err(error);
                }
                let dimension = match i64::try_from(vector.len()) {
                    Ok(dimension) => dimension,
                    Err(_) => {
                        let error = AppError::InvalidInput("本地向量化维度超出范围".to_string());
                        finish_embedding_job_error(
                            db,
                            job.id,
                            "failed",
                            &embedding_batch_checkpoint(
                                profile.id,
                                latest_chunk_id,
                                processed,
                                embedded,
                                skipped,
                                blocked,
                            ),
                            &error,
                        );
                        return Err(error);
                    }
                };
                if dimension != profile.dimension {
                    let error = AppError::InvalidInput(format!(
                        "本地向量化返回维度与向量化方案不一致: 方案维度={}, 实际={dimension}",
                        profile.dimension
                    ));
                    finish_embedding_job_error(
                        db,
                        job.id,
                        "failed",
                        &embedding_batch_checkpoint(
                            profile.id,
                            latest_chunk_id,
                            processed,
                            embedded,
                            skipped,
                            blocked,
                        ),
                        &error,
                    );
                    return Err(error);
                }
                if let Err(error) = db.upsert_knowledge_chunk_embedding(
                    candidate.chunk_id,
                    profile.id,
                    &candidate.content_hash,
                    &vector,
                ) {
                    let status = if db
                        .is_knowledge_job_cancel_requested(job.id)
                        .unwrap_or(false)
                    {
                        "cancelled"
                    } else {
                        "failed"
                    };
                    finish_embedding_job_error(
                        db,
                        job.id,
                        status,
                        &embedding_batch_checkpoint(
                            profile.id,
                            latest_chunk_id,
                            processed,
                            embedded,
                            skipped,
                            blocked,
                        ),
                        &error,
                    );
                    return Err(error);
                }
                embedded += 1;
                processed += 1;
                latest_chunk_id = candidate.chunk_id;
            }
        }
        let completed = processed >= total_chunks;
        let next_checkpoint = embedding_batch_checkpoint(
            profile.id,
            latest_chunk_id,
            processed,
            embedded,
            skipped,
            blocked,
        );
        if completed {
            if let Err(error) = db.finish_knowledge_job(
                job.id,
                "completed",
                "本地向量构建批次完成，可执行完整性校验",
                None,
                &next_checkpoint,
            ) {
                if db
                    .is_knowledge_job_cancel_requested(job.id)
                    .unwrap_or(false)
                {
                    let cancelled = AppError::InvalidInput("本地向量构建任务已取消".to_string());
                    finish_embedding_job_error(
                        db,
                        job.id,
                        "cancelled",
                        &next_checkpoint,
                        &cancelled,
                    );
                    return Err(cancelled);
                }
                return Err(error);
            }
        } else {
            let updated = db.update_knowledge_job_progress(
                job.id,
                processed,
                total_chunks,
                "已保存本地向量构建检查点",
                &next_checkpoint,
            )?;
            if !updated {
                let error = AppError::InvalidInput("向量构建任务已取消或不再运行".to_string());
                let status = if db
                    .is_knowledge_job_cancel_requested(job.id)
                    .unwrap_or(false)
                {
                    "cancelled"
                } else {
                    "failed"
                };
                finish_embedding_job_error(db, job.id, status, &next_checkpoint, &error);
                return Err(error);
            }
        }
        Ok(KnowledgeEmbeddingBatchResult {
            profile_id: profile.id,
            job_key,
            total_chunks,
            processed_chunks: processed,
            embedded_chunks: embedded,
            skipped_chunks: skipped,
            blocked_chunks: blocked,
            completed,
            checkpoint: next_checkpoint,
        })
    }

    async fn embed_remote_candidate_segments(
        db: &Database,
        job_id: i64,
        profile: &KnowledgeEmbeddingProfile,
        candidate: &KnowledgeEmbeddingRebuildCandidate,
        document_prefix: &str,
    ) -> Result<(Vec<f32>, i64, i64), AppError> {
        let segments = split_remote_embedding_segments(document_prefix, &candidate.content)?;
        if segments.is_empty() {
            return Err(AppError::InvalidInput(
                "远程向量化片段为空，无法构建索引".to_string(),
            ));
        }
        let segment_count = i64::try_from(segments.len())
            .map_err(|_| AppError::InvalidInput("远程向量化子段数量超出范围".to_string()))?;
        let input_characters = segments.iter().try_fold(0_i64, |total, segment| {
            let characters = i64::try_from(segment.chars().count())
                .map_err(|_| AppError::InvalidInput("远程输入字符数超出范围".to_string()))?;
            total
                .checked_add(characters)
                .ok_or_else(|| AppError::InvalidInput("远程输入字符数超出范围".to_string()))
        })?;
        let mut vectors = Vec::with_capacity(segments.len());
        for segment_batch in segments.chunks(REMOTE_EMBEDDING_SEGMENTS_PER_REQUEST) {
            ensure_remote_embedding_job_not_cancelled(db, job_id)?;
            let result = AiProviderService::embed_with_preflight(
                db,
                remote_embedding_request(profile, segment_batch.to_vec()),
                || {
                    ensure_remote_embedding_job_not_cancelled(db, job_id)?;
                    KnowledgePolicyService::sanitize_remote_embedding_content(
                        db,
                        candidate.document_id,
                        &candidate.content,
                    )
                    .map(|_| ())
                },
            )
            .await?;
            if result.dimension != profile.dimension
                || result.vectors.len() != segment_batch.len()
                || result
                    .vectors
                    .iter()
                    .any(|vector| i64::try_from(vector.len()).ok() != Some(profile.dimension))
            {
                return Err(AppError::InvalidInput(
                    "远程向量化返回维度或数量与向量化方案不一致".to_string(),
                ));
            }
            vectors.extend(result.vectors);
        }
        Ok((
            merge_remote_embedding_segments(vectors, profile.dimension)?,
            segment_count,
            input_characters,
        ))
    }

    /// 远程批次必须逐片段经过 PolicyService 后才组成 Provider 请求。这里没有从本地
    /// 失败自动切换到远程的路径；只有调用方明确选择 remote Profile 才会进入本方法。
    /// Provider 返回的全部向量会先做数量和维度校验，再写入 SQLite，防止错误维度留下
    /// 半批次索引。
    pub async fn build_remote_embedding_batch(
        db: &Database,
        input: BuildKnowledgeEmbeddingBatchInput,
    ) -> Result<KnowledgeEmbeddingBatchResult, AppError> {
        KnowledgeRolloutService::require(db, "hybrid_rag")?;
        if input.profile_id <= 0 {
            return Err(AppError::InvalidInput(
                "向量化方案 ID 必须为正数".to_string(),
            ));
        }
        let profile = db
            .get_knowledge_embedding_profile_by_id(input.profile_id)?
            .ok_or_else(|| AppError::NotFound(format!("向量化方案不存在: {}", input.profile_id)))?;
        if profile.mode != "remote" || profile.status != "building" || profile.is_active {
            return Err(AppError::InvalidInput(
                "仅允许为非活动的 remote building Profile 执行远程向量构建".to_string(),
            ));
        }
        if profile.dimension <= 0 || profile.provider_key.trim().is_empty() {
            return Err(AppError::InvalidInput(
                "remote Profile 必须先完成 Provider 短文本测试并保存维度".to_string(),
            ));
        }
        let job_key = normalized_embedding_job_key(input.job_key.as_deref(), profile.id)?;
        let job = match db.get_knowledge_job(&job_key)? {
            Some(job) => {
                if job.profile_id != Some(profile.id) || job.job_type != "embedding_build" {
                    return Err(AppError::InvalidInput(
                        "向量化任务标识已被其他任务占用".to_string(),
                    ));
                }
                if job.status == "running" {
                    return Err(AppError::InvalidInput(
                        "远程向量构建任务正在运行，请等待、停止构建或在恢复后重试".to_string(),
                    ));
                }
                let queued = if job.status == "queued" {
                    job
                } else if job.status == "completed" {
                    db.restart_completed_knowledge_embedding_job(job.id, profile.id)?
                } else {
                    db.restart_knowledge_job(job.id)?
                };
                db.mark_knowledge_job_running(
                    queued.id,
                    "embedding",
                    "继续远程向量构建",
                    &queued.checkpoint,
                )?
            }
            None => {
                let checkpoint = serde_json::json!({
                    "profileId": profile.id,
                    "lastChunkId": 0,
                    "processed": 0,
                    "embedded": 0,
                    "skipped": 0,
                    "blocked": 0,
                });
                let created = db.create_knowledge_job(&CreateKnowledgeJobInput {
                    job_key: job_key.clone(),
                    job_type: "embedding_build".to_string(),
                    source_id: None,
                    profile_id: Some(profile.id),
                    message: "等待远程向量构建".to_string(),
                    checkpoint: checkpoint.clone(),
                })?;
                db.mark_knowledge_job_running(
                    created.id,
                    "embedding",
                    "开始远程向量构建",
                    &checkpoint,
                )?
            }
        };
        let mut candidates = Vec::new();
        if let Err(error) =
            db.visit_knowledge_embedding_rebuild_candidates(profile.id, true, |candidate| {
                candidates.push(candidate);
                Ok(())
            })
        {
            finish_embedding_job_error(db, job.id, "failed", &job.checkpoint, &error);
            return Err(error);
        }
        let total_chunks = i64::try_from(candidates.len())
            .map_err(|_| AppError::Custom("知识片段数量超出范围".to_string()))?;
        let last_chunk_id = job
            .checkpoint
            .get("lastChunkId")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(0);
        // 历史实现按 document_id 排序，却以 chunk_id 作检查点过滤；两种顺序并不
        // 单调，恢复时会跳过尚未处理的片段。以持久化的 content_hash 为唯一完成事实，
        // 每批重新扫描未匹配向量，既可安全续跑，也不会重复外发已完成内容。
        let persisted_chunks = candidates
            .iter()
            .filter(|candidate| {
                candidate.existing_embedding_content_hash.as_deref()
                    == Some(candidate.content_hash.as_str())
            })
            .count();
        let persisted_chunks = i64::try_from(persisted_chunks)
            .map_err(|_| AppError::InvalidInput("已完成向量数量超出范围".to_string()))?;
        let mut processed = persisted_chunks;
        let mut embedded = persisted_chunks;
        let skipped = 0_i64;
        let blocked = job
            .checkpoint
            .get("blocked")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(0);
        let batch_size = input.batch_size.unwrap_or(16).clamp(1, 64) as usize;
        let document_prefix = profile
            .config
            .get("documentPrefix")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("passage: ")
            .to_string();
        let selected = candidates
            .into_iter()
            .filter(|candidate| {
                candidate.existing_embedding_content_hash.as_deref()
                    != Some(candidate.content_hash.as_str())
            })
            .take(batch_size)
            .collect::<Vec<_>>();
        let mut latest_chunk_id = last_chunk_id;
        let mut pending = Vec::new();
        let mut policy_blocked = false;
        for mut candidate in selected {
            let sanitized = match KnowledgePolicyService::sanitize_remote_embedding_content(
                db,
                candidate.document_id,
                &candidate.content,
            ) {
                Ok(value) => value,
                Err(_) => {
                    // 远程正文未获授权时不能把片段计为已处理，否则任务会完成而完整性校验
                    // 永远失败。保留原检查点并终结任务，用户完成来源授权后可安全重试。
                    policy_blocked = true;
                    break;
                }
            };
            candidate.content = sanitized;
            pending.push(candidate);
        }
        if policy_blocked {
            let error = AppError::InvalidInput(
                "远程向量化存在未授权或不安全片段，请完成来源授权后重试".to_string(),
            );
            finish_embedding_job_error(
                db,
                job.id,
                "failed",
                &embedding_batch_checkpoint(
                    profile.id,
                    latest_chunk_id,
                    processed,
                    embedded,
                    skipped,
                    blocked,
                ),
                &error,
            );
            return Err(error);
        }
        if db.is_knowledge_job_cancel_requested(job.id)? {
            let error = AppError::InvalidInput("远程向量构建任务已取消".to_string());
            finish_embedding_job_error(
                db,
                job.id,
                "cancelled",
                &embedding_batch_checkpoint(
                    profile.id,
                    latest_chunk_id,
                    processed,
                    embedded,
                    skipped,
                    blocked,
                ),
                &error,
            );
            return Err(error);
        }
        if !pending.is_empty() {
            let mut input_segments = 0_i64;
            let mut input_characters = 0_i64;
            for candidate in pending {
                if db.is_knowledge_job_cancel_requested(job.id)? {
                    let error = AppError::InvalidInput("远程向量构建任务已取消".to_string());
                    finish_embedding_job_error(
                        db,
                        job.id,
                        "cancelled",
                        &embedding_batch_checkpoint(
                            profile.id,
                            latest_chunk_id,
                            processed,
                            embedded,
                            skipped,
                            blocked,
                        ),
                        &error,
                    );
                    return Err(error);
                }
                let (vector, segment_count, character_count) =
                    match Self::embed_remote_candidate_segments(
                        db,
                        job.id,
                        &profile,
                        &candidate,
                        &document_prefix,
                    )
                    .await
                    {
                        Ok(result) => result,
                        Err(error) => {
                            let status = if db
                                .is_knowledge_job_cancel_requested(job.id)
                                .unwrap_or(false)
                            {
                                "cancelled"
                            } else {
                                "failed"
                            };
                            finish_embedding_job_error(
                                db,
                                job.id,
                                status,
                                &embedding_batch_checkpoint(
                                    profile.id,
                                    latest_chunk_id,
                                    processed,
                                    embedded,
                                    skipped,
                                    blocked,
                                ),
                                &error,
                            );
                            return Err(error);
                        }
                    };
                if let Err(error) = db.upsert_knowledge_chunk_embedding(
                    candidate.chunk_id,
                    profile.id,
                    &candidate.content_hash,
                    &vector,
                ) {
                    let status = if db
                        .is_knowledge_job_cancel_requested(job.id)
                        .unwrap_or(false)
                    {
                        "cancelled"
                    } else {
                        "failed"
                    };
                    finish_embedding_job_error(
                        db,
                        job.id,
                        status,
                        &embedding_batch_checkpoint(
                            profile.id,
                            latest_chunk_id,
                            processed,
                            embedded,
                            skipped,
                            blocked,
                        ),
                        &error,
                    );
                    return Err(error);
                }
                embedded += 1;
                processed += 1;
                latest_chunk_id = candidate.chunk_id;
                input_segments = input_segments.saturating_add(segment_count);
                input_characters = input_characters.saturating_add(character_count);
            }
            let _ = AuditService::create(db, CreateAuditLogInput {
                actor: "local-user".to_string(), source: "knowledge".to_string(), server_alias: String::new(),
                action: "knowledge_remote_embedding_batch".to_string(), risk: "L2".to_string(), result: "成功".to_string(),
                summary: "完成远程向量化批次".to_string(),
                detail_json: Some(serde_json::json!({"profileId": profile.id, "providerKey": profile.provider_key, "model": profile.model, "inputSegments": input_segments, "inputCharacters": input_characters}).to_string()),
                request_id: None, approval_id: None,
            });
        }
        let checkpoint = embedding_batch_checkpoint(
            profile.id,
            latest_chunk_id,
            processed,
            embedded,
            skipped,
            blocked,
        );
        let completed = processed >= total_chunks;
        if completed {
            db.finish_knowledge_job(
                job.id,
                "completed",
                "远程向量构建批次完成，可执行完整性校验",
                None,
                &checkpoint,
            )?;
        } else if !db.update_knowledge_job_progress(
            job.id,
            processed,
            total_chunks,
            "已保存远程向量构建检查点",
            &checkpoint,
        )? {
            let error = AppError::InvalidInput("向量构建任务已取消或不再运行".to_string());
            finish_embedding_job_error(db, job.id, "failed", &checkpoint, &error);
            return Err(error);
        } else if !db.queue_knowledge_job_next_batch(job.id)? {
            let error = AppError::InvalidInput("向量构建任务已取消或不再运行".to_string());
            let status = if db
                .is_knowledge_job_cancel_requested(job.id)
                .unwrap_or(false)
            {
                "cancelled"
            } else {
                "failed"
            };
            finish_embedding_job_error(db, job.id, status, &checkpoint, &error);
            return Err(error);
        }
        Ok(KnowledgeEmbeddingBatchResult {
            profile_id: profile.id,
            job_key,
            total_chunks,
            processed_chunks: processed,
            embedded_chunks: embedded,
            skipped_chunks: skipped,
            blocked_chunks: blocked,
            completed,
            checkpoint,
        })
    }

    /// 远程 Profile 探测只发送固定短文本。探测不读取任何知识文档，因此不会成为绕
    /// 开来源、敏感级别检查的正文发送通道。
    pub async fn test_remote_profile(
        db: &Database,
        profile_id: i64,
    ) -> Result<KnowledgeEmbeddingProfileTestResult, AppError> {
        KnowledgeRolloutService::require(db, "hybrid_rag")?;
        let profile = db
            .get_knowledge_embedding_profile_by_id(profile_id)?
            .ok_or_else(|| AppError::NotFound(format!("向量化方案不存在: {profile_id}")))?;
        if profile.mode != "remote"
            || profile.status != "draft"
            || profile.is_active
            || profile.provider_key.trim().is_empty()
        {
            return Err(AppError::InvalidInput(
                "仅允许测试已指定服务商的非活动远程草稿向量化方案".to_string(),
            ));
        }
        let provider = db.get_ai_provider(&profile.provider_key)?.ok_or_else(|| {
            AppError::NotFound(format!("AI Provider '{}' 不存在", profile.provider_key))
        })?;
        let probe_text = "知识库远程向量化短文本探测".to_string();
        let query_prefix = profile
            .config
            .get("queryPrefix")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("query: ")
            .to_string();
        let response = AiProviderService::embed_with_preflight(
            db,
            remote_embedding_request(
                &profile,
                vec![with_embedding_prefix(&query_prefix, &probe_text)],
            ),
            || Ok(()),
        )
        .await?;
        if response.dimension <= 0 || response.vectors.len() != 1 {
            return Err(AppError::InvalidInput(
                "远程向量化探测未返回有效单向量".to_string(),
            ));
        }
        let document_prefix = profile
            .config
            .get("documentPrefix")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("passage: ")
            .to_string();
        let fingerprint = Self::calculate_fingerprint(&KnowledgeEmbeddingFingerprintInput {
            mode: "remote".to_string(),
            provider_protocol: provider.protocol,
            endpoint_identity: provider.endpoint,
            provider_key: profile.provider_key.clone(),
            model: response.model,
            model_revision: profile.model_revision.clone(),
            dimension: response.dimension,
            normalized: profile.normalized,
            query_prefix,
            document_prefix,
            chunk_strategy_id: profile
                .config
                .get("chunkStrategyId")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("knowledge-structure-chunker-v1")
                .to_string(),
            normalization_version: profile
                .config
                .get("normalizationVersion")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("knowledge-normalize-v1")
                .to_string(),
        })?;
        let profile =
            db.upsert_knowledge_embedding_profile(&UpsertKnowledgeEmbeddingProfileInput {
                id: Some(profile.id),
                profile_key: profile.profile_key,
                name: profile.name,
                mode: profile.mode,
                provider_key: profile.provider_key,
                model: profile.model,
                model_revision: profile.model_revision,
                dimension: response.dimension,
                normalized: profile.normalized,
                config: profile.config,
                fingerprint,
            })?;
        Ok(KnowledgeEmbeddingProfileTestResult {
            profile,
            dimension: response.dimension,
            probe_text,
        })
    }

    /// 仅允许测试尚未构建的本地 Profile。实际响应维度同时写入指纹，避免以配置猜测
    /// 维度后误把不兼容向量空间混入同一 Profile。
    pub fn test_local_profile(
        db: &Database,
        app_data_dir: &Path,
        profile_id: i64,
    ) -> Result<KnowledgeEmbeddingProfileTestResult, AppError> {
        KnowledgeRolloutService::require(db, "local_embedding")?;
        let profile = db
            .get_knowledge_embedding_profile_by_id(profile_id)?
            .ok_or_else(|| AppError::NotFound(format!("向量化方案不存在: {profile_id}")))?;
        if profile.mode != "local" || profile.status != "draft" || profile.is_active {
            return Err(AppError::InvalidInput(
                "仅允许测试非活动的本地草稿向量化方案".to_string(),
            ));
        }
        let model_key = profile
            .config
            .get("modelKey")
            .and_then(serde_json::Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(&profile.model)
            .to_string();
        let query_prefix = profile
            .config
            .get("queryPrefix")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("query: ")
            .to_string();
        let document_prefix = profile
            .config
            .get("documentPrefix")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("passage: ")
            .to_string();
        let probe_text = "知识库本地向量化短文本探测".to_string();
        let vectors = KnowledgeLocalEmbeddingService::generate_embeddings(
            app_data_dir,
            crate::models::GenerateKnowledgeLocalEmbeddingsInput {
                model_key,
                texts: vec![probe_text.clone()],
                prefix: query_prefix.clone(),
                batch_size: Some(1),
            },
            || false,
        )?;
        let dimension = i64::try_from(
            vectors
                .first()
                .ok_or_else(|| AppError::Custom("本地向量化未返回探测向量".to_string()))?
                .len(),
        )
        .map_err(|_| AppError::InvalidInput("本地向量化维度超出范围".to_string()))?;
        let fingerprint = Self::calculate_fingerprint(&KnowledgeEmbeddingFingerprintInput {
            mode: "local".to_string(),
            provider_protocol: "fastembed".to_string(),
            endpoint_identity: String::new(),
            provider_key: profile.provider_key.clone(),
            model: profile.model.clone(),
            model_revision: profile.model_revision.clone(),
            dimension,
            normalized: profile.normalized,
            query_prefix,
            document_prefix,
            chunk_strategy_id: profile
                .config
                .get("chunkStrategyId")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("knowledge-structure-chunker-v1")
                .to_string(),
            normalization_version: profile
                .config
                .get("normalizationVersion")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("knowledge-normalize-v1")
                .to_string(),
        })?;
        let profile =
            db.upsert_knowledge_embedding_profile(&UpsertKnowledgeEmbeddingProfileInput {
                id: Some(profile.id),
                profile_key: profile.profile_key,
                name: profile.name,
                mode: profile.mode,
                provider_key: profile.provider_key,
                model: profile.model,
                model_revision: profile.model_revision,
                dimension,
                normalized: profile.normalized,
                config: profile.config,
                fingerprint,
            })?;
        Ok(KnowledgeEmbeddingProfileTestResult {
            profile,
            dimension,
            probe_text,
        })
    }

    /// 创建独立构建状态。该操作绝不切换当前活动 Profile。
    pub fn begin_profile_rebuild(
        db: &Database,
        profile_id: i64,
    ) -> Result<KnowledgeEmbeddingLifecycleResult, AppError> {
        Self::require_profile_rollout(db, profile_id)?;
        let profile = db.begin_knowledge_embedding_profile_build(profile_id)?;
        let validation = db.validate_knowledge_embedding_profile(profile.id)?;
        let result = KnowledgeEmbeddingLifecycleResult {
            profile,
            validation,
        };
        audit_embedding_lifecycle(
            db,
            "knowledge_profile_rebuild_begin",
            "开始蓝绿 Profile 重建",
            &result,
        );
        Ok(result)
    }

    /// 批量向量写入结束后校验覆盖率；失败 Profile 会被标记 failed，旧索引继续可用。
    pub fn complete_profile_rebuild(
        db: &Database,
        profile_id: i64,
    ) -> Result<KnowledgeEmbeddingLifecycleResult, AppError> {
        Self::require_profile_rollout(db, profile_id)?;
        let validation = db.complete_knowledge_embedding_profile_build(profile_id)?;
        let profile = db
            .get_knowledge_embedding_profile_by_id(profile_id)?
            .ok_or_else(|| AppError::NotFound(format!("向量化方案不存在: {profile_id}")))?;
        let result = KnowledgeEmbeddingLifecycleResult {
            profile,
            validation,
        };
        audit_embedding_lifecycle(
            db,
            "knowledge_profile_rebuild_complete",
            "完成蓝绿 Profile 重建",
            &result,
        );
        Ok(result)
    }

    pub fn validate_profile_rebuild(
        db: &Database,
        profile_id: i64,
    ) -> Result<KnowledgeEmbeddingIndexValidation, AppError> {
        Self::require_profile_rollout(db, profile_id)?;
        db.validate_knowledge_embedding_profile(profile_id)
    }

    /// 通过同一 SQLite 事务激活完整 Profile，旧活动 Profile 自动保留为 ready。
    pub fn activate_profile_rebuild(
        db: &Database,
        profile_id: i64,
    ) -> Result<KnowledgeEmbeddingLifecycleResult, AppError> {
        Self::require_profile_rollout(db, profile_id)?;
        let profile = db.activate_knowledge_embedding_profile(profile_id)?;
        let validation = db.validate_knowledge_embedding_profile(profile.id)?;
        let result = KnowledgeEmbeddingLifecycleResult {
            profile,
            validation,
        };
        audit_embedding_lifecycle(
            db,
            "knowledge_profile_activate",
            "激活知识 Profile",
            &result,
        );
        Ok(result)
    }

    /// 回滚复用正常激活的完整性校验，避免把已过期旧索引重新投入服务。
    pub fn rollback_profile_rebuild(
        db: &Database,
        previous_profile_id: i64,
    ) -> Result<KnowledgeEmbeddingLifecycleResult, AppError> {
        Self::activate_profile_rebuild(db, previous_profile_id)
    }

    /// 清理只能作用于已退出服务的 Profile；活动 Profile 和其向量永不隐式删除。
    pub fn retire_profile_rebuild(
        db: &Database,
        profile_id: i64,
    ) -> Result<KnowledgeEmbeddingLifecycleResult, AppError> {
        Self::require_profile_rollout(db, profile_id)?;
        let profile = db.retire_knowledge_embedding_profile(profile_id)?;
        let validation = db.validate_knowledge_embedding_profile(profile.id)?;
        let result = KnowledgeEmbeddingLifecycleResult {
            profile,
            validation,
        };
        audit_embedding_lifecycle(
            db,
            "knowledge_profile_retire",
            "清理已退出服务的知识 Profile",
            &result,
        );
        Ok(result)
    }

    /// 估算一个目标 Profile 的蓝绿重建成本，不返回知识正文或秘密内容。
    pub fn estimate_rebuild(
        db: &Database,
        input: EstimateKnowledgeEmbeddingRebuildInput,
    ) -> Result<KnowledgeEmbeddingRebuildEstimate, AppError> {
        Self::require_profile_rollout(db, input.profile_id)?;
        if input.profile_id <= 0 {
            return Err(AppError::InvalidInput("向量化方案 ID 必须为正数".into()));
        }
        let target = db
            .get_knowledge_embedding_profile_by_id(input.profile_id)?
            .ok_or_else(|| AppError::NotFound(format!("向量化方案不存在: {}", input.profile_id)))?;
        if target.dimension <= 0 {
            return Err(AppError::InvalidInput(
                "目标向量化方案尚未保存实际维度，无法预估重建磁盘占用".into(),
            ));
        }
        if !matches!(target.mode.as_str(), "local" | "remote") {
            return Err(AppError::InvalidInput(
                "目标向量化方案模式仅支持本地或远程".into(),
            ));
        }

        let remote_enabled = Self::remote_embedding_enabled(db)?;
        let mut workload = RebuildWorkload::new(target.mode.as_str(), remote_enabled);
        db.visit_knowledge_embedding_rebuild_candidates(
            target.id,
            target.mode == "remote",
            |candidate| workload.add(candidate),
        )?;
        let affected_documents = i64::try_from(workload.document_ids.len())
            .map_err(|_| AppError::Custom("受影响文档数量超出范围".into()))?;
        let affected_chunks = workload.affected_chunks;
        let reusable_chunks = workload.reusable_chunks;
        let chunks_to_embed = workload.chunks_to_embed;
        let vector_bytes_per_chunk = target
            .dimension
            .checked_mul(4)
            .ok_or_else(|| AppError::InvalidInput("向量化维度过大，无法预估磁盘占用".into()))?;
        let estimated_index_bytes = affected_chunks
            .checked_mul(vector_bytes_per_chunk)
            .ok_or_else(|| AppError::InvalidInput("知识片段数量过大，无法预估磁盘占用".into()))?;
        let additional_disk_bytes = affected_chunks
            .saturating_sub(workload.existing_rows)
            .checked_mul(vector_bytes_per_chunk)
            .ok_or_else(|| AppError::InvalidInput("知识片段数量过大，无法预估磁盘占用".into()))?;

        let current_index = db
            .get_active_knowledge_embedding_profile()?
            .map(|profile| build_current_index_availability(db, &profile))
            .transpose()?;
        Ok(KnowledgeEmbeddingRebuildEstimate {
            target_profile_id: target.id,
            target_profile_key: target.profile_key,
            target_mode: target.mode.clone(),
            target_dimension: target.dimension,
            affected_documents,
            affected_chunks,
            reusable_chunks,
            chunks_to_embed,
            local_work_chunks: if target.mode == "local" {
                chunks_to_embed
            } else {
                0
            },
            remote_eligible_chunks: workload.remote_eligible_chunks,
            remote_characters: workload.remote_characters,
            remote_blocked_chunks: workload.remote_blocked_chunks,
            estimated_index_bytes,
            additional_disk_bytes,
            requires_remote_confirmation: target.mode == "remote"
                && workload.remote_eligible_chunks > 0,
            remote_sources: workload.remote_sources.into_values().collect(),
            current_index,
        })
    }

    /// 在调用远程 Provider 前执行来源、文档和内容的分层授权检查。
    #[allow(dead_code)]
    pub fn authorize_remote_embedding(
        db: &Database,
        document_id: i64,
        content: &str,
    ) -> Result<(), AppError> {
        KnowledgePolicyService::authorize_remote_embedding(db, document_id, content)
    }

    pub fn calculate_fingerprint(
        input: &KnowledgeEmbeddingFingerprintInput,
    ) -> Result<String, AppError> {
        let mode = input.mode.trim().to_lowercase();
        if !matches!(mode.as_str(), "local" | "remote") {
            return Err(AppError::InvalidInput(
                "向量化模式仅支持本地或远程".to_string(),
            ));
        }
        if input.dimension < 0 {
            return Err(AppError::InvalidInput("向量化维度不能为负数".to_string()));
        }
        let model = required(&input.model, "向量化模型")?;
        let chunk_strategy_id = required(&input.chunk_strategy_id, "分块策略标识")?;
        let normalization_version = required(&input.normalization_version, "规范化版本")?;
        let provider_protocol = input.provider_protocol.trim().to_lowercase();
        let endpoint_identity = input
            .endpoint_identity
            .trim()
            .trim_end_matches('/')
            .to_lowercase();
        if endpoint_identity.contains('@') {
            return Err(AppError::InvalidInput(
                "端点身份不得包含用户名或凭据".to_string(),
            ));
        }
        let canonical = CanonicalEmbeddingFingerprint {
            mode: &mode,
            provider_protocol: &provider_protocol,
            endpoint_identity: &endpoint_identity,
            provider_key: input.provider_key.trim(),
            model: &model,
            model_revision: input.model_revision.trim(),
            dimension: input.dimension,
            normalized: input.normalized,
            query_prefix: &input.query_prefix,
            document_prefix: &input.document_prefix,
            chunk_strategy_id: &chunk_strategy_id,
            normalization_version: &normalization_version,
        };
        let bytes = serde_json::to_vec(&canonical)?;
        Ok(format!("{:x}", Sha256::digest(bytes)))
    }

    pub fn search_active_vectors(
        db: &Database,
        input: KnowledgeVectorSearchInput,
    ) -> Result<Vec<KnowledgeSearchHit>, AppError> {
        KnowledgeRolloutService::require(db, "local_embedding")?;
        let mut input = input;
        input.filters = KnowledgeRetrievalService::apply_hard_filters(db, input.filters)?;
        if input.query_vector.is_empty() {
            return Err(AppError::InvalidInput("查询向量不能为空".to_string()));
        }
        let profile = db
            .get_active_knowledge_embedding_profile()?
            .ok_or_else(|| AppError::NotFound("当前没有活动向量化方案".to_string()))?;
        let dimension = i64::try_from(input.query_vector.len())
            .map_err(|_| AppError::InvalidInput("查询向量维度超出范围".to_string()))?;
        if profile.dimension != dimension {
            return Err(AppError::InvalidInput(format!(
                "查询向量维度与活动向量化方案不匹配: 方案维度={}, 查询维度={dimension}",
                profile.dimension
            )));
        }
        let query_norm = input
            .query_vector
            .iter()
            .map(|value| f64::from(*value) * f64::from(*value))
            .sum::<f64>()
            .sqrt();
        if !query_norm.is_finite() || query_norm <= 0.0 {
            return Err(AppError::InvalidInput(
                "查询向量范数必须是大于 0 的有限数".to_string(),
            ));
        }
        let limit = input.filters.limit.unwrap_or(20).clamp(1, 100);
        let include_context = input.filters.include_context.unwrap_or(false);
        let mut scored = db
            .list_active_knowledge_vector_candidates_filtered(50_000, &input.filters)?
            .into_iter()
            .filter(|candidate| candidate.profile_id == profile.id)
            .filter(|candidate| candidate_matches(candidate, &input))
            .filter_map(|candidate| {
                let score = cosine_similarity(
                    &input.query_vector,
                    query_norm,
                    &candidate.vector,
                    candidate.vector_norm,
                )?;
                Some((score, candidate))
            })
            .collect::<Vec<_>>();
        scored.sort_by(|left, right| {
            right
                .0
                .total_cmp(&left.0)
                .then_with(|| left.1.chunk_id.cmp(&right.1.chunk_id))
        });
        scored
            .into_iter()
            .take(usize::try_from(limit).unwrap_or(100))
            .map(|(score, candidate)| {
                let start_line = candidate
                    .location
                    .get("startLine")
                    .and_then(serde_json::Value::as_i64);
                let end_line = candidate
                    .location
                    .get("endLine")
                    .and_then(serde_json::Value::as_i64);
                let excerpt = candidate.content.chars().take(400).collect::<String>();
                let snapshot_id = candidate
                    .location
                    .get("snapshotId")
                    .and_then(serde_json::Value::as_i64);
                let symbol_key = candidate
                    .location
                    .get("symbolKey")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                let is_code = snapshot_id.is_some();
                Ok(KnowledgeSearchHit {
                    score,
                    channels: vec!["vector".to_string()],
                    citation: KnowledgeCitation {
                        citation_key: if let Some(snapshot_id) = snapshot_id {
                            format!("code:snapshot:{snapshot_id}:chunk:{}", candidate.chunk_id)
                        } else {
                            format!(
                                "document:{}:version:{}:chunk:{}",
                                candidate.document_id,
                                candidate.document_version_id,
                                candidate.chunk_id
                            )
                        },
                        source_type: if is_code {
                            "code_snapshot".to_string()
                        } else {
                            "knowledge_document".to_string()
                        },
                        document_id: Some(candidate.document_id),
                        document_version_id: Some(candidate.document_version_id),
                        chunk_id: Some(candidate.chunk_id),
                        project_id: candidate.project_id,
                        release_id: candidate.release_id,
                        title: candidate.title,
                        logical_path: candidate.logical_path,
                        heading_path: candidate.heading_path,
                        commit_sha: candidate.commit_sha,
                        external_key: String::new(),
                        snapshot_id,
                        symbol_key,
                        start_line,
                        end_line,
                        excerpt,
                    },
                    content: if include_context {
                        candidate.content
                    } else {
                        String::new()
                    },
                    diagnostics: serde_json::json!({
                        "profileId": profile.id,
                        "profileFingerprint": profile.fingerprint,
                    }),
                })
            })
            .collect()
    }
}

/// 批处理失败必须终结持久化任务；否则任务会永久卡在 running，既不能重试也不能从
/// 重启恢复。收尾失败不覆盖原始错误，便于调用方看到真正的模型/维度/写库原因。
fn finish_embedding_job_error(
    db: &Database,
    job_id: i64,
    status: &str,
    checkpoint: &serde_json::Value,
    error: &AppError,
) {
    // 失败任务与 Profile 必须一起结束。否则 UI 会把不可继续的索引显示为 building，
    // 并且蓝绿切换的失败语义无法审计。取消则保留 building，允许用户在安全检查点继续。
    if status == "failed" {
        if let Err(profile_error) = db.fail_knowledge_embedding_profile_for_job(job_id) {
            log::error!("无法将失败的向量 Profile 标记为 failed (job {job_id}): {profile_error}");
        }
    }
    let message = if status == "cancelled" {
        "向量构建已取消，已保存安全检查点"
    } else {
        "向量构建失败，已保存安全检查点"
    };
    if let Err(finish_error) = db.finish_knowledge_job(
        job_id,
        status,
        message,
        Some(&error.to_string()),
        checkpoint,
    ) {
        log::error!("无法结束向量构建任务 {job_id}: {finish_error}");
    }
}

struct RebuildWorkload {
    document_ids: HashSet<i64>,
    affected_chunks: i64,
    reusable_chunks: i64,
    chunks_to_embed: i64,
    existing_rows: i64,
    remote_mode: bool,
    remote_enabled: bool,
    remote_eligible_chunks: i64,
    remote_characters: i64,
    remote_blocked_chunks: i64,
    remote_sources: BTreeMap<(Option<i64>, String, String), KnowledgeRemoteRebuildSourceEstimate>,
}

impl RebuildWorkload {
    fn new(mode: &str, remote_enabled: bool) -> Self {
        Self {
            document_ids: HashSet::new(),
            affected_chunks: 0,
            reusable_chunks: 0,
            chunks_to_embed: 0,
            existing_rows: 0,
            remote_mode: mode == "remote",
            remote_enabled,
            remote_eligible_chunks: 0,
            remote_characters: 0,
            remote_blocked_chunks: 0,
            remote_sources: BTreeMap::new(),
        }
    }

    fn add(&mut self, candidate: KnowledgeEmbeddingRebuildCandidate) -> Result<(), AppError> {
        self.document_ids.insert(candidate.document_id);
        self.affected_chunks = self
            .affected_chunks
            .checked_add(1)
            .ok_or_else(|| AppError::Custom("受影响片段数量超出范围".into()))?;
        if candidate.existing_embedding_content_hash.is_some() {
            self.existing_rows = self
                .existing_rows
                .checked_add(1)
                .ok_or_else(|| AppError::Custom("已有向量数量超出范围".into()))?;
        }
        if candidate.existing_embedding_content_hash.as_deref()
            == Some(candidate.content_hash.as_str())
        {
            self.reusable_chunks = self
                .reusable_chunks
                .checked_add(1)
                .ok_or_else(|| AppError::Custom("可复用片段数量超出范围".into()))?;
            return Ok(());
        }
        self.chunks_to_embed = self
            .chunks_to_embed
            .checked_add(1)
            .ok_or_else(|| AppError::Custom("待向量化片段数量超出范围".into()))?;
        if !self.remote_mode {
            return Ok(());
        }
        let source_key = if candidate.source_key.is_empty() {
            "unassociated".to_string()
        } else {
            candidate.source_key.clone()
        };
        let display_name = if candidate.source_name.is_empty() {
            "未关联知识源".to_string()
        } else {
            candidate.source_name.clone()
        };
        let source = self
            .remote_sources
            .entry((
                candidate.source_id,
                source_key.clone(),
                display_name.clone(),
            ))
            .or_insert_with(|| KnowledgeRemoteRebuildSourceEstimate {
                source_id: candidate.source_id,
                source_key,
                display_name,
                eligible_chunks: 0,
                eligible_characters: 0,
                blocked_chunks: 0,
            });
        let eligible = self.remote_enabled
            && candidate.source_id.is_some()
            && candidate.source_enabled
            && candidate.source_allows_remote_embedding
            && matches!(candidate.sensitivity.as_str(), "public" | "internal")
            && !candidate.content.trim().is_empty()
            && detect_sensitive_content(&candidate.content).is_none();
        if eligible {
            let characters = i64::try_from(candidate.content.chars().count())
                .map_err(|_| AppError::Custom("远程字符数超出范围".into()))?;
            source.eligible_chunks += 1;
            source.eligible_characters = source
                .eligible_characters
                .checked_add(characters)
                .ok_or_else(|| AppError::Custom("远程字符数超出范围".into()))?;
            self.remote_eligible_chunks = self
                .remote_eligible_chunks
                .checked_add(1)
                .ok_or_else(|| AppError::Custom("远程片段数量超出范围".into()))?;
            self.remote_characters = self
                .remote_characters
                .checked_add(characters)
                .ok_or_else(|| AppError::Custom("远程字符数超出范围".into()))?;
        } else {
            source.blocked_chunks += 1;
            self.remote_blocked_chunks = self
                .remote_blocked_chunks
                .checked_add(1)
                .ok_or_else(|| AppError::Custom("远程阻断片段数量超出范围".into()))?;
        }
        Ok(())
    }
}

fn build_current_index_availability(
    db: &Database,
    profile: &crate::models::KnowledgeEmbeddingProfile,
) -> Result<KnowledgeEmbeddingIndexAvailability, AppError> {
    let mut total_chunks = 0_i64;
    let mut indexed_chunks = 0_i64;
    db.visit_knowledge_embedding_rebuild_candidates(profile.id, false, |candidate| {
        total_chunks = total_chunks
            .checked_add(1)
            .ok_or_else(|| AppError::Custom("当前索引片段数量超出范围".into()))?;
        if candidate.existing_embedding_content_hash.as_deref()
            == Some(candidate.content_hash.as_str())
        {
            indexed_chunks = indexed_chunks
                .checked_add(1)
                .ok_or_else(|| AppError::Custom("当前索引向量数量超出范围".into()))?;
        }
        Ok(())
    })?;
    let missing_chunks = total_chunks.saturating_sub(indexed_chunks);
    Ok(KnowledgeEmbeddingIndexAvailability {
        profile_id: profile.id,
        profile_key: profile.profile_key.clone(),
        total_chunks,
        indexed_chunks,
        missing_chunks,
        available: missing_chunks == 0,
    })
}

fn candidate_matches(
    candidate: &KnowledgeVectorCandidate,
    input: &KnowledgeVectorSearchInput,
) -> bool {
    let filters = &input.filters;
    // 候选 SQL 已通过与 FTS 相同的项目/发布版本可见性谓词；全版本文档的
    // `candidate.release_id` 仍为 NULL，仅是其原始版本元数据，不能再据此二次拒绝。
    // 保留这里的其他防御性匹配，以便未来候选来源调整时不会放宽来源、类型或快照范围。
    (filters.project_ids.is_empty()
        || candidate
            .project_id
            .is_some_and(|id| filters.project_ids.contains(&id)))
        && (filters.source_ids.is_empty()
            || candidate
                .source_id
                .is_some_and(|id| filters.source_ids.contains(&id)))
        && (filters.document_types.is_empty()
            || filters.document_types.contains(&candidate.doc_type))
        && (filters.sensitivities.is_empty()
            || filters.sensitivities.contains(&candidate.sensitivity))
        && filters.snapshot_id.is_none_or(|snapshot_id| {
            candidate
                .location
                .get("snapshotId")
                .and_then(serde_json::Value::as_i64)
                == Some(snapshot_id)
        })
}

fn cosine_similarity(
    query: &[f32],
    query_norm: f64,
    candidate: &[f32],
    candidate_norm: f64,
) -> Option<f64> {
    if query.len() != candidate.len() || !candidate_norm.is_finite() || candidate_norm <= 0.0 {
        return None;
    }
    let dot = query
        .iter()
        .zip(candidate)
        .map(|(left, right)| f64::from(*left) * f64::from(*right))
        .sum::<f64>();
    let score = dot / (query_norm * candidate_norm);
    score.is_finite().then_some(score)
}

fn required(value: &str, label: &str) -> Result<String, AppError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(AppError::InvalidInput(format!("{label}不能为空")));
    }
    Ok(value.to_string())
}

fn normalized_embedding_job_key(value: Option<&str>, profile_id: i64) -> Result<String, AppError> {
    let value = value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format!("knowledge-embedding-profile-{profile_id}"));
    if value.len() > 120
        || !value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
    {
        return Err(AppError::InvalidInput(
            "向量化任务标识只能包含字母、数字、连字符和下划线，且不超过 120 个字符".to_string(),
        ));
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::{
        ensure_remote_embedding_job_not_cancelled, merge_remote_embedding_segments,
        remote_embedding_request, split_remote_embedding_segments, with_embedding_prefix,
        KnowledgeEmbeddingService, REMOTE_EMBEDDING_SAFE_SEGMENT_CHARS,
    };
    use crate::database::Database;
    use crate::models::{
        BuildKnowledgeEmbeddingBatchInput, CreateKnowledgeDocumentVersionInput,
        CreateKnowledgeJobInput, EstimateKnowledgeEmbeddingRebuildInput, KnowledgeChunkWriteInput,
        KnowledgeEmbeddingFingerprintInput, KnowledgeEmbeddingProfile,
        UpsertKnowledgeDocumentInput, UpsertKnowledgeEmbeddingProfileInput,
        UpsertKnowledgeSourceInput,
    };

    fn input() -> KnowledgeEmbeddingFingerprintInput {
        KnowledgeEmbeddingFingerprintInput {
            mode: "local".to_string(),
            provider_protocol: "fastembed".to_string(),
            endpoint_identity: String::new(),
            provider_key: "local".to_string(),
            model: "multilingual-e5-small".to_string(),
            model_revision: "main".to_string(),
            dimension: 384,
            normalized: true,
            query_prefix: "query: ".to_string(),
            document_prefix: "passage: ".to_string(),
            chunk_strategy_id: "knowledge-structure-chunker-v1".to_string(),
            normalization_version: "knowledge-normalize-v1".to_string(),
        }
    }

    #[test]
    fn remote_embedding_prefix_is_applied_once() {
        assert_eq!(
            with_embedding_prefix("query: ", "服务器为什么返回 404"),
            "query: 服务器为什么返回 404"
        );
        assert_eq!(
            with_embedding_prefix("passage: ", "passage: 已带前缀"),
            "passage: 已带前缀"
        );
        assert_eq!(with_embedding_prefix("", "原文"), "原文");
    }

    #[test]
    fn remote_embedding_segments_are_token_limit_safe_and_merge_normalized() {
        let prefix = "passage: ";
        let segments = split_remote_embedding_segments(
            prefix,
            &"中".repeat(REMOTE_EMBEDDING_SAFE_SEGMENT_CHARS * 2 + 1),
        )
        .expect("短前缀应能生成安全子段");
        assert_eq!(segments.len(), 3);
        assert!(segments.iter().all(|segment| {
            segment.starts_with(prefix)
                && segment.chars().count() <= REMOTE_EMBEDDING_SAFE_SEGMENT_CHARS
        }));
        assert!(split_remote_embedding_segments(&"x".repeat(65), "正文",).is_err());

        let merged = merge_remote_embedding_segments(vec![vec![1.0, 0.0], vec![0.0, 1.0]], 2)
            .expect("分段向量应能合并");
        assert!((merged[0] - 0.707_106_77).abs() < 0.000_1);
        assert!((merged[1] - 0.707_106_77).abs() < 0.000_1);
    }

    #[test]
    fn remote_embedding_stops_before_send_when_job_is_cancelled(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let database = Database::init(":memory:")?;
        let job = database.create_knowledge_job(&CreateKnowledgeJobInput {
            job_key: "remote-embedding-cancelled".to_string(),
            job_type: "embedding_build".to_string(),
            source_id: None,
            profile_id: Some(1),
            message: "排队".to_string(),
            checkpoint: serde_json::json!({}),
        })?;
        database.request_knowledge_job_cancel(job.id)?;
        assert!(ensure_remote_embedding_job_not_cancelled(&database, job.id).is_err());
        Ok(())
    }

    #[test]
    fn remote_embedding_request_uses_probe_compatible_payload() {
        let profile = KnowledgeEmbeddingProfile {
            id: 1,
            profile_key: "remote-e5".to_string(),
            name: "远程语义检索".to_string(),
            mode: "remote".to_string(),
            provider_key: "multilingual-e5-small-int8".to_string(),
            model: "multilingual-e5-small-int8".to_string(),
            model_revision: String::new(),
            dimension: 384,
            normalized: true,
            config: serde_json::json!({}),
            fingerprint: "remote-e5-fingerprint".to_string(),
            status: "draft".to_string(),
            is_active: false,
            created_at: String::new(),
            updated_at: String::new(),
        };

        let request = remote_embedding_request(&profile, vec!["passage: 正文".to_string()]);
        assert_eq!(request.provider_key, profile.provider_key);
        assert_eq!(request.model.as_deref(), Some(profile.model.as_str()));
        assert_eq!(request.inputs, vec!["passage: 正文"]);
        assert_eq!(request.dimensions, None);
    }

    #[test]
    fn remote_embedding_is_always_available_even_with_legacy_config(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let db = Database::init(":memory:")?;
        assert!(KnowledgeEmbeddingService::remote_embedding_enabled(&db)?);

        // 兼容旧数据库：历史遗留的 false 配置也不能再关闭远程向量能力。
        db.set_config("knowledge.remote_embedding.enabled", "false")?;
        assert!(KnowledgeEmbeddingService::remote_embedding_enabled(&db)?);
        Ok(())
    }

    #[test]
    fn fingerprint_is_deterministic_and_covers_vector_compatibility_fields(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let baseline = KnowledgeEmbeddingService::calculate_fingerprint(&input())?;
        assert_eq!(
            baseline,
            KnowledgeEmbeddingService::calculate_fingerprint(&input())?
        );
        let mut changed = input();
        changed.query_prefix = "检索: ".to_string();
        assert_ne!(
            baseline,
            KnowledgeEmbeddingService::calculate_fingerprint(&changed)?
        );
        changed = input();
        changed.chunk_strategy_id = "knowledge-structure-chunker-v2".to_string();
        assert_ne!(
            baseline,
            KnowledgeEmbeddingService::calculate_fingerprint(&changed)?
        );
        changed = input();
        changed.normalization_version = "knowledge-normalize-v2".to_string();
        assert_ne!(
            baseline,
            KnowledgeEmbeddingService::calculate_fingerprint(&changed)?
        );
        Ok(())
    }

    #[test]
    fn remote_embedding_requires_all_authorization_layers() -> Result<(), Box<dyn std::error::Error>>
    {
        let database = Database::init(":memory:")?;
        let source = database.upsert_knowledge_source(&UpsertKnowledgeSourceInput {
            id: None,
            source_key: "remote-source".to_string(),
            project_id: None,
            source_type: "manual_markdown".to_string(),
            display_name: "远程测试来源".to_string(),
            root_path: String::new(),
            git_workspace_key: String::new(),
            include_globs: Vec::new(),
            exclude_globs: Vec::new(),
            version_strategy: "unversioned".to_string(),
            sync_mode: "manual".to_string(),
            allow_remote_embedding: false,
            enabled: true,
        })?;
        let document = database.upsert_knowledge_document(&UpsertKnowledgeDocumentInput {
            id: None,
            document_key: "remote-document".to_string(),
            project_id: None,
            source_id: Some(source.id),
            doc_type: "requirement".to_string(),
            title: "退款审批".to_string(),
            logical_path: "REQ-1.md".to_string(),
            sensitivity: "internal".to_string(),
            tags: Vec::new(),
            allow_ai: true,
            allow_mcp: false,
        })?;

        assert!(KnowledgeEmbeddingService::authorize_remote_embedding(
            &database,
            document.id,
            "退款需要人工审批",
        )
        .is_err());

        let source = database.upsert_knowledge_source(&UpsertKnowledgeSourceInput {
            allow_remote_embedding: true,
            ..UpsertKnowledgeSourceInput {
                id: Some(source.id),
                source_key: source.source_key,
                project_id: source.project_id,
                source_type: source.source_type,
                display_name: source.display_name,
                root_path: source.root_path,
                git_workspace_key: source.git_workspace_key,
                include_globs: source.include_globs,
                exclude_globs: source.exclude_globs,
                version_strategy: source.version_strategy,
                sync_mode: source.sync_mode,
                allow_remote_embedding: false,
                enabled: source.enabled,
            }
        })?;
        let restricted = database.upsert_knowledge_document(&UpsertKnowledgeDocumentInput {
            id: Some(document.id),
            document_key: document.document_key,
            project_id: document.project_id,
            source_id: document.source_id,
            doc_type: document.doc_type,
            title: document.title,
            logical_path: document.logical_path,
            sensitivity: "restricted".to_string(),
            tags: document.tags,
            allow_ai: document.allow_ai,
            allow_mcp: document.allow_mcp,
        })?;
        assert!(KnowledgeEmbeddingService::authorize_remote_embedding(
            &database,
            restricted.id,
            "退款需要人工审批",
        )
        .is_err());

        let approved = database.upsert_knowledge_document(&UpsertKnowledgeDocumentInput {
            id: Some(restricted.id),
            document_key: restricted.document_key,
            project_id: restricted.project_id,
            source_id: Some(source.id),
            doc_type: restricted.doc_type,
            title: restricted.title,
            logical_path: restricted.logical_path,
            sensitivity: "internal".to_string(),
            tags: restricted.tags,
            allow_ai: restricted.allow_ai,
            allow_mcp: restricted.allow_mcp,
        })?;
        // 旧版曾写入全局 false；远程向量是否允许发送只由来源与内容策略决定。
        database.set_config("knowledge.remote_embedding.enabled", "false")?;
        KnowledgeEmbeddingService::authorize_remote_embedding(
            &database,
            approved.id,
            "退款需要人工审批",
        )?;
        assert!(KnowledgeEmbeddingService::authorize_remote_embedding(
            &database,
            approved.id,
            "api_key=do-not-send",
        )
        .is_err());
        Ok(())
    }

    #[test]
    fn rebuild_estimate_aggregates_work_and_blocks_ineligible_remote_chunks(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let database = Database::init(":memory:")?;
        let source = database.upsert_knowledge_source(&UpsertKnowledgeSourceInput {
            id: None,
            source_key: "estimate-source".to_string(),
            project_id: None,
            source_type: "manual_markdown".to_string(),
            display_name: "预估来源".to_string(),
            root_path: String::new(),
            git_workspace_key: String::new(),
            include_globs: Vec::new(),
            exclude_globs: Vec::new(),
            version_strategy: "unversioned".to_string(),
            sync_mode: "manual".to_string(),
            allow_remote_embedding: true,
            enabled: true,
        })?;
        let approved_document =
            database.upsert_knowledge_document(&UpsertKnowledgeDocumentInput {
                id: None,
                document_key: "estimate-approved".to_string(),
                project_id: None,
                source_id: Some(source.id),
                doc_type: "requirement".to_string(),
                title: "可远程文档".to_string(),
                logical_path: "approved.md".to_string(),
                sensitivity: "internal".to_string(),
                tags: Vec::new(),
                allow_ai: true,
                allow_mcp: false,
            })?;
        let restricted_document =
            database.upsert_knowledge_document(&UpsertKnowledgeDocumentInput {
                id: None,
                document_key: "estimate-restricted".to_string(),
                project_id: None,
                source_id: Some(source.id),
                doc_type: "requirement".to_string(),
                title: "受限文档".to_string(),
                logical_path: "restricted.md".to_string(),
                sensitivity: "restricted".to_string(),
                tags: Vec::new(),
                allow_ai: false,
                allow_mcp: false,
            })?;
        for (document, content, hash) in [
            (
                approved_document,
                "可以发送的正文",
                "estimate-approved-chunk",
            ),
            (
                restricted_document,
                "不得发送的正文",
                "estimate-restricted-chunk",
            ),
        ] {
            database.create_knowledge_document_version(
                &CreateKnowledgeDocumentVersionInput {
                    document_id: document.id,
                    release_id: None,
                    version_label: "unversioned".to_string(),
                    git_branch: String::new(),
                    commit_sha: String::new(),
                    source_path: document.logical_path.clone(),
                    mime_type: "text/markdown".to_string(),
                    content: content.to_string(),
                    content_hash: format!("{hash}-version"),
                    parsed_meta: serde_json::json!({}),
                    token_estimate: 4,
                },
                &[KnowledgeChunkWriteInput {
                    chunk_index: 0,
                    heading_path: document.title.clone(),
                    content: content.to_string(),
                    content_hash: hash.to_string(),
                    location: serde_json::json!({"startLine": 1, "endLine": 1}),
                    token_estimate: 4,
                }],
            )?;
        }
        let profile =
            database.upsert_knowledge_embedding_profile(&UpsertKnowledgeEmbeddingProfileInput {
                id: None,
                profile_key: "estimate-remote".to_string(),
                name: "远程预估".to_string(),
                mode: "remote".to_string(),
                provider_key: "provider-a".to_string(),
                model: "text-embedding-3-small".to_string(),
                model_revision: String::new(),
                dimension: 3,
                normalized: true,
                config: serde_json::json!({}),
                fingerprint: "estimate-remote-fingerprint".to_string(),
            })?;
        let estimate = KnowledgeEmbeddingService::estimate_rebuild(
            &database,
            EstimateKnowledgeEmbeddingRebuildInput {
                profile_id: profile.id,
            },
        )?;
        assert_eq!(estimate.affected_documents, 2);
        assert_eq!(estimate.affected_chunks, 2);
        assert_eq!(estimate.chunks_to_embed, 2);
        assert_eq!(estimate.remote_eligible_chunks, 1);
        assert_eq!(
            estimate.remote_characters,
            "可以发送的正文".chars().count() as i64
        );
        assert_eq!(estimate.remote_blocked_chunks, 1);
        assert_eq!(estimate.estimated_index_bytes, 24);
        assert_eq!(estimate.additional_disk_bytes, 24);
        assert!(estimate.requires_remote_confirmation);
        assert!(estimate.current_index.is_none());

        let local_profile =
            database.upsert_knowledge_embedding_profile(&UpsertKnowledgeEmbeddingProfileInput {
                id: None,
                profile_key: "estimate-local".to_string(),
                name: "本地预估".to_string(),
                mode: "local".to_string(),
                provider_key: "local".to_string(),
                model: "multilingual-e5-small".to_string(),
                model_revision: String::new(),
                dimension: 4,
                normalized: true,
                config: serde_json::json!({}),
                fingerprint: "estimate-local-fingerprint".to_string(),
            })?;
        let local_estimate = KnowledgeEmbeddingService::estimate_rebuild(
            &database,
            EstimateKnowledgeEmbeddingRebuildInput {
                profile_id: local_profile.id,
            },
        )?;
        assert_eq!(local_estimate.local_work_chunks, 2);
        assert_eq!(local_estimate.remote_characters, 0);
        assert_eq!(local_estimate.estimated_index_bytes, 32);
        assert!(!local_estimate.requires_remote_confirmation);
        Ok(())
    }

    #[test]
    fn local_batch_reuses_matching_content_hash_and_persists_completion_checkpoint(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let database = Database::init(":memory:")?;
        let document = database.upsert_knowledge_document(&UpsertKnowledgeDocumentInput {
            id: None,
            document_key: "batch-document".to_string(),
            project_id: None,
            source_id: None,
            doc_type: "requirement".to_string(),
            title: "批量向量".to_string(),
            logical_path: "batch.md".to_string(),
            sensitivity: "internal".to_string(),
            tags: Vec::new(),
            allow_ai: true,
            allow_mcp: false,
        })?;
        let version = database.create_knowledge_document_version(
            &CreateKnowledgeDocumentVersionInput {
                document_id: document.id,
                release_id: None,
                version_label: "unversioned".to_string(),
                git_branch: String::new(),
                commit_sha: String::new(),
                source_path: "batch.md".to_string(),
                mime_type: "text/markdown".to_string(),
                content: "批量向量正文".to_string(),
                content_hash: "batch-version".to_string(),
                parsed_meta: serde_json::json!({}),
                token_estimate: 4,
            },
            &[KnowledgeChunkWriteInput {
                chunk_index: 0,
                heading_path: "批量".to_string(),
                content: "已存在向量的片段".to_string(),
                content_hash: "batch-chunk".to_string(),
                location: serde_json::json!({}),
                token_estimate: 4,
            }],
        )?;
        let profile =
            database.upsert_knowledge_embedding_profile(&UpsertKnowledgeEmbeddingProfileInput {
                id: None,
                profile_key: "batch-profile".to_string(),
                name: "Batch Profile".to_string(),
                mode: "local".to_string(),
                provider_key: "local".to_string(),
                model: "offline-model".to_string(),
                model_revision: String::new(),
                dimension: 2,
                normalized: true,
                config: serde_json::json!({}),
                fingerprint: "batch-profile-fingerprint".to_string(),
            })?;
        database.begin_knowledge_embedding_profile_build(profile.id)?;
        let chunk = database.list_knowledge_chunks(version.id)?[0].clone();
        database.upsert_knowledge_chunk_embedding(
            chunk.id,
            profile.id,
            &chunk.content_hash,
            &[1.0, 0.0],
        )?;
        let app_data =
            std::env::temp_dir().join(format!("tauri-ssh-embedding-batch-{}", std::process::id()));
        let result = KnowledgeEmbeddingService::build_local_embedding_batch(
            &database,
            &app_data,
            BuildKnowledgeEmbeddingBatchInput {
                profile_id: profile.id,
                job_key: Some("batch-reuse-test".to_string()),
                batch_size: Some(8),
            },
        )?;
        assert!(result.completed);
        assert_eq!(result.embedded_chunks, 0);
        assert_eq!(result.skipped_chunks, 1);
        assert_eq!(result.checkpoint["lastChunkId"], chunk.id);
        assert_eq!(
            database
                .get_knowledge_job("batch-reuse-test")?
                .ok_or("缺少向量构建任务")?
                .status,
            "completed"
        );

        // 缓存中不存在模型时，底层推理失败也必须终结任务，不能遗留 running 状态。
        let failure_version = database.create_knowledge_document_version(
            &CreateKnowledgeDocumentVersionInput {
                document_id: document.id,
                release_id: None,
                version_label: "unversioned-failure".to_string(),
                git_branch: String::new(),
                commit_sha: "failure-commit".to_string(),
                source_path: "batch-failure.md".to_string(),
                mime_type: "text/markdown".to_string(),
                content: "需要真实本地模型的批量向量正文".to_string(),
                content_hash: "batch-failure-version".to_string(),
                parsed_meta: serde_json::json!({}),
                token_estimate: 6,
            },
            &[KnowledgeChunkWriteInput {
                chunk_index: 0,
                heading_path: "失败路径".to_string(),
                content: "没有导入模型时应安全失败并保留检查点".to_string(),
                content_hash: "batch-failure-chunk".to_string(),
                location: serde_json::json!({}),
                token_estimate: 6,
            }],
        )?;
        assert!(KnowledgeEmbeddingService::build_local_embedding_batch(
            &database,
            &app_data,
            BuildKnowledgeEmbeddingBatchInput {
                profile_id: profile.id,
                job_key: Some("batch-failure-test".to_string()),
                batch_size: Some(8),
            },
        )
        .is_err());
        let failed = database
            .get_knowledge_job("batch-failure-test")?
            .ok_or("缺少失败向量构建任务")?;
        assert_eq!(failed.status, "failed");
        assert!(failed.checkpoint["processed"].as_i64().unwrap_or_default() > 0);
        assert_eq!(
            failed.checkpoint["lastChunkId"].as_i64(),
            Some(chunk.id),
            "失败前未写入的片段不能被检查点越过"
        );
        assert_eq!(
            database
                .get_knowledge_embedding_profile_by_id(profile.id)?
                .ok_or("缺少失败 Profile")?
                .status,
            "failed",
            "真实本地批处理失败必须同步终结非活动蓝绿 Profile"
        );

        // 用同一持久化 jobKey 从安全检查点恢复：模拟用户修复本地模型环境后，已成功
        // 导入该片段的向量。批处理必须继续处理失败片段，而不能因错误检查点跳过它。
        let failure_chunk = database.list_knowledge_chunks(failure_version.id)?[0].clone();
        database.begin_knowledge_embedding_profile_build(profile.id)?;
        database.upsert_knowledge_chunk_embedding(
            failure_chunk.id,
            profile.id,
            &failure_chunk.content_hash,
            &[0.0, 1.0],
        )?;
        let resumed = KnowledgeEmbeddingService::build_local_embedding_batch(
            &database,
            &app_data,
            BuildKnowledgeEmbeddingBatchInput {
                profile_id: profile.id,
                job_key: Some("batch-failure-test".to_string()),
                batch_size: Some(8),
            },
        )?;
        assert!(resumed.completed);
        assert_eq!(
            resumed.checkpoint["lastChunkId"].as_i64(),
            Some(failure_chunk.id)
        );
        assert!(
            database
                .complete_knowledge_embedding_profile_build(profile.id)?
                .complete
        );
        Ok(())
    }
}
