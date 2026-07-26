//! 实例资源内容管理（资源包/光影/数据包/模组）。
//!
//! 资源文件直接落在实例目录内：资源包进入 `resourcepacks/`、光影进入
//! `shaderpacks/`、数据包进入所选世界的 `datapacks/`、模组进入 `mods/`。
//! 导入经过受管暂存与原子 rename，索引写入与文件落位互为补偿；
//! 启用/停用通过 `.disabled` 后缀切换，游戏忽略该后缀文件。
//! 不做删除，删除随回收站扩展另行设计。

use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};

use rusqlite::{TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{AppService, CoreError, ManagedInstanceSummary, Result, unix_timestamp};

const MAX_RESOURCE_BYTES: u64 = 512 * 1024 * 1024;
const DISABLED_SUFFIX: &str = ".disabled";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InstanceResourceKind {
    ResourcePack,
    Shader,
    Datapack,
    Mod,
}

impl InstanceResourceKind {
    pub(crate) const fn database_value(self) -> &'static str {
        match self {
            Self::ResourcePack => "resourcepack",
            Self::Shader => "shader",
            Self::Datapack => "datapack",
            Self::Mod => "mod",
        }
    }

    fn from_database(value: &str) -> Result<Self> {
        match value {
            "resourcepack" => Ok(Self::ResourcePack),
            "shader" => Ok(Self::Shader),
            "datapack" => Ok(Self::Datapack),
            "mod" => Ok(Self::Mod),
            _ => Err(CoreError::InvalidStoredState(format!(
                "未知资源类型：{value}"
            ))),
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::ResourcePack => "资源包",
            Self::Shader => "光影包",
            Self::Datapack => "数据包",
            Self::Mod => "模组",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceResource {
    pub id: String,
    pub instance_id: String,
    pub kind: InstanceResourceKind,
    pub display_name: String,
    /// 基准文件名（不含 `.disabled` 后缀）。
    pub file_name: String,
    pub relative_path: String,
    pub size: u64,
    pub sha256: String,
    pub enabled: bool,
    pub world_name: Option<String>,
    pub imported_at_unix_seconds: i64,
}

impl AppService {
    /// 列举实例世界：`.minecraft/saves/` 下含 `level.dat` 的目录名。
    pub fn list_instance_worlds(&self, instance_id: &str) -> Result<Vec<String>> {
        let instance = self.ready_instance(instance_id)?;
        let saves = Path::new(&instance.root_directory)
            .join(".minecraft")
            .join("saves");
        if !saves.exists() {
            return Ok(Vec::new());
        }
        let metadata = fs::symlink_metadata(&saves)?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(CoreError::Content("实例 saves 目录不安全".to_owned()));
        }
        let mut worlds = Vec::new();
        for child in fs::read_dir(&saves)? {
            let child = child?;
            let metadata = fs::symlink_metadata(child.path())?;
            if !metadata.is_dir() || metadata.file_type().is_symlink() {
                continue;
            }
            if !child.path().join("level.dat").is_file() {
                continue;
            }
            let name = child
                .file_name()
                .into_string()
                .map_err(|_| CoreError::Content("世界目录名包含无效字符".to_owned()))?;
            worlds.push(name);
        }
        worlds.sort();
        Ok(worlds)
    }

    pub fn list_instance_resources(
        &self,
        instance_id: &str,
        kind: Option<InstanceResourceKind>,
    ) -> Result<Vec<InstanceResource>> {
        let connection = self.connection()?;
        let kind_filter = kind.map(InstanceResourceKind::database_value);
        let mut statement = connection.prepare(
            "
            SELECT id, instance_id, kind, display_name, file_name, relative_path,
                   size, sha256, enabled, world_name, imported_at_unix_seconds
            FROM instance_resources
            WHERE instance_id = ?1 AND (?2 IS NULL OR kind = ?2)
            ORDER BY kind, display_name COLLATE NOCASE, id
            ",
        )?;
        let rows = statement
            .query_map(params![instance_id, kind_filter], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, i64>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, bool>(8)?,
                    row.get::<_, Option<String>>(9)?,
                    row.get::<_, i64>(10)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        rows.into_iter()
            .map(
                |(
                    id,
                    instance_id,
                    kind,
                    display_name,
                    file_name,
                    relative_path,
                    size,
                    sha256,
                    enabled,
                    world_name,
                    imported_at_unix_seconds,
                )| {
                    Ok(InstanceResource {
                        id,
                        instance_id,
                        kind: InstanceResourceKind::from_database(&kind)?,
                        display_name,
                        file_name,
                        relative_path,
                        size: resource_sqlite_unsigned(size, "资源大小")?,
                        sha256,
                        enabled,
                        world_name,
                        imported_at_unix_seconds,
                    })
                },
            )
            .collect()
    }

    /// 把本地资源文件事务化导入实例。同名拒绝且不覆盖；失败不留半成品。
    pub fn import_instance_resource(
        &self,
        instance_id: &str,
        kind: InstanceResourceKind,
        source_path: &Path,
        world_name: Option<&str>,
    ) -> Result<InstanceResource> {
        let instance = self.ready_instance(instance_id)?;
        let world_name = match (kind, world_name) {
            (InstanceResourceKind::Datapack, Some(world)) => {
                let worlds = self.list_instance_worlds(instance_id)?;
                if !worlds.iter().any(|candidate| candidate == world) {
                    return Err(CoreError::Content(format!(
                        "世界 {world} 不存在，数据包必须装入用户选择的世界"
                    )));
                }
                Some(world.to_owned())
            }
            (InstanceResourceKind::Datapack, None) => {
                return Err(CoreError::Content(
                    "导入数据包必须先选择目标世界".to_owned(),
                ));
            }
            (_, Some(_)) => {
                return Err(CoreError::Content("只有数据包可以选择目标世界".to_owned()));
            }
            (_, None) => None,
        };
        let metadata = fs::symlink_metadata(source_path).map_err(|_| {
            CoreError::Content(format!("找不到要导入的文件：{}", source_path.display()))
        })?;
        if !metadata.is_file() {
            return Err(CoreError::Content("导入来源必须是文件".to_owned()));
        }
        if metadata.len() == 0 || metadata.len() > MAX_RESOURCE_BYTES {
            return Err(CoreError::Content(format!(
                "{}大小必须在 1 字节到 512 MiB 之间",
                kind.label()
            )));
        }
        let file_name = source_path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| CoreError::Content("导入文件名包含无效字符".to_owned()))?
            .to_owned();
        validate_resource_filename(&file_name, kind)?;
        let sha256 = resource_file_sha256(source_path)?;
        let instance_root = Path::new(&instance.root_directory);
        let target_directory =
            resource_target_directory(instance_root, kind, world_name.as_deref());
        fs::create_dir_all(&target_directory)?;
        let target = target_directory.join(&file_name);
        let disabled_target = target_directory.join(format!("{file_name}{DISABLED_SUFFIX}"));
        if target.exists() || disabled_target.exists() {
            return Err(CoreError::Content(format!(
                "同名文件 {file_name} 已存在，已拒绝导入且未覆盖"
            )));
        }

        let resource_id = Uuid::new_v4().to_string();
        let staging_directory = self
            .selected_data_directory()?
            .join(".staging")
            .join("resources")
            .join(&resource_id);
        fs::create_dir_all(&staging_directory)?;
        let staged = staging_directory.join(&file_name);
        if let Err(error) = fs::copy(source_path, &staged) {
            let _ = fs::remove_dir_all(&staging_directory);
            return Err(error.into());
        }
        // 复制后再次校验目标，缩小同名竞争窗口。
        if target.exists() || disabled_target.exists() {
            let _ = fs::remove_dir_all(&staging_directory);
            return Err(CoreError::Content(format!(
                "同名文件 {file_name} 已存在，已拒绝导入且未覆盖"
            )));
        }
        if let Err(error) = fs::rename(&staged, &target) {
            let _ = fs::remove_dir_all(&staging_directory);
            return Err(CoreError::Content(format!(
                "无法把{}放入实例目录：{error}",
                kind.label()
            )));
        }
        let relative_path = target
            .strip_prefix(instance_root)
            .map_err(|_| CoreError::Content("资源目标路径不在实例内".to_owned()))?
            .to_string_lossy()
            .replace('\\', "/");
        let resource = InstanceResource {
            id: resource_id,
            instance_id: instance.id.clone(),
            kind,
            display_name: display_name(&file_name),
            file_name: file_name.clone(),
            relative_path,
            size: metadata.len(),
            sha256,
            enabled: true,
            world_name,
            imported_at_unix_seconds: unix_timestamp(),
        };
        if let Err(error) = self.insert_instance_resource(&resource) {
            // 补偿：索引失败时移除已落位文件，回到导入前状态。
            let _ = fs::remove_file(&target);
            let _ = fs::remove_dir_all(&staging_directory);
            return Err(CoreError::Content(format!(
                "资源索引写入失败，已回滚文件：{error}"
            )));
        }
        let _ = fs::remove_dir_all(&staging_directory);
        Ok(resource)
    }

    /// 通过 `.disabled` 后缀切换启用状态；数据库失败时 rename 回滚。
    pub fn set_instance_resource_enabled(
        &self,
        resource_id: &str,
        enabled: bool,
    ) -> Result<InstanceResource> {
        let mut resource = self
            .instance_resource(resource_id)?
            .ok_or_else(|| CoreError::Content("资源项不存在".to_owned()))?;
        if resource.enabled == enabled {
            return Ok(resource);
        }
        let instance = self.ready_instance(&resource.instance_id)?;
        let target_directory = resource_target_directory(
            Path::new(&instance.root_directory),
            resource.kind,
            resource.world_name.as_deref(),
        );
        let current = target_directory.join(disk_file_name(&resource.file_name, resource.enabled));
        let next = target_directory.join(disk_file_name(&resource.file_name, enabled));
        if !current.exists() {
            return Err(CoreError::Content(format!(
                "资源文件 {} 不在预期位置，可能被外部移动或删除",
                resource.file_name
            )));
        }
        if next.exists() {
            return Err(CoreError::Content(format!(
                "目标位置已存在 {}，为避免覆盖已停止切换",
                disk_file_name(&resource.file_name, enabled)
            )));
        }
        fs::rename(&current, &next)?;
        if let Err(error) = self.update_instance_resource_enabled(resource_id, enabled) {
            let _ = fs::rename(&next, &current);
            return Err(CoreError::Content(format!(
                "启用状态索引更新失败，已回滚文件：{error}"
            )));
        }
        resource.enabled = enabled;
        Ok(resource)
    }

    pub(crate) fn ready_instance(&self, instance_id: &str) -> Result<ManagedInstanceSummary> {
        let instance = self
            .list_instances()?
            .into_iter()
            .find(|instance| instance.id == instance_id)
            .ok_or_else(|| CoreError::Content("目标实例不存在".to_owned()))?;
        if instance.state != "ready" {
            return Err(CoreError::Content(format!(
                "实例 {} 当前状态不是 ready",
                instance.name
            )));
        }
        Ok(instance)
    }

    fn instance_resource(&self, resource_id: &str) -> Result<Option<InstanceResource>> {
        Ok(self
            .list_instance_resources_by_id(resource_id)?
            .into_iter()
            .next())
    }

    pub(crate) fn list_instance_resources_by_id(
        &self,
        resource_id: &str,
    ) -> Result<Vec<InstanceResource>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "
            SELECT id, instance_id, kind, display_name, file_name, relative_path,
                   size, sha256, enabled, world_name, imported_at_unix_seconds
            FROM instance_resources
            WHERE id = ?1
            ",
        )?;
        let rows = statement
            .query_map(params![resource_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, i64>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, bool>(8)?,
                    row.get::<_, Option<String>>(9)?,
                    row.get::<_, i64>(10)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        rows.into_iter()
            .map(
                |(
                    id,
                    instance_id,
                    kind,
                    display_name,
                    file_name,
                    relative_path,
                    size,
                    sha256,
                    enabled,
                    world_name,
                    imported_at_unix_seconds,
                )| {
                    Ok(InstanceResource {
                        id,
                        instance_id,
                        kind: InstanceResourceKind::from_database(&kind)?,
                        display_name,
                        file_name,
                        relative_path,
                        size: resource_sqlite_unsigned(size, "资源大小")?,
                        sha256,
                        enabled,
                        world_name,
                        imported_at_unix_seconds,
                    })
                },
            )
            .collect()
    }

    fn insert_instance_resource(&self, resource: &InstanceResource) -> Result<()> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
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
                resource_sqlite_integer(resource.size, "资源大小")?,
                resource.sha256,
                resource.enabled,
                resource.world_name,
                resource.imported_at_unix_seconds,
            ],
        )?;
        transaction.commit()?;
        Ok(())
    }

    fn update_instance_resource_enabled(&self, resource_id: &str, enabled: bool) -> Result<()> {
        let changed = self.connection()?.execute(
            "UPDATE instance_resources SET enabled = ?2 WHERE id = ?1",
            params![resource_id, enabled],
        )?;
        if changed == 0 {
            return Err(CoreError::Content("资源项不存在".to_owned()));
        }
        Ok(())
    }
}

fn resource_target_directory(
    instance_root: &Path,
    kind: InstanceResourceKind,
    world_name: Option<&str>,
) -> PathBuf {
    let minecraft = instance_root.join(".minecraft");
    match kind {
        InstanceResourceKind::ResourcePack => minecraft.join("resourcepacks"),
        InstanceResourceKind::Shader => minecraft.join("shaderpacks"),
        InstanceResourceKind::Datapack => minecraft
            .join("saves")
            .join(world_name.unwrap_or_default())
            .join("datapacks"),
        InstanceResourceKind::Mod => minecraft.join("mods"),
    }
}

fn disk_file_name(base: &str, enabled: bool) -> String {
    if enabled {
        base.to_owned()
    } else {
        format!("{base}{DISABLED_SUFFIX}")
    }
}

fn display_name(file_name: &str) -> String {
    file_name
        .rsplit_once('.')
        .map_or(file_name, |(stem, _)| stem)
        .to_owned()
}

fn validate_resource_filename(filename: &str, kind: InstanceResourceKind) -> Result<()> {
    let invalid = filename.is_empty()
        || filename.len() > 240
        || filename.chars().any(char::is_control)
        || filename.chars().any(|character| {
            matches!(
                character,
                '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|'
            )
        })
        || filename.ends_with([' ', '.']);
    let lowered = filename.to_ascii_lowercase();
    let extension_ok = lowered.ends_with(".zip") || lowered.ends_with(".jar");
    if invalid || !extension_ok {
        return Err(CoreError::Content(format!(
            "{}文件名不安全或不是 ZIP/JAR：{filename}",
            kind.label()
        )));
    }
    Ok(())
}

fn resource_file_sha256(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path)?;
    let mut buffer = vec![0_u8; 128 * 1024];
    let mut hasher = Sha256::new();
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(encode_hex(hasher.finalize()))
}

fn encode_hex(bytes: impl AsRef<[u8]>) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let bytes = bytes.as_ref();
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}

fn resource_sqlite_integer(value: u64, label: &str) -> Result<i64> {
    i64::try_from(value).map_err(|_| CoreError::Content(format!("{label}超出可存储范围")))
}

fn resource_sqlite_unsigned(value: i64, label: &str) -> Result<u64> {
    u64::try_from(value).map_err(|_| CoreError::InvalidStoredState(format!("{label}不能是负数")))
}
