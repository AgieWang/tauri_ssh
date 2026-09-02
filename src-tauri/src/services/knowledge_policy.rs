use crate::database::Database;
use crate::error::AppError;
use crate::models::{KnowledgeCitation, KnowledgeDocument};
use regex::{Captures, Regex};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

const REDACTED_VALUE: &str = "[REDACTED]";
const REDACTED_TOKEN: &str = "[REDACTED_TOKEN]";
const REDACTED_CONNECTION_STRING: &str = "[REDACTED_CONNECTION_STRING]";

static CONNECTION_STRING_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)(?:jdbc:[^\s'\"`]+|(?:redis|postgres(?:ql)?|mongodb(?:\+[a-z0-9]+)?):\/\/[^\s'\"`]+|[a-z][a-z0-9+.-]*:\/\/[^\s'\"`]*@[^\s'\"`]+)"#)
        .expect("连接串脱敏正则必须有效")
});
static AUTHORIZATION_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)(?P<prefix>\b(?:proxy-)?authorization\b\s*:\s*[\"']?(?:bearer|basic)\s+)[^\s,;\"'`}\]]+"#)
        .expect("认证头脱敏正则必须有效")
});
static TOKEN_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:sk-|ghp_|github_pat_|glpat-|xox[abpsr]-)[a-z0-9_-]+")
        .expect("令牌脱敏正则必须有效")
});
static SECRET_ASSIGNMENT_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?m)(?P<prefix>[\"']?(?P<key>[A-Za-z_][A-Za-z0-9_.-]*)[\"']?\s*(?:=|:)\s*)(?P<value>\"(?:\\.|[^\"\\\r\n])*\"|'(?:\\.|[^'\\\r\n])*'|[^,;}\]\r\n]+)"#)
        .expect("凭据赋值脱敏正则必须有效")
});
static RUNTIME_SECRET_REFERENCE_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"^(?:process\.env|import\.meta\.env|response|config|runtime|environment|env|this)(?:(?:\.[A-Za-z_$][A-Za-z0-9_$]*)|(?:\[\s*[\"'][A-Za-z_$][A-Za-z0-9_$]*[\"']\s*\]))+$"#,
    )
    .expect("运行时凭据引用正则必须有效")
});
static PRIVATE_KEY_PEM_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)-----BEGIN [A-Z0-9 ]*PRIVATE KEY[A-Z0-9 ]*-----")
        .expect("私钥 PEM 检测正则必须有效")
});
static CERTIFICATE_PEM_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)-----BEGIN [A-Z0-9 ]*CERTIFICATE[A-Z0-9 ]*-----")
        .expect("证书 PEM 检测正则必须有效")
});

/// 知识库远程聊天默认可用。每一份正文仍必须经过活动状态、敏感级别和内容安全检查；
/// 远程 Embedding 继续维持独立授权，避免扩大向量化的发送范围。
pub struct KnowledgePolicyService;

impl KnowledgePolicyService {
    /// 所有本地来源读取共用的边界检查。调用方只能传入已登记根目录下的普通文件：
    /// 符号链接、路径穿越和目录均在读取正文前被拒绝，防止不同同步器产生策略差异。
    pub fn authorize_local_file(root: &Path, candidate: &Path) -> Result<PathBuf, AppError> {
        let canonical_root = fs::canonicalize(root).map_err(|error| {
            AppError::InvalidInput(format!("知识源根目录无法访问: {}: {error}", root.display()))
        })?;
        let metadata = fs::symlink_metadata(candidate)?;
        if metadata.file_type().is_symlink() {
            return Err(AppError::InvalidInput(format!(
                "知识源不允许跟随符号链接: {}",
                candidate.display()
            )));
        }
        let canonical_path = fs::canonicalize(candidate)?;
        if !canonical_path.starts_with(&canonical_root) {
            return Err(AppError::InvalidInput(format!(
                "知识源文件越出授权根目录: {}",
                candidate.display()
            )));
        }
        if !metadata.is_file() {
            return Err(AppError::InvalidInput(format!(
                "知识源只允许读取普通文件: {}",
                candidate.display()
            )));
        }
        Ok(canonical_path)
    }

    /// MCP 只能读取来源、文档状态和敏感级别均允许公开给 MCP 的文档。调用方在返回
    /// 文档正文、引用摘录或检索命中前必须经过此检查，不能把 MCP 当成前端 API 的旁路。
    pub fn authorize_mcp_document(
        db: &Database,
        document: &KnowledgeDocument,
        content: &str,
    ) -> Result<(), AppError> {
        if document.status != "active" || document.deleted_at.is_some() {
            return Err(AppError::InvalidInput(
                "非活动知识文档不能通过 MCP 读取".into(),
            ));
        }
        if !document.allow_mcp {
            return Err(AppError::InvalidInput("知识文档未授权 MCP 读取".into()));
        }
        if !matches!(document.sensitivity.as_str(), "public" | "internal") {
            return Err(AppError::InvalidInput(
                "confidential 或 restricted 知识文档不能通过 MCP 返回".into(),
            ));
        }
        if let Some(source_id) = document.source_id {
            let source = db
                .get_knowledge_source_by_id(source_id)?
                .ok_or_else(|| AppError::NotFound(format!("知识源不存在: {source_id}")))?;
            if !source.enabled {
                return Err(AppError::InvalidInput(
                    "知识源已禁用，不能通过 MCP 读取".into(),
                ));
            }
        }
        if let Some(rule) = detect_sensitive_content(content) {
            return Err(AppError::InvalidInput(format!(
                "内容安全检查阻断 MCP 输出（规则: {rule}）"
            )));
        }
        Ok(())
    }

    /// 桌面 Command 与浏览器 Dev API 共用的正文输出边界。restricted 文档可以保留
    /// 最小元数据用于审计和跳过原因展示，但其版本正文、片段、差异和引用摘录均不得
    /// 通过任一 UI/API 输出通道返回。
    pub fn authorize_content_output(document: &KnowledgeDocument) -> Result<(), AppError> {
        if document.sensitivity == "restricted" {
            return Err(AppError::NotFound("受限知识正文不可读取".to_string()));
        }
        if document.status != "active" || document.deleted_at.is_some() {
            return Err(AppError::NotFound("非活动知识文档不可读取".to_string()));
        }
        Ok(())
    }

    /// 远程 Embedding 与远程问答共用文档、来源、敏感级别及秘密检查边界。将判断放在
    /// PolicyService 可避免任一 Provider 适配器自行放宽来源授权或记录正文。
    pub fn authorize_remote_embedding(
        db: &Database,
        document_id: i64,
        content: &str,
    ) -> Result<(), AppError> {
        if document_id <= 0 {
            return Err(AppError::InvalidInput("知识文档 ID 必须为正数".into()));
        }
        let document = db
            .get_knowledge_document_by_id(document_id)?
            .ok_or_else(|| AppError::NotFound(format!("知识文档不存在: {document_id}")))?;
        if document.status != "active" {
            return Err(AppError::InvalidInput(
                "非活动知识文档不能远程向量化".into(),
            ));
        }
        let source_id = document
            .source_id
            .ok_or_else(|| AppError::InvalidInput("未关联知识源的文档不能远程向量化".into()))?;
        let source = db
            .get_knowledge_source_by_id(source_id)?
            .ok_or_else(|| AppError::NotFound(format!("知识源不存在: {source_id}")))?;
        if !source.enabled || !source.allow_remote_embedding {
            return Err(AppError::InvalidInput(
                "知识源未授权远程向量化；知识正文不会发送到远程服务商".into(),
            ));
        }
        if !matches!(document.sensitivity.as_str(), "public" | "internal") {
            return Err(AppError::InvalidInput(
                "confidential 或 restricted 文档不能远程向量化".into(),
            ));
        }
        if content.trim().is_empty() {
            return Err(AppError::InvalidInput("远程向量化内容不能为空".into()));
        }
        if let Some(rule) = detect_sensitive_content(content) {
            return Err(AppError::InvalidInput(format!(
                "内容安全检查阻断远程向量化（规则: {rule}）"
            )));
        }
        Ok(())
    }

    /// 远程 Embedding 与远程问答一致：来源与敏感级别仍须授权，但可遮蔽的凭据以
    /// 占位符参与向量化，避免代码报告因密码字段示例而永久阻断整个构建。
    pub fn sanitize_remote_embedding_content(
        db: &Database,
        document_id: i64,
        content: &str,
    ) -> Result<String, AppError> {
        let document = db
            .get_knowledge_document_by_id(document_id)?
            .ok_or_else(|| AppError::NotFound(format!("知识文档不存在: {document_id}")))?;
        if document.status != "active"
            || !matches!(document.sensitivity.as_str(), "public" | "internal")
        {
            return Err(AppError::InvalidInput(
                "该知识文档不能远程向量化".to_string(),
            ));
        }
        let source_id = document.source_id.ok_or_else(|| {
            AppError::InvalidInput("未关联知识源的文档不能远程向量化".to_string())
        })?;
        let source = db
            .get_knowledge_source_by_id(source_id)?
            .ok_or_else(|| AppError::NotFound(format!("知识源不存在: {source_id}")))?;
        if !source.enabled || !source.allow_remote_embedding {
            return Err(AppError::InvalidInput("知识源未授权远程向量化".to_string()));
        }
        if content.trim().is_empty() {
            return Err(AppError::InvalidInput("远程向量化内容不能为空".to_string()));
        }
        Self::sanitize_remote_ai_context(content)
    }

    /// 检查将 RAG 上下文发送给远程聊天 Provider 的元数据安全边界。可遮蔽的秘密由
    /// `sanitize_remote_ai_context` 在外发前替换；私钥和证书仍在该方法中硬性拒绝。
    pub fn authorize_remote_ai_context(
        db: &Database,
        citations: &[KnowledgeCitation],
        context: &str,
    ) -> Result<(), AppError> {
        if context.trim().is_empty() || citations.is_empty() {
            return Err(AppError::InvalidInput(
                "远程知识问答必须包含已审核的证据片段".to_string(),
            ));
        }
        for citation in citations {
            let Some(document_id) = citation.document_id else {
                return Err(AppError::InvalidInput(
                    "远程知识问答不接受没有文档归属的关系证据".to_string(),
                ));
            };
            let document = db
                .get_knowledge_document_by_id(document_id)?
                .ok_or_else(|| AppError::NotFound(format!("知识文档不存在: {document_id}")))?;
            if document.status != "active" {
                return Err(AppError::InvalidInput(
                    "非活动知识文档不能发送到远程 AI".to_string(),
                ));
            }
            if !matches!(document.sensitivity.as_str(), "public" | "internal") {
                return Err(AppError::InvalidInput(
                    "confidential 或 restricted 知识文档不能发送到远程 AI".to_string(),
                ));
            }
            if let Some(source_id) = document.source_id {
                let source = db
                    .get_knowledge_source_by_id(source_id)?
                    .ok_or_else(|| AppError::NotFound(format!("知识源不存在: {source_id}")))?;
                if !source.enabled {
                    return Err(AppError::InvalidInput(
                        "知识源已禁用，不能发送到 AI Provider".to_string(),
                    ));
                }
            }
        }
        Ok(())
    }

    /// 将可遮蔽的秘密替换为固定占位符后，才允许组装远程 AI Provider 请求。私钥与
    /// 证书无法安全地按片段恢复上下文，因此保持失败关闭，绝不降级为外发。
    pub fn sanitize_remote_ai_context(context: &str) -> Result<String, AppError> {
        match detect_sensitive_content(context) {
            Some("private_key") => {
                return Err(AppError::InvalidInput(
                    "内容安全检查阻断远程 AI：不允许发送私钥".to_string(),
                ));
            }
            Some("certificate") => {
                return Err(AppError::InvalidInput(
                    "内容安全检查阻断远程 AI：不允许发送证书".to_string(),
                ));
            }
            _ => {}
        }

        let without_connections = CONNECTION_STRING_PATTERN
            .replace_all(context, REDACTED_CONNECTION_STRING)
            .into_owned();
        let without_authorization = AUTHORIZATION_PATTERN
            .replace_all(&without_connections, |captures: &Captures<'_>| {
                format!("{}{}", &captures["prefix"], REDACTED_TOKEN)
            })
            .into_owned();
        let without_assignments = SECRET_ASSIGNMENT_PATTERN
            .replace_all(&without_authorization, |captures: &Captures<'_>| {
                if secret_key_matches(&captures["key"].to_ascii_lowercase(), SECRET_KEYS)
                    && assignment_value_contains_secret(&captures["value"])
                {
                    format!("{}{}", &captures["prefix"], REDACTED_VALUE)
                } else {
                    captures[0].to_string()
                }
            })
            .into_owned();
        Ok(TOKEN_PATTERN
            .replace_all(&without_assignments, REDACTED_TOKEN)
            .into_owned())
    }
}

const SECRET_KEYS: &[&str] = &[
    "password",
    "passwd",
    "secret",
    "token",
    "api_key",
    "api-key",
    "apikey",
    "client_secret",
    "github_token",
    "gitlab_token",
    "access_key",
    "private_key",
    "aws_access_key_id",
    "aws_secret_access_key",
];

/// 统一的轻量秘密检测。命中时只返回规则 ID，调用者不得写入命中的正文或令牌。
pub fn detect_sensitive_content(content: &str) -> Option<&'static str> {
    let normalized = content.to_ascii_lowercase();
    if PRIVATE_KEY_PEM_PATTERN.is_match(content) {
        return Some("private_key");
    }
    if CERTIFICATE_PEM_PATTERN.is_match(content) {
        return Some("certificate");
    }
    if normalized.contains("aws_access_key_id")
        || normalized.contains("aws_secret_access_key")
        || contains_token_prefix(&normalized, "xoxb-")
        || contains_token_prefix(&normalized, "xoxp-")
        || contains_token_prefix(&normalized, "ghp_")
        || contains_token_prefix(&normalized, "github_pat_")
        || contains_token_prefix(&normalized, "glpat-")
        || contains_token_prefix(&normalized, "sk-")
    {
        return Some("cloud_or_service_token");
    }
    if contains_secret_assignment(&normalized)
        || normalized.contains("authorization: bearer ")
        || normalized.contains("jdbc:")
        || normalized.contains("redis://")
        || normalized.contains("postgres://")
        || normalized.contains("mongodb://")
        || normalized.contains("://") && normalized.contains('@')
    {
        return Some("credential_or_connection_string");
    }
    None
}

/// 令牌前缀必须位于一个非标识符边界之后，并且后面至少有一个字母或数字；
/// 这样不会把普通单词中的 task-、disk- 等片段误报成 sk- 令牌。
fn contains_token_prefix(content: &str, prefix: &str) -> bool {
    let mut offset = 0;
    while let Some(relative) = content[offset..].find(prefix) {
        let start = offset + relative;
        let boundary = content[..start]
            .chars()
            .next_back()
            .map_or(true, |character| {
                !character.is_ascii_alphanumeric() && character != '_'
            });
        let has_payload = content[start + prefix.len()..]
            .chars()
            .next()
            .is_some_and(|character| character.is_ascii_alphanumeric());
        if boundary && has_payload {
            return true;
        }
        offset = start + prefix.len();
    }
    false
}

/// 覆盖 `.env`、YAML、properties、JSON 风格的常见凭据赋值。这里只识别键名与赋值符，
/// 不提取或记录值；远程 AI 场景宁可拒绝疑似配置，也不能把凭据外发。
fn contains_secret_assignment(content: &str) -> bool {
    SECRET_ASSIGNMENT_PATTERN
        .captures_iter(content)
        .any(|captures| {
            secret_key_matches(&captures["key"].to_ascii_lowercase(), SECRET_KEYS)
                && assignment_value_contains_secret(&captures["value"])
        })
}

/// 布尔控制项和运行时变量只描述“如何取得或处理凭据”，源码中并没有秘密值。
/// 仅在赋值右侧看起来包含字面量时阻断，既避免 `blCancelToken: true` 误伤整份
/// 页面，也继续拒绝 `.env`、YAML、properties、JSON 和源码中的真实明文值。
fn assignment_value_contains_secret(value: &str) -> bool {
    let value = value.trim().trim_end_matches([',', ';']);
    if value.is_empty()
        || matches!(
            value.to_ascii_lowercase().as_str(),
            "true" | "false" | "null" | "undefined"
        )
    {
        return false;
    }
    if (value.starts_with("${") && value.ends_with('}'))
        || (!value.starts_with(['"', '\'']) && RUNTIME_SECRET_REFERENCE_PATTERN.is_match(value))
    {
        return false;
    }
    let unquoted = value.trim_matches(['"', '\'']).trim();
    !unquoted.is_empty()
}

fn secret_key_matches(key: &str, secret_keys: &[&str]) -> bool {
    let compact_key = key
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect::<String>();
    secret_keys.iter().any(|secret_key| {
        let compact_secret_key = secret_key
            .chars()
            .filter(|character| character.is_ascii_alphanumeric())
            .collect::<String>();
        key == *secret_key
            || key.ends_with(&format!("_{secret_key}"))
            || key.ends_with(&format!("-{secret_key}"))
            || compact_key.ends_with(&compact_secret_key)
    })
}

#[cfg(test)]
mod tests {
    use super::{detect_sensitive_content, KnowledgePolicyService};
    use crate::database::Database;
    use crate::models::{
        KnowledgeCitation, KnowledgeDocument, UpsertKnowledgeDocumentInput,
        UpsertKnowledgeSourceInput,
    };
    use std::fs;

    #[test]
    fn identifies_secret_categories_without_returning_the_secret() {
        assert_eq!(
            detect_sensitive_content("-----BEGIN PRIVATE KEY-----\nsecret"),
            Some("private_key")
        );
        assert_eq!(
            detect_sensitive_content("password=not-for-logs"),
            Some("credential_or_connection_string")
        );
        assert_eq!(
            detect_sensitive_content("GITLAB_TOKEN=glpat-example"),
            Some("cloud_or_service_token")
        );
        assert_eq!(
            detect_sensitive_content("spring.datasource.password: not-for-ai"),
            Some("credential_or_connection_string")
        );
        assert_eq!(
            detect_sensitive_content("GITHUB_TOKEN: not-for-ai"),
            Some("credential_or_connection_string")
        );
        for content in [
            "DB_PASSWORD=not-for-ai",
            "OPENAI_API_KEY=not-for-ai",
            "DATABASE_PASSWORD: not-for-ai",
            "export API_KEY=not-for-ai",
            "const DB_PASSWORD = \"not-for-ai\";",
            "const OPENAI_API_KEY = \"not-for-ai\";",
            "{\"openai_api_key\":\"not-for-ai\"}",
            "dbPassword: not-for-ai",
            "token: \"actual-token-value\"",
        ] {
            assert_eq!(
                detect_sensitive_content(content),
                Some("credential_or_connection_string"),
                "应拒绝常见环境变量格式: {content}"
            );
        }
        assert_eq!(
            detect_sensitive_content("jdbc:mysql://user:password@db.example/app"),
            Some("credential_or_connection_string")
        );
        assert_eq!(detect_sensitive_content("正常需求说明"), None);
        for content in [
            "blCancelToken: true",
            "cancelToken: false",
            "const DB_PASSWORD = process.env.DB_PASSWORD;",
            "token: response.data",
            "headers[\"token\"] = runtimeToken",
        ] {
            assert_eq!(
                detect_sensitive_content(content),
                None,
                "不含秘密值的控制字段或运行时引用不应阻断源码索引: {content}"
            );
        }
        for content in [
            "password: hunter2.example",
            "API_KEY=secret.value",
            "token: literal(value)",
            "password: literal[index]",
        ] {
            assert_eq!(
                detect_sensitive_content(content),
                Some("credential_or_connection_string"),
                "带标点的未加引号字面量仍应按秘密处理: {content}"
            );
        }
        assert_eq!(
            detect_sensitive_content("task-1 与 disk-queue 是普通标识"),
            None
        );
        assert_eq!(
            detect_sensitive_content("Authorization: sk-live-example"),
            Some("cloud_or_service_token")
        );
    }

    #[test]
    fn remote_ai_context_masks_sendable_secrets_but_keeps_normal_evidence() {
        let input = concat!(
            "DB_PASSWORD=actual-secret-value\n",
            "service.password: correct horse battery staple # local only\n",
            "quoted_password: \"escaped \\\"secret\\\" value\"\n",
            "headers: { Authorization: Bearer actual-bearer-token }\n",
            "database_url: jdbc:mysql://user:actual-secret-value@db.example/app\n",
            "blCancelToken: true\n",
            "token: response.data\n",
            "runtime_password: process.env.DB_PASSWORD\n",
            "api_key: import.meta.env.VITE_API_KEY\n",
            "password: hunter2.example\n",
            "client_secret: literal(value)\n",
            "access_key: literal[index]\n",
            "const normalValue = 42;"
        );
        let sanitized = KnowledgePolicyService::sanitize_remote_ai_context(input)
            .expect("可遮蔽的内容应继续用于远程 AI");

        assert!(!sanitized.contains("actual-secret-value"));
        assert!(!sanitized.contains("actual-bearer-token"));
        assert!(!sanitized.contains("correct horse battery staple"));
        assert!(!sanitized.contains("escaped \\\"secret\\\" value"));
        assert!(!sanitized.contains("hunter2.example"));
        assert!(!sanitized.contains("literal(value)"));
        assert!(!sanitized.contains("literal[index]"));
        assert!(!sanitized.contains("db.example"));
        assert!(sanitized.contains("DB_PASSWORD=[REDACTED]"));
        assert!(sanitized.contains("service.password: [REDACTED]"));
        assert!(sanitized.contains("quoted_password: [REDACTED]"));
        assert!(sanitized.contains("Authorization: Bearer [REDACTED_TOKEN]"));
        assert!(sanitized.contains("[REDACTED_CONNECTION_STRING]"));
        assert!(sanitized.contains("blCancelToken: true"));
        assert!(sanitized.contains("token: response.data"));
        assert!(sanitized.contains("runtime_password: process.env.DB_PASSWORD"));
        assert!(sanitized.contains("api_key: import.meta.env.VITE_API_KEY"));
        assert!(sanitized.contains("const normalValue = 42;"));
    }

    #[test]
    fn remote_ai_context_still_rejects_private_key_and_certificate() {
        for content in [
            "-----BEGIN PRIVATE KEY-----\nnot-for-ai",
            "-----BEGIN PGP PRIVATE KEY BLOCK-----\nnot-for-ai",
            "-----BEGIN CERTIFICATE-----\nnot-for-ai",
            "-----BEGIN TRUSTED CERTIFICATE-----\nnot-for-ai",
        ] {
            assert!(KnowledgePolicyService::sanitize_remote_ai_context(content).is_err());
        }
    }

    #[test]
    fn local_file_authorization_rejects_escape_and_symlink(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let root = std::env::temp_dir().join(format!(
            "tauri-ssh-knowledge-policy-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        ));
        let source_root = root.join("source");
        fs::create_dir_all(&source_root)?;
        let inside = source_root.join("inside.md");
        let outside = root.join("outside.md");
        fs::write(&inside, "内部文档")?;
        fs::write(&outside, "外部文档")?;
        assert!(KnowledgePolicyService::authorize_local_file(&source_root, &inside).is_ok());
        assert!(KnowledgePolicyService::authorize_local_file(&source_root, &outside).is_err());
        #[cfg(unix)]
        {
            let link = source_root.join("outside-link.md");
            std::os::unix::fs::symlink(&outside, &link)?;
            assert!(KnowledgePolicyService::authorize_local_file(&source_root, &link).is_err());
        }
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn restricted_documents_cannot_cross_content_output_boundary() {
        let document = KnowledgeDocument {
            id: 1,
            document_key: "restricted".to_string(),
            project_id: None,
            source_id: None,
            doc_type: "code".to_string(),
            title: "受限".to_string(),
            logical_path: "secret.env".to_string(),
            source_folder_name: None,
            status: "active".to_string(),
            sensitivity: "restricted".to_string(),
            tags: Vec::new(),
            latest_version_id: None,
            allow_ai: false,
            allow_mcp: false,
            created_at: String::new(),
            updated_at: String::new(),
            deleted_at: None,
        };
        assert!(KnowledgePolicyService::authorize_content_output(&document).is_err());
    }

    #[test]
    fn remote_ai_context_is_available_without_explicit_source_or_document_opt_in(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let database = Database::init(":memory:")?;
        let source = database.upsert_knowledge_source(&UpsertKnowledgeSourceInput {
            id: None,
            source_key: "remote-ai-default".to_string(),
            project_id: None,
            source_type: "manual_markdown".to_string(),
            display_name: "默认远程 AI 来源".to_string(),
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
            document_key: "remote-ai-default-document".to_string(),
            project_id: None,
            source_id: Some(source.id),
            doc_type: "code".to_string(),
            title: "默认远程 AI 代码证据".to_string(),
            logical_path: "src/example.rs".to_string(),
            sensitivity: "internal".to_string(),
            tags: Vec::new(),
            allow_ai: false,
            allow_mcp: false,
        })?;
        let citation = KnowledgeCitation {
            citation_key: "code:1:file:1".to_string(),
            source_type: "code_file".to_string(),
            document_id: Some(document.id),
            document_version_id: None,
            chunk_id: None,
            project_id: None,
            release_id: None,
            title: document.title,
            logical_path: "src/example.rs".to_string(),
            heading_path: String::new(),
            commit_sha: String::new(),
            external_key: String::new(),
            snapshot_id: None,
            symbol_key: String::new(),
            start_line: Some(1),
            end_line: Some(1),
            excerpt: "pub fn example() {}".to_string(),
        };

        KnowledgePolicyService::authorize_remote_ai_context(
            &database,
            &[citation.clone()],
            "pub fn example() {}",
        )?;
        KnowledgePolicyService::authorize_remote_ai_context(
            &database,
            &[citation],
            "API_KEY=must-not-send",
        )?;
        Ok(())
    }
}
