use std::{
    fs,
    path::{Path, PathBuf},
};

use rusqlite::{OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{AppService, CoreError, ManagedInstanceSummary, Result, unix_timestamp};

const RETENTION_SECONDS: i64 = 30 * 24 * 60 * 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RecycleItemKind {
    Instance,
    Screenshot,
    Resource,
    World,
}

impl RecycleItemKind {
    fn database_value(self) -> &'static str {
        match self {
            Self::Instance => "instance",
            Self::Screenshot => "screenshot",
            Self::Resource => "resource",
            Self::World => "world",
        }
    }

    fn from_database(value: &str) -> Result<Self> {
        match value {
            "instance" => Ok(Self::Instance),
            "screenshot" => Ok(Self::Screenshot),
            "resource" => Ok(Self::Resource),
            "world" => Ok(Self::World),
            _ => Err(CoreError::InvalidStoredState(format!(
                "未知回收站对象类型：{value}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RecycleItemState {
    Moving,
    Ready,
    Restoring,
    Purging,
    Failed,
}

impl RecycleItemState {
    fn database_value(self) -> &'static str {
        match self {
            Self::Moving => "moving",
            Self::Ready => "ready",
            Self::Restoring => "restoring",
            Self::Purging => "purging",
            Self::Failed => "failed",
        }
    }

    fn from_database(value: &str) -> Result<Self> {
        match value {
            "moving" => Ok(Self::Moving),
            "ready" => Ok(Self::Ready),
            "restoring" => Ok(Self::Restoring),
            "purging" => Ok(Self::Purging),
            "failed" => Ok(Self::Failed),
            _ => Err(CoreError::InvalidStoredState(format!(
                "未知回收站事务状态：{value}"
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecycleBinItem {
    pub id: String,
    pub kind: RecycleItemKind,
    pub subject_id: String,
    pub display_name: String,
    pub original_path: String,
    pub recycled_path: String,
    pub original_state: String,
    pub size_bytes: u64,
    pub deleted_at_unix_seconds: i64,
    pub expires_at_unix_seconds: i64,
    pub state: RecycleItemState,
    /// 恢复所需的附加数据（资源索引行 JSON）；无附加数据时为 None。
    #[serde(default)]
    pub payload: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecyclePurgeResult {
    pub item_id: String,
    pub released_bytes: u64,
    pub removed_subjects: u64,
}

impl AppService {
    pub fn list_recycle_bin_items(&self) -> Result<Vec<RecycleBinItem>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "
            SELECT id, item_kind, subject_id, display_name, original_path, recycled_path,
                   original_state, size_bytes, deleted_at_unix_seconds,
                   expires_at_unix_seconds, state, payload
            FROM recycle_bin_items
            ORDER BY deleted_at_unix_seconds DESC, id
            ",
        )?;
        let rows = statement.query_map([], read_item_row)?;
        rows.map(|row| row.and_then(decode_item))
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(CoreError::from)
    }

    pub fn recycle_instance(&self, instance_id: &str) -> Result<RecycleBinItem> {
        let instance = self.active_instance(instance_id)?;
        self.ensure_instance_can_be_recycled(instance_id, &instance.state)?;

        let data_directory = self.selected_data_directory()?;
        let source = PathBuf::from(&instance.root_directory);
        validate_instance_source(&data_directory, &source)?;
        let size_bytes = directory_size(&source)?;
        let item_id = Uuid::new_v4().to_string();
        let recycled_path = data_directory
            .join(".recycle")
            .join("instances")
            .join(&item_id);
        let recycle_root = recycled_path
            .parent()
            .ok_or_else(|| CoreError::Recycle("无法确定受管回收站目录".to_owned()))?;
        fs::create_dir_all(recycle_root)?;
        validate_recycled_path(&data_directory, &recycled_path)?;
        if recycled_path.exists() {
            return Err(CoreError::Recycle("回收站目标已被占用，请重试".to_owned()));
        }

        let deleted_at = unix_timestamp();
        let item = RecycleBinItem {
            id: item_id,
            kind: RecycleItemKind::Instance,
            subject_id: instance.id.clone(),
            display_name: instance.name,
            original_path: path_text(&source),
            recycled_path: path_text(&recycled_path),
            original_state: instance.state,
            size_bytes,
            deleted_at_unix_seconds: deleted_at,
            expires_at_unix_seconds: deleted_at.saturating_add(RETENTION_SECONDS),
            state: RecycleItemState::Moving,
            payload: None,
        };

        self.persist_move_intent(&item)?;
        if let Err(error) = fs::rename(&source, &recycled_path) {
            let _ = self.rollback_move_intent(&item);
            return Err(CoreError::Recycle(format!(
                "无法将实例目录移入回收站：{error}"
            )));
        }
        if let Err(error) = self.finalize_move(&item) {
            if fs::rename(&recycled_path, &source).is_ok() {
                let _ = self.rollback_move_intent(&item);
            }
            return Err(error);
        }

        Ok(RecycleBinItem {
            state: RecycleItemState::Ready,
            ..item
        })
    }

    /// 把截图移入回收站（与实例、资源、世界共用同一事务模型）。
    pub fn delete_instance_screenshot(
        &self,
        instance_id: &str,
        file_name: &str,
    ) -> Result<RecycleBinItem> {
        let instance = self.ready_instance(instance_id)?;
        validate_plain_filename(file_name, ".png")?;
        let source = Path::new(&instance.root_directory)
            .join(".minecraft")
            .join("screenshots")
            .join(file_name);
        if !source.is_file() {
            return Err(CoreError::Recycle(format!("截图 {file_name} 不存在")));
        }
        self.recycle_entry(
            RecycleItemKind::Screenshot,
            &instance,
            file_name.to_owned(),
            &source,
            None,
        )
    }

    /// 把资源内容（资源包/光影/数据包）移入回收站，索引行随恢复无损往返。
    pub fn delete_instance_resource(&self, resource_id: &str) -> Result<RecycleBinItem> {
        let resource = self
            .list_instance_resources_by_id(resource_id)?
            .into_iter()
            .next()
            .ok_or_else(|| CoreError::Recycle("资源项不存在".to_owned()))?;
        let instance = self.ready_instance(&resource.instance_id)?;
        let disk_name = if resource.enabled {
            resource.file_name.clone()
        } else {
            format!("{}.disabled", resource.file_name)
        };
        let source = Path::new(&instance.root_directory).join(&resource.relative_path);
        let source = source
            .parent()
            .map_or_else(|| source.clone(), |parent| parent.join(&disk_name));
        if !source.is_file() {
            return Err(CoreError::Recycle(format!(
                "资源文件 {disk_name} 不在预期位置，可能被外部移动或删除"
            )));
        }
        let payload = serde_json::to_string(&resource)?;
        self.recycle_entry(
            RecycleItemKind::Resource,
            &instance,
            resource.display_name.clone(),
            &source,
            Some(payload),
        )
    }

    /// 把世界目录移入回收站。
    pub fn delete_instance_world(
        &self,
        instance_id: &str,
        world_name: &str,
    ) -> Result<RecycleBinItem> {
        let instance = self.ready_instance(instance_id)?;
        let source = Path::new(&instance.root_directory)
            .join(".minecraft")
            .join("saves")
            .join(world_name);
        if !source.is_dir() || !source.join("level.dat").is_file() {
            return Err(CoreError::Recycle(format!("世界 {world_name} 不存在")));
        }
        self.recycle_entry(
            RecycleItemKind::World,
            &instance,
            world_name.to_owned(),
            &source,
            None,
        )
    }

    /// 恢复非实例回收项目（截图/资源/世界）；原位置被占用时拒绝，不覆盖。
    pub fn restore_recycled_entry(&self, item_id: &str) -> Result<RecycleBinItem> {
        let item = self.recycle_item(item_id)?;
        if item.kind == RecycleItemKind::Instance || item.state != RecycleItemState::Ready {
            return Err(CoreError::Recycle(
                "该回收站项目当前不能恢复，请等待正在进行的操作完成".to_owned(),
            ));
        }
        let data_directory = self.selected_data_directory()?;
        let original_path = PathBuf::from(&item.original_path);
        let recycled_path = PathBuf::from(&item.recycled_path);
        self.validate_entry_original(&data_directory, &item.subject_id, &original_path)?;
        validate_entry_recycled_path(&data_directory, &recycled_path)?;
        if original_path.exists() {
            return Err(CoreError::Recycle(
                "原位置已被占用；为避免覆盖现有内容，恢复已停止".to_owned(),
            ));
        }
        if !recycled_path.exists() {
            return Err(CoreError::Recycle(
                "回收站中的内容缺失，无法安全恢复".to_owned(),
            ));
        }
        if let Some(parent) = original_path.parent() {
            fs::create_dir_all(parent)?;
        }
        self.persist_restore_intent(&item)?;
        if let Err(error) = fs::rename(&recycled_path, &original_path) {
            let _ = self.rollback_restore_intent(&item);
            return Err(CoreError::Recycle(format!("无法恢复内容：{error}")));
        }
        if let Err(error) = self.finalize_restore(&item) {
            if fs::rename(&original_path, &recycled_path).is_ok() {
                let _ = self.rollback_restore_intent(&item);
            }
            return Err(error);
        }
        Ok(item)
    }

    fn recycle_entry(
        &self,
        kind: RecycleItemKind,
        instance: &ManagedInstanceSummary,
        display_name: String,
        source: &Path,
        payload: Option<String>,
    ) -> Result<RecycleBinItem> {
        let data_directory = self.selected_data_directory()?;
        validate_entry_source(&data_directory, source)?;
        let size_bytes = directory_size(source)?;
        let item_id = Uuid::new_v4().to_string();
        let recycled_path = data_directory
            .join(".recycle")
            .join("entries")
            .join(&item_id);
        let recycle_root = recycled_path
            .parent()
            .ok_or_else(|| CoreError::Recycle("无法确定受管回收站目录".to_owned()))?;
        fs::create_dir_all(recycle_root)?;
        validate_entry_recycled_path(&data_directory, &recycled_path)?;
        if recycled_path.exists() {
            return Err(CoreError::Recycle("回收站目标已被占用，请重试".to_owned()));
        }

        let deleted_at = unix_timestamp();
        let item = RecycleBinItem {
            id: item_id,
            kind,
            subject_id: instance.id.clone(),
            display_name,
            original_path: path_text(source),
            recycled_path: path_text(&recycled_path),
            original_state: "ready".to_owned(),
            size_bytes,
            deleted_at_unix_seconds: deleted_at,
            expires_at_unix_seconds: deleted_at.saturating_add(RETENTION_SECONDS),
            state: RecycleItemState::Moving,
            payload,
        };

        self.persist_move_intent(&item)?;
        if let Err(error) = fs::rename(source, &recycled_path) {
            let _ = self.rollback_move_intent(&item);
            return Err(CoreError::Recycle(format!("无法将内容移入回收站：{error}")));
        }
        if let Err(error) = self.finalize_move(&item) {
            if fs::rename(&recycled_path, source).is_ok() {
                let _ = self.rollback_move_intent(&item);
            }
            return Err(error);
        }

        Ok(RecycleBinItem {
            state: RecycleItemState::Ready,
            ..item
        })
    }

    /// 校验恢复目标仍在该实例目录内。
    fn validate_entry_original(
        &self,
        data_directory: &Path,
        instance_id: &str,
        target: &Path,
    ) -> Result<()> {
        let instance = self
            .list_instances()?
            .into_iter()
            .find(|instance| instance.id == instance_id)
            .ok_or_else(|| CoreError::Recycle("回收项目引用的实例不存在".to_owned()))?;
        let instance_root = PathBuf::from(&instance.root_directory);
        if !target.starts_with(&instance_root) {
            return Err(CoreError::Recycle(
                "恢复目标不在所属实例目录内，已拒绝".to_owned(),
            ));
        }
        let instances_root = data_directory.join("instances");
        fs::create_dir_all(&instances_root)?;
        let canonical_root = fs::canonicalize(&instances_root)
            .map_err(|error| CoreError::Recycle(format!("受管实例根不可访问：{error}")))?;
        let canonical_instance = fs::canonicalize(&instance_root)
            .map_err(|error| CoreError::Recycle(format!("实例目录不可访问：{error}")))?;
        if !canonical_instance.starts_with(&canonical_root) {
            return Err(CoreError::Recycle("实例目录不在受管区域内".to_owned()));
        }
        Ok(())
    }

    pub fn restore_recycle_bin_item(&self, item_id: &str) -> Result<ManagedInstanceSummary> {
        let item = self.recycle_item(item_id)?;
        if item.kind != RecycleItemKind::Instance || item.state != RecycleItemState::Ready {
            return Err(CoreError::Recycle(
                "该回收站项目当前不能恢复，请等待正在进行的操作完成".to_owned(),
            ));
        }
        let data_directory = self.selected_data_directory()?;
        let original_path = PathBuf::from(&item.original_path);
        let recycled_path = PathBuf::from(&item.recycled_path);
        validate_restore_target(&data_directory, &original_path)?;
        validate_recycled_path(&data_directory, &recycled_path)?;
        if original_path.exists() {
            return Err(CoreError::Recycle(
                "原位置已被占用；为避免覆盖现有内容，恢复已停止".to_owned(),
            ));
        }
        if !recycled_path.exists() {
            return Err(CoreError::Recycle(
                "回收站中的实例目录缺失，无法安全恢复".to_owned(),
            ));
        }

        self.persist_restore_intent(&item)?;
        if let Err(error) = fs::rename(&recycled_path, &original_path) {
            let _ = self.rollback_restore_intent(&item);
            return Err(CoreError::Recycle(format!("无法恢复实例目录：{error}")));
        }
        if let Err(error) = self.finalize_restore(&item) {
            if fs::rename(&original_path, &recycled_path).is_ok() {
                let _ = self.rollback_restore_intent(&item);
            }
            return Err(error);
        }
        self.instance_by_id(&item.subject_id)
    }

    pub fn purge_recycle_bin_item(&self, item_id: &str) -> Result<RecyclePurgeResult> {
        let item = self.recycle_item(item_id)?;
        if item.state != RecycleItemState::Ready {
            return Err(CoreError::Recycle(
                "该回收站项目当前不能永久删除，请等待正在进行的操作完成".to_owned(),
            ));
        }
        let data_directory = self.selected_data_directory()?;
        let recycled_path = PathBuf::from(&item.recycled_path);
        self.validate_item_paths(
            &data_directory,
            &item,
            Path::new(&item.original_path),
            &recycled_path,
        )?;
        self.persist_purge_intent(&item)?;
        if let Err(error) = remove_owned_path(&recycled_path) {
            let _ = self.rollback_purge_intent(&item);
            return Err(CoreError::Recycle(format!(
                "无法永久删除回收站文件：{error}"
            )));
        }
        self.finalize_purge(&item)?;
        Ok(RecyclePurgeResult {
            item_id: item.id,
            released_bytes: item.size_bytes,
            removed_subjects: 1,
        })
    }

    pub(crate) fn recover_interrupted_recycle_operations(&self) -> Result<()> {
        let items = self
            .list_recycle_bin_items()?
            .into_iter()
            .filter(|item| {
                matches!(
                    item.state,
                    RecycleItemState::Moving
                        | RecycleItemState::Restoring
                        | RecycleItemState::Purging
                )
            })
            .collect::<Vec<_>>();
        for item in items {
            let recovery = match item.state {
                RecycleItemState::Moving => self.recover_move(&item),
                RecycleItemState::Restoring => self.recover_restore(&item),
                RecycleItemState::Purging => self.recover_purge(&item),
                RecycleItemState::Ready | RecycleItemState::Failed => Ok(()),
            };
            if recovery.is_err() {
                self.mark_recycle_failed(&item)?;
            }
        }
        Ok(())
    }

    fn recover_move(&self, item: &RecycleBinItem) -> Result<()> {
        let data_directory = self.selected_data_directory()?;
        let source = PathBuf::from(&item.original_path);
        let destination = PathBuf::from(&item.recycled_path);
        self.validate_item_paths(&data_directory, item, &source, &destination)?;
        match (source.exists(), destination.exists()) {
            (true, false) => {
                fs::rename(&source, &destination)?;
                self.finalize_move(item)
            }
            (false, true) => self.finalize_move(item),
            _ => Err(CoreError::Recycle(
                "中断的回收操作存在路径冲突或数据缺失".to_owned(),
            )),
        }
    }

    fn recover_restore(&self, item: &RecycleBinItem) -> Result<()> {
        let data_directory = self.selected_data_directory()?;
        let source = PathBuf::from(&item.recycled_path);
        let destination = PathBuf::from(&item.original_path);
        self.validate_item_paths(&data_directory, item, &destination, &source)?;
        match (source.exists(), destination.exists()) {
            (true, false) => {
                fs::rename(&source, &destination)?;
                self.finalize_restore(item)
            }
            (false, true) => self.finalize_restore(item),
            (true, true) => self.rollback_restore_intent(item),
            (false, false) => Err(CoreError::Recycle(
                "中断的恢复操作找不到任一实例副本".to_owned(),
            )),
        }
    }

    fn recover_purge(&self, item: &RecycleBinItem) -> Result<()> {
        let data_directory = self.selected_data_directory()?;
        let path = PathBuf::from(&item.recycled_path);
        self.validate_item_paths(&data_directory, item, Path::new(&item.original_path), &path)?;
        remove_owned_path(&path)?;
        self.finalize_purge(item)
    }

    /// 按项目类型校验原位置与回收站位置都在受管区域内。
    fn validate_item_paths(
        &self,
        data_directory: &Path,
        item: &RecycleBinItem,
        original: &Path,
        recycled: &Path,
    ) -> Result<()> {
        if item.kind == RecycleItemKind::Instance {
            validate_restore_target(data_directory, original)?;
            validate_recycled_path(data_directory, recycled)
        } else {
            self.validate_entry_original(data_directory, &item.subject_id, original)?;
            validate_entry_recycled_path(data_directory, recycled)
        }
    }

    fn active_instance(&self, instance_id: &str) -> Result<ManagedInstanceSummary> {
        let connection = self.connection()?;
        connection
            .query_row(
                "
                SELECT id, name, game_version, loader_kind, loader_version, root_directory, state
                FROM instances
                WHERE id = ?1
                  AND NOT EXISTS (
                      SELECT 1 FROM recycle_bin_items
                      WHERE subject_id = instances.id AND item_kind = 'instance'
                  )
                ",
                params![instance_id],
                read_instance_row,
            )
            .optional()?
            .ok_or_else(|| CoreError::Recycle("实例不存在或已经在回收站中".to_owned()))
    }

    fn instance_by_id(&self, instance_id: &str) -> Result<ManagedInstanceSummary> {
        let connection = self.connection()?;
        connection
            .query_row(
                "SELECT id, name, game_version, loader_kind, loader_version, root_directory, state FROM instances WHERE id = ?1",
                params![instance_id],
                read_instance_row,
            )
            .optional()?
            .ok_or_else(|| CoreError::Recycle("恢复后的实例索引不存在".to_owned()))
    }

    fn ensure_instance_can_be_recycled(&self, instance_id: &str, state: &str) -> Result<()> {
        if state != "ready" {
            return Err(CoreError::Recycle(format!(
                "实例当前状态为“{state}”，不能移入回收站"
            )));
        }
        let connection = self.connection()?;
        let active_launch: bool = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM launch_sessions WHERE instance_id = ?1 AND state IN ('starting', 'running'))",
            params![instance_id],
            |row| row.get(0),
        )?;
        if active_launch {
            return Err(CoreError::Recycle(
                "实例仍在运行，请先停止游戏再移入回收站".to_owned(),
            ));
        }
        let active_task: bool = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM content_install_tasks WHERE instance_id = ?1 AND state IN ('queued', 'running', 'committing', 'paused', 'awaiting_recovery'))",
            params![instance_id],
            |row| row.get(0),
        )?;
        if active_task {
            return Err(CoreError::Recycle(
                "实例仍有未完成的内容任务，请先处理任务再移入回收站".to_owned(),
            ));
        }
        Ok(())
    }

    fn recycle_item(&self, item_id: &str) -> Result<RecycleBinItem> {
        let connection = self.connection()?;
        connection
            .query_row(
                "
                SELECT id, item_kind, subject_id, display_name, original_path, recycled_path,
                       original_state, size_bytes, deleted_at_unix_seconds,
                       expires_at_unix_seconds, state, payload
                FROM recycle_bin_items WHERE id = ?1
                ",
                params![item_id],
                read_item_row,
            )
            .optional()?
            .map(decode_item)
            .transpose()?
            .ok_or_else(|| CoreError::Recycle("回收站项目不存在".to_owned()))
    }

    fn persist_move_intent(&self, item: &RecycleBinItem) -> Result<()> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute(
            "
            INSERT INTO recycle_bin_items (
                id, item_kind, subject_id, display_name, original_path, recycled_path,
                original_state, size_bytes, deleted_at_unix_seconds,
                expires_at_unix_seconds, state, payload
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 'moving', ?11)
            ",
            params![
                item.id,
                item.kind.database_value(),
                item.subject_id,
                item.display_name,
                item.original_path,
                item.recycled_path,
                item.original_state,
                sqlite_integer(item.size_bytes)?,
                item.deleted_at_unix_seconds,
                item.expires_at_unix_seconds,
                item.payload,
            ],
        )?;
        if item.kind == RecycleItemKind::Instance {
            transaction.execute(
                "UPDATE instances SET state = 'recycling' WHERE id = ?1",
                params![item.subject_id],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    fn finalize_move(&self, item: &RecycleBinItem) -> Result<()> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        match item.kind {
            RecycleItemKind::Instance => {
                transaction.execute(
                    "UPDATE instances SET root_directory = ?1, state = 'recycled' WHERE id = ?2",
                    params![item.recycled_path, item.subject_id],
                )?;
            }
            RecycleItemKind::Resource => {
                // 资源索引行与文件进入同一删除语义：文件移走后删除索引。
                let payload = item.payload.as_deref().ok_or_else(|| {
                    CoreError::InvalidStoredState("资源回收项目缺少索引负载".to_owned())
                })?;
                let resource: crate::InstanceResource =
                    serde_json::from_str(payload).map_err(|error| {
                        CoreError::InvalidStoredState(format!("资源索引负载无效：{error}"))
                    })?;
                transaction.execute(
                    "DELETE FROM instance_resources WHERE id = ?1",
                    params![resource.id],
                )?;
            }
            RecycleItemKind::Screenshot | RecycleItemKind::World => {}
        }
        transaction.execute(
            "UPDATE recycle_bin_items SET state = 'ready' WHERE id = ?1",
            params![item.id],
        )?;
        transaction.commit()?;
        Ok(())
    }

    fn rollback_move_intent(&self, item: &RecycleBinItem) -> Result<()> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if item.kind == RecycleItemKind::Instance {
            transaction.execute(
                "UPDATE instances SET root_directory = ?1, state = ?2 WHERE id = ?3",
                params![item.original_path, item.original_state, item.subject_id],
            )?;
        }
        transaction.execute(
            "DELETE FROM recycle_bin_items WHERE id = ?1",
            params![item.id],
        )?;
        transaction.commit()?;
        Ok(())
    }

    fn persist_restore_intent(&self, item: &RecycleBinItem) -> Result<()> {
        self.update_operation_state(item, RecycleItemState::Restoring, "restoring")
    }

    fn finalize_restore(&self, item: &RecycleBinItem) -> Result<()> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        match item.kind {
            RecycleItemKind::Instance => {
                transaction.execute(
                    "UPDATE instances SET root_directory = ?1, state = ?2 WHERE id = ?3",
                    params![item.original_path, item.original_state, item.subject_id],
                )?;
            }
            RecycleItemKind::Resource => {
                // 从 payload 无损重建索引行，保留启用与自动更新标志。
                let payload = item.payload.as_deref().ok_or_else(|| {
                    CoreError::InvalidStoredState("资源回收项目缺少索引负载".to_owned())
                })?;
                let resource: crate::InstanceResource =
                    serde_json::from_str(payload).map_err(|error| {
                        CoreError::InvalidStoredState(format!("资源索引负载无效：{error}"))
                    })?;
                transaction.execute(
                    "
                    INSERT INTO instance_resources (
                        id, instance_id, kind, display_name, file_name, relative_path,
                        size, sha256, enabled, world_name, imported_at_unix_seconds
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
                    ",
                    params![
                        resource.id,
                        resource.instance_id,
                        resource.kind.database_value(),
                        resource.display_name,
                        resource.file_name,
                        resource.relative_path,
                        sqlite_integer(resource.size)?,
                        resource.sha256,
                        resource.enabled,
                        resource.world_name,
                        resource.imported_at_unix_seconds,
                    ],
                )?;
            }
            RecycleItemKind::Screenshot | RecycleItemKind::World => {}
        }
        transaction.execute(
            "DELETE FROM recycle_bin_items WHERE id = ?1",
            params![item.id],
        )?;
        transaction.commit()?;
        Ok(())
    }

    fn rollback_restore_intent(&self, item: &RecycleBinItem) -> Result<()> {
        self.update_operation_state(item, RecycleItemState::Ready, "recycled")
    }

    fn persist_purge_intent(&self, item: &RecycleBinItem) -> Result<()> {
        self.update_operation_state(item, RecycleItemState::Purging, "purging")
    }

    fn rollback_purge_intent(&self, item: &RecycleBinItem) -> Result<()> {
        self.update_operation_state(item, RecycleItemState::Ready, "recycled")
    }

    fn finalize_purge(&self, item: &RecycleBinItem) -> Result<()> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if item.kind == RecycleItemKind::Instance {
            transaction.execute(
                "DELETE FROM instances WHERE id = ?1",
                params![item.subject_id],
            )?;
        }
        transaction.execute(
            "DELETE FROM recycle_bin_items WHERE id = ?1",
            params![item.id],
        )?;
        transaction.commit()?;
        Ok(())
    }

    fn update_operation_state(
        &self,
        item: &RecycleBinItem,
        recycle_state: RecycleItemState,
        instance_state: &str,
    ) -> Result<()> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute(
            "UPDATE recycle_bin_items SET state = ?1 WHERE id = ?2",
            params![recycle_state.database_value(), item.id],
        )?;
        if item.kind == RecycleItemKind::Instance {
            transaction.execute(
                "UPDATE instances SET state = ?1 WHERE id = ?2",
                params![instance_state, item.subject_id],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    fn mark_recycle_failed(&self, item: &RecycleBinItem) -> Result<()> {
        self.update_operation_state(item, RecycleItemState::Failed, "recycle_failed")
    }
}

type StoredItem = (
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    i64,
    i64,
    i64,
    String,
    Option<String>,
);

fn read_item_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredItem> {
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
    ))
}

fn decode_item(stored: StoredItem) -> rusqlite::Result<RecycleBinItem> {
    let (
        id,
        kind,
        subject_id,
        display_name,
        original_path,
        recycled_path,
        original_state,
        size,
        deleted,
        expires,
        state,
        payload,
    ) = stored;
    let kind = RecycleItemKind::from_database(&kind).map_err(to_sql_error)?;
    let state = RecycleItemState::from_database(&state).map_err(to_sql_error)?;
    let size_bytes = u64::try_from(size).map_err(|_| {
        to_sql_error(CoreError::InvalidStoredState(
            "回收站对象大小不能为负数".to_owned(),
        ))
    })?;
    Ok(RecycleBinItem {
        id,
        kind,
        subject_id,
        display_name,
        original_path,
        recycled_path,
        original_state,
        size_bytes,
        deleted_at_unix_seconds: deleted,
        expires_at_unix_seconds: expires,
        state,
        payload,
    })
}

fn read_instance_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ManagedInstanceSummary> {
    Ok(ManagedInstanceSummary {
        id: row.get(0)?,
        name: row.get(1)?,
        game_version: row.get(2)?,
        loader_kind: row.get(3)?,
        loader_version: row.get(4)?,
        root_directory: row.get(5)?,
        state: row.get(6)?,
    })
}

fn validate_instance_source(data_directory: &Path, source: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(source)
        .map_err(|error| CoreError::Recycle(format!("实例目录不可访问：{error}")))?;
    if !metadata.is_dir() || is_link_type(&metadata.file_type()) {
        return Err(CoreError::Recycle(
            "实例根必须是 MoyuMax 受管的真实本地目录".to_owned(),
        ));
    }
    let root = data_directory.join("instances");
    let canonical_root = fs::canonicalize(&root)
        .map_err(|error| CoreError::Recycle(format!("受管实例根不可访问：{error}")))?;
    let canonical_source = fs::canonicalize(source)
        .map_err(|error| CoreError::Recycle(format!("实例目录不可访问：{error}")))?;
    if canonical_source.parent() != Some(canonical_root.as_path()) {
        return Err(CoreError::Recycle(
            "拒绝移动受管实例根以外的目录".to_owned(),
        ));
    }
    Ok(())
}

fn validate_restore_target(data_directory: &Path, target: &Path) -> Result<()> {
    let root = data_directory.join("instances");
    fs::create_dir_all(&root)?;
    let canonical_root = fs::canonicalize(&root)
        .map_err(|error| CoreError::Recycle(format!("受管实例根不可访问：{error}")))?;
    let parent = target
        .parent()
        .ok_or_else(|| CoreError::Recycle("实例原位置无效".to_owned()))?;
    let canonical_parent = fs::canonicalize(parent)
        .map_err(|error| CoreError::Recycle(format!("实例原位置不可访问：{error}")))?;
    if canonical_parent != canonical_root || target.file_name().is_none() {
        return Err(CoreError::Recycle(
            "拒绝恢复到受管实例根以外的位置".to_owned(),
        ));
    }
    Ok(())
}

fn validate_recycled_path(data_directory: &Path, path: &Path) -> Result<()> {
    let root = data_directory.join(".recycle").join("instances");
    fs::create_dir_all(&root)?;
    let canonical_root = fs::canonicalize(&root)
        .map_err(|error| CoreError::Recycle(format!("受管回收站不可访问：{error}")))?;
    let parent = path
        .parent()
        .ok_or_else(|| CoreError::Recycle("回收站路径无效".to_owned()))?;
    let canonical_parent = fs::canonicalize(parent)
        .map_err(|error| CoreError::Recycle(format!("回收站路径不可访问：{error}")))?;
    if canonical_parent != canonical_root || path.file_name().is_none() {
        return Err(CoreError::Recycle(
            "拒绝访问 MoyuMax 受管回收站以外的路径".to_owned(),
        ));
    }
    Ok(())
}

/// 截图/资源/世界删除来源必须位于受管实例区域内且是真实路径（非链接）。
fn validate_entry_source(data_directory: &Path, source: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(source)
        .map_err(|error| CoreError::Recycle(format!("待删除内容不可访问：{error}")))?;
    if is_link_type(&metadata.file_type()) {
        return Err(CoreError::Recycle("拒绝移动链接路径".to_owned()));
    }
    let root = data_directory.join("instances");
    let canonical_root = fs::canonicalize(&root)
        .map_err(|error| CoreError::Recycle(format!("受管实例根不可访问：{error}")))?;
    let canonical_source = fs::canonicalize(source)
        .map_err(|error| CoreError::Recycle(format!("待删除内容不可访问：{error}")))?;
    if !canonical_source.starts_with(&canonical_root) {
        return Err(CoreError::Recycle(
            "拒绝移动受管实例区域以外的内容".to_owned(),
        ));
    }
    Ok(())
}

fn validate_entry_recycled_path(data_directory: &Path, path: &Path) -> Result<()> {
    let root = data_directory.join(".recycle").join("entries");
    fs::create_dir_all(&root)?;
    let canonical_root = fs::canonicalize(&root)
        .map_err(|error| CoreError::Recycle(format!("受管回收站不可访问：{error}")))?;
    let parent = path
        .parent()
        .ok_or_else(|| CoreError::Recycle("回收站路径无效".to_owned()))?;
    let canonical_parent = fs::canonicalize(parent)
        .map_err(|error| CoreError::Recycle(format!("回收站路径不可访问：{error}")))?;
    if canonical_parent != canonical_root || path.file_name().is_none() {
        return Err(CoreError::Recycle(
            "拒绝访问 MoyuMax 受管回收站以外的路径".to_owned(),
        ));
    }
    Ok(())
}

fn validate_plain_filename(file_name: &str, extension: &str) -> Result<()> {
    let path = Path::new(file_name);
    let valid = !file_name.is_empty()
        && file_name.len() <= 240
        && !file_name.chars().any(char::is_control)
        && path.components().count() == 1
        && matches!(
            path.components().next(),
            Some(std::path::Component::Normal(_))
        )
        && file_name.to_ascii_lowercase().ends_with(extension);
    if !valid {
        return Err(CoreError::Recycle(format!(
            "文件名不安全或不是 {extension}：{file_name}"
        )));
    }
    Ok(())
}

fn directory_size(path: &Path) -> Result<u64> {
    let metadata = fs::symlink_metadata(path)?;
    if is_link_type(&metadata.file_type()) || metadata.is_file() {
        return Ok(metadata.len());
    }
    if !metadata.is_dir() {
        return Ok(0);
    }
    let mut total = 0_u64;
    for entry in fs::read_dir(path)? {
        total = total.saturating_add(directory_size(&entry?.path())?);
    }
    Ok(total)
}

fn remove_owned_path(path: &Path) -> std::io::Result<()> {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return Ok(());
    };
    if metadata.is_dir() && !is_link_type(&metadata.file_type()) {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    }
}

fn sqlite_integer(value: u64) -> Result<i64> {
    i64::try_from(value)
        .map_err(|_| CoreError::Recycle("回收站对象大小超过 SQLite 可表示范围".to_owned()))
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

fn path_text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn to_sql_error(error: CoreError) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(error))
}
