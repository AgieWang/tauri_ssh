use serde::{Deserialize, Serialize};

/// 上传意图只携带由本地选择器授予的临时句柄；Rust 侧仍须复核路径与文件签名。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadKnowledgeAssetInput {
    pub project_id: i64,
    #[serde(default)]
    pub project_version_id: Option<i64>,
    /// 明确声明该附件适用于项目全部版本；不能用“未选择版本”代替。
    #[serde(default)]
    pub cross_version_scope: Option<String>,
    pub file_handle: String,
    #[serde(default)]
    pub display_name: Option<String>,
    /// 文件夹上传时保留安全的来源名称，普通文件上传省略；绝对路径永远不进入该字段。
    #[serde(default)]
    pub source_folder_name: Option<String>,
    /// 图片发送给远程识别服务前必须由本次上传明确同意；后端仍会复核图片类型与 Provider。
    #[serde(default)]
    pub allow_remote_ocr: bool,
    /// 仅保存已配置 Provider 的稳定引用，绝不从前端接收 API Key 或图像正文。
    #[serde(default)]
    pub ocr_provider_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadKnowledgeAssetFileInput {
    pub file_handle: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub allow_remote_ocr: bool,
    #[serde(default)]
    pub ocr_provider_key: Option<String>,
}

/// 批量上传以单文件为失败边界；合法文件保持排队结果，失败文件可单独重新选择后重试。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadKnowledgeAssetBatchInput {
    pub project_id: i64,
    #[serde(default)]
    pub project_version_id: Option<i64>,
    #[serde(default)]
    pub cross_version_scope: Option<String>,
    /// 一次文件夹选择对应一个来源文件夹，文件仍按独立文档入库。
    #[serde(default)]
    pub source_folder_name: Option<String>,
    pub files: Vec<UploadKnowledgeAssetFileInput>,
}

/// 此路径仅来自桌面文件选择器，用于换取一次性句柄；不会写入数据库或返回给前端。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrepareKnowledgeUploadFileInput {
    pub selected_path: String,
}

/// 文件夹选择只返回经过后端递归校验的文件句柄；目录本身不会暴露给前端或持久化。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrepareKnowledgeUploadDirectoryInput {
    pub selected_path: String,
}

/// 选择文件后返回的一次性句柄。页面仅保存该句柄和显示信息，不能再读取原始路径。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedKnowledgeUploadFile {
    pub file_handle: String,
    pub display_name: String,
    pub size_bytes: i64,
}

/// HTML 原型文件夹的准备结果。文件夹按可上传文件批量导入，脚本不会在 WebView 或后端执行。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedKnowledgeUploadDirectory {
    pub directory_name: String,
    pub files: Vec<PreparedKnowledgeUploadFile>,
    pub skipped_count: i64,
    pub total_size_bytes: i64,
}

/// 上传任务创建后的逐文件结果。文件复制已完成，但解析与索引由后台任务继续执行。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeDocumentUploadResult {
    pub document_id: i64,
    pub asset_id: i64,
    pub import_job_id: i64,
    pub import_job_key: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeDocumentUploadBatchItemResult {
    pub display_name: String,
    pub result: Option<KnowledgeDocumentUploadResult>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeDocumentUploadBatchResult {
    pub items: Vec<KnowledgeDocumentUploadBatchItemResult>,
}
