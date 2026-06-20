use std::{collections::BTreeSet, time::Instant};

use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use base64::{engine::general_purpose, Engine as _};
use rand::RngCore;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::database::Database;
use crate::error::AppError;
use crate::models::{
    AiProvider, AiProviderAskInput, AiProviderAskResult, AiProviderModelListInput,
    AiProviderModelListResult, AiProviderRoute, AiProviderTestResult, UpsertAiProviderInput,
    UpsertAiProviderRouteInput,
};

const SECRET_SEED_KEY: &str = "ai_provider_secret_seed";

pub struct AiProviderService;

impl AiProviderService {
    pub fn list(db: &Database) -> Result<Vec<AiProvider>, AppError> {
        db.list_ai_providers()
    }

    pub fn upsert(db: &Database, input: UpsertAiProviderInput) -> Result<AiProvider, AppError> {
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

        let api_key = match (&row.secret_nonce, &row.secret_ciphertext) {
            (Some(nonce), Some(ciphertext)) => Some(Self::decrypt_secret(db, nonce, ciphertext)?),
            _ => None,
        };

        let started = Instant::now();
        let response = Self::perform_test_request(&row.provider, api_key.as_deref()).await;
        let latency_ms = started.elapsed().as_millis() as i64;

        match response {
            Ok((status_code, message)) => {
                db.update_ai_provider_latency(&row.provider.key, latency_ms, "configured")?;
                Ok(AiProviderTestResult {
                    ok: true,
                    provider_key: row.provider.key,
                    provider_name: row.provider.name,
                    model: row.provider.default_model,
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
                model: row.provider.default_model,
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

        let stored_secret = match input
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
        };

        let models = Self::perform_model_list_request(&input, stored_secret.as_deref()).await?;
        Ok(AiProviderModelListResult {
            provider_key: input.key,
            models,
            source: "provider_api".into(),
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

        let api_key = match (&row.secret_nonce, &row.secret_ciphertext) {
            (Some(nonce), Some(ciphertext)) => Some(Self::decrypt_secret(db, nonce, ciphertext)?),
            _ => None,
        };
        let started = Instant::now();
        let answer = Self::perform_chat_request(
            &row.provider,
            api_key.as_deref(),
            input.system_prompt.as_deref(),
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
        Ok(())
    }

    fn resolve_chat_provider(
        db: &Database,
        provider_key: Option<&str>,
    ) -> Result<crate::database::AiProviderSecretRow, AppError> {
        if let Some(key) = provider_key
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            return db
                .get_ai_provider_secret_row(key)?
                .ok_or_else(|| AppError::NotFound(format!("AI Provider '{}' 不存在", key)));
        }

        let providers = db.list_ai_providers()?;
        let provider = providers
            .into_iter()
            .find(|item| item.enabled && item.status == "configured")
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
                "{}/v1beta/models/{}:generateContent?key={}",
                endpoint, provider.default_model, key
            );
            let response = client
                .post(url)
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

        if protocol.contains("local") && api_key.is_none() {
            let response = client
                .get(format!("{}/models", endpoint))
                .send()
                .await
                .map_err(Self::http_error)?;
            return Self::status_result(response, "本地 OpenAI-compatible 模型列表读取成功").await;
        }

        let key = api_key.ok_or_else(|| AppError::InvalidInput("连接测试需要 API Key".into()))?;
        let response = client
            .get(format!("{}/models", endpoint))
            .header(AUTHORIZATION, format!("Bearer {}", key))
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
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .map_err(|e| AppError::Custom(format!("HTTP 客户端初始化失败: {}", e)))?;
        let endpoint = provider.endpoint.trim_end_matches('/');
        let protocol = provider.protocol.to_lowercase();
        let system_text = system_prompt
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("你是一个 SSH 运维助手。请用中文简洁回答，涉及命令时说明风险，不要假装已经执行命令。");

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
                "{}/v1beta/models/{}:generateContent?key={}",
                endpoint, provider.default_model, key
            );
            let response = client
                .post(url)
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
        if let Some(key) = api_key {
            request = request.header(AUTHORIZATION, format!("Bearer {}", key));
        } else if !protocol.contains("local") {
            return Err(AppError::InvalidInput("AI 问答需要 API Key".into()));
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
                .query(&[("key", key)])
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
        } else if protocol.contains("local") && api_key.is_none() {
            client
                .get(format!("{}/models", endpoint))
                .send()
                .await
                .map_err(Self::http_error)?
        } else {
            let key =
                api_key.ok_or_else(|| AppError::InvalidInput("读取模型列表需要 API Key".into()))?;
            client
                .get(format!("{}/models", endpoint))
                .header(AUTHORIZATION, format!("Bearer {}", key))
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
        let text = response.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(AppError::Custom(format!(
                "Provider 返回 HTTP {}: {}",
                status.as_u16(),
                text.chars().take(500).collect::<String>()
            )));
        }

        let value: Value = serde_json::from_str(&text)?;
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

    async fn response_json(response: reqwest::Response) -> Result<Value, AppError> {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(AppError::Custom(format!(
                "Provider 返回 HTTP {}: {}",
                status.as_u16(),
                text.chars().take(500).collect::<String>()
            )));
        }
        serde_json::from_str(&text)
            .map_err(|e| AppError::Custom(format!("Provider JSON 解析失败: {}", e)))
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
        let text = response.text().await.unwrap_or_default();
        Err(AppError::Custom(format!(
            "Provider 返回 HTTP {}: {}",
            status.as_u16(),
            text.chars().take(500).collect::<String>()
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
            AppError::Custom("Provider 连接超时".into())
        } else if err.is_connect() {
            AppError::Custom(format!("Provider 网络连接失败: {}", err))
        } else {
            AppError::Custom(format!("Provider 请求失败: {}", err))
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
