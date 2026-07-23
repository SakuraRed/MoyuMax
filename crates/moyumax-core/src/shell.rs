//! 托盘生命周期相关的本地设置、壳层状态与退出影响查询。
//!
//! 本模块只维护 SQLite `app_settings` 中的键值与只读汇总查询，
//! 不包含任何 Tauri 或界面概念，供桌面壳、托盘和未来 CLI 共用。

use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::{AppService, CoreError, Result, read_setting, write_setting};

const SETTING_WINDOW_CLOSE_BEHAVIOR: &str = "window_close_behavior";
const SETTING_SHELL_STATE: &str = "shell_state";
const SETTING_TASKS_PAUSED: &str = "tasks_paused";
const SETTING_DOWNLOAD_SPEED_LIMIT: &str = "download_speed_limit_bytes";
const SETTING_UI_THEME: &str = "ui_theme";
const SETTING_UI_LANGUAGE: &str = "ui_language";

/// 关闭主窗口时的行为。默认每次询问。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WindowCloseBehavior {
    /// 每次关闭都询问最小化到托盘还是退出。
    #[default]
    Ask,
    /// 关闭窗口直接最小化到系统托盘。
    MinimizeToTray,
    /// 关闭窗口即退出，仍需通过退出影响检查。
    Exit,
}

impl WindowCloseBehavior {
    fn database_value(self) -> &'static str {
        match self {
            Self::Ask => "ask",
            Self::MinimizeToTray => "minimize_to_tray",
            Self::Exit => "exit",
        }
    }

    fn from_database(value: &str) -> Result<Self> {
        match value {
            "ask" => Ok(Self::Ask),
            "minimize_to_tray" => Ok(Self::MinimizeToTray),
            "exit" => Ok(Self::Exit),
            other => Err(CoreError::InvalidStoredState(format!(
                "未知的关闭窗口行为：{other}"
            ))),
        }
    }
}

/// 从托盘唤醒时需要恢复的壳层界面状态。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShellState {
    pub page: String,
    pub scroll_top: u32,
}

/// 退出时仍在运行的游戏会话。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExitImpactSession {
    pub session_id: String,
    pub instance_id: String,
    pub instance_name: String,
}

/// 退出影响汇总：运行中游戏、活动任务与已暂停任务。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExitImpactSummary {
    pub running_sessions: Vec<ExitImpactSession>,
    pub active_install_tasks: u64,
    pub active_content_tasks: u64,
    pub executing_install_tasks: u64,
    pub executing_content_tasks: u64,
    pub paused_tasks: u64,
}

impl ExitImpactSummary {
    /// 存在运行中游戏或活动任务时，退出前必须向用户说明影响。
    /// 已暂停任务不影响退出，只作为信息展示。
    #[must_use]
    pub fn requires_confirmation(&self) -> bool {
        !self.running_sessions.is_empty()
            || self.active_install_tasks > 0
            || self.active_content_tasks > 0
    }

    /// 正在执行（running/committing）的任务数量。优雅退出只需等待执行中任务收敛；
    /// 排队任务没有进行中的传输，下次启动会自动继续。
    #[must_use]
    pub fn executing_tasks(&self) -> u64 {
        self.executing_install_tasks + self.executing_content_tasks
    }
}

/// 托盘菜单使用的最近实例条目。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentInstanceSummary {
    pub id: String,
    pub name: String,
    pub game_version: String,
    pub loader_kind: String,
    pub last_started_at_unix_seconds: Option<i64>,
    pub is_running: bool,
}

impl AppService {
    pub fn window_close_behavior(&self) -> Result<WindowCloseBehavior> {
        let connection = self.connection()?;
        read_setting(&connection, SETTING_WINDOW_CLOSE_BEHAVIOR)?
            .map(|value| WindowCloseBehavior::from_database(&value))
            .transpose()
            .map(|behavior| behavior.unwrap_or_default())
    }

    pub fn set_window_close_behavior(&self, behavior: WindowCloseBehavior) -> Result<()> {
        let connection = self.connection()?;
        write_setting(
            &connection,
            SETTING_WINDOW_CLOSE_BEHAVIOR,
            behavior.database_value(),
        )?;
        Ok(())
    }

    /// 读取上次持久化的壳层状态。数据损坏时返回错误而不是静默丢弃，
    /// 调用方可以在提示后覆盖写入新状态。
    pub fn shell_state(&self) -> Result<Option<ShellState>> {
        let connection = self.connection()?;
        read_setting(&connection, SETTING_SHELL_STATE)?
            .map(|serialized| serde_json::from_str(&serialized).map_err(CoreError::from))
            .transpose()
    }

    pub fn persist_shell_state(&self, state: &ShellState) -> Result<()> {
        let page = state.page.trim();
        if page.is_empty() || page.len() > 64 || page.chars().any(char::is_control) {
            return Err(CoreError::InvalidStoredState("壳层页面标识无效".to_owned()));
        }
        let serialized = serde_json::to_string(state)?;
        let connection = self.connection()?;
        write_setting(&connection, SETTING_SHELL_STATE, &serialized)?;
        Ok(())
    }

    /// 全局下载限速（字节/秒,0 为不限速）。持久化并在启动时应用到全局限速器。
    pub fn download_speed_limit(&self) -> Result<u64> {
        let connection = self.connection()?;
        let value = read_setting(&connection, SETTING_DOWNLOAD_SPEED_LIMIT)?;
        match value {
            None => Ok(0),
            Some(text) => text
                .parse::<u64>()
                .map_err(|_| CoreError::InvalidStoredState("下载限速设置已损坏".to_owned())),
        }
    }

    pub fn set_download_speed_limit(&self, bytes_per_sec: u64) -> Result<()> {
        let connection = self.connection()?;
        write_setting(
            &connection,
            SETTING_DOWNLOAD_SPEED_LIMIT,
            &bytes_per_sec.to_string(),
        )?;
        crate::global_rate_limiter().set_rate(bytes_per_sec);
        Ok(())
    }

    /// 全局任务暂停标志。持久化保证重启后仍然保持暂停。
    pub fn tasks_paused(&self) -> Result<bool> {
        let connection = self.connection()?;
        Ok(read_setting(&connection, SETTING_TASKS_PAUSED)?.is_some_and(|value| value == "true"))
    }

    pub fn set_tasks_paused(&self, paused: bool) -> Result<()> {
        let connection = self.connection()?;
        write_setting(
            &connection,
            SETTING_TASKS_PAUSED,
            if paused { "true" } else { "false" },
        )?;
        Ok(())
    }

    /// 界面主题：system（跟随系统）、light、dark。
    pub fn ui_theme(&self) -> Result<String> {
        let connection = self.connection()?;
        Ok(read_setting(&connection, SETTING_UI_THEME)?
            .filter(|value| matches!(value.as_str(), "system" | "light" | "dark"))
            .unwrap_or_else(|| "system".to_owned()))
    }

    pub fn set_ui_theme(&self, theme: &str) -> Result<()> {
        if !matches!(theme, "system" | "light" | "dark") {
            return Err(CoreError::Content(
                "主题必须是 system、light 或 dark".to_owned(),
            ));
        }
        let connection = self.connection()?;
        write_setting(&connection, SETTING_UI_THEME, theme)?;
        Ok(())
    }

    /// 界面语言：zh-CN、zh-TW、en。
    pub fn ui_language(&self) -> Result<String> {
        let connection = self.connection()?;
        Ok(read_setting(&connection, SETTING_UI_LANGUAGE)?
            .filter(|value| matches!(value.as_str(), "zh-CN" | "zh-TW" | "en"))
            .unwrap_or_else(|| "zh-CN".to_owned()))
    }

    pub fn set_ui_language(&self, language: &str) -> Result<()> {
        if !matches!(language, "zh-CN" | "zh-TW" | "en") {
            return Err(CoreError::Content(
                "语言必须是 zh-CN、zh-TW 或 en".to_owned(),
            ));
        }
        let connection = self.connection()?;
        write_setting(&connection, SETTING_UI_LANGUAGE, language)?;
        Ok(())
    }

    /// 退出影响汇总：运行中的游戏会话、活动任务数量和已暂停任务数量。
    pub fn exit_impact_summary(&self) -> Result<ExitImpactSummary> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "
            SELECT session.id, session.instance_id, instance.name
            FROM launch_sessions session
            JOIN instances instance ON instance.id = session.instance_id
            WHERE session.state IN ('starting', 'running')
            ORDER BY session.started_at_unix_seconds, session.id
            ",
        )?;
        let running_sessions = statement
            .query_map([], |row| {
                Ok(ExitImpactSession {
                    session_id: row.get(0)?,
                    instance_id: row.get(1)?,
                    instance_name: row.get(2)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let active_install_tasks = count_tasks(&connection, "install_tasks", ACTIVE_TASK_STATES)?;
        let active_content_tasks =
            count_tasks(&connection, "content_install_tasks", ACTIVE_TASK_STATES)?;
        let executing_install_tasks =
            count_tasks(&connection, "install_tasks", EXECUTING_TASK_STATES)?;
        let executing_content_tasks =
            count_tasks(&connection, "content_install_tasks", EXECUTING_TASK_STATES)?;
        let paused_tasks = count_tasks(&connection, "install_tasks", "('paused')")?
            + count_tasks(&connection, "content_install_tasks", "('paused')")?;
        Ok(ExitImpactSummary {
            running_sessions,
            active_install_tasks,
            active_content_tasks,
            executing_install_tasks,
            executing_content_tasks,
            paused_tasks,
        })
    }

    /// 托盘菜单的最近实例列表：按最近启动排序，从未启动的按创建时间排在后面。
    /// 回收站中的实例不出现。
    pub fn recent_instances(&self, limit: u32) -> Result<Vec<RecentInstanceSummary>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "
            SELECT instance.id, instance.name, instance.game_version, instance.loader_kind,
                   (SELECT MAX(session.started_at_unix_seconds)
                    FROM launch_sessions session
                    WHERE session.instance_id = instance.id) AS last_started,
                   EXISTS(
                       SELECT 1 FROM launch_sessions running
                       WHERE running.instance_id = instance.id
                         AND running.state IN ('starting', 'running')
                   ) AS is_running
            FROM instances instance
            WHERE NOT EXISTS (
                SELECT 1
                FROM recycle_bin_items bin
                WHERE bin.subject_id = instance.id
                  AND bin.item_kind = 'instance'
            )
            ORDER BY (last_started IS NULL), last_started DESC,
                     instance.created_at_unix_seconds DESC, instance.id
            LIMIT ?1
            ",
        )?;
        let rows = statement.query_map(params![i64::from(limit)], |row| {
            Ok(RecentInstanceSummary {
                id: row.get(0)?,
                name: row.get(1)?,
                game_version: row.get(2)?,
                loader_kind: row.get(3)?,
                last_started_at_unix_seconds: row.get(4)?,
                is_running: row.get(5)?,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(CoreError::from)
    }
}

const ACTIVE_TASK_STATES: &str = "('queued', 'running', 'committing', 'awaiting_recovery')";
const EXECUTING_TASK_STATES: &str = "('running', 'committing')";

fn count_tasks(connection: &rusqlite::Connection, table: &str, states: &str) -> Result<u64> {
    let query = format!("SELECT COUNT(*) FROM {table} WHERE state IN {states}");
    let count: i64 = connection.query_row(&query, [], |row| row.get(0))?;
    u64::try_from(count).map_err(|_| CoreError::InvalidStoredState("任务计数溢出".to_owned()))
}
