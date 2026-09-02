use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::io::{Cursor, Read};
use std::sync::LazyLock;

use calamine::{DataType, Reader as CalamineReader};
use quick_xml::events::Event;
use quick_xml::Reader;
use regex::Regex;

use crate::error::AppError;
use crate::models::{
    KnowledgeChunkOptions, KnowledgeChunkWriteInput, KnowledgeParseAndChunkInput,
    KnowledgeParseAndChunkResult, KnowledgeParseInput, KnowledgeParsedBlock,
    KnowledgeParsedDocument,
};
use crate::services::knowledge_domain::upload_validation::{
    FileParseDeadline, FILE_PARSE_TIMEOUT, MAX_ASSET_SIZE_BYTES, MAX_CONTAINER_ENTRIES,
    MAX_CONTAINER_EXPANDED_SIZE_BYTES,
};

pub const CONTENT_NORMALIZATION_VERSION: &str = "knowledge-normalize-v1";
pub const STRUCTURE_CHUNK_STRATEGY_ID: &str = "knowledge-structure-chunker-v1";

/// 项目内所有 Markdown 入口共用这一组扩展名，避免同步、代码分析和解析器
/// 对同一个文件做出不同判断。扩展名比较始终不区分大小写。
pub const MARKDOWN_EXTENSIONS: &[&str] = &["md", "mdx", "markdown", "mdown", "mkdn"];

pub fn is_markdown_path(path: &str) -> bool {
    std::path::Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            MARKDOWN_EXTENSIONS.contains(&extension.to_ascii_lowercase().as_str())
        })
}

pub fn is_markdown_mime(mime_type: &str) -> bool {
    matches!(
        mime_type.trim().to_ascii_lowercase().as_str(),
        "text/markdown" | "text/x-markdown" | "application/markdown"
    )
}

const DEFAULT_TARGET_CHARS: usize = 1_800;
const DEFAULT_MAX_CHARS: usize = 2_600;
const DEFAULT_OVERLAP_CHARS: usize = 200;

type DocxBodyParseResult = (Vec<KnowledgeParsedBlock>, Vec<String>, HashSet<String>);

/// 文档解析器边界。具体格式实现只负责产出稳定结构块，不直接访问数据库。
pub trait KnowledgeParser: Sync {
    fn parser_id(&self) -> &'static str;
    /// 仅根据已验证的 MIME/扩展名选择解析器；内容签名与配额检查由后续接入层统一负责。
    fn supports(&self, input: &KnowledgeParseInput) -> bool;
    fn parse(&self, input: &KnowledgeParseInput) -> Result<KnowledgeParsedDocument, AppError>;
}

/// 分块器边界。输入必须是已经规范化的结构块，输出可直接交给 Database 层事务写入。
pub trait KnowledgeChunker {
    fn strategy_id(&self) -> &'static str;
    fn chunk(
        &self,
        parsed: &KnowledgeParsedDocument,
        options: Option<&KnowledgeChunkOptions>,
    ) -> Result<Vec<KnowledgeChunkWriteInput>, AppError>;
}

pub struct KnowledgeParserService;

impl KnowledgeParserService {
    pub fn parse_and_chunk(
        input: KnowledgeParseAndChunkInput,
    ) -> Result<KnowledgeParseAndChunkResult, AppError> {
        // 解析器自身不接触文件系统，但仍遵守与上传容器一致的 30 秒协作式截止时间；
        // 后续 Office/HTML 适配器会在每个条目或 DOM 遍历边界继续检查该截止时间。
        let deadline = FileParseDeadline::new(FILE_PARSE_TIMEOUT);
        let parser = default_parser_registry().resolve(&input.document)?;
        deadline.check()?;
        let parsed = parser.parse(&input.document)?;
        deadline.check()?;
        let chunker = StructureAwareChunker;
        let chunks = chunker.chunk(&parsed, input.options.as_ref())?;
        deadline.check()?;
        Ok(KnowledgeParseAndChunkResult {
            parsed,
            chunk_strategy_id: chunker.strategy_id().to_string(),
            chunks,
        })
    }
}

/// 有序解析器注册表。顺序即兼容优先级，必须保持与旧的格式分支完全一致。
pub struct KnowledgeParserRegistry {
    parsers: &'static [&'static dyn KnowledgeParser],
}

impl KnowledgeParserRegistry {
    pub fn new(parsers: &'static [&'static dyn KnowledgeParser]) -> Self {
        Self { parsers }
    }

    pub fn resolve(
        &self,
        input: &KnowledgeParseInput,
    ) -> Result<&'static dyn KnowledgeParser, AppError> {
        self.parsers
            .iter()
            .copied()
            .find(|parser| parser.supports(input))
            .ok_or_else(|| {
                AppError::InvalidInput(format!(
                    "不支持的知识文档格式: mimeType='{}', path='{}'",
                    input.mime_type, input.source_path
                ))
            })
    }
}

struct MarkdownParser;
struct DocxParser;
struct XlsxParser;
struct PptxParser;
struct PdfParser;
struct HtmlParser;
struct ImageMetadataParser;
struct ImageLocalOcrParser;
struct ImageOcrParser;
struct TextParser {
    log_mode: bool,
}
struct SqlParser;
struct JsonParser;
struct YamlParser;
struct StructureAwareChunker;

static MARKDOWN_PARSER: MarkdownParser = MarkdownParser;
static DOCX_PARSER: DocxParser = DocxParser;
static XLSX_PARSER: XlsxParser = XlsxParser;
static PPTX_PARSER: PptxParser = PptxParser;
static PDF_PARSER: PdfParser = PdfParser;
static HTML_PARSER: HtmlParser = HtmlParser;
static IMAGE_METADATA_PARSER: ImageMetadataParser = ImageMetadataParser;
static IMAGE_LOCAL_OCR_PARSER: ImageLocalOcrParser = ImageLocalOcrParser;
static IMAGE_OCR_PARSER: ImageOcrParser = ImageOcrParser;
static JSON_PARSER: JsonParser = JsonParser;
static YAML_PARSER: YamlParser = YamlParser;
static SQL_PARSER: SqlParser = SqlParser;
static LOG_PARSER: TextParser = TextParser { log_mode: true };
static TEXT_PARSER: TextParser = TextParser { log_mode: false };
static DEFAULT_PARSERS: [&dyn KnowledgeParser; 14] = [
    &MARKDOWN_PARSER,
    &DOCX_PARSER,
    &XLSX_PARSER,
    &PPTX_PARSER,
    &PDF_PARSER,
    &HTML_PARSER,
    &IMAGE_METADATA_PARSER,
    &IMAGE_LOCAL_OCR_PARSER,
    &IMAGE_OCR_PARSER,
    &JSON_PARSER,
    &YAML_PARSER,
    &SQL_PARSER,
    &LOG_PARSER,
    &TEXT_PARSER,
];

/// 图片二进制只在受控后端内读取。元数据既用于无 OCR 时的可解释索引，也用于受控预览，
/// 因此不能依赖文件名或前端传入的尺寸。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnowledgeImageMetadata {
    pub width: Option<i64>,
    pub height: Option<i64>,
}

pub fn inspect_image_metadata(mime_type: &str, bytes: &[u8]) -> KnowledgeImageMetadata {
    let (width, height) = match mime_type.trim().to_ascii_lowercase().as_str() {
        "image/png" => png_dimensions(bytes),
        "image/jpeg" => jpeg_dimensions(bytes),
        "image/gif" => gif_dimensions(bytes),
        "image/webp" => webp_dimensions(bytes),
        "image/svg+xml" => svg_dimensions(bytes),
        _ => None,
    }
    .unwrap_or((None, None));
    KnowledgeImageMetadata { width, height }
}

static HTML_TITLE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?is)<title\b[^>]*>(?P<title>.*?)</title\s*>").expect("HTML 标题正则必须有效")
});
static HTML_HEAD_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?is)<head\b[^>]*>.*?</head\s*>").expect("HTML 头部正则必须有效")
});
static HTML_TAG_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?is)<[^>]+>").expect("HTML 标签正则必须有效"));
static HTML_WHITESPACE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\s+").expect("空白正则必须有效"));
static HTML_HEADING_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?is)<h(?P<level>[1-6])\b[^>]*>(?P<content>.*?)</h[1-6]\s*>")
        .expect("HTML 标题层级正则必须有效")
});
static HTML_CONTROL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?is)<(?P<tag>input|button|select|textarea)\b(?P<attributes>[^>]*)>")
        .expect("HTML 控件正则必须有效")
});
static HTML_ATTRIBUTE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?is)\b(?P<name>type|name|value|placeholder|aria-label)\s*=\s*[\"'](?P<value>[^\"']*)[\"']"#)
        .expect("HTML 属性正则必须有效")
});
static HTML_RESOURCE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?is)\b(?:href|src)\s*=\s*[\"'](?P<value>[^\"']+)[\"']"#)
        .expect("HTML 资源正则必须有效")
});

fn default_parser_registry() -> KnowledgeParserRegistry {
    KnowledgeParserRegistry::new(&DEFAULT_PARSERS)
}

fn document_format(input: &KnowledgeParseInput) -> (String, String) {
    let mime = input.mime_type.trim().to_lowercase();
    let extension = std::path::Path::new(input.source_path.trim())
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_lowercase();
    (mime, extension)
}

fn png_dimensions(bytes: &[u8]) -> Option<(Option<i64>, Option<i64>)> {
    let header = bytes.get(16..24)?;
    let width = u32::from_be_bytes(header.get(..4)?.try_into().ok()?);
    let height = u32::from_be_bytes(header.get(4..)?.try_into().ok()?);
    non_zero_dimensions(width, height)
}

fn gif_dimensions(bytes: &[u8]) -> Option<(Option<i64>, Option<i64>)> {
    let width = u16::from_le_bytes(bytes.get(6..8)?.try_into().ok()?);
    let height = u16::from_le_bytes(bytes.get(8..10)?.try_into().ok()?);
    non_zero_dimensions(u32::from(width), u32::from(height))
}

fn jpeg_dimensions(bytes: &[u8]) -> Option<(Option<i64>, Option<i64>)> {
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
            let height = u16::from_be_bytes(bytes.get(offset + 3..offset + 5)?.try_into().ok()?);
            let width = u16::from_be_bytes(bytes.get(offset + 5..offset + 7)?.try_into().ok()?);
            return non_zero_dimensions(u32::from(width), u32::from(height));
        }
        offset += length;
    }
    None
}

fn webp_dimensions(bytes: &[u8]) -> Option<(Option<i64>, Option<i64>)> {
    if bytes.get(..4) != Some(b"RIFF") || bytes.get(8..12) != Some(b"WEBP") {
        return None;
    }
    match bytes.get(12..16)? {
        b"VP8X" => {
            let width =
                u32::from_le_bytes([*bytes.get(24)?, *bytes.get(25)?, *bytes.get(26)?, 0]) + 1;
            let height =
                u32::from_le_bytes([*bytes.get(27)?, *bytes.get(28)?, *bytes.get(29)?, 0]) + 1;
            non_zero_dimensions(width, height)
        }
        b"VP8 " => {
            if bytes.get(23..26)? != [0x9d, 0x01, 0x2a] {
                return None;
            }
            let width = u16::from_le_bytes(bytes.get(26..28)?.try_into().ok()?) & 0x3fff;
            let height = u16::from_le_bytes(bytes.get(28..30)?.try_into().ok()?) & 0x3fff;
            non_zero_dimensions(u32::from(width), u32::from(height))
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
            let width = (packed & 0x3fff) + 1;
            let height = ((packed >> 14) & 0x3fff) + 1;
            non_zero_dimensions(width, height)
        }
        _ => None,
    }
}

fn svg_dimensions(bytes: &[u8]) -> Option<(Option<i64>, Option<i64>)> {
    let source = std::str::from_utf8(bytes).ok()?;
    let svg_start = source.find("<svg")?;
    let svg_end = source[svg_start..].find('>')? + svg_start;
    let tag = &source[svg_start..=svg_end];
    let width = svg_numeric_attribute(tag, "width");
    let height = svg_numeric_attribute(tag, "height");
    if width.is_some() && height.is_some() {
        return Some((width, height));
    }
    let view_box = svg_attribute(tag, "viewBox")?;
    let values = view_box
        .split(|character: char| character.is_ascii_whitespace() || character == ',')
        .filter_map(|value| value.parse::<f64>().ok())
        .collect::<Vec<_>>();
    (values.len() == 4).then(|| {
        (
            positive_f64_to_i64(values[2]),
            positive_f64_to_i64(values[3]),
        )
    })
}

fn svg_numeric_attribute(tag: &str, name: &str) -> Option<i64> {
    let value = svg_attribute(tag, name)?;
    let digits = value
        .trim()
        .trim_end_matches(|character: char| character.is_ascii_alphabetic() || character == '%');
    positive_f64_to_i64(digits.parse::<f64>().ok()?)
}

fn svg_attribute<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let pattern = format!("{name}=");
    let start = tag.find(&pattern)? + pattern.len();
    let quote = *tag.as_bytes().get(start)?;
    if !matches!(quote, b'\'' | b'\"') {
        return None;
    }
    let value_start = start + 1;
    let value_end = tag[value_start..].find(quote as char)? + value_start;
    Some(&tag[value_start..value_end])
}

fn non_zero_dimensions(width: u32, height: u32) -> Option<(Option<i64>, Option<i64>)> {
    (width > 0 && height > 0).then_some((Some(i64::from(width)), Some(i64::from(height))))
}

fn positive_f64_to_i64(value: f64) -> Option<i64> {
    (value.is_finite() && value > 0.0 && value <= i64::MAX as f64).then_some(value.round() as i64)
}

impl KnowledgeParser for MarkdownParser {
    fn parser_id(&self) -> &'static str {
        "markdown-parser-v1"
    }

    fn supports(&self, input: &KnowledgeParseInput) -> bool {
        let (mime, extension) = document_format(input);
        is_markdown_mime(&mime)
            || MARKDOWN_EXTENSIONS
                .iter()
                .any(|candidate| *candidate == extension)
    }

    fn parse(&self, input: &KnowledgeParseInput) -> Result<KnowledgeParsedDocument, AppError> {
        let normalized = normalize_content(&input.content);
        let lines = normalized.lines().collect::<Vec<_>>();
        let mut blocks = Vec::new();
        let mut warnings = Vec::new();
        let mut front_matter = serde_json::json!({});
        let mut index = 0_usize;

        if lines.first().is_some_and(|line| line.trim() == "---") {
            let closing = lines
                .iter()
                .enumerate()
                .skip(1)
                .find(|(_, line)| line.trim() == "---")
                .map(|(line_index, _)| line_index);
            if let Some(closing) = closing {
                let raw = lines[1..closing].join("\n");
                let raw_block = || KnowledgeParsedBlock {
                    block_type: "front_matter_raw".to_string(),
                    heading_path: Vec::new(),
                    content: lines[..=closing].join("\n"),
                    start_line: 1,
                    end_line: usize_to_i64(closing + 1),
                    metadata: serde_json::json!({"valid": false}),
                };
                match parse_markdown_front_matter(&raw) {
                    Ok(yaml) => match serde_json::to_value(yaml) {
                        Ok(value) => {
                            front_matter = value;
                            blocks.push(KnowledgeParsedBlock {
                                block_type: "front_matter".to_string(),
                                heading_path: Vec::new(),
                                content: lines[..=closing].join("\n"),
                                start_line: 1,
                                end_line: usize_to_i64(closing + 1),
                                metadata: front_matter.clone(),
                            });
                        }
                        Err(error) => {
                            warnings.push(format!(
                                "Markdown front matter 转换失败，已按原文保留：{error}"
                            ));
                            blocks.push(raw_block());
                        }
                    },
                    Err(error) => {
                        warnings.push(format!(
                            "Markdown front matter 解析失败，已按原文保留：{error}"
                        ));
                        blocks.push(raw_block());
                    }
                }
                index = closing + 1;
            } else {
                // 历史资料偶尔只有 front matter 的起始分隔符。保留原文作为普通
                // Markdown 内容，而不是让单个文档阻断整个版本的回填。
                warnings.push(
                    "Markdown front matter 缺少结束分隔符，已按普通 Markdown 解析".to_string(),
                );
            }
        }

        let mut headings = Vec::<String>::new();
        while index < lines.len() {
            if lines[index].trim().is_empty() {
                index += 1;
                continue;
            }
            if let Some((level, title)) = markdown_heading(lines[index]) {
                headings.truncate(level.saturating_sub(1));
                headings.push(title.to_string());
                blocks.push(block(
                    "heading",
                    &headings,
                    lines[index].to_string(),
                    index,
                    index,
                    serde_json::json!({"level": level}),
                ));
                index += 1;
                continue;
            }
            if let Some((marker, language)) = markdown_fence(lines[index]) {
                let start = index;
                index += 1;
                while index < lines.len() && !lines[index].trim_start().starts_with(marker) {
                    index += 1;
                }
                if index >= lines.len() {
                    // 历史仓库中的未闭合围栏不应阻断整批知识回填。将 EOF 视为闭合点，
                    // 保留原始代码内容和可追溯的行号，并把格式问题写入解析警告。
                    warnings.push(format!(
                        "Markdown 代码块未闭合，已将文件结尾作为结束位置（起始行 {}）",
                        start + 1
                    ));
                    blocks.push(block(
                        "code_block",
                        &headings,
                        lines[start..].join("\n"),
                        start,
                        lines.len().saturating_sub(1),
                        serde_json::json!({"language": language, "closed": false}),
                    ));
                    break;
                }
                blocks.push(block(
                    "code_block",
                    &headings,
                    lines[start..=index].join("\n"),
                    start,
                    index,
                    serde_json::json!({"language": language}),
                ));
                index += 1;
                continue;
            }
            if is_markdown_table_start(&lines, index) {
                let start = index;
                index += 2;
                while index < lines.len()
                    && !lines[index].trim().is_empty()
                    && lines[index].contains('|')
                {
                    index += 1;
                }
                blocks.push(block(
                    "table",
                    &headings,
                    lines[start..index].join("\n"),
                    start,
                    index.saturating_sub(1),
                    serde_json::json!({"columns": table_column_count(lines[start])}),
                ));
                continue;
            }

            let start = index;
            index += 1;
            while index < lines.len()
                && !lines[index].trim().is_empty()
                && markdown_heading(lines[index]).is_none()
                && markdown_fence(lines[index]).is_none()
                && !is_markdown_table_start(&lines, index)
            {
                index += 1;
            }
            blocks.push(block(
                "paragraph",
                &headings,
                lines[start..index].join("\n"),
                start,
                index.saturating_sub(1),
                serde_json::json!({}),
            ));
        }

        Ok(KnowledgeParsedDocument {
            parser_id: self.parser_id().to_string(),
            normalization_version: CONTENT_NORMALIZATION_VERSION.to_string(),
            normalized_content: normalized,
            front_matter,
            blocks,
            warnings,
        })
    }
}

/// 部分转换工具会把 Markdown 链接直接写进 YAML 标量，例如
/// `source: [原件.docx](<../原件.docx>)`。这不是合法 YAML，但语义明确；仅在严格
/// 解析失败后引用这类标量，既兼容已有资料，也不会放宽其他 Front Matter 错误。
fn parse_markdown_front_matter(raw: &str) -> Result<serde_yaml::Value, serde_yaml::Error> {
    match serde_yaml::from_str::<serde_yaml::Value>(raw) {
        Ok(value) => Ok(value),
        Err(original_error) => {
            let normalized = quote_unquoted_markdown_link_scalars(raw);
            if normalized == raw {
                return Err(original_error);
            }
            serde_yaml::from_str::<serde_yaml::Value>(&normalized).map_err(|_| original_error)
        }
    }
}

fn quote_unquoted_markdown_link_scalars(raw: &str) -> String {
    raw.lines()
        .map(|line| {
            let Some((key, raw_value)) = line.split_once(':') else {
                return line.to_string();
            };
            let value = raw_value.trim();
            if key.trim().is_empty()
                || !value.starts_with('[')
                || !value.contains("](")
                || !value.ends_with(')')
            {
                return line.to_string();
            }
            let leading_whitespace = &raw_value[..raw_value.len() - raw_value.trim_start().len()];
            let escaped = value.replace('\'', "''");
            format!("{key}:{leading_whitespace}'{escaped}'")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

impl KnowledgeParser for DocxParser {
    fn parser_id(&self) -> &'static str {
        "docx-parser-v1"
    }

    fn supports(&self, input: &KnowledgeParseInput) -> bool {
        let (mime, extension) = document_format(input);
        mime == "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
            || extension == "docx"
    }

    fn parse(&self, input: &KnowledgeParseInput) -> Result<KnowledgeParsedDocument, AppError> {
        let bytes = input.binary_content.as_deref().ok_or_else(|| {
            AppError::InvalidInput("DOCX 解析需要受控上传路径提供的二进制内容".to_string())
        })?;
        parse_docx_document(self.parser_id(), bytes)
    }
}

impl KnowledgeParser for XlsxParser {
    fn parser_id(&self) -> &'static str {
        "xlsx-parser-v1"
    }

    fn supports(&self, input: &KnowledgeParseInput) -> bool {
        let (mime, extension) = document_format(input);
        mime == "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
            || extension == "xlsx"
    }

    fn parse(&self, input: &KnowledgeParseInput) -> Result<KnowledgeParsedDocument, AppError> {
        let bytes = input.binary_content.as_deref().ok_or_else(|| {
            AppError::InvalidInput("XLSX 解析需要受控上传路径提供的二进制内容".to_string())
        })?;
        parse_xlsx_document(self.parser_id(), bytes)
    }
}

impl KnowledgeParser for PptxParser {
    fn parser_id(&self) -> &'static str {
        "pptx-parser-v1"
    }

    fn supports(&self, input: &KnowledgeParseInput) -> bool {
        let (mime, extension) = document_format(input);
        mime == "application/vnd.openxmlformats-officedocument.presentationml.presentation"
            || extension == "pptx"
    }

    fn parse(&self, input: &KnowledgeParseInput) -> Result<KnowledgeParsedDocument, AppError> {
        let bytes = input.binary_content.as_deref().ok_or_else(|| {
            AppError::InvalidInput("PPTX 解析需要受控上传路径提供的二进制内容".to_string())
        })?;
        parse_pptx_document(self.parser_id(), bytes)
    }
}

impl KnowledgeParser for PdfParser {
    fn parser_id(&self) -> &'static str {
        "pdf-parser-v1"
    }

    fn supports(&self, input: &KnowledgeParseInput) -> bool {
        let (mime, extension) = document_format(input);
        mime == "application/pdf" || extension == "pdf"
    }

    fn parse(&self, input: &KnowledgeParseInput) -> Result<KnowledgeParsedDocument, AppError> {
        let bytes = input.binary_content.as_deref().ok_or_else(|| {
            AppError::InvalidInput("PDF 解析需要受控上传路径提供的二进制内容".to_string())
        })?;
        parse_pdf_document(self.parser_id(), bytes)
    }
}

impl KnowledgeParser for HtmlParser {
    fn parser_id(&self) -> &'static str {
        "html-parser-v1"
    }

    fn supports(&self, input: &KnowledgeParseInput) -> bool {
        let (mime, extension) = document_format(input);
        mime == "text/html" || matches!(extension.as_str(), "html" | "htm")
    }

    fn parse(&self, input: &KnowledgeParseInput) -> Result<KnowledgeParsedDocument, AppError> {
        let title = HTML_TITLE_RE
            .captures(&input.content)
            .and_then(|captures| captures.name("title"))
            .map(|capture| visible_html_text(capture.as_str()))
            .filter(|title| !title.is_empty());
        // 标题单独保存到元数据，且不将 head、script、style 等非可见内容混入正文。
        let without_head = HTML_HEAD_RE.replace(&input.content, "");
        let sanitized = sanitize_html(&without_head);
        let visible_text = visible_html_text(&sanitized);
        let mut blocks = Vec::new();
        if let Some(title) = title.as_ref() {
            blocks.push(html_block(
                "title",
                title,
                &input.content,
                serde_json::json!({}),
            ));
        }
        for captures in HTML_HEADING_RE.captures_iter(&sanitized) {
            let content =
                visible_html_text(captures.name("content").map_or("", |value| value.as_str()));
            if content.is_empty() {
                continue;
            }
            let level = captures
                .name("level")
                .and_then(|value| value.as_str().parse::<i64>().ok())
                .unwrap_or(1);
            blocks.push(html_block(
                "heading",
                &content,
                &input.content,
                serde_json::json!({"level": level}),
            ));
        }
        if !visible_text.is_empty() {
            blocks.push(html_block(
                "paragraph",
                &visible_text,
                &input.content,
                serde_json::json!({"visible": true}),
            ));
        }
        for captures in HTML_CONTROL_RE.captures_iter(&sanitized) {
            let tag = captures.name("tag").map_or("", |value| value.as_str());
            let attributes = captures
                .name("attributes")
                .map_or("", |value| value.as_str());
            let metadata = html_control_metadata(tag, attributes);
            let label = metadata
                .get("label")
                .and_then(serde_json::Value::as_str)
                .filter(|value| !value.is_empty())
                .unwrap_or(tag)
                .to_string();
            blocks.push(html_block("control", &label, &input.content, metadata));
        }
        for reference in HTML_RESOURCE_RE
            .captures_iter(&sanitized)
            .filter_map(|captures| captures.name("value"))
            .map(|value| value.as_str().trim())
            .filter(|value| is_safe_relative_resource(value))
        {
            blocks.push(html_block(
                "resource_reference",
                reference,
                &input.content,
                serde_json::json!({"relative": true}),
            ));
        }
        Ok(parsed_document(
            self.parser_id(),
            visible_text,
            serde_json::json!({"title": title}),
            blocks,
        ))
    }
}

impl KnowledgeParser for TextParser {
    fn parser_id(&self) -> &'static str {
        if self.log_mode {
            "log-parser-v1"
        } else {
            "text-parser-v1"
        }
    }

    fn supports(&self, input: &KnowledgeParseInput) -> bool {
        let (mime, extension) = document_format(input);
        if self.log_mode {
            extension == "log"
        } else {
            mime.starts_with("text/") || extension == "txt"
        }
    }

    fn parse(&self, input: &KnowledgeParseInput) -> Result<KnowledgeParsedDocument, AppError> {
        let normalized = normalize_content(&input.content);
        let lines = normalized.lines().collect::<Vec<_>>();
        let mut blocks = Vec::new();
        if self.log_mode {
            for (index, line) in lines.iter().enumerate() {
                if !line.trim().is_empty() {
                    blocks.push(block(
                        "log_line",
                        &[],
                        (*line).to_string(),
                        index,
                        index,
                        serde_json::json!({}),
                    ));
                }
            }
        } else {
            let mut index = 0_usize;
            while index < lines.len() {
                while index < lines.len() && lines[index].trim().is_empty() {
                    index += 1;
                }
                if index >= lines.len() {
                    break;
                }
                let start = index;
                while index < lines.len() && !lines[index].trim().is_empty() {
                    index += 1;
                }
                blocks.push(block(
                    "paragraph",
                    &[],
                    lines[start..index].join("\n"),
                    start,
                    index.saturating_sub(1),
                    serde_json::json!({}),
                ));
            }
        }
        Ok(parsed_document(
            self.parser_id(),
            normalized,
            serde_json::json!({}),
            blocks,
        ))
    }
}

/// 图片未得到 OCR 正文时仍是可管理的项目资料：保留由受控二进制读取的类型、尺寸和
/// “未提取正文”事实，使其可按标题和元数据查找，但绝不把图片假装成已经全文索引。
impl KnowledgeParser for ImageMetadataParser {
    fn parser_id(&self) -> &'static str {
        "image-metadata-parser-v1"
    }

    fn supports(&self, input: &KnowledgeParseInput) -> bool {
        input
            .mime_type
            .trim()
            .to_ascii_lowercase()
            .starts_with("image/")
    }

    fn parse(&self, input: &KnowledgeParseInput) -> Result<KnowledgeParsedDocument, AppError> {
        let bytes = input
            .binary_content
            .as_deref()
            .ok_or_else(|| AppError::InvalidInput("图片解析缺少受控二进制内容".to_string()))?;
        let metadata = inspect_image_metadata(&input.mime_type, bytes);
        let dimensions = match (metadata.width, metadata.height) {
            (Some(width), Some(height)) => format!("{width} × {height}"),
            _ => "未识别".to_string(),
        };
        let content = format!(
            "图片文件：{}\n类型：{}\n尺寸：{}\n文字提取：未获得 OCR 正文",
            input.source_path, input.mime_type, dimensions
        );
        let mut parsed = parsed_document(
            self.parser_id(),
            content.clone(),
            serde_json::json!({
                "assetKind": "image",
                "mimeType": input.mime_type,
                "width": metadata.width,
                "height": metadata.height,
                "textExtraction": "unavailable",
            }),
            vec![block(
                "image_metadata",
                &[],
                content,
                0,
                3,
                serde_json::json!({
                    "mimeType": input.mime_type,
                    "width": metadata.width,
                    "height": metadata.height,
                    "textExtraction": "unavailable",
                }),
            )],
        );
        parsed
            .warnings
            .push("未提取图片文字，当前仅支持标题和元数据搜索".to_string());
        Ok(parsed)
    }
}

/// 本机 OCR 的输出与远程识别分开解析，避免把设备能力或本机处理误标为远程发送。
impl KnowledgeParser for ImageLocalOcrParser {
    fn parser_id(&self) -> &'static str {
        "local-image-ocr-parser-v1"
    }

    fn supports(&self, input: &KnowledgeParseInput) -> bool {
        input
            .mime_type
            .eq_ignore_ascii_case("application/x-knowledge-local-ocr")
    }

    fn parse(&self, input: &KnowledgeParseInput) -> Result<KnowledgeParsedDocument, AppError> {
        if input.content.trim().is_empty() {
            return Err(AppError::InvalidInput(
                "本机 OCR 未识别到可索引文字".to_string(),
            ));
        }
        let mut parsed = TextParser { log_mode: false }.parse(&KnowledgeParseInput {
            source_path: input.source_path.clone(),
            mime_type: "text/plain".to_string(),
            content: input.content.clone(),
            binary_content: None,
        })?;
        parsed.parser_id = self.parser_id().to_string();
        parsed.front_matter = serde_json::json!({"recognition": "local_ocr"});
        Ok(parsed)
    }
}

/// 远程 OCR 的返回文本不会再被当作原始图片解析。该受控 MIME 只能由后端导入任务
/// 构造，便于在解析产物中区分“原文提取”和“远程识别”。
impl KnowledgeParser for ImageOcrParser {
    fn parser_id(&self) -> &'static str {
        "remote-image-ocr-parser-v1"
    }

    fn supports(&self, input: &KnowledgeParseInput) -> bool {
        input
            .mime_type
            .eq_ignore_ascii_case("application/x-knowledge-ocr")
    }

    fn parse(&self, input: &KnowledgeParseInput) -> Result<KnowledgeParsedDocument, AppError> {
        if input.content.trim().is_empty() {
            return Err(AppError::InvalidInput(
                "图片 OCR 未识别到可索引文字".to_string(),
            ));
        }
        let mut parsed = TextParser { log_mode: false }.parse(&KnowledgeParseInput {
            source_path: input.source_path.clone(),
            mime_type: "text/plain".to_string(),
            content: input.content.clone(),
            binary_content: None,
        })?;
        parsed.parser_id = self.parser_id().to_string();
        parsed.front_matter = serde_json::json!({"recognition": "remote_ocr"});
        Ok(parsed)
    }
}

impl KnowledgeParser for SqlParser {
    fn parser_id(&self) -> &'static str {
        "sql-parser-v1"
    }

    fn supports(&self, input: &KnowledgeParseInput) -> bool {
        let (mime, extension) = document_format(input);
        matches!(mime.as_str(), "application/sql" | "text/sql") || extension == "sql"
    }

    fn parse(&self, input: &KnowledgeParseInput) -> Result<KnowledgeParsedDocument, AppError> {
        let normalized = normalize_content(&input.content);
        let statements = split_sql_statements(&normalized)?;
        let blocks = statements
            .into_iter()
            .map(|statement| {
                block(
                    "sql_statement",
                    &[],
                    statement.content,
                    statement.start_line,
                    statement.end_line,
                    serde_json::json!({}),
                )
            })
            .collect();
        Ok(parsed_document(
            self.parser_id(),
            normalized,
            serde_json::json!({}),
            blocks,
        ))
    }
}

impl KnowledgeParser for JsonParser {
    fn parser_id(&self) -> &'static str {
        "json-parser-v1"
    }

    fn supports(&self, input: &KnowledgeParseInput) -> bool {
        let (mime, extension) = document_format(input);
        matches!(mime.as_str(), "application/json" | "text/json") || extension == "json"
    }

    fn parse(&self, input: &KnowledgeParseInput) -> Result<KnowledgeParsedDocument, AppError> {
        let normalized = normalize_content(&input.content);
        let value = serde_json::from_str::<serde_json::Value>(&normalized)
            .map_err(|error| AppError::InvalidInput(format!("JSON 解析失败: {error}")))?;
        let end_line = normalized.lines().count().saturating_sub(1);
        let mut blocks = Vec::new();
        match &value {
            serde_json::Value::Object(map) => {
                for (key, child) in map {
                    blocks.push(block(
                        "json_member",
                        std::slice::from_ref(key),
                        serde_json::to_string_pretty(child)?,
                        0,
                        end_line,
                        serde_json::json!({"jsonPointer": format!("/{}", escape_json_pointer(key))}),
                    ));
                }
            }
            serde_json::Value::Array(values) => {
                for (index, child) in values.iter().enumerate() {
                    blocks.push(block(
                        "json_item",
                        &[index.to_string()],
                        serde_json::to_string_pretty(child)?,
                        0,
                        end_line,
                        serde_json::json!({"jsonPointer": format!("/{index}")}),
                    ));
                }
            }
            _ => blocks.push(block(
                "json_value",
                &[],
                serde_json::to_string_pretty(&value)?,
                0,
                end_line,
                serde_json::json!({"jsonPointer": ""}),
            )),
        }
        Ok(parsed_document(
            self.parser_id(),
            normalized,
            serde_json::json!({}),
            blocks,
        ))
    }
}

impl KnowledgeParser for YamlParser {
    fn parser_id(&self) -> &'static str {
        "yaml-parser-v1"
    }

    fn supports(&self, input: &KnowledgeParseInput) -> bool {
        let (mime, extension) = document_format(input);
        matches!(
            mime.as_str(),
            "application/yaml" | "application/x-yaml" | "text/yaml"
        ) || matches!(extension.as_str(), "yaml" | "yml")
    }

    fn parse(&self, input: &KnowledgeParseInput) -> Result<KnowledgeParsedDocument, AppError> {
        let normalized = normalize_content(&input.content);
        let mut document_count = 0_usize;
        for document in serde_yaml::Deserializer::from_str(&normalized) {
            serde_yaml::Value::deserialize(document)
                .map_err(|error| AppError::InvalidInput(format!("YAML 解析失败: {error}")))?;
            document_count += 1;
        }
        if document_count == 0 {
            return Err(AppError::InvalidInput("YAML 文档为空".to_string()));
        }

        let lines = normalized.lines().collect::<Vec<_>>();
        let mut blocks = Vec::new();
        let mut index = 0_usize;
        while index < lines.len() {
            while index < lines.len()
                && (lines[index].trim().is_empty() || lines[index].trim() == "---")
            {
                index += 1;
            }
            if index >= lines.len() {
                break;
            }
            let start = index;
            let heading = yaml_top_level_key(lines[index]);
            index += 1;
            while index < lines.len()
                && lines[index].trim() != "---"
                && (lines[index].starts_with(' ')
                    || lines[index].starts_with('-')
                    || yaml_top_level_key(lines[index]).is_none())
            {
                index += 1;
            }
            let heading_path = heading.into_iter().collect::<Vec<_>>();
            blocks.push(block(
                "yaml_node",
                &heading_path,
                lines[start..index].join("\n"),
                start,
                index.saturating_sub(1),
                serde_json::json!({}),
            ));
        }
        Ok(parsed_document(
            self.parser_id(),
            normalized,
            serde_json::json!({"documentCount": document_count}),
            blocks,
        ))
    }
}

fn parse_pdf_document(parser_id: &str, bytes: &[u8]) -> Result<KnowledgeParsedDocument, AppError> {
    if bytes.len() > MAX_ASSET_SIZE_BYTES as usize {
        return Err(AppError::InvalidInput(
            "PDF 文件超过可解析大小限制".to_string(),
        ));
    }
    let deadline = FileParseDeadline::new(FILE_PARSE_TIMEOUT);
    let document = lopdf::Document::load_mem(bytes)
        .map_err(|_| AppError::InvalidInput("PDF 无法读取、已损坏或需要密码".to_string()))?;
    let pages = document.get_pages();
    let mut blocks = Vec::new();
    let mut normalized_pages = Vec::new();
    let mut warnings = Vec::new();
    for page_number in pages.keys().copied() {
        deadline.check()?;
        let text = document
            .extract_text(&[page_number])
            .map(|text| normalize_content(&text))
            .unwrap_or_default();
        if text.trim().is_empty() {
            let message = format!("第 {page_number} 页需要 OCR");
            warnings.push(message.clone());
            blocks.push(KnowledgeParsedBlock {
                block_type: "ocr_required".to_string(),
                heading_path: vec![format!("第 {page_number} 页")],
                content: message,
                start_line: i64::from(page_number),
                end_line: i64::from(page_number),
                metadata: serde_json::json!({"pageNumber": page_number, "requiresOcr": true}),
            });
            continue;
        }
        normalized_pages.push(format!("[第 {page_number} 页]\n{text}"));
        blocks.push(KnowledgeParsedBlock {
            block_type: "pdf_page".to_string(),
            heading_path: vec![format!("第 {page_number} 页")],
            content: text,
            start_line: i64::from(page_number),
            end_line: i64::from(page_number),
            metadata: serde_json::json!({"pageNumber": page_number, "requiresOcr": false}),
        });
    }
    Ok(KnowledgeParsedDocument {
        parser_id: parser_id.to_string(),
        normalization_version: CONTENT_NORMALIZATION_VERSION.to_string(),
        normalized_content: normalize_content(&normalized_pages.join("\n\n")),
        front_matter: serde_json::json!({"pageCount": pages.len()}),
        blocks,
        warnings,
    })
}

fn parse_pptx_document(parser_id: &str, bytes: &[u8]) -> Result<KnowledgeParsedDocument, AppError> {
    if bytes.len() > MAX_ASSET_SIZE_BYTES as usize {
        return Err(AppError::InvalidInput(
            "PPTX 文件超过可解析大小限制".to_string(),
        ));
    }
    let deadline = FileParseDeadline::new(FILE_PARSE_TIMEOUT);
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))
        .map_err(|_| AppError::InvalidInput("PPTX 不是可读取的 Office 压缩容器".to_string()))?;
    if archive.len() > MAX_CONTAINER_ENTRIES {
        return Err(AppError::InvalidInput(
            "PPTX 压缩条目数量超过限制".to_string(),
        ));
    }
    let mut expanded_size = 0_u64;
    let mut names = BTreeSet::new();
    let mut warnings = Vec::new();
    for index in 0..archive.len() {
        deadline.check()?;
        let entry = archive
            .by_index(index)
            .map_err(|_| AppError::InvalidInput("PPTX 包含无法读取的条目".to_string()))?;
        if entry.encrypted() || entry.is_symlink() || entry.enclosed_name().is_none() {
            return Err(AppError::InvalidInput(
                "PPTX 包含不安全或加密的压缩条目".to_string(),
            ));
        }
        expanded_size = expanded_size
            .checked_add(entry.size())
            .ok_or_else(|| AppError::InvalidInput("PPTX 展开大小超出支持范围".to_string()))?;
        if expanded_size > MAX_CONTAINER_EXPANDED_SIZE_BYTES {
            return Err(AppError::InvalidInput("PPTX 展开大小超过限制".to_string()));
        }
        let name = entry.name().to_string();
        if is_nested_docx_container(&name) {
            return Err(AppError::InvalidInput(
                "PPTX 不允许包含嵌套压缩容器".to_string(),
            ));
        }
        if name.starts_with("ppt/embeddings/")
            || name.starts_with("ppt/activeX/")
            || name.eq_ignore_ascii_case("ppt/vbaProject.bin")
        {
            warnings.push("PPTX 包含嵌入对象或宏，已跳过且不会执行".to_string());
        }
        names.insert(name);
    }
    if !names.contains("[Content_Types].xml")
        || !names.contains("_rels/.rels")
        || !names.contains("ppt/presentation.xml")
        || !names.contains("ppt/_rels/presentation.xml.rels")
    {
        return Err(AppError::InvalidInput(
            "PPTX 缺少必要的演示文稿结构".to_string(),
        ));
    }
    let presentation_xml = read_pptx_part(&mut archive, "ppt/presentation.xml", &deadline)?;
    let presentation_relationships = ooxml_relationships(
        &read_pptx_part(&mut archive, "ppt/_rels/presentation.xml.rels", &deadline)?,
        &deadline,
    )?;
    let slide_parts = pptx_slide_parts(&presentation_xml, &presentation_relationships, &deadline)?;
    let mut blocks = Vec::new();
    let mut normalized_slides = Vec::new();
    for (slide_index, slide_part) in slide_parts.iter().enumerate() {
        deadline.check()?;
        if !names.contains(slide_part) {
            return Err(AppError::InvalidInput("PPTX 幻灯片定义不存在".to_string()));
        }
        let slide_number = slide_index + 1;
        let (slide_text, tables) = parse_pptx_slide_body(
            &read_pptx_part(&mut archive, slide_part, &deadline)?,
            &deadline,
        )?;
        if !slide_text.is_empty() {
            normalized_slides.push(format!("[第 {slide_number} 页]\n{slide_text}"));
            blocks.push(KnowledgeParsedBlock {
                block_type: "slide".to_string(),
                heading_path: vec![format!("第 {slide_number} 页")],
                content: slide_text,
                start_line: usize_to_i64(slide_number),
                end_line: usize_to_i64(slide_number),
                metadata: serde_json::json!({"slideNumber": slide_number, "reference": format!("slide-{slide_number}")}),
            });
        }
        for table in tables {
            blocks.push(KnowledgeParsedBlock {
                block_type: "table".to_string(),
                heading_path: vec![format!("第 {slide_number} 页")],
                content: table,
                start_line: usize_to_i64(slide_number),
                end_line: usize_to_i64(slide_number),
                metadata: serde_json::json!({"slideNumber": slide_number, "reference": format!("slide-{slide_number}")}),
            });
        }
        let relationship_part = pptx_slide_relationship_part(slide_part)
            .ok_or_else(|| AppError::InvalidInput("PPTX 幻灯片路径不安全".to_string()))?;
        if !names.contains(&relationship_part) {
            continue;
        }
        let relationships = ooxml_relationships(
            &read_pptx_part(&mut archive, &relationship_part, &deadline)?,
            &deadline,
        )?;
        for (_, target, relation_type) in relationships {
            if relation_type.ends_with("/notesSlide") {
                if let Some(notes_part) = pptx_related_part(&target, "notesSlides/") {
                    if names.contains(&notes_part) {
                        let notes = parse_pptx_text(
                            &read_pptx_part(&mut archive, &notes_part, &deadline)?,
                            &deadline,
                        )?;
                        if !notes.is_empty() {
                            blocks.push(KnowledgeParsedBlock {
                                block_type: "speaker_notes".to_string(),
                                heading_path: vec![format!("第 {slide_number} 页")],
                                content: notes,
                                start_line: usize_to_i64(slide_number),
                                end_line: usize_to_i64(slide_number),
                                metadata: serde_json::json!({"slideNumber": slide_number, "reference": format!("slide-{slide_number}")}),
                            });
                        }
                    }
                }
            }
            if relation_type.ends_with("/image") {
                if let Some(image_part) = pptx_related_part(&target, "media/") {
                    if names.contains(&image_part) {
                        blocks.push(KnowledgeParsedBlock {
                            block_type: "image_reference".to_string(),
                            heading_path: vec![format!("第 {slide_number} 页")],
                            content: image_part.trim_start_matches("ppt/").to_string(),
                            start_line: usize_to_i64(slide_number),
                            end_line: usize_to_i64(slide_number),
                            metadata: serde_json::json!({"slideNumber": slide_number, "reference": format!("slide-{slide_number}")}),
                        });
                    }
                }
            }
        }
    }
    Ok(KnowledgeParsedDocument {
        parser_id: parser_id.to_string(),
        normalization_version: CONTENT_NORMALIZATION_VERSION.to_string(),
        normalized_content: normalize_content(&normalized_slides.join("\n\n")),
        front_matter: serde_json::json!({"slideCount": slide_parts.len()}),
        blocks,
        warnings,
    })
}

fn read_pptx_part(
    archive: &mut zip::ZipArchive<Cursor<&[u8]>>,
    name: &str,
    deadline: &FileParseDeadline,
) -> Result<Vec<u8>, AppError> {
    deadline.check()?;
    let mut entry = archive
        .by_name(name)
        .map_err(|_| AppError::InvalidInput(format!("PPTX 缺少必要条目: {name}")))?;
    if entry.size() > MAX_ASSET_SIZE_BYTES {
        return Err(AppError::InvalidInput(format!("PPTX 条目过大: {name}")));
    }
    let mut content = Vec::with_capacity(entry.size() as usize);
    entry.read_to_end(&mut content)?;
    deadline.check()?;
    Ok(content)
}

fn pptx_slide_parts(
    presentation_xml: &[u8],
    relationships: &[(String, String, String)],
    deadline: &FileParseDeadline,
) -> Result<Vec<String>, AppError> {
    let targets = relationships
        .iter()
        .map(|(id, target, _)| (id.as_str(), target.as_str()))
        .collect::<HashMap<_, _>>();
    let mut reader = Reader::from_reader(presentation_xml);
    reader.config_mut().trim_text(true);
    let mut buffer = Vec::new();
    let mut slides = Vec::new();
    loop {
        deadline.check()?;
        match reader.read_event_into(&mut buffer) {
            Ok(Event::Empty(event)) | Ok(Event::Start(event))
                if xml_local_name(event.name().as_ref()) == "sldId" =>
            {
                if let Some(relationship_id) = ooxml_relationship_id(&event) {
                    let target = targets.get(relationship_id.as_str()).ok_or_else(|| {
                        AppError::InvalidInput("PPTX 幻灯片缺少关系定义".to_string())
                    })?;
                    let part = pptx_presentation_part(target).ok_or_else(|| {
                        AppError::InvalidInput("PPTX 幻灯片关系包含不安全路径".to_string())
                    })?;
                    slides.push(part);
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => return Err(AppError::InvalidInput("PPTX 演示 XML 无法解析".to_string())),
            _ => {}
        }
        buffer.clear();
    }
    Ok(slides)
}

fn pptx_presentation_part(target: &str) -> Option<String> {
    (!target.is_empty()
        && !target.starts_with('/')
        && !target.contains(':')
        && !target
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
        && target.starts_with("slides/")
        && target.ends_with(".xml"))
    .then(|| format!("ppt/{target}"))
}

fn ooxml_relationship_id(event: &quick_xml::events::BytesStart<'_>) -> Option<String> {
    event
        .attributes()
        .with_checks(false)
        .flatten()
        .find_map(|attribute| {
            std::str::from_utf8(attribute.key.as_ref())
                .ok()
                .filter(|name| name.ends_with(":id"))
                .and_then(|_| std::str::from_utf8(attribute.value.as_ref()).ok())
                .and_then(|value| quick_xml::escape::unescape(value).ok())
                .map(|value| value.into_owned())
        })
}

fn pptx_slide_relationship_part(slide_part: &str) -> Option<String> {
    let filename = slide_part.strip_prefix("ppt/slides/")?;
    (!filename.is_empty() && !filename.contains('/') && filename.ends_with(".xml"))
        .then(|| format!("ppt/slides/_rels/{filename}.rels"))
}

fn pptx_related_part(target: &str, expected_directory: &str) -> Option<String> {
    let target = target.strip_prefix("../")?;
    (!target.is_empty()
        && !target.starts_with('/')
        && !target.contains(':')
        && !target
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
        && target.starts_with(expected_directory))
    .then(|| format!("ppt/{target}"))
}

fn parse_pptx_slide_body(
    xml: &[u8],
    deadline: &FileParseDeadline,
) -> Result<(String, Vec<String>), AppError> {
    let mut reader = Reader::from_reader(xml);
    reader.config_mut().trim_text(true);
    let mut buffer = Vec::new();
    let mut paragraphs = Vec::new();
    let mut paragraph = String::new();
    let mut in_text = false;
    let mut table_depth = 0_usize;
    let mut table_rows = Vec::<Vec<String>>::new();
    let mut current_row = Vec::<String>::new();
    let mut current_cell = String::new();
    let mut tables = Vec::new();
    loop {
        deadline.check()?;
        match reader.read_event_into(&mut buffer) {
            Ok(Event::Start(event)) => match xml_local_name(event.name().as_ref()) {
                "t" => in_text = true,
                "tbl" => table_depth += 1,
                _ => {}
            },
            Ok(Event::Text(event)) if in_text => {
                let text = event
                    .decode()
                    .map_err(|_| AppError::InvalidInput("PPTX 文本编码无效".to_string()))?;
                let text = quick_xml::escape::unescape(&text)
                    .map_err(|_| AppError::InvalidInput("PPTX 文本转义无效".to_string()))?;
                paragraph.push_str(&text);
            }
            Ok(Event::CData(event)) if in_text => {
                let text = String::from_utf8_lossy(event.as_ref());
                paragraph.push_str(&text);
            }
            Ok(Event::End(event)) => match xml_local_name(event.name().as_ref()) {
                "t" => in_text = false,
                "p" => {
                    let content = normalize_docx_text(&paragraph);
                    if !content.is_empty() {
                        paragraphs.push(content.clone());
                        if table_depth > 0 {
                            if !current_cell.is_empty() {
                                current_cell.push(' ');
                            }
                            current_cell.push_str(&content);
                        }
                    }
                    paragraph.clear();
                }
                "tc" if table_depth > 0 => current_row.push(std::mem::take(&mut current_cell)),
                "tr" if table_depth > 0 => {
                    if !current_row.is_empty() {
                        table_rows.push(std::mem::take(&mut current_row));
                    }
                }
                "tbl" if table_depth > 0 => {
                    table_depth -= 1;
                    if table_depth == 0 && !table_rows.is_empty() {
                        tables.push(
                            table_rows
                                .drain(..)
                                .map(|row| row.join(" | "))
                                .collect::<Vec<_>>()
                                .join("\n"),
                        );
                    }
                }
                _ => {}
            },
            Ok(Event::Eof) => break,
            Err(_) => {
                return Err(AppError::InvalidInput(
                    "PPTX 幻灯片 XML 无法解析".to_string(),
                ))
            }
            _ => {}
        }
        buffer.clear();
    }
    Ok((paragraphs.join("\n"), tables))
}

fn parse_pptx_text(xml: &[u8], deadline: &FileParseDeadline) -> Result<String, AppError> {
    Ok(parse_pptx_slide_body(xml, deadline)?.0)
}

fn parse_xlsx_document(parser_id: &str, bytes: &[u8]) -> Result<KnowledgeParsedDocument, AppError> {
    if bytes.len() > MAX_ASSET_SIZE_BYTES as usize {
        return Err(AppError::InvalidInput(
            "XLSX 文件超过可解析大小限制".to_string(),
        ));
    }
    let deadline = FileParseDeadline::new(FILE_PARSE_TIMEOUT);
    // calamine 负责读取单元格和已缓存的公式结果；表定义仍需从受限 OOXML 容器中读取。
    // 先做容器复核，避免解析器绕过上传阶段的压缩条目与路径限制。
    let table_blocks = parse_xlsx_table_blocks(bytes, &deadline)?;
    let mut workbook = calamine::open_workbook_auto_from_rs(Cursor::new(bytes.to_vec()))
        .map_err(|_| AppError::InvalidInput("XLSX 不是可读取的工作簿".to_string()))?;
    let defined_names = workbook.defined_names().to_vec();
    let sheet_names = workbook.sheet_names();
    let mut blocks = Vec::new();
    let mut normalized = Vec::new();
    for sheet_name in sheet_names {
        deadline.check()?;
        let range = workbook
            .worksheet_range(&sheet_name)
            .map_err(|error| AppError::InvalidInput(format!("XLSX 工作表读取失败: {error}")))?;
        let formulas = workbook
            .worksheet_formula(&sheet_name)
            .map_err(|error| AppError::InvalidInput(format!("XLSX 公式读取失败: {error}")))?;
        let mut sheet_text = Vec::new();
        for (row_index, column_index, cached_value) in range.cells() {
            deadline.check()?;
            let row = u32::try_from(row_index)
                .map_err(|_| AppError::InvalidInput("XLSX 行号超出支持范围".to_string()))?;
            let column = u32::try_from(column_index)
                .map_err(|_| AppError::InvalidInput("XLSX 列号超出支持范围".to_string()))?;
            let formula = formulas
                .get_value((row, column))
                .filter(|formula| !formula.is_empty());
            if cached_value.is_empty() && formula.is_none() {
                continue;
            }
            let cell = xlsx_cell_reference(row, column);
            let reference = format!("{sheet_name}!{cell}");
            let cached = cached_value.to_string();
            let content = formula
                .map(|formula| format!("{reference}: {formula}（缓存值：{cached}）"))
                .unwrap_or_else(|| format!("{reference}: {cached}"));
            sheet_text.push(content.clone());
            blocks.push(KnowledgeParsedBlock {
                block_type: "cell".to_string(), heading_path: vec![sheet_name.clone()], content,
                start_line: usize_to_i64(row_index.saturating_add(1)),
                end_line: usize_to_i64(row_index.saturating_add(1)),
                metadata: serde_json::json!({"sheet": sheet_name, "cell": cell, "formula": formula, "cachedValue": cached, "formulaEvaluated": false}),
            });
        }
        if !sheet_text.is_empty() {
            normalized.push(format!("[{sheet_name}]\n{}", sheet_text.join("\n")));
        }
    }
    for (name, formula) in defined_names {
        blocks.push(KnowledgeParsedBlock {
            block_type: "named_range".to_string(),
            heading_path: Vec::new(),
            content: format!("{name}: {formula}"),
            start_line: 1,
            end_line: 1,
            metadata: serde_json::json!({"name": name, "formula": formula}),
        });
    }
    blocks.extend(table_blocks);
    Ok(KnowledgeParsedDocument {
        parser_id: parser_id.to_string(),
        normalization_version: CONTENT_NORMALIZATION_VERSION.to_string(),
        normalized_content: normalize_content(&normalized.join("\n\n")),
        front_matter: serde_json::json!({"sheetCount": workbook.sheet_names().len(), "definedNameCount": workbook.defined_names().len()}),
        blocks,
        warnings: Vec::new(),
    })
}

/// XLSX 表格定义不在 calamine 的单元格 API 中暴露，因此只读取受控 OOXML 关系与表定义；
/// 不解释公式、不加载外部资源，也不把任意压缩条目当作可读文本。
fn parse_xlsx_table_blocks(
    bytes: &[u8],
    deadline: &FileParseDeadline,
) -> Result<Vec<KnowledgeParsedBlock>, AppError> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))
        .map_err(|_| AppError::InvalidInput("XLSX 不是可读取的 Office 压缩容器".to_string()))?;
    if archive.len() > MAX_CONTAINER_ENTRIES {
        return Err(AppError::InvalidInput(
            "XLSX 压缩条目数量超过限制".to_string(),
        ));
    }

    let mut expanded_size = 0_u64;
    let mut names = BTreeSet::new();
    for index in 0..archive.len() {
        deadline.check()?;
        let entry = archive
            .by_index(index)
            .map_err(|_| AppError::InvalidInput("XLSX 包含无法读取的条目".to_string()))?;
        if entry.encrypted() || entry.is_symlink() || entry.enclosed_name().is_none() {
            return Err(AppError::InvalidInput(
                "XLSX 包含不安全或加密的压缩条目".to_string(),
            ));
        }
        expanded_size = expanded_size
            .checked_add(entry.size())
            .ok_or_else(|| AppError::InvalidInput("XLSX 展开大小超出支持范围".to_string()))?;
        if expanded_size > MAX_CONTAINER_EXPANDED_SIZE_BYTES {
            return Err(AppError::InvalidInput("XLSX 展开大小超过限制".to_string()));
        }
        let name = entry.name().to_string();
        if is_nested_docx_container(&name) {
            return Err(AppError::InvalidInput(
                "XLSX 不允许包含嵌套压缩容器".to_string(),
            ));
        }
        names.insert(name);
    }
    if !names.contains("[Content_Types].xml")
        || !names.contains("_rels/.rels")
        || !names.contains("xl/workbook.xml")
        || !names.contains("xl/_rels/workbook.xml.rels")
        || !names.iter().any(|name| name.starts_with("xl/worksheets/"))
    {
        return Err(AppError::InvalidInput(
            "XLSX 缺少必要的工作簿结构".to_string(),
        ));
    }

    let workbook_xml = read_xlsx_part(&mut archive, "xl/workbook.xml", deadline)?;
    let workbook_relationships = ooxml_relationships(
        &read_xlsx_part(&mut archive, "xl/_rels/workbook.xml.rels", deadline)?,
        deadline,
    )?;
    let sheet_names = xlsx_sheet_names(&workbook_xml, &workbook_relationships, deadline)?;
    let mut blocks = Vec::new();
    let mut seen = HashSet::new();
    for relationship_part in names
        .iter()
        .filter(|name| name.starts_with("xl/worksheets/_rels/") && name.ends_with(".rels"))
    {
        deadline.check()?;
        let sheet_part = xlsx_sheet_part_for_relationship(relationship_part)
            .ok_or_else(|| AppError::InvalidInput("XLSX 工作表关系路径不安全".to_string()))?;
        let Some(sheet_name) = sheet_names.get(&sheet_part) else {
            continue;
        };
        let relationships = ooxml_relationships(
            &read_xlsx_part(&mut archive, relationship_part, deadline)?,
            deadline,
        )?;
        for (_, target, _) in relationships
            .into_iter()
            .filter(|(_, _, relation_type)| relation_type.ends_with("/table"))
        {
            let table_part = xlsx_table_part_for_relationship(&target)
                .ok_or_else(|| AppError::InvalidInput("XLSX 表格关系包含不安全路径".to_string()))?;
            if !names.contains(&table_part) {
                return Err(AppError::InvalidInput("XLSX 表格定义不存在".to_string()));
            }
            let (name, reference) = parse_xlsx_table_definition(
                &read_xlsx_part(&mut archive, &table_part, deadline)?,
                deadline,
            )?;
            if seen.insert((sheet_name.clone(), name.clone(), reference.clone())) {
                blocks.push(KnowledgeParsedBlock {
                    block_type: "table".to_string(),
                    heading_path: vec![sheet_name.clone()],
                    content: format!("{sheet_name}!{reference}: {name}"),
                    start_line: 1,
                    end_line: 1,
                    metadata: serde_json::json!({
                        "sheet": sheet_name,
                        "name": name,
                        "reference": reference,
                    }),
                });
            }
        }
    }
    Ok(blocks)
}

fn read_xlsx_part(
    archive: &mut zip::ZipArchive<Cursor<&[u8]>>,
    name: &str,
    deadline: &FileParseDeadline,
) -> Result<Vec<u8>, AppError> {
    deadline.check()?;
    let mut entry = archive
        .by_name(name)
        .map_err(|_| AppError::InvalidInput(format!("XLSX 缺少必要条目: {name}")))?;
    if entry.size() > MAX_ASSET_SIZE_BYTES {
        return Err(AppError::InvalidInput(format!("XLSX 条目过大: {name}")));
    }
    let mut content = Vec::with_capacity(entry.size() as usize);
    entry.read_to_end(&mut content)?;
    deadline.check()?;
    Ok(content)
}

fn ooxml_relationships(
    xml: &[u8],
    deadline: &FileParseDeadline,
) -> Result<Vec<(String, String, String)>, AppError> {
    let mut reader = Reader::from_reader(xml);
    reader.config_mut().trim_text(true);
    let mut buffer = Vec::new();
    let mut relationships = Vec::new();
    loop {
        deadline.check()?;
        match reader.read_event_into(&mut buffer) {
            Ok(Event::Empty(event)) | Ok(Event::Start(event))
                if xml_local_name(event.name().as_ref()) == "Relationship" =>
            {
                if let (Some(id), Some(target), Some(relation_type)) = (
                    docx_attribute_value(&event, "Id"),
                    docx_attribute_value(&event, "Target"),
                    docx_attribute_value(&event, "Type"),
                ) {
                    relationships.push((id, target, relation_type));
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => {
                return Err(AppError::InvalidInput("XLSX 关系 XML 无法解析".to_string()));
            }
            _ => {}
        }
        buffer.clear();
    }
    Ok(relationships)
}

fn xlsx_sheet_names(
    workbook_xml: &[u8],
    relationships: &[(String, String, String)],
    deadline: &FileParseDeadline,
) -> Result<HashMap<String, String>, AppError> {
    let relationship_targets = relationships
        .iter()
        .map(|(id, target, _)| (id.as_str(), target.as_str()))
        .collect::<HashMap<_, _>>();
    let mut reader = Reader::from_reader(workbook_xml);
    reader.config_mut().trim_text(true);
    let mut buffer = Vec::new();
    let mut sheet_names = HashMap::new();
    loop {
        deadline.check()?;
        match reader.read_event_into(&mut buffer) {
            Ok(Event::Empty(event)) | Ok(Event::Start(event))
                if xml_local_name(event.name().as_ref()) == "sheet" =>
            {
                let name = docx_attribute_value(&event, "name");
                let relationship_id = docx_attribute_value(&event, "id");
                if let (Some(name), Some(relationship_id)) = (name, relationship_id) {
                    let target = relationship_targets
                        .get(relationship_id.as_str())
                        .ok_or_else(|| {
                            AppError::InvalidInput("XLSX 工作表缺少关系定义".to_string())
                        })?;
                    let part = xlsx_workbook_part(target).ok_or_else(|| {
                        AppError::InvalidInput("XLSX 工作表关系包含不安全路径".to_string())
                    })?;
                    sheet_names.insert(part, name);
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => {
                return Err(AppError::InvalidInput(
                    "XLSX 工作簿 XML 无法解析".to_string(),
                ));
            }
            _ => {}
        }
        buffer.clear();
    }
    Ok(sheet_names)
}

fn xlsx_workbook_part(target: &str) -> Option<String> {
    (!target.is_empty()
        && !target.starts_with('/')
        && !target.contains(':')
        && !target
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
        && target.starts_with("worksheets/"))
    .then(|| format!("xl/{target}"))
}

fn xlsx_sheet_part_for_relationship(relationship_part: &str) -> Option<String> {
    let filename = relationship_part
        .strip_prefix("xl/worksheets/_rels/")?
        .strip_suffix(".rels")?;
    (!filename.is_empty() && !filename.contains('/') && filename.ends_with(".xml"))
        .then(|| format!("xl/worksheets/{filename}"))
}

fn xlsx_table_part_for_relationship(target: &str) -> Option<String> {
    let target = target.strip_prefix("../")?;
    (!target.is_empty()
        && !target.starts_with('/')
        && !target.contains(':')
        && !target
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
        && target.starts_with("tables/")
        && target.ends_with(".xml"))
    .then(|| format!("xl/{target}"))
}

fn parse_xlsx_table_definition(
    xml: &[u8],
    deadline: &FileParseDeadline,
) -> Result<(String, String), AppError> {
    let mut reader = Reader::from_reader(xml);
    reader.config_mut().trim_text(true);
    let mut buffer = Vec::new();
    loop {
        deadline.check()?;
        match reader.read_event_into(&mut buffer) {
            Ok(Event::Empty(event)) | Ok(Event::Start(event))
                if xml_local_name(event.name().as_ref()) == "table" =>
            {
                let name = docx_attribute_value(&event, "displayName")
                    .or_else(|| docx_attribute_value(&event, "name"))
                    .filter(|name| !name.trim().is_empty())
                    .ok_or_else(|| AppError::InvalidInput("XLSX 表格缺少名称".to_string()))?;
                let reference = docx_attribute_value(&event, "ref")
                    .filter(|reference| is_safe_xlsx_table_reference(reference))
                    .ok_or_else(|| AppError::InvalidInput("XLSX 表格范围无效".to_string()))?;
                return Ok((name, reference));
            }
            Ok(Event::Eof) => break,
            Err(_) => {
                return Err(AppError::InvalidInput("XLSX 表格 XML 无法解析".to_string()));
            }
            _ => {}
        }
        buffer.clear();
    }
    Err(AppError::InvalidInput("XLSX 表格定义缺失".to_string()))
}

fn is_safe_xlsx_table_reference(reference: &str) -> bool {
    !reference.is_empty()
        && reference.len() <= 64
        && reference
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'$' | b':' | b'_'))
        && reference.bytes().any(|byte| byte.is_ascii_alphabetic())
        && reference.bytes().any(|byte| byte.is_ascii_digit())
}

fn xlsx_cell_reference(row: u32, column: u32) -> String {
    let mut value = column + 1;
    let mut letters = String::new();
    while value > 0 {
        let remainder = (value - 1) % 26;
        letters.insert(
            0,
            char::from_u32(u32::from(b'A') + remainder).unwrap_or('A'),
        );
        value = (value - 1) / 26;
    }
    format!("{letters}{}", row + 1)
}

fn parse_docx_document(parser_id: &str, bytes: &[u8]) -> Result<KnowledgeParsedDocument, AppError> {
    if bytes.len() > MAX_ASSET_SIZE_BYTES as usize {
        return Err(AppError::InvalidInput(
            "DOCX 文件超过可解析大小限制".to_string(),
        ));
    }
    let deadline = FileParseDeadline::new(FILE_PARSE_TIMEOUT);
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))
        .map_err(|_| AppError::InvalidInput("DOCX 不是可读取的 Office 压缩容器".to_string()))?;
    if archive.len() > MAX_CONTAINER_ENTRIES {
        return Err(AppError::InvalidInput(
            "DOCX 压缩条目数量超过限制".to_string(),
        ));
    }
    let mut expanded_size = 0_u64;
    let mut names = BTreeSet::new();
    for index in 0..archive.len() {
        deadline.check()?;
        let entry = archive
            .by_index(index)
            .map_err(|_| AppError::InvalidInput("DOCX 包含无法读取的条目".to_string()))?;
        if entry.encrypted() || entry.is_symlink() || entry.enclosed_name().is_none() {
            return Err(AppError::InvalidInput(
                "DOCX 包含不安全或加密的压缩条目".to_string(),
            ));
        }
        expanded_size = expanded_size
            .checked_add(entry.size())
            .ok_or_else(|| AppError::InvalidInput("DOCX 展开大小超出支持范围".to_string()))?;
        if expanded_size > MAX_CONTAINER_EXPANDED_SIZE_BYTES {
            return Err(AppError::InvalidInput("DOCX 展开大小超过限制".to_string()));
        }
        let name = entry.name().to_string();
        if is_nested_docx_container(&name) {
            return Err(AppError::InvalidInput(
                "DOCX 不允许包含嵌套压缩容器".to_string(),
            ));
        }
        names.insert(name);
    }
    if !names.contains("[Content_Types].xml") || !names.contains("word/document.xml") {
        return Err(AppError::InvalidInput(
            "DOCX 缺少必要的 Word 文档结构".to_string(),
        ));
    }
    let mut document_xml = Vec::new();
    archive
        .by_name("word/document.xml")
        .map_err(|_| AppError::InvalidInput("DOCX 缺少正文 XML".to_string()))?
        .take(16 * 1024 * 1024)
        .read_to_end(&mut document_xml)?;
    deadline.check()?;
    let (mut blocks, mut warnings, referenced_images) = parse_docx_body(&document_xml, &deadline)?;
    let relationship_images = parse_docx_image_relationships(&mut archive, &deadline)?;
    for (relationship_id, target) in relationship_images
        .into_iter()
        .filter(|(relationship_id, _)| referenced_images.contains(relationship_id))
    {
        blocks.push(KnowledgeParsedBlock {
            block_type: "image_reference".to_string(),
            heading_path: Vec::new(),
            content: target.clone(),
            start_line: 1,
            end_line: 1,
            metadata: serde_json::json!({
                "part": "word/_rels/document.xml.rels",
                "relationshipId": relationship_id,
                "target": target,
            }),
        });
    }
    if names
        .iter()
        .any(|name| name.starts_with("word/embeddings/"))
    {
        warnings.push("DOCX 包含嵌入对象，当前仅保留原始文件引用，未解析其内容".to_string());
    }
    if names.iter().any(|name| name.starts_with("word/activeX/")) {
        warnings.push("DOCX 包含 ActiveX 控件，已跳过且不会执行".to_string());
    }
    let normalized_content = blocks
        .iter()
        .filter(|block| {
            matches!(
                block.block_type.as_str(),
                "heading" | "paragraph" | "list_item" | "table"
            )
        })
        .map(|block| block.content.as_str())
        .collect::<Vec<_>>()
        .join("\n\n");
    let title = blocks
        .iter()
        .find(|block| block.block_type == "heading")
        .map(|block| block.content.clone());
    Ok(KnowledgeParsedDocument {
        parser_id: parser_id.to_string(),
        normalization_version: CONTENT_NORMALIZATION_VERSION.to_string(),
        normalized_content: normalize_content(&normalized_content),
        front_matter: serde_json::json!({"title": title, "part": "word/document.xml"}),
        blocks,
        warnings,
    })
}

fn parse_docx_body(
    xml: &[u8],
    deadline: &FileParseDeadline,
) -> Result<DocxBodyParseResult, AppError> {
    let mut reader = Reader::from_reader(Cursor::new(xml));
    reader.config_mut().trim_text(true);
    let mut buffer = Vec::new();
    let mut blocks = Vec::new();
    let mut warnings = Vec::new();
    let mut paragraph_text = String::new();
    let mut paragraph_style = None::<String>;
    let mut paragraph_is_list = false;
    let mut in_text = false;
    let mut table_depth = 0_usize;
    let mut current_cell = String::new();
    let mut current_row = Vec::<String>::new();
    let mut table_rows = Vec::<Vec<String>>::new();
    let mut paragraph_index = 0_usize;
    let mut referenced_images = HashSet::new();
    loop {
        deadline.check()?;
        match reader
            .read_event_into(&mut buffer)
            .map_err(|error| AppError::InvalidInput(format!("DOCX 正文 XML 解析失败: {error}")))?
        {
            Event::Start(event) => match xml_local_name(event.name().as_ref()) {
                "p" => {
                    paragraph_text.clear();
                    paragraph_style = None;
                    paragraph_is_list = false;
                }
                "tbl" => table_depth = table_depth.saturating_add(1),
                "tr" => current_row.clear(),
                "tc" => current_cell.clear(),
                "t" => in_text = true,
                "tab" => paragraph_text.push('\t'),
                "br" | "cr" => paragraph_text.push('\n'),
                "pStyle" => paragraph_style = docx_attribute_value(&event, "val"),
                "numPr" => paragraph_is_list = true,
                "blip" => {
                    if let Some(relationship_id) = docx_attribute_value(&event, "embed") {
                        referenced_images.insert(relationship_id);
                    }
                }
                "object" | "oleObject" | "altChunk" => {
                    warnings.push("DOCX 包含当前不支持的嵌入对象，已跳过且不会执行".to_string())
                }
                _ => {}
            },
            Event::Empty(event) => match xml_local_name(event.name().as_ref()) {
                // Word 常将段落样式、编号、换行和制表符写成空元素，必须与普通开始标签
                // 同等处理，不能因此把标题或列表降级成普通段落。
                "tab" => paragraph_text.push('\t'),
                "br" | "cr" => paragraph_text.push('\n'),
                "pStyle" => paragraph_style = docx_attribute_value(&event, "val"),
                "numPr" => paragraph_is_list = true,
                "blip" => {
                    if let Some(relationship_id) = docx_attribute_value(&event, "embed") {
                        referenced_images.insert(relationship_id);
                    }
                }
                "object" | "oleObject" | "altChunk" => {
                    warnings.push("DOCX 包含当前不支持的嵌入对象，已跳过且不会执行".to_string())
                }
                _ => {}
            },
            Event::Text(event) if in_text => {
                let decoded = event.decode().map_err(|error| {
                    AppError::InvalidInput(format!("DOCX 文本编码无效: {error}"))
                })?;
                let text = quick_xml::escape::unescape(&decoded).map_err(|error| {
                    AppError::InvalidInput(format!("DOCX 文本转义无效: {error}"))
                })?;
                paragraph_text.push_str(&text);
            }
            Event::End(event) => match xml_local_name(event.name().as_ref()) {
                "t" => in_text = false,
                "p" => {
                    let content = normalize_docx_text(&paragraph_text);
                    if !content.is_empty() {
                        paragraph_index += 1;
                        if table_depth > 0 {
                            if !current_cell.is_empty() {
                                current_cell.push('\n');
                            }
                            current_cell.push_str(&content);
                        } else {
                            let block_type = if paragraph_is_list {
                                "list_item"
                            } else if paragraph_style.as_deref().is_some_and(|style| {
                                style.to_ascii_lowercase().starts_with("heading")
                            }) {
                                "heading"
                            } else {
                                "paragraph"
                            };
                            blocks.push(KnowledgeParsedBlock {
                                block_type: block_type.to_string(),
                                heading_path: Vec::new(),
                                content,
                                start_line: i64::try_from(paragraph_index).unwrap_or(i64::MAX),
                                end_line: i64::try_from(paragraph_index).unwrap_or(i64::MAX),
                                metadata: serde_json::json!({
                                    "part": "word/document.xml",
                                    "paragraphIndex": paragraph_index,
                                    "style": paragraph_style,
                                }),
                            });
                        }
                    }
                }
                "tc" => current_row.push(normalize_docx_text(&current_cell)),
                "tr" => {
                    if !current_row.is_empty() {
                        table_rows.push(std::mem::take(&mut current_row));
                    }
                }
                "tbl" => {
                    table_depth = table_depth.saturating_sub(1);
                    if table_depth == 0 && !table_rows.is_empty() {
                        let rows = std::mem::take(&mut table_rows);
                        let column_count = rows.iter().map(Vec::len).max().unwrap_or_default();
                        let content = rows
                            .iter()
                            .map(|row| row.join(" | "))
                            .collect::<Vec<_>>()
                            .join("\n");
                        blocks.push(KnowledgeParsedBlock {
                            block_type: "table".to_string(),
                            heading_path: Vec::new(),
                            content,
                            start_line: i64::try_from(paragraph_index.saturating_add(1))
                                .unwrap_or(i64::MAX),
                            end_line: i64::try_from(paragraph_index.saturating_add(1))
                                .unwrap_or(i64::MAX),
                            metadata: serde_json::json!({
                                "part": "word/document.xml",
                                "rowCount": rows.len(),
                                "columnCount": column_count,
                            }),
                        });
                    }
                }
                _ => {}
            },
            Event::Eof => break,
            _ => {}
        }
        buffer.clear();
    }
    Ok((blocks, warnings, referenced_images))
}

fn parse_docx_image_relationships(
    archive: &mut zip::ZipArchive<Cursor<&[u8]>>,
    deadline: &FileParseDeadline,
) -> Result<HashMap<String, String>, AppError> {
    let Ok(relationship_file) = archive.by_name("word/_rels/document.xml.rels") else {
        return Ok(HashMap::new());
    };
    let mut xml = Vec::new();
    relationship_file.take(1024 * 1024).read_to_end(&mut xml)?;
    deadline.check()?;
    let mut reader = Reader::from_reader(Cursor::new(xml));
    let mut buffer = Vec::new();
    let mut images = HashMap::new();
    loop {
        deadline.check()?;
        match reader
            .read_event_into(&mut buffer)
            .map_err(|error| AppError::InvalidInput(format!("DOCX 关系 XML 解析失败: {error}")))?
        {
            Event::Empty(event) | Event::Start(event)
                if xml_local_name(event.name().as_ref()) == "Relationship" =>
            {
                let relation_type = docx_attribute_value(&event, "Type").unwrap_or_default();
                if relation_type.ends_with("/image") {
                    if let (Some(id), Some(target)) = (
                        docx_attribute_value(&event, "Id"),
                        docx_attribute_value(&event, "Target"),
                    ) {
                        if is_safe_docx_part_reference(&target) {
                            images.insert(id, target);
                        }
                    }
                }
            }
            Event::Eof => break,
            _ => {}
        }
        buffer.clear();
    }
    Ok(images)
}

fn xml_local_name(name: &[u8]) -> &str {
    std::str::from_utf8(name)
        .ok()
        .and_then(|name| name.rsplit(':').next())
        .unwrap_or_default()
}

fn docx_attribute_value(
    event: &quick_xml::events::BytesStart<'_>,
    expected_name: &str,
) -> Option<String> {
    event
        .attributes()
        .with_checks(false)
        .flatten()
        .find_map(|attribute| {
            (xml_local_name(attribute.key.as_ref()) == expected_name)
                .then(|| {
                    std::str::from_utf8(attribute.value.as_ref())
                        .ok()
                        .and_then(|value| quick_xml::escape::unescape(value).ok())
                        .map(|value| value.into_owned())
                })
                .flatten()
        })
}

fn normalize_docx_text(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
}

fn is_nested_docx_container(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    [".zip", ".docx", ".xlsx", ".pptx"]
        .iter()
        .any(|extension| lower.ends_with(extension))
}

fn is_safe_docx_part_reference(target: &str) -> bool {
    !target.is_empty()
        && !target.starts_with('/')
        && !target.contains(':')
        && !target.split('/').any(|part| part == "..")
        && target.starts_with("media/")
}

fn sanitize_html(content: &str) -> String {
    let mut builder = ammonia::Builder::default();
    // 原型图中的控件只作为可读结构提取；不保留事件、样式、表单提交目标或任意 URL。
    builder
        .add_tags(&[
            "button", "form", "input", "label", "option", "select", "textarea",
        ])
        .add_tag_attributes("button", &["aria-label", "name", "type", "value"])
        .add_tag_attributes(
            "input",
            &["aria-label", "name", "placeholder", "type", "value"],
        )
        .add_tag_attributes("label", &["aria-label"])
        .add_tag_attributes("option", &["value"])
        .add_tag_attributes("select", &["aria-label", "name"])
        .add_tag_attributes("textarea", &["aria-label", "name", "placeholder"])
        // 空白名单会移除 http(s)、data、javascript 等绝对 URL；相对路径仅作为引用文本，
        // 不会在解析期间加载或打开。
        .url_schemes(HashSet::new())
        .url_relative(ammonia::UrlRelative::PassThrough);
    builder.clean(content).to_string()
}

fn visible_html_text(content: &str) -> String {
    HTML_WHITESPACE_RE
        .replace_all(&HTML_TAG_RE.replace_all(content, " "), " ")
        .trim()
        .to_string()
}

fn html_block(
    block_type: &str,
    content: &str,
    source: &str,
    mut metadata: serde_json::Value,
) -> KnowledgeParsedBlock {
    let (start_line, end_line) = if let Some(start_offset) = source.find(content) {
        let start_line = source[..start_offset].lines().count().saturating_add(1);
        let end_line = start_line.saturating_add(content.lines().count().saturating_sub(1));
        (start_line, end_line)
    } else {
        // 清洗和空白归一化后的整页可见正文未必能逐字回映原文；保留整页范围而不伪造行号。
        (1, source.lines().count().max(1))
    };
    if let Some(object) = metadata.as_object_mut() {
        object.insert("sourceStartLine".to_string(), serde_json::json!(start_line));
        object.insert("sourceEndLine".to_string(), serde_json::json!(end_line));
    }
    KnowledgeParsedBlock {
        block_type: block_type.to_string(),
        heading_path: Vec::new(),
        content: content.to_string(),
        start_line: usize_to_i64(start_line),
        end_line: usize_to_i64(end_line),
        metadata,
    }
}

fn html_control_metadata(tag: &str, attributes: &str) -> serde_json::Value {
    let attributes = HTML_ATTRIBUTE_RE
        .captures_iter(attributes)
        .filter_map(|captures| {
            let name = captures.name("name")?.as_str().to_ascii_lowercase();
            let value = captures.name("value")?.as_str().trim().to_string();
            (!value.is_empty()).then_some((name, serde_json::Value::String(value)))
        })
        .collect::<serde_json::Map<_, _>>();
    let label = ["aria-label", "placeholder", "value", "name"]
        .iter()
        .find_map(|key| attributes.get(*key).and_then(serde_json::Value::as_str))
        .unwrap_or(tag)
        .to_string();
    serde_json::json!({"tag": tag, "label": label, "attributes": attributes})
}

fn is_safe_relative_resource(value: &str) -> bool {
    let value = value.trim();
    !value.is_empty()
        && !value.starts_with('/')
        && !value.starts_with('#')
        && !value.contains(':')
        && !value.chars().any(char::is_control)
}

impl KnowledgeChunker for StructureAwareChunker {
    fn strategy_id(&self) -> &'static str {
        STRUCTURE_CHUNK_STRATEGY_ID
    }

    fn chunk(
        &self,
        parsed: &KnowledgeParsedDocument,
        options: Option<&KnowledgeChunkOptions>,
    ) -> Result<Vec<KnowledgeChunkWriteInput>, AppError> {
        let (target, max, overlap) = normalized_chunk_options(options)?;
        let mut chunks = Vec::new();
        let mut pending = Vec::<KnowledgeParsedBlock>::new();
        let mut pending_chars = 0_usize;

        for block in &parsed.blocks {
            let block_chars = block.content.chars().count();
            if block_chars > max {
                flush_blocks(&mut chunks, &mut pending, &mut pending_chars, parsed)?;
                split_large_block(&mut chunks, block, max, overlap, parsed)?;
                continue;
            }
            let separator_chars = usize::from(!pending.is_empty()) * 2;
            if !pending.is_empty() && pending_chars + separator_chars + block_chars > max {
                flush_blocks(&mut chunks, &mut pending, &mut pending_chars, parsed)?;
            }
            pending_chars += usize::from(!pending.is_empty()) * 2 + block_chars;
            pending.push(block.clone());
            if pending_chars >= target {
                flush_blocks(&mut chunks, &mut pending, &mut pending_chars, parsed)?;
            }
        }
        flush_blocks(&mut chunks, &mut pending, &mut pending_chars, parsed)?;
        for (index, chunk) in chunks.iter_mut().enumerate() {
            chunk.chunk_index = usize_to_i64(index);
        }
        Ok(chunks)
    }
}

fn normalize_content(content: &str) -> String {
    content
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n")
        .trim_matches('\n')
        .to_string()
}

fn parsed_document(
    parser_id: &str,
    normalized_content: String,
    front_matter: serde_json::Value,
    blocks: Vec<KnowledgeParsedBlock>,
) -> KnowledgeParsedDocument {
    KnowledgeParsedDocument {
        parser_id: parser_id.to_string(),
        normalization_version: CONTENT_NORMALIZATION_VERSION.to_string(),
        normalized_content,
        front_matter,
        blocks,
        warnings: Vec::new(),
    }
}

fn block(
    block_type: &str,
    heading_path: &[String],
    content: String,
    start_line: usize,
    end_line: usize,
    metadata: serde_json::Value,
) -> KnowledgeParsedBlock {
    KnowledgeParsedBlock {
        block_type: block_type.to_string(),
        heading_path: heading_path.to_vec(),
        content,
        start_line: usize_to_i64(start_line + 1),
        end_line: usize_to_i64(end_line + 1),
        metadata,
    }
}

fn markdown_heading(line: &str) -> Option<(usize, &str)> {
    let trimmed = line.trim_start();
    let level = trimmed
        .chars()
        .take_while(|character| *character == '#')
        .count();
    if !(1..=6).contains(&level) || !trimmed[level..].starts_with(' ') {
        return None;
    }
    let title = trimmed[level..].trim();
    (!title.is_empty()).then_some((level, title))
}

fn markdown_fence(line: &str) -> Option<(&'static str, String)> {
    let trimmed = line.trim_start();
    if let Some(language) = trimmed.strip_prefix("```") {
        Some(("```", language.trim().to_string()))
    } else {
        trimmed
            .strip_prefix("~~~")
            .map(|language| ("~~~", language.trim().to_string()))
    }
}

fn is_markdown_table_start(lines: &[&str], index: usize) -> bool {
    index + 1 < lines.len()
        && lines[index].contains('|')
        && lines[index + 1]
            .split('|')
            .filter(|part| !part.trim().is_empty())
            .all(|part| {
                let part = part.trim().trim_matches(':');
                part.len() >= 3 && part.chars().all(|character| character == '-')
            })
}

fn table_column_count(line: &str) -> usize {
    line.split('|')
        .filter(|part| !part.trim().is_empty())
        .count()
}

struct SqlStatement {
    content: String,
    start_line: usize,
    end_line: usize,
}

fn split_sql_statements(content: &str) -> Result<Vec<SqlStatement>, AppError> {
    let chars = content.chars().collect::<Vec<_>>();
    let mut statements = Vec::new();
    let mut buffer = String::new();
    let mut line = 0_usize;
    let mut start_line = 0_usize;
    let mut quote = None::<char>;
    let mut line_comment = false;
    let mut block_comment = false;
    let mut index = 0_usize;
    while index < chars.len() {
        let current = chars[index];
        let next = chars.get(index + 1).copied();
        if buffer.trim().is_empty() && !current.is_whitespace() {
            start_line = line;
        }
        if current == '\n' {
            line += 1;
            line_comment = false;
        }
        if quote.is_none() && !line_comment && !block_comment && current == '-' && next == Some('-')
        {
            line_comment = true;
        } else if quote.is_none()
            && !line_comment
            && !block_comment
            && current == '/'
            && next == Some('*')
        {
            block_comment = true;
        } else if block_comment && current == '*' && next == Some('/') {
            buffer.push(current);
            buffer.push('/');
            index += 2;
            block_comment = false;
            continue;
        } else if !line_comment && !block_comment && matches!(current, '\'' | '"' | '`') {
            if quote == Some(current) {
                if next == Some(current) {
                    buffer.push(current);
                    buffer.push(current);
                    index += 2;
                    continue;
                }
                quote = None;
            } else if quote.is_none() {
                quote = Some(current);
            }
        }
        buffer.push(current);
        if current == ';' && quote.is_none() && !line_comment && !block_comment {
            let statement = buffer.trim().to_string();
            if !statement.is_empty() {
                statements.push(SqlStatement {
                    content: statement,
                    start_line,
                    end_line: line,
                });
            }
            buffer.clear();
        }
        index += 1;
    }
    if quote.is_some() || block_comment {
        return Err(AppError::InvalidInput(
            "SQL 包含未闭合的引号或块注释".to_string(),
        ));
    }
    let remaining = buffer.trim();
    if !remaining.is_empty() {
        statements.push(SqlStatement {
            content: remaining.to_string(),
            start_line,
            end_line: line,
        });
    }
    Ok(statements)
}

fn yaml_top_level_key(line: &str) -> Option<String> {
    if line.chars().next().is_some_and(char::is_whitespace) || line.trim_start().starts_with('#') {
        return None;
    }
    line.split_once(':')
        .map(|(key, _)| key.trim().trim_matches(['\'', '"']).to_string())
        .filter(|key| !key.is_empty())
}

fn normalized_chunk_options(
    options: Option<&KnowledgeChunkOptions>,
) -> Result<(usize, usize, usize), AppError> {
    let target = positive_usize(
        options.and_then(|value| value.target_chars),
        DEFAULT_TARGET_CHARS,
        "目标分块字符数",
    )?;
    let max = positive_usize(
        options.and_then(|value| value.max_chars),
        DEFAULT_MAX_CHARS,
        "最大分块字符数",
    )?;
    let overlap = non_negative_usize(
        options.and_then(|value| value.overlap_chars),
        DEFAULT_OVERLAP_CHARS,
        "重叠字符数",
    )?;
    if target > max {
        return Err(AppError::InvalidInput(
            "目标分块字符数不能大于最大分块字符数".to_string(),
        ));
    }
    if overlap >= max {
        return Err(AppError::InvalidInput(
            "重叠字符数必须小于最大分块字符数".to_string(),
        ));
    }
    Ok((target, max, overlap))
}

fn positive_usize(value: Option<i64>, default: usize, label: &str) -> Result<usize, AppError> {
    let value = value.unwrap_or(i64::try_from(default).unwrap_or(i64::MAX));
    if value <= 0 {
        return Err(AppError::InvalidInput(format!("{label}必须大于 0")));
    }
    usize::try_from(value).map_err(|_| AppError::InvalidInput(format!("{label}超出范围")))
}

fn non_negative_usize(value: Option<i64>, default: usize, label: &str) -> Result<usize, AppError> {
    let value = value.unwrap_or(i64::try_from(default).unwrap_or(i64::MAX));
    if value < 0 {
        return Err(AppError::InvalidInput(format!("{label}不能为负数")));
    }
    usize::try_from(value).map_err(|_| AppError::InvalidInput(format!("{label}超出范围")))
}

fn flush_blocks(
    chunks: &mut Vec<KnowledgeChunkWriteInput>,
    pending: &mut Vec<KnowledgeParsedBlock>,
    pending_chars: &mut usize,
    parsed: &KnowledgeParsedDocument,
) -> Result<(), AppError> {
    if pending.is_empty() {
        return Ok(());
    }
    let content = pending
        .iter()
        .map(|block| block.content.as_str())
        .collect::<Vec<_>>()
        .join("\n\n");
    let start_line = pending.first().map_or(1, |block| block.start_line);
    let end_line = pending.last().map_or(start_line, |block| block.end_line);
    let heading_path = pending
        .iter()
        .rev()
        .find(|block| !block.heading_path.is_empty())
        .map(|block| block.heading_path.join(" > "))
        .unwrap_or_default();
    let block_types = pending
        .iter()
        .map(|block| block.block_type.clone())
        .collect::<Vec<_>>();
    chunks.push(chunk_record(
        content,
        heading_path,
        start_line,
        end_line,
        serde_json::json!({
            "startLine": start_line,
            "endLine": end_line,
            "blockTypes": block_types,
            "parserId": parsed.parser_id,
            "normalizationVersion": parsed.normalization_version,
            "chunkStrategyId": STRUCTURE_CHUNK_STRATEGY_ID,
        }),
    )?);
    pending.clear();
    *pending_chars = 0;
    Ok(())
}

fn split_large_block(
    chunks: &mut Vec<KnowledgeChunkWriteInput>,
    block: &KnowledgeParsedBlock,
    max: usize,
    overlap: usize,
    parsed: &KnowledgeParsedDocument,
) -> Result<(), AppError> {
    let characters = block.content.chars().collect::<Vec<_>>();
    let mut start = 0_usize;
    while start < characters.len() {
        let end = (start + max).min(characters.len());
        let content = characters[start..end].iter().collect::<String>();
        chunks.push(chunk_record(
            content,
            block.heading_path.join(" > "),
            block.start_line,
            block.end_line,
            serde_json::json!({
                "startLine": block.start_line,
                "endLine": block.end_line,
                "blockTypes": [block.block_type.clone()],
                "parserId": parsed.parser_id,
                "normalizationVersion": parsed.normalization_version,
                "chunkStrategyId": STRUCTURE_CHUNK_STRATEGY_ID,
                "characterStart": start,
                "characterEnd": end,
            }),
        )?);
        if end == characters.len() {
            break;
        }
        start = end.saturating_sub(overlap);
    }
    Ok(())
}

fn chunk_record(
    content: String,
    heading_path: String,
    start_line: i64,
    end_line: i64,
    location: serde_json::Value,
) -> Result<KnowledgeChunkWriteInput, AppError> {
    let character_count = content.chars().count();
    let token_estimate = usize_to_i64(character_count.saturating_add(3) / 4);
    Ok(KnowledgeChunkWriteInput {
        chunk_index: 0,
        heading_path,
        content_hash: format!("{:x}", Sha256::digest(content.as_bytes())),
        content,
        location: merge_location(location, start_line, end_line),
        token_estimate,
    })
}

fn merge_location(
    mut location: serde_json::Value,
    start_line: i64,
    end_line: i64,
) -> serde_json::Value {
    if let Some(object) = location.as_object_mut() {
        object
            .entry("startLine")
            .or_insert_with(|| serde_json::json!(start_line));
        object
            .entry("endLine")
            .or_insert_with(|| serde_json::json!(end_line));
    }
    location
}

fn escape_json_pointer(value: &str) -> String {
    value.replace('~', "~0").replace('/', "~1")
}

fn usize_to_i64(value: usize) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

#[cfg(test)]
mod tests {
    use super::{default_parser_registry, KnowledgeParserService, STRUCTURE_CHUNK_STRATEGY_ID};
    use crate::models::{KnowledgeChunkOptions, KnowledgeParseAndChunkInput, KnowledgeParseInput};
    use lopdf::dictionary;
    use std::io::Write;
    use zip::write::SimpleFileOptions;
    use zip::{CompressionMethod, ZipWriter};

    #[test]
    fn markdown_preserves_front_matter_headings_tables_and_code(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let result = KnowledgeParserService::parse_and_chunk(KnowledgeParseAndChunkInput {
            document: KnowledgeParseInput {
                source_path: "requirements/refund.md".to_string(),
                mime_type: "text/markdown".to_string(),
                content: "---\nversion: v2.3.1\n---\n# 退款审批\n\n| 字段 | 说明 |\n| --- | --- |\n| id | 编号 |\n\n```rust\nfn approve() {}\n```"
                    .to_string(),
                binary_content: None,
            },
            options: None,
        })?;
        assert_eq!(result.parsed.front_matter["version"], "v2.3.1");
        assert!(result
            .parsed
            .blocks
            .iter()
            .any(|block| block.block_type == "table"));
        assert!(result
            .parsed
            .blocks
            .iter()
            .any(|block| block.block_type == "code_block"));
        assert_eq!(result.chunk_strategy_id, STRUCTURE_CHUNK_STRATEGY_ID);
        assert!(!result.chunks.is_empty());
        Ok(())
    }

    #[test]
    fn markdown_unclosed_code_fence_is_indexed_with_a_warning(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let result = KnowledgeParserService::parse_and_chunk(KnowledgeParseAndChunkInput {
            document: KnowledgeParseInput {
                source_path: "legacy/invalid-fence.md".to_string(),
                mime_type: "text/markdown".to_string(),
                content: "# 历史脚本\n\n```shell\necho unfinished".to_string(),
                binary_content: None,
            },
            options: None,
        })?;
        let code = result
            .parsed
            .blocks
            .iter()
            .find(|block| block.block_type == "code_block")
            .expect("未闭合围栏仍应保留为代码块");
        assert_eq!(code.metadata["closed"], false);
        assert!(result
            .parsed
            .warnings
            .iter()
            .any(|warning| warning.contains("未闭合")));
        assert!(!result.chunks.is_empty());
        Ok(())
    }

    #[test]
    fn markdown_unclosed_front_matter_is_indexed_with_a_warning(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let result = KnowledgeParserService::parse_and_chunk(KnowledgeParseAndChunkInput {
            document: KnowledgeParseInput {
                source_path: "legacy/invalid-front-matter.md".to_string(),
                mime_type: "text/markdown".to_string(),
                content: "---\ntitle: 历史文档\n# 仍需索引的正文".to_string(),
                binary_content: None,
            },
            options: None,
        })?;
        assert!(result
            .parsed
            .warnings
            .iter()
            .any(|warning| warning.contains("front matter 缺少结束分隔符")));
        assert!(!result.chunks.is_empty());
        Ok(())
    }

    #[test]
    fn markdown_invalid_front_matter_is_preserved_without_blocking_body(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let result = KnowledgeParserService::parse_and_chunk(KnowledgeParseAndChunkInput {
            document: KnowledgeParseInput {
                source_path: "legacy/invalid-yaml.md".to_string(),
                mime_type: "text/markdown".to_string(),
                content: "---\ntitle: [未闭合\n---\n# 仍需索引的正文".to_string(),
                binary_content: None,
            },
            options: None,
        })?;
        assert_eq!(result.parsed.front_matter, serde_json::json!({}));
        let raw = result
            .parsed
            .blocks
            .iter()
            .find(|block| block.block_type == "front_matter_raw")
            .expect("无效 front matter 应保留原文块");
        assert_eq!(raw.metadata["valid"], false);
        assert!(result
            .parsed
            .blocks
            .iter()
            .any(|block| block.block_type == "heading" && block.content == "# 仍需索引的正文"));
        assert!(result
            .parsed
            .warnings
            .iter()
            .any(|warning| warning.contains("front matter 解析失败")));
        assert!(!result.chunks.is_empty());
        Ok(())
    }

    #[test]
    fn markdown_accepts_unquoted_markdown_link_in_front_matter() {
        let parsed = KnowledgeParserService::parse_and_chunk(KnowledgeParseAndChunkInput {
            document: KnowledgeParseInput {
                source_path: "需求文档.md".to_string(),
                mime_type: "text/markdown".to_string(),
                content: "---\ntype: converted-reference\nsource: [需求原件.docx](<../需求原件.docx>)\n---\n\n# 需求文档"
                    .to_string(),
                binary_content: None,
            },
            options: None,
        })
        .expect("转换生成的 Markdown 链接元数据应作为字符串解析");

        assert_eq!(
            parsed.parsed.front_matter["source"],
            "[需求原件.docx](<../需求原件.docx>)"
        );
    }

    #[test]
    fn remote_ocr_parser_only_indexes_non_empty_recognized_text(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let parsed = KnowledgeParserService::parse_and_chunk(KnowledgeParseAndChunkInput {
            document: KnowledgeParseInput {
                source_path: "退款流程.png".to_string(),
                mime_type: "application/x-knowledge-ocr".to_string(),
                content: "退款审批流程\n\n提交申请".to_string(),
                binary_content: None,
            },
            options: None,
        })?;
        assert_eq!(parsed.parsed.parser_id, "remote-image-ocr-parser-v1");
        assert_eq!(parsed.parsed.front_matter["recognition"], "remote_ocr");
        assert!(!parsed.chunks.is_empty());

        let empty = KnowledgeParserService::parse_and_chunk(KnowledgeParseAndChunkInput {
            document: KnowledgeParseInput {
                source_path: "空白.png".to_string(),
                mime_type: "application/x-knowledge-ocr".to_string(),
                content: "   ".to_string(),
                binary_content: None,
            },
            options: None,
        });
        assert!(empty.is_err(), "OCR 空结果不得被索引为成功文档");
        Ok(())
    }

    #[test]
    fn local_ocr_parser_marks_recognition_as_local() -> Result<(), Box<dyn std::error::Error>> {
        let parsed = KnowledgeParserService::parse_and_chunk(KnowledgeParseAndChunkInput {
            document: KnowledgeParseInput {
                source_path: "退款流程.png".to_string(),
                mime_type: "application/x-knowledge-local-ocr".to_string(),
                content: "退款审批流程\n\n提交申请".to_string(),
                binary_content: None,
            },
            options: None,
        })?;
        assert_eq!(parsed.parsed.parser_id, "local-image-ocr-parser-v1");
        assert_eq!(parsed.parsed.front_matter["recognition"], "local_ocr");
        assert!(!parsed.chunks.is_empty());
        Ok(())
    }

    #[test]
    fn image_metadata_parser_keeps_dimensions_without_claiming_ocr_text(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let parsed = KnowledgeParserService::parse_and_chunk(KnowledgeParseAndChunkInput {
            document: KnowledgeParseInput {
                source_path: "退款流程.png".to_string(),
                mime_type: "image/png".to_string(),
                content: String::new(),
                binary_content: Some(
                    b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR\0\0\x04\0\0\0\x03\0fixture".to_vec(),
                ),
            },
            options: None,
        })?;
        assert_eq!(parsed.parsed.parser_id, "image-metadata-parser-v1");
        assert_eq!(parsed.parsed.front_matter["width"], 1024);
        assert_eq!(parsed.parsed.front_matter["height"], 768);
        assert_eq!(parsed.parsed.front_matter["textExtraction"], "unavailable");
        assert!(parsed
            .parsed
            .warnings
            .iter()
            .any(|warning| warning.contains("未提取图片文字")));
        assert!(parsed.parsed.normalized_content.contains("1024 × 768"));
        Ok(())
    }

    #[test]
    fn html_parser_sanitizes_untrusted_content_and_keeps_visible_structure(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let result = KnowledgeParserService::parse_and_chunk(KnowledgeParseAndChunkInput {
            document: KnowledgeParseInput {
                source_path: "prototype/审批.html".to_string(),
                mime_type: "text/html".to_string(),
                content: r#"<!doctype html>
<html><head><title>退款审批原型</title><script>window.evil = true</script></head>
<body><h1>提交退款</h1><p>请填写申请信息。</p>
<button onclick="steal()" aria-label="提交申请">提交</button>
<input name="reason" placeholder="退款原因" onfocus="steal()">
<img src="assets/refund.png"><a href="https://example.invalid/evil">危险链接</a>
<script>fetch('https://example.invalid')</script></body></html>"#
                    .to_string(),
                binary_content: None,
            },
            options: None,
        })?;
        assert_eq!(result.parsed.parser_id, "html-parser-v1");
        assert_eq!(result.parsed.front_matter["title"], "退款审批原型");
        assert!(result.parsed.normalized_content.contains("提交退款"));
        assert!(!result.parsed.normalized_content.contains("window.evil"));
        assert!(result.parsed.blocks.iter().any(|block| {
            block.block_type == "control" && block.metadata["label"] == "提交申请"
        }));
        assert!(result.parsed.blocks.iter().any(|block| {
            block.block_type == "resource_reference" && block.content == "assets/refund.png"
        }));
        assert!(!result.parsed.blocks.iter().any(|block| {
            block.block_type == "resource_reference" && block.content.contains("example.invalid")
        }));
        assert!(result
            .parsed
            .blocks
            .iter()
            .all(|block| block.start_line >= 1));
        Ok(())
    }

    #[test]
    fn docx_parser_extracts_structured_content_images_and_warnings(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut writer = ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        for (name, content) in [
            ("[Content_Types].xml", "<Types/>"),
            (
                "word/document.xml",
                r#"<w:document xmlns:w="w" xmlns:r="r"><w:body>
<w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>退款审批</w:t></w:r></w:p>
<w:p><w:r><w:t>申请说明</w:t></w:r></w:p>
<w:p><w:pPr><w:numPr/></w:pPr><w:r><w:t>第一项</w:t></w:r></w:p>
<w:tbl><w:tr><w:tc><w:p><w:r><w:t>字段</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>说明</w:t></w:r></w:p></w:tc></w:tr></w:tbl>
<w:p><w:r><w:drawing><a:blip r:embed="rId5"/></w:drawing></w:r></w:p>
</w:body></w:document>"#,
            ),
            (
                "word/_rels/document.xml.rels",
                r#"<Relationships><Relationship Id="rId5" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/image1.png"/><Relationship Id="rId6" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="https://example.invalid/image.png"/></Relationships>"#,
            ),
            ("word/embeddings/object.bin", "ignored"),
        ] {
            writer.start_file(name, options)?;
            writer.write_all(content.as_bytes())?;
        }
        let bytes = writer.finish()?.into_inner();
        let result = KnowledgeParserService::parse_and_chunk(KnowledgeParseAndChunkInput {
            document: KnowledgeParseInput {
                source_path: "设计.docx".to_string(),
                mime_type:
                    "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
                        .to_string(),
                content: String::new(),
                binary_content: Some(bytes),
            },
            options: None,
        })?;
        assert_eq!(result.parsed.parser_id, "docx-parser-v1");
        assert!(result
            .parsed
            .blocks
            .iter()
            .any(|block| block.block_type == "heading" && block.content == "退款审批"));
        assert!(result
            .parsed
            .blocks
            .iter()
            .any(|block| block.block_type == "paragraph" && block.content == "申请说明"));
        assert!(result
            .parsed
            .blocks
            .iter()
            .any(|block| block.block_type == "list_item" && block.content == "第一项"));
        assert!(result
            .parsed
            .blocks
            .iter()
            .any(|block| block.block_type == "table" && block.content.contains("字段 | 说明")));
        assert!(result
            .parsed
            .blocks
            .iter()
            .any(|block| block.block_type == "image_reference"
                && block.content == "media/image1.png"));
        assert!(!result
            .parsed
            .blocks
            .iter()
            .any(|block| block.content.contains("example.invalid")));
        assert!(result
            .parsed
            .warnings
            .iter()
            .any(|warning| warning.contains("嵌入对象")));
        Ok(())
    }

    #[test]
    fn xlsx_parser_preserves_cells_tables_named_ranges_and_formula_cache(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut writer = ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        for (name, content) in [
            (
                "[Content_Types].xml",
                r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/></Types>"#,
            ),
            (
                "_rels/.rels",
                r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#,
            ),
            (
                "xl/workbook.xml",
                r#"<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="项目数据" sheetId="1" r:id="rId1"/></sheets><definedNames><definedName name="金额区间">'项目数据'!$B$2:$B$3</definedName></definedNames></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>项目</t></is></c><c r="B1" t="inlineStr"><is><t>金额</t></is></c></row><row r="2"><c r="A2" t="inlineStr"><is><t>退款审批</t></is></c><c r="B2"><v>100</v></c></row><row r="3"><c r="A3" t="inlineStr"><is><t>合计</t></is></c><c r="B3"><f>SUM(B2:B2)</f><v>100</v></c></row></sheetData></worksheet>"#,
            ),
            (
                "xl/worksheets/_rels/sheet1.xml.rels",
                r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/table" Target="../tables/table1.xml"/></Relationships>"#,
            ),
            (
                "xl/tables/table1.xml",
                r#"<table xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" id="1" name="金额表" displayName="金额表" ref="A1:B3"><tableColumns count="2"><tableColumn id="1" name="项目"/><tableColumn id="2" name="金额"/></tableColumns></table>"#,
            ),
        ] {
            writer.start_file(name, options)?;
            writer.write_all(content.as_bytes())?;
        }
        let bytes = writer.finish()?.into_inner();
        let result = KnowledgeParserService::parse_and_chunk(KnowledgeParseAndChunkInput {
            document: KnowledgeParseInput {
                source_path: "项目数据.xlsx".to_string(),
                mime_type: "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
                    .to_string(),
                content: String::new(),
                binary_content: Some(bytes),
            },
            options: None,
        })?;
        assert_eq!(result.parsed.parser_id, "xlsx-parser-v1");
        assert!(result
            .parsed
            .normalized_content
            .contains("项目数据!A2: 退款审批"));
        assert!(result.parsed.blocks.iter().any(|block| {
            block.block_type == "cell"
                && block.metadata["sheet"] == "项目数据"
                && block.metadata["cell"] == "B3"
                && block.metadata["formula"] == "SUM(B2:B2)"
                && block.metadata["cachedValue"] == "100"
                && block.metadata["formulaEvaluated"] == false
        }));
        assert!(result.parsed.blocks.iter().any(|block| {
            block.block_type == "named_range"
                && block.metadata["name"] == "金额区间"
                && block.content.contains("$B$2:$B$3")
        }));
        assert!(result.parsed.blocks.iter().any(|block| {
            block.block_type == "table"
                && block.metadata["sheet"] == "项目数据"
                && block.metadata["name"] == "金额表"
                && block.metadata["reference"] == "A1:B3"
        }));
        Ok(())
    }

    #[test]
    fn pptx_parser_preserves_slide_order_notes_tables_and_images(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut writer = ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        for (name, content) in [
            ("[Content_Types].xml", r#"<Types/>"#),
            (
                "_rels/.rels",
                r#"<Relationships><Relationship Id="rId1" Type="officeDocument" Target="ppt/presentation.xml"/></Relationships>"#,
            ),
            (
                "ppt/presentation.xml",
                r#"<p:presentation xmlns:p="p" xmlns:r="r"><p:sldIdLst><p:sldId id="256" r:id="rId1"/></p:sldIdLst></p:presentation>"#,
            ),
            (
                "ppt/_rels/presentation.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide" Target="slides/slide1.xml"/></Relationships>"#,
            ),
            (
                "ppt/slides/slide1.xml",
                r#"<p:sld xmlns:p="p" xmlns:a="a"><p:cSld><p:spTree><p:sp><p:txBody><a:p><a:r><a:t>项目概览</a:t></a:r></a:p></p:txBody></p:sp><a:tbl><a:tr><a:tc><a:txBody><a:p><a:r><a:t>模块</a:t></a:r></a:p></a:txBody></a:tc><a:tc><a:txBody><a:p><a:r><a:t>状态</a:t></a:r></a:p></a:txBody></a:tc></a:tr></a:tbl></p:spTree></p:cSld></p:sld>"#,
            ),
            (
                "ppt/slides/_rels/slide1.xml.rels",
                r#"<Relationships><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/notesSlide" Target="../notesSlides/notesSlide1.xml"/><Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="../media/image1.png"/><Relationship Id="rId4" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="https://example.invalid/image.png"/></Relationships>"#,
            ),
            (
                "ppt/notesSlides/notesSlide1.xml",
                r#"<p:notes xmlns:p="p" xmlns:a="a"><a:p><a:r><a:t>演讲备注</a:t></a:r></a:p></p:notes>"#,
            ),
            ("ppt/media/image1.png", "not-decoded-image"),
            ("ppt/embeddings/object.bin", "ignored"),
        ] {
            writer.start_file(name, options)?;
            writer.write_all(content.as_bytes())?;
        }
        let result = KnowledgeParserService::parse_and_chunk(KnowledgeParseAndChunkInput {
            document: KnowledgeParseInput {
                source_path: "项目概览.pptx".to_string(),
                mime_type:
                    "application/vnd.openxmlformats-officedocument.presentationml.presentation"
                        .to_string(),
                content: String::new(),
                binary_content: Some(writer.finish()?.into_inner()),
            },
            options: None,
        })?;
        assert_eq!(result.parsed.parser_id, "pptx-parser-v1");
        assert!(result.parsed.normalized_content.contains("[第 1 页]"));
        assert!(result.parsed.blocks.iter().any(|block| {
            block.block_type == "slide"
                && block.content.contains("项目概览")
                && block.metadata["slideNumber"] == 1
        }));
        assert!(result.parsed.blocks.iter().any(|block| {
            block.block_type == "table" && block.content.contains("模块 | 状态")
        }));
        assert!(result.parsed.blocks.iter().any(|block| {
            block.block_type == "speaker_notes" && block.content == "演讲备注"
        }));
        assert!(result.parsed.blocks.iter().any(|block| {
            block.block_type == "image_reference" && block.content == "media/image1.png"
        }));
        assert!(!result
            .parsed
            .blocks
            .iter()
            .any(|block| block.content.contains("example.invalid")));
        assert!(result
            .parsed
            .warnings
            .iter()
            .any(|warning| warning.contains("嵌入对象")));
        Ok(())
    }

    #[test]
    fn pdf_parser_extracts_text_layer_and_marks_scanned_pages_for_ocr(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut document = lopdf::Document::with_version("1.5");
        let pages_id = document.new_object_id();
        let font_id = document.add_object(dictionary! {
            "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Courier",
        });
        let resources_id = document.add_object(dictionary! {
            "Font" => dictionary! { "F1" => font_id },
        });
        let content = lopdf::content::Content {
            operations: vec![
                lopdf::content::Operation::new("BT", vec![]),
                lopdf::content::Operation::new("Tf", vec!["F1".into(), 18.into()]),
                lopdf::content::Operation::new("Td", vec![72.into(), 720.into()]),
                lopdf::content::Operation::new(
                    "Tj",
                    vec![lopdf::Object::string_literal("Project overview")],
                ),
                lopdf::content::Operation::new("ET", vec![]),
            ],
        };
        let content_id = document.add_object(lopdf::Stream::new(dictionary! {}, content.encode()?));
        let text_page_id = document.add_object(dictionary! {
            "Type" => "Page", "Parent" => pages_id, "Contents" => content_id,
        });
        let scanned_page_id = document.add_object(dictionary! {
            "Type" => "Page", "Parent" => pages_id,
        });
        document.objects.insert(
            pages_id,
            lopdf::Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![text_page_id.into(), scanned_page_id.into()],
                "Count" => 2,
                "Resources" => resources_id,
                "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
            }),
        );
        let catalog_id = document.add_object(dictionary! {
            "Type" => "Catalog", "Pages" => pages_id,
        });
        document.trailer.set("Root", catalog_id);
        let mut bytes = Vec::new();
        document.save_to(&mut bytes)?;
        let result = KnowledgeParserService::parse_and_chunk(KnowledgeParseAndChunkInput {
            document: KnowledgeParseInput {
                source_path: "项目说明.pdf".to_string(),
                mime_type: "application/pdf".to_string(),
                content: String::new(),
                binary_content: Some(bytes),
            },
            options: None,
        })?;
        assert_eq!(result.parsed.parser_id, "pdf-parser-v1");
        assert!(result
            .parsed
            .normalized_content
            .contains("Project overview"));
        assert!(result.parsed.blocks.iter().any(|block| {
            block.block_type == "pdf_page"
                && block.metadata["pageNumber"] == 1
                && block.metadata["requiresOcr"] == false
        }));
        assert!(result.parsed.blocks.iter().any(|block| {
            block.block_type == "ocr_required"
                && block.metadata["pageNumber"] == 2
                && block.metadata["requiresOcr"] == true
                && block.content.contains("需要 OCR")
        }));
        Ok(())
    }

    #[test]
    fn format_parsers_validate_json_yaml_and_sql() -> Result<(), Box<dyn std::error::Error>> {
        let json = KnowledgeParserService::parse_and_chunk(KnowledgeParseAndChunkInput {
            document: KnowledgeParseInput {
                source_path: "config.json".to_string(),
                mime_type: "application/json".to_string(),
                content: r#"{"api":{"path":"/refund"},"enabled":true}"#.to_string(),
                binary_content: None,
            },
            options: None,
        })?;
        assert_eq!(json.parsed.blocks.len(), 2);

        let yaml = KnowledgeParserService::parse_and_chunk(KnowledgeParseAndChunkInput {
            document: KnowledgeParseInput {
                source_path: "config.yaml".to_string(),
                mime_type: "application/yaml".to_string(),
                content: "server:\n  port: 8080\nfeature:\n  enabled: true".to_string(),
                binary_content: None,
            },
            options: None,
        })?;
        assert_eq!(yaml.parsed.blocks.len(), 2);

        let sql = KnowledgeParserService::parse_and_chunk(KnowledgeParseAndChunkInput {
            document: KnowledgeParseInput {
                source_path: "migration.sql".to_string(),
                mime_type: "application/sql".to_string(),
                content: "INSERT INTO demo(value) VALUES ('a;b');\nSELECT * FROM demo;".to_string(),
                binary_content: None,
            },
            options: None,
        })?;
        assert_eq!(sql.parsed.blocks.len(), 2);

        let invalid_json = KnowledgeParserService::parse_and_chunk(KnowledgeParseAndChunkInput {
            document: KnowledgeParseInput {
                source_path: "broken.json".to_string(),
                mime_type: "application/json".to_string(),
                content: "{broken".to_string(),
                binary_content: None,
            },
            options: None,
        });
        assert!(invalid_json.is_err());
        Ok(())
    }

    #[test]
    fn parser_registry_keeps_existing_format_priority_and_ids(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let registry = default_parser_registry();
        for (source_path, mime_type, expected_parser_id) in [
            ("requirements.md", "text/plain", "markdown-parser-v1"),
            (
                "requirements.markdown",
                "application/octet-stream",
                "markdown-parser-v1",
            ),
            ("release.mdown", "text/x-markdown", "markdown-parser-v1"),
            ("guide.mkdn", "text/plain", "markdown-parser-v1"),
            ("component.mdx", "text/plain", "markdown-parser-v1"),
            ("events.log", "text/plain", "log-parser-v1"),
            ("settings.txt", "application/json", "json-parser-v1"),
            ("config.yml", "application/octet-stream", "yaml-parser-v1"),
            ("migration.sql", "text/plain", "sql-parser-v1"),
            ("notes.txt", "text/plain", "text-parser-v1"),
        ] {
            let parser = registry.resolve(&KnowledgeParseInput {
                source_path: source_path.to_string(),
                mime_type: mime_type.to_string(),
                content: String::new(),
                binary_content: None,
            })?;
            assert_eq!(parser.parser_id(), expected_parser_id);
        }

        let unsupported = registry.resolve(&KnowledgeParseInput {
            source_path: "archive.bin".to_string(),
            mime_type: "application/octet-stream".to_string(),
            content: String::new(),
            binary_content: None,
        });
        assert!(unsupported.is_err());
        Ok(())
    }

    #[test]
    fn text_and_log_parser_outputs_remain_stable() -> Result<(), Box<dyn std::error::Error>> {
        let text = KnowledgeParserService::parse_and_chunk(KnowledgeParseAndChunkInput {
            document: KnowledgeParseInput {
                source_path: "notes.txt".to_string(),
                mime_type: "text/plain".to_string(),
                content: "第一段\n继续\n\n第二段".to_string(),
                binary_content: None,
            },
            options: None,
        })?;
        assert_eq!(text.parsed.parser_id, "text-parser-v1");
        assert_eq!(text.parsed.blocks.len(), 2);
        assert_eq!(text.parsed.blocks[0].block_type, "paragraph");
        assert_eq!(text.parsed.blocks[0].start_line, 1);
        assert_eq!(text.parsed.blocks[1].start_line, 4);

        let log = KnowledgeParserService::parse_and_chunk(KnowledgeParseAndChunkInput {
            document: KnowledgeParseInput {
                source_path: "service.log".to_string(),
                mime_type: "text/plain".to_string(),
                content: "INFO started\n\nWARN slow query".to_string(),
                binary_content: None,
            },
            options: None,
        })?;
        assert_eq!(log.parsed.parser_id, "log-parser-v1");
        assert_eq!(log.parsed.blocks.len(), 2);
        assert!(log
            .parsed
            .blocks
            .iter()
            .all(|block| block.block_type == "log_line"));
        assert_eq!(log.parsed.blocks[1].start_line, 3);
        Ok(())
    }

    #[test]
    fn chunker_is_deterministic_bounded_and_overlapping() -> Result<(), Box<dyn std::error::Error>>
    {
        let input = KnowledgeParseAndChunkInput {
            document: KnowledgeParseInput {
                source_path: "large.txt".to_string(),
                mime_type: "text/plain".to_string(),
                content: "abcdefghij".repeat(40),
                binary_content: None,
            },
            options: Some(KnowledgeChunkOptions {
                target_chars: Some(80),
                max_chars: Some(100),
                overlap_chars: Some(20),
            }),
        };
        let first = KnowledgeParserService::parse_and_chunk(input.clone())?;
        let second = KnowledgeParserService::parse_and_chunk(input)?;
        assert_eq!(first.chunks.len(), 5);
        assert_eq!(
            first
                .chunks
                .iter()
                .map(|chunk| chunk.content_hash.clone())
                .collect::<Vec<_>>(),
            second
                .chunks
                .iter()
                .map(|chunk| chunk.content_hash.clone())
                .collect::<Vec<_>>()
        );
        assert!(first
            .chunks
            .iter()
            .all(|chunk| chunk.content.chars().count() <= 100));
        Ok(())
    }

    #[test]
    fn retrieval_baseline_fixture_covers_exact_semantic_and_conflict_cases(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let fixture = serde_json::from_str::<serde_json::Value>(include_str!(
            "../../tests/fixtures/knowledge_retrieval_baseline.json"
        ))?;
        let query_ids = fixture["queries"]
            .as_array()
            .ok_or("评测查询必须是数组")?
            .iter()
            .filter_map(|query| query["id"].as_str())
            .collect::<std::collections::HashSet<_>>();
        for required in [
            "exact-requirement-id",
            "chinese-semantic-term",
            "source-path",
            "code-symbol",
            "api-route",
            "field-and-sql",
            "version-isolation",
            "conflicting-evidence",
        ] {
            assert!(query_ids.contains(required), "缺少评测场景: {required}");
        }
        Ok(())
    }
}
