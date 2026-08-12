use std::{
    collections::BTreeSet,
    sync::LazyLock,
    time::{Duration, Instant},
};

use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use base64::{engine::general_purpose, Engine as _};
use rand::RngCore;
use reqwest::{
    header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE, RETRY_AFTER},
    RequestBuilder, Response,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio::{sync::Semaphore, time::sleep};

use crate::database::Database;
use crate::error::AppError;
use crate::models::{
    AiProvider, AiProviderAskInput, AiProviderAskResult, AiProviderEmbeddingInput,
    AiProviderEmbeddingResult, AiProviderModelListInput, AiProviderModelListResult,
    AiProviderRoute, AiProviderTestResult, UpsertAiProviderInput, UpsertAiProviderRouteInput,
};
use crate::services::ai_skill::AiSkillService;

const SECRET_SEED_KEY: &str = "ai_provider_secret_seed";
const REMOTE_EMBEDDING_TIMEOUT: Duration = Duration::from_secs(30);
/// 版本需求分析会携带较长的证据上下文并生成带引用的表格回答。原 60 秒总超时会在
/// Provider 已开始返回正文时主动中断读取，因此聊天请求使用独立的长回答超时。
const REMOTE_CHAT_TIMEOUT: Duration = Duration::from_secs(180);
const REMOTE_CHAT_CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const REMOTE_EMBEDDING_MAX_INPUTS: usize = 32;
const REMOTE_EMBEDDING_MAX_CHARACTERS: usize = 100_000;
const REMOTE_EMBEDDING_MAX_ATTEMPTS: u32 = 3;
const REMOTE_EMBEDDING_MAX_RETRY_WAIT: Duration = Duration::from_secs(30);
static REMOTE_EMBEDDING_SEMAPHORE: LazyLock<Semaphore> = LazyLock::new(|| Semaphore::new(2));
const REMOTE_OCR_TIMEOUT: Duration = Duration::from_secs(90);
const REMOTE_OCR_MAX_IMAGE_BYTES: usize = 10 * 1024 * 1024;
const REMOTE_OCR_MAX_RESPONSE_CHARACTERS: usize = 60_000;
static REMOTE_OCR_SEMAPHORE: LazyLock<Semaphore> = LazyLock::new(|| Semaphore::new(1));

pub struct AiProviderService;

const DEFAULT_AI_RESPONSE_LANGUAGE_PROMPT: &str = "全应用默认使用简体中文回答。除非用户明确要求其他语言，所有自然语言说明、分析、建议、摘要和生成内容都必须使用中文；代码、命令、SQL、配置键、JSON 字段名、Git 类型标识等固定格式内容保持原格式。";

struct EmbeddingRequestMetrics {
    attempts: i64,
    retry_wait_ms: i64,
    rate_limited: bool,
}

/// 仅供知识导入任务使用的视觉识别结果。它不属于 IPC 返回值，确保图像正文和 API Key
/// 不会经由前端或审计输出扩散。
pub(crate) struct AiProviderImageOcrResult {
    pub provider_key: String,
    pub model: String,
    pub text: String,
}

impl AiProviderService {
    pub fn list(db: &Database) -> Result<Vec<AiProvider>, AppError> {
        db.list_ai_providers()
    }

    pub fn upsert(db: &Database, mut input: UpsertAiProviderInput) -> Result<AiProvider, AppError> {
        input.default_model = input.default_model.trim().to_string();
        input.embedding_model = input.embedding_model.trim().to_string();
        Self::normalize_provider_capabilities(&mut input);
        if !Self::requires_api_key(&input.protocol, &input.auth_type) {
            // 切换到无认证端点时必须同时清除历史密钥，避免后续误发给新的服务地址。
            input.api_key = None;
            input.clear_api_key = Some(true);
        }
        Self::validate_provider(&input)?;
        let encrypted_secret = match input
            .api_key
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
        {
            Some(secret) => {
                let (nonce, ciphertext) = Self::encrypt_secret(db, secret)?;
                Some((nonce, ciphertext))
            }
            None => None,
        };
        let encrypted_ref = encrypted_secret
            .as_ref()
            .map(|(nonce, ciphertext)| (nonce.as_str(), ciphertext.as_str()));
        let clear_api_key = input.clear_api_key.unwrap_or(false);
        db.upsert_ai_provider(&input, encrypted_ref, clear_api_key)
    }

    pub fn delete(db: &Database, key: &str) -> Result<(), AppError> {
        if key.trim().is_empty() {
            return Err(AppError::InvalidInput("Provider key 不能为空".into()));
        }
        if !db.delete_ai_provider(key)? {
            return Err(AppError::NotFound(format!("AI Provider '{}' 不存在", key)));
        }
        Ok(())
    }

    pub fn list_routes(db: &Database) -> Result<Vec<AiProviderRoute>, AppError> {
        db.list_ai_provider_routes()
    }

    pub fn upsert_route(
        db: &Database,
        input: UpsertAiProviderRouteInput,
    ) -> Result<AiProviderRoute, AppError> {
        if input.scenario.trim().is_empty() {
            return Err(AppError::InvalidInput("场景不能为空".into()));
        }
        for key in [&input.primary_provider_key, &input.fallback_provider_key] {
            if db.get_ai_provider(key)?.is_none() {
                return Err(AppError::InvalidInput(format!("Provider '{}' 不存在", key)));
            }
        }
        db.upsert_ai_provider_route(&input)
    }

    pub async fn test(db: &Database, key: &str) -> Result<AiProviderTestResult, AppError> {
        let row = db
            .get_ai_provider_secret_row(key)?
            .ok_or_else(|| AppError::NotFound(format!("AI Provider '{}' 不存在", key)))?;
        if !row.provider.enabled {
            return Err(AppError::InvalidInput("Provider 已禁用".into()));
        }

        let api_key = if Self::requires_api_key(&row.provider.protocol, &row.provider.auth_type) {
            match (&row.secret_nonce, &row.secret_ciphertext) {
                (Some(nonce), Some(ciphertext)) => {
                    Some(Self::decrypt_secret(db, nonce, ciphertext)?)
                }
                _ => None,
            }
        } else {
            None
        };

        let started = Instant::now();
        let supports_chat = Self::supports_chat(&row.provider);
        let supports_embedding = Self::supports_embedding(&row.provider);
        let response = async {
            let mut messages = Vec::new();
            let mut status_code = 200;
            if supports_chat {
                let (status, message) =
                    Self::perform_test_request(&row.provider, api_key.as_deref()).await?;
                status_code = status;
                messages.push(message);
            }
            if supports_embedding {
                let model = row.provider.embedding_model.trim();
                let inputs = vec!["query: Tauri SSH Provider 连通性测试".to_string()];
                let (vectors, _) = Self::perform_embedding_request(
                    &row.provider,
                    api_key.as_deref(),
                    model,
                    &inputs,
                    None,
                )
                .await?;
                let dimension = vectors.first().map(Vec::len).unwrap_or_default();
                if dimension == 0 {
                    return Err(AppError::Custom("Embedding 接口未返回有效向量".into()));
                }
                messages.push(format!("Embedding 接口测试成功（{dimension} 维）"));
            }
            Ok((status_code, messages.join("；")))
        }
        .await;
        let latency_ms = started.elapsed().as_millis() as i64;
        let tested_model = if supports_chat {
            row.provider.default_model.clone()
        } else {
            row.provider.embedding_model.clone()
        };

        match response {
            Ok((status_code, message)) => {
                db.update_ai_provider_latency(&row.provider.key, latency_ms, "configured")?;
                Ok(AiProviderTestResult {
                    ok: true,
                    provider_key: row.provider.key,
                    provider_name: row.provider.name,
                    model: tested_model.clone(),
                    endpoint: row.provider.endpoint,
                    latency_ms,
                    status_code: Some(status_code),
                    message,
                })
            }
            Err(err) => Ok(AiProviderTestResult {
                ok: false,
                provider_key: row.provider.key,
                provider_name: row.provider.name,
                model: tested_model,
                endpoint: row.provider.endpoint,
                latency_ms,
                status_code: None,
                message: err.to_string(),
            }),
        }
    }

    pub async fn list_models(
        db: &Database,
        input: AiProviderModelListInput,
    ) -> Result<AiProviderModelListResult, AppError> {
        if input.key.trim().is_empty() {
            return Err(AppError::InvalidInput("Provider key 不能为空".into()));
        }
        if input.endpoint.trim().is_empty() {
            return Err(AppError::InvalidInput("Base URL 不能为空".into()));
        }

        let stored_secret = if Self::requires_api_key(&input.protocol, &input.auth_type) {
            match input
                .api_key
                .as_deref()
                .map(str::trim)
                .filter(|v| !v.is_empty())
            {
                Some(value) => Some(value.to_string()),
                None => db.get_ai_provider_secret_row(&input.key)?.and_then(|row| {
                    match (row.secret_nonce, row.secret_ciphertext) {
                        (Some(nonce), Some(ciphertext)) => {
                            Self::decrypt_secret(db, &nonce, &ciphertext).ok()
                        }
                        _ => None,
                    }
                }),
            }
        } else {
            None
        };

        let models = Self::perform_model_list_request(&input, stored_secret.as_deref()).await?;
        Ok(AiProviderModelListResult {
            provider_key: input.key,
            models,
            source: "provider_api".into(),
        })
    }

    /// 使用 Provider 独立的 Embedding 模型生成向量；不读取或修改聊天默认模型。
    #[allow(dead_code)]
    pub async fn embed(
        db: &Database,
        input: AiProviderEmbeddingInput,
    ) -> Result<AiProviderEmbeddingResult, AppError> {
        Self::embed_with_preflight(db, input, || Ok(())).await
    }

    /// 在全局并发调度器取得 permit 后、实际发送正文前执行最后一道调用方策略校验。
    ///
    /// 远程 Embedding 的来源授权由知识库层决定；若只在排队前校验，用户在等待期间
    /// 关闭授权时，已排队正文仍可能被发送。因此调用方可借此钩子在发送临界点重新校验。
    pub async fn embed_with_preflight<F>(
        db: &Database,
        input: AiProviderEmbeddingInput,
        preflight: F,
    ) -> Result<AiProviderEmbeddingResult, AppError>
    where
        F: FnOnce() -> Result<(), AppError>,
    {
        let provider_key = input.provider_key.trim();
        if provider_key.is_empty() {
            return Err(AppError::InvalidInput(
                "Embedding Provider key 不能为空".into(),
            ));
        }
        if input.inputs.is_empty() {
            return Err(AppError::InvalidInput("Embedding 输入不能为空".into()));
        }
        if input.inputs.len() > REMOTE_EMBEDDING_MAX_INPUTS {
            return Err(AppError::InvalidInput(format!(
                "单次 Embedding 最多允许 {} 条输入",
                REMOTE_EMBEDDING_MAX_INPUTS
            )));
        }
        if input.inputs.iter().any(|value| value.trim().is_empty()) {
            return Err(AppError::InvalidInput(
                "Embedding 输入不能包含空文本".into(),
            ));
        }
        let input_characters = input
            .inputs
            .iter()
            .map(|value| value.chars().count())
            .sum::<usize>();
        if input_characters > REMOTE_EMBEDDING_MAX_CHARACTERS {
            return Err(AppError::InvalidInput(format!(
                "单次 Embedding 文本不能超过 {} 个字符",
                REMOTE_EMBEDDING_MAX_CHARACTERS
            )));
        }
        if let Some(dimensions) = input.dimensions {
            if dimensions <= 0 {
                return Err(AppError::InvalidInput("Embedding 维度必须为正整数".into()));
            }
        }

        let row = db
            .get_ai_provider_secret_row(provider_key)?
            .ok_or_else(|| AppError::NotFound(format!("AI Provider '{provider_key}' 不存在")))?;
        if !row.provider.enabled || row.provider.status != "configured" {
            return Err(AppError::InvalidInput(
                "Embedding Provider 尚未配置完成或已禁用".into(),
            ));
        }
        if !Self::supports_embedding(&row.provider) {
            return Err(AppError::InvalidInput(
                "该 Provider 未启用 Embedding 能力".into(),
            ));
        }
        let model = input
            .model
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(row.provider.embedding_model.trim())
            .to_string();
        if model.is_empty() {
            return Err(AppError::InvalidInput(
                "请为 Provider 配置独立 Embedding 模型".into(),
            ));
        }
        let api_key = if Self::requires_api_key(&row.provider.protocol, &row.provider.auth_type) {
            match (&row.secret_nonce, &row.secret_ciphertext) {
                (Some(nonce), Some(ciphertext)) => {
                    Some(Self::decrypt_secret(db, nonce, ciphertext)?)
                }
                _ => None,
            }
        } else {
            None
        };
        let started = Instant::now();
        // 远程请求按全局并发上限串接，避免多个知识任务同时压垮 Provider。
        let _permit = REMOTE_EMBEDDING_SEMAPHORE
            .acquire()
            .await
            .map_err(|_| AppError::Custom("Embedding 请求调度器不可用".into()))?;
        preflight()?;
        let (vectors, metrics) = Self::perform_embedding_request(
            &row.provider,
            api_key.as_deref(),
            &model,
            &input.inputs,
            input.dimensions,
        )
        .await?;
        let dimension = i64::try_from(vectors[0].len())
            .map_err(|_| AppError::InvalidInput("Embedding 维度超出范围".into()))?;
        Ok(AiProviderEmbeddingResult {
            provider_key: row.provider.key,
            provider_name: row.provider.name,
            model,
            dimension,
            vectors,
            latency_ms: started.elapsed().as_millis() as i64,
            input_count: i64::try_from(input.inputs.len())
                .map_err(|_| AppError::InvalidInput("Embedding 输入数量超出范围".into()))?,
            input_characters: i64::try_from(input_characters)
                .map_err(|_| AppError::InvalidInput("Embedding 字符数量超出范围".into()))?,
            attempts: metrics.attempts,
            retry_wait_ms: metrics.retry_wait_ms,
            rate_limited: metrics.rate_limited,
        })
    }

    pub async fn ask(
        db: &Database,
        input: AiProviderAskInput,
    ) -> Result<AiProviderAskResult, AppError> {
        let prompt = input.prompt.trim();
        if prompt.is_empty() {
            return Err(AppError::InvalidInput("AI 问题不能为空".into()));
        }

        let row = Self::resolve_chat_provider(db, input.provider_key.as_deref())?;
        if !row.provider.enabled {
            return Err(AppError::InvalidInput("Provider 已禁用".into()));
        }
        if row.provider.status != "configured" {
            return Err(AppError::InvalidInput("Provider 尚未配置完成".into()));
        }

        let api_key = if Self::requires_api_key(&row.provider.protocol, &row.provider.auth_type) {
            match (&row.secret_nonce, &row.secret_ciphertext) {
                (Some(nonce), Some(ciphertext)) => {
                    Some(Self::decrypt_secret(db, nonce, ciphertext)?)
                }
                _ => None,
            }
        } else {
            None
        };
        let started = Instant::now();
        let system_prompt = Self::build_system_prompt_with_skills(
            db,
            input.system_prompt.as_deref(),
            input.skill_scope.as_deref(),
            input.use_skill_trigger.unwrap_or(true),
            prompt,
        )?;

        let answer = Self::perform_chat_request(
            &row.provider,
            api_key.as_deref(),
            system_prompt.as_deref(),
            prompt,
        )
        .await?;

        Ok(AiProviderAskResult {
            provider_key: row.provider.key,
            provider_name: row.provider.name,
            model: row.provider.default_model,
            answer,
            latency_ms: started.elapsed().as_millis() as i64,
        })
    }

    /// 对已审核的受控图片执行一次远程 OCR。调用方必须在发送前取得用户明确同意，且
    /// 只能传入从内容寻址资产读取的字节，不能把任意前端文件或 URL 交给 Provider。
    pub(crate) async fn recognize_image(
        db: &Database,
        provider_key: &str,
        mime_type: &str,
        image_bytes: &[u8],
    ) -> Result<AiProviderImageOcrResult, AppError> {
        let provider_key = provider_key.trim();
        if provider_key.is_empty() {
            return Err(AppError::InvalidInput(
                "图片 OCR 必须指定视觉识别服务".into(),
            ));
        }
        if image_bytes.is_empty() || image_bytes.len() > REMOTE_OCR_MAX_IMAGE_BYTES {
            return Err(AppError::InvalidInput(format!(
                "远程 OCR 图片大小必须在 1 字节至 {}MB 之间",
                REMOTE_OCR_MAX_IMAGE_BYTES / 1024 / 1024
            )));
        }
        if !matches!(
            mime_type.trim().to_ascii_lowercase().as_str(),
            "image/png" | "image/jpeg" | "image/webp" | "image/gif"
        ) {
            return Err(AppError::InvalidInput(
                "远程 OCR 仅支持 PNG、JPEG、WebP 或 GIF 图片；SVG 不会发送到远程服务".into(),
            ));
        }

        let row = db
            .get_ai_provider_secret_row(provider_key)?
            .ok_or_else(|| AppError::NotFound(format!("AI Provider '{provider_key}' 不存在")))?;
        if !row.provider.enabled || row.provider.status != "configured" {
            return Err(AppError::InvalidInput(
                "视觉识别服务尚未配置完成或已禁用".into(),
            ));
        }
        if !Self::supports_vision(&row.provider) {
            return Err(AppError::InvalidInput("所选服务未声明视觉识别能力".into()));
        }
        if row.provider.default_model.trim().is_empty() {
            return Err(AppError::InvalidInput(
                "视觉识别服务必须配置默认模型".into(),
            ));
        }
        if !row
            .provider
            .protocol
            .to_ascii_lowercase()
            .contains("openai")
        {
            return Err(AppError::InvalidInput(
                "当前视觉 OCR 仅支持 OpenAI-compatible Chat Completions 服务".into(),
            ));
        }
        let api_key = if Self::requires_api_key(&row.provider.protocol, &row.provider.auth_type) {
            match (&row.secret_nonce, &row.secret_ciphertext) {
                (Some(nonce), Some(ciphertext)) => {
                    Some(Self::decrypt_secret(db, nonce, ciphertext)?)
                }
                _ => return Err(AppError::InvalidInput("视觉识别服务缺少 API Key".into())),
            }
        } else {
            None
        };

        let _permit = REMOTE_OCR_SEMAPHORE
            .acquire()
            .await
            .map_err(|_| AppError::Custom("远程 OCR 请求调度器不可用".into()))?;
        let text = Self::perform_vision_ocr_request(
            &row.provider,
            api_key.as_deref(),
            mime_type,
            image_bytes,
        )
        .await?;
        if text.chars().count() > REMOTE_OCR_MAX_RESPONSE_CHARACTERS {
            return Err(AppError::Custom(format!(
                "远程 OCR 返回文本超过 {} 个字符限制",
                REMOTE_OCR_MAX_RESPONSE_CHARACTERS
            )));
        }
        Ok(AiProviderImageOcrResult {
            provider_key: row.provider.key,
            model: row.provider.default_model,
            text,
        })
    }

    fn build_system_prompt_with_skills(
        db: &Database,
        system_prompt: Option<&str>,
        skill_scope: Option<&str>,
        use_skill_trigger: bool,
        prompt: &str,
    ) -> Result<Option<String>, AppError> {
        let skill_fragment = match skill_scope.map(str::trim).filter(|value| !value.is_empty()) {
            Some(scope) if use_skill_trigger => {
                AiSkillService::build_prompt_for_ai(db, scope, prompt)?
            }
            _ => String::new(),
        };
        let base = system_prompt.map(str::trim).unwrap_or("");
        let mut parts = vec![DEFAULT_AI_RESPONSE_LANGUAGE_PROMPT.to_string()];
        if !base.is_empty() {
            parts.push(base.to_string());
        }
        if !skill_fragment.trim().is_empty() {
            parts.push(skill_fragment);
        }
        Ok(Some(parts.join("\n\n")))
    }

    fn validate_provider(input: &UpsertAiProviderInput) -> Result<(), AppError> {
        if input.key.trim().is_empty() {
            return Err(AppError::InvalidInput("Provider key 不能为空".into()));
        }
        if input.name.trim().is_empty() {
            return Err(AppError::InvalidInput("Provider 名称不能为空".into()));
        }
        if input.endpoint.trim().is_empty() {
            return Err(AppError::InvalidInput("Base URL 不能为空".into()));
        }
        if !["global", "china", "gateway", "local"].contains(&input.region.as_str()) {
            return Err(AppError::InvalidInput("Provider 区域无效".into()));
        }
        let supports_chat = Self::input_supports_capability(input, "chat");
        let supports_embedding = Self::input_supports_capability(input, "embedding");
        if !supports_chat && !supports_embedding {
            return Err(AppError::InvalidInput(
                "Provider 至少需要聊天或 Embedding 能力".into(),
            ));
        }
        if supports_chat && input.default_model.is_empty() {
            return Err(AppError::InvalidInput(
                "聊天 Provider 必须配置默认模型".into(),
            ));
        }
        if supports_embedding && input.embedding_model.is_empty() {
            return Err(AppError::InvalidInput(
                "Embedding Provider 必须配置 Embedding 模型".into(),
            ));
        }
        Ok(())
    }

    fn normalize_provider_capabilities(input: &mut UpsertAiProviderInput) {
        let mut normalized = Vec::new();
        for capability in &input.capabilities {
            let value = capability.trim();
            if value.is_empty()
                || normalized
                    .iter()
                    .any(|existing: &String| existing.eq_ignore_ascii_case(value))
            {
                continue;
            }
            normalized.push(value.to_string());
        }
        if !input.default_model.is_empty()
            && !normalized
                .iter()
                .any(|capability| capability.eq_ignore_ascii_case("chat"))
        {
            normalized.push("chat".to_string());
        }
        if !input.embedding_model.is_empty()
            && !normalized
                .iter()
                .any(|capability| capability.eq_ignore_ascii_case("embedding"))
        {
            normalized.push("embedding".to_string());
        }
        input.capabilities = normalized;
    }

    fn input_supports_capability(input: &UpsertAiProviderInput, expected: &str) -> bool {
        input
            .capabilities
            .iter()
            .any(|capability| capability.eq_ignore_ascii_case(expected))
    }

    fn has_explicit_provider_capabilities(provider: &AiProvider) -> bool {
        provider.capabilities.iter().any(|capability| {
            capability.eq_ignore_ascii_case("chat") || capability.eq_ignore_ascii_case("embedding")
        })
    }

    fn supports_chat(provider: &AiProvider) -> bool {
        if Self::has_explicit_provider_capabilities(provider) {
            return provider
                .capabilities
                .iter()
                .any(|capability| capability.eq_ignore_ascii_case("chat"));
        }
        !provider.default_model.trim().is_empty()
    }

    fn supports_embedding(provider: &AiProvider) -> bool {
        if Self::has_explicit_provider_capabilities(provider) {
            return provider
                .capabilities
                .iter()
                .any(|capability| capability.eq_ignore_ascii_case("embedding"));
        }
        !provider.embedding_model.trim().is_empty()
    }

    fn supports_vision(provider: &AiProvider) -> bool {
        provider
            .capabilities
            .iter()
            .any(|capability| capability.eq_ignore_ascii_case("vision"))
    }

    fn requires_api_key(protocol: &str, auth_type: &str) -> bool {
        let protocol = protocol.trim().to_lowercase();
        let auth_type = auth_type.trim().to_lowercase();
        if protocol.contains("local") {
            return false;
        }
        !matches!(
            auth_type.as_str(),
            "" | "none" | "no auth" | "no authentication" | "无认证"
        )
    }

    fn apply_openai_auth(request: RequestBuilder, auth_type: &str, key: &str) -> RequestBuilder {
        match auth_type.trim().to_lowercase().as_str() {
            "x-api-key" => request.header("x-api-key", key),
            "api key" | "api-key" | "apikey" => request.header("api-key", key),
            _ => request.header(AUTHORIZATION, format!("Bearer {key}")),
        }
    }

    fn resolve_chat_provider(
        db: &Database,
        provider_key: Option<&str>,
    ) -> Result<crate::database::AiProviderSecretRow, AppError> {
        if let Some(key) = provider_key
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            let row = db
                .get_ai_provider_secret_row(key)?
                .ok_or_else(|| AppError::NotFound(format!("AI Provider '{}' 不存在", key)))?;
            if !Self::supports_chat(&row.provider) {
                return Err(AppError::InvalidInput(format!(
                    "AI Provider '{}' 仅支持 Embedding，不能用于聊天",
                    key
                )));
            }
            return Ok(row);
        }

        let providers = db.list_ai_providers()?;
        let provider = providers
            .into_iter()
            .find(|item| item.enabled && item.status == "configured" && Self::supports_chat(item))
            .ok_or_else(|| AppError::InvalidInput("请先配置并测试一个可用的 AI Provider".into()))?;

        db.get_ai_provider_secret_row(&provider.key)?
            .ok_or_else(|| AppError::NotFound(format!("AI Provider '{}' 不存在", provider.key)))
    }

    async fn perform_test_request(
        provider: &AiProvider,
        api_key: Option<&str>,
    ) -> Result<(u16, String), AppError> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| AppError::Custom(format!("HTTP 客户端初始化失败: {}", e)))?;

        let endpoint = provider.endpoint.trim_end_matches('/');
        let protocol = provider.protocol.to_lowercase();

        if protocol.contains("ollama") || endpoint.contains(":11434") {
            let url = format!("{}/api/tags", endpoint);
            let response = client.get(url).send().await.map_err(Self::http_error)?;
            return Self::status_result(response, "Ollama 模型列表读取成功").await;
        }

        if protocol.contains("gemini") {
            let key =
                api_key.ok_or_else(|| AppError::InvalidInput("Gemini 测试需要 API Key".into()))?;
            let url = format!(
                "{}/v1beta/models/{}:generateContent",
                endpoint, provider.default_model
            );
            let response = client
                .post(url)
                .header("x-goog-api-key", key)
                .json(&json!({
                    "contents": [{ "parts": [{ "text": "ping" }] }],
                    "generationConfig": { "maxOutputTokens": 1 }
                }))
                .send()
                .await
                .map_err(Self::http_error)?;
            return Self::status_result(response, "Gemini 生成接口测试成功").await;
        }

        if protocol.contains("messages api") || provider.key == "anthropic" {
            let key =
                api_key.ok_or_else(|| AppError::InvalidInput("Claude 测试需要 API Key".into()))?;
            let response = client
                .post(format!("{}/v1/messages", endpoint))
                .headers(Self::anthropic_headers(key)?)
                .json(&json!({
                    "model": provider.default_model,
                    "max_tokens": 1,
                    "messages": [{ "role": "user", "content": "ping" }]
                }))
                .send()
                .await
                .map_err(Self::http_error)?;
            return Self::status_result(response, "Claude Messages 接口测试成功").await;
        }

        if !Self::requires_api_key(&provider.protocol, &provider.auth_type) {
            let response = client
                .get(format!("{}/models", endpoint))
                .send()
                .await
                .map_err(Self::http_error)?;
            return Self::status_result(response, "OpenAI-compatible 模型列表读取成功").await;
        }

        let key = api_key.ok_or_else(|| AppError::InvalidInput("连接测试需要 API Key".into()))?;
        let response = Self::apply_openai_auth(
            client.get(format!("{}/models", endpoint)),
            &provider.auth_type,
            key,
        )
        .send()
        .await
        .map_err(Self::http_error)?;
        Self::status_result(response, "OpenAI-compatible 模型列表读取成功").await
    }

    async fn perform_chat_request(
        provider: &AiProvider,
        api_key: Option<&str>,
        system_prompt: Option<&str>,
        prompt: &str,
    ) -> Result<String, AppError> {
        let client = reqwest::Client::builder()
            .connect_timeout(REMOTE_CHAT_CONNECT_TIMEOUT)
            .timeout(REMOTE_CHAT_TIMEOUT)
            .build()
            .map_err(|e| AppError::Custom(format!("HTTP 客户端初始化失败: {}", e)))?;
        let endpoint = provider.endpoint.trim_end_matches('/');
        let protocol = provider.protocol.to_lowercase();
        let system_text = system_prompt
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(DEFAULT_AI_RESPONSE_LANGUAGE_PROMPT);

        if protocol.contains("ollama") || endpoint.contains(":11434") {
            let response = client
                .post(format!("{}/api/chat", endpoint))
                .json(&json!({
                    "model": provider.default_model,
                    "stream": false,
                    "messages": [
                        { "role": "system", "content": system_text },
                        { "role": "user", "content": prompt }
                    ]
                }))
                .send()
                .await
                .map_err(Self::http_error)?;
            return Self::parse_chat_response(response, &["message", "content"]).await;
        }

        if protocol.contains("gemini") {
            let key =
                api_key.ok_or_else(|| AppError::InvalidInput("Gemini 问答需要 API Key".into()))?;
            let url = format!(
                "{}/v1beta/models/{}:generateContent",
                endpoint, provider.default_model
            );
            let response = client
                .post(url)
                .header("x-goog-api-key", key)
                .json(&json!({
                    "systemInstruction": { "parts": [{ "text": system_text }] },
                    "contents": [{ "role": "user", "parts": [{ "text": prompt }] }]
                }))
                .send()
                .await
                .map_err(Self::http_error)?;
            return Self::parse_gemini_chat_response(response).await;
        }

        if protocol.contains("messages api") || provider.key == "anthropic" {
            let key =
                api_key.ok_or_else(|| AppError::InvalidInput("Claude 问答需要 API Key".into()))?;
            let response = client
                .post(format!("{}/v1/messages", endpoint))
                .headers(Self::anthropic_headers(key)?)
                .json(&json!({
                    "model": provider.default_model,
                    "max_tokens": 1024,
                    "system": system_text,
                    "messages": [{ "role": "user", "content": prompt }]
                }))
                .send()
                .await
                .map_err(Self::http_error)?;
            return Self::parse_anthropic_chat_response(response).await;
        }

        if protocol.contains("minimax") {
            return Err(AppError::InvalidInput(
                "MiniMax 专有聊天协议尚未接入，请使用 OpenAI-compatible Provider 或将 MiniMax 配置为兼容接口".into(),
            ));
        }

        let mut request = client.post(format!("{}/chat/completions", endpoint));
        if Self::requires_api_key(&provider.protocol, &provider.auth_type) {
            let key =
                api_key.ok_or_else(|| AppError::InvalidInput("AI 问答需要 API Key".into()))?;
            request = Self::apply_openai_auth(request, &provider.auth_type, key);
        }
        let response = request
            .json(&json!({
                "model": provider.default_model,
                "stream": false,
                "messages": [
                    { "role": "system", "content": system_text },
                    { "role": "user", "content": prompt }
                ]
            }))
            .send()
            .await
            .map_err(Self::http_error)?;
        Self::parse_chat_response(response, &["choices", "0", "message", "content"]).await
    }

    async fn perform_vision_ocr_request(
        provider: &AiProvider,
        api_key: Option<&str>,
        mime_type: &str,
        image_bytes: &[u8],
    ) -> Result<String, AppError> {
        let client = reqwest::Client::builder()
            .timeout(REMOTE_OCR_TIMEOUT)
            .build()
            .map_err(|error| AppError::Custom(format!("HTTP 客户端初始化失败: {error}")))?;
        let endpoint = provider.endpoint.trim_end_matches('/');
        let mut request = client.post(format!("{endpoint}/chat/completions"));
        if Self::requires_api_key(&provider.protocol, &provider.auth_type) {
            let key =
                api_key.ok_or_else(|| AppError::InvalidInput("视觉 OCR 需要 API Key".into()))?;
            request = Self::apply_openai_auth(request, &provider.auth_type, key);
        }
        let image_data = general_purpose::STANDARD.encode(image_bytes);
        let response = request
            .json(&json!({
                "model": provider.default_model,
                "stream": false,
                "max_tokens": 12000,
                "messages": [{
                    "role": "user",
                    "content": [
                        {
                            "type": "text",
                            "text": "仅提取图片中清晰可见的文字，尽量保留标题、段落、表格行列和代码的阅读顺序。不要解释、补全、总结或猜测；看不清的内容请省略。"
                        },
                        {
                            "type": "image_url",
                            "image_url": { "url": format!("data:{mime_type};base64,{image_data}") }
                        }
                    ]
                }]
            }))
            .send()
            .await
            .map_err(Self::http_error)?;
        Self::parse_chat_response(response, &["choices", "0", "message", "content"]).await
    }

    #[allow(dead_code)]
    async fn perform_embedding_request(
        provider: &AiProvider,
        api_key: Option<&str>,
        model: &str,
        inputs: &[String],
        dimensions: Option<i64>,
    ) -> Result<(Vec<Vec<f32>>, EmbeddingRequestMetrics), AppError> {
        let client = reqwest::Client::builder()
            .timeout(REMOTE_EMBEDDING_TIMEOUT)
            .build()
            .map_err(|error| AppError::Custom(format!("HTTP 客户端初始化失败: {error}")))?;
        let endpoint = provider.endpoint.trim_end_matches('/');
        let protocol = provider.protocol.to_lowercase();

        if protocol.contains("ollama") || endpoint.contains(":11434") {
            let (response, metrics) = Self::post_embedding_with_retry(
                &client,
                &format!("{endpoint}/api/embed"),
                "none",
                None,
                &Self::ollama_embedding_payload(model, inputs),
            )
            .await?;
            return Ok((
                Self::parse_ollama_embedding_response(response, inputs.len()).await?,
                metrics,
            ));
        }

        if protocol.contains("gemini")
            || protocol.contains("messages api")
            || provider.key == "anthropic"
            || protocol.contains("minimax")
        {
            return Err(AppError::InvalidInput(
                "当前 Provider 协议尚未实现 Embedding；请使用 OpenAI-compatible 或 Ollama-compatible Provider"
                    .into(),
            ));
        }

        let request_api_key = if Self::requires_api_key(&provider.protocol, &provider.auth_type) {
            Some(api_key.ok_or_else(|| {
                AppError::InvalidInput("OpenAI-compatible Embedding 需要 API Key".into())
            })?)
        } else {
            None
        };
        let (response, metrics) = Self::post_embedding_with_retry(
            &client,
            &format!("{endpoint}/embeddings"),
            &provider.auth_type,
            request_api_key,
            &Self::openai_embedding_payload(model, inputs, dimensions),
        )
        .await?;
        Ok((
            Self::parse_openai_embedding_response(response, inputs.len()).await?,
            metrics,
        ))
    }

    async fn post_embedding_with_retry(
        client: &reqwest::Client,
        url: &str,
        auth_type: &str,
        api_key: Option<&str>,
        payload: &Value,
    ) -> Result<(Response, EmbeddingRequestMetrics), AppError> {
        let mut retry_wait = Duration::ZERO;
        let mut rate_limited = false;
        for attempt in 1..=REMOTE_EMBEDDING_MAX_ATTEMPTS {
            let mut request = client.post(url).json(payload);
            if let Some(api_key) = api_key {
                request = Self::apply_openai_auth(request, auth_type, api_key);
            }
            match request.send().await {
                Ok(response) if response.status().is_success() => {
                    return Ok((
                        response,
                        EmbeddingRequestMetrics {
                            attempts: i64::from(attempt),
                            retry_wait_ms: i64::try_from(retry_wait.as_millis())
                                .unwrap_or(i64::MAX),
                            rate_limited,
                        },
                    ));
                }
                Ok(response) => {
                    let status = response.status();
                    let can_retry = status.as_u16() == 429 || status.is_server_error();
                    if !can_retry || attempt == REMOTE_EMBEDDING_MAX_ATTEMPTS {
                        return Err(AppError::Custom(format!(
                            "Provider Embedding 返回 HTTP {}",
                            status.as_u16()
                        )));
                    }
                    rate_limited |= status.as_u16() == 429;
                    let wait = Self::embedding_retry_delay(attempt, response.headers());
                    retry_wait = retry_wait.saturating_add(wait);
                    sleep(wait).await;
                }
                Err(error) => {
                    let can_retry = error.is_timeout() || error.is_connect();
                    if !can_retry || attempt == REMOTE_EMBEDDING_MAX_ATTEMPTS {
                        return Err(Self::http_error(error));
                    }
                    let wait = Self::embedding_retry_delay(attempt, &HeaderMap::new());
                    retry_wait = retry_wait.saturating_add(wait);
                    sleep(wait).await;
                }
            }
        }
        Err(AppError::Custom("Embedding 重试流程异常结束".into()))
    }

    fn embedding_retry_delay(attempt: u32, headers: &HeaderMap) -> Duration {
        let retry_after = headers
            .get(RETRY_AFTER)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.trim().parse::<u64>().ok())
            .map(Duration::from_secs);
        retry_after
            .unwrap_or_else(|| {
                let multiplier = 1_u64 << attempt.saturating_sub(1).min(4);
                Duration::from_millis(250_u64.saturating_mul(multiplier))
            })
            .min(REMOTE_EMBEDDING_MAX_RETRY_WAIT)
    }

    #[allow(dead_code)]
    fn openai_embedding_payload(model: &str, inputs: &[String], dimensions: Option<i64>) -> Value {
        let mut payload = json!({
            "model": model,
            "input": inputs,
            "encoding_format": "float",
        });
        if let Some(dimensions) = dimensions {
            payload["dimensions"] = json!(dimensions);
        }
        payload
    }

    #[allow(dead_code)]
    fn ollama_embedding_payload(model: &str, inputs: &[String]) -> Value {
        json!({
            "model": model,
            "input": inputs,
        })
    }

    async fn perform_model_list_request(
        input: &AiProviderModelListInput,
        api_key: Option<&str>,
    ) -> Result<Vec<String>, AppError> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| AppError::Custom(format!("HTTP 客户端初始化失败: {}", e)))?;

        let endpoint = input.endpoint.trim_end_matches('/');
        let protocol = input.protocol.to_lowercase();

        let response = if protocol.contains("ollama") || endpoint.contains(":11434") {
            client
                .get(format!("{}/api/tags", endpoint))
                .send()
                .await
                .map_err(Self::http_error)?
        } else if protocol.contains("gemini") {
            let key = api_key
                .ok_or_else(|| AppError::InvalidInput("读取 Gemini 模型列表需要 API Key".into()))?;
            client
                .get(format!("{}/v1beta/models", endpoint))
                .header("x-goog-api-key", key)
                .send()
                .await
                .map_err(Self::http_error)?
        } else if protocol.contains("messages api") || input.key == "anthropic" {
            let key = api_key
                .ok_or_else(|| AppError::InvalidInput("读取 Claude 模型列表需要 API Key".into()))?;
            client
                .get(format!("{}/v1/models", endpoint))
                .headers(Self::anthropic_headers(key)?)
                .send()
                .await
                .map_err(Self::http_error)?
        } else if !Self::requires_api_key(&input.protocol, &input.auth_type) {
            client
                .get(format!("{}/models", endpoint))
                .send()
                .await
                .map_err(Self::http_error)?
        } else {
            let key =
                api_key.ok_or_else(|| AppError::InvalidInput("读取模型列表需要 API Key".into()))?;
            Self::apply_openai_auth(
                client.get(format!("{}/models", endpoint)),
                &input.auth_type,
                key,
            )
            .send()
            .await
            .map_err(Self::http_error)?
        };

        Self::parse_model_list_response(response).await
    }

    async fn parse_model_list_response(
        response: reqwest::Response,
    ) -> Result<Vec<String>, AppError> {
        let status = response.status();
        let content_type = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        let text = response.text().await.map_err(|_| {
            AppError::Custom("读取 Provider 响应失败，响应可能被中断，请稍后重试".into())
        })?;
        if !status.is_success() {
            return Err(AppError::Custom(format!(
                "Provider 返回 HTTP {}",
                status.as_u16()
            )));
        }

        let value = Self::parse_provider_response_value(&text, content_type.as_deref())?;
        let mut models = BTreeSet::new();
        Self::collect_model_ids(&value, &mut models);
        let result = models.into_iter().collect::<Vec<_>>();
        if result.is_empty() {
            return Err(AppError::Custom("Provider 返回中没有可用模型 ID".into()));
        }
        Ok(result)
    }

    async fn parse_chat_response(
        response: reqwest::Response,
        content_path: &[&str],
    ) -> Result<String, AppError> {
        let value = Self::response_json(response).await?;
        if let Some(answer) = value
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            return Ok(answer.to_string());
        }
        if let Some(answer) = Self::value_at_mixed_path(&value, content_path)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            return Ok(answer.to_string());
        }
        if let Some(answer) = value
            .get("output_text")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            return Ok(answer.to_string());
        }
        Err(AppError::Custom("Provider 返回中没有可读回答".into()))
    }

    async fn parse_gemini_chat_response(response: reqwest::Response) -> Result<String, AppError> {
        let value = Self::response_json(response).await?;
        let parts = value
            .get("candidates")
            .and_then(Value::as_array)
            .and_then(|items| items.first())
            .and_then(|candidate| candidate.get("content"))
            .and_then(|content| content.get("parts"))
            .and_then(Value::as_array)
            .ok_or_else(|| AppError::Custom("Gemini 返回中没有可读回答".into()))?;
        let answer = parts
            .iter()
            .filter_map(|part| part.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_string();
        if answer.is_empty() {
            return Err(AppError::Custom("Gemini 返回为空".into()));
        }
        Ok(answer)
    }

    async fn parse_anthropic_chat_response(
        response: reqwest::Response,
    ) -> Result<String, AppError> {
        let value = Self::response_json(response).await?;
        let content = value
            .get("content")
            .and_then(Value::as_array)
            .ok_or_else(|| AppError::Custom("Claude 返回中没有 content".into()))?;
        let answer = content
            .iter()
            .filter_map(|part| part.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_string();
        if answer.is_empty() {
            return Err(AppError::Custom("Claude 返回为空".into()));
        }
        Ok(answer)
    }

    #[allow(dead_code)]
    async fn parse_openai_embedding_response(
        response: reqwest::Response,
        expected_count: usize,
    ) -> Result<Vec<Vec<f32>>, AppError> {
        let value = Self::response_json(response).await?;
        Self::parse_openai_embedding_value(&value, expected_count)
    }

    #[allow(dead_code)]
    fn parse_openai_embedding_value(
        value: &Value,
        expected_count: usize,
    ) -> Result<Vec<Vec<f32>>, AppError> {
        let data = value
            .get("data")
            .and_then(Value::as_array)
            .ok_or_else(|| AppError::Custom("Embedding 返回缺少 data 数组".into()))?;
        if data.len() != expected_count {
            return Err(AppError::Custom(format!(
                "Embedding 返回数量不匹配: expected={expected_count}, actual={}",
                data.len()
            )));
        }
        let mut vectors = vec![None; expected_count];
        for item in data {
            let index = item
                .get("index")
                .and_then(Value::as_u64)
                .and_then(|value| usize::try_from(value).ok())
                .filter(|index| *index < expected_count)
                .ok_or_else(|| AppError::Custom("Embedding 返回包含无效 index".into()))?;
            if vectors[index].is_some() {
                return Err(AppError::Custom("Embedding 返回包含重复 index".into()));
            }
            vectors[index] = Some(Self::parse_embedding_vector(
                item.get("embedding"),
                "OpenAI-compatible Embedding",
            )?);
        }
        Self::validate_embedding_vectors(vectors, expected_count)
    }

    #[allow(dead_code)]
    async fn parse_ollama_embedding_response(
        response: reqwest::Response,
        expected_count: usize,
    ) -> Result<Vec<Vec<f32>>, AppError> {
        let value = Self::response_json(response).await?;
        Self::parse_ollama_embedding_value(&value, expected_count)
    }

    #[allow(dead_code)]
    fn parse_ollama_embedding_value(
        value: &Value,
        expected_count: usize,
    ) -> Result<Vec<Vec<f32>>, AppError> {
        let raw_vectors = value
            .get("embeddings")
            .and_then(Value::as_array)
            .cloned()
            .or_else(|| {
                if expected_count == 1 {
                    value.get("embedding").map(|vector| vec![vector.clone()])
                } else {
                    None
                }
            })
            .ok_or_else(|| AppError::Custom("Ollama Embedding 返回缺少 embeddings 数组".into()))?;
        if raw_vectors.len() != expected_count {
            return Err(AppError::Custom(format!(
                "Ollama Embedding 返回数量不匹配: expected={expected_count}, actual={}",
                raw_vectors.len()
            )));
        }
        let vectors = raw_vectors
            .iter()
            .map(|vector| Self::parse_embedding_vector(Some(vector), "Ollama Embedding"))
            .collect::<Result<Vec<_>, _>>()?;
        Self::validate_embedding_vectors(vectors.into_iter().map(Some).collect(), expected_count)
    }

    #[allow(dead_code)]
    fn parse_embedding_vector(
        value: Option<&Value>,
        provider_name: &str,
    ) -> Result<Vec<f32>, AppError> {
        let values = value
            .and_then(Value::as_array)
            .ok_or_else(|| AppError::Custom(format!("{provider_name} 返回的向量不是数组")))?;
        if values.is_empty() {
            return Err(AppError::Custom(format!("{provider_name} 返回了空向量")));
        }
        values
            .iter()
            .map(|item| {
                let value = item.as_f64().ok_or_else(|| {
                    AppError::Custom(format!("{provider_name} 返回的向量包含非数值"))
                })?;
                if !value.is_finite() {
                    return Err(AppError::Custom(format!(
                        "{provider_name} 返回的向量包含非有限数"
                    )));
                }
                let value = value as f32;
                if !value.is_finite() {
                    return Err(AppError::Custom(format!(
                        "{provider_name} 返回的向量超出 f32 范围"
                    )));
                }
                Ok(value)
            })
            .collect()
    }

    #[allow(dead_code)]
    fn validate_embedding_vectors(
        vectors: Vec<Option<Vec<f32>>>,
        expected_count: usize,
    ) -> Result<Vec<Vec<f32>>, AppError> {
        if vectors.len() != expected_count {
            return Err(AppError::Custom("Embedding 返回数量不匹配".into()));
        }
        let vectors = vectors
            .into_iter()
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| AppError::Custom("Embedding 返回缺少 index".into()))?;
        let dimension = vectors
            .first()
            .map(Vec::len)
            .ok_or_else(|| AppError::Custom("Embedding 返回为空".into()))?;
        if vectors.iter().any(|vector| vector.len() != dimension) {
            return Err(AppError::Custom("Embedding 返回的向量维度不一致".into()));
        }
        Ok(vectors)
    }

    async fn response_json(response: reqwest::Response) -> Result<Value, AppError> {
        let status = response.status();
        let content_length = response.content_length();
        let content_type = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        let text = response.text().await.map_err(|error| {
            // 只记录协议元数据和错误分类，不记录 URL、请求正文或远端响应正文，避免
            // Provider 凭据、用户问题及知识证据进入日志。
            log::warn!(
                "Provider 响应正文读取中断: status={}, content_type={}, content_length={:?}, timeout={}, body={}, decode={}",
                status.as_u16(),
                content_type.as_deref().unwrap_or("unknown"),
                content_length,
                error.is_timeout(),
                error.is_body(),
                error.is_decode(),
            );
            AppError::ProviderTransient(if error.is_timeout() {
                "Provider 回答超时，问题和证据已保留，请重试回答".into()
            } else {
                "Provider 响应传输中断，问题和证据已保留，请重试回答".into()
            })
        })?;
        if !status.is_success() {
            return Err(AppError::Custom(format!(
                "Provider 返回 HTTP {}",
                status.as_u16()
            )));
        }
        Self::parse_provider_response_value(&text, content_type.as_deref())
    }

    /// 部分兼容网关会在非流式请求中携带 BOM、SSE 分帧或代码围栏；这些格式均可在
    /// 不放宽内容校验的前提下恢复为 JSON。空响应、HTML 错页及读取中断仍明确失败，
    /// 且不向 UI 回传远端正文，避免意外泄露敏感内容。
    fn parse_provider_response_value(
        response_body: &str,
        content_type: Option<&str>,
    ) -> Result<Value, AppError> {
        let body = response_body.trim_start_matches('\u{feff}').trim();
        if body.is_empty() {
            return Err(AppError::Custom(
                "Provider 返回空响应，请稍后重试或检查服务网关配置".into(),
            ));
        }
        if let Ok(value) = serde_json::from_str(body) {
            return Ok(value);
        }

        if let Some(fenced_json) = Self::extract_fenced_json(body) {
            if let Ok(value) = serde_json::from_str(&fenced_json) {
                return Ok(value);
            }
        }

        if content_type
            .map(|value| value.to_ascii_lowercase().contains("text/event-stream"))
            .unwrap_or(false)
        {
            let events = body
                .lines()
                .filter_map(|line| line.trim().strip_prefix("data:"))
                .map(str::trim)
                .filter(|value| !value.is_empty() && *value != "[DONE]")
                .map(|value| {
                    serde_json::from_str::<Value>(value).map_err(|_| {
                        AppError::Custom("Provider SSE 响应包含无法解析的数据帧".into())
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            if !events.is_empty() {
                let streamed_text = events
                    .iter()
                    .filter_map(|event| {
                        Self::value_at_mixed_path(event, &["choices", "0", "delta", "content"])
                            .and_then(Value::as_str)
                    })
                    .collect::<String>();
                if !streamed_text.trim().is_empty() {
                    return Ok(json!({ "output_text": streamed_text }));
                }
                if let Some(value) = events.last() {
                    return Ok(value.clone());
                }
            }
        }

        let is_plain_text = content_type
            .map(|value| value.to_ascii_lowercase().starts_with("text/plain"))
            .unwrap_or(false);
        let lower_body = body.to_ascii_lowercase();
        let looks_like_html = ["<!doctype html", "<html", "<body"]
            .iter()
            .any(|marker| lower_body.contains(marker));
        if is_plain_text && !looks_like_html {
            return Ok(Value::String(body.to_string()));
        }

        let format_hint = content_type
            .filter(|value| !value.trim().is_empty())
            .map(|value| format!("（Content-Type: {value}）"))
            .unwrap_or_default();
        Err(AppError::Custom(format!(
            "Provider 返回格式无法识别{format_hint}，请检查模型服务或网关配置"
        )))
    }

    fn extract_fenced_json(body: &str) -> Option<String> {
        let lines = body.lines().collect::<Vec<_>>();
        if lines.len() < 3 || !lines.first()?.trim_start().starts_with("```") {
            return None;
        }
        let closing_index = lines
            .iter()
            .enumerate()
            .skip(1)
            .find_map(|(index, line)| (line.trim() == "```").then_some(index))?;
        if closing_index != lines.len() - 1 {
            return None;
        }
        Some(lines[1..closing_index].join("\n"))
    }

    fn collect_model_ids(value: &Value, models: &mut BTreeSet<String>) {
        for path in [&["data"][..], &["models"][..], &["model"][..]] {
            if let Some(items) = Self::value_at_path(value, path).and_then(Value::as_array) {
                for item in items {
                    if let Some(model) = item
                        .get("id")
                        .or_else(|| item.get("name"))
                        .and_then(Value::as_str)
                        .map(Self::normalize_model_name)
                        .filter(|item| !item.is_empty())
                    {
                        models.insert(model);
                    }
                }
            }
        }
    }

    fn value_at_path<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Value> {
        path.iter()
            .try_fold(value, |current, key| current.get(*key))
    }

    fn value_at_mixed_path<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Value> {
        path.iter().try_fold(value, |current, key| {
            if let Ok(index) = key.parse::<usize>() {
                current.get(index)
            } else {
                current.get(*key)
            }
        })
    }

    fn normalize_model_name(value: &str) -> String {
        value
            .trim()
            .strip_prefix("models/")
            .unwrap_or_else(|| value.trim())
            .to_string()
    }

    async fn status_result(
        response: reqwest::Response,
        success_message: &str,
    ) -> Result<(u16, String), AppError> {
        let status = response.status();
        if status.is_success() {
            return Ok((status.as_u16(), success_message.into()));
        }
        Err(AppError::Custom(format!(
            "Provider 返回 HTTP {}",
            status.as_u16()
        )))
    }

    fn anthropic_headers(api_key: &str) -> Result<HeaderMap, AppError> {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert(
            "x-api-key",
            HeaderValue::from_str(api_key)
                .map_err(|_| AppError::InvalidInput("API Key 包含非法字符".into()))?,
        );
        headers.insert("anthropic-version", HeaderValue::from_static("2023-06-01"));
        Ok(headers)
    }

    fn http_error(err: reqwest::Error) -> AppError {
        if err.is_timeout() {
            AppError::ProviderTransient("Provider 连接超时，请重试".into())
        } else if err.is_connect() {
            // reqwest 错误可能包含 URL query；任何 Provider 原始错误都不得进入 IPC/UI。
            AppError::ProviderTransient("Provider 网络连接失败，请检查网络后重试".into())
        } else {
            AppError::ProviderTransient("Provider 请求失败，请重试".into())
        }
    }

    fn encrypt_secret(db: &Database, secret: &str) -> Result<(String, String), AppError> {
        let key = Self::secret_key(db)?;
        let cipher = Aes256Gcm::new_from_slice(&key)
            .map_err(|_| AppError::Custom("密钥初始化失败".into()))?;
        let mut nonce_bytes = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ciphertext = cipher
            .encrypt(nonce, secret.as_bytes())
            .map_err(|_| AppError::Custom("API Key 加密失败".into()))?;
        Ok((
            general_purpose::STANDARD.encode(nonce_bytes),
            general_purpose::STANDARD.encode(ciphertext),
        ))
    }

    fn decrypt_secret(db: &Database, nonce: &str, ciphertext: &str) -> Result<String, AppError> {
        let key = Self::secret_key(db)?;
        let nonce_bytes = general_purpose::STANDARD
            .decode(nonce)
            .map_err(|_| AppError::Custom("密钥 nonce 解码失败".into()))?;
        let ciphertext_bytes = general_purpose::STANDARD
            .decode(ciphertext)
            .map_err(|_| AppError::Custom("密钥密文解码失败".into()))?;
        let cipher = Aes256Gcm::new_from_slice(&key)
            .map_err(|_| AppError::Custom("密钥初始化失败".into()))?;
        let plaintext = cipher
            .decrypt(Nonce::from_slice(&nonce_bytes), ciphertext_bytes.as_ref())
            .map_err(|_| AppError::Custom("API Key 解密失败".into()))?;
        String::from_utf8(plaintext).map_err(|_| AppError::Custom("API Key 不是合法 UTF-8".into()))
    }

    fn secret_key(db: &Database) -> Result<[u8; 32], AppError> {
        let seed = match db.get_config(SECRET_SEED_KEY)? {
            Some(value) => value,
            None => {
                let mut bytes = [0u8; 32];
                rand::thread_rng().fill_bytes(&mut bytes);
                let value = general_purpose::STANDARD.encode(bytes);
                db.set_config(SECRET_SEED_KEY, &value)?;
                value
            }
        };
        let mut hasher = Sha256::new();
        hasher.update(seed.as_bytes());
        hasher.update(b"tauri-ssh-ai-provider");
        let digest = hasher.finalize();
        let mut key = [0u8; 32];
        key.copy_from_slice(&digest);
        Ok(key)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, RETRY_AFTER};
    use tokio::io::AsyncWriteExt;

    use super::{AiProviderService, REMOTE_CHAT_TIMEOUT};
    use crate::database::Database;
    use crate::error::AppError;
    use crate::models::{AiProviderEmbeddingInput, UpsertAiProviderInput};

    fn provider_input(
        key: &str,
        default_model: &str,
        embedding_model: &str,
        capabilities: Vec<&str>,
    ) -> UpsertAiProviderInput {
        UpsertAiProviderInput {
            key: key.to_string(),
            name: key.to_string(),
            region: "local".to_string(),
            protocol: "OpenAI-compatible".to_string(),
            default_model: default_model.to_string(),
            embedding_model: embedding_model.to_string(),
            status: "unconfigured".to_string(),
            endpoint: "http://127.0.0.1:18080/v1".to_string(),
            auth_type: "none".to_string(),
            api_key: None,
            clear_api_key: None,
            cost_level: "低".to_string(),
            capabilities: capabilities.into_iter().map(str::to_string).collect(),
            models: Vec::new(),
            scenario_fit: Vec::new(),
            fallback: String::new(),
            enabled: true,
        }
    }

    #[test]
    fn embedding_only_provider_allows_empty_chat_model() -> Result<(), Box<dyn std::error::Error>> {
        let database = Database::init(":memory:")?;
        let provider = AiProviderService::upsert(
            &database,
            provider_input(
                "embedding-only",
                "",
                "multilingual-e5-small-int8",
                vec!["embedding"],
            ),
        )?;

        assert!(provider.default_model.is_empty());
        assert_eq!(provider.embedding_model, "multilingual-e5-small-int8");
        assert!(AiProviderService::supports_embedding(&provider));
        assert!(!AiProviderService::supports_chat(&provider));
        assert!(!AiProviderService::requires_api_key(
            &provider.protocol,
            &provider.auth_type,
        ));
        Ok(())
    }

    #[test]
    fn provider_capability_requires_its_corresponding_model(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let database = Database::init(":memory:")?;

        assert!(AiProviderService::upsert(
            &database,
            provider_input("chat-without-model", "", "", vec!["chat"]),
        )
        .is_err());
        assert!(AiProviderService::upsert(
            &database,
            provider_input("embedding-without-model", "", "", vec!["embedding"]),
        )
        .is_err());
        Ok(())
    }

    #[test]
    fn switching_to_no_auth_clears_stored_api_key() -> Result<(), Box<dyn std::error::Error>> {
        let database = Database::init(":memory:")?;
        let mut authenticated = provider_input("auth-switch", "chat-model", "", vec!["chat"]);
        authenticated.auth_type = "Bearer API Key".to_string();
        authenticated.api_key = Some("original-secret".to_string());
        AiProviderService::upsert(&database, authenticated)?;

        let stored = database
            .get_ai_provider_secret_row("auth-switch")?
            .ok_or("Provider 未保存")?;
        assert!(stored.secret_nonce.is_some());
        assert!(stored.secret_ciphertext.is_some());

        let mut no_auth = provider_input("auth-switch", "chat-model", "", vec!["chat"]);
        no_auth.api_key = Some("must-not-be-stored".to_string());
        no_auth.clear_api_key = Some(false);
        AiProviderService::upsert(&database, no_auth)?;

        let cleared = database
            .get_ai_provider_secret_row("auth-switch")?
            .ok_or("Provider 未保存")?;
        assert!(cleared.secret_nonce.is_none());
        assert!(cleared.secret_ciphertext.is_none());
        Ok(())
    }

    #[test]
    fn openai_auth_type_selects_the_expected_header() -> Result<(), Box<dyn std::error::Error>> {
        let client = reqwest::Client::new();
        let bearer = AiProviderService::apply_openai_auth(
            client.get("http://127.0.0.1/models"),
            "Bearer API Key",
            "secret",
        )
        .build()?;
        assert_eq!(
            bearer
                .headers()
                .get(AUTHORIZATION)
                .and_then(|v| v.to_str().ok()),
            Some("Bearer secret")
        );

        let x_api_key = AiProviderService::apply_openai_auth(
            client.get("http://127.0.0.1/models"),
            "x-api-key",
            "secret",
        )
        .build()?;
        assert_eq!(
            x_api_key
                .headers()
                .get("x-api-key")
                .and_then(|v| v.to_str().ok()),
            Some("secret")
        );
        assert!(x_api_key.headers().get(AUTHORIZATION).is_none());

        let api_key = AiProviderService::apply_openai_auth(
            client.get("http://127.0.0.1/models"),
            "API Key",
            "secret",
        )
        .build()?;
        assert_eq!(
            api_key
                .headers()
                .get("api-key")
                .and_then(|v| v.to_str().ok()),
            Some("secret")
        );
        assert!(api_key.headers().get(AUTHORIZATION).is_none());
        Ok(())
    }

    #[test]
    fn embedding_model_is_persisted_independently_from_chat_default(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let database = Database::init(":memory:")?;
        let provider = AiProviderService::upsert(
            &database,
            UpsertAiProviderInput {
                key: "embedding-test".to_string(),
                name: "Embedding Test".to_string(),
                region: "local".to_string(),
                protocol: "OpenAI-compatible".to_string(),
                default_model: "chat-model-v1".to_string(),
                embedding_model: " text-embedding-v1 ".to_string(),
                status: "configured".to_string(),
                endpoint: "http://127.0.0.1:11434".to_string(),
                auth_type: "none".to_string(),
                api_key: None,
                clear_api_key: None,
                cost_level: "低".to_string(),
                capabilities: vec!["chat".to_string()],
                models: vec!["chat-model-v1".to_string()],
                scenario_fit: Vec::new(),
                fallback: String::new(),
                enabled: true,
            },
        )?;

        assert_eq!(provider.default_model, "chat-model-v1");
        assert_eq!(provider.embedding_model, "text-embedding-v1");
        assert!(provider.capabilities.iter().any(|item| item == "embedding"));

        let updated = AiProviderService::upsert(
            &database,
            UpsertAiProviderInput {
                key: provider.key,
                name: provider.name,
                region: provider.region,
                protocol: provider.protocol,
                default_model: provider.default_model,
                embedding_model: "text-embedding-v2".to_string(),
                status: provider.status,
                endpoint: provider.endpoint,
                auth_type: provider.auth_type,
                api_key: None,
                clear_api_key: None,
                cost_level: provider.cost_level,
                capabilities: provider.capabilities,
                models: provider.models,
                scenario_fit: provider.scenario_fit,
                fallback: provider.fallback,
                enabled: provider.enabled,
            },
        )?;
        assert_eq!(updated.default_model, "chat-model-v1");
        assert_eq!(updated.embedding_model, "text-embedding-v2");
        Ok(())
    }

    #[test]
    fn openai_embedding_payload_and_response_are_contract_safe(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let inputs = vec!["需求 A".to_string(), "需求 B".to_string()];
        let payload =
            AiProviderService::openai_embedding_payload("text-embedding-3-small", &inputs, Some(3));
        assert_eq!(payload["model"], "text-embedding-3-small");
        assert_eq!(payload["input"], serde_json::json!(["需求 A", "需求 B"]));
        assert_eq!(payload["encoding_format"], "float");
        assert_eq!(payload["dimensions"], 3);

        let vectors = AiProviderService::parse_openai_embedding_value(
            &serde_json::json!({
                "data": [
                    {"index": 1, "embedding": [0.0, 1.0]},
                    {"index": 0, "embedding": [1.0, 0.0]}
                ]
            }),
            2,
        )?;
        assert_eq!(vectors, vec![vec![1.0, 0.0], vec![0.0, 1.0]]);
        assert!(AiProviderService::parse_openai_embedding_value(
            &serde_json::json!({
                "data": [
                    {"index": 0, "embedding": [1.0, 0.0]},
                    {"index": 1, "embedding": [1.0]}
                ]
            }),
            2,
        )
        .is_err());
        Ok(())
    }

    #[test]
    fn ollama_embedding_payload_and_response_are_contract_safe(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let inputs = vec!["退款审批".to_string()];
        let payload = AiProviderService::ollama_embedding_payload("nomic-embed-text", &inputs);
        assert_eq!(
            payload,
            serde_json::json!({
                "model": "nomic-embed-text",
                "input": ["退款审批"]
            })
        );

        let vectors = AiProviderService::parse_ollama_embedding_value(
            &serde_json::json!({"embedding": [0.25, 0.75]}),
            1,
        )?;
        assert_eq!(vectors, vec![vec![0.25, 0.75]]);
        assert!(AiProviderService::parse_ollama_embedding_value(
            &serde_json::json!({"embeddings": [[0.0], [1.0, 2.0]]}),
            2,
        )
        .is_err());
        Ok(())
    }

    #[test]
    fn provider_response_parser_tolerates_common_gateway_wrappers(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let bom_json = AiProviderService::parse_provider_response_value(
            "\u{feff}{\"choices\":[{\"message\":{\"content\":\"ok\"}}]}",
            Some("application/json"),
        )?;
        assert_eq!(bom_json["choices"][0]["message"]["content"], "ok");

        let fenced_json = AiProviderService::parse_provider_response_value(
            "```json\n{\"output_text\":\"from fenced json\"}\n```",
            Some("text/plain"),
        )?;
        assert_eq!(fenced_json["output_text"], "from fenced json");

        let sse_json = AiProviderService::parse_provider_response_value(
            "data: {\"choices\":[{\"delta\":{\"content\":\"part \"}}]}\n\ndata: {\"choices\":[{\"delta\":{\"content\":\"complete\"}}]}\n\ndata: [DONE]",
            Some("text/event-stream"),
        )?;
        assert_eq!(sse_json["output_text"], "part complete");

        let plain_text = AiProviderService::parse_provider_response_value(
            "可直接展示的回答",
            Some("text/plain; charset=utf-8"),
        )?;
        assert_eq!(plain_text, serde_json::json!("可直接展示的回答"));
        Ok(())
    }

    #[test]
    fn provider_response_parser_rejects_empty_and_html_bodies_without_echoing_them() {
        let empty =
            AiProviderService::parse_provider_response_value("  ", Some("application/json"))
                .expect_err("空响应必须明确失败");
        assert!(empty.to_string().contains("返回空响应"));

        let html = AiProviderService::parse_provider_response_value(
            "<html><body>gateway failure</body></html>",
            Some("text/html"),
        )
        .expect_err("HTML 错页不能被当作模型回答");
        assert!(html.to_string().contains("格式无法识别"));
        assert!(!html.to_string().contains("gateway failure"));

        let mislabeled_html = AiProviderService::parse_provider_response_value(
            "upstream error\n<html><body>gateway failure</body></html>",
            Some("text/plain"),
        )
        .expect_err("带前缀的 HTML 错页不能被当作模型回答");
        assert!(mislabeled_html.to_string().contains("格式无法识别"));
        assert!(!mislabeled_html.to_string().contains("gateway failure"));

        let malformed_sse = AiProviderService::parse_provider_response_value(
            "data: {\"choices\":\n\ndata: [DONE]",
            Some("text/event-stream"),
        )
        .expect_err("损坏的 SSE 帧不能被静默忽略");
        assert!(malformed_sse.to_string().contains("SSE"));
    }

    #[tokio::test]
    async fn truncated_provider_body_is_reported_as_retryable() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("应启动本地测试服务");
        let address = listener.local_addr().expect("应读取测试服务地址");
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.expect("应收到测试请求");
            stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 200\r\nConnection: close\r\n\r\n{\"choices\":[",
                )
                .await
                .expect("应写入截断响应");
        });
        let response = reqwest::get(format!("http://{address}"))
            .await
            .expect("应收到响应头");
        let error = AiProviderService::response_json(response)
            .await
            .expect_err("截断的正文不能被当作成功响应");
        server.await.expect("测试服务应正常退出");

        assert!(matches!(error, AppError::ProviderTransient(_)));
    }

    #[test]
    fn chat_timeout_allows_long_evidence_answers_to_finish() {
        assert!(REMOTE_CHAT_TIMEOUT >= Duration::from_secs(180));
    }

    #[tokio::test]
    async fn embedding_rejects_oversized_batches_before_provider_access(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let database = Database::init(":memory:")?;
        let result = AiProviderService::embed(
            &database,
            AiProviderEmbeddingInput {
                provider_key: "not-needed".to_string(),
                model: None,
                inputs: (0..33).map(|index| format!("文本 {index}")).collect(),
                dimensions: None,
            },
        )
        .await;
        assert!(result.is_err());

        let mut headers = HeaderMap::new();
        headers.insert(RETRY_AFTER, HeaderValue::from_static("2"));
        assert_eq!(
            AiProviderService::embedding_retry_delay(1, &headers),
            Duration::from_secs(2)
        );
        assert_eq!(
            AiProviderService::embedding_retry_delay(2, &HeaderMap::new()),
            Duration::from_millis(500)
        );
        Ok(())
    }

    #[tokio::test]
    async fn embedding_preflight_blocks_before_sending_to_provider(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let database = Database::init(":memory:")?;
        let mut provider = provider_input(
            "embedding-preflight",
            "",
            "multilingual-e5-small-int8",
            vec!["embedding"],
        );
        provider.status = "configured".to_string();
        AiProviderService::upsert(&database, provider)?;

        let result = AiProviderService::embed_with_preflight(
            &database,
            AiProviderEmbeddingInput {
                provider_key: "embedding-preflight".to_string(),
                model: None,
                inputs: vec!["不应发送到远程服务的测试文本".to_string()],
                dimensions: None,
            },
            || {
                Err(crate::error::AppError::InvalidInput(
                    "远程授权已关闭".to_string(),
                ))
            },
        )
        .await;

        assert!(matches!(
            result,
            Err(crate::error::AppError::InvalidInput(message)) if message == "远程授权已关闭"
        ));
        Ok(())
    }
}
