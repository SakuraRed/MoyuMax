use std::{
    fs,
    io::Write,
    path::{Component, Path, PathBuf},
    time::UNIX_EPOCH,
};

use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;
use zip::{CompressionMethod, ZipArchive, ZipWriter, write::SimpleFileOptions};

use crate::{AppService, CoreError, Result, read_setting, unix_timestamp, write_setting};

const BACKUP_SCHEMA_VERSION: u16 = 2;
const DEFAULT_BACKUP_INTERVAL_MINUTES: u64 = 30;
const DEFAULT_BACKUP_KEEP_COUNT: usize = 20;
const SETTING_BACKUP_INTERVAL: &str = "world_backup_interval_minutes";
const SETTING_BACKUP_KEEP: &str = "world_backup_keep_count";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BackupTrigger {
    PreLaunch,
    PostExit,
    Manual,
    Scheduled,
}

impl BackupTrigger {
    const fn database_value(self) -> &'static str {
        match self {
            Self::PreLaunch => "pre_launch",
            Self::PostExit => "post_exit",
            Self::Manual => "manual",
            Self::Scheduled => "scheduled",
        }
    }

    fn from_database(value: &str) -> Result<Self> {
        match value {
            "pre_launch" => Ok(Self::PreLaunch),
            "post_exit" => Ok(Self::PostExit),
            "manual" => Ok(Self::Manual),
            "scheduled" => Ok(Self::Scheduled),
            _ => Err(CoreError::InvalidStoredState(format!(
                "未知世界备份触发原因：{value}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum BackupKind {
    #[default]
    Full,
    Incremental,
}

impl BackupKind {
    const fn database_value(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Incremental => "incremental",
        }
    }

    fn from_database(value: &str) -> Result<Self> {
        match value {
            "full" => Ok(Self::Full),
            "incremental" => Ok(Self::Incremental),
            _ => Err(CoreError::InvalidStoredState(format!(
                "未知世界备份类型：{value}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BackupState {
    Staging,
    Ready,
    Skipped,
    Failed,
}

impl BackupState {
    const fn database_value(self) -> &'static str {
        match self {
            Self::Staging => "staging",
            Self::Ready => "ready",
            Self::Skipped => "skipped",
            Self::Failed => "failed",
        }
    }

    fn from_database(value: &str) -> Result<Self> {
        match value {
            "staging" => Ok(Self::Staging),
            "ready" => Ok(Self::Ready),
            "skipped" => Ok(Self::Skipped),
            "failed" => Ok(Self::Failed),
            _ => Err(CoreError::InvalidStoredState(format!(
                "未知世界备份状态：{value}"
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorldBackupSummary {
    pub id: String,
    pub instance_id: String,
    pub instance_name: String,
    pub launch_session_id: Option<String>,
    pub trigger: BackupTrigger,
    pub state: BackupState,
    pub archive_path: Option<String>,
    pub world_count: u64,
    pub source_bytes: u64,
    pub archive_bytes: u64,
    pub created_at_unix_seconds: i64,
    pub completed_at_unix_seconds: Option<i64>,
    pub error_summary: Option<String>,
    #[serde(default)]
    pub kind: BackupKind,
    #[serde(default)]
    pub base_backup_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupManifestFile {
    pub path: String,
    pub size: u64,
    pub mtime: i64,
}

/// 备份归档 manifest.json 的可读子集（v1 清单缺失字段按默认处理）。
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct BackupManifest {
    pub kind: BackupKind,
    pub files: Vec<BackupManifestFile>,
    pub deleted: Vec<String>,
}

#[derive(Debug)]
struct ArchiveEntry {
    source: PathBuf,
    name: String,
    directory: bool,
    size: u64,
    mtime: i64,
}

#[derive(Debug)]
struct BackupPreparation {
    worlds: Vec<String>,
    entries: Vec<ArchiveEntry>,
    source_bytes: u64,
}

impl AppService {
    pub fn list_world_backups(&self, instance_id: Option<&str>) -> Result<Vec<WorldBackupSummary>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "
            SELECT id, instance_id, instance_name, launch_session_id, trigger, state,
                   archive_path, world_count, source_bytes, archive_bytes,
                   created_at_unix_seconds, completed_at_unix_seconds, error_summary,
                   kind, base_backup_id
            FROM world_backups
            WHERE ?1 IS NULL OR instance_id = ?1
            ORDER BY created_at_unix_seconds DESC, id DESC
            ",
        )?;
        let rows = statement.query_map(params![instance_id], read_backup_row)?;
        rows.map(|row| row.and_then(decode_backup))
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(CoreError::from)
    }

    pub fn create_world_backup(
        &self,
        instance_id: &str,
        trigger: BackupTrigger,
        launch_session_id: Option<&str>,
    ) -> Result<WorldBackupSummary> {
        if let Some(session_id) = launch_session_id
            && let Some(existing) = self.backup_for_session(session_id, trigger)?
        {
            return Ok(existing);
        }

        let (instance_name, instance_root) = self.backup_instance(instance_id)?;
        let backup_id = Uuid::new_v4().to_string();
        let created_at = unix_timestamp();
        let mut backup = WorldBackupSummary {
            id: backup_id,
            instance_id: instance_id.to_owned(),
            instance_name,
            launch_session_id: launch_session_id.map(str::to_owned),
            trigger,
            state: BackupState::Staging,
            archive_path: None,
            world_count: 0,
            source_bytes: 0,
            archive_bytes: 0,
            created_at_unix_seconds: created_at,
            completed_at_unix_seconds: None,
            error_summary: None,
            kind: BackupKind::Full,
            base_backup_id: None,
        };

        let preparation =
            match prepare_world_backup(&self.selected_data_directory()?, &instance_root) {
                Ok(preparation) => preparation,
                Err(error) => {
                    backup.state = BackupState::Failed;
                    backup.completed_at_unix_seconds = Some(unix_timestamp());
                    backup.error_summary = Some(error.to_string());
                    self.insert_world_backup(&backup)?;
                    return Err(CoreError::Backup(error.to_string()));
                }
            };
        backup.world_count = u64::try_from(preparation.worlds.len())
            .map_err(|_| CoreError::Backup("世界数量超过可表示范围".to_owned()))?;
        backup.source_bytes = preparation.source_bytes;

        if preparation.worlds.is_empty() {
            backup.state = BackupState::Skipped;
            backup.completed_at_unix_seconds = Some(unix_timestamp());
            self.insert_world_backup(&backup)?;
            return Ok(backup);
        }

        let backup_root = self
            .selected_data_directory()?
            .join("backups")
            .join("instances")
            .join(instance_id);
        fs::create_dir_all(&backup_root)?;
        let archive_path = backup_root.join(format!("{}.zip", backup.id));
        validate_archive_path(&self.selected_data_directory()?, instance_id, &archive_path)?;
        let partial_path = partial_path(&archive_path);
        if archive_path.exists() || partial_path.exists() {
            return Err(CoreError::Backup("备份目标已被占用，请重试".to_owned()));
        }
        backup.archive_path = Some(path_text(&archive_path)?);
        self.insert_world_backup(&backup)?;

        let write_result = (|| -> Result<u64> {
            let included = preparation.entries.iter().collect::<Vec<_>>();
            let file =
                write_world_backup_zip(&partial_path, &backup, &preparation, &included, &[])?;
            file.sync_all()?;
            fs::rename(&partial_path, &archive_path)?;
            Ok(fs::metadata(&archive_path)?.len())
        })();
        let archive_bytes = match write_result {
            Ok(bytes) => bytes,
            Err(error) => {
                let _ = remove_file_if_exists(&partial_path);
                let _ = remove_file_if_exists(&archive_path);
                self.mark_world_backup_failed(&backup.id, &error.to_string())?;
                return Err(CoreError::Backup(error.to_string()));
            }
        };
        backup.state = BackupState::Ready;
        backup.archive_bytes = archive_bytes;
        backup.completed_at_unix_seconds = Some(unix_timestamp());
        self.finalize_world_backup(&backup)?;
        self.prune_world_backups(instance_id)?;
        Ok(backup)
    }

    pub(crate) fn recover_interrupted_world_backups(&self) -> Result<()> {
        let staging = self
            .list_world_backups(None)?
            .into_iter()
            .filter(|backup| backup.state == BackupState::Staging)
            .collect::<Vec<_>>();
        for mut backup in staging {
            let Some(archive_text) = &backup.archive_path else {
                self.mark_world_backup_failed(&backup.id, "启动器中断了尚未写入的备份")?;
                continue;
            };
            let archive = PathBuf::from(archive_text);
            if validate_archive_path(
                &self.selected_data_directory()?,
                &backup.instance_id,
                &archive,
            )
            .is_err()
            {
                self.mark_world_backup_failed(&backup.id, "备份路径不在受管目录内")?;
                continue;
            }
            let partial = partial_path(&archive);
            if archive.is_file() {
                let _ = remove_file_if_exists(&partial);
                backup.state = BackupState::Ready;
                backup.archive_bytes = fs::metadata(&archive)?.len();
                backup.completed_at_unix_seconds = Some(unix_timestamp());
                self.finalize_world_backup(&backup)?;
            } else {
                let _ = remove_file_if_exists(&partial);
                self.mark_world_backup_failed(&backup.id, "启动器中断了备份写入，临时文件已清理")?;
            }
        }
        Ok(())
    }

    pub(crate) fn backup_for_session(
        &self,
        session_id: &str,
        trigger: BackupTrigger,
    ) -> Result<Option<WorldBackupSummary>> {
        let connection = self.connection()?;
        connection
            .query_row(
                "
                SELECT id, instance_id, instance_name, launch_session_id, trigger, state,
                       archive_path, world_count, source_bytes, archive_bytes,
                       created_at_unix_seconds, completed_at_unix_seconds, error_summary,
                       kind, base_backup_id
                FROM world_backups
                WHERE launch_session_id = ?1 AND trigger = ?2
                ",
                params![session_id, trigger.database_value()],
                read_backup_row,
            )
            .optional()?
            .map(decode_backup)
            .transpose()
            .map_err(CoreError::from)
    }

    fn backup_instance(&self, instance_id: &str) -> Result<(String, PathBuf)> {
        let connection = self.connection()?;
        connection
            .query_row(
                "
                SELECT name, root_directory
                FROM instances
                WHERE id = ?1
                  AND NOT EXISTS (
                      SELECT 1 FROM recycle_bin_items
                      WHERE subject_id = instances.id AND item_kind = 'instance'
                  )
                ",
                params![instance_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        PathBuf::from(row.get::<_, String>(1)?),
                    ))
                },
            )
            .optional()?
            .ok_or_else(|| CoreError::Backup("实例不存在或当前位于回收站".to_owned()))
    }

    fn insert_world_backup(&self, backup: &WorldBackupSummary) -> Result<()> {
        self.connection()?.execute(
            "
            INSERT INTO world_backups (
                id, instance_id, instance_name, launch_session_id, trigger, state,
                archive_path, world_count, source_bytes, archive_bytes,
                created_at_unix_seconds, completed_at_unix_seconds, error_summary,
                kind, base_backup_id
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
            ",
            params![
                backup.id,
                backup.instance_id,
                backup.instance_name,
                backup.launch_session_id,
                backup.trigger.database_value(),
                backup.state.database_value(),
                backup.archive_path,
                sqlite_integer(backup.world_count, "世界数量")?,
                sqlite_integer(backup.source_bytes, "存档源大小")?,
                sqlite_integer(backup.archive_bytes, "备份归档大小")?,
                backup.created_at_unix_seconds,
                backup.completed_at_unix_seconds,
                backup.error_summary,
                backup.kind.database_value(),
                backup.base_backup_id,
            ],
        )?;
        Ok(())
    }

    fn finalize_world_backup(&self, backup: &WorldBackupSummary) -> Result<()> {
        let changed = self.connection()?.execute(
            "
            UPDATE world_backups
            SET state = 'ready', archive_bytes = ?2,
                completed_at_unix_seconds = ?3, error_summary = NULL
            WHERE id = ?1 AND state = 'staging'
            ",
            params![
                backup.id,
                sqlite_integer(backup.archive_bytes, "备份归档大小")?,
                backup.completed_at_unix_seconds,
            ],
        )?;
        if changed != 1 {
            return Err(CoreError::Backup(
                "备份归档已写入，但状态无法原子提交；重启后将自动核对".to_owned(),
            ));
        }
        Ok(())
    }

    fn mark_world_backup_failed(&self, backup_id: &str, error_summary: &str) -> Result<()> {
        self.connection()?.execute(
            "
            UPDATE world_backups
            SET state = 'failed', completed_at_unix_seconds = ?2, error_summary = ?3
            WHERE id = ?1
            ",
            params![backup_id, unix_timestamp(), error_summary],
        )?;
        Ok(())
    }

    fn prune_world_backups(&self, instance_id: &str) -> Result<()> {
        let keep = usize::try_from(self.world_backup_keep_count()?)
            .map_err(|_| CoreError::Backup("备份保留数量超出可表示范围".to_owned()))?;
        // 按创建时间 + 插入序升序；从最旧开始，只清理没有增量子级的备份，恢复链不得悬空。
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "
            SELECT id, instance_id, instance_name, launch_session_id, trigger, state,
                   archive_path, world_count, source_bytes, archive_bytes,
                   created_at_unix_seconds, completed_at_unix_seconds, error_summary,
                   kind, base_backup_id
            FROM world_backups
            WHERE instance_id = ?1 AND state = 'ready'
            ORDER BY created_at_unix_seconds, rowid
            ",
        )?;
        let mut backups = statement
            .query_map(params![instance_id], read_backup_row)?
            .collect::<std::result::Result<Vec<_>, _>>()?
            .into_iter()
            .map(decode_backup)
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(CoreError::from)?;
        drop(statement);
        drop(connection);
        let data_directory = self.selected_data_directory()?;
        let mut blocked = std::collections::HashSet::new();
        while backups.len() > keep {
            let Some(position) = backups.iter().position(|candidate| {
                !blocked.contains(&candidate.id)
                    && !backups
                        .iter()
                        .any(|other| other.base_backup_id.as_deref() == Some(candidate.id.as_str()))
            }) else {
                break;
            };
            let backup = backups.remove(position);
            let removable = backup
                .archive_path
                .as_deref()
                .map(PathBuf::from)
                .is_none_or(|path| {
                    validate_archive_path(&data_directory, instance_id, &path).is_ok()
                        && remove_file_if_exists(&path).is_ok()
                });
            if !removable {
                blocked.insert(backup.id.clone());
                backups.insert(position, backup);
                continue;
            }
            self.connection()?.execute(
                "DELETE FROM world_backups WHERE id = ?1",
                params![backup.id],
            )?;
        }
        Ok(())
    }

    /// 定时（运行期间）备份：有可用基准时创建只含差异的增量备份，否则回退为全量。
    /// 定时备份不占用会话触发唯一槽，launch_session_id 存 NULL。
    pub fn create_scheduled_world_backup(&self, instance_id: &str) -> Result<WorldBackupSummary> {
        let (instance_name, instance_root) = self.backup_instance(instance_id)?;
        let preparation = prepare_world_backup(&self.selected_data_directory()?, &instance_root)?;
        if preparation.worlds.is_empty() {
            return Err(CoreError::Backup("实例没有世界，跳过定时备份".to_owned()));
        }
        let base = self.latest_ready_backup(instance_id)?;
        let base_manifest = base
            .as_ref()
            .and_then(|backup| backup.archive_path.as_deref().map(PathBuf::from))
            .and_then(|path| read_backup_manifest(&path).ok())
            .filter(|manifest| !manifest.files.is_empty());
        let Some((base_backup, manifest)) = base.zip(base_manifest) else {
            // 没有可用基准索引：回退为全量定时备份。
            return self.create_world_backup(instance_id, BackupTrigger::Scheduled, None);
        };

        let base_index = manifest
            .files
            .iter()
            .map(|file| (file.path.clone(), (file.size, file.mtime)))
            .collect::<std::collections::HashMap<_, _>>();
        let mut changed = Vec::new();
        let mut current_paths = std::collections::HashSet::new();
        for entry in &preparation.entries {
            if entry.directory {
                continue;
            }
            let path = entry.name.trim_start_matches("worlds/").to_owned();
            current_paths.insert(path.clone());
            let changed_file = base_index
                .get(&path)
                .is_none_or(|(size, mtime)| *size != entry.size || *mtime != entry.mtime);
            if changed_file {
                changed.push(entry);
            }
        }
        let mut deleted = manifest
            .files
            .iter()
            .map(|file| file.path.clone())
            .filter(|path| !current_paths.contains(path))
            .collect::<Vec<_>>();
        deleted.sort();
        if changed.is_empty() && deleted.is_empty() {
            // saves 自基准以来没有变化，不产生新备份。
            return Ok(base_backup);
        }

        let created_at = unix_timestamp();
        let mut backup = WorldBackupSummary {
            id: Uuid::new_v4().to_string(),
            instance_id: instance_id.to_owned(),
            instance_name,
            launch_session_id: None,
            trigger: BackupTrigger::Scheduled,
            state: BackupState::Staging,
            archive_path: None,
            world_count: u64::try_from(preparation.worlds.len())
                .map_err(|_| CoreError::Backup("世界数量超过可表示范围".to_owned()))?,
            source_bytes: preparation.source_bytes,
            archive_bytes: 0,
            created_at_unix_seconds: created_at,
            completed_at_unix_seconds: None,
            error_summary: None,
            kind: BackupKind::Incremental,
            base_backup_id: Some(base_backup.id.clone()),
        };
        let backup_root = self
            .selected_data_directory()?
            .join("backups")
            .join("instances")
            .join(instance_id);
        fs::create_dir_all(&backup_root)?;
        let archive_path = backup_root.join(format!("{}.zip", backup.id));
        validate_archive_path(&self.selected_data_directory()?, instance_id, &archive_path)?;
        let partial_path = partial_path(&archive_path);
        if archive_path.exists() || partial_path.exists() {
            return Err(CoreError::Backup("备份目标已被占用，请重试".to_owned()));
        }
        backup.archive_path = Some(path_text(&archive_path)?);
        self.insert_world_backup(&backup)?;

        let write_result = (|| -> Result<u64> {
            let file =
                write_world_backup_zip(&partial_path, &backup, &preparation, &changed, &deleted)?;
            file.sync_all()?;
            fs::rename(&partial_path, &archive_path)?;
            Ok(fs::metadata(&archive_path)?.len())
        })();
        let archive_bytes = match write_result {
            Ok(bytes) => bytes,
            Err(error) => {
                let _ = remove_file_if_exists(&partial_path);
                let _ = remove_file_if_exists(&archive_path);
                self.mark_world_backup_failed(&backup.id, &error.to_string())?;
                return Err(CoreError::Backup(error.to_string()));
            }
        };
        backup.state = BackupState::Ready;
        backup.archive_bytes = archive_bytes;
        backup.completed_at_unix_seconds = Some(unix_timestamp());
        self.finalize_world_backup(&backup)?;
        self.prune_world_backups(instance_id)?;
        Ok(backup)
    }

    fn latest_ready_backup(&self, instance_id: &str) -> Result<Option<WorldBackupSummary>> {
        let connection = self.connection()?;
        connection
            .query_row(
                "
                SELECT id, instance_id, instance_name, launch_session_id, trigger, state,
                       archive_path, world_count, source_bytes, archive_bytes,
                       created_at_unix_seconds, completed_at_unix_seconds, error_summary,
                       kind, base_backup_id
                FROM world_backups
                WHERE instance_id = ?1 AND state = 'ready' AND archive_path IS NOT NULL
                ORDER BY created_at_unix_seconds DESC, rowid DESC
                LIMIT 1
                ",
                params![instance_id],
                read_backup_row,
            )
            .optional()?
            .map(decode_backup)
            .transpose()
            .map_err(CoreError::from)
    }

    /// 定时备份间隔（分钟）；0 表示关闭运行期间备份。
    pub fn world_backup_interval_minutes(&self) -> Result<u64> {
        let connection = self.connection()?;
        let value = read_setting(&connection, SETTING_BACKUP_INTERVAL)?;
        Ok(value
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(DEFAULT_BACKUP_INTERVAL_MINUTES))
    }

    pub fn set_world_backup_interval_minutes(&self, minutes: u64) -> Result<()> {
        if minutes > 24 * 60 {
            return Err(CoreError::Backup("备份间隔不能超过 1440 分钟".to_owned()));
        }
        let connection = self.connection()?;
        write_setting(&connection, SETTING_BACKUP_INTERVAL, &minutes.to_string())
    }

    /// 每个实例保留的成功备份数量（1–100）。
    pub fn world_backup_keep_count(&self) -> Result<u64> {
        let connection = self.connection()?;
        let value = read_setting(&connection, SETTING_BACKUP_KEEP)?;
        Ok(value
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(DEFAULT_BACKUP_KEEP_COUNT as u64))
    }

    pub fn set_world_backup_keep_count(&self, count: u64) -> Result<()> {
        if count == 0 || count > 100 {
            return Err(CoreError::Backup(
                "备份保留数量必须在 1 到 100 之间".to_owned(),
            ));
        }
        let connection = self.connection()?;
        write_setting(&connection, SETTING_BACKUP_KEEP, &count.to_string())
    }
}

/// 游戏会话运行期间的定时备份循环：按间隔创建增量备份，会话结束或间隔为 0 即停止。
pub fn spawn_scheduled_world_backups(service: AppService, session_id: String) {
    tokio::spawn(async move {
        loop {
            let interval = match service.world_backup_interval_minutes() {
                Ok(interval) => interval,
                Err(_) => break,
            };
            if interval == 0 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_secs(interval.saturating_mul(60))).await;
            let session = service
                .list_launch_sessions()
                .ok()
                .and_then(|sessions| sessions.into_iter().find(|item| item.id == session_id));
            let Some(session) = session else { break };
            if session.state != crate::LaunchSessionState::Running {
                break;
            }
            let _ = service.create_scheduled_world_backup(&session.instance_id);
        }
    });
}

type StoredBackup = (
    String,
    String,
    String,
    Option<String>,
    String,
    String,
    Option<String>,
    i64,
    i64,
    i64,
    i64,
    Option<i64>,
    Option<String>,
    String,
    Option<String>,
);

fn read_backup_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredBackup> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
        row.get(6)?,
        row.get(7)?,
        row.get(8)?,
        row.get(9)?,
        row.get(10)?,
        row.get(11)?,
        row.get(12)?,
        row.get(13)?,
        row.get(14)?,
    ))
}

fn decode_backup(stored: StoredBackup) -> rusqlite::Result<WorldBackupSummary> {
    let (
        id,
        instance_id,
        instance_name,
        launch_session_id,
        trigger,
        state,
        archive_path,
        world_count,
        source_bytes,
        archive_bytes,
        created_at,
        completed_at,
        error_summary,
        kind,
        base_backup_id,
    ) = stored;
    Ok(WorldBackupSummary {
        id,
        instance_id,
        instance_name,
        launch_session_id,
        trigger: BackupTrigger::from_database(&trigger).map_err(to_sql_error)?,
        state: BackupState::from_database(&state).map_err(to_sql_error)?,
        archive_path,
        world_count: sqlite_unsigned(world_count, "世界数量").map_err(to_sql_error)?,
        source_bytes: sqlite_unsigned(source_bytes, "存档源大小").map_err(to_sql_error)?,
        archive_bytes: sqlite_unsigned(archive_bytes, "备份归档大小").map_err(to_sql_error)?,
        created_at_unix_seconds: created_at,
        completed_at_unix_seconds: completed_at,
        error_summary,
        kind: BackupKind::from_database(&kind).map_err(to_sql_error)?,
        base_backup_id,
    })
}

fn prepare_world_backup(data_directory: &Path, instance_root: &Path) -> Result<BackupPreparation> {
    validate_instance_root(data_directory, instance_root)?;
    let saves = instance_root.join(".minecraft").join("saves");
    if !saves.exists() {
        return Ok(BackupPreparation {
            worlds: Vec::new(),
            entries: Vec::new(),
            source_bytes: 0,
        });
    }
    let saves_metadata = fs::symlink_metadata(&saves)?;
    if !saves_metadata.is_dir() || is_link_type(&saves_metadata.file_type()) {
        return Err(CoreError::Backup(
            "世界存档根不是可安全读取的本地目录".to_owned(),
        ));
    }
    let mut worlds = Vec::new();
    let mut entries = Vec::new();
    let mut children = fs::read_dir(&saves)?.collect::<std::io::Result<Vec<_>>>()?;
    children.sort_by_key(fs::DirEntry::file_name);
    for child in children {
        let metadata = fs::symlink_metadata(child.path())?;
        if is_link_type(&metadata.file_type()) {
            return Err(CoreError::Backup(format!(
                "存档包含不允许跟随的链接：{}",
                child.path().display()
            )));
        }
        if !metadata.is_dir() {
            continue;
        }
        let world_name = child.file_name().into_string().map_err(|_| {
            CoreError::Backup("世界目录名称不是有效 Unicode，无法安全归档".to_owned())
        })?;
        worlds.push(world_name);
        collect_archive_entries(&saves, &child.path(), &mut entries)?;
    }
    let source_bytes = entries
        .iter()
        .filter(|entry| !entry.directory)
        .fold(0_u64, |total, entry| total.saturating_add(entry.size));
    Ok(BackupPreparation {
        worlds,
        entries,
        source_bytes,
    })
}

fn collect_archive_entries(
    saves_root: &Path,
    current: &Path,
    entries: &mut Vec<ArchiveEntry>,
) -> Result<()> {
    let metadata = fs::symlink_metadata(current)?;
    if is_link_type(&metadata.file_type()) {
        return Err(CoreError::Backup(format!(
            "存档包含不允许跟随的链接：{}",
            current.display()
        )));
    }
    let name = archive_name(saves_root, current, metadata.is_dir())?;
    let mtime = metadata
        .modified()
        .ok()
        .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
        .and_then(|duration| i64::try_from(duration.as_nanos()).ok())
        .unwrap_or_default();
    entries.push(ArchiveEntry {
        source: current.to_path_buf(),
        name,
        directory: metadata.is_dir(),
        size: if metadata.is_file() {
            metadata.len()
        } else {
            0
        },
        mtime,
    });
    if metadata.is_dir() {
        let mut children = fs::read_dir(current)?.collect::<std::io::Result<Vec<_>>>()?;
        children.sort_by_key(fs::DirEntry::file_name);
        for child in children {
            collect_archive_entries(saves_root, &child.path(), entries)?;
        }
    }
    Ok(())
}

fn archive_name(saves_root: &Path, path: &Path, directory: bool) -> Result<String> {
    let relative = path
        .strip_prefix(saves_root)
        .map_err(|_| CoreError::Backup("世界文件越过存档根".to_owned()))?;
    let mut components = Vec::new();
    for component in relative.components() {
        let Component::Normal(value) = component else {
            return Err(CoreError::Backup("世界文件包含不安全路径".to_owned()));
        };
        components.push(value.to_str().ok_or_else(|| {
            CoreError::Backup("世界文件名称不是有效 Unicode，无法安全归档".to_owned())
        })?);
    }
    if components.is_empty() {
        return Err(CoreError::Backup("世界归档路径为空".to_owned()));
    }
    let mut name = format!("worlds/{}", components.join("/"));
    if directory {
        name.push('/');
    }
    Ok(name)
}

fn write_world_backup_zip(
    partial_path: &Path,
    backup: &WorldBackupSummary,
    preparation: &BackupPreparation,
    included: &[&ArchiveEntry],
    deleted: &[String],
) -> Result<fs::File> {
    let file = fs::File::create(partial_path)?;
    let mut writer = ZipWriter::new(file);
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .unix_permissions(0o600);
    let files_index = preparation
        .entries
        .iter()
        .filter(|entry| !entry.directory)
        .map(|entry| {
            json!({
                "path": entry.name.trim_start_matches("worlds/"),
                "size": entry.size,
                "mtime": entry.mtime,
            })
        })
        .collect::<Vec<_>>();
    let manifest = serde_json::to_vec_pretty(&json!({
        "schemaVersion": BACKUP_SCHEMA_VERSION,
        "kind": backup.kind,
        "backupId": backup.id,
        "baseBackupId": backup.base_backup_id,
        "instanceId": backup.instance_id,
        "instanceName": backup.instance_name,
        "trigger": backup.trigger,
        "createdAtUnixSeconds": backup.created_at_unix_seconds,
        "worlds": preparation.worlds,
        "files": files_index,
        "deleted": deleted,
    }))?;
    writer
        .start_file("manifest.json", options)
        .map_err(zip_error)?;
    writer.write_all(&manifest)?;
    for entry in included {
        if entry.directory {
            writer
                .add_directory(&entry.name, options)
                .map_err(zip_error)?;
        } else {
            writer.start_file(&entry.name, options).map_err(zip_error)?;
            let mut source = fs::File::open(&entry.source)?;
            std::io::copy(&mut source, &mut writer)?;
        }
    }
    writer.finish().map_err(zip_error)
}

/// 读取备份归档的 manifest.json（v1 清单缺少文件索引时返回空索引）。
pub(crate) fn read_backup_manifest(archive_path: &Path) -> Result<BackupManifest> {
    let file = fs::File::open(archive_path)?;
    let mut archive = ZipArchive::new(file)
        .map_err(|error| CoreError::Backup(format!("备份归档无法读取：{error}")))?;
    let mut entry = archive
        .by_name("manifest.json")
        .map_err(|error| CoreError::Backup(format!("备份归档缺少清单：{error}")))?;
    let mut buffer = Vec::new();
    std::io::copy(&mut entry, &mut buffer)?;
    let manifest = serde_json::from_slice(&buffer)
        .map_err(|error| CoreError::Backup(format!("备份清单无效：{error}")))?;
    Ok(manifest)
}

fn validate_instance_root(data_directory: &Path, instance_root: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(instance_root)?;
    if !metadata.is_dir() || is_link_type(&metadata.file_type()) {
        return Err(CoreError::Backup(
            "实例根不是 MoyuMax 受管的真实本地目录".to_owned(),
        ));
    }
    let managed_root = fs::canonicalize(data_directory.join("instances"))?;
    let instance = fs::canonicalize(instance_root)?;
    if instance.parent() != Some(managed_root.as_path()) {
        return Err(CoreError::Backup("拒绝备份受管实例根以外的目录".to_owned()));
    }
    Ok(())
}

fn validate_archive_path(data_directory: &Path, instance_id: &str, path: &Path) -> Result<()> {
    let root = data_directory
        .join("backups")
        .join("instances")
        .join(instance_id);
    fs::create_dir_all(&root)?;
    let canonical_root = fs::canonicalize(&root)?;
    let parent = path
        .parent()
        .ok_or_else(|| CoreError::Backup("备份归档路径无效".to_owned()))?;
    let canonical_parent = fs::canonicalize(parent)?;
    if canonical_parent != canonical_root
        || path.extension().and_then(|value| value.to_str()) != Some("zip")
    {
        return Err(CoreError::Backup("拒绝访问受管备份根以外的归档".to_owned()));
    }
    Ok(())
}

fn partial_path(archive_path: &Path) -> PathBuf {
    PathBuf::from(format!("{}.partial", archive_path.to_string_lossy()))
}

fn remove_file_if_exists(path: &Path) -> std::io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() && !is_link_type(&metadata.file_type()) => {
            Err(std::io::Error::other("备份路径意外成为目录"))
        }
        Ok(_) => fs::remove_file(path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn is_link_type(file_type: &fs::FileType) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::FileTypeExt;

        file_type.is_symlink() || file_type.is_symlink_dir() || file_type.is_symlink_file()
    }
    #[cfg(not(windows))]
    {
        file_type.is_symlink()
    }
}

fn sqlite_integer(value: u64, label: &str) -> Result<i64> {
    i64::try_from(value).map_err(|_| CoreError::Backup(format!("{label}超过 SQLite 可表示范围")))
}

fn sqlite_unsigned(value: i64, label: &str) -> Result<u64> {
    u64::try_from(value).map_err(|_| CoreError::InvalidStoredState(format!("{label}不能为负数")))
}

fn path_text(path: &Path) -> Result<String> {
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| CoreError::Backup("备份路径不是有效 Unicode".to_owned()))
}

fn zip_error(error: zip::result::ZipError) -> CoreError {
    CoreError::Backup(format!("无法写入世界备份 ZIP：{error}"))
}

fn to_sql_error(error: CoreError) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(error))
}
