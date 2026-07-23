use std::{
    fs,
    io::Write,
    path::{Component, Path, PathBuf},
};

use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

use crate::{AppService, CoreError, Result, unix_timestamp};

const BACKUP_SCHEMA_VERSION: u16 = 1;
const DEFAULT_SUCCESSFUL_BACKUPS_PER_INSTANCE: usize = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BackupTrigger {
    PreLaunch,
    PostExit,
    Manual,
}

impl BackupTrigger {
    const fn database_value(self) -> &'static str {
        match self {
            Self::PreLaunch => "pre_launch",
            Self::PostExit => "post_exit",
            Self::Manual => "manual",
        }
    }

    fn from_database(value: &str) -> Result<Self> {
        match value {
            "pre_launch" => Ok(Self::PreLaunch),
            "post_exit" => Ok(Self::PostExit),
            "manual" => Ok(Self::Manual),
            _ => Err(CoreError::InvalidStoredState(format!(
                "未知世界备份触发原因：{value}"
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
}

#[derive(Debug)]
struct ArchiveEntry {
    source: PathBuf,
    name: String,
    directory: bool,
    size: u64,
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
                   created_at_unix_seconds, completed_at_unix_seconds, error_summary
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
            let file = write_world_backup_zip(&partial_path, &backup, &preparation)?;
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
                       created_at_unix_seconds, completed_at_unix_seconds, error_summary
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
                created_at_unix_seconds, completed_at_unix_seconds, error_summary
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
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
        let backups = self
            .list_world_backups(Some(instance_id))?
            .into_iter()
            .filter(|backup| backup.state == BackupState::Ready)
            .skip(DEFAULT_SUCCESSFUL_BACKUPS_PER_INSTANCE)
            .collect::<Vec<_>>();
        for backup in backups {
            let Some(path) = backup.archive_path.as_deref().map(PathBuf::from) else {
                continue;
            };
            if validate_archive_path(&self.selected_data_directory()?, instance_id, &path).is_err()
            {
                continue;
            }
            if remove_file_if_exists(&path).is_err() {
                continue;
            }
            self.connection()?.execute(
                "DELETE FROM world_backups WHERE id = ?1",
                params![backup.id],
            )?;
        }
        Ok(())
    }
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
    entries.push(ArchiveEntry {
        source: current.to_path_buf(),
        name,
        directory: metadata.is_dir(),
        size: if metadata.is_file() {
            metadata.len()
        } else {
            0
        },
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
) -> Result<fs::File> {
    let file = fs::File::create(partial_path)?;
    let mut writer = ZipWriter::new(file);
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .unix_permissions(0o600);
    let manifest = serde_json::to_vec_pretty(&json!({
        "schemaVersion": BACKUP_SCHEMA_VERSION,
        "backupId": backup.id,
        "instanceId": backup.instance_id,
        "instanceName": backup.instance_name,
        "trigger": backup.trigger,
        "createdAtUnixSeconds": backup.created_at_unix_seconds,
        "worlds": preparation.worlds,
    }))?;
    writer
        .start_file("manifest.json", options)
        .map_err(zip_error)?;
    writer.write_all(&manifest)?;
    for entry in &preparation.entries {
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
