use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use thiserror::Error;

mod catalog;
mod install;

pub use catalog::*;
pub use install::*;

const SETTING_ONBOARDING_COMPLETE: &str = "onboarding_complete";
const SETTING_ONBOARDING_SELECTION: &str = "onboarding_selection";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Language {
    #[serde(rename = "zh-CN")]
    ZhCn,
    #[serde(rename = "zh-TW")]
    ZhTw,
    #[serde(rename = "en")]
    En,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OnboardingSelection {
    pub language: Language,
    pub data_directory: String,
    pub telemetry_enabled: bool,
    pub update_checks_enabled: bool,
    pub nat_detection_enabled: bool,
    pub instance_isolation_enabled: bool,
}

impl OnboardingSelection {
    #[must_use]
    pub fn recommended(data_directory: String) -> Self {
        Self {
            language: Language::ZhCn,
            data_directory,
            telemetry_enabled: false,
            update_checks_enabled: true,
            nat_detection_enabled: false,
            instance_isolation_enabled: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BootstrapState {
    pub requires_onboarding: bool,
    pub default_data_directory: String,
    pub defaults: OnboardingSelection,
    pub settings: Option<OnboardingSelection>,
}

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("无法访问本地状态目录：{0}")]
    Io(#[from] std::io::Error),
    #[error("无法读写本地状态数据库：{0}")]
    Database(#[from] rusqlite::Error),
    #[error("本地设置数据已损坏：{0}")]
    Serialization(#[from] serde_json::Error),
    #[error("数据位置不受支持：{0}")]
    InvalidDataDirectory(String),
    #[error("首次运行状态不完整，请重新完成设置")]
    IncompleteOnboardingState,
    #[error("游戏元数据不可用：{0}")]
    Metadata(String),
    #[error("安装请求无效：{0}")]
    InvalidInstallRequest(String),
    #[error("安装状态数据无效：{0}")]
    InvalidStoredState(String),
    #[error("无法获取在线元数据：{0}")]
    Network(#[from] reqwest::Error),
}

pub type Result<T> = std::result::Result<T, CoreError>;

#[derive(Debug, Clone)]
pub struct AppService {
    database_path: PathBuf,
    default_data_directory: String,
}

impl AppService {
    pub fn open(database_path: &Path, default_data_directory: &Path) -> Result<Self> {
        if let Some(parent) = database_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let service = Self {
            database_path: database_path.to_path_buf(),
            default_data_directory: path_text(default_data_directory),
        };
        service.migrate()?;
        service.recover_interrupted_install_tasks()?;
        Ok(service)
    }

    pub fn bootstrap_state(&self) -> Result<BootstrapState> {
        let connection = self.connection()?;
        let is_complete = read_setting(&connection, SETTING_ONBOARDING_COMPLETE)?
            .is_some_and(|value| value == "true");
        let settings = if is_complete {
            let serialized = read_setting(&connection, SETTING_ONBOARDING_SELECTION)?
                .ok_or(CoreError::IncompleteOnboardingState)?;
            Some(serde_json::from_str(&serialized)?)
        } else {
            None
        };
        let defaults = OnboardingSelection::recommended(self.default_data_directory.clone());

        Ok(BootstrapState {
            requires_onboarding: !is_complete,
            default_data_directory: self.default_data_directory.clone(),
            defaults,
            settings,
        })
    }

    pub fn complete_onboarding(&self, selection: &OnboardingSelection) -> Result<()> {
        validate_data_directory(&selection.data_directory)?;

        let serialized = serde_json::to_string(selection)?;
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        write_setting(&transaction, SETTING_ONBOARDING_SELECTION, &serialized)?;
        write_setting(&transaction, SETTING_ONBOARDING_COMPLETE, "true")?;
        transaction.commit()?;
        Ok(())
    }

    pub fn skip_onboarding(&self) -> Result<()> {
        self.complete_onboarding(&OnboardingSelection::recommended(
            self.default_data_directory.clone(),
        ))
    }

    fn migrate(&self) -> Result<()> {
        let connection = self.connection()?;
        connection.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS app_settings (
                key TEXT PRIMARY KEY NOT NULL,
                value TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS metadata_cache (
                cache_key TEXT PRIMARY KEY NOT NULL,
                payload_json TEXT NOT NULL,
                updated_at_unix_seconds INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS managed_java_environments (
                id TEXT PRIMARY KEY NOT NULL,
                distribution TEXT NOT NULL,
                full_version TEXT NOT NULL,
                architecture TEXT NOT NULL,
                home_directory TEXT NOT NULL,
                status TEXT NOT NULL,
                UNIQUE(distribution, full_version, architecture)
            );
            CREATE TABLE IF NOT EXISTS install_tasks (
                id TEXT PRIMARY KEY NOT NULL,
                state TEXT NOT NULL,
                current_stage TEXT,
                plan_json TEXT NOT NULL,
                staging_directory TEXT NOT NULL,
                target_directory TEXT NOT NULL,
                created_at_unix_seconds INTEGER NOT NULL,
                updated_at_unix_seconds INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_install_tasks_state_created
                ON install_tasks(state, created_at_unix_seconds);
            CREATE TABLE IF NOT EXISTS install_task_java (
                task_id TEXT PRIMARY KEY NOT NULL REFERENCES install_tasks(id) ON DELETE CASCADE,
                environment_id TEXT NOT NULL REFERENCES managed_java_environments(id),
                action TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS instances (
                id TEXT PRIMARY KEY NOT NULL,
                name TEXT NOT NULL,
                game_version TEXT NOT NULL,
                loader_kind TEXT NOT NULL,
                loader_version TEXT,
                root_directory TEXT NOT NULL UNIQUE,
                state TEXT NOT NULL,
                created_at_unix_seconds INTEGER NOT NULL
            );
            PRAGMA user_version = 2;
            ",
        )?;
        Ok(())
    }

    pub(crate) fn connection(&self) -> Result<Connection> {
        let connection = Connection::open(&self.database_path)?;
        connection.busy_timeout(Duration::from_secs(5))?;
        connection.pragma_update(None, "foreign_keys", true)?;
        Ok(connection)
    }
}

pub(crate) fn unix_timestamp() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};

    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| {
            i64::try_from(duration.as_secs()).unwrap_or(i64::MAX)
        })
}

fn validate_data_directory(value: &str) -> Result<()> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(CoreError::InvalidDataDirectory(
            "请选择一个本地文件夹".to_owned(),
        ));
    }
    if trimmed.starts_with(r"\\") || trimmed.starts_with("//") {
        return Err(CoreError::InvalidDataDirectory(
            "不支持 SMB、NAS 或 UNC 网络路径，无法保证数据库锁定与原子迁移".to_owned(),
        ));
    }

    let path = Path::new(trimmed);
    if !path.is_absolute() {
        return Err(CoreError::InvalidDataDirectory(
            "数据位置必须是本地绝对路径".to_owned(),
        ));
    }
    if path.exists() && !path.is_dir() {
        return Err(CoreError::InvalidDataDirectory(
            "所选位置已经被同名文件占用".to_owned(),
        ));
    }

    fs::create_dir_all(path)?;
    Ok(())
}

fn read_setting(connection: &Connection, key: &str) -> Result<Option<String>> {
    connection
        .query_row(
            "SELECT value FROM app_settings WHERE key = ?1",
            params![key],
            |row| row.get(0),
        )
        .optional()
        .map_err(CoreError::from)
}

fn write_setting(connection: &Connection, key: &str, value: &str) -> Result<()> {
    connection.execute(
        "
        INSERT INTO app_settings (key, value) VALUES (?1, ?2)
        ON CONFLICT(key) DO UPDATE SET value = excluded.value
        ",
        params![key, value],
    )?;
    Ok(())
}

fn path_text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}
