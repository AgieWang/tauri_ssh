//! 上传文件在进入资产存储和解析器前的共同校验边界。
//!
//! 扩展名只是用户意图，不能作为可信类型依据；这里将文件名、推断 MIME、文件签名和
//! OOXML 容器结构一起校验。后续解析器也复用同一限额，避免各格式分别实现不一致的
//! 压缩包与时间限制。

use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::time::{Duration, Instant};

use crate::error::AppError;

pub(crate) const MAX_ASSET_SIZE_BYTES: u64 = 20 * 1024 * 1024;
pub(crate) const MAX_UPLOAD_BATCH_FILES: usize = 50;
pub(crate) const MAX_UPLOAD_BATCH_SIZE_BYTES: u64 = 100 * 1024 * 1024;
pub(crate) const MAX_UPLOAD_DIRECTORY_DEPTH: usize = 12;
pub(crate) const MAX_CONTAINER_ENTRIES: usize = 2_000;
pub(crate) const MAX_CONTAINER_EXPANDED_SIZE_BYTES: u64 = 200 * 1024 * 1024;
pub(crate) const MAX_CONTAINER_DEPTH: usize = 1;
pub(crate) const FILE_PARSE_TIMEOUT: Duration = Duration::from_secs(30);
pub(crate) const MAX_IMAGE_EDGE_PIXELS: u32 = 12_000;
pub(crate) const MAX_IMAGE_PIXEL_COUNT: u64 = 40_000_000;

const SIGNATURE_READ_LIMIT: usize = 64 * 1024;

#[derive(Debug, Clone, Copy)]
pub(crate) struct UploadValidationLimits {
    pub(crate) max_file_size_bytes: u64,
    pub(crate) max_container_entries: usize,
    pub(crate) max_container_expanded_size_bytes: u64,
    pub(crate) max_container_depth: usize,
    pub(crate) timeout: Duration,
}

impl Default for UploadValidationLimits {
    fn default() -> Self {
        Self {
            max_file_size_bytes: MAX_ASSET_SIZE_BYTES,
            max_container_entries: MAX_CONTAINER_ENTRIES,
            max_container_expanded_size_bytes: MAX_CONTAINER_EXPANDED_SIZE_BYTES,
            max_container_depth: MAX_CONTAINER_DEPTH,
            timeout: FILE_PARSE_TIMEOUT,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ValidatedKnowledgeUploadFile {
    pub(crate) mime_type: &'static str,
    pub(crate) document_type: &'static str,
}

/// 供实际解析器在处理大文件或容器条目时复用的协作式截止时间。
///
/// Rust 解析器不会为了中止不可信输入而启动失控线程；解析器必须在可中断边界调用
/// `check`，超时后立即失败关闭，不能继续将结果写入索引。
#[derive(Debug, Clone)]
pub(crate) struct FileParseDeadline {
    started_at: Instant,
    timeout: Duration,
}

impl FileParseDeadline {
    pub(crate) fn new(timeout: Duration) -> Self {
        Self {
            started_at: Instant::now(),
            timeout,
        }
    }

    pub(crate) fn check(&self) -> Result<(), AppError> {
        if self.started_at.elapsed() > self.timeout {
            return Err(AppError::InvalidInput(
                "文件校验或解析超时，请缩小文件后重试".to_string(),
            ));
        }
        Ok(())
    }
}

pub(crate) fn validate_upload_file(
    source_path: &Path,
    display_name: &str,
) -> Result<ValidatedKnowledgeUploadFile, AppError> {
    validate_upload_file_with_limits(source_path, display_name, UploadValidationLimits::default())
}

fn validate_upload_file_with_limits(
    source_path: &Path,
    display_name: &str,
    limits: UploadValidationLimits,
) -> Result<ValidatedKnowledgeUploadFile, AppError> {
    let deadline = FileParseDeadline::new(limits.timeout);
    let metadata = std::fs::metadata(source_path)?;
    if metadata.len() > limits.max_file_size_bytes {
        return Err(AppError::InvalidInput(format!(
            "单个文件不能超过 {}MB",
            limits.max_file_size_bytes / 1024 / 1024
        )));
    }
    let file_type = upload_file_type(display_name)?;
    validate_file_signature(source_path, file_type, &deadline)?;
    if file_type.is_raster_image() {
        validate_raster_image_dimensions(source_path, &deadline)?;
    }
    if file_type.is_ooxml() {
        validate_ooxml_container(source_path, file_type, limits, &deadline)?;
    }
    deadline.check()?;
    Ok(ValidatedKnowledgeUploadFile {
        mime_type: file_type.mime_type(),
        document_type: file_type.document_type(),
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UploadFileType {
    Markdown,
    Text,
    Html,
    Css,
    JavaScript,
    Docx,
    Xlsx,
    Pptx,
    LegacyDoc,
    LegacyXls,
    LegacyPpt,
    Pdf,
    Png,
    Jpeg,
    Webp,
    Gif,
    Svg,
}

impl UploadFileType {
    fn mime_type(self) -> &'static str {
        match self {
            Self::Markdown => "text/markdown",
            Self::Text => "text/plain",
            Self::Html => "text/html",
            Self::Css => "text/css",
            Self::JavaScript => "text/javascript",
            Self::Docx => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
            Self::Xlsx => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
            Self::Pptx => {
                "application/vnd.openxmlformats-officedocument.presentationml.presentation"
            }
            Self::LegacyDoc | Self::LegacyXls | Self::LegacyPpt => "application/x-ole-storage",
            Self::Pdf => "application/pdf",
            Self::Png => "image/png",
            Self::Jpeg => "image/jpeg",
            Self::Webp => "image/webp",
            Self::Gif => "image/gif",
            Self::Svg => "image/svg+xml",
        }
    }

    fn document_type(self) -> &'static str {
        match self {
            Self::Markdown => "markdown",
            Self::Text => "text",
            Self::Html => "html",
            Self::Css | Self::JavaScript => "text",
            Self::Docx => "docx",
            Self::Xlsx => "xlsx",
            Self::Pptx => "pptx",
            Self::LegacyDoc | Self::LegacyXls | Self::LegacyPpt => "legacy_office",
            Self::Pdf => "pdf",
            Self::Png | Self::Jpeg | Self::Webp | Self::Gif | Self::Svg => "image",
        }
    }

    fn is_ooxml(self) -> bool {
        matches!(self, Self::Docx | Self::Xlsx | Self::Pptx)
    }

    fn is_legacy_office(self) -> bool {
        matches!(self, Self::LegacyDoc | Self::LegacyXls | Self::LegacyPpt)
    }

    fn is_raster_image(self) -> bool {
        matches!(self, Self::Png | Self::Jpeg | Self::Webp | Self::Gif)
    }

    fn required_ooxml_entry(self) -> Option<&'static str> {
        match self {
            Self::Docx => Some("word/document.xml"),
            Self::Xlsx => Some("xl/workbook.xml"),
            Self::Pptx => Some("ppt/presentation.xml"),
            _ => None,
        }
    }
}

fn upload_file_type(display_name: &str) -> Result<UploadFileType, AppError> {
    let extension = Path::new(display_name.trim())
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    match extension.as_str() {
        "md" | "mdx" => Ok(UploadFileType::Markdown),
        "txt" | "log" | "json" | "yaml" | "yml" | "sql" => Ok(UploadFileType::Text),
        "html" | "htm" => Ok(UploadFileType::Html),
        "css" => Ok(UploadFileType::Css),
        "js" | "mjs" => Ok(UploadFileType::JavaScript),
        "docx" => Ok(UploadFileType::Docx),
        "xlsx" => Ok(UploadFileType::Xlsx),
        "pptx" => Ok(UploadFileType::Pptx),
        "doc" => Ok(UploadFileType::LegacyDoc),
        "xls" => Ok(UploadFileType::LegacyXls),
        "ppt" => Ok(UploadFileType::LegacyPpt),
        "pdf" => Ok(UploadFileType::Pdf),
        "png" => Ok(UploadFileType::Png),
        "jpg" | "jpeg" => Ok(UploadFileType::Jpeg),
        "webp" => Ok(UploadFileType::Webp),
        "gif" => Ok(UploadFileType::Gif),
        "svg" => Ok(UploadFileType::Svg),
        _ => Err(AppError::InvalidInput(
            "暂不支持该文件类型，请选择文档、PDF 或图片文件".to_string(),
        )),
    }
}

pub(crate) fn is_supported_directory_upload_extension(extension: &str) -> bool {
    matches!(
        extension.trim().to_ascii_lowercase().as_str(),
        "md" | "mdx"
            | "txt"
            | "log"
            | "json"
            | "yaml"
            | "yml"
            | "sql"
            | "html"
            | "htm"
            | "css"
            | "js"
            | "mjs"
            | "docx"
            | "xlsx"
            | "pptx"
            | "doc"
            | "xls"
            | "ppt"
            | "pdf"
            | "png"
            | "jpg"
            | "jpeg"
            | "webp"
            | "gif"
            | "svg"
    )
}

fn validate_file_signature(
    source_path: &Path,
    file_type: UploadFileType,
    deadline: &FileParseDeadline,
) -> Result<(), AppError> {
    let mut file = File::open(source_path)?;
    let mut bytes = Vec::with_capacity(SIGNATURE_READ_LIMIT);
    file.by_ref()
        .take(SIGNATURE_READ_LIMIT as u64)
        .read_to_end(&mut bytes)?;
    deadline.check()?;
    let matches = match file_type {
        UploadFileType::Markdown
        | UploadFileType::Text
        | UploadFileType::Css
        | UploadFileType::JavaScript => looks_like_text(&bytes),
        UploadFileType::Html => looks_like_text(&bytes) && looks_like_html(&bytes),
        UploadFileType::Svg => looks_like_text(&bytes) && looks_like_svg(&bytes),
        UploadFileType::Docx | UploadFileType::Xlsx | UploadFileType::Pptx => {
            looks_like_zip(&bytes)
        }
        UploadFileType::LegacyDoc | UploadFileType::LegacyXls | UploadFileType::LegacyPpt => {
            bytes.starts_with(b"\xd0\xcf\x11\xe0\xa1\xb1\x1a\xe1")
        }
        UploadFileType::Pdf => bytes.starts_with(b"%PDF-"),
        UploadFileType::Png => bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
        UploadFileType::Jpeg => bytes.starts_with(b"\xff\xd8\xff"),
        UploadFileType::Webp => {
            bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP"
        }
        UploadFileType::Gif => bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a"),
    };
    if matches {
        if file_type.is_legacy_office() {
            return Err(legacy_office_converter_error(file_type));
        }
        Ok(())
    } else {
        Err(AppError::InvalidInput(
            "文件内容与扩展名或预期类型不一致，已拒绝导入".to_string(),
        ))
    }
}

/// 旧版二进制 Office 需要独立、隔离且禁宏的转换器。本应用尚未配置该转换器时，先在
/// 签名校验后明确拒绝，既不把二进制内容当文本解析，也不尝试调用用户机器上的 Office。
fn legacy_office_converter_error(file_type: UploadFileType) -> AppError {
    let (legacy_name, replacement) = match file_type {
        UploadFileType::LegacyDoc => ("DOC", "DOCX"),
        UploadFileType::LegacyXls => ("XLS", "XLSX"),
        UploadFileType::LegacyPpt => ("PPT", "PPTX"),
        _ => return AppError::Custom("旧版 Office 类型判断错误".to_string()),
    };
    AppError::InvalidInput(format!(
        "暂未配置安全转换器，不能导入旧版 {legacy_name} 文件。请在 Office 或 WPS 中另存为 {replacement} 后重试。"
    ))
}

/// 图片预览会在 WebView 解码，文件体积不能代表解码后的内存占用。上传阶段先限制
/// 可识别的宽高与总像素，预览读取阶段还会再次复核，防止内容寻址资产被离线替换后绕过。
fn validate_raster_image_dimensions(
    source_path: &Path,
    deadline: &FileParseDeadline,
) -> Result<(), AppError> {
    let mut file = File::open(source_path)?;
    let mut bytes = Vec::with_capacity(SIGNATURE_READ_LIMIT);
    file.by_ref()
        .take(SIGNATURE_READ_LIMIT as u64)
        .read_to_end(&mut bytes)?;
    deadline.check()?;
    let (width, height) = raster_image_dimensions(&bytes)
        .ok_or_else(|| AppError::InvalidInput("无法安全识别图片尺寸，已拒绝导入".to_string()))?;
    validate_image_pixel_limits(width, height)
}

pub(crate) fn validate_image_pixel_limits(width: i64, height: i64) -> Result<(), AppError> {
    if width <= 0 || height <= 0 {
        return Err(AppError::InvalidInput(
            "图片尺寸无效，已拒绝导入".to_string(),
        ));
    }
    let width = u64::try_from(width)
        .map_err(|_| AppError::InvalidInput("图片尺寸无效，已拒绝导入".to_string()))?;
    let height = u64::try_from(height)
        .map_err(|_| AppError::InvalidInput("图片尺寸无效，已拒绝导入".to_string()))?;
    let pixels = width
        .checked_mul(height)
        .ok_or_else(|| AppError::InvalidInput("图片像素数超出支持范围，已拒绝导入".to_string()))?;
    if width > u64::from(MAX_IMAGE_EDGE_PIXELS)
        || height > u64::from(MAX_IMAGE_EDGE_PIXELS)
        || pixels > MAX_IMAGE_PIXEL_COUNT
    {
        return Err(AppError::InvalidInput(format!(
            "图片尺寸超过 {} × {} 或 {} 万像素限制",
            MAX_IMAGE_EDGE_PIXELS,
            MAX_IMAGE_EDGE_PIXELS,
            MAX_IMAGE_PIXEL_COUNT / 10_000
        )));
    }
    Ok(())
}

fn raster_image_dimensions(bytes: &[u8]) -> Option<(i64, i64)> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Some((
            i64::from(u32::from_be_bytes(bytes.get(16..20)?.try_into().ok()?)),
            i64::from(u32::from_be_bytes(bytes.get(20..24)?.try_into().ok()?)),
        ));
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return Some((
            i64::from(u16::from_le_bytes(bytes.get(6..8)?.try_into().ok()?)),
            i64::from(u16::from_le_bytes(bytes.get(8..10)?.try_into().ok()?)),
        ));
    }
    if bytes.get(..4) == Some(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        return webp_dimensions(bytes).map(|(width, height)| (i64::from(width), i64::from(height)));
    }
    jpeg_dimensions(bytes).map(|(width, height)| (i64::from(width), i64::from(height)))
}

fn jpeg_dimensions(bytes: &[u8]) -> Option<(u16, u16)> {
    if !bytes.starts_with(&[0xff, 0xd8]) {
        return None;
    }
    let mut offset = 2;
    while offset + 9 <= bytes.len() {
        if bytes[offset] != 0xff {
            offset += 1;
            continue;
        }
        while bytes.get(offset) == Some(&0xff) {
            offset += 1;
        }
        let marker = *bytes.get(offset)?;
        offset += 1;
        if marker == 0xd9 || marker == 0xda {
            return None;
        }
        let length = usize::from(u16::from_be_bytes(
            bytes.get(offset..offset + 2)?.try_into().ok()?,
        ));
        if length < 7 || offset.checked_add(length)? > bytes.len() {
            return None;
        }
        if matches!(marker, 0xc0..=0xc3 | 0xc5..=0xc7 | 0xc9..=0xcb | 0xcd..=0xcf) {
            return Some((
                u16::from_be_bytes(bytes.get(offset + 5..offset + 7)?.try_into().ok()?),
                u16::from_be_bytes(bytes.get(offset + 3..offset + 5)?.try_into().ok()?),
            ));
        }
        offset += length;
    }
    None
}

fn webp_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    match bytes.get(12..16)? {
        b"VP8X" => Some((
            u32::from_le_bytes([*bytes.get(24)?, *bytes.get(25)?, *bytes.get(26)?, 0]) + 1,
            u32::from_le_bytes([*bytes.get(27)?, *bytes.get(28)?, *bytes.get(29)?, 0]) + 1,
        )),
        b"VP8 " => {
            if bytes.get(23..26)? != [0x9d, 0x01, 0x2a] {
                return None;
            }
            Some((
                u32::from(u16::from_le_bytes(bytes.get(26..28)?.try_into().ok()?) & 0x3fff),
                u32::from(u16::from_le_bytes(bytes.get(28..30)?.try_into().ok()?) & 0x3fff),
            ))
        }
        b"VP8L" => {
            if bytes.get(20) != Some(&0x2f) {
                return None;
            }
            let packed = u32::from_le_bytes([
                *bytes.get(21)?,
                *bytes.get(22)?,
                *bytes.get(23)?,
                *bytes.get(24)?,
            ]);
            Some(((packed & 0x3fff) + 1, ((packed >> 14) & 0x3fff) + 1))
        }
        _ => None,
    }
}

fn validate_ooxml_container(
    source_path: &Path,
    file_type: UploadFileType,
    limits: UploadValidationLimits,
    deadline: &FileParseDeadline,
) -> Result<(), AppError> {
    let file = File::open(source_path)?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|_| AppError::InvalidInput("Office 文件不是可读取的压缩容器".to_string()))?;
    if archive.len() > limits.max_container_entries {
        return Err(AppError::InvalidInput(format!(
            "Office 文件包含的条目超过 {} 个",
            limits.max_container_entries
        )));
    }
    let mut expanded_size = 0_u64;
    let mut has_content_types = false;
    let mut has_required_entry = false;
    for index in 0..archive.len() {
        deadline.check()?;
        let mut entry = archive
            .by_index(index)
            .map_err(|_| AppError::InvalidInput("Office 文件包含无法读取的压缩条目".to_string()))?;
        let entry_name = entry.name().to_string();
        if entry.encrypted() {
            return Err(AppError::InvalidInput(
                "不支持加密的 Office 文件".to_string(),
            ));
        }
        if entry.enclosed_name().is_none() || entry.is_symlink() {
            return Err(AppError::InvalidInput(
                "Office 文件包含不安全的压缩路径".to_string(),
            ));
        }
        expanded_size = expanded_size
            .checked_add(entry.size())
            .ok_or_else(|| AppError::InvalidInput("Office 文件展开大小超出支持范围".to_string()))?;
        if expanded_size > limits.max_container_expanded_size_bytes {
            return Err(AppError::InvalidInput(format!(
                "Office 文件展开后不能超过 {}MB",
                limits.max_container_expanded_size_bytes / 1024 / 1024
            )));
        }
        if is_nested_container_name(&entry_name) || contains_zip_signature(&mut entry, deadline)? {
            if limits.max_container_depth <= 1 {
                return Err(AppError::InvalidInput(
                    "Office 文件不允许包含嵌套压缩容器".to_string(),
                ));
            }
        }
        has_content_types |= entry_name == "[Content_Types].xml";
        has_required_entry |= file_type.required_ooxml_entry() == Some(entry_name.as_str());
        if entry_name.eq_ignore_ascii_case("word/vbaproject.bin")
            || entry_name.eq_ignore_ascii_case("xl/vbaproject.bin")
            || entry_name.eq_ignore_ascii_case("ppt/vbaproject.bin")
        {
            return Err(AppError::InvalidInput(
                "不支持包含宏的 Office 文件".to_string(),
            ));
        }
    }
    if !has_content_types || !has_required_entry {
        return Err(AppError::InvalidInput(
            "Office 文件缺少与扩展名匹配的必要结构".to_string(),
        ));
    }
    Ok(())
}

fn looks_like_zip(bytes: &[u8]) -> bool {
    matches!(
        bytes.get(..4),
        Some(b"PK\x03\x04" | b"PK\x05\x06" | b"PK\x07\x08")
    )
}

fn looks_like_text(bytes: &[u8]) -> bool {
    let bytes = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes);
    let Ok(content) = std::str::from_utf8(bytes) else {
        return false;
    };
    !content.chars().any(|character| {
        character == '\0' || (character.is_control() && !character.is_whitespace())
    })
}

fn looks_like_html(bytes: &[u8]) -> bool {
    let Ok(content) = std::str::from_utf8(bytes) else {
        return false;
    };
    content
        .trim_start_matches('\u{feff}')
        .trim_start()
        .starts_with('<')
}

fn looks_like_svg(bytes: &[u8]) -> bool {
    let Ok(content) = std::str::from_utf8(bytes) else {
        return false;
    };
    let lower = content
        .trim_start_matches('\u{feff}')
        .trim_start()
        .to_ascii_lowercase();
    lower.starts_with("<svg") || (lower.starts_with("<?xml") && lower.contains("<svg"))
}

fn is_nested_container_name(entry_name: &str) -> bool {
    let lower = entry_name.to_ascii_lowercase();
    [".zip", ".docx", ".xlsx", ".pptx"]
        .iter()
        .any(|extension| lower.ends_with(extension))
}

fn contains_zip_signature<R: Read>(
    entry: &mut zip::read::ZipFile<'_, R>,
    deadline: &FileParseDeadline,
) -> Result<bool, AppError> {
    let mut magic = [0_u8; 4];
    let count = entry.read(&mut magic)?;
    deadline.check()?;
    Ok(count == magic.len() && looks_like_zip(&magic))
}

#[cfg(test)]
mod tests {
    use super::{
        validate_upload_file, validate_upload_file_with_limits, UploadValidationLimits,
        MAX_CONTAINER_DEPTH,
    };
    use std::fs;
    use std::io::Write;
    use std::path::Path;
    use zip::write::SimpleFileOptions;
    use zip::{CompressionMethod, ZipWriter};

    fn test_root(name: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!(
            "tauri-knowledge-upload-validation-{name}-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        fs::create_dir_all(&root).expect("应创建临时目录");
        root
    }
    fn write_ooxml(path: &Path, entries: &[(&str, &[u8])]) {
        let file = fs::File::create(path).expect("应创建测试 Office 文件");
        let mut archive = ZipWriter::new(file);
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        for (name, content) in entries {
            archive.start_file(*name, options).expect("应创建压缩条目");
            archive.write_all(content).expect("应写入压缩条目");
        }
        archive.finish().expect("应完成压缩文件");
    }
    #[test]
    fn validates_binary_signatures_and_rejects_wrong_ooxml_content() {
        let root = test_root("signatures");
        let png = root.join("图片.png");
        fs::write(
            &png,
            b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR\0\0\x04\0\0\0\x03\0fixture",
        )
        .unwrap();
        let validated_png = validate_upload_file(&png, "图片.png").unwrap();
        assert_eq!(validated_png.mime_type, "image/png");
        assert_eq!(validated_png.document_type, "image");
        let pdf = root.join("说明.pdf");
        fs::write(&pdf, b"%PDF-1.7\nfixture").unwrap();
        assert_eq!(
            validate_upload_file(&pdf, "说明.pdf").unwrap().mime_type,
            "application/pdf"
        );
        let fake_docx = root.join("伪造.docx");
        fs::write(&fake_docx, "这不是 Office 文件").unwrap();
        assert!(validate_upload_file(&fake_docx, "伪造.docx").is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn legacy_office_requires_an_explicit_safe_converter() {
        let root = test_root("legacy-office");
        for (name, replacement) in [
            ("历史说明.doc", "DOCX"),
            ("历史清单.xls", "XLSX"),
            ("历史汇报.ppt", "PPTX"),
        ] {
            let path = root.join(name);
            fs::write(&path, b"\xd0\xcf\x11\xe0\xa1\xb1\x1a\xe1legacy-office").unwrap();
            let error = validate_upload_file(&path, name).unwrap_err();
            assert!(error.to_string().contains("暂未配置安全转换器"));
            assert!(error.to_string().contains(replacement));
        }
        let forged = root.join("伪造.doc");
        fs::write(&forged, "not-an-office-document").unwrap();
        assert!(validate_upload_file(&forged, "伪造.doc")
            .unwrap_err()
            .to_string()
            .contains("文件内容与扩展名"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_small_file_with_unsafe_decoded_pixel_count() {
        let root = test_root("image-pixels");
        let image = root.join("超大像素.png");
        fs::write(
            &image,
            b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR\0\0\x4e\x20\0\0\x4e\x20fixture",
        )
        .unwrap();
        let error = validate_upload_file(&image, "超大像素.png").unwrap_err();
        assert!(error.to_string().contains("像素限制"));
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn validates_ooxml_structure_and_container_limits() {
        let root = test_root("container");
        let docx = root.join("设计.docx");
        write_ooxml(
            &docx,
            &[
                ("[Content_Types].xml", b"<Types/>"),
                ("word/document.xml", b"<w:document/>"),
            ],
        );
        assert_eq!(
            validate_upload_file(&docx, "设计.docx")
                .unwrap()
                .document_type,
            "docx"
        );
        let too_many = root.join("条目过多.docx");
        write_ooxml(
            &too_many,
            &[
                ("[Content_Types].xml", b"<Types/>"),
                ("word/document.xml", b"<w:document/>"),
                ("word/extra.xml", b"<x/>"),
            ],
        );
        let limits = UploadValidationLimits {
            max_container_entries: 2,
            ..UploadValidationLimits::default()
        };
        assert!(validate_upload_file_with_limits(&too_many, "条目过多.docx", limits).is_err());
        let limits = UploadValidationLimits {
            max_container_expanded_size_bytes: 4,
            ..UploadValidationLimits::default()
        };
        assert!(validate_upload_file_with_limits(&docx, "设计.docx", limits).is_err());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn rejects_nested_containers_by_name_or_signature() {
        let root = test_root("nested");
        let docx = root.join("嵌套.docx");
        write_ooxml(
            &docx,
            &[
                ("[Content_Types].xml", b"<Types/>"),
                ("word/document.xml", b"<w:document/>"),
                ("word/attachment.zip", b"PK\x03\x04nested"),
            ],
        );
        assert_eq!(MAX_CONTAINER_DEPTH, 1);
        assert!(validate_upload_file(&docx, "嵌套.docx").is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn enforces_file_size_and_cooperative_validation_deadline() {
        let root = test_root("file-size-timeout");
        let markdown = root.join("说明.md");
        fs::write(&markdown, "安全文本").unwrap();
        let size_limits = UploadValidationLimits {
            max_file_size_bytes: 1,
            ..UploadValidationLimits::default()
        };
        assert!(validate_upload_file_with_limits(&markdown, "说明.md", size_limits).is_err());
        let timeout_limits = UploadValidationLimits {
            timeout: std::time::Duration::ZERO,
            ..UploadValidationLimits::default()
        };
        assert!(validate_upload_file_with_limits(&markdown, "说明.md", timeout_limits).is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
