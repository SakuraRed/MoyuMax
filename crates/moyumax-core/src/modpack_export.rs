//! 实例导出为 Modrinth mrpack（PCL 4.5 导出树的简化版）。
//!
//! 产物是单文件 .mrpack（ZIP）：`modrinth.index.json`（formatVersion 1）+
//! `overrides/`。已从 Modrinth 安装的内容（本地记录含 projectId/versionId/
//! sha1/sha512/大小）写成 `files` 引用，下载地址按 Modrinth CDN 稳定格式
//! 重建；拿不到引用信息的文件（手动导入的 Mod、资源包、光影、配置等）诚实
//! 降级为 overrides 里的文件本体。相对 PCL 砍掉的：CurseForge 格式、树形
//! glob 规则、打包资源文件（离线下载缓存）、Modrinth 上传模式、禁用状态
//! 保留（mrpack 无此概念，禁用内容按普通引用导出）。

use std::{
    collections::{BTreeMap, HashSet},
    fs,
    path::{Component, Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

use crate::{AppService, CoreError, Result, validate_pack_relative_path};

const MODRINTH_CDN_BASE: &str = "https://cdn.modrinth.com/data";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportModpackOptions {
    pub name: String,
    pub version: String,
    pub include_config: bool,
    pub include_resource_packs: bool,
    pub include_shaders: bool,
    pub include_servers: bool,
    pub include_screenshots: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportModpackReport {
    pub instance_id: String,
    pub pack_name: String,
    pub pack_version: String,
    pub output_path: String,
    pub total_bytes: u64,
    /// 写入 files 引用的内容数（安装时按 URL 重新下载）。
    pub referenced_files: u64,
    /// 打入 overrides 的文件本体数。
    pub bundled_files: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MrIndex {
    format_version: u16,
    game: &'static str,
    version_id: String,
    name: String,
    files: Vec<MrFile>,
    dependencies: BTreeMap<String, String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MrFile {
    path: String,
    hashes: MrHashes,
    env: MrEnv,
    downloads: Vec<String>,
    file_size: u64,
}

#[derive(Serialize)]
struct MrHashes {
    sha1: String,
    sha512: String,
}

#[derive(Serialize)]
struct MrEnv {
    client: &'static str,
    server: &'static str,
}

impl AppService {
    /// 把实例内容导出为 Modrinth mrpack；先写暂存再原子 rename 到目标位置。
    pub fn export_instance_modpack(
        &self,
        instance_id: &str,
        destination: &Path,
        options: &ExportModpackOptions,
    ) -> Result<ExportModpackReport> {
        let instance = self.ready_instance(instance_id)?;
        let name = options.name.trim();
        if name.is_empty() || name.chars().any(char::is_control) {
            return Err(CoreError::Content("整合包名不能为空".to_owned()));
        }
        let version = options.version.trim();
        if version.is_empty() || version.chars().any(char::is_control) {
            return Err(CoreError::Content("整合包版本号不能为空".to_owned()));
        }
        if destination.extension().and_then(|value| value.to_str()) != Some("mrpack") {
            return Err(CoreError::Content("导出目标必须是 .mrpack 文件".to_owned()));
        }
        let parent = destination
            .parent()
            .ok_or_else(|| CoreError::Content("导出目标路径无效".to_owned()))?;
        if !parent.is_dir() {
            return Err(CoreError::Content("导出目标所在目录不存在".to_owned()));
        }
        let instance_root = PathBuf::from(&instance.root_directory);
        if destination.starts_with(&instance_root) {
            return Err(CoreError::Content(
                "不能把整合包导出到实例目录内部".to_owned(),
            ));
        }
        // 闭环要求：自家导入必须能读回，而导入解析要求声明受支持的加载器。
        let loader_dependency = match (instance.loader_kind.as_str(), &instance.loader_version) {
            ("fabric", Some(version)) => ("fabric-loader", version.clone()),
            ("quilt", Some(version)) => ("quilt-loader", version.clone()),
            ("forge", Some(version)) => ("forge", version.clone()),
            ("neoforge", Some(version)) => ("neoforge", version.clone()),
            _ => {
                return Err(CoreError::Content(
                    "仅支持导出带 Fabric/Quilt/Forge/NeoForge 加载器的实例".to_owned(),
                ));
            }
        };
        let minecraft_root = instance_root.join(".minecraft");

        // files 引用：本地记录齐全（projectId/versionId/sha1/sha512/大小）的
        // Modrinth 内容；磁盘文件已缺失的陈旧记录跳过。
        let mut files = Vec::new();
        let mut referenced_paths = HashSet::new();
        for content in self.list_installed_content(instance_id)? {
            let Some(relative) = content.relative_path.strip_prefix(".minecraft/") else {
                continue;
            };
            if !minecraft_root.join(relative).is_file() {
                continue;
            }
            let path = validate_pack_relative_path(relative)?;
            referenced_paths.insert(path.clone());
            files.push(MrFile {
                path,
                hashes: MrHashes {
                    sha1: content.sha1,
                    sha512: content.sha512,
                },
                env: MrEnv {
                    client: "required",
                    server: "optional",
                },
                downloads: vec![format!(
                    "{MODRINTH_CDN_BASE}/{}/versions/{}/{}",
                    content.project_id, content.version_id, content.file_name
                )],
                file_size: content.size,
            });
        }
        files.sort_by(|left, right| left.path.cmp(&right.path));

        // overrides：mods 里未被引用的本体 + 按选项勾选的目录与文件。
        let mut overrides: Vec<(PathBuf, String)> = Vec::new();
        collect_directory_overrides(
            &minecraft_root,
            &minecraft_root.join("mods"),
            &referenced_paths,
            &mut overrides,
        )?;
        if options.include_config {
            collect_directory_overrides(
                &minecraft_root,
                &minecraft_root.join("config"),
                &referenced_paths,
                &mut overrides,
            )?;
            collect_file_override(&minecraft_root, "options.txt", &mut overrides)?;
        }
        if options.include_resource_packs {
            collect_directory_overrides(
                &minecraft_root,
                &minecraft_root.join("resourcepacks"),
                &referenced_paths,
                &mut overrides,
            )?;
        }
        if options.include_shaders {
            collect_directory_overrides(
                &minecraft_root,
                &minecraft_root.join("shaderpacks"),
                &referenced_paths,
                &mut overrides,
            )?;
        }
        if options.include_servers {
            collect_file_override(&minecraft_root, "servers.dat", &mut overrides)?;
        }
        if options.include_screenshots {
            collect_directory_overrides(
                &minecraft_root,
                &minecraft_root.join("screenshots"),
                &referenced_paths,
                &mut overrides,
            )?;
        }
        overrides.sort_by(|left, right| left.1.cmp(&right.1));
        let bundled_files = overrides.len() as u64;

        let mut dependencies = BTreeMap::new();
        dependencies.insert("minecraft".to_owned(), instance.game_version.clone());
        dependencies.insert(loader_dependency.0.to_owned(), loader_dependency.1);
        let index = MrIndex {
            format_version: 1,
            game: "minecraft",
            version_id: version.to_owned(),
            name: name.to_owned(),
            files,
            dependencies,
        };
        let referenced_files = index.files.len() as u64;

        let partial = parent.join(format!(
            ".{}.moyu-partial",
            destination
                .file_name()
                .and_then(|value| value.to_str())
                .ok_or_else(|| CoreError::Content("导出目标路径无效".to_owned()))?
        ));
        let write_result = write_mrpack(&partial, &index, &overrides);
        match write_result {
            Ok(()) => {
                fs::rename(&partial, destination)?;
                let total_bytes = fs::metadata(destination)?.len();
                Ok(ExportModpackReport {
                    instance_id: instance_id.to_owned(),
                    pack_name: name.to_owned(),
                    pack_version: version.to_owned(),
                    output_path: destination.to_string_lossy().into_owned(),
                    total_bytes,
                    referenced_files,
                    bundled_files,
                })
            }
            Err(error) => {
                let _ = fs::remove_file(&partial);
                Err(error)
            }
        }
    }
}

/// 递归收集目录内文件为 overrides 条目；跳过已被引用的路径，拒绝符号链接。
fn collect_directory_overrides(
    minecraft_root: &Path,
    directory: &Path,
    referenced_paths: &HashSet<String>,
    overrides: &mut Vec<(PathBuf, String)>,
) -> Result<()> {
    if !directory.exists() {
        return Ok(());
    }
    let mut children = fs::read_dir(directory)?.collect::<std::io::Result<Vec<_>>>()?;
    children.sort_by_key(std::fs::DirEntry::path);
    for child in children {
        let metadata = fs::symlink_metadata(child.path())?;
        if metadata.file_type().is_symlink() {
            return Err(CoreError::Content(format!(
                "实例目录包含链接，已拒绝打包：{}",
                child.path().display()
            )));
        }
        if metadata.is_dir() {
            collect_directory_overrides(
                minecraft_root,
                &child.path(),
                referenced_paths,
                overrides,
            )?;
            continue;
        }
        if !metadata.is_file() {
            continue;
        }
        let child_path = child.path();
        let relative = child_path
            .strip_prefix(minecraft_root)
            .map_err(|_| CoreError::Content("导出文件越过实例根".to_owned()))?;
        let relative = pack_relative_from_components(relative)?;
        if referenced_paths.contains(&relative) {
            continue;
        }
        overrides.push((child_path, relative));
    }
    Ok(())
}

/// 收集单个文件（存在时）为 overrides 条目。
fn collect_file_override(
    minecraft_root: &Path,
    relative: &str,
    overrides: &mut Vec<(PathBuf, String)>,
) -> Result<()> {
    let source = minecraft_root.join(relative);
    if !source.is_file() {
        return Ok(());
    }
    let relative = validate_pack_relative_path(relative)?;
    overrides.push((source, relative));
    Ok(())
}

/// 把磁盘相对路径转成 mrpack 相对路径（仅普通组件、正斜杠）。
fn pack_relative_from_components(relative: &Path) -> Result<String> {
    let mut components = Vec::new();
    for component in relative.components() {
        let Component::Normal(value) = component else {
            return Err(CoreError::Content("导出文件包含不安全路径".to_owned()));
        };
        components.push(value.to_string_lossy().into_owned());
    }
    validate_pack_relative_path(&components.join("/"))
}

fn write_mrpack(
    destination: &Path,
    index: &MrIndex,
    overrides: &[(PathBuf, String)],
) -> Result<()> {
    let file = fs::File::create(destination)?;
    let mut writer = ZipWriter::new(file);
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .unix_permissions(0o600);
    writer
        .start_file("modrinth.index.json", options)
        .map_err(zip_write_error)?;
    std::io::Write::write_all(&mut writer, &serde_json::to_vec_pretty(index)?)?;
    for (source, relative) in overrides {
        writer
            .start_file(format!("overrides/{relative}"), options)
            .map_err(zip_write_error)?;
        let mut input = fs::File::open(source)?;
        std::io::copy(&mut input, &mut writer)?;
    }
    writer.finish().map_err(zip_write_error)?;
    Ok(())
}

fn zip_write_error(error: zip::result::ZipError) -> CoreError {
    CoreError::Archive(format!("无法写出整合包：{error}"))
}
