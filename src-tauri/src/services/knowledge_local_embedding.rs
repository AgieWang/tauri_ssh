use std::fs;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

#[cfg(feature = "local-embedding-fastembed")]
use std::sync::OnceLock;

use chrono::Utc;
use reqwest::{redirect, Url};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::AppError;
use crate::models::{
    DownloadKnowledgeLocalEmbeddingModelInput, GenerateKnowledgeLocalEmbeddingsInput,
    ImportKnowledgeLocalEmbeddingModelInput, KnowledgeLocalEmbeddingCacheEntry,
    KnowledgeLocalEmbeddingDownloadProgress, KnowledgeLocalEmbeddingModelImportResult,
    KnowledgeLocalEmbeddingRuntimeStatus, RemoveKnowledgeLocalEmbeddingModelInput,
};

/// 本地模型仅存放于应用数据目录，绝不从调用方传入的任意目录加载。
const LOCAL_EMBEDDING_CACHE_DIRECTORY: &str = "knowledge-models";
const FASTEMBED_RUNTIME: &str = "fastembed-5.17.0";
const MODEL_MANIFEST_FILE: &str = ".knowledge-model.json";
const MAX_MODEL_CACHE_BYTES: u64 = 8 * 1024 * 1024 * 1024;
const DOWNLOAD_RETRY_COUNT: usize = 3;

/// ONNX Runtime 是进程级全局库。首次启用的模型包确定其位置，之后拒绝切换，
/// 以免多个模型包在同一进程内加载不同 ABI 的动态库。
#[cfg(feature = "local-embedding-fastembed")]
struct InitializedOnnxRuntime {
    path: PathBuf,
    sha256: String,
}

#[cfg(feature = "local-embedding-fastembed")]
static INITIALIZED_ONNX_RUNTIME: OnceLock<InitializedOnnxRuntime> = OnceLock::new();

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModelManifest {
    model_key: String,
    sha256: String,
    size_bytes: i64,
    imported_at: String,
}

/// 内部镜像的最小清单格式。文件 URL 必须与镜像同源，目录摘要采用本地相同算法。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MirrorModelManifest {
    model_key: String,
    sha256: String,
    files: Vec<MirrorModelFile>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MirrorModelFile {
    path: String,
    sha256: String,
    size_bytes: i64,
    url: String,
}

pub struct KnowledgeLocalEmbeddingService;

impl KnowledgeLocalEmbeddingService {
    /// 返回受控缓存的元数据。该方法不会下载、解压或加载任何模型。
    pub fn runtime_status(
        app_data_dir: &Path,
    ) -> Result<KnowledgeLocalEmbeddingRuntimeStatus, AppError> {
        let cache_dir = Self::cache_dir(app_data_dir)?;
        let cache_exists = cache_dir.exists();
        let cached_models = if cache_exists {
            let verified_cache_dir = validate_cache_dir(app_data_dir, &cache_dir)?;
            Self::list_cached_models(&verified_cache_dir)?
        } else {
            Vec::new()
        };
        let fastembed_feature_enabled = cfg!(feature = "local-embedding-fastembed");
        // 缓存清单已经逐文件校验，具备运行时加载的必要前置条件；首次真正使用时仍会
        // 通过短文本探测确认具体模型布局和 ONNX Runtime 是否可加载。
        let runtime_available = fastembed_feature_enabled && !cached_models.is_empty();
        let mut warnings = Vec::new();
        if !fastembed_feature_enabled {
            warnings.push(
                "当前构建未启用本地向量化运行时；不会自动下载 ONNX Runtime 或模型".to_string(),
            );
        } else if cached_models.is_empty() {
            warnings.push("未发现已验证的本地向量化模型缓存".to_string());
        }
        Ok(KnowledgeLocalEmbeddingRuntimeStatus {
            runtime: FASTEMBED_RUNTIME.to_string(),
            fastembed_feature_enabled,
            runtime_available,
            automatic_download_enabled: false,
            cache_dir: cache_dir.to_string_lossy().to_string(),
            cache_exists,
            cached_models,
            warnings,
        })
    }

    /// 显式将一个已下载的模型目录复制到受控缓存。模型文件不会原地引用，也不会
    /// 因导入而联网；SHA-256 不匹配时会清除临时目录且不会暴露半成品缓存。
    pub fn import_model(
        app_data_dir: &Path,
        input: ImportKnowledgeLocalEmbeddingModelInput,
    ) -> Result<KnowledgeLocalEmbeddingModelImportResult, AppError> {
        let model_key = validated_model_key(&input.model_key)?;
        let expected_sha256 = validated_sha256(&input.expected_sha256)?;
        let source = validate_import_source(Path::new(input.source_path.trim()))?;
        let cache_dir = Self::ensure_cache_dir(app_data_dir)?;
        cleanup_incomplete_imports(&cache_dir)?;
        let target = cache_dir.join(&model_key);
        if target.exists() {
            return Err(AppError::InvalidInput(format!(
                "本地向量化模型缓存已存在: {model_key}；请先显式清理后再导入"
            )));
        }

        let staging = cache_dir.join(format!(
            ".import-{model_key}-{}-{}",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        fs::create_dir(&staging)?;
        let imported = (|| -> Result<KnowledgeLocalEmbeddingModelImportResult, AppError> {
            let size_bytes = copy_model_directory(&source, &staging)?;
            let size_bytes = i64::try_from(size_bytes)
                .map_err(|_| AppError::InvalidInput("模型文件总大小超出支持范围".to_string()))?;
            let actual_sha256 = directory_sha256(&staging)?;
            if actual_sha256 != expected_sha256 {
                return Err(AppError::InvalidInput(
                    "离线模型 SHA-256 校验失败；缓存未保留".to_string(),
                ));
            }
            let manifest = ModelManifest {
                model_key: model_key.clone(),
                sha256: actual_sha256.clone(),
                size_bytes,
                imported_at: Utc::now().to_rfc3339(),
            };
            write_manifest(&staging, &manifest)?;
            fs::rename(&staging, &target)?;
            Ok(KnowledgeLocalEmbeddingModelImportResult {
                model_key,
                sha256: actual_sha256,
                size_bytes,
                imported_at: manifest.imported_at,
            })
        })();
        if imported.is_err() && staging.exists() {
            let _ = fs::remove_dir_all(&staging);
        }
        imported
    }

    /// 从已配置的内部镜像显式下载模型。镜像清单、每个文件和完整目录均需通过 SHA-256
    /// 校验；禁用重定向以免下载途中切换到非受控主机。
    pub async fn download_model_from_mirror<F>(
        app_data_dir: &Path,
        mirror_url: &str,
        input: DownloadKnowledgeLocalEmbeddingModelInput,
        mut on_progress: F,
    ) -> Result<KnowledgeLocalEmbeddingModelImportResult, AppError>
    where
        F: FnMut(KnowledgeLocalEmbeddingDownloadProgress),
    {
        let model_key = validated_model_key(&input.model_key)?;
        let mirror_url = validate_mirror_url(mirror_url)?;
        let cache_dir = Self::ensure_cache_dir(app_data_dir)?;
        cleanup_incomplete_imports(&cache_dir)?;
        let target = cache_dir.join(&model_key);
        if target.exists() {
            return Err(AppError::InvalidInput(format!(
                "本地向量化模型缓存已存在: {model_key}；请先显式清理后再下载"
            )));
        }
        let client = reqwest::Client::builder()
            .redirect(redirect::Policy::none())
            .timeout(Duration::from_secs(60))
            .build()
            .map_err(|error| AppError::Custom(format!("创建内部模型镜像客户端失败: {error}")))?;
        let manifest_url = mirror_url
            .join(&format!("models/{model_key}/manifest.json"))
            .map_err(|_| AppError::InvalidInput("内部模型镜像地址无效".to_string()))?;
        let manifest = get_json_with_retry::<MirrorModelManifest>(&client, manifest_url).await?;
        if manifest.model_key != model_key {
            return Err(AppError::InvalidInput(
                "内部模型镜像清单与请求模型不匹配".to_string(),
            ));
        }
        let expected_sha256 = validated_sha256(&manifest.sha256)?;
        if manifest.files.is_empty() {
            return Err(AppError::InvalidInput(
                "内部模型镜像清单未包含模型文件".to_string(),
            ));
        }
        let total_bytes = manifest.files.iter().try_fold(0_i64, |total, file| {
            if file.size_bytes < 0 {
                return Err(AppError::InvalidInput(
                    "内部模型镜像清单包含无效文件大小".to_string(),
                ));
            }
            total
                .checked_add(file.size_bytes)
                .ok_or_else(|| AppError::InvalidInput("内部模型镜像文件总大小超出范围".to_string()))
        })?;
        if u64::try_from(total_bytes).unwrap_or(u64::MAX) > MAX_MODEL_CACHE_BYTES {
            return Err(AppError::InvalidInput(format!(
                "内部模型镜像超过 {}GB 缓存上限",
                MAX_MODEL_CACHE_BYTES / 1024 / 1024 / 1024
            )));
        }
        let files_total = i64::try_from(manifest.files.len())
            .map_err(|_| AppError::InvalidInput("内部模型镜像文件数量超出范围".to_string()))?;
        on_progress(download_progress(
            "downloading",
            &model_key,
            0,
            files_total,
            0,
            total_bytes,
        ));

        let staging = cache_dir.join(format!(
            ".import-{model_key}-{}-{}",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        fs::create_dir(&staging)?;
        let result = async {
            let mut bytes_downloaded = 0_i64;
            for (index, file) in manifest.files.iter().enumerate() {
                let relative_path = safe_mirror_relative_path(&file.path)?;
                let file_url = same_origin_mirror_file_url(&mirror_url, &file.url)?;
                let destination = staging.join(&relative_path);
                let parent = destination.parent().ok_or_else(|| {
                    AppError::InvalidInput("内部模型镜像文件路径无效".to_string())
                })?;
                fs::create_dir_all(parent)?;
                let downloaded = download_file_with_retry(&client, file_url, &destination).await?;
                let expected_file_sha = validated_sha256(&file.sha256)?;
                let actual_file_sha = sha256_file(&destination)?;
                if actual_file_sha != expected_file_sha || downloaded != file.size_bytes {
                    return Err(AppError::InvalidInput(
                        "内部模型镜像文件校验失败；缓存未保留".to_string(),
                    ));
                }
                bytes_downloaded = bytes_downloaded.checked_add(downloaded).ok_or_else(|| {
                    AppError::InvalidInput("内部模型镜像文件总大小超出范围".to_string())
                })?;
                on_progress(download_progress(
                    "downloading",
                    &model_key,
                    i64::try_from(index + 1).unwrap_or(files_total),
                    files_total,
                    bytes_downloaded,
                    total_bytes,
                ));
            }
            let actual_sha256 = directory_sha256(&staging)?;
            if actual_sha256 != expected_sha256 {
                return Err(AppError::InvalidInput(
                    "内部模型镜像目录 SHA-256 校验失败；缓存未保留".to_string(),
                ));
            }
            let manifest = ModelManifest {
                model_key: model_key.clone(),
                sha256: actual_sha256.clone(),
                size_bytes: bytes_downloaded,
                imported_at: Utc::now().to_rfc3339(),
            };
            write_manifest(&staging, &manifest)?;
            fs::rename(&staging, &target)?;
            on_progress(download_progress(
                "completed",
                &model_key,
                files_total,
                files_total,
                bytes_downloaded,
                total_bytes,
            ));
            Ok(KnowledgeLocalEmbeddingModelImportResult {
                model_key,
                sha256: actual_sha256,
                size_bytes: bytes_downloaded,
                imported_at: manifest.imported_at,
            })
        }
        .await;
        if result.is_err() && staging.exists() {
            let _ = fs::remove_dir_all(&staging);
        }
        result
    }

    /// 使用已校验的本地缓存生成向量。该方法没有远程回退分支；运行时不可用、模型不完整
    /// 或调用被取消时均返回结构化错误，由上层决定重试或保留旧索引。
    pub fn generate_embeddings<F>(
        app_data_dir: &Path,
        input: GenerateKnowledgeLocalEmbeddingsInput,
        mut is_cancelled: F,
    ) -> Result<Vec<Vec<f32>>, AppError>
    where
        F: FnMut() -> bool,
    {
        let model_key = validated_model_key(&input.model_key)?;
        if input.texts.is_empty() {
            return Err(AppError::InvalidInput("本地向量化文本不能为空".to_string()));
        }
        if input.prefix.trim().is_empty() {
            return Err(AppError::InvalidInput(
                "本地向量化必须提供明确的查询或文档前缀".to_string(),
            ));
        }
        let batch_size = input.batch_size.unwrap_or(16).clamp(1, 64) as usize;
        let cache_dir = Self::cache_dir(app_data_dir)?;
        let cache_dir = validate_cache_dir(app_data_dir, &cache_dir)?;
        let model_dir = verified_model_dir(&cache_dir, &model_key)?;
        let prefixed = input
            .texts
            .iter()
            .map(|text| {
                if text.trim().is_empty() {
                    Err(AppError::InvalidInput(
                        "本地向量化不接受空白文本".to_string(),
                    ))
                } else {
                    Ok(format!("{}{}", input.prefix, text))
                }
            })
            .collect::<Result<Vec<_>, _>>()?;
        generate_embeddings_with_fastembed(
            &model_dir,
            &model_key,
            &prefixed,
            batch_size,
            &mut is_cancelled,
        )
    }

    /// 仅允许删除本应用托管且模型键合法的缓存目录，绝不递归删除调用方任意路径。
    pub fn remove_model(
        app_data_dir: &Path,
        input: RemoveKnowledgeLocalEmbeddingModelInput,
    ) -> Result<(), AppError> {
        let model_key = validated_model_key(&input.model_key)?;
        let cache_dir = Self::cache_dir(app_data_dir)?;
        if !cache_dir.exists() {
            return Ok(());
        }
        let cache_dir = validate_cache_dir(app_data_dir, &cache_dir)?;
        let target = cache_dir.join(model_key);
        if !target.exists() {
            return Ok(());
        }
        let metadata = fs::symlink_metadata(&target)?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(AppError::InvalidInput(
                "本地向量化模型缓存路径不合法，拒绝清理".to_string(),
            ));
        }
        fs::remove_dir_all(target)?;
        Ok(())
    }

    /// 仅供后续显式导入/下载流程使用；创建的根目录始终受应用数据目录约束。
    #[allow(dead_code)] // 5.2 导入与下载流程接入前保留受控目录创建边界。
    pub fn ensure_cache_dir(app_data_dir: &Path) -> Result<PathBuf, AppError> {
        let cache_dir = Self::cache_dir(app_data_dir)?;
        fs::create_dir_all(&cache_dir)?;
        validate_cache_dir(app_data_dir, &cache_dir)
    }

    fn cache_dir(app_data_dir: &Path) -> Result<PathBuf, AppError> {
        if app_data_dir.as_os_str().is_empty() {
            return Err(AppError::InvalidInput("应用数据目录不能为空".to_string()));
        }
        Ok(app_data_dir.join(LOCAL_EMBEDDING_CACHE_DIRECTORY))
    }

    fn list_cached_models(
        cache_dir: &Path,
    ) -> Result<Vec<KnowledgeLocalEmbeddingCacheEntry>, AppError> {
        let mut models = fs::read_dir(cache_dir)?
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let file_type = entry.file_type().ok()?;
                if !file_type.is_dir() || file_type.is_symlink() {
                    return None;
                }
                let model_key = entry.file_name().to_string_lossy().to_string();
                if !is_safe_model_key(&model_key) {
                    return None;
                }
                let manifest = read_manifest(&entry.path()).ok()?;
                if manifest.model_key != model_key || validated_sha256(&manifest.sha256).is_err() {
                    return None;
                }
                let actual_sha256 = directory_sha256(&entry.path()).ok()?;
                if actual_sha256 != manifest.sha256 {
                    return None;
                }
                let size_bytes = directory_size(&entry.path()).ok()?;
                let size_bytes = i64::try_from(size_bytes).ok()?;
                if size_bytes != manifest.size_bytes {
                    return None;
                }
                Some(KnowledgeLocalEmbeddingCacheEntry {
                    model_key,
                    size_bytes,
                    sha256: manifest.sha256,
                    imported_at: manifest.imported_at,
                })
            })
            .collect::<Vec<_>>();
        models.sort_by(|left, right| left.model_key.cmp(&right.model_key));
        Ok(models)
    }
}

fn verified_model_dir(cache_dir: &Path, model_key: &str) -> Result<PathBuf, AppError> {
    let model_dir = cache_dir.join(model_key);
    let metadata = fs::symlink_metadata(&model_dir)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(AppError::InvalidInput(
            "本地向量化模型缓存路径不合法".to_string(),
        ));
    }
    let canonical_model_dir = fs::canonicalize(&model_dir)?;
    if !canonical_model_dir.starts_with(cache_dir) {
        return Err(AppError::InvalidInput(
            "本地向量化模型缓存越出受控目录".to_string(),
        ));
    }
    let manifest = read_manifest(&canonical_model_dir)?;
    if manifest.model_key != model_key || directory_sha256(&canonical_model_dir)? != manifest.sha256
    {
        return Err(AppError::InvalidInput(
            "本地向量化模型缓存完整性校验失败".to_string(),
        ));
    }
    Ok(canonical_model_dir)
}

#[cfg(feature = "local-embedding-fastembed")]
fn generate_embeddings_with_fastembed<F>(
    model_dir: &Path,
    model_key: &str,
    texts: &[String],
    batch_size: usize,
    is_cancelled: &mut F,
) -> Result<Vec<Vec<f32>>, AppError>
where
    F: FnMut() -> bool,
{
    use fastembed::{Pooling, TextEmbedding, TokenizerFiles, UserDefinedEmbeddingModel};

    configure_fastembed_runtime(model_dir)?;

    let read_required = |name: &str| read_model_file(model_dir, name);
    let tokenizer_files = TokenizerFiles {
        tokenizer_file: read_required("tokenizer.json")?,
        config_file: read_required("config.json")?,
        special_tokens_map_file: read_required("special_tokens_map.json")?,
        tokenizer_config_file: read_required("tokenizer_config.json")?,
    };
    // FastEmbed 官方包使用 `onnx/model.onnx`，而内部离线包可以扁平为 `model.onnx`；
    // 两种位置均经过同一模型包边界校验，不能借此读取任意嵌套路径。
    let model_file = if model_dir.join("model.onnx").exists() {
        read_required("model.onnx")?
    } else {
        read_required("onnx/model.onnx")?
    };
    let pooling = if model_key.to_ascii_lowercase().contains("bge") {
        // BGE 的 CLS 表征与其官方训练/检索约定一致；误用 mean 会让中文基准失真。
        Pooling::Cls
    } else {
        // multilingual-e5 等候选遵循 query/document 前缀加 mean pooling 的约定。
        Pooling::Mean
    };
    let model = UserDefinedEmbeddingModel::new(model_file, tokenizer_files).with_pooling(pooling);
    let mut embedding = TextEmbedding::try_new_from_user_defined(model, Default::default())
        .map_err(|error| AppError::Custom(format!("加载本地向量化模型失败: {error}")))?;
    let mut vectors = Vec::with_capacity(texts.len());
    for batch in texts.chunks(batch_size) {
        if is_cancelled() {
            return Err(AppError::Custom("本地向量化已取消".to_string()));
        }
        let batch_vectors = embedding
            .embed(batch, Some(batch.len()))
            .map_err(|error| AppError::Custom(format!("本地向量化推理失败: {error}")))?;
        for vector in batch_vectors {
            vectors.push(normalize_embedding_vector(vector)?);
        }
    }
    if vectors.len() != texts.len() {
        return Err(AppError::Custom(
            "本地向量化返回向量数量与输入不一致".to_string(),
        ));
    }
    ensure_consistent_dimensions(&vectors)?;
    Ok(vectors)
}

#[cfg(feature = "local-embedding-fastembed")]
fn read_model_file(model_dir: &Path, relative_path: &str) -> Result<Vec<u8>, AppError> {
    let path = model_dir.join(relative_path);
    let metadata = fs::symlink_metadata(&path).map_err(|_| {
        AppError::InvalidInput(format!("本地向量化模型缺少安全的 {relative_path} 文件"))
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(AppError::InvalidInput(format!(
            "本地向量化模型缺少安全的 {relative_path} 文件"
        )));
    }
    let canonical = fs::canonicalize(&path)?;
    if !canonical.starts_with(model_dir) {
        return Err(AppError::InvalidInput(
            "本地向量化模型文件越出受控目录".to_string(),
        ));
    }
    Ok(fs::read(canonical)?)
}

/// 只允许从已验证的模型包 `runtime/` 目录加载当前平台的 ONNX Runtime；不采纳
/// 调用方传入的任意路径，也不使用构建机或用户环境中残留的同名动态库。
#[cfg(feature = "local-embedding-fastembed")]
fn configure_fastembed_runtime(model_dir: &Path) -> Result<(), AppError> {
    let runtime_path = verified_onnx_runtime_library(model_dir)?;
    let runtime_sha256 = sha256_file(&runtime_path)?;
    let initialized = INITIALIZED_ONNX_RUNTIME.get_or_init(|| {
        // `ort` 首次访问 API 时读取该变量。路径已通过模型目录边界和符号链接检查，
        // 因此不会把未经授权的环境路径带入知识库推理链路。
        std::env::set_var("ORT_DYLIB_PATH", &runtime_path);
        InitializedOnnxRuntime {
            path: runtime_path.clone(),
            sha256: runtime_sha256.clone(),
        }
    });
    if initialized.path != runtime_path || initialized.sha256 != runtime_sha256 {
        return Err(AppError::InvalidInput(
            "当前进程已使用另一份 ONNX Runtime；请重启应用后再切换本地向量化模型包".to_string(),
        ));
    }
    Ok(())
}

#[cfg(feature = "local-embedding-fastembed")]
fn verified_onnx_runtime_library(model_dir: &Path) -> Result<PathBuf, AppError> {
    let runtime_root = model_dir.join("runtime");
    let metadata = fs::symlink_metadata(&runtime_root)
        .map_err(|_| AppError::InvalidInput("本地向量化模型包缺少受控 ONNX Runtime".to_string()))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(AppError::InvalidInput(
            "本地向量化 ONNX Runtime 目录不合法".to_string(),
        ));
    }
    let canonical_runtime_root = fs::canonicalize(&runtime_root)?;
    if !canonical_runtime_root.starts_with(model_dir) {
        return Err(AppError::InvalidInput(
            "本地向量化 ONNX Runtime 越出模型包边界".to_string(),
        ));
    }
    let library = canonical_runtime_root.join(onnx_runtime_library_name());
    let metadata = fs::symlink_metadata(&library).map_err(|_| {
        AppError::InvalidInput(format!(
            "本地向量化模型包缺少当前平台 ONNX Runtime: {}",
            onnx_runtime_library_name()
        ))
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(AppError::InvalidInput(
            "本地向量化 ONNX Runtime 文件不合法".to_string(),
        ));
    }
    let canonical_library = fs::canonicalize(&library)?;
    if !canonical_library.starts_with(&canonical_runtime_root) {
        return Err(AppError::InvalidInput(
            "本地向量化 ONNX Runtime 越出受控目录".to_string(),
        ));
    }
    Ok(canonical_library)
}

#[cfg(feature = "local-embedding-fastembed")]
fn onnx_runtime_library_name() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        "onnxruntime.dll"
    }
    #[cfg(target_os = "macos")]
    {
        "libonnxruntime.dylib"
    }
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        "libonnxruntime.so"
    }
}

#[cfg(not(feature = "local-embedding-fastembed"))]
fn generate_embeddings_with_fastembed<F>(
    _model_dir: &Path,
    _model_key: &str,
    _texts: &[String],
    _batch_size: usize,
    _is_cancelled: &mut F,
) -> Result<Vec<Vec<f32>>, AppError>
where
    F: FnMut() -> bool,
{
    Err(AppError::InvalidInput(
        "当前构建未启用本地向量化运行时，不能运行本地向量化".to_string(),
    ))
}

#[cfg_attr(not(feature = "local-embedding-fastembed"), allow(dead_code))]
fn normalize_embedding_vector(mut vector: Vec<f32>) -> Result<Vec<f32>, AppError> {
    if vector.is_empty() || vector.iter().any(|value| !value.is_finite()) {
        return Err(AppError::InvalidInput(
            "本地向量化返回空向量或非有限数".to_string(),
        ));
    }
    let norm = vector
        .iter()
        .map(|value| f64::from(*value) * f64::from(*value))
        .sum::<f64>()
        .sqrt();
    if !norm.is_finite() || norm <= 0.0 {
        return Err(AppError::InvalidInput(
            "本地向量化返回零范数向量".to_string(),
        ));
    }
    for value in &mut vector {
        *value = (f64::from(*value) / norm) as f32;
    }
    Ok(vector)
}

#[cfg_attr(not(feature = "local-embedding-fastembed"), allow(dead_code))]
fn ensure_consistent_dimensions(vectors: &[Vec<f32>]) -> Result<(), AppError> {
    let Some(first) = vectors.first() else {
        return Ok(());
    };
    if vectors.iter().any(|vector| vector.len() != first.len()) {
        return Err(AppError::InvalidInput(
            "本地向量化返回了不一致的向量维度".to_string(),
        ));
    }
    Ok(())
}

fn is_safe_model_key(value: &str) -> bool {
    !value.is_empty()
        && value != "."
        && value != ".."
        && value.bytes().any(|byte| byte.is_ascii_alphanumeric())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn validated_model_key(value: &str) -> Result<String, AppError> {
    let value = value.trim();
    if !is_safe_model_key(value) {
        return Err(AppError::InvalidInput(
            "本地向量化模型标识只能包含字母、数字、点、短横线和下划线".to_string(),
        ));
    }
    Ok(value.to_string())
}

fn validated_sha256(value: &str) -> Result<String, AppError> {
    let value = value.trim().to_ascii_lowercase();
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(AppError::InvalidInput(
            "离线模型必须提供 64 位 SHA-256 校验值".to_string(),
        ));
    }
    Ok(value)
}

fn validate_mirror_url(value: &str) -> Result<Url, AppError> {
    let mut url = Url::parse(value.trim())
        .map_err(|_| AppError::InvalidInput("内部模型镜像地址无效".to_string()))?;
    if !url.username().is_empty() || url.password().is_some() || url.query().is_some() {
        return Err(AppError::InvalidInput(
            "内部模型镜像地址不能包含凭据或查询参数".to_string(),
        ));
    }
    let is_localhost = matches!(
        url.host_str(),
        Some("localhost") | Some("127.0.0.1") | Some("::1")
    );
    if url.scheme() != "https" && !(url.scheme() == "http" && is_localhost) {
        return Err(AppError::InvalidInput(
            "内部模型镜像必须使用 HTTPS（本机测试镜像可使用 HTTP）".to_string(),
        ));
    }
    if url.host_str().is_none() {
        return Err(AppError::InvalidInput("内部模型镜像缺少主机".to_string()));
    }
    if !url.path().ends_with('/') {
        let path = format!("{}/", url.path());
        url.set_path(&path);
    }
    Ok(url)
}

fn same_origin_mirror_file_url(mirror_url: &Url, value: &str) -> Result<Url, AppError> {
    let url = mirror_url
        .join(value)
        .map_err(|_| AppError::InvalidInput("内部模型镜像文件地址无效".to_string()))?;
    if url.scheme() != mirror_url.scheme()
        || url.host_str() != mirror_url.host_str()
        || url.port_or_known_default() != mirror_url.port_or_known_default()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(AppError::InvalidInput(
            "内部模型镜像文件必须与配置镜像同源".to_string(),
        ));
    }
    Ok(url)
}

fn safe_mirror_relative_path(value: &str) -> Result<PathBuf, AppError> {
    let path = Path::new(value);
    if path.as_os_str().is_empty() || path.is_absolute() {
        return Err(AppError::InvalidInput(
            "内部模型镜像文件路径必须是相对路径".to_string(),
        ));
    }
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => normalized.push(part),
            _ => {
                return Err(AppError::InvalidInput(
                    "内部模型镜像文件路径不能包含上级目录".to_string(),
                ));
            }
        }
    }
    Ok(normalized)
}

fn download_progress(
    stage: &str,
    model_key: &str,
    files_completed: i64,
    files_total: i64,
    bytes_downloaded: i64,
    total_bytes: i64,
) -> KnowledgeLocalEmbeddingDownloadProgress {
    KnowledgeLocalEmbeddingDownloadProgress {
        stage: stage.to_string(),
        model_key: model_key.to_string(),
        files_completed,
        files_total,
        bytes_downloaded,
        total_bytes,
    }
}

async fn get_json_with_retry<T>(client: &reqwest::Client, url: Url) -> Result<T, AppError>
where
    T: serde::de::DeserializeOwned,
{
    let mut last_error = None;
    for attempt in 0..DOWNLOAD_RETRY_COUNT {
        match client.get(url.clone()).send().await {
            Ok(response) if response.status().is_success() => match response.json::<T>().await {
                Ok(value) => return Ok(value),
                Err(error) => last_error = Some(error.to_string()),
            },
            Ok(response) => last_error = Some(format!("HTTP {}", response.status())),
            Err(error) => last_error = Some(error.to_string()),
        }
        if attempt + 1 < DOWNLOAD_RETRY_COUNT {
            tokio::time::sleep(Duration::from_millis(250 * (1_u64 << attempt))).await;
        }
    }
    Err(AppError::Custom(format!(
        "读取内部模型镜像清单失败（已重试 {DOWNLOAD_RETRY_COUNT} 次）：{}",
        last_error.unwrap_or_else(|| "未知错误".to_string())
    )))
}

async fn download_file_with_retry(
    client: &reqwest::Client,
    url: Url,
    destination: &Path,
) -> Result<i64, AppError> {
    let mut last_error = None;
    for attempt in 0..DOWNLOAD_RETRY_COUNT {
        let attempt_path = destination.with_extension(format!("download-{attempt}"));
        let result = async {
            let mut response = client
                .get(url.clone())
                .send()
                .await
                .map_err(|error| AppError::Custom(error.to_string()))?
                .error_for_status()
                .map_err(|error| AppError::Custom(error.to_string()))?;
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&attempt_path)?;
            let mut total = 0_i64;
            while let Some(chunk) = response
                .chunk()
                .await
                .map_err(|error| AppError::Custom(error.to_string()))?
            {
                let size = i64::try_from(chunk.len())
                    .map_err(|_| AppError::InvalidInput("模型文件大小超出范围".to_string()))?;
                total = total
                    .checked_add(size)
                    .ok_or_else(|| AppError::InvalidInput("模型文件大小超出范围".to_string()))?;
                if u64::try_from(total).unwrap_or(u64::MAX) > MAX_MODEL_CACHE_BYTES {
                    return Err(AppError::InvalidInput("模型文件超过缓存上限".to_string()));
                }
                file.write_all(&chunk)?;
            }
            file.sync_all()?;
            fs::rename(&attempt_path, destination)?;
            Ok(total)
        }
        .await;
        match result {
            Ok(size) => return Ok(size),
            Err(error) => {
                let _ = fs::remove_file(&attempt_path);
                last_error = Some(error.to_string());
            }
        }
        if attempt + 1 < DOWNLOAD_RETRY_COUNT {
            tokio::time::sleep(Duration::from_millis(250 * (1_u64 << attempt))).await;
        }
    }
    Err(AppError::Custom(format!(
        "下载内部模型镜像文件失败（已重试 {DOWNLOAD_RETRY_COUNT} 次）：{}",
        last_error.unwrap_or_else(|| "未知错误".to_string())
    )))
}

fn sha256_file(path: &Path) -> Result<String, AppError> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn validate_import_source(path: &Path) -> Result<PathBuf, AppError> {
    if !path.is_absolute() {
        return Err(AppError::InvalidInput(
            "离线模型导入路径必须是绝对目录".to_string(),
        ));
    }
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(AppError::InvalidInput(
            "离线模型导入路径必须是非符号链接目录".to_string(),
        ));
    }
    Ok(fs::canonicalize(path)?)
}

fn cleanup_incomplete_imports(cache_dir: &Path) -> Result<(), AppError> {
    for entry in fs::read_dir(cache_dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        let file_type = entry.file_type()?;
        if name.starts_with(".import-") && file_type.is_dir() && !file_type.is_symlink() {
            fs::remove_dir_all(entry.path())?;
        }
    }
    Ok(())
}

fn copy_model_directory(source: &Path, target: &Path) -> Result<u64, AppError> {
    let mut entries = fs::read_dir(source)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    let mut total = 0_u64;
    for entry in entries {
        let file_type = entry.file_type()?;
        let destination = target.join(entry.file_name());
        if file_type.is_symlink() {
            return Err(AppError::InvalidInput(
                "离线模型目录不能包含符号链接".to_string(),
            ));
        }
        if file_type.is_dir() {
            fs::create_dir(&destination)?;
            total = total.saturating_add(copy_model_directory(&entry.path(), &destination)?);
            continue;
        }
        if !file_type.is_file() {
            return Err(AppError::InvalidInput(
                "离线模型目录包含不支持的文件类型".to_string(),
            ));
        }
        total = total.saturating_add(copy_file(&entry.path(), &destination)?);
        if total > MAX_MODEL_CACHE_BYTES {
            return Err(AppError::InvalidInput(format!(
                "离线模型超过 {}GB 缓存上限",
                MAX_MODEL_CACHE_BYTES / 1024 / 1024 / 1024
            )));
        }
    }
    Ok(total)
}

fn copy_file(source: &Path, target: &Path) -> Result<u64, AppError> {
    let mut reader = fs::File::open(source)?;
    let mut writer = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(target)?;
    let mut total = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        writer.write_all(&buffer[..count])?;
        total = total.saturating_add(count as u64);
    }
    writer.sync_all()?;
    Ok(total)
}

/// 目录摘要同时包含相对路径与文件内容，避免相同字节被替换为不同模型布局后仍然通过校验。
fn directory_sha256(path: &Path) -> Result<String, AppError> {
    let mut files = Vec::new();
    collect_regular_files(path, path, &mut files)?;
    files.sort();
    let mut hasher = Sha256::new();
    for relative in files {
        if relative == Path::new(MODEL_MANIFEST_FILE) {
            continue;
        }
        let relative_text = relative.to_string_lossy();
        hasher.update(relative_text.as_bytes());
        hasher.update([0]);
        let mut file = fs::File::open(path.join(&relative))?;
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            let count = file.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            hasher.update(&buffer[..count]);
        }
        hasher.update([0]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn collect_regular_files(
    root: &Path,
    current: &Path,
    files: &mut Vec<PathBuf>,
) -> Result<(), AppError> {
    for entry in fs::read_dir(current)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if file_type.is_symlink() {
            return Err(AppError::InvalidInput(
                "本地向量化模型缓存不能包含符号链接".to_string(),
            ));
        }
        if file_type.is_dir() {
            collect_regular_files(root, &entry.path(), files)?;
        } else if file_type.is_file() {
            let entry_path = entry.path();
            let relative = entry_path
                .strip_prefix(root)
                .map_err(|_| AppError::InvalidInput("模型缓存路径越出受控目录".to_string()))?;
            files.push(relative.to_path_buf());
        } else {
            return Err(AppError::InvalidInput(
                "本地向量化模型缓存包含不支持的文件类型".to_string(),
            ));
        }
    }
    Ok(())
}

fn write_manifest(model_dir: &Path, manifest: &ModelManifest) -> Result<(), AppError> {
    let manifest_path = model_dir.join(MODEL_MANIFEST_FILE);
    let staging_path = model_dir.join(format!("{MODEL_MANIFEST_FILE}.new"));
    let content = serde_json::to_vec_pretty(manifest)?;
    {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&staging_path)?;
        file.write_all(&content)?;
        file.sync_all()?;
    }
    fs::rename(staging_path, manifest_path)?;
    Ok(())
}

fn read_manifest(model_dir: &Path) -> Result<ModelManifest, AppError> {
    let path = model_dir.join(MODEL_MANIFEST_FILE);
    let metadata = fs::symlink_metadata(&path)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(AppError::InvalidInput(
            "本地向量化模型清单不合法".to_string(),
        ));
    }
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

/// 应用数据目录本身可由平台重定向，但模型缓存根不允许是符号链接，避免状态接口枚举任意目录。
fn validate_cache_dir(app_data_dir: &Path, cache_dir: &Path) -> Result<PathBuf, AppError> {
    let cache_metadata = fs::symlink_metadata(cache_dir)?;
    if cache_metadata.file_type().is_symlink() {
        return Err(AppError::InvalidInput(
            "本地向量化模型缓存根不能是符号链接".to_string(),
        ));
    }
    if !cache_metadata.is_dir() {
        return Err(AppError::InvalidInput(
            "本地向量化模型缓存根必须是目录".to_string(),
        ));
    }
    let canonical_root = fs::canonicalize(app_data_dir)?;
    let canonical_cache = fs::canonicalize(cache_dir)?;
    if !canonical_cache.starts_with(&canonical_root) {
        return Err(AppError::InvalidInput(
            "本地向量化模型缓存越出应用数据目录".to_string(),
        ));
    }
    Ok(canonical_cache)
}

fn directory_size(path: &Path) -> Result<u64, std::io::Error> {
    let mut total = 0_u64;
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            total = total.saturating_add(directory_size(&entry.path())?);
        } else if file_type.is_file() {
            if entry.file_name() == MODEL_MANIFEST_FILE {
                continue;
            }
            total = total.saturating_add(entry.metadata()?.len());
        }
    }
    Ok(total)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use axum::{routing::get, Json, Router};
    use chrono::Utc;
    use serde_json::json;
    use sha2::{Digest, Sha256};

    use crate::models::{
        DownloadKnowledgeLocalEmbeddingModelInput, ImportKnowledgeLocalEmbeddingModelInput,
        RemoveKnowledgeLocalEmbeddingModelInput,
    };

    use super::{
        directory_sha256, ensure_consistent_dimensions, normalize_embedding_vector,
        validate_mirror_url, validated_model_key, KnowledgeLocalEmbeddingService,
    };

    #[cfg(feature = "local-embedding-fastembed")]
    #[test]
    fn local_runtime_must_stay_inside_the_model_package() -> Result<(), Box<dyn std::error::Error>>
    {
        use super::{onnx_runtime_library_name, verified_onnx_runtime_library};

        let root = std::env::temp_dir().join(format!(
            "tauri-ssh-local-embedding-runtime-{}",
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let runtime = root.join("runtime");
        fs::create_dir_all(&runtime)?;
        fs::write(runtime.join(onnx_runtime_library_name()), b"runtime")?;

        let verified = verified_onnx_runtime_library(&fs::canonicalize(&root)?)?;
        assert!(verified.starts_with(fs::canonicalize(&runtime)?));

        fs::remove_file(&verified)?;
        assert!(verified_onnx_runtime_library(&fs::canonicalize(&root)?).is_err());
        Ok(())
    }

    #[cfg(feature = "local-embedding-fastembed")]
    #[test]
    fn accepts_fastembed_standard_onnx_model_layout() -> Result<(), Box<dyn std::error::Error>> {
        use super::read_model_file;

        let root = std::env::temp_dir().join(format!(
            "tauri-ssh-local-embedding-onnx-layout-{}",
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        fs::create_dir_all(root.join("onnx"))?;
        fs::write(root.join("onnx/model.onnx"), b"model")?;

        assert_eq!(
            read_model_file(&fs::canonicalize(&root)?, "onnx/model.onnx")?,
            b"model"
        );
        Ok(())
    }

    /// 只在提供经校验的离线候选模型包时运行，防止普通测试联网下载模型。
    #[cfg(feature = "local-embedding-fastembed")]
    #[test]
    #[ignore = "requires KNOWLEDGE_LOCAL_EMBEDDING_TEST_MODEL_DIR with a verified offline model pack"]
    fn runs_real_bge_short_text_inference() -> Result<(), Box<dyn std::error::Error>> {
        use super::generate_embeddings_with_fastembed;

        let model_dir = std::env::var("KNOWLEDGE_LOCAL_EMBEDDING_TEST_MODEL_DIR")?;
        let model_dir = fs::canonicalize(model_dir)?;
        let texts = vec![
            "查询: 退款审批需求的实现方案".to_string(),
            "查询: OrderService 中的审批接口".to_string(),
        ];
        let vectors = generate_embeddings_with_fastembed(
            &model_dir,
            "bge-small-zh-v1.5",
            &texts,
            2,
            &mut || false,
        )?;
        assert_eq!(vectors.len(), texts.len());
        assert_eq!(vectors[0].len(), 512);
        assert!(vectors
            .iter()
            .flatten()
            .all(|value| value.is_finite() && value.abs() <= 1.0));
        Ok(())
    }

    /// 只在提供经校验的离线 E5 模型包时运行，验证 query/document 前缀与真实维度。
    #[cfg(feature = "local-embedding-fastembed")]
    #[test]
    #[ignore = "requires KNOWLEDGE_LOCAL_EMBEDDING_TEST_MODEL_DIR with a verified offline model pack"]
    fn runs_real_e5_short_text_inference() -> Result<(), Box<dyn std::error::Error>> {
        use super::generate_embeddings_with_fastembed;

        let model_dir = std::env::var("KNOWLEDGE_LOCAL_EMBEDDING_TEST_MODEL_DIR")?;
        let model_dir = fs::canonicalize(model_dir)?;
        let texts = vec![
            "query: 退款审批需求的具体实现方案".to_string(),
            "passage: OrderService 负责处理退款审批请求。".to_string(),
        ];
        let vectors = generate_embeddings_with_fastembed(
            &model_dir,
            "multilingual-e5-small-int8",
            &texts,
            2,
            &mut || false,
        )?;
        assert_eq!(vectors.len(), texts.len());
        assert_eq!(vectors[0].len(), 384);
        assert!(vectors
            .iter()
            .flatten()
            .all(|value| value.is_finite() && value.abs() <= 1.0));
        Ok(())
    }

    #[test]
    fn runtime_status_never_enables_automatic_download() -> Result<(), Box<dyn std::error::Error>> {
        let root = std::env::temp_dir().join(format!(
            "tauri-ssh-local-embedding-cache-{}",
            std::process::id()
        ));
        fs::create_dir_all(root.join("knowledge-models/model-a"))?;
        fs::write(
            root.join("knowledge-models/model-a/weights.onnx"),
            b"weights",
        )?;

        let status = KnowledgeLocalEmbeddingService::runtime_status(&root)?;
        assert!(!status.automatic_download_enabled);
        assert!(!status.runtime_available);
        assert!(status.cached_models.is_empty());
        Ok(())
    }

    #[test]
    fn cache_root_stays_below_app_data_dir() -> Result<(), Box<dyn std::error::Error>> {
        let root = std::env::temp_dir().join(format!(
            "tauri-ssh-local-embedding-root-{}",
            std::process::id()
        ));
        fs::create_dir_all(&root)?;
        let cache = KnowledgeLocalEmbeddingService::ensure_cache_dir(&root)?;
        assert!(cache.starts_with(fs::canonicalize(&root)?));
        Ok(())
    }

    #[test]
    fn model_key_rejects_dot_paths_before_cache_cleanup() -> Result<(), Box<dyn std::error::Error>>
    {
        for key in [".", "..", "...", "---", "___"] {
            assert!(
                validated_model_key(key).is_err(),
                "危险或无意义的模型标识不应通过校验: {key}"
            );
        }

        let root = std::env::temp_dir().join(format!(
            "tauri-ssh-local-embedding-remove-guard-{}",
            std::process::id()
        ));
        let sentinel = root.join("sentinel.txt");
        fs::create_dir_all(root.join("knowledge-models"))?;
        fs::write(&sentinel, b"must remain")?;
        for key in [".", ".."] {
            assert!(KnowledgeLocalEmbeddingService::remove_model(
                &root,
                RemoveKnowledgeLocalEmbeddingModelInput {
                    model_key: key.to_string(),
                },
            )
            .is_err());
        }
        assert!(sentinel.exists());
        assert!(root.join("knowledge-models").exists());
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn runtime_status_rejects_external_cache_root_symlink() -> Result<(), Box<dyn std::error::Error>>
    {
        use std::os::unix::fs::symlink;

        let root = std::env::temp_dir().join(format!(
            "tauri-ssh-local-embedding-symlink-{}",
            std::process::id()
        ));
        let external = std::env::temp_dir().join(format!(
            "tauri-ssh-local-embedding-external-{}",
            std::process::id()
        ));
        fs::create_dir_all(&root)?;
        fs::create_dir_all(&external)?;
        symlink(&external, root.join("knowledge-models"))?;

        assert!(KnowledgeLocalEmbeddingService::runtime_status(&root).is_err());
        Ok(())
    }

    #[test]
    fn import_model_verifies_hash_and_exposes_only_verified_cache(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let root = std::env::temp_dir().join(format!(
            "tauri-ssh-local-embedding-import-{}",
            std::process::id()
        ));
        let source = std::env::temp_dir().join(format!(
            "tauri-ssh-local-embedding-source-{}",
            std::process::id()
        ));
        fs::create_dir_all(source.join("nested"))?;
        fs::write(source.join("config.json"), b"{}")?;
        fs::write(source.join("nested/weights.onnx"), b"weights")?;
        let expected_sha256 = directory_sha256(&source)?;

        let imported = KnowledgeLocalEmbeddingService::import_model(
            &root,
            ImportKnowledgeLocalEmbeddingModelInput {
                model_key: "multilingual-e5-small".to_string(),
                source_path: source.to_string_lossy().to_string(),
                expected_sha256: expected_sha256.clone(),
            },
        )?;
        assert_eq!(imported.sha256, expected_sha256);
        assert_eq!(imported.size_bytes, 9);
        let status = KnowledgeLocalEmbeddingService::runtime_status(&root)?;
        assert_eq!(status.cached_models.len(), 1);
        assert_eq!(status.cached_models[0].sha256, expected_sha256);

        fs::write(
            root.join("knowledge-models/multilingual-e5-small/nested/weights.onnx"),
            b"changed",
        )?;
        assert!(KnowledgeLocalEmbeddingService::runtime_status(&root)?
            .cached_models
            .is_empty());
        KnowledgeLocalEmbeddingService::remove_model(
            &root,
            RemoveKnowledgeLocalEmbeddingModelInput {
                model_key: "multilingual-e5-small".to_string(),
            },
        )?;
        assert!(!root.join("knowledge-models/multilingual-e5-small").exists());
        Ok(())
    }

    #[test]
    fn failed_import_cleans_staging_cache() -> Result<(), Box<dyn std::error::Error>> {
        let root = std::env::temp_dir().join(format!(
            "tauri-ssh-local-embedding-failed-import-{}",
            std::process::id()
        ));
        let source = std::env::temp_dir().join(format!(
            "tauri-ssh-local-embedding-failed-source-{}",
            std::process::id()
        ));
        fs::create_dir_all(&source)?;
        fs::write(source.join("weights.onnx"), b"weights")?;
        assert!(KnowledgeLocalEmbeddingService::import_model(
            &root,
            ImportKnowledgeLocalEmbeddingModelInput {
                model_key: "invalid-checksum".to_string(),
                source_path: source.to_string_lossy().to_string(),
                expected_sha256: "0".repeat(64),
            },
        )
        .is_err());
        let cache = root.join("knowledge-models");
        for entry in fs::read_dir(cache)? {
            assert!(!entry?.file_name().to_string_lossy().starts_with(".import-"));
        }
        Ok(())
    }

    #[test]
    fn mirror_url_rejects_credentials_and_non_local_http() {
        assert!(validate_mirror_url("https://models.example.test/internal").is_ok());
        assert!(validate_mirror_url("http://models.example.test/internal").is_err());
        assert!(validate_mirror_url("https://user:secret@models.example.test/").is_err());
        assert!(validate_mirror_url("http://127.0.0.1:18080/").is_ok());
    }

    #[test]
    fn normalizes_vectors_and_rejects_dimension_mismatch() -> Result<(), Box<dyn std::error::Error>>
    {
        let vector = normalize_embedding_vector(vec![3.0, 4.0])?;
        assert!((vector[0] - 0.6).abs() < 0.000_01);
        assert!((vector[1] - 0.8).abs() < 0.000_01);
        assert!(normalize_embedding_vector(vec![0.0, 0.0]).is_err());
        assert!(ensure_consistent_dimensions(&[vec![1.0], vec![1.0, 2.0]]).is_err());
        Ok(())
    }

    #[tokio::test]
    async fn downloads_verified_model_from_same_origin_mirror(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let source = std::env::temp_dir().join(format!(
            "tauri-ssh-local-embedding-mirror-source-{}",
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        fs::create_dir_all(&source)?;
        let weights = b"weights-from-mirror".to_vec();
        fs::write(source.join("model.onnx"), &weights)?;
        let directory_hash = directory_sha256(&source)?;
        let file_hash = format!("{:x}", Sha256::digest(&weights));
        let manifest = json!({
            "modelKey": "mirror-model",
            "sha256": directory_hash,
            "files": [{
                "path": "model.onnx",
                "sha256": file_hash,
                "sizeBytes": weights.len(),
                "url": "models/mirror-model/model.onnx"
            }]
        });
        let app = Router::new()
            .route(
                "/models/mirror-model/manifest.json",
                get({
                    let manifest = manifest.clone();
                    move || {
                        let manifest = manifest.clone();
                        async move { Json(manifest) }
                    }
                }),
            )
            .route(
                "/models/mirror-model/model.onnx",
                get({
                    let weights = weights.clone();
                    move || {
                        let weights = weights.clone();
                        async move { weights }
                    }
                }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?;
        let server = tokio::spawn(async move { axum::serve(listener, app).await });
        let root = std::env::temp_dir().join(format!(
            "tauri-ssh-local-embedding-mirror-root-{}",
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let mut progress = Vec::new();
        let result = KnowledgeLocalEmbeddingService::download_model_from_mirror(
            &root,
            &format!("http://{address}/"),
            DownloadKnowledgeLocalEmbeddingModelInput {
                model_key: "mirror-model".to_string(),
            },
            |event| progress.push(event),
        )
        .await?;
        server.abort();
        assert_eq!(result.size_bytes, i64::try_from(weights.len())?);
        assert_eq!(
            fs::read(root.join("knowledge-models/mirror-model/model.onnx"))?,
            weights
        );
        assert_eq!(
            progress.last().map(|event| event.stage.as_str()),
            Some("completed")
        );
        assert_eq!(
            KnowledgeLocalEmbeddingService::runtime_status(&root)?
                .cached_models
                .len(),
            1
        );
        Ok(())
    }
}
