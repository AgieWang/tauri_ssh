use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::AppError;
use crate::services::knowledge_policy::KnowledgePolicyService;

const MAX_EXPORT_BYTES: usize = 5 * 1024 * 1024;

pub struct KnowledgeQaExportService;

impl KnowledgeQaExportService {
    /// 将用户明确选择的问答 Markdown 写入本地文件。正文先经过统一脱敏，避免把
    /// 证据摘录中的令牌、连接串等敏感值带入普通文档；写入采用同目录临时文件。
    pub fn save_markdown(path: &str, content: &str) -> Result<String, AppError> {
        let mut target = PathBuf::from(path.trim());
        if !target.is_absolute() {
            return Err(AppError::InvalidInput(
                "Markdown 导出路径必须是用户选择的绝对路径".to_string(),
            ));
        }
        if target.extension().is_none() {
            target.set_extension("md");
        }
        let extension = target
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if !matches!(extension.as_str(), "md" | "markdown") {
            return Err(AppError::InvalidInput(
                "问答文档只能保存为 .md 或 .markdown 文件".to_string(),
            ));
        }
        let parent = target
            .parent()
            .ok_or_else(|| AppError::InvalidInput("Markdown 导出目录无效".to_string()))?;
        if !parent.is_dir() {
            return Err(AppError::InvalidInput(
                "Markdown 导出目录不存在或不是目录".to_string(),
            ));
        }
        if target.exists()
            && fs::symlink_metadata(&target)
                .map(|metadata| metadata.file_type().is_symlink())
                .unwrap_or(false)
        {
            return Err(AppError::InvalidInput(
                "Markdown 导出目标不能是符号链接".to_string(),
            ));
        }

        let sanitized = KnowledgePolicyService::sanitize_remote_ai_context(content)?;
        if sanitized.trim().is_empty() {
            return Err(AppError::InvalidInput(
                "不能保存空的问答 Markdown 文档".to_string(),
            ));
        }
        if sanitized.len() > MAX_EXPORT_BYTES {
            return Err(AppError::InvalidInput(format!(
                "问答 Markdown 文档不能超过 {} MB",
                MAX_EXPORT_BYTES / 1024 / 1024
            )));
        }

        let temp = temporary_path(&target);
        let write_result = (|| -> Result<(), AppError> {
            let mut file = OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&temp)?;
            file.write_all(sanitized.as_bytes())?;
            file.flush()?;
            file.sync_all()?;
            drop(file);

            // macOS/Linux 的 rename 可原子替换；Windows 先删除已经由保存对话框确认的
            // 目标，再完成同目录替换，避免留下半写入文件。
            if cfg!(windows) && target.exists() {
                fs::remove_file(&target)?;
            }
            fs::rename(&temp, &target)?;
            Ok(())
        })();
        if write_result.is_err() {
            let _ = fs::remove_file(&temp);
        }
        write_result.map(|()| target.to_string_lossy().into_owned())
    }
}

fn temporary_path(target: &Path) -> PathBuf {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let file_name = target
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("knowledge-qa");
    target
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(format!(
            ".{file_name}.{timestamp}-{}.tmp",
            std::process::id()
        ))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::KnowledgeQaExportService;

    #[test]
    fn saves_markdown_with_masked_secret_and_default_extension(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let root = std::env::temp_dir().join(format!("tauri-ssh-qa-export-{}", std::process::id()));
        fs::create_dir_all(&root)?;
        let path = root.join("conversation");
        let saved = KnowledgeQaExportService::save_markdown(
            path.to_string_lossy().as_ref(),
            "# 回答\n\nTOKEN=sk-test123\n",
        )?;
        assert!(saved.ends_with("conversation.md"));
        let content = fs::read_to_string(&saved)?;
        assert!(content.contains("[REDACTED]"));
        assert!(!content.contains("sk-test123"));
        fs::remove_dir_all(&root)?;
        Ok(())
    }
}
