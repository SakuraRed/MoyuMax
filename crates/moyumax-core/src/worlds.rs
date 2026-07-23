//! 世界存档管理：清单、导入导出与备份回滚。
//!
//! 世界以 `.minecraft/saves/` 下的目录为单位。导入导出走 ZIP；回滚先创建
//! 恢复点备份，再把备份解压到实例内暂存并原子交换 saves 目录，任何失败
//! 都回到回滚前状态，重启后收敛未完成的交换。

use std::{
    fs,
    io::Write,
    path::{Component, Path, PathBuf},
    time::UNIX_EPOCH,
};

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

use crate::{
    AppService, BackupTrigger, CoreError, ManagedInstanceSummary, Result, WorldBackupSummary,
};

const ROLLBACK_STAGING: &str = "rollback-staging";
const ROLLBACK_OLD_SAVES: &str = "rollback-old-saves";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceWorldInfo {
    pub name: String,
    pub size_bytes: u64,
    pub last_played_unix_seconds: Option<i64>,
}

impl AppService {
    /// 按实例列出世界清单（名称、占用、level.dat 最近修改时间）。
    pub fn list_instance_world_details(&self, instance_id: &str) -> Result<Vec<InstanceWorldInfo>> {
        let instance = self.ready_instance(instance_id)?;
        let saves = saves_directory(&instance);
        let mut worlds = Vec::new();
        for name in self.list_instance_worlds(instance_id)? {
            let world = saves.join(&name);
            worlds.push(InstanceWorldInfo {
                size_bytes: directory_size(&world)?,
                last_played_unix_seconds: level_dat_mtime(&world),
                name,
            });
        }
        Ok(worlds)
    }

    /// 把单个世界目录打包为 ZIP（`<世界名>/...` 布局）写到用户选择的位置。
    pub fn export_instance_world(
        &self,
        instance_id: &str,
        world_name: &str,
        destination: &Path,
    ) -> Result<u64> {
        let instance = self.ready_instance(instance_id)?;
        let world = self.existing_world(&instance, world_name)?;
        if destination.extension().and_then(|value| value.to_str()) != Some("zip") {
            return Err(CoreError::Backup("导出目标必须是 .zip 文件".to_owned()));
        }
        let parent = destination
            .parent()
            .ok_or_else(|| CoreError::Backup("导出目标路径无效".to_owned()))?;
        if !parent.is_dir() {
            return Err(CoreError::Backup("导出目标所在目录不存在".to_owned()));
        }
        if destination.starts_with(&world) {
            return Err(CoreError::Backup("不能把世界导出到它自身内部".to_owned()));
        }
        let partial = parent.join(format!(
            ".{}.moyu-partial",
            destination
                .file_name()
                .and_then(|value| value.to_str())
                .ok_or_else(|| CoreError::Backup("导出目标路径无效".to_owned()))?
        ));
        let write_result = write_world_zip(&partial, &world, world_name);
        match write_result {
            Ok(()) => {
                fs::rename(&partial, destination)?;
                Ok(fs::metadata(destination)?.len())
            }
            Err(error) => {
                let _ = fs::remove_file(&partial);
                Err(error)
            }
        }
    }

    /// 从 ZIP 导入世界：识别根级 level.dat 或单一顶层目录布局，同名拒绝。
    pub fn import_instance_world(
        &self,
        instance_id: &str,
        source_zip: &Path,
    ) -> Result<InstanceWorldInfo> {
        let instance = self.ready_instance(instance_id)?;
        if !source_zip.is_file() {
            return Err(CoreError::Backup(format!(
                "找不到要导入的世界 ZIP：{}",
                source_zip.display()
            )));
        }
        let file = fs::File::open(source_zip)?;
        let mut archive = ZipArchive::new(file)
            .map_err(|error| CoreError::Backup(format!("世界 ZIP 无法读取：{error}")))?;
        let layout = detect_world_layout(&mut archive)?;
        let world_name = match &layout {
            WorldLayout::Root => source_zip
                .file_stem()
                .and_then(|value| value.to_str())
                .ok_or_else(|| CoreError::Backup("无法从 ZIP 文件名推断世界名".to_owned()))?
                .to_owned(),
            WorldLayout::SingleDirectory(name) => name.clone(),
        };
        validate_world_name(&world_name)?;
        let saves = saves_directory(&instance);
        let target = saves.join(&world_name);
        if target.exists() {
            return Err(CoreError::Backup(format!(
                "世界 {world_name} 已存在，已拒绝导入且未覆盖"
            )));
        }
        let staging_root = self
            .selected_data_directory()?
            .join(".staging")
            .join("worlds")
            .join(Uuid::new_v4().to_string());
        let staged_world = staging_root.join(&world_name);
        fs::create_dir_all(&staged_world)?;
        if let Err(error) = extract_world_zip(&mut archive, &layout, &staged_world) {
            let _ = fs::remove_dir_all(&staging_root);
            return Err(error);
        }
        if !staged_world.join("level.dat").is_file() {
            let _ = fs::remove_dir_all(&staging_root);
            return Err(CoreError::Backup("ZIP 中没有可用的 level.dat".to_owned()));
        }
        fs::create_dir_all(&saves)?;
        if let Err(error) = fs::rename(&staged_world, &target) {
            let _ = fs::remove_dir_all(&staging_root);
            return Err(CoreError::Backup(format!("无法把世界放入 saves：{error}")));
        }
        let _ = fs::remove_dir_all(&staging_root);
        Ok(InstanceWorldInfo {
            size_bytes: directory_size(&target)?,
            last_played_unix_seconds: level_dat_mtime(&target),
            name: world_name,
        })
    }

    /// 回滚到已完成备份：先创建恢复点备份，再原子交换 saves。
    pub fn rollback_world_backup(&self, backup_id: &str) -> Result<WorldBackupSummary> {
        let backup = self
            .list_world_backups(None)?
            .into_iter()
            .find(|candidate| candidate.id == backup_id)
            .ok_or_else(|| CoreError::Backup("备份不存在".to_owned()))?;
        if backup.state != crate::BackupState::Ready {
            return Err(CoreError::Backup("只有已完成的备份可以回滚".to_owned()));
        }
        let archive_path = backup
            .archive_path
            .as_deref()
            .ok_or_else(|| CoreError::Backup("备份缺少归档路径".to_owned()))?;
        if !Path::new(archive_path).is_file() {
            return Err(CoreError::Backup("备份归档文件已丢失，无法回滚".to_owned()));
        }
        let instance = self.ready_instance(&backup.instance_id)?;
        // 恢复点失败必须中止回滚，saves 保持不变。
        let recovery_point =
            self.create_world_backup(&backup.instance_id, BackupTrigger::Manual, None)?;
        if recovery_point.state != crate::BackupState::Ready
            && recovery_point.state != crate::BackupState::Skipped
        {
            return Err(CoreError::Backup(
                "恢复点备份未能完成，已中止回滚".to_owned(),
            ));
        }

        let instance_root = Path::new(&instance.root_directory);
        let staging = instance_root.join(".moyumax").join(ROLLBACK_STAGING);
        if staging.exists() {
            fs::remove_dir_all(&staging)?;
        }
        fs::create_dir_all(&staging)?;
        let chain = self.resolve_backup_chain(&backup)?;
        let chain_result = (|| -> Result<()> {
            for link in &chain {
                let link_path = link
                    .archive_path
                    .as_deref()
                    .ok_or_else(|| CoreError::Backup("恢复链中的备份缺少归档路径".to_owned()))?;
                let archive_file = fs::File::open(link_path)?;
                let mut archive = ZipArchive::new(archive_file)
                    .map_err(|error| CoreError::Backup(format!("备份归档无法读取：{error}")))?;
                extract_backup_worlds(&mut archive, &staging)?;
                if link.kind == crate::BackupKind::Incremental {
                    let manifest = crate::read_backup_manifest(Path::new(link_path))?;
                    apply_manifest_deletions(&staging, &manifest.deleted)?;
                }
            }
            Ok(())
        })();
        if let Err(error) = chain_result {
            let _ = fs::remove_dir_all(&staging);
            return Err(error);
        }

        let saves = saves_directory(&instance);
        let old_saves = instance_root.join(".moyumax").join(ROLLBACK_OLD_SAVES);
        if old_saves.exists() {
            fs::remove_dir_all(&old_saves)?;
        }
        let had_saves = saves.exists();
        if had_saves {
            fs::rename(&saves, &old_saves)?;
        }
        if let Err(error) = fs::rename(&staging, &saves) {
            // 交换失败：把原 saves 放回去。
            if had_saves {
                let _ = fs::rename(&old_saves, &saves);
            }
            return Err(CoreError::Backup(format!(
                "回滚交换失败，已恢复原状：{error}"
            )));
        }
        if old_saves.exists() {
            fs::remove_dir_all(&old_saves)?;
        }
        Ok(recovery_point)
    }

    /// 解析回滚恢复链：从目标沿 base_backup_id 回到最近的全量备份。
    fn resolve_backup_chain(&self, target: &WorldBackupSummary) -> Result<Vec<WorldBackupSummary>> {
        let all = self.list_world_backups(Some(&target.instance_id))?;
        let mut chain = vec![target.clone()];
        let mut current = target.clone();
        while current.kind == crate::BackupKind::Incremental {
            let base_id = current
                .base_backup_id
                .as_deref()
                .ok_or_else(|| CoreError::Backup("增量备份缺少基准，恢复链断裂".to_owned()))?;
            let base = all
                .iter()
                .find(|candidate| candidate.id == base_id)
                .ok_or_else(|| {
                    CoreError::Backup("恢复链基准备份缺失，无法回滚到该时间点".to_owned())
                })?;
            if base.state != crate::BackupState::Ready {
                return Err(CoreError::Backup("恢复链基准备份不可用".to_owned()));
            }
            chain.push(base.clone());
            current = base.clone();
            if chain.len() > 128 {
                return Err(CoreError::Backup("恢复链过长，已中止回滚".to_owned()));
            }
        }
        chain.reverse();
        Ok(chain)
    }

    /// 重启收敛：完成或撤销中断的回滚交换，saves 必须完整可用。
    pub(crate) fn recover_interrupted_world_rollbacks(&self) -> Result<()> {
        for instance in self.list_instances()? {
            let saves = saves_directory(&instance);
            let staging = Path::new(&instance.root_directory)
                .join(".moyumax")
                .join(ROLLBACK_STAGING);
            let old_saves = Path::new(&instance.root_directory)
                .join(".moyumax")
                .join(ROLLBACK_OLD_SAVES);
            if !saves.exists() {
                if old_saves.exists() {
                    // 交换中段中断：原 saves 完整保留，优先恢复。
                    fs::rename(&old_saves, &saves)?;
                    if staging.exists() {
                        let _ = fs::remove_dir_all(&staging);
                    }
                } else if staging.exists() {
                    fs::rename(&staging, &saves)?;
                }
                continue;
            }
            if old_saves.exists() {
                let _ = fs::remove_dir_all(&old_saves);
            }
            if staging.exists() {
                let _ = fs::remove_dir_all(&staging);
            }
        }
        Ok(())
    }

    fn existing_world(
        &self,
        instance: &ManagedInstanceSummary,
        world_name: &str,
    ) -> Result<PathBuf> {
        validate_world_name(world_name)?;
        let world = saves_directory(instance).join(world_name);
        if !world.is_dir() || !world.join("level.dat").is_file() {
            return Err(CoreError::Backup(format!("世界 {world_name} 不存在")));
        }
        Ok(world)
    }
}

#[derive(Debug)]
enum WorldLayout {
    Root,
    SingleDirectory(String),
}

fn saves_directory(instance: &ManagedInstanceSummary) -> PathBuf {
    Path::new(&instance.root_directory)
        .join(".minecraft")
        .join("saves")
}

fn validate_world_name(name: &str) -> Result<()> {
    let invalid = name.is_empty()
        || name.len() > 120
        || name.chars().any(char::is_control)
        || name.chars().any(|character| {
            matches!(
                character,
                '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|'
            )
        })
        || name.ends_with([' ', '.']);
    if invalid {
        return Err(CoreError::Backup(format!("世界名不安全：{name}")));
    }
    Ok(())
}

fn detect_world_layout(archive: &mut ZipArchive<fs::File>) -> Result<WorldLayout> {
    let mut root_level_dat = false;
    let mut top_directories = std::collections::HashSet::new();
    let mut dir_level_dat: Option<String> = None;
    for index in 0..archive.len() {
        let entry = archive.by_index(index).map_err(zip_read_error)?;
        let Some(path) = entry.enclosed_name() else {
            return Err(CoreError::Backup("世界 ZIP 包含不安全路径条目".to_owned()));
        };
        let mut components = path.components();
        let Some(Component::Normal(first)) = components.next() else {
            continue;
        };
        let first = first.to_string_lossy().to_string();
        match components.next() {
            None => {
                if first.eq_ignore_ascii_case("level.dat") {
                    root_level_dat = true;
                }
            }
            Some(Component::Normal(second)) => {
                top_directories.insert(first.clone());
                if second.eq_ignore_ascii_case("level.dat") && components.next().is_none() {
                    dir_level_dat = Some(first);
                }
            }
            _ => {}
        }
    }
    if root_level_dat {
        return Ok(WorldLayout::Root);
    }
    if top_directories.len() == 1
        && let Some(name) = dir_level_dat
    {
        return Ok(WorldLayout::SingleDirectory(name));
    }
    Err(CoreError::Backup(
        "世界 ZIP 布局无效：需要根级 level.dat 或单一世界目录".to_owned(),
    ))
}

fn extract_world_zip(
    archive: &mut ZipArchive<fs::File>,
    layout: &WorldLayout,
    destination: &Path,
) -> Result<()> {
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(zip_read_error)?;
        let Some(path) = entry.enclosed_name() else {
            return Err(CoreError::Backup("世界 ZIP 包含不安全路径条目".to_owned()));
        };
        let relative = match layout {
            WorldLayout::Root => path.clone(),
            WorldLayout::SingleDirectory(name) => path
                .strip_prefix(name)
                .map_err(|_| CoreError::Backup("世界 ZIP 条目越过世界目录".to_owned()))?
                .to_path_buf(),
        };
        if relative.as_os_str().is_empty() {
            continue;
        }
        let target = destination.join(&relative);
        if entry.is_dir() {
            fs::create_dir_all(&target)?;
            continue;
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut output = fs::File::create(&target)?;
        std::io::copy(&mut entry, &mut output)?;
        output.flush()?;
    }
    Ok(())
}

fn extract_backup_worlds(archive: &mut ZipArchive<fs::File>, staging: &Path) -> Result<()> {
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(zip_read_error)?;
        let Some(path) = entry.enclosed_name() else {
            return Err(CoreError::Backup("备份归档包含不安全路径条目".to_owned()));
        };
        let Ok(relative) = path.strip_prefix("worlds") else {
            continue;
        };
        if relative.as_os_str().is_empty() {
            continue;
        }
        let target = staging.join(relative);
        if entry.is_dir() {
            fs::create_dir_all(&target)?;
            continue;
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut output = fs::File::create(&target)?;
        std::io::copy(&mut entry, &mut output)?;
        output.flush()?;
    }
    Ok(())
}

/// 应用增量清单中的删除项（路径必须停留在暂存区内）。
fn apply_manifest_deletions(staging: &Path, deleted: &[String]) -> Result<()> {
    for path in deleted {
        let relative = Path::new(path);
        if relative
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
        {
            return Err(CoreError::Backup(format!(
                "增量清单包含不安全删除路径：{path}"
            )));
        }
        let target = staging.join(relative);
        if target.is_dir() {
            fs::remove_dir_all(&target)?;
        } else if target.exists() {
            fs::remove_file(&target)?;
        }
    }
    Ok(())
}

fn write_world_zip(destination: &Path, world: &Path, world_name: &str) -> Result<()> {
    let file = fs::File::create(destination)?;
    let mut writer = ZipWriter::new(file);
    let options = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o600);
    write_world_zip_entries(&mut writer, options, world, world, world_name)?;
    writer.finish().map_err(zip_read_error)?;
    Ok(())
}

fn write_world_zip_entries(
    writer: &mut ZipWriter<fs::File>,
    options: SimpleFileOptions,
    root: &Path,
    current: &Path,
    world_name: &str,
) -> Result<()> {
    let mut children = fs::read_dir(current)?.collect::<std::io::Result<Vec<_>>>()?;
    children.sort_by_key(std::fs::DirEntry::path);
    for child in children {
        let metadata = fs::symlink_metadata(child.path())?;
        if metadata.file_type().is_symlink() {
            return Err(CoreError::Backup("世界目录包含链接，已拒绝打包".to_owned()));
        }
        let child_path = child.path();
        let relative = child_path
            .strip_prefix(root)
            .map_err(|_| CoreError::Backup("世界文件越过存档根".to_owned()))?;
        let mut components = vec![world_name.to_owned()];
        for component in relative.components() {
            let Component::Normal(value) = component else {
                return Err(CoreError::Backup("世界文件包含不安全路径".to_owned()));
            };
            components.push(value.to_string_lossy().into_owned());
        }
        if metadata.is_dir() {
            writer
                .add_directory(format!("{}/", components.join("/")), options)
                .map_err(zip_read_error)?;
            write_world_zip_entries(writer, options, root, &child.path(), world_name)?;
        } else {
            writer
                .start_file(components.join("/"), options)
                .map_err(zip_read_error)?;
            let mut source = fs::File::open(child.path())?;
            std::io::copy(&mut source, writer)?;
        }
    }
    Ok(())
}

fn directory_size(root: &Path) -> Result<u64> {
    let mut total = 0_u64;
    if !root.exists() {
        return Ok(0);
    }
    for child in fs::read_dir(root)? {
        let child = child?;
        let metadata = fs::symlink_metadata(child.path())?;
        if metadata.is_dir() && !metadata.file_type().is_symlink() {
            total = total.saturating_add(directory_size(&child.path())?);
        } else if metadata.is_file() {
            total = total.saturating_add(metadata.len());
        }
    }
    Ok(total)
}

fn level_dat_mtime(world: &Path) -> Option<i64> {
    let metadata = fs::metadata(world.join("level.dat")).ok()?;
    let modified = metadata.modified().ok()?;
    let seconds = modified.duration_since(UNIX_EPOCH).ok()?.as_secs();
    i64::try_from(seconds).ok()
}

fn zip_read_error(error: zip::result::ZipError) -> CoreError {
    CoreError::Backup(format!("ZIP 读写失败：{error}"))
}
