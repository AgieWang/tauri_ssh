//! macOS Vision 本地 OCR 适配器。
//!
//! 图片内容只会写入应用数据目录下的私有临时文件，并交给系统 Vision 框架读取；不会
//! 发送到网络。开发工具或 Vision 不可用时返回可降级状态，上传任务仍会以图片元数据
//! 方式完成，避免把本机能力缺失变成资料导入失败。

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use rand::RngCore;

use crate::error::AppError;

const LOCAL_OCR_TIMEOUT: Duration = Duration::from_secs(30);
const LOCAL_OCR_MAX_TEXT_CHARACTERS: usize = 60_000;
const LOCAL_OCR_TEMP_DIRECTORY: &str = "knowledge-ocr-tmp";
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// 适配器主动区分“识别成功”和“当前设备暂不能识别”。后者不是上传失败，调用方必须
/// 保留原始图片并按元数据建立索引。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LocalImageOcrOutcome {
    Recognized { engine: String, text: String },
    Unavailable { reason: String },
}

pub(crate) struct KnowledgeLocalOcrService;

impl KnowledgeLocalOcrService {
    /// Vision 不接收 stdin，因此仅为本次调用写入随机命名、受控权限的临时文件；无论
    /// 成功、超时还是失败均会清理该文件，不把图片正文留在普通临时目录。
    pub(crate) fn recognize_image(
        app_data_dir: &Path,
        mime_type: &str,
        bytes: &[u8],
    ) -> Result<LocalImageOcrOutcome, AppError> {
        if !is_supported_raster_image(mime_type) {
            return Ok(LocalImageOcrOutcome::Unavailable {
                reason: "当前图片格式不支持本机文字识别".to_string(),
            });
        }
        if bytes.is_empty() {
            return Err(AppError::InvalidInput(
                "图片内容为空，无法进行本机文字识别".to_string(),
            ));
        }
        if !cfg!(target_os = "macos") {
            return Ok(LocalImageOcrOutcome::Unavailable {
                reason: "当前系统暂不提供本机文字识别".to_string(),
            });
        }

        let temporary_file = match write_private_image_file(app_data_dir, mime_type, bytes) {
            Ok(path) => path,
            Err(error) => {
                log::warn!("本机 OCR 临时文件不可用: {error}");
                return Ok(LocalImageOcrOutcome::Unavailable {
                    reason: "本机文字识别临时处理不可用".to_string(),
                });
            }
        };
        let _cleanup = TemporaryImageFile::new(temporary_file.clone());
        let output = match run_macos_vision(&temporary_file) {
            Ok(output) => output,
            Err(error) => {
                log::warn!("本机 OCR 适配器不可用: {error}");
                return Ok(LocalImageOcrOutcome::Unavailable {
                    reason: "本机文字识别适配器不可用".to_string(),
                });
            }
        };
        let text = match normalize_recognized_text(&output) {
            Ok(text) => text,
            Err(error) => {
                log::warn!("本机 OCR 结果未通过限制: {error}");
                return Ok(LocalImageOcrOutcome::Unavailable {
                    reason: "本机识别结果超过安全限制".to_string(),
                });
            }
        };
        if text.is_empty() {
            return Ok(LocalImageOcrOutcome::Unavailable {
                reason: "本机未识别到可索引文字".to_string(),
            });
        }
        Ok(LocalImageOcrOutcome::Recognized {
            engine: "macos-vision".to_string(),
            text,
        })
    }
}

fn is_supported_raster_image(mime_type: &str) -> bool {
    matches!(
        mime_type.trim().to_ascii_lowercase().as_str(),
        "image/png" | "image/jpeg" | "image/webp" | "image/gif"
    )
}

fn write_private_image_file(
    app_data_dir: &Path,
    mime_type: &str,
    bytes: &[u8],
) -> Result<PathBuf, AppError> {
    let directory = app_data_dir.join(LOCAL_OCR_TEMP_DIRECTORY);
    fs::create_dir_all(&directory)?;
    let directory_metadata = fs::symlink_metadata(&directory)?;
    if directory_metadata.file_type().is_symlink() || !directory_metadata.is_dir() {
        return Err(AppError::InvalidInput(
            "本机文字识别临时目录不安全".to_string(),
        ));
    }
    let extension = match mime_type.trim().to_ascii_lowercase().as_str() {
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "image/webp" => "webp",
        "image/gif" => "gif",
        _ => {
            return Err(AppError::InvalidInput(
                "图片格式不支持本机文字识别".to_string(),
            ))
        }
    };
    let path = directory.join(format!(
        "ocr-{}-{}.{}",
        random_suffix(),
        TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed),
        extension
    ));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(path)
}

fn random_suffix() -> String {
    let mut random = [0_u8; 16];
    rand::rngs::OsRng.fill_bytes(&mut random);
    random.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn run_macos_vision(image_path: &Path) -> Result<String, AppError> {
    if !Path::new("/usr/bin/xcrun").is_file() {
        return Ok(String::new());
    }
    let mut child = Command::new("/usr/bin/xcrun")
        .args(["swift", "-", image_path.to_string_lossy().as_ref()])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| AppError::Custom(format!("本机文字识别启动失败: {error}")))?;
    let source = macos_vision_source();
    child
        .stdin
        .as_mut()
        .ok_or_else(|| AppError::Custom("本机文字识别无法写入适配器".to_string()))?
        .write_all(source.as_bytes())?;
    drop(child.stdin.take());

    let started = Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            if !status.success() {
                return Ok(String::new());
            }
            let output = child.wait_with_output()?;
            return String::from_utf8(output.stdout)
                .map_err(|_| AppError::Custom("本机文字识别返回了无效文本".to_string()));
        }
        if started.elapsed() >= LOCAL_OCR_TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            return Ok(String::new());
        }
        thread::sleep(Duration::from_millis(20));
    }
}

fn normalize_recognized_text(output: &str) -> Result<String, AppError> {
    let text = output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    if text.chars().count() > LOCAL_OCR_MAX_TEXT_CHARACTERS {
        return Err(AppError::InvalidInput(format!(
            "本机文字识别结果超过 {} 个字符限制",
            LOCAL_OCR_MAX_TEXT_CHARACTERS
        )));
    }
    Ok(text)
}

/// 固定的内嵌脚本不接收用户控制的源代码、参数或网络地址；只把上层写入的私有图片
/// 路径交给 Vision。识别语言覆盖中文和英文，便于团队文档中的中英混合界面截图。
fn macos_vision_source() -> &'static str {
    r#"import Foundation
import Vision

let path = CommandLine.arguments[1]
let data = try Data(contentsOf: URL(fileURLWithPath: path))
let request = VNRecognizeTextRequest()
request.recognitionLevel = .accurate
request.usesLanguageCorrection = true
request.recognitionLanguages = ["zh-Hans", "en-US"]
let handler = VNImageRequestHandler(data: data, options: [:])
try handler.perform([request])
for observation in request.results ?? [] {
    if let text = observation.topCandidates(1).first?.string {
        print(text)
    }
}
"#
}

struct TemporaryImageFile {
    path: PathBuf,
}

impl TemporaryImageFile {
    fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl Drop for TemporaryImageFile {
    fn drop(&mut self) {
        if let Err(error) = fs::remove_file(&self.path) {
            if error.kind() != std::io::ErrorKind::NotFound {
                log::warn!("清理本机 OCR 临时图片失败: {error}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{is_supported_raster_image, normalize_recognized_text};

    #[test]
    fn local_ocr_only_accepts_supported_raster_formats() {
        assert!(is_supported_raster_image("image/png"));
        assert!(is_supported_raster_image("image/jpeg"));
        assert!(!is_supported_raster_image("image/svg+xml"));
        assert!(!is_supported_raster_image("application/pdf"));
    }

    #[test]
    fn local_ocr_normalizes_blank_lines_without_losing_text() {
        let text = normalize_recognized_text("  退款审批  \n\n  提交申请  \n").unwrap();
        assert_eq!(text, "退款审批\n提交申请");
    }
}
