use std::collections::{BTreeSet, HashMap};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use base64::{engine::general_purpose, Engine as _};
use rand::RngCore;
use sha2::{Digest, Sha256};

use crate::database::knowledge_domain::documents::{
    CommitKnowledgeDocumentDraft, CreateKnowledgeDocumentUpload, KnowledgeAssetRecord,
    NewKnowledgeAsset,
};
use crate::database::Database;
use crate::error::AppError;
use crate::models::{
    CommitKnowledgeDocumentDraftInput, CompareKnowledgeDocumentVersionsInput, KnowledgeDocument,
    KnowledgeDocumentCommitResult, KnowledgeDocumentComparison,
    KnowledgeDocumentDeletionImpactPreview, KnowledgeDocumentDetail, KnowledgeDocumentDraft,
    KnowledgeDocumentDraftInput, KnowledgeDocumentDraftSaveResult, KnowledgeDocumentImagePreview,
    KnowledgeDocumentUploadBatchItemResult, KnowledgeDocumentUploadBatchResult,
    KnowledgeDocumentUploadResult, KnowledgeDocumentVersion, KnowledgeListInput, KnowledgePage,
    PreparedKnowledgeUploadDirectory, PreparedKnowledgeUploadFile, RestoreKnowledgeDocumentResult,
    RestoreKnowledgeDocumentVersionToDraftInput, RestoreKnowledgeDocumentVersionToDraftResult,
    UploadKnowledgeAssetBatchInput, UploadKnowledgeAssetInput,
};
use crate::services::knowledge::{audit_knowledge, required_text, validate_positive_id};
use crate::services::knowledge_domain::upload_validation::{
    is_supported_directory_upload_extension, validate_image_pixel_limits, validate_upload_file,
    MAX_ASSET_SIZE_BYTES, MAX_UPLOAD_BATCH_FILES, MAX_UPLOAD_BATCH_SIZE_BYTES,
    MAX_UPLOAD_DIRECTORY_DEPTH,
};
use crate::services::knowledge_parser::inspect_image_metadata;
use crate::services::knowledge_policy::KnowledgePolicyService;
use crate::services::knowledge_rollout::KnowledgeRolloutService;

pub(crate) const DOMAIN: &str = "documents";

/// 原始附件只允许保存在应用数据目录的受控子目录中。文件名不参与存储路径，避免
/// 用户输入、路径分隔符或同名文件影响存储范围与去重结果。
const ASSET_STORAGE_DIRECTORY: &str = "knowledge-assets";
const SHA256_DIRECTORY: &str = "sha256";
const COPY_BUFFER_SIZE: usize = 64 * 1024;
const UPLOAD_GRANT_TTL_MINUTES: i64 = 10;
const MAX_IMAGE_PREVIEW_BYTES: i64 = 5 * 1024 * 1024;
static STAGING_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// 选择文件后在内存中保存短期授权；句柄只能消费一次，前端和持久化数据均不保存绝对路径。
#[derive(Default)]
pub(crate) struct KnowledgeUploadGrantRegistry {
    grants: Mutex<HashMap<String, GrantedKnowledgeUploadFile>>,
}

struct GrantedKnowledgeUploadFile {
    source_path: PathBuf,
    size_bytes: u64,
    expires_at: chrono::DateTime<chrono::Utc>,
}

impl KnowledgeUploadGrantRegistry {
    pub(crate) fn grant(
        &self,
        selected_path: &Path,
    ) -> Result<PreparedKnowledgeUploadFile, AppError> {
        let source_path = validate_source_file(selected_path)?;
        let metadata = fs::metadata(&source_path)?;
        let display_name = source_path
            .file_name()
            .and_then(|name| name.to_str())
            .map(normalize_file_name)
            .transpose()?
            .ok_or_else(|| AppError::InvalidInput("上传文件缺少可用名称".to_string()))?;
        // 文件选择阶段先拒绝伪造类型和恶意容器；提交阶段还会再次检查，防止选择后替换。
        validate_upload_file(&source_path, &display_name)?;
        let mut random = [0_u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut random);
        let handle = format!("upload-{:x}", Sha256::digest(random));
        let mut grants = self
            .grants
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let now = chrono::Utc::now();
        grants.retain(|_, grant| grant.expires_at > now);
        grants.insert(
            handle.clone(),
            GrantedKnowledgeUploadFile {
                source_path,
                size_bytes: metadata.len(),
                expires_at: now + chrono::Duration::minutes(UPLOAD_GRANT_TTL_MINUTES),
            },
        );
        Ok(PreparedKnowledgeUploadFile {
            file_handle: handle,
            display_name,
            size_bytes: i64::try_from(metadata.len())
                .map_err(|_| AppError::InvalidInput("文件大小超出支持范围".to_string()))?,
        })
    }

    pub(crate) fn grant_directory(
        &self,
        selected_path: &Path,
    ) -> Result<PreparedKnowledgeUploadDirectory, AppError> {
        let root = validate_source_directory(selected_path)?;
        let directory_name = root
            .file_name()
            .and_then(|name| name.to_str())
            .map(normalize_file_name)
            .transpose()?
            .unwrap_or_else(|| "未命名文件夹".to_string());
        let mut candidates = Vec::new();
        let mut skipped_count = 0_usize;
        collect_directory_upload_files(&root, &root, 0, &mut candidates, &mut skipped_count)?;
        candidates.sort_by(|left, right| left.to_string_lossy().cmp(&right.to_string_lossy()));

        let mut files = Vec::new();
        let mut total_size_bytes = 0_u64;
        for candidate in candidates {
            let relative_name = match directory_relative_display_name(&root, &candidate) {
                Ok(name) => name,
                Err(AppError::InvalidInput(_)) => {
                    skipped_count = skipped_count.saturating_add(1);
                    continue;
                }
                Err(error) => return Err(error),
            };
            let mut prepared = match self.grant(&candidate) {
                Ok(prepared) => prepared,
                Err(AppError::InvalidInput(_)) => {
                    skipped_count = skipped_count.saturating_add(1);
                    continue;
                }
                Err(error) => return Err(error),
            };
            total_size_bytes = total_size_bytes
                .checked_add(prepared.size_bytes as u64)
                .ok_or_else(|| AppError::InvalidInput("文件夹总大小超出支持范围".to_string()))?;
            if total_size_bytes > MAX_UPLOAD_BATCH_SIZE_BYTES {
                return Err(AppError::InvalidInput(
                    "文件夹内可上传文件总大小不能超过 100MB".to_string(),
                ));
            }
            // 用相对路径区分同名资源；提交时后端仍会将其规范化为安全文件名。
            prepared.display_name = relative_name;
            files.push(prepared);
            if files.len() > MAX_UPLOAD_BATCH_FILES {
                return Err(AppError::InvalidInput(
                    "文件夹最多支持 50 个可上传文件".to_string(),
                ));
            }
        }
        if files.is_empty() {
            return Err(AppError::InvalidInput(
                "文件夹中没有可上传的文档或 HTML 原型资源".to_string(),
            ));
        }
        Ok(PreparedKnowledgeUploadDirectory {
            directory_name,
            files,
            skipped_count: i64::try_from(skipped_count)
                .map_err(|_| AppError::InvalidInput("跳过文件数量超出支持范围".to_string()))?,
            total_size_bytes: i64::try_from(total_size_bytes)
                .map_err(|_| AppError::InvalidInput("文件夹总大小超出支持范围".to_string()))?,
        })
    }

    fn consume(&self, handle: &str) -> Result<PathBuf, AppError> {
        if handle.trim().is_empty() || handle.len() > 128 {
            return Err(AppError::InvalidInput("上传文件句柄无效".to_string()));
        }
        let mut grants = self
            .grants
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let now = chrono::Utc::now();
        grants.retain(|_, grant| grant.expires_at > now);
        let grant = grants
            .remove(handle)
            .ok_or_else(|| AppError::InvalidInput("上传文件已失效，请重新选择文件".to_string()))?;
        Ok(grant.source_path)
    }

    fn total_size(&self, handles: &[String]) -> Result<u64, AppError> {
        let grants = self
            .grants
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let now = chrono::Utc::now();
        handles.iter().try_fold(0_u64, |total, handle| {
            let Some(grant) = grants.get(handle).filter(|grant| grant.expires_at > now) else {
                // 失效句柄保留给逐项导入路径报告，不能使同批其他文件失去处理机会。
                return Ok(total);
            };
            total
                .checked_add(grant.size_bytes)
                .ok_or_else(|| AppError::InvalidInput("上传文件总大小超出支持范围".to_string()))
        })
    }
}

/// 已登记的受控附件。前端和后续解析流程只使用稳定键，不接触应用数据目录的绝对路径。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StoredKnowledgeAsset {
    pub id: i64,
    pub asset_key: String,
    pub content_hash: String,
    pub storage_key: String,
    pub original_name: String,
    pub normalized_name: String,
    pub mime_type: String,
    pub size_bytes: i64,
    pub reused_existing_content: bool,
}

pub(crate) struct KnowledgeDocumentService;

impl KnowledgeDocumentService {
    /// 普通目录列表是内容输出入口的一部分，因此在 DAO 完成分页前过滤 restricted，
    /// 不能先分页后在前端删除，否则 total 与页面会泄露受限记录的存在。
    pub(crate) fn list_documents(
        db: &Database,
        input: Option<KnowledgeListInput>,
    ) -> Result<KnowledgePage<KnowledgeDocument>, AppError> {
        KnowledgeRolloutService::require(db, "catalog")?;
        db.list_visible_knowledge_documents(&input.unwrap_or_else(empty_list_input))
    }

    /// 回收站沿用与普通目录相同的项目、版本、关键字与敏感级别约束，仅改变软删除状态。
    pub(crate) fn list_deleted_documents(
        db: &Database,
        input: Option<KnowledgeListInput>,
    ) -> Result<KnowledgePage<KnowledgeDocument>, AppError> {
        KnowledgeRolloutService::require(db, "catalog")?;
        db.list_deleted_visible_knowledge_documents(&input.unwrap_or_else(empty_list_input))
    }

    /// 处理中上传只返回原文件元数据、任务和可操作状态，绝不伪造解析正文；正式可读版本
    /// 仍必须经过正文输出策略，受限文档保持不可从详情或历史接口读取。
    pub(crate) fn get_document_detail(
        db: &Database,
        document_id: i64,
    ) -> Result<KnowledgeDocumentDetail, AppError> {
        KnowledgeRolloutService::require(db, "catalog")?;
        validate_positive_id(document_id, "知识文档 ID")?;
        let document = db
            .get_knowledge_document_by_id(document_id)?
            .ok_or_else(|| AppError::NotFound(format!("知识文档不存在: {document_id}")))?;
        if document.sensitivity == "restricted" {
            return Err(AppError::NotFound("受限知识文档不可读取".to_string()));
        }
        let processing = db.get_knowledge_document_processing_summary(&document)?;
        let versions = if processing.content_available {
            KnowledgePolicyService::authorize_content_output(&document)?;
            db.list_knowledge_document_versions(document_id)?
        } else {
            Vec::new()
        };
        Ok(KnowledgeDocumentDetail {
            document,
            versions,
            processing,
        })
    }

    /// 图片预览通过受控资产副本返回，既不把 app data 路径交给 WebView，也不允许把任意
    /// 文档或过大的原始文件作为 IPC 载荷读取。受限、已删除或非图片文档均不能使用该入口。
    pub(crate) fn get_document_image_preview(
        db: &Database,
        app_data_dir: &Path,
        document_id: i64,
    ) -> Result<KnowledgeDocumentImagePreview, AppError> {
        KnowledgeRolloutService::require(db, "catalog")?;
        validate_positive_id(document_id, "知识文档 ID")?;
        let document = db
            .get_knowledge_document_by_id(document_id)?
            .ok_or_else(|| AppError::NotFound(format!("知识文档不存在: {document_id}")))?;
        KnowledgePolicyService::authorize_content_output(&document)?;
        let asset = db
            .get_knowledge_document_current_image_asset(document_id)?
            .ok_or_else(|| AppError::NotFound("当前文档没有可预览的图片资产".to_string()))?;
        if asset.mime_type.eq_ignore_ascii_case("image/svg+xml") {
            return Err(AppError::InvalidInput(
                "SVG 原始内容不能直接预览，请转换为 PNG 或 JPEG 后上传".to_string(),
            ));
        }
        if asset.size_bytes > MAX_IMAGE_PREVIEW_BYTES {
            return Err(AppError::InvalidInput(format!(
                "图片超过 {}MB，暂不生成预览；可保留其标题和元数据搜索",
                MAX_IMAGE_PREVIEW_BYTES / 1024 / 1024
            )));
        }
        let bytes = read_verified_preview_asset(app_data_dir, &asset)?;
        let metadata = inspect_image_metadata(&asset.mime_type, &bytes);
        let width = metadata.width.ok_or_else(|| {
            AppError::InvalidInput("无法安全识别图片尺寸，暂不生成预览".to_string())
        })?;
        let height = metadata.height.ok_or_else(|| {
            AppError::InvalidInput("无法安全识别图片尺寸，暂不生成预览".to_string())
        })?;
        validate_image_pixel_limits(width, height)?;
        Ok(KnowledgeDocumentImagePreview {
            document_id,
            mime_type: asset.mime_type.clone(),
            size_bytes: asset.size_bytes,
            width: Some(width),
            height: Some(height),
            data_url: format!(
                "data:{};base64,{}",
                asset.mime_type,
                general_purpose::STANDARD.encode(bytes)
            ),
        })
    }

    pub(crate) fn list_document_versions(
        db: &Database,
        document_id: i64,
    ) -> Result<Vec<KnowledgeDocumentVersion>, AppError> {
        KnowledgeRolloutService::require(db, "catalog")?;
        validate_positive_id(document_id, "知识文档 ID")?;
        let document = db
            .get_knowledge_document_by_id(document_id)?
            .ok_or_else(|| AppError::NotFound(format!("知识文档不存在: {document_id}")))?;
        KnowledgePolicyService::authorize_content_output(&document)?;
        db.list_knowledge_document_versions(document_id)
    }

    /// 比较始终先校验同一逻辑文档及内容输出权限。Office 文件使用已保存的规范化文本
    /// 产生可读行差异，同时单独展示原始资产与解析器签名变化，避免“正文相同”掩盖
    /// 文件或解析结果已经变更的事实。
    pub(crate) fn compare_document_versions(
        db: &Database,
        input: CompareKnowledgeDocumentVersionsInput,
    ) -> Result<KnowledgeDocumentComparison, AppError> {
        KnowledgeRolloutService::require(db, "catalog")?;
        validate_positive_id(input.from_version_id, "起始文档版本 ID")?;
        validate_positive_id(input.to_version_id, "目标文档版本 ID")?;
        let from_version = db
            .get_knowledge_document_version_by_id(input.from_version_id)?
            .ok_or_else(|| {
                AppError::NotFound(format!("知识文档版本不存在: {}", input.from_version_id))
            })?;
        let to_version = db
            .get_knowledge_document_version_by_id(input.to_version_id)?
            .ok_or_else(|| {
                AppError::NotFound(format!("知识文档版本不存在: {}", input.to_version_id))
            })?;
        if from_version.document_id != to_version.document_id {
            return Err(AppError::InvalidInput(
                "仅允许比较同一逻辑文档的版本".to_string(),
            ));
        }
        let document = db
            .get_knowledge_document_by_id(from_version.document_id)?
            .ok_or_else(|| {
                AppError::NotFound(format!("知识文档不存在: {}", from_version.document_id))
            })?;
        KnowledgePolicyService::authorize_content_output(&document)?;

        let from_parse_artifacts =
            db.list_knowledge_document_comparison_artifacts(from_version.id)?;
        let to_parse_artifacts = db.list_knowledge_document_comparison_artifacts(to_version.id)?;
        let from_asset_hashes = comparison_asset_hashes(&from_parse_artifacts);
        let to_asset_hashes = comparison_asset_hashes(&to_parse_artifacts);
        let from_parsers = comparison_parser_signatures(&from_parse_artifacts);
        let to_parsers = comparison_parser_signatures(&to_parse_artifacts);
        let content_changed = from_version.content_hash != to_version.content_hash;
        let asset_changed = from_asset_hashes != to_asset_hashes;
        let parser_changed = from_parsers != to_parsers;

        let from_lines = from_version.content.lines().collect::<Vec<_>>();
        let to_lines = to_version.content.lines().collect::<Vec<_>>();
        let common_prefix = from_lines
            .iter()
            .zip(&to_lines)
            .take_while(|(left, right)| left == right)
            .count();
        let max_suffix = from_lines
            .len()
            .saturating_sub(common_prefix)
            .min(to_lines.len().saturating_sub(common_prefix));
        let common_suffix = (0..max_suffix)
            .take_while(|offset| {
                from_lines[from_lines.len() - 1 - offset] == to_lines[to_lines.len() - 1 - offset]
            })
            .count();
        let removed_end = from_lines.len().saturating_sub(common_suffix);
        let added_end = to_lines.len().saturating_sub(common_suffix);
        let removed_lines = from_lines[common_prefix..removed_end]
            .iter()
            .map(|line| (*line).to_string())
            .collect::<Vec<_>>();
        let added_lines = to_lines[common_prefix..added_end]
            .iter()
            .map(|line| (*line).to_string())
            .collect::<Vec<_>>();
        Ok(KnowledgeDocumentComparison {
            from_version,
            to_version,
            content_changed,
            asset_changed,
            parser_changed,
            unchanged: !content_changed && !asset_changed && !parser_changed,
            common_prefix_lines: i64::try_from(common_prefix).unwrap_or(i64::MAX),
            common_suffix_lines: i64::try_from(common_suffix).unwrap_or(i64::MAX),
            removed_lines,
            added_lines,
            from_asset_hashes,
            to_asset_hashes,
            from_parse_artifacts,
            to_parse_artifacts,
        })
    }

    /// 删除确认页只呈现受管索引和历史引用的计数，永久删除始终不通过此路径开放。
    pub(crate) fn preview_deletion(
        db: &Database,
        document_id: i64,
    ) -> Result<KnowledgeDocumentDeletionImpactPreview, AppError> {
        KnowledgeRolloutService::require(db, "catalog")?;
        validate_positive_id(document_id, "知识文档 ID")?;
        let document = db
            .get_knowledge_document_by_id(document_id)?
            .ok_or_else(|| AppError::NotFound(format!("知识文档不存在: {document_id}")))?;
        if document.sensitivity == "restricted" {
            return Err(AppError::NotFound("受限知识文档不可读取".to_string()));
        }
        db.preview_knowledge_document_deletion(document_id)
    }

    /// 普通删除仅隐藏逻辑文档并移除其派生全文索引；版本、向量、资产和关系保留，
    /// 以便恢复与历史引用保持可追溯。
    pub(crate) fn soft_delete(
        db: &Database,
        document_id: i64,
    ) -> Result<KnowledgeDocumentDeletionImpactPreview, AppError> {
        let preview = Self::preview_deletion(db, document_id)?;
        db.soft_delete_knowledge_document(document_id)?;
        audit_knowledge(
            db,
            "knowledge_document_soft_delete",
            "L1",
            "成功",
            "软删除知识文档并移除全文索引",
            serde_json::json!({
                "documentId": document_id,
                "versionCount": preview.version_count,
                "chunkCount": preview.chunk_count,
                "vectorCount": preview.vector_count,
                "relationCount": preview.relation_count,
                "assetCount": preview.asset_count,
                "ftsEntryCount": preview.fts_entry_count,
                "permanentDeletionEnabled": false,
            }),
        );
        Ok(preview)
    }

    /// 恢复前再次按可见性规则校验，再幂等回建全部有效版本的全文索引。
    pub(crate) fn restore(
        db: &Database,
        document_id: i64,
    ) -> Result<RestoreKnowledgeDocumentResult, AppError> {
        KnowledgeRolloutService::require(db, "catalog")?;
        validate_positive_id(document_id, "知识文档 ID")?;
        let document = db
            .get_knowledge_document_including_deleted_by_id(document_id)?
            .ok_or_else(|| AppError::NotFound(format!("知识文档不存在: {document_id}")))?;
        if document.sensitivity == "restricted" {
            return Err(AppError::NotFound("受限知识文档不可读取".to_string()));
        }
        let restored = db.restore_knowledge_document(document_id)?;
        audit_knowledge(
            db,
            "knowledge_document_restore",
            "L1",
            "成功",
            "恢复知识文档并重建全文索引",
            serde_json::json!({
                "documentId": document_id,
                "rebuiltFtsEntries": restored.rebuilt_fts_entries,
            }),
        );
        Ok(restored)
    }

    /// 将文件选择结果转为一次性句柄。后续提交只能消费该句柄，避免上传接口接收任意路径。
    pub(crate) fn prepare_upload_file(
        registry: &KnowledgeUploadGrantRegistry,
        selected_path: &str,
    ) -> Result<PreparedKnowledgeUploadFile, AppError> {
        registry.grant(Path::new(selected_path))
    }

    pub(crate) fn prepare_upload_directory(
        registry: &KnowledgeUploadGrantRegistry,
        selected_path: &str,
    ) -> Result<PreparedKnowledgeUploadDirectory, AppError> {
        registry.grant_directory(Path::new(selected_path))
    }

    /// 消费一次性句柄、复核原文件、复制内容寻址资产并创建可取消的导入任务。
    pub(crate) fn create_upload_import(
        db: &Database,
        app_data_dir: &Path,
        registry: &KnowledgeUploadGrantRegistry,
        input: UploadKnowledgeAssetInput,
    ) -> Result<KnowledgeDocumentUploadResult, AppError> {
        KnowledgeRolloutService::require(db, "catalog")?;
        validate_positive_id(input.project_id, "项目 ID")?;
        if !db.knowledge_project_exists(input.project_id)? {
            return Err(AppError::NotFound(format!(
                "知识项目不存在: {}",
                input.project_id
            )));
        }
        let cross_version_scope = validate_document_version_scope(
            db,
            input.project_id,
            input.project_version_id,
            input.cross_version_scope.as_deref(),
        )?;
        let source_folder_name = normalize_upload_folder_name(input.source_folder_name.as_deref())?;
        let source_path = registry.consume(&input.file_handle)?;
        let display_name = input
            .display_name
            .as_deref()
            .map(normalize_file_name)
            .transpose()?
            .unwrap_or_else(|| {
                source_path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("未命名文件")
                    .to_string()
            });
        // 显示名称可由用户修正，因此消费句柄后重新以真实内容核验类型和容器结构。
        let validated_file = validate_upload_file(&source_path, &display_name)?;
        let ocr_provider_key = normalize_ocr_upload_options(
            validated_file.mime_type,
            input.allow_remote_ocr,
            input.ocr_provider_key.as_deref(),
        )?;
        let asset = Self::store_uploaded_asset(
            db,
            app_data_dir,
            &source_path,
            Some(&display_name),
            validated_file.mime_type,
        )?;
        let title = infer_upload_title(&display_name);
        let upload_key = new_upload_key();
        let upload = db.create_knowledge_document_upload(&CreateKnowledgeDocumentUpload {
            upload_key,
            project_id: input.project_id,
            release_id: input.project_version_id,
            cross_version_scope,
            asset_id: asset.id,
            asset_key: asset.asset_key,
            original_name: asset.normalized_name,
            source_folder_name,
            mime_type: asset.mime_type,
            document_type: validated_file.document_type.to_string(),
            title,
            allow_remote_ocr: input.allow_remote_ocr,
            ocr_provider_key,
        })?;
        audit_knowledge(
            db,
            "knowledge_document_upload_create",
            "L1",
            "成功",
            "创建知识文档导入任务",
            serde_json::json!({
                "projectId": input.project_id,
                "documentId": upload.document_id,
                "assetId": upload.asset_id,
                "importJobId": upload.import_job_id,
            }),
        );
        Ok(KnowledgeDocumentUploadResult {
            document_id: upload.document_id,
            asset_id: upload.asset_id,
            import_job_id: upload.import_job_id,
            import_job_key: upload.import_job_key,
            status: upload.status,
        })
    }

    /// 批量导入按文件隔离失败：任何单项错误都不影响已经成功创建的导入任务，也不会
    /// 泄漏本地绝对路径。每项使用独立的一次性句柄，因此重复提交只影响该文件。
    pub(crate) fn create_upload_import_batch(
        db: &Database,
        app_data_dir: &Path,
        registry: &KnowledgeUploadGrantRegistry,
        input: UploadKnowledgeAssetBatchInput,
    ) -> Result<KnowledgeDocumentUploadBatchResult, AppError> {
        if input.files.is_empty() || input.files.len() > MAX_UPLOAD_BATCH_FILES {
            return Err(AppError::InvalidInput(
                "请一次选择 1 至 50 个文件".to_string(),
            ));
        }
        // 先校验来源名称，再消费任何一次性句柄，避免非法元数据造成部分入队。
        let source_folder_name = normalize_upload_folder_name(input.source_folder_name.as_deref())?;
        let handles = input
            .files
            .iter()
            .map(|file| file.file_handle.clone())
            .collect::<Vec<_>>();
        if registry.total_size(&handles)? > MAX_UPLOAD_BATCH_SIZE_BYTES {
            return Err(AppError::InvalidInput(
                "一次上传总大小不能超过 100MB".to_string(),
            ));
        }
        let items = input
            .files
            .into_iter()
            .map(|file| {
                let display_name = file
                    .display_name
                    .clone()
                    .filter(|name| !name.trim().is_empty())
                    .unwrap_or_else(|| "已选文件".to_string());
                let result = Self::create_upload_import(
                    db,
                    app_data_dir,
                    registry,
                    UploadKnowledgeAssetInput {
                        project_id: input.project_id,
                        project_version_id: input.project_version_id,
                        cross_version_scope: input.cross_version_scope.clone(),
                        file_handle: file.file_handle,
                        display_name: file.display_name,
                        source_folder_name: source_folder_name.clone(),
                        allow_remote_ocr: file.allow_remote_ocr,
                        ocr_provider_key: file.ocr_provider_key,
                    },
                );
                match result {
                    Ok(result) => KnowledgeDocumentUploadBatchItemResult {
                        display_name,
                        result: Some(result),
                        error_message: None,
                    },
                    Err(error) => KnowledgeDocumentUploadBatchItemResult {
                        display_name,
                        result: None,
                        error_message: Some(safe_upload_error(&error)),
                    },
                }
            })
            .collect();
        Ok(KnowledgeDocumentUploadBatchResult { items })
    }

    /// 创建或保存人工草稿。草稿表从未被正式搜索、向量、图谱或问答查询引用，避免
    /// 未提交的内容被误作为项目事实返回。
    pub(crate) fn save_manual_draft(
        db: &Database,
        input: KnowledgeDocumentDraftInput,
    ) -> Result<KnowledgeDocumentDraftSaveResult, AppError> {
        KnowledgeRolloutService::require(db, "catalog")?;
        validate_positive_id(input.project_id, "项目 ID")?;
        if !db.knowledge_project_exists(input.project_id)? {
            return Err(AppError::NotFound(format!(
                "知识项目不存在: {}",
                input.project_id
            )));
        }
        let title = required_text(&input.title, "文档标题")?;
        if title.chars().count() > 200 {
            return Err(AppError::InvalidInput(
                "文档标题不能超过 200 个字符".to_string(),
            ));
        }
        if input.content.len() > 2 * 1024 * 1024 {
            return Err(AppError::InvalidInput("草稿正文不能超过 2MB".to_string()));
        }
        let doc_type = normalize_manual_document_type(&input.doc_type)?;
        let editor_label = input
            .editor_label
            .as_deref()
            .map(|value| required_text(value, "编辑者"))
            .transpose()?
            .unwrap_or_else(|| "本地用户".to_string());
        if editor_label.chars().count() > 80 {
            return Err(AppError::InvalidInput(
                "编辑者名称不能超过 80 个字符".to_string(),
            ));
        }
        validate_document_belongs_to_project(db, input.document_id, input.project_id)?;

        let result = match input.revision {
            None => {
                if input.draft_id.is_some() {
                    return Err(AppError::InvalidInput(
                        "新建草稿时不能提供草稿标识".to_string(),
                    ));
                }
                let draft = db.create_knowledge_document_draft(
                    &crate::database::knowledge_domain::documents::NewKnowledgeDocumentDraft {
                        document_id: input.document_id,
                        project_id: input.project_id,
                        title,
                        content: input.content,
                        doc_type,
                        base_version_id: input.base_version_id,
                        editor_label,
                    },
                )?;
                KnowledgeDocumentDraftSaveResult {
                    draft: draft_into_model(draft),
                    conflict: false,
                }
            }
            Some(revision) => {
                validate_positive_id(revision, "草稿修订号")?;
                let draft_id = input.draft_id.ok_or_else(|| {
                    AppError::InvalidInput("更新草稿时必须提供草稿标识".to_string())
                })?;
                let current = db
                    .get_knowledge_document_draft(draft_id)?
                    .ok_or_else(|| AppError::NotFound(format!("草稿不存在: {draft_id}")))?;
                if current.project_id != input.project_id {
                    return Err(AppError::InvalidInput("草稿不属于当前项目".to_string()));
                }
                if current.document_id != input.document_id {
                    return Err(AppError::InvalidInput(
                        "草稿不属于当前文档，不能覆盖其内容".to_string(),
                    ));
                }
                let saved = db.update_knowledge_document_draft(
                    draft_id,
                    revision,
                    &title,
                    &input.content,
                    &editor_label,
                )?;
                let conflict = saved.is_none();
                let draft = saved.unwrap_or(current);
                KnowledgeDocumentDraftSaveResult {
                    draft: draft_into_model(draft),
                    conflict,
                }
            }
        };
        audit_knowledge(
            db,
            "knowledge_document_draft_save",
            "L1",
            if result.conflict { "冲突" } else { "成功" },
            if result.conflict {
                "草稿保存发生修订冲突"
            } else {
                "保存知识文档草稿"
            },
            serde_json::json!({
                "projectId": result.draft.project_id,
                "draftId": result.draft.id,
                "documentId": result.draft.document_id,
                "revision": result.draft.revision,
                "conflict": result.conflict,
            }),
        );
        Ok(result)
    }

    /// 恢复不改写来源版本或后续版本：历史正文先进入带父版本标识的新草稿，用户确认后
    /// 再复用已有提交路径创建新的不可变版本。若目标草稿已被更新，返回当前草稿正文供
    /// 调用方比较，而不是采用“最后写入覆盖”。
    pub(crate) fn restore_version_to_draft(
        db: &Database,
        input: RestoreKnowledgeDocumentVersionToDraftInput,
    ) -> Result<RestoreKnowledgeDocumentVersionToDraftResult, AppError> {
        KnowledgeRolloutService::require(db, "catalog")?;
        validate_positive_id(input.source_version_id, "历史文档版本 ID")?;
        if input.draft_id.is_some() != input.revision.is_some() {
            return Err(AppError::InvalidInput(
                "恢复到已有草稿时必须同时提供草稿 ID 和修订号".to_string(),
            ));
        }
        let source_version = db
            .get_knowledge_document_version_by_id(input.source_version_id)?
            .ok_or_else(|| {
                AppError::NotFound(format!("知识文档版本不存在: {}", input.source_version_id))
            })?;
        let document = db
            .get_knowledge_document_by_id(source_version.document_id)?
            .ok_or_else(|| {
                AppError::NotFound(format!("知识文档不存在: {}", source_version.document_id))
            })?;
        KnowledgePolicyService::authorize_content_output(&document)?;
        let project_id = document.project_id.ok_or_else(|| {
            AppError::InvalidInput("历史文档未关联项目，暂不能创建恢复草稿".to_string())
        })?;
        let saved = Self::save_manual_draft(
            db,
            KnowledgeDocumentDraftInput {
                draft_id: input.draft_id,
                document_id: Some(document.id),
                project_id,
                title: document.title,
                content: source_version.content.clone(),
                doc_type: "markdown".to_string(),
                base_version_id: Some(source_version.id),
                revision: input.revision,
                editor_label: input.editor_label,
            },
        )?;
        audit_knowledge(
            db,
            "knowledge_document_version_restore_to_draft",
            "L1",
            if saved.conflict { "冲突" } else { "成功" },
            if saved.conflict {
                "恢复历史文档版本时发生草稿修订冲突"
            } else {
                "从历史文档版本创建恢复草稿"
            },
            serde_json::json!({
                "documentId": document.id,
                "sourceVersionId": source_version.id,
                "draftId": saved.draft.id,
                "conflict": saved.conflict,
            }),
        );
        Ok(RestoreKnowledgeDocumentVersionToDraftResult {
            source_version,
            draft: saved.draft,
            conflict: saved.conflict,
        })
    }

    /// 提交草稿会冻结正文、作者、说明、父版本和内容哈希，同时留下可恢复的索引任务。
    /// 每次提交必须明确关联一个项目版本，或明确声明适用于项目全部版本。
    pub(crate) fn commit_manual_draft(
        db: &Database,
        input: CommitKnowledgeDocumentDraftInput,
    ) -> Result<KnowledgeDocumentCommitResult, AppError> {
        Self::commit_manual_draft_with_analysis_draft_id(db, input, None)
    }

    /// AI 分析确认是唯一能写入分析草稿关联的路径。该关联用于进程中断恢复，保持在
    /// 服务层内部，避免 IPC 调用方伪造或覆盖分析草稿与正式版本的审计关系。
    pub(crate) fn commit_analysis_draft(
        db: &Database,
        input: CommitKnowledgeDocumentDraftInput,
        analysis_draft_id: i64,
    ) -> Result<KnowledgeDocumentCommitResult, AppError> {
        validate_positive_id(analysis_draft_id, "分析草稿 ID")?;
        Self::commit_manual_draft_with_analysis_draft_id(db, input, Some(analysis_draft_id))
    }

    fn commit_manual_draft_with_analysis_draft_id(
        db: &Database,
        input: CommitKnowledgeDocumentDraftInput,
        analysis_draft_id: Option<i64>,
    ) -> Result<KnowledgeDocumentCommitResult, AppError> {
        KnowledgeRolloutService::require(db, "catalog")?;
        validate_positive_id(input.draft_id, "草稿 ID")?;
        validate_positive_id(input.revision, "草稿修订号")?;
        let draft = db
            .get_knowledge_document_draft(input.draft_id)?
            .ok_or_else(|| AppError::NotFound(format!("草稿不存在: {}", input.draft_id)))?;
        if draft.deleted_at.is_some() {
            return Err(AppError::InvalidInput(
                "草稿已经提交或删除，不能重复提交".to_string(),
            ));
        }
        let version_label = required_text(&input.version_label, "版本名称")?;
        if version_label.chars().count() > 80 {
            return Err(AppError::InvalidInput(
                "版本名称不能超过 80 个字符".to_string(),
            ));
        }
        let cross_version_scope = validate_document_version_scope(
            db,
            draft.project_id,
            input.project_version_id,
            input.cross_version_scope.as_deref(),
        )?;
        let author_label = input
            .author_label
            .as_deref()
            .map(|value| required_text(value, "提交人"))
            .transpose()?
            .unwrap_or(draft.editor_label.clone());
        if author_label.chars().count() > 80 {
            return Err(AppError::InvalidInput(
                "提交人不能超过 80 个字符".to_string(),
            ));
        }
        let commit_message = input.commit_message.unwrap_or_default().trim().to_string();
        if commit_message.chars().count() > 500 {
            return Err(AppError::InvalidInput(
                "提交说明不能超过 500 个字符".to_string(),
            ));
        }
        let content_hash = format!("{:x}", Sha256::digest(draft.content.as_bytes()));
        let token_estimate = i64::try_from(draft.content.chars().count().div_ceil(4))
            .map_err(|_| AppError::InvalidInput("草稿正文长度超出支持范围".to_string()))?;
        let committed = db.commit_knowledge_document_draft(&CommitKnowledgeDocumentDraft {
            draft_id: draft.id,
            expected_revision: input.revision,
            version_label,
            release_id: input.project_version_id,
            cross_version_scope,
            commit_message,
            author_label,
            analysis_draft_id,
            content_hash,
            token_estimate,
        })?;
        audit_knowledge(
            db,
            "knowledge_document_commit",
            "L1",
            "成功",
            "提交知识文档版本",
            serde_json::json!({
                "projectId": draft.project_id,
                "draftId": draft.id,
                "documentId": committed.document_id,
                "documentVersionId": committed.document_version_id,
                "parentVersionId": committed.parent_version_id,
                "projectVersionId": input.project_version_id,
                "crossVersionScope": input.cross_version_scope,
                "indexJobId": committed.index_job_id,
            }),
        );
        Ok(KnowledgeDocumentCommitResult {
            document_id: committed.document_id,
            document_version_id: committed.document_version_id,
            parent_version_id: committed.parent_version_id,
            content_hash: committed.content_hash,
            index_job_id: committed.index_job_id,
            index_job_status: committed.index_job_status,
        })
    }

    /// 将已经由本地选择器授予的普通文件复制到应用受控目录，并登记内容寻址元数据。
    ///
    /// 复制完成前不创建数据库记录；数据库登记失败时只会清理本次创建、且哈希仍相符的
    /// 精确目标文件。重复内容直接复用同一个存储键，引用计数由后续文档版本关联事务维护。
    pub(crate) fn store_uploaded_asset(
        db: &Database,
        app_data_dir: &Path,
        source_path: &Path,
        display_name: Option<&str>,
        mime_type: &str,
    ) -> Result<StoredKnowledgeAsset, AppError> {
        let source = validate_source_file(source_path)?;
        let original_name = display_name
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(ToOwned::to_owned)
            .or_else(|| {
                source
                    .file_name()
                    .and_then(|name| name.to_str())
                    .map(ToOwned::to_owned)
            })
            .ok_or_else(|| AppError::InvalidInput("上传文件缺少可用名称".to_string()))?;
        let normalized_name = normalize_file_name(&original_name)?;
        let mime_type = normalize_mime_type(mime_type)?;
        let validated_file = validate_upload_file(&source, &normalized_name)?;
        if mime_type != validated_file.mime_type {
            return Err(AppError::InvalidInput(
                "文件 MIME 类型与扩展名或内容校验结果不一致".to_string(),
            ));
        }
        let storage_root = ensure_storage_root(app_data_dir)?;

        let staged = copy_source_to_staging(&source, &storage_root)?;
        let storage_key = storage_key_for(&staged.content_hash);
        let target = storage_root.join(&storage_key);
        let created_target = publish_staged_asset(&staged.path, &target, &staged.content_hash)?;
        let size_bytes = i64::try_from(staged.size_bytes)
            .map_err(|_| AppError::InvalidInput("文件大小超出支持范围".to_string()))?;

        let saved = db.upsert_knowledge_asset(&NewKnowledgeAsset {
            asset_key: format!("sha256:{}", staged.content_hash),
            content_hash: staged.content_hash.clone(),
            storage_key: storage_key.clone(),
            original_name: original_name.clone(),
            normalized_name: normalized_name.clone(),
            mime_type: mime_type.clone(),
            size_bytes,
        });
        let asset = match saved {
            Ok(asset) => asset,
            Err(error) => {
                cleanup_new_target_if_matching(
                    &storage_root,
                    &target,
                    &staged.content_hash,
                    created_target,
                );
                return Err(error);
            }
        };
        if asset.content_hash != staged.content_hash
            || asset.storage_key != storage_key
            || asset.size_bytes != size_bytes
        {
            cleanup_new_target_if_matching(
                &storage_root,
                &target,
                &staged.content_hash,
                created_target,
            );
            return Err(AppError::Custom(
                "资产元数据与受控文件不一致，已拒绝本次上传".to_string(),
            ));
        }

        Ok(StoredKnowledgeAsset {
            id: asset.id,
            asset_key: asset.asset_key,
            content_hash: asset.content_hash,
            storage_key: asset.storage_key,
            original_name: asset.original_name,
            normalized_name: asset.normalized_name,
            mime_type: asset.mime_type,
            size_bytes: asset.size_bytes,
            reused_existing_content: !created_target,
        })
    }
}

fn empty_list_input() -> KnowledgeListInput {
    KnowledgeListInput {
        project_id: None,
        release_id: None,
        source_id: None,
        keyword: None,
        status: None,
        offset: None,
        limit: None,
    }
}

/// OCR 是图片上传的显式外发授权。非图片携带这些字段通常说明前端状态错配，必须在
/// 信任边界拒绝，而不是让后台任务在未知语义下忽略。
fn normalize_ocr_upload_options(
    mime_type: &str,
    allow_remote_ocr: bool,
    ocr_provider_key: Option<&str>,
) -> Result<String, AppError> {
    let provider_key = ocr_provider_key.unwrap_or_default().trim();
    if !mime_type.starts_with("image/") {
        if allow_remote_ocr || !provider_key.is_empty() {
            return Err(AppError::InvalidInput(
                "远程 OCR 仅支持图片文件".to_string(),
            ));
        }
        return Ok(String::new());
    }
    if mime_type.eq_ignore_ascii_case("image/svg+xml") && allow_remote_ocr {
        return Err(AppError::InvalidInput(
            "SVG 不会发送至远程文字识别服务，请先转换为 PNG 或 JPEG".to_string(),
        ));
    }
    if allow_remote_ocr && provider_key.is_empty() {
        return Err(AppError::InvalidInput(
            "请选择已配置的视觉识别服务后再上传图片".to_string(),
        ));
    }
    if !allow_remote_ocr && !provider_key.is_empty() {
        return Err(AppError::InvalidInput(
            "请先确认允许将图片发送至所选识别服务".to_string(),
        ));
    }
    Ok(provider_key.to_string())
}

fn comparison_asset_hashes(
    artifacts: &[crate::models::KnowledgeDocumentComparisonArtifact],
) -> Vec<String> {
    artifacts
        .iter()
        .filter_map(|artifact| artifact.asset_hash.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn comparison_parser_signatures(
    artifacts: &[crate::models::KnowledgeDocumentComparisonArtifact],
) -> BTreeSet<(String, String)> {
    artifacts
        .iter()
        .map(|artifact| (artifact.parser_id.clone(), artifact.parser_version.clone()))
        .collect()
}

fn validate_document_belongs_to_project(
    db: &Database,
    document_id: Option<i64>,
    project_id: i64,
) -> Result<(), AppError> {
    let Some(document_id) = document_id else {
        return Ok(());
    };
    validate_positive_id(document_id, "文档 ID")?;
    let document = db
        .get_knowledge_document_by_id(document_id)?
        .ok_or_else(|| AppError::NotFound(format!("知识文档不存在: {document_id}")))?;
    if document.project_id != Some(project_id) {
        return Err(AppError::InvalidInput("文档不属于当前项目".to_string()));
    }
    Ok(())
}

/// 仅支持当前产品已经可解释的项目级跨版本范围。范围必须由用户明确选择，不能因为
/// 没有选版本而自动落到“最新版本”或未绑定状态。
fn validate_document_version_scope(
    db: &Database,
    project_id: i64,
    project_version_id: Option<i64>,
    requested_cross_version_scope: Option<&str>,
) -> Result<String, AppError> {
    let cross_version_scope = requested_cross_version_scope
        .unwrap_or_default()
        .trim()
        .to_string();
    if project_version_id.is_some() && !cross_version_scope.is_empty() {
        return Err(AppError::InvalidInput(
            "请选择一个项目版本，或选择跨版本范围，不能同时使用两者".to_string(),
        ));
    }
    if let Some(release_id) = project_version_id {
        validate_positive_id(release_id, "项目版本 ID")?;
        let belongs_to_project = db
            .list_knowledge_releases(project_id)?
            .iter()
            .any(|release| release.id == release_id);
        if !belongs_to_project {
            return Err(AppError::InvalidInput("项目版本不属于当前项目".to_string()));
        }
        return Ok(String::new());
    }
    if cross_version_scope == "project_all_versions" {
        return Ok(cross_version_scope);
    }
    Err(AppError::InvalidInput(
        "请选择关联版本，或明确选择“适用于全部版本”".to_string(),
    ))
}

fn normalize_manual_document_type(value: &str) -> Result<String, AppError> {
    let value = value.trim().to_ascii_lowercase();
    if ["markdown", "rich_text"].contains(&value.as_str()) {
        Ok(value)
    } else {
        Err(AppError::InvalidInput(
            "人工文档类型仅支持 Markdown 或富文本".to_string(),
        ))
    }
}

fn infer_upload_title(file_name: &str) -> String {
    Path::new(file_name)
        .file_stem()
        .and_then(|value| value.to_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("未命名文档")
        .chars()
        .take(200)
        .collect()
}

fn normalize_upload_folder_name(folder_name: Option<&str>) -> Result<Option<String>, AppError> {
    folder_name.map(normalize_file_name).transpose()
}

fn new_upload_key() -> String {
    let mut random = [0_u8; 24];
    rand::rngs::OsRng.fill_bytes(&mut random);
    format!("upload-{:x}", Sha256::digest(random))
}

fn safe_upload_error(error: &AppError) -> String {
    match error {
        AppError::Io(_) => "无法读取所选文件，请确认文件仍存在且可访问".to_string(),
        _ => error.to_string(),
    }
}

fn draft_into_model(
    draft: crate::database::knowledge_domain::documents::KnowledgeDocumentDraftRecord,
) -> KnowledgeDocumentDraft {
    KnowledgeDocumentDraft {
        id: draft.id,
        document_id: draft.document_id,
        project_id: draft.project_id,
        title: draft.title,
        content: draft.content,
        doc_type: draft.doc_type,
        base_version_id: draft.base_version_id,
        revision: draft.revision,
        editor_label: draft.editor_label,
    }
}

struct StagedAsset {
    path: PathBuf,
    content_hash: String,
    size_bytes: u64,
}

fn validate_source_file(source_path: &Path) -> Result<PathBuf, AppError> {
    if !source_path.is_absolute() {
        return Err(AppError::InvalidInput(
            "上传文件路径必须是绝对路径".to_string(),
        ));
    }
    let metadata = fs::symlink_metadata(source_path)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(AppError::InvalidInput(
            "上传内容必须是非符号链接的普通文件".to_string(),
        ));
    }
    if metadata.len() > MAX_ASSET_SIZE_BYTES {
        return Err(AppError::InvalidInput(format!(
            "单个文件不能超过 {}MB",
            MAX_ASSET_SIZE_BYTES / 1024 / 1024
        )));
    }
    fs::canonicalize(source_path).map_err(Into::into)
}

fn validate_source_directory(source_path: &Path) -> Result<PathBuf, AppError> {
    if !source_path.is_absolute() {
        return Err(AppError::InvalidInput(
            "上传文件夹路径必须是绝对路径".to_string(),
        ));
    }
    let metadata = fs::symlink_metadata(source_path)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(AppError::InvalidInput(
            "上传内容必须是非符号链接的文件夹".to_string(),
        ));
    }
    let canonical = fs::canonicalize(source_path)?;
    let canonical_metadata = fs::symlink_metadata(&canonical)?;
    if canonical_metadata.file_type().is_symlink() || !canonical_metadata.is_dir() {
        return Err(AppError::InvalidInput("上传文件夹路径不合法".to_string()));
    }
    Ok(canonical)
}

fn collect_directory_upload_files(
    root: &Path,
    current: &Path,
    depth: usize,
    candidates: &mut Vec<PathBuf>,
    skipped_count: &mut usize,
) -> Result<(), AppError> {
    if depth > MAX_UPLOAD_DIRECTORY_DEPTH {
        return Err(AppError::InvalidInput(format!(
            "文件夹层级不能超过 {MAX_UPLOAD_DIRECTORY_DEPTH} 层"
        )));
    }
    let mut entries = fs::read_dir(current)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() {
            *skipped_count = skipped_count.saturating_add(1);
            continue;
        }
        if metadata.is_dir() {
            collect_directory_upload_files(
                root,
                &path,
                depth.saturating_add(1),
                candidates,
                skipped_count,
            )?;
            continue;
        }
        if !metadata.is_file() {
            *skipped_count = skipped_count.saturating_add(1);
            continue;
        }
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default();
        if !is_supported_directory_upload_extension(extension) {
            *skipped_count = skipped_count.saturating_add(1);
            continue;
        }
        candidates.push(path);
        if candidates.len() > MAX_UPLOAD_BATCH_FILES {
            return Err(AppError::InvalidInput(
                "文件夹最多支持 50 个可上传文件".to_string(),
            ));
        }
    }
    if !current.starts_with(root) {
        return Err(AppError::InvalidInput(
            "文件夹内容超出已选择的目录范围".to_string(),
        ));
    }
    Ok(())
}

fn directory_relative_display_name(root: &Path, path: &Path) -> Result<String, AppError> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| AppError::InvalidInput("文件夹内容超出已选择的目录范围".to_string()))?;
    let mut components = Vec::new();
    for component in relative.components() {
        let Component::Normal(value) = component else {
            return Err(AppError::InvalidInput("文件夹内存在不安全路径".to_string()));
        };
        components.push(normalize_file_name(value.to_str().ok_or_else(|| {
            AppError::InvalidInput("文件夹内存在无法识别的文件名".to_string())
        })?)?);
    }
    if components.is_empty() {
        return Err(AppError::InvalidInput(
            "文件夹内文件缺少可用名称".to_string(),
        ));
    }
    Ok(components.join("/"))
}

fn ensure_storage_root(app_data_dir: &Path) -> Result<PathBuf, AppError> {
    if app_data_dir.as_os_str().is_empty() || !app_data_dir.is_absolute() {
        return Err(AppError::InvalidInput(
            "应用数据目录必须是绝对路径".to_string(),
        ));
    }
    fs::create_dir_all(app_data_dir)?;
    let app_data_metadata = fs::symlink_metadata(app_data_dir)?;
    if app_data_metadata.file_type().is_symlink() || !app_data_metadata.is_dir() {
        return Err(AppError::InvalidInput(
            "应用数据目录必须是非符号链接目录".to_string(),
        ));
    }
    let storage_root = app_data_dir.join(ASSET_STORAGE_DIRECTORY);
    fs::create_dir_all(&storage_root)?;
    let root_metadata = fs::symlink_metadata(&storage_root)?;
    if root_metadata.file_type().is_symlink() || !root_metadata.is_dir() {
        return Err(AppError::InvalidInput(
            "知识资产存储根路径不合法".to_string(),
        ));
    }
    let canonical_app_data_dir = fs::canonicalize(app_data_dir)?;
    let canonical_storage_root = fs::canonicalize(storage_root)?;
    if !canonical_storage_root.starts_with(&canonical_app_data_dir) {
        return Err(AppError::InvalidInput(
            "知识资产存储路径越出应用数据目录".to_string(),
        ));
    }
    Ok(canonical_storage_root)
}

fn copy_source_to_staging(source: &Path, storage_root: &Path) -> Result<StagedAsset, AppError> {
    let staging_path = create_staging_file(storage_root)?;
    let copied = (|| -> Result<StagedAsset, AppError> {
        let mut reader = File::open(source)?;
        let mut writer = OpenOptions::new().write(true).open(&staging_path)?;
        let mut hasher = Sha256::new();
        let mut total = 0_u64;
        let mut buffer = [0_u8; COPY_BUFFER_SIZE];
        loop {
            let count = reader.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            total = total.saturating_add(count as u64);
            if total > MAX_ASSET_SIZE_BYTES {
                return Err(AppError::InvalidInput(format!(
                    "单个文件不能超过 {}MB",
                    MAX_ASSET_SIZE_BYTES / 1024 / 1024
                )));
            }
            writer.write_all(&buffer[..count])?;
            hasher.update(&buffer[..count]);
        }
        writer.sync_all()?;
        set_owner_only_permissions(&staging_path)?;
        Ok(StagedAsset {
            path: staging_path.clone(),
            content_hash: format!("{:x}", hasher.finalize()),
            size_bytes: total,
        })
    })();
    if copied.is_err() {
        let _ = fs::remove_file(&staging_path);
    }
    copied
}

/// 通过同一文件系统的硬链接以“仅在目标不存在时创建”的方式发布，避免 `rename` 覆盖
/// 已有内容寻址文件；链接成功后再删除临时名称，文件内容从未出现半写状态。
fn publish_staged_asset(
    staging_path: &Path,
    target: &Path,
    expected_hash: &str,
) -> Result<bool, AppError> {
    let parent = target
        .parent()
        .ok_or_else(|| AppError::InvalidInput("知识资产目标路径缺少受控父目录".to_string()))?;
    fs::create_dir_all(parent)?;
    validate_asset_parent(parent)?;
    match fs::hard_link(staging_path, target) {
        Ok(()) => {
            fs::remove_file(staging_path)?;
            Ok(true)
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            let verified = verify_asset_file(target, expected_hash)?;
            if !verified {
                let _ = fs::remove_file(staging_path);
                return Err(AppError::InvalidInput(
                    "同一内容哈希对应的已存文件校验失败".to_string(),
                ));
            }
            fs::remove_file(staging_path)?;
            Ok(false)
        }
        Err(error) => {
            let _ = fs::remove_file(staging_path);
            Err(error.into())
        }
    }
}

fn create_staging_file(storage_root: &Path) -> Result<PathBuf, AppError> {
    for _ in 0..8 {
        let sequence = STAGING_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = storage_root.join(format!(
            ".asset-upload-{}-{}-{sequence}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(_) => return Ok(path),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        }
    }
    Err(AppError::Custom(
        "无法创建受控上传临时文件，请稍后重试".to_string(),
    ))
}

fn storage_key_for(content_hash: &str) -> String {
    format!(
        "{}/{}/{}",
        SHA256_DIRECTORY,
        &content_hash[..2],
        content_hash
    )
}

fn validate_asset_parent(parent: &Path) -> Result<(), AppError> {
    let metadata = fs::symlink_metadata(parent)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(AppError::InvalidInput(
            "知识资产目标父目录不合法".to_string(),
        ));
    }
    Ok(())
}

fn verify_asset_file(path: &Path, expected_hash: &str) -> Result<bool, AppError> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Ok(false);
    }
    if metadata.len() > MAX_ASSET_SIZE_BYTES {
        return Ok(false);
    }
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; COPY_BUFFER_SIZE];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hasher.finalize()) == expected_hash)
}

fn cleanup_new_target_if_matching(
    storage_root: &Path,
    target: &Path,
    expected_hash: &str,
    created_target: bool,
) {
    if !created_target || !target.starts_with(storage_root) {
        return;
    }
    if verify_asset_file(target, expected_hash).unwrap_or(false) {
        let _ = fs::remove_file(target);
    }
}

fn normalize_file_name(name: &str) -> Result<String, AppError> {
    let normalized = name
        .trim()
        .chars()
        .filter(|character| !character.is_control())
        .map(|character| match character {
            '/' | '\\' | ':' => '_',
            _ => character,
        })
        .collect::<String>();
    let normalized = normalized
        .trim_matches(|character| matches!(character, '.' | '_'))
        .trim()
        .to_string();
    if normalized.is_empty() {
        return Err(AppError::InvalidInput(
            "文件名称仅包含不支持的字符".to_string(),
        ));
    }
    if normalized.chars().count() > 180 {
        return Err(AppError::InvalidInput(
            "文件名称不能超过 180 个字符".to_string(),
        ));
    }
    Ok(normalized)
}

fn normalize_mime_type(mime_type: &str) -> Result<String, AppError> {
    let normalized = mime_type.trim().to_ascii_lowercase();
    if normalized.is_empty()
        || normalized.len() > 160
        || normalized.chars().any(char::is_control)
        || !normalized.contains('/')
    {
        return Err(AppError::InvalidInput(
            "文件类型不合法，请重新选择文件".to_string(),
        ));
    }
    Ok(normalized)
}

fn read_verified_preview_asset(
    app_data_dir: &Path,
    asset: &KnowledgeAssetRecord,
) -> Result<Vec<u8>, AppError> {
    if !asset.mime_type.starts_with("image/")
        || asset.size_bytes < 0
        || asset.size_bytes > MAX_IMAGE_PREVIEW_BYTES
        || asset.content_hash.len() != 64
        || !asset
            .content_hash
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
        || asset.storage_key.contains("..")
        || !asset.storage_key.starts_with("sha256/")
    {
        return Err(AppError::InvalidInput("图片预览资产元数据无效".to_string()));
    }
    let path = app_data_dir
        .join(ASSET_STORAGE_DIRECTORY)
        .join(&asset.storage_key);
    let metadata = fs::symlink_metadata(&path)?;
    if !metadata.file_type().is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() != asset.size_bytes as u64
    {
        return Err(AppError::InvalidInput(
            "图片预览资产已损坏或被替换".to_string(),
        ));
    }
    let bytes = fs::read(path)?;
    if format!("{:x}", Sha256::digest(&bytes)) != asset.content_hash {
        return Err(AppError::InvalidInput(
            "图片预览资产哈希校验失败".to_string(),
        ));
    }
    Ok(bytes)
}

#[cfg(unix)]
fn set_owner_only_permissions(path: &Path) -> Result<(), AppError> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).map_err(Into::into)
}

#[cfg(not(unix))]
fn set_owner_only_permissions(_path: &Path) -> Result<(), AppError> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use sha2::{Digest, Sha256};
    use std::fs;

    use super::{
        normalize_file_name, storage_key_for, KnowledgeDocumentService,
        KnowledgeUploadGrantRegistry,
    };
    use crate::database::knowledge_domain::documents::NewKnowledgeDocumentParseArtifact;
    use crate::database::Database;
    use crate::models::{
        CommitKnowledgeDocumentDraftInput, KnowledgeDocumentDraftInput, KnowledgeListInput,
        RestoreKnowledgeDocumentVersionToDraftInput, UploadKnowledgeAssetBatchInput,
        UploadKnowledgeAssetFileInput, UploadKnowledgeAssetInput, UpsertKnowledgeDocumentInput,
        UpsertKnowledgeProjectInput,
    };

    fn database(root: &std::path::Path) -> Result<Database, Box<dyn std::error::Error>> {
        Ok(Database::init(
            &root.join("knowledge-test.db").to_string_lossy(),
        )?)
    }

    fn test_root(label: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!(
            "tauri-ssh-knowledge-assets-{label}-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        fs::create_dir_all(&root).expect("测试临时目录应可创建");
        root
    }

    fn create_project(database: &Database) -> Result<i64, Box<dyn std::error::Error>> {
        Ok(database
            .upsert_knowledge_project(&UpsertKnowledgeProjectInput {
                id: None,
                project_key: "document-draft-test".to_string(),
                name: "文档草稿测试项目".to_string(),
                aliases: Vec::new(),
                description: String::new(),
                git_workspace_keys: Vec::new(),
                git_workspace_key: String::new(),
                default_branch: "main".to_string(),
                enabled: true,
            })?
            .id)
    }

    #[test]
    fn manual_draft_uses_optimistic_revision_and_never_creates_a_formal_document(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let root = test_root("draft");
        let database = database(&root)?;
        let project_id = create_project(&database)?;
        let created = KnowledgeDocumentService::save_manual_draft(
            &database,
            KnowledgeDocumentDraftInput {
                draft_id: None,
                document_id: None,
                project_id,
                title: "部署说明".to_string(),
                content: "第一版".to_string(),
                doc_type: "markdown".to_string(),
                base_version_id: None,
                revision: None,
                editor_label: None,
            },
        )?;
        assert!(!created.conflict);
        assert_eq!(created.draft.revision, 1);
        assert_eq!(
            database
                .list_knowledge_documents(&crate::models::KnowledgeListInput {
                    project_id: Some(project_id),
                    release_id: None,
                    source_id: None,
                    keyword: None,
                    status: None,
                    offset: None,
                    limit: None,
                })?
                .total,
            0,
            "草稿不得进入正式文档目录"
        );
        let updated = KnowledgeDocumentService::save_manual_draft(
            &database,
            KnowledgeDocumentDraftInput {
                draft_id: Some(created.draft.id),
                document_id: None,
                project_id,
                title: "部署说明".to_string(),
                content: "第二版".to_string(),
                doc_type: "markdown".to_string(),
                base_version_id: None,
                revision: Some(1),
                editor_label: Some("测试用户".to_string()),
            },
        )?;
        assert!(!updated.conflict);
        assert_eq!(updated.draft.revision, 2);
        let conflict = KnowledgeDocumentService::save_manual_draft(
            &database,
            KnowledgeDocumentDraftInput {
                draft_id: Some(created.draft.id),
                document_id: None,
                project_id,
                title: "部署说明".to_string(),
                content: "过期内容".to_string(),
                doc_type: "markdown".to_string(),
                base_version_id: None,
                revision: Some(1),
                editor_label: None,
            },
        )?;
        assert!(conflict.conflict);
        assert_eq!(conflict.draft.content, "第二版");
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn committing_a_draft_creates_an_immutable_version_parent_and_index_job(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let root = test_root("commit");
        let database = database(&root)?;
        let project_id = create_project(&database)?;
        let first_draft = KnowledgeDocumentService::save_manual_draft(
            &database,
            KnowledgeDocumentDraftInput {
                draft_id: None,
                document_id: None,
                project_id,
                title: "部署说明".to_string(),
                content: "第一版正文".to_string(),
                doc_type: "markdown".to_string(),
                base_version_id: None,
                revision: None,
                editor_label: Some("张三".to_string()),
            },
        )?;
        let first_commit = KnowledgeDocumentService::commit_manual_draft(
            &database,
            CommitKnowledgeDocumentDraftInput {
                draft_id: first_draft.draft.id,
                revision: first_draft.draft.revision,
                version_label: "初始版本".to_string(),
                project_version_id: None,
                cross_version_scope: Some("project_all_versions".to_string()),
                commit_message: Some("首次提交".to_string()),
                author_label: None,
            },
        )?;
        assert_eq!(first_commit.parent_version_id, None);
        let bindings =
            database.list_knowledge_document_version_bindings(first_commit.document_version_id)?;
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].release_id, None);
        assert_eq!(bindings[0].cross_version_scope, "project_all_versions");
        assert_eq!(
            database
                .get_knowledge_job_by_id(first_commit.index_job_id)?
                .expect("提交必须创建索引任务")
                .status,
            "queued"
        );
        assert!(database
            .get_knowledge_document_draft(first_draft.draft.id)?
            .expect("归档草稿仍应可追溯")
            .deleted_at
            .is_some());
        let first_version = database
            .get_knowledge_document_version_by_id(first_commit.document_version_id)?
            .expect("提交必须创建正式版本");
        assert_eq!(first_version.content, "第一版正文");
        assert_eq!(first_version.content_hash, first_commit.content_hash);

        let second_draft = KnowledgeDocumentService::save_manual_draft(
            &database,
            KnowledgeDocumentDraftInput {
                draft_id: None,
                document_id: Some(first_commit.document_id),
                project_id,
                title: "部署说明".to_string(),
                content: "第二版正文".to_string(),
                doc_type: "markdown".to_string(),
                base_version_id: Some(first_commit.document_version_id),
                revision: None,
                editor_label: Some("李四".to_string()),
            },
        )?;
        let second_commit = KnowledgeDocumentService::commit_manual_draft(
            &database,
            CommitKnowledgeDocumentDraftInput {
                draft_id: second_draft.draft.id,
                revision: second_draft.draft.revision,
                version_label: "第二版".to_string(),
                project_version_id: None,
                cross_version_scope: Some("project_all_versions".to_string()),
                commit_message: None,
                author_label: Some("李四".to_string()),
            },
        )?;
        assert_eq!(
            second_commit.parent_version_id,
            Some(first_commit.document_version_id)
        );
        assert_eq!(
            database
                .list_knowledge_document_versions(first_commit.document_id)?
                .len(),
            2,
            "历史版本必须保留，提交新版本不能覆盖旧版本"
        );
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn restoring_history_creates_a_new_draft_and_returns_current_content_on_conflict(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let root = test_root("restore-version");
        let database = database(&root)?;
        let project_id = create_project(&database)?;
        let initial_draft = KnowledgeDocumentService::save_manual_draft(
            &database,
            KnowledgeDocumentDraftInput {
                draft_id: None,
                document_id: None,
                project_id,
                title: "部署说明".to_string(),
                content: "第一版正文".to_string(),
                doc_type: "markdown".to_string(),
                base_version_id: None,
                revision: None,
                editor_label: Some("张三".to_string()),
            },
        )?;
        let initial_commit = KnowledgeDocumentService::commit_manual_draft(
            &database,
            CommitKnowledgeDocumentDraftInput {
                draft_id: initial_draft.draft.id,
                revision: initial_draft.draft.revision,
                version_label: "初始版本".to_string(),
                project_version_id: None,
                cross_version_scope: Some("project_all_versions".to_string()),
                commit_message: None,
                author_label: None,
            },
        )?;
        let later_draft = KnowledgeDocumentService::save_manual_draft(
            &database,
            KnowledgeDocumentDraftInput {
                draft_id: None,
                document_id: Some(initial_commit.document_id),
                project_id,
                title: "部署说明".to_string(),
                content: "后续版本正文".to_string(),
                doc_type: "markdown".to_string(),
                base_version_id: Some(initial_commit.document_version_id),
                revision: None,
                editor_label: Some("李四".to_string()),
            },
        )?;
        let later_commit = KnowledgeDocumentService::commit_manual_draft(
            &database,
            CommitKnowledgeDocumentDraftInput {
                draft_id: later_draft.draft.id,
                revision: later_draft.draft.revision,
                version_label: "后续版本".to_string(),
                project_version_id: None,
                cross_version_scope: Some("project_all_versions".to_string()),
                commit_message: None,
                author_label: None,
            },
        )?;

        let restored = KnowledgeDocumentService::restore_version_to_draft(
            &database,
            RestoreKnowledgeDocumentVersionToDraftInput {
                source_version_id: initial_commit.document_version_id,
                draft_id: None,
                revision: None,
                editor_label: Some("王五".to_string()),
            },
        )?;
        assert!(!restored.conflict);
        assert_eq!(
            restored.source_version.id,
            initial_commit.document_version_id
        );
        assert_eq!(restored.draft.content, "第一版正文");
        assert_eq!(
            restored.draft.base_version_id,
            Some(initial_commit.document_version_id)
        );
        let restored_commit = KnowledgeDocumentService::commit_manual_draft(
            &database,
            CommitKnowledgeDocumentDraftInput {
                draft_id: restored.draft.id,
                revision: restored.draft.revision,
                version_label: "恢复初始版本".to_string(),
                project_version_id: None,
                cross_version_scope: Some("project_all_versions".to_string()),
                commit_message: Some("恢复历史内容".to_string()),
                author_label: None,
            },
        )?;
        assert_eq!(
            restored_commit.parent_version_id,
            Some(initial_commit.document_version_id)
        );
        assert_eq!(
            database
                .list_knowledge_document_versions(initial_commit.document_id)?
                .len(),
            3,
            "恢复必须创建新版本，不能覆盖历史版本"
        );
        assert_ne!(
            later_commit.document_version_id,
            restored_commit.document_version_id
        );

        let target_draft = KnowledgeDocumentService::save_manual_draft(
            &database,
            KnowledgeDocumentDraftInput {
                draft_id: None,
                document_id: Some(initial_commit.document_id),
                project_id,
                title: "部署说明".to_string(),
                content: "待恢复草稿".to_string(),
                doc_type: "markdown".to_string(),
                base_version_id: Some(later_commit.document_version_id),
                revision: None,
                editor_label: None,
            },
        )?;
        let updated_target = KnowledgeDocumentService::save_manual_draft(
            &database,
            KnowledgeDocumentDraftInput {
                draft_id: Some(target_draft.draft.id),
                document_id: Some(initial_commit.document_id),
                project_id,
                title: "部署说明".to_string(),
                content: "正在编辑的新内容".to_string(),
                doc_type: "markdown".to_string(),
                base_version_id: Some(later_commit.document_version_id),
                revision: Some(target_draft.draft.revision),
                editor_label: None,
            },
        )?;
        let conflict = KnowledgeDocumentService::restore_version_to_draft(
            &database,
            RestoreKnowledgeDocumentVersionToDraftInput {
                source_version_id: initial_commit.document_version_id,
                draft_id: Some(target_draft.draft.id),
                revision: Some(target_draft.draft.revision),
                editor_label: None,
            },
        )?;
        assert!(conflict.conflict);
        assert_eq!(conflict.source_version.content, "第一版正文");
        assert_eq!(conflict.draft.content, updated_target.draft.content);
        assert_eq!(conflict.draft.revision, updated_target.draft.revision);

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn upload_grant_is_single_use_and_creates_a_queued_import_record(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let root = test_root("upload-import");
        let database = database(&root)?;
        let project_id = create_project(&database)?;
        let source = root.join("方案说明.md");
        fs::write(&source, "# 方案说明\n正文")?;
        let registry = KnowledgeUploadGrantRegistry::default();
        let prepared = KnowledgeDocumentService::prepare_upload_file(
            &registry,
            source.to_str().expect("临时路径必须是 UTF-8"),
        )?;
        assert_eq!(prepared.display_name, "方案说明.md");
        let result = KnowledgeDocumentService::create_upload_import(
            &database,
            &root,
            &registry,
            UploadKnowledgeAssetInput {
                project_id,
                project_version_id: None,
                cross_version_scope: Some("project_all_versions".to_string()),
                file_handle: prepared.file_handle.clone(),
                display_name: None,
                source_folder_name: None,
                allow_remote_ocr: false,
                ocr_provider_key: None,
            },
        )?;
        assert_eq!(result.status, "queued");
        assert_eq!(
            database
                .get_knowledge_job_by_id(result.import_job_id)?
                .expect("上传必须创建导入任务")
                .job_type,
            "upload_import"
        );
        assert_eq!(
            database
                .request_knowledge_job_cancel(result.import_job_id)?
                .status,
            "cancelled",
            "尚未执行的导入任务必须可取消"
        );
        let documents = database.list_knowledge_documents(&crate::models::KnowledgeListInput {
            project_id: Some(project_id),
            release_id: None,
            source_id: None,
            keyword: None,
            status: Some("processing".to_string()),
            offset: None,
            limit: None,
        })?;
        assert_eq!(documents.total, 1, "解析前上传文档必须明确为处理中");
        assert!(
            KnowledgeDocumentService::create_upload_import(
                &database,
                &root,
                &registry,
                UploadKnowledgeAssetInput {
                    project_id,
                    project_version_id: None,
                    cross_version_scope: Some("project_all_versions".to_string()),
                    file_handle: prepared.file_handle,
                    display_name: None,
                    source_folder_name: None,
                    allow_remote_ocr: false,
                    ocr_provider_key: None,
                },
            )
            .is_err(),
            "一次性句柄不得被重复提交"
        );
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn directory_upload_recursively_prepares_html_prototype_assets(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let root = test_root("directory-upload");
        let prototype = root.join("退款原型");
        let assets = prototype.join("assets");
        fs::create_dir_all(&assets)?;
        let html = "<!doctype html><html><body><h1>退款审批</h1></body></html>".as_bytes();
        let css = b"body { color: #123456; }";
        let javascript = b"document.querySelector('h1');";
        fs::write(prototype.join("index.html"), html)?;
        fs::write(assets.join("style.css"), css)?;
        fs::write(assets.join("app.js"), javascript)?;
        fs::write(prototype.join("preview.exe"), b"not supported")?;

        let registry = KnowledgeUploadGrantRegistry::default();
        let prepared = KnowledgeDocumentService::prepare_upload_directory(
            &registry,
            prototype.to_str().expect("临时路径必须是 UTF-8"),
        )?;

        assert_eq!(prepared.directory_name, "退款原型");
        assert_eq!(prepared.files.len(), 3);
        assert_eq!(prepared.skipped_count, 1);
        assert_eq!(
            prepared.total_size_bytes,
            i64::try_from(html.len() + css.len() + javascript.len())?,
        );
        assert_eq!(
            prepared
                .files
                .iter()
                .map(|file| file.display_name.as_str())
                .collect::<Vec<_>>(),
            vec!["assets/app.js", "assets/style.css", "index.html"],
        );
        let handles = prepared
            .files
            .iter()
            .map(|file| file.file_handle.clone())
            .collect::<Vec<_>>();
        assert_eq!(
            registry.total_size(&handles)?,
            prepared.total_size_bytes as u64
        );

        let database = database(&root)?;
        let project_id = create_project(&database)?;
        let batch = KnowledgeDocumentService::create_upload_import_batch(
            &database,
            &root,
            &registry,
            UploadKnowledgeAssetBatchInput {
                project_id,
                project_version_id: None,
                cross_version_scope: Some("project_all_versions".to_string()),
                source_folder_name: Some(prepared.directory_name.clone()),
                files: prepared
                    .files
                    .iter()
                    .map(|file| UploadKnowledgeAssetFileInput {
                        file_handle: file.file_handle.clone(),
                        display_name: Some(file.display_name.clone()),
                        allow_remote_ocr: false,
                        ocr_provider_key: None,
                    })
                    .collect(),
            },
        )?;
        assert!(batch.items.iter().all(|item| item.result.is_some()));
        let documents = database.list_knowledge_documents(&KnowledgeListInput {
            project_id: Some(project_id),
            release_id: None,
            source_id: None,
            keyword: None,
            status: None,
            offset: None,
            limit: None,
        })?;
        assert_eq!(documents.total, 3);
        assert!(documents
            .items
            .iter()
            .all(|document| { document.logical_path.starts_with("upload-folder/退款原型/") }));
        assert!(documents
            .items
            .iter()
            .any(|document| document.logical_path == "upload-folder/退款原型/index.html"));

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn formal_document_rejects_an_omitted_version_scope() -> Result<(), Box<dyn std::error::Error>>
    {
        let root = test_root("missing-version-scope");
        let database = database(&root)?;
        let project_id = create_project(&database)?;
        let draft = KnowledgeDocumentService::save_manual_draft(
            &database,
            KnowledgeDocumentDraftInput {
                draft_id: None,
                document_id: None,
                project_id,
                title: "未绑定资料".to_string(),
                content: "正文".to_string(),
                doc_type: "markdown".to_string(),
                base_version_id: None,
                revision: None,
                editor_label: None,
            },
        )?
        .draft;
        let error = KnowledgeDocumentService::commit_manual_draft(
            &database,
            CommitKnowledgeDocumentDraftInput {
                draft_id: draft.id,
                revision: draft.revision,
                version_label: "v1".to_string(),
                project_version_id: None,
                cross_version_scope: None,
                commit_message: None,
                author_label: None,
            },
        )
        .expect_err("未选择版本或跨版本范围必须被拒绝");
        assert!(error.to_string().contains("请选择关联版本"));
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn document_queries_filter_restricted_and_explain_processing_partial_and_failure(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let root = test_root("query-status");
        let database = database(&root)?;
        let project_id = create_project(&database)?;

        let source = root.join("处理中.md");
        fs::write(&source, "等待解析")?;
        let registry = KnowledgeUploadGrantRegistry::default();
        let prepared = KnowledgeDocumentService::prepare_upload_file(
            &registry,
            source.to_str().expect("临时路径必须是 UTF-8"),
        )?;
        let upload = KnowledgeDocumentService::create_upload_import(
            &database,
            &root,
            &registry,
            UploadKnowledgeAssetInput {
                project_id,
                project_version_id: None,
                cross_version_scope: Some("project_all_versions".to_string()),
                file_handle: prepared.file_handle,
                display_name: None,
                source_folder_name: None,
                allow_remote_ocr: false,
                ocr_provider_key: None,
            },
        )?;
        let processing =
            KnowledgeDocumentService::get_document_detail(&database, upload.document_id)?;
        assert_eq!(processing.processing.status, "processing");
        assert!(!processing.processing.content_available);
        assert!(processing.versions.is_empty(), "处理中不得返回虚构正文");
        assert_eq!(
            processing
                .processing
                .task
                .as_ref()
                .map(|task| task.job_type.as_str()),
            Some("upload_import")
        );

        database.finish_knowledge_job(
            upload.import_job_id,
            "failed",
            "导入失败",
            Some("损坏文件"),
            &serde_json::json!({ "documentId": upload.document_id }),
        )?;
        let failed = KnowledgeDocumentService::get_document_detail(&database, upload.document_id)?;
        assert_eq!(failed.processing.status, "failed");
        assert!(!failed.processing.content_available);
        assert_eq!(failed.processing.available_actions, vec!["重新尝试"]);

        let draft = KnowledgeDocumentService::save_manual_draft(
            &database,
            KnowledgeDocumentDraftInput {
                draft_id: None,
                document_id: None,
                project_id,
                title: "部分解析文档".to_string(),
                content: "可用正文".to_string(),
                doc_type: "markdown".to_string(),
                base_version_id: None,
                revision: None,
                editor_label: None,
            },
        )?;
        let committed = KnowledgeDocumentService::commit_manual_draft(
            &database,
            CommitKnowledgeDocumentDraftInput {
                draft_id: draft.draft.id,
                revision: draft.draft.revision,
                version_label: "初始版本".to_string(),
                project_version_id: None,
                cross_version_scope: Some("project_all_versions".to_string()),
                commit_message: None,
                author_label: None,
            },
        )?;
        database.insert_knowledge_document_parse_artifact(&NewKnowledgeDocumentParseArtifact {
            document_version_id: committed.document_version_id,
            asset_id: None,
            parser_id: "markdown".to_string(),
            parser_version: "1".to_string(),
            quality_level: "partial".to_string(),
            warning_json: serde_json::to_string(&vec!["图片未提取文字"])?,
            normalized_hash: "partial-document-hash".to_string(),
            structure_json: "[]".to_string(),
        })?;
        let partial =
            KnowledgeDocumentService::get_document_detail(&database, committed.document_id)?;
        assert_eq!(partial.processing.status, "partial");
        assert!(partial.processing.content_available);
        assert_eq!(partial.versions.len(), 1);
        assert_eq!(
            partial
                .processing
                .parser
                .as_ref()
                .map(|parser| parser.warnings.as_slice()),
            Some(["图片未提取文字".to_string()].as_slice())
        );

        let preview = KnowledgeDocumentService::preview_deletion(&database, committed.document_id)?;
        assert_eq!(preview.version_count, 1);
        assert!(!preview.permanent_deletion_enabled);
        KnowledgeDocumentService::soft_delete(&database, committed.document_id)?;
        assert!(database
            .get_knowledge_document_by_id(committed.document_id)?
            .is_none());
        assert!(
            KnowledgeDocumentService::get_document_detail(&database, committed.document_id)
                .is_err(),
            "软删除后详情入口必须返回 NotFound"
        );
        assert!(
            KnowledgeDocumentService::list_document_versions(&database, committed.document_id)
                .is_err(),
            "软删除后历史版本入口必须返回 NotFound"
        );
        let restored = KnowledgeDocumentService::restore(&database, committed.document_id)?;
        assert_eq!(restored.document.id, committed.document_id);
        assert!(database
            .get_knowledge_document_by_id(committed.document_id)?
            .is_some());
        assert!(KnowledgeDocumentService::restore(&database, committed.document_id).is_err());

        let restricted = database.upsert_knowledge_document(&UpsertKnowledgeDocumentInput {
            id: None,
            document_key: "restricted-query-document".to_string(),
            project_id: Some(project_id),
            source_id: None,
            doc_type: "markdown".to_string(),
            title: "受限文档".to_string(),
            logical_path: "restricted.md".to_string(),
            sensitivity: "restricted".to_string(),
            tags: Vec::new(),
            allow_ai: false,
            allow_mcp: false,
        })?;
        let visible = KnowledgeDocumentService::list_documents(
            &database,
            Some(KnowledgeListInput {
                project_id: Some(project_id),
                release_id: None,
                source_id: None,
                keyword: None,
                status: None,
                offset: None,
                limit: None,
            }),
        )?;
        assert!(!visible
            .items
            .iter()
            .any(|document| document.id == restricted.id));
        assert!(KnowledgeDocumentService::get_document_detail(&database, restricted.id).is_err());
        assert!(
            KnowledgeDocumentService::list_document_versions(&database, restricted.id).is_err()
        );
        assert!(KnowledgeDocumentService::preview_deletion(&database, restricted.id).is_err());
        assert!(KnowledgeDocumentService::soft_delete(&database, restricted.id).is_err());

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn batch_upload_keeps_queued_files_when_one_handle_has_expired(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let root = test_root("upload-batch");
        let database = database(&root)?;
        let project_id = create_project(&database)?;
        let source = root.join("可导入.md");
        fs::write(&source, "批量导入正文")?;
        let registry = KnowledgeUploadGrantRegistry::default();
        let prepared = KnowledgeDocumentService::prepare_upload_file(
            &registry,
            source.to_str().expect("临时路径必须是 UTF-8"),
        )?;
        let batch = KnowledgeDocumentService::create_upload_import_batch(
            &database,
            &root,
            &registry,
            UploadKnowledgeAssetBatchInput {
                project_id,
                project_version_id: None,
                cross_version_scope: Some("project_all_versions".to_string()),
                source_folder_name: None,
                files: vec![
                    UploadKnowledgeAssetFileInput {
                        file_handle: prepared.file_handle,
                        display_name: Some(prepared.display_name),
                        allow_remote_ocr: false,
                        ocr_provider_key: None,
                    },
                    UploadKnowledgeAssetFileInput {
                        file_handle: "upload-expired".to_string(),
                        display_name: Some("已失效.md".to_string()),
                        allow_remote_ocr: false,
                        ocr_provider_key: None,
                    },
                ],
            },
        )?;
        assert_eq!(batch.items.len(), 2);
        assert!(batch.items[0].result.is_some());
        assert_eq!(batch.items[0].error_message, None);
        assert!(batch.items[1].result.is_none());
        assert!(batch.items[1]
            .error_message
            .as_deref()
            .is_some_and(|message| message.contains("失效")));
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn upload_rechecks_changed_file_and_rejects_batches_above_the_limit(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let root = test_root("upload-recheck");
        let database = database(&root)?;
        let project_id = create_project(&database)?;
        let source = root.join("待校验.md");
        fs::write(&source, "初始 Markdown 内容")?;
        let registry = KnowledgeUploadGrantRegistry::default();
        let prepared = KnowledgeDocumentService::prepare_upload_file(
            &registry,
            source.to_str().expect("临时路径必须是 UTF-8"),
        )?;
        // 文件选择完成后被替换成二进制内容，消费句柄时必须再次拒绝，不能落盘或创建任务。
        fs::write(&source, [0_u8, 159, 146, 150])?;
        assert!(KnowledgeDocumentService::create_upload_import(
            &database,
            &root,
            &registry,
            UploadKnowledgeAssetInput {
                project_id,
                project_version_id: None,
                cross_version_scope: Some("project_all_versions".to_string()),
                file_handle: prepared.file_handle,
                display_name: None,
                source_folder_name: None,
                allow_remote_ocr: false,
                ocr_provider_key: None,
            },
        )
        .is_err());
        assert_eq!(
            database
                .list_knowledge_documents(&KnowledgeListInput {
                    project_id: Some(project_id),
                    release_id: None,
                    source_id: None,
                    keyword: None,
                    status: None,
                    offset: None,
                    limit: None,
                })?
                .total,
            0
        );

        let files = (0..51)
            .map(|index| UploadKnowledgeAssetFileInput {
                file_handle: format!("upload-{index}"),
                display_name: Some(format!("文件{index}.md")),
                allow_remote_ocr: false,
                ocr_provider_key: None,
            })
            .collect();
        assert!(KnowledgeDocumentService::create_upload_import_batch(
            &database,
            &root,
            &registry,
            UploadKnowledgeAssetBatchInput {
                project_id,
                project_version_id: None,
                cross_version_scope: Some("project_all_versions".to_string()),
                source_folder_name: None,
                files,
            },
        )
        .is_err());
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn upload_copies_to_content_addressed_storage_and_reuses_duplicate_content(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let root = test_root("deduplicate");
        let source = root.join("外部来源.md");
        let content = "# 项目说明\n".as_bytes();
        fs::write(&source, content)?;
        let database = database(&root)?;

        let first = KnowledgeDocumentService::store_uploaded_asset(
            &database,
            &root,
            &source,
            Some("项目/说明.md"),
            "text/markdown",
        )?;
        let second = KnowledgeDocumentService::store_uploaded_asset(
            &database,
            &root,
            &source,
            Some("重复文件.md"),
            "text/markdown",
        )?;
        let hash = format!("{:x}", Sha256::digest(content));
        assert_eq!(first.id, second.id);
        assert!(!first.reused_existing_content);
        assert!(second.reused_existing_content);
        assert_eq!(first.storage_key, storage_key_for(&hash));
        assert_eq!(first.normalized_name, "项目_说明.md");
        assert_eq!(
            fs::read(root.join("knowledge-assets").join(&first.storage_key))?,
            content
        );
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn upload_rejects_tampered_existing_content_addressed_file(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let root = test_root("tampered");
        let source = root.join("文档.md");
        let trusted_content = "可信内容".as_bytes();
        fs::write(&source, trusted_content)?;
        let hash = format!("{:x}", Sha256::digest(trusted_content));
        let target = root.join("knowledge-assets").join(storage_key_for(&hash));
        fs::create_dir_all(target.parent().expect("哈希目标应有父目录"))?;
        let tampered_content = "被替换的内容".as_bytes();
        fs::write(&target, tampered_content)?;

        let result = KnowledgeDocumentService::store_uploaded_asset(
            &database(&root)?,
            &root,
            &source,
            None,
            "text/markdown",
        );
        assert!(result.is_err());
        assert_eq!(fs::read(target)?, tampered_content);
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn asset_storage_failure_does_not_create_processing_document(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let root = test_root("storage-failure");
        let database = database(&root)?;
        let project_id = create_project(&database)?;
        let source = root.join("无法复制.md");
        fs::write(&source, "正文")?;
        // 受控资产根必须是目录。预先创建普通文件可稳定模拟复制前的本地存储失败，
        // 不依赖权限、磁盘空间或外部文件系统行为。
        fs::write(root.join("knowledge-assets"), "not-a-directory")?;
        let registry = KnowledgeUploadGrantRegistry::default();
        let prepared = KnowledgeDocumentService::prepare_upload_file(
            &registry,
            source.to_str().expect("临时路径必须是 UTF-8"),
        )?;

        assert!(KnowledgeDocumentService::create_upload_import(
            &database,
            &root,
            &registry,
            UploadKnowledgeAssetInput {
                project_id,
                project_version_id: None,
                cross_version_scope: Some("project_all_versions".to_string()),
                file_handle: prepared.file_handle,
                display_name: None,
                source_folder_name: None,
                allow_remote_ocr: false,
                ocr_provider_key: None,
            },
        )
        .is_err());
        let documents = database.list_knowledge_documents(&KnowledgeListInput {
            project_id: Some(project_id),
            release_id: None,
            source_id: None,
            keyword: None,
            status: None,
            offset: None,
            limit: None,
        })?;
        assert_eq!(documents.total, 0, "资产复制失败不得创建处理中或孤立文档");
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn upload_rejects_symbolic_link_source_and_storage_target(
    ) -> Result<(), Box<dyn std::error::Error>> {
        use std::os::unix::fs::symlink;

        let root = test_root("symlink");
        let external = test_root("external");
        let source = external.join("真实文件.md");
        let content = "内容".as_bytes();
        fs::write(&source, content)?;
        let linked_source = root.join("链接文件.md");
        symlink(&source, &linked_source)?;
        assert!(KnowledgeDocumentService::store_uploaded_asset(
            &database(&root)?,
            &root,
            &linked_source,
            None,
            "text/markdown",
        )
        .is_err());

        let hash = format!("{:x}", Sha256::digest(content));
        let target = root.join("knowledge-assets").join(storage_key_for(&hash));
        fs::create_dir_all(target.parent().expect("哈希目标应有父目录"))?;
        symlink(&source, &target)?;
        assert!(KnowledgeDocumentService::store_uploaded_asset(
            &database(&root)?,
            &root,
            &source,
            None,
            "text/markdown",
        )
        .is_err());
        fs::remove_dir_all(root)?;
        fs::remove_dir_all(external)?;
        Ok(())
    }

    #[test]
    fn file_name_normalization_rejects_unsafe_or_overlong_names() {
        assert_eq!(
            normalize_file_name(" ../设计\\说明.md ").unwrap(),
            "设计_说明.md"
        );
        assert!(normalize_file_name("../").is_err());
        assert!(normalize_file_name(&"a".repeat(181)).is_err());
    }

    #[test]
    fn image_preview_reads_only_verified_current_managed_asset(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let root = test_root("image-preview");
        let database = database(&root)?;
        let project_id = create_project(&database)?;
        let source = root.join("退款流程.png");
        fs::write(
            &source,
            b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR\0\0\x04\0\0\0\x03\0fixture",
        )?;
        let registry = KnowledgeUploadGrantRegistry::default();
        let prepared = KnowledgeDocumentService::prepare_upload_file(
            &registry,
            source.to_str().expect("临时路径必须是 UTF-8"),
        )?;
        let upload = KnowledgeDocumentService::create_upload_import(
            &database,
            &root,
            &registry,
            UploadKnowledgeAssetInput {
                project_id,
                project_version_id: None,
                cross_version_scope: Some("project_all_versions".to_string()),
                file_handle: prepared.file_handle,
                display_name: None,
                source_folder_name: None,
                allow_remote_ocr: false,
                ocr_provider_key: None,
            },
        )?;
        crate::services::knowledge_domain::jobs::KnowledgeUploadImportJobService::run_upload_import_job(
            &database,
            &root,
            upload.import_job_id,
        )?;

        let preview = KnowledgeDocumentService::get_document_image_preview(
            &database,
            &root,
            upload.document_id,
        )?;
        assert_eq!(preview.mime_type, "image/png");
        assert_eq!((preview.width, preview.height), (Some(1024), Some(768)));
        assert!(preview.data_url.starts_with("data:image/png;base64,"));
        assert!(!preview.data_url.contains(root.to_string_lossy().as_ref()));
        fs::remove_dir_all(root)?;
        Ok(())
    }
}
