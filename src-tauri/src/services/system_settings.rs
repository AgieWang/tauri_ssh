use crate::database::Database;
use crate::error::AppError;
use crate::models::{SystemSettings, SystemSettingsExportResult, UpdateSystemSettingsInput};

pub struct SystemSettingsService;

const KEY_THEME: &str = "settings.theme";
const KEY_AUTO_UPDATE: &str = "settings.auto_update";
const KEY_AUDIT_RETENTION_DAYS: &str = "settings.audit_retention_days";
const KEY_LOG_LEVEL: &str = "settings.log_level";
const KEY_BACKUP_DIR: &str = "settings.backup_dir";
const KEY_PLATFORM: &str = "settings.platform";
const KEY_CLOSE_BEHAVIOR: &str = "settings.close_behavior";
const KEY_LANGUAGE: &str = "settings.language";

impl SystemSettingsService {
    pub fn get(db: &Database) -> Result<SystemSettings, AppError> {
        Ok(SystemSettings {
            theme: get_value(db, KEY_THEME, "system")?,
            auto_update: get_bool(db, KEY_AUTO_UPDATE, true)?,
            audit_retention_days: get_i64(db, KEY_AUDIT_RETENTION_DAYS, 90)?,
            log_level: get_value(db, KEY_LOG_LEVEL, "info")?,
            backup_dir: get_value(db, KEY_BACKUP_DIR, "应用数据目录 / backups")?,
            platform: get_value(db, KEY_PLATFORM, "macos-windows")?,
            close_behavior: get_value(db, KEY_CLOSE_BEHAVIOR, "minimize")?,
            language: get_value(db, KEY_LANGUAGE, "zh-CN")?,
        })
    }

    pub fn update(
        db: &Database,
        mut input: UpdateSystemSettingsInput,
    ) -> Result<SystemSettings, AppError> {
        normalize(&mut input);
        validate(&input)?;
        db.set_config(KEY_THEME, &input.theme)?;
        db.set_config(KEY_AUTO_UPDATE, bool_text(input.auto_update))?;
        db.set_config(
            KEY_AUDIT_RETENTION_DAYS,
            &input.audit_retention_days.to_string(),
        )?;
        db.set_config(KEY_LOG_LEVEL, &input.log_level)?;
        db.set_config(KEY_BACKUP_DIR, &input.backup_dir)?;
        db.set_config(KEY_PLATFORM, &input.platform)?;
        db.set_config(KEY_CLOSE_BEHAVIOR, &input.close_behavior)?;
        db.set_config(KEY_LANGUAGE, &input.language)?;
        Self::get(db)
    }

    pub fn reset(db: &Database) -> Result<SystemSettings, AppError> {
        Self::update(
            db,
            UpdateSystemSettingsInput {
                theme: "system".into(),
                auto_update: true,
                audit_retention_days: 90,
                log_level: "info".into(),
                backup_dir: "应用数据目录 / backups".into(),
                platform: "macos-windows".into(),
                close_behavior: "minimize".into(),
                language: "zh-CN".into(),
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
}

fn normalize(input: &mut UpdateSystemSettingsInput) {
    input.theme = input.theme.trim().to_string();
    input.log_level = input.log_level.trim().to_lowercase();
    input.backup_dir = input.backup_dir.trim().to_string();
    input.platform = input.platform.trim().to_string();
    input.close_behavior = input.close_behavior.trim().to_lowercase();
    input.language = input.language.trim().to_string();
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
    Ok(())
}

fn get_value(db: &Database, key: &str, fallback: &str) -> Result<String, AppError> {
    Ok(db.get_config(key)?.unwrap_or_else(|| fallback.into()))
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
