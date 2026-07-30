use crate::database::Database;
use crate::error::AppError;
use crate::models::{
    AiUnrestrictedState, EnableAiUnrestrictedInput, SystemSettings, SystemSettingsExportResult,
    UpdateSystemSettingsInput,
};
use chrono::{DateTime, Duration, Utc};
use regex::Regex;
use std::collections::HashSet;
use tauri_plugin_autostart::ManagerExt;

pub struct SystemSettingsService;

const KEY_THEME: &str = "settings.theme";
const KEY_AUTO_UPDATE: &str = "settings.auto_update";
const KEY_MCP_ENABLED: &str = "settings.mcp_enabled";
const KEY_LAUNCH_ON_STARTUP: &str = "settings.launch_on_startup";
const KEY_AUDIT_RETENTION_DAYS: &str = "settings.audit_retention_days";
const KEY_LOG_LEVEL: &str = "settings.log_level";
const KEY_BACKUP_DIR: &str = "settings.backup_dir";
const KEY_DATABASE_DOWNLOAD_DIR: &str = "settings.database_download_dir";
const KEY_PLATFORM: &str = "settings.platform";
const KEY_CLOSE_BEHAVIOR: &str = "settings.close_behavior";
const KEY_LANGUAGE: &str = "settings.language";
const KEY_AI_UNRESTRICTED_UNTIL: &str = "settings.ai_unrestricted_until";
const KEY_DANGEROUS_COMMANDS: &str = "settings.dangerous_commands";

const IMMUTABLE_DANGEROUS_PATTERNS: &[&str] = &[
    "rm -rf /",
    "mkfs.",
    ":(){:|:&};:",
    "shutdown",
    "poweroff",
    "halt",
    "dd if=",
];

impl SystemSettingsService {
    pub fn get(db: &Database) -> Result<SystemSettings, AppError> {
        Ok(SystemSettings {
            theme: get_value(db, KEY_THEME, "system")?,
            auto_update: get_bool(db, KEY_AUTO_UPDATE, true)?,
            mcp_enabled: Self::is_mcp_enabled(db)?,
            launch_on_startup: get_bool(db, KEY_LAUNCH_ON_STARTUP, false)?,
            audit_retention_days: get_i64(db, KEY_AUDIT_RETENTION_DAYS, 90)?,
            log_level: get_value(db, KEY_LOG_LEVEL, "info")?,
            backup_dir: get_value(db, KEY_BACKUP_DIR, "应用数据目录 / backups")?,
            database_download_dir: get_value(
                db,
                KEY_DATABASE_DOWNLOAD_DIR,
                &default_database_download_dir(),
            )?,
            platform: get_value(db, KEY_PLATFORM, "macos-windows")?,
            close_behavior: get_value(db, KEY_CLOSE_BEHAVIOR, "minimize")?,
            language: get_value(db, KEY_LANGUAGE, "zh-CN")?,
            ai_unrestricted_until: get_optional_value(db, KEY_AI_UNRESTRICTED_UNTIL)?,
            dangerous_commands: get_dangerous_commands(db)?,
        })
    }

    pub fn update(
        db: &Database,
        mut input: UpdateSystemSettingsInput,
    ) -> Result<SystemSettings, AppError> {
        normalize(&mut input);
        validate(&input)?;
        let ai_unrestricted_active = input
            .ai_unrestricted_until
            .as_ref()
            .is_some_and(|until| ai_unrestricted_state_from_until(Some(until.clone())).active);
        db.set_config(KEY_THEME, &input.theme)?;
        db.set_config(KEY_AUTO_UPDATE, bool_text(input.auto_update))?;
        db.set_config(KEY_MCP_ENABLED, bool_text(input.mcp_enabled))?;
        db.set_config(KEY_LAUNCH_ON_STARTUP, bool_text(input.launch_on_startup))?;
        db.set_config(
            KEY_AUDIT_RETENTION_DAYS,
            &input.audit_retention_days.to_string(),
        )?;
        db.set_config(KEY_LOG_LEVEL, &input.log_level)?;
        db.set_config(KEY_BACKUP_DIR, &input.backup_dir)?;
        db.set_config(KEY_DATABASE_DOWNLOAD_DIR, &input.database_download_dir)?;
        db.set_config(KEY_PLATFORM, &input.platform)?;
        db.set_config(KEY_CLOSE_BEHAVIOR, &input.close_behavior)?;
        db.set_config(KEY_LANGUAGE, &input.language)?;
        if ai_unrestricted_active {
            db.enable_ai_unrestricted_and_approve_pending(
                input.ai_unrestricted_until.as_deref().unwrap_or_default(),
            )?;
        } else {
            db.set_config(
                KEY_AI_UNRESTRICTED_UNTIL,
                input.ai_unrestricted_until.as_deref().unwrap_or(""),
            )?;
        }
        db.set_config(
            KEY_DANGEROUS_COMMANDS,
            &serde_json::to_string(&input.dangerous_commands)?,
        )?;
        Self::get(db)
    }

    pub fn reset(db: &Database) -> Result<SystemSettings, AppError> {
        Self::update(
            db,
            UpdateSystemSettingsInput {
                theme: "system".into(),
                auto_update: true,
                mcp_enabled: default_mcp_enabled(),
                launch_on_startup: false,
                audit_retention_days: 90,
                log_level: "info".into(),
                backup_dir: "应用数据目录 / backups".into(),
                database_download_dir: default_database_download_dir(),
                platform: "macos-windows".into(),
                close_behavior: "minimize".into(),
                language: "zh-CN".into(),
                ai_unrestricted_until: None,
                dangerous_commands: default_dangerous_commands(),
            },
        )
    }

    pub fn export(db: &Database) -> Result<SystemSettingsExportResult, AppError> {
        let settings = Self::get(db)?;
        Ok(SystemSettingsExportResult {
            file_name: format!(
                "tauri-ssh-settings-{}.json",
                chrono::Local::now().format("%Y%m%d%H%M%S")
            ),
            content: serde_json::to_string_pretty(&settings)?,
        })
    }

    pub fn get_with_autostart(
        db: &Database,
        app: &tauri::AppHandle,
    ) -> Result<SystemSettings, AppError> {
        let mut settings = Self::get(db)?;
        settings.launch_on_startup = Self::is_launch_on_startup_enabled(app)?;
        db.set_config(KEY_LAUNCH_ON_STARTUP, bool_text(settings.launch_on_startup))?;
        Ok(settings)
    }

    pub fn update_with_autostart(
        db: &Database,
        app: &tauri::AppHandle,
        mut input: UpdateSystemSettingsInput,
    ) -> Result<SystemSettings, AppError> {
        input.launch_on_startup = Self::set_launch_on_startup(app, input.launch_on_startup)?;
        Self::update(db, input)
    }

    pub fn reset_with_autostart(
        db: &Database,
        app: &tauri::AppHandle,
    ) -> Result<SystemSettings, AppError> {
        Self::set_launch_on_startup(app, false)?;
        Self::reset(db)
    }

    pub fn is_mcp_enabled(db: &Database) -> Result<bool, AppError> {
        get_bool(db, KEY_MCP_ENABLED, default_mcp_enabled())
    }

    fn is_launch_on_startup_enabled(app: &tauri::AppHandle) -> Result<bool, AppError> {
        app.autolaunch()
            .is_enabled()
            .map_err(|e| AppError::Custom(format!("读取开机自启动状态失败: {}", e)))
    }

    fn set_launch_on_startup(app: &tauri::AppHandle, enabled: bool) -> Result<bool, AppError> {
        let manager = app.autolaunch();
        if enabled {
            manager
                .enable()
                .map_err(|e| AppError::Custom(format!("启用开机自启动失败: {}", e)))?;
        } else {
            manager
                .disable()
                .map_err(|e| AppError::Custom(format!("关闭开机自启动失败: {}", e)))?;
        }
        manager
            .is_enabled()
            .map_err(|e| AppError::Custom(format!("读取开机自启动状态失败: {}", e)))
    }

    pub fn get_ai_unrestricted_state(db: &Database) -> Result<AiUnrestrictedState, AppError> {
        Ok(ai_unrestricted_state_from_until(get_optional_value(
            db,
            KEY_AI_UNRESTRICTED_UNTIL,
        )?))
    }

    pub fn enable_ai_unrestricted_mode(
        db: &Database,
        input: EnableAiUnrestrictedInput,
    ) -> Result<AiUnrestrictedState, AppError> {
        let minutes = input.minutes.unwrap_or(30).clamp(1, 30);
        let until = Utc::now() + Duration::minutes(minutes);
        db.enable_ai_unrestricted_and_approve_pending(&until.to_rfc3339())?;
        Self::get_ai_unrestricted_state(db)
    }

    pub fn disable_ai_unrestricted_mode(db: &Database) -> Result<AiUnrestrictedState, AppError> {
        db.set_config(KEY_AI_UNRESTRICTED_UNTIL, "")?;
        Self::get_ai_unrestricted_state(db)
    }

    pub fn dangerous_command_match(
        db: &Database,
        command: &str,
    ) -> Result<Option<String>, AppError> {
        let normalized = command.trim().to_lowercase();
        if normalized.is_empty() {
            return Ok(None);
        }
        if normalized.contains("dd if=") && normalized.contains(" of=") {
            return Ok(Some("dd if= ... of=".into()));
        }
        for pattern in IMMUTABLE_DANGEROUS_PATTERNS {
            if dangerous_pattern_matches(pattern, &normalized) {
                return Ok(Some((*pattern).into()));
            }
        }
        for pattern in get_dangerous_commands(db)? {
            if dangerous_pattern_matches(&pattern, &normalized) {
                return Ok(Some(pattern));
            }
        }
        Ok(None)
    }
}

fn normalize(input: &mut UpdateSystemSettingsInput) {
    input.theme = input.theme.trim().to_string();
    input.log_level = input.log_level.trim().to_lowercase();
    input.backup_dir = input.backup_dir.trim().to_string();
    input.database_download_dir = input.database_download_dir.trim().to_string();
    input.platform = input.platform.trim().to_string();
    input.close_behavior = input.close_behavior.trim().to_lowercase();
    input.language = input.language.trim().to_string();
    input.ai_unrestricted_until = input
        .ai_unrestricted_until
        .as_ref()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    input.dangerous_commands = normalize_dangerous_commands(&input.dangerous_commands);
}

fn validate(input: &UpdateSystemSettingsInput) -> Result<(), AppError> {
    if !["system", "light", "dark"].contains(&input.theme.as_str()) {
        return Err(AppError::InvalidInput("主题设置无效".into()));
    }
    if !(1..=3650).contains(&input.audit_retention_days) {
        return Err(AppError::InvalidInput(
            "审计保留天数必须在 1-3650 之间".into(),
        ));
    }
    if !["debug", "info", "warn", "error"].contains(&input.log_level.as_str()) {
        return Err(AppError::InvalidInput("日志级别无效".into()));
    }
    if input.backup_dir.is_empty() {
        return Err(AppError::InvalidInput("备份位置不能为空".into()));
    }
    if input.database_download_dir.is_empty() {
        return Err(AppError::InvalidInput("数据库导出下载目录不能为空".into()));
    }
    if input.platform != "macos-windows" {
        return Err(AppError::InvalidInput(
            "首发平台当前仅支持 macOS + Windows".into(),
        ));
    }
    if !["minimize", "exit"].contains(&input.close_behavior.as_str()) {
        return Err(AppError::InvalidInput("关闭行为无效".into()));
    }
    if !["zh-CN", "en-US"].contains(&input.language.as_str()) {
        return Err(AppError::InvalidInput("语言设置无效".into()));
    }
    if let Some(until) = &input.ai_unrestricted_until {
        DateTime::parse_from_rfc3339(until)
            .map_err(|_| AppError::InvalidInput("AI 临时放行截止时间必须为 RFC3339 时间".into()))?;
    }
    if input.dangerous_commands.is_empty() {
        return Err(AppError::InvalidInput("危险命令黑名单不能为空".into()));
    }
    Ok(())
}

fn get_value(db: &Database, key: &str, fallback: &str) -> Result<String, AppError> {
    Ok(db.get_config(key)?.unwrap_or_else(|| fallback.into()))
}

fn get_optional_value(db: &Database, key: &str) -> Result<Option<String>, AppError> {
    Ok(db
        .get_config(key)?
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty()))
}

fn get_bool(db: &Database, key: &str, fallback: bool) -> Result<bool, AppError> {
    Ok(db
        .get_config(key)?
        .and_then(|value| value.parse::<bool>().ok())
        .unwrap_or(fallback))
}

fn get_i64(db: &Database, key: &str, fallback: i64) -> Result<i64, AppError> {
    Ok(db
        .get_config(key)?
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(fallback))
}

fn bool_text(value: bool) -> &'static str {
    if value {
        "true"
    } else {
        "false"
    }
}

fn default_mcp_enabled() -> bool {
    !cfg!(debug_assertions)
}

fn default_database_download_dir() -> String {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_default();
    if !home.trim().is_empty() {
        return std::path::Path::new(&home)
            .join("Downloads")
            .to_string_lossy()
            .to_string();
    }
    "应用数据目录 / database-downloads".into()
}

fn default_dangerous_commands() -> Vec<String> {
    [
        r"(?:^|[\s;&|])rm\s+-[a-z]*r[a-z]*f?[a-z]*\s+(?:/|~|\$home|\*)",
        r"(?:^|[\s;&|])rm\s+-[a-z]*f[a-z]*r[a-z]*\s+(?:/|~|\$home|\*)",
        r"\bmkfs[\.\w]*\b",
        r"\bmke2fs\b",
        r"\bwipefs\b",
        r"\bdd\b[^\n]*\bof=/dev/",
        r":\s*\(\s*\)\s*\{\s*:\s*\|\s*:\s*&\s*\}\s*;\s*:",
        r">\s*/dev/sd[a-z]",
        r"\bchmod\s+-r\s+0*777\s+/(?:\s|$)",
        r"\bchown\s+-r\s+\w+\s+/(?:\s|$)",
        r"\bshutdown\b",
        r"\bpoweroff\b",
        r"\bhalt\b",
        r"\breboot\b",
        r"\binit\s+0\b",
        r"\biptables\s+-f\b",
        r"\bfirewall-cmd\b.*--reload\b",
        r"\b(drop\s+database|drop\s+schema)\b",
        r"\bdrop\s+table\b",
        r"\btruncate\s+table\b",
        r"\bflushall\b",
        r"\bflushdb\b",
        r"\b(curl|wget)\b.*\|\s*(sh|bash|zsh)\b",
        r"\b(find)\b.*\s-delete\b",
    ]
    .into_iter()
    .map(String::from)
    .collect()
}

fn get_dangerous_commands(db: &Database) -> Result<Vec<String>, AppError> {
    let defaults = default_dangerous_commands();
    let Some(raw) = db.get_config(KEY_DANGEROUS_COMMANDS)? else {
        return Ok(defaults);
    };
    let parsed = serde_json::from_str::<Vec<String>>(&raw).unwrap_or_else(|_| {
        raw.lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(String::from)
            .collect()
    });
    let mut merged = defaults;
    merged.extend(parsed);
    let normalized = normalize_dangerous_commands(&merged);
    if normalized.is_empty() {
        Ok(default_dangerous_commands())
    } else {
        Ok(normalized)
    }
}

fn normalize_dangerous_commands(commands: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    commands
        .iter()
        .map(|item| item.trim())
        .filter(|item| !item.is_empty())
        .filter_map(|item| {
            let key = item.to_lowercase();
            if seen.insert(key) {
                Some(item.to_string())
            } else {
                None
            }
        })
        .collect()
}

fn dangerous_pattern_matches(pattern: &str, normalized_command: &str) -> bool {
    let value = pattern.trim().to_lowercase();
    if value.is_empty() {
        return false;
    }
    Regex::new(&value)
        .map(|regex| regex.is_match(normalized_command))
        .unwrap_or_else(|_| normalized_command.contains(&value))
}

fn ai_unrestricted_state_from_until(until: Option<String>) -> AiUnrestrictedState {
    let Some(until_text) = until else {
        return AiUnrestrictedState {
            active: false,
            until: None,
            remaining_seconds: 0,
        };
    };
    let Ok(until_time) = DateTime::parse_from_rfc3339(&until_text) else {
        return AiUnrestrictedState {
            active: false,
            until: None,
            remaining_seconds: 0,
        };
    };
    let remaining = until_time.with_timezone(&Utc) - Utc::now();
    let remaining_seconds = remaining.num_seconds().max(0);
    AiUnrestrictedState {
        active: remaining_seconds > 0,
        until: if remaining_seconds > 0 {
            Some(until_text)
        } else {
            None
        },
        remaining_seconds,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{CreateApprovalRequestInput, UpdateSystemSettingsInput};

    #[test]
    fn update_with_active_ai_unrestricted_until_approves_existing_queue() {
        let db = Database::init(":memory:").expect("init db");
        let approval = db
            .create_approval_request(&CreateApprovalRequestInput {
                source: "test".into(),
                requester: "test".into(),
                server_alias: String::new(),
                action: "test_action".into(),
                risk: "blocked".into(),
                command: "rm -rf /".into(),
                resource: "test".into(),
                reason: "test".into(),
                summary: "test".into(),
                payload_json: None,
                expires_at: None,
            })
            .expect("create approval");
        assert_eq!(approval.status, "pending");

        SystemSettingsService::update(
            &db,
            UpdateSystemSettingsInput {
                theme: "system".into(),
                auto_update: true,
                mcp_enabled: true,
                launch_on_startup: false,
                audit_retention_days: 90,
                log_level: "info".into(),
                backup_dir: "backups".into(),
                database_download_dir: "database-downloads".into(),
                platform: "macos-windows".into(),
                close_behavior: "minimize".into(),
                language: "zh-CN".into(),
                ai_unrestricted_until: Some((Utc::now() + Duration::minutes(10)).to_rfc3339()),
                dangerous_commands: default_dangerous_commands(),
            },
        )
        .expect("update system settings");

        let approval = db
            .get_approval_request(approval.id)
            .expect("get approval")
            .expect("approval exists");
        assert_eq!(approval.status, "approved");
        assert_eq!(approval.decided_by, "ai-unrestricted");
    }
}
