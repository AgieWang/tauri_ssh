use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use ssh2::{FileStat, OpenFlags, OpenType, Sftp};

use crate::database::Database;
use crate::error::AppError;
use crate::models::{
    SftpCreateDirectoryInput, SftpCreateFileInput, SftpDeleteInput, SftpFileEntry, SftpListInput,
    SftpListResult, SftpOperationResult, SftpReadTextInput, SftpReadTextResult, SftpRenameInput,
    SftpTransferPathInput, SftpWriteTextInput,
};
use crate::services::terminal::TerminalService;

const DEFAULT_READ_LIMIT_BYTES: u64 = 1024 * 1024;
const MAX_READ_LIMIT_BYTES: u64 = 5 * 1024 * 1024;

pub struct SftpService;

impl SftpService {
    pub fn list(db: &Database, input: SftpListInput) -> Result<SftpListResult, AppError> {
        let path = normalize_remote_path(&input.path)?;
        let server_alias = input.server_alias;
        let sftp = connect_sftp(db, &server_alias)?;
        let entries = sftp
            .readdir(Path::new(&path))
            .map_err(|e| AppError::Custom(format!("读取远程目录失败: {}", e)))?;
        let mut entries = entries
            .into_iter()
            .filter_map(|(entry_path, stat)| {
                let name = entry_path.file_name()?.to_string_lossy().to_string();
                if name == "." || name == ".." {
                    return None;
                }
                Some(file_entry(&path, &entry_path, &stat))
            })
            .collect::<Vec<_>>();
        entries.sort_by(
            |left, right| match (left.file_type.as_str(), right.file_type.as_str()) {
                ("directory", "file") => std::cmp::Ordering::Less,
                ("file", "directory") => std::cmp::Ordering::Greater,
                _ => left.name.to_lowercase().cmp(&right.name.to_lowercase()),
            },
        );
        Ok(SftpListResult {
            server_alias,
            parent: parent_remote_path(&path),
            path,
            entries,
        })
    }

    pub fn read_text(
        db: &Database,
        input: SftpReadTextInput,
    ) -> Result<SftpReadTextResult, AppError> {
        let path = normalize_remote_file_path(&input.path)?;
        let max_bytes = input
            .max_bytes
            .unwrap_or(DEFAULT_READ_LIMIT_BYTES)
            .clamp(1, MAX_READ_LIMIT_BYTES);
        let server_alias = input.server_alias;
        let sftp = connect_sftp(db, &server_alias)?;
        let stat = sftp
            .stat(Path::new(&path))
            .map_err(|e| AppError::Custom(format!("读取远程文件状态失败: {}", e)))?;
        if is_directory(&stat) {
            return Err(AppError::InvalidInput("不能以文本方式读取目录".into()));
        }
        let size = stat.size.unwrap_or(0);
        let file = sftp
            .open(Path::new(&path))
            .map_err(|e| AppError::Custom(format!("打开远程文件失败: {}", e)))?;
        let mut buffer = Vec::new();
        let mut limited = file.take(max_bytes + 1);
        limited.read_to_end(&mut buffer)?;
        let truncated = buffer.len() as u64 > max_bytes;
        if truncated {
            buffer.truncate(max_bytes as usize);
        }
        let content = String::from_utf8(buffer)
            .map_err(|_| AppError::InvalidInput("远程文件不是有效 UTF-8 文本".into()))?;
        Ok(SftpReadTextResult {
            server_alias,
            path,
            content,
            size,
            truncated,
        })
    }

    pub fn write_text(
        db: &Database,
        input: SftpWriteTextInput,
    ) -> Result<SftpOperationResult, AppError> {
        let path = normalize_remote_file_path(&input.path)?;
        let content = input.content;
        let bytes = content.as_bytes().len() as u64;
        let server_alias = input.server_alias;
        let sftp = connect_sftp(db, &server_alias)?;
        write_remote_file(&sftp, &path, content.as_bytes())?;
        Ok(operation_result(
            server_alias,
            path,
            "远程文件已保存",
            Some(bytes),
        ))
    }

    pub fn upload(
        db: &Database,
        input: SftpTransferPathInput,
    ) -> Result<SftpOperationResult, AppError> {
        let remote_path = normalize_remote_file_path(&input.remote_path)?;
        let local_path = normalize_local_path(&input.local_path)?;
        if !local_path.is_file() {
            return Err(AppError::InvalidInput(format!(
                "本地文件不存在：{}",
                local_path.display()
            )));
        }
        let bytes = local_path.metadata()?.len();
        let server_alias = input.server_alias;
        let sftp = connect_sftp(db, &server_alias)?;
        write_remote_file_from_path(&sftp, &remote_path, &local_path)?;
        Ok(operation_result(
            server_alias,
            remote_path,
            "本地文件已上传到远端",
            Some(bytes),
        ))
    }

    pub fn download(
        db: &Database,
        input: SftpTransferPathInput,
    ) -> Result<SftpOperationResult, AppError> {
        let remote_path = normalize_remote_file_path(&input.remote_path)?;
        let local_path = normalize_local_path(&input.local_path)?;
        let server_alias = input.server_alias;
        let sftp = connect_sftp(db, &server_alias)?;
        let mut remote = sftp
            .open(Path::new(&remote_path))
            .map_err(|e| AppError::Custom(format!("打开远程文件失败: {}", e)))?;
        let mut content = Vec::new();
        remote.read_to_end(&mut content)?;
        if let Some(parent) = local_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&local_path, &content)?;
        Ok(operation_result(
            server_alias,
            remote_path,
            format!("远程文件已下载到 {}", local_path.display()),
            Some(content.len() as u64),
        ))
    }

    pub fn create_directory(
        db: &Database,
        input: SftpCreateDirectoryInput,
    ) -> Result<SftpOperationResult, AppError> {
        let path = normalize_remote_file_path(&input.path)?;
        let server_alias = input.server_alias;
        let sftp = connect_sftp(db, &server_alias)?;
        sftp.mkdir(Path::new(&path), 0o755)
            .map_err(|e| AppError::Custom(format!("创建远程目录失败: {}", e)))?;
        Ok(operation_result(server_alias, path, "远程目录已创建", None))
    }

    pub fn create_file(
        db: &Database,
        input: SftpCreateFileInput,
    ) -> Result<SftpOperationResult, AppError> {
        Self::write_text(
            db,
            SftpWriteTextInput {
                server_alias: input.server_alias,
                path: input.path,
                content: input.content.unwrap_or_default(),
            },
        )
    }

    pub fn rename(db: &Database, input: SftpRenameInput) -> Result<SftpOperationResult, AppError> {
        let from_path = normalize_remote_file_path(&input.from_path)?;
        let to_path = normalize_remote_file_path(&input.to_path)?;
        let server_alias = input.server_alias;
        let sftp = connect_sftp(db, &server_alias)?;
        sftp.rename(Path::new(&from_path), Path::new(&to_path), None)
            .map_err(|e| AppError::Custom(format!("远程重命名失败: {}", e)))?;
        Ok(operation_result(
            server_alias,
            to_path,
            "远程路径已重命名",
            None,
        ))
    }

    pub fn delete(db: &Database, input: SftpDeleteInput) -> Result<SftpOperationResult, AppError> {
        let path = normalize_remote_file_path(&input.path)?;
        let server_alias = input.server_alias;
        let sftp = connect_sftp(db, &server_alias)?;
        if input.file_type == "directory" {
            sftp.rmdir(Path::new(&path)).map_err(|e| {
                AppError::Custom(format!("删除远程目录失败，请确认目录为空: {}", e))
            })?;
        } else {
            sftp.unlink(Path::new(&path))
                .map_err(|e| AppError::Custom(format!("删除远程文件失败: {}", e)))?;
        }
        Ok(operation_result(server_alias, path, "远程路径已删除", None))
    }
}

fn connect_sftp(db: &Database, server_alias: &str) -> Result<Sftp, AppError> {
    let (_, session) = TerminalService::connect_saved_server(db, server_alias, 30)?;
    session
        .sftp()
        .map_err(|e| AppError::Custom(format!("SFTP 会话创建失败: {}", e)))
}

fn write_remote_file(sftp: &Sftp, path: &str, content: &[u8]) -> Result<(), AppError> {
    let mut file = sftp
        .open_mode(
            Path::new(path),
            OpenFlags::WRITE | OpenFlags::CREATE | OpenFlags::TRUNCATE,
            0o644,
            OpenType::File,
        )
        .map_err(|e| AppError::Custom(format!("打开远程文件写入失败: {}", e)))?;
    file.write_all(content)?;
    Ok(())
}

fn write_remote_file_from_path(sftp: &Sftp, path: &str, local_path: &Path) -> Result<(), AppError> {
    let mut local = std::fs::File::open(local_path)?;
    let mut remote = sftp
        .open_mode(
            Path::new(path),
            OpenFlags::WRITE | OpenFlags::CREATE | OpenFlags::TRUNCATE,
            0o644,
            OpenType::File,
        )
        .map_err(|e| AppError::Custom(format!("打开远程文件写入失败: {}", e)))?;
    std::io::copy(&mut local, &mut remote)?;
    Ok(())
}

fn file_entry(parent: &str, path: &Path, stat: &FileStat) -> SftpFileEntry {
    let name = path
        .file_name()
        .map(|item| item.to_string_lossy().to_string())
        .unwrap_or_else(|| path.display().to_string());
    let remote_path = join_remote_path(parent, &name);
    let file_type = if is_directory(stat) {
        "directory"
    } else if is_symlink(stat) {
        "symlink"
    } else {
        "file"
    }
    .to_string();
    let permissions = stat
        .perm
        .map(format_permissions)
        .unwrap_or_else(|| "---------".into());
    let readonly = stat.perm.map(|mode| mode & 0o200 == 0).unwrap_or(false);
    SftpFileEntry {
        name,
        path: remote_path,
        parent: parent.to_string(),
        file_type,
        size: stat.size.unwrap_or(0),
        modified_at: stat.mtime.map(|value| value as i64),
        permissions,
        readonly,
    }
}

fn is_directory(stat: &FileStat) -> bool {
    stat.perm
        .map(|mode| mode & 0o170000 == 0o040000)
        .unwrap_or(false)
}

fn is_symlink(stat: &FileStat) -> bool {
    stat.perm
        .map(|mode| mode & 0o170000 == 0o120000)
        .unwrap_or(false)
}

fn format_permissions(mode: u32) -> String {
    let file_type = match mode & 0o170000 {
        0o040000 => 'd',
        0o120000 => 'l',
        _ => '-',
    };
    let mut value = String::from(file_type);
    for bit in [
        0o400, 0o200, 0o100, 0o040, 0o020, 0o010, 0o004, 0o002, 0o001,
    ] {
        value.push(match bit {
            0o400 | 0o040 | 0o004 => {
                if mode & bit != 0 {
                    'r'
                } else {
                    '-'
                }
            }
            0o200 | 0o020 | 0o002 => {
                if mode & bit != 0 {
                    'w'
                } else {
                    '-'
                }
            }
            _ => {
                if mode & bit != 0 {
                    'x'
                } else {
                    '-'
                }
            }
        });
    }
    value
}

fn normalize_remote_path(path: &str) -> Result<String, AppError> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Ok(".".into());
    }
    if trimmed.contains('\0') {
        return Err(AppError::InvalidInput("远程路径非法".into()));
    }
    Ok(if trimmed == "/" {
        "/".into()
    } else {
        trimmed.trim_end_matches('/').to_string()
    })
}

fn normalize_remote_file_path(path: &str) -> Result<String, AppError> {
    let normalized = normalize_remote_path(path)?;
    if normalized == "." || normalized == "/" {
        return Err(AppError::InvalidInput(
            "不能对根目录或空路径执行该操作".into(),
        ));
    }
    Ok(normalized)
}

fn normalize_local_path(path: &str) -> Result<PathBuf, AppError> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err(AppError::InvalidInput("本地路径不能为空".into()));
    }
    if let Some(rest) = trimmed.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return Ok(PathBuf::from(home).join(rest));
        }
    }
    Ok(PathBuf::from(trimmed))
}

fn parent_remote_path(path: &str) -> String {
    if path == "." || path == "/" {
        return path.into();
    }
    let trimmed = path.trim_end_matches('/');
    match trimmed.rsplit_once('/') {
        Some(("", _)) => "/".into(),
        Some((parent, _)) if !parent.is_empty() => parent.into(),
        _ => ".".into(),
    }
}

fn join_remote_path(parent: &str, name: &str) -> String {
    if parent == "/" {
        format!("/{}", name)
    } else if parent == "." {
        name.into()
    } else {
        format!("{}/{}", parent.trim_end_matches('/'), name)
    }
}

fn operation_result(
    server_alias: String,
    path: String,
    message: impl Into<String>,
    bytes: Option<u64>,
) -> SftpOperationResult {
    SftpOperationResult {
        ok: true,
        server_alias,
        path,
        message: message.into(),
        bytes,
    }
}
