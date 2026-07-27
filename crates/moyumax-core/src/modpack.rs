//! Modrinth 与 CurseForge 整合包安装与更新。
//!
//! 只走声明式解析与文件下载：.mrpack 的 `modrinth.index.json` 与 CF 的
//! `manifest.json` 都是纯数据，绝不执行包内脚本。下载经统一镜像策略与哈希
//! 校验，提交带日志与补偿回滚；更新按受管清单做文件级差异，用户改动过的
//! 文件保留并汇总提示。

use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Component, Path, PathBuf},
    time::Duration,
};

use futures_util::StreamExt;
use reqwest::{Client, Url};
use rusqlite::{OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha512};
use uuid::Uuid;
use zip::ZipArchive;

use crate::{
    AppService, ArtifactKind, CoreError, DownloadCandidate, ResolvedArtifact, Result,
    SourceCandidates, candidates_for, unix_timestamp,
};

const MCI_MIRROR_BASE: &str = "https://mod.mcimirror.top";
const MAX_PACK_FILES: usize = 4096;
const MODPACK_JOURNAL_NAME: &str = "modpack-journal.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModpackProvider {
    Modrinth,
    Curseforge,
}

impl ModpackProvider {
    const fn database_value(self) -> &'static str {
        match self {
            Self::Modrinth => "modrinth",
            Self::Curseforge => "curseforge",
        }
    }

    fn from_database(value: &str) -> Result<Self> {
        match value {
            "modrinth" => Ok(Self::Modrinth),
            "curseforge" => Ok(Self::Curseforge),
            _ => Err(CoreError::InvalidStoredState(format!(
                "未知整合包来源：{value}"
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModpackFile {
    pub relative_path: String,
    pub url: Option<String>,
    /// 包内声明的全部下载地址(mrpack downloads 列表,含 forgecdn/modrinth 互为备用)。
    pub urls: Vec<String>,
    pub sha1: Option<String>,
    pub sha512: Option<String>,
    pub size: u64,
    pub cf_project_id: Option<u64>,
    pub cf_file_id: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModpackOverrideFile {
    pub relative_path: String,
    pub zip_entry: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModpackPlan {
    pub provider: ModpackProvider,
    pub name: String,
    pub version: String,
    pub game_version: String,
    pub loader_kind: String,
    pub loader_version: String,
    pub files: Vec<ModpackFile>,
    pub overrides: Vec<ModpackOverrideFile>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModpackPreview {
    pub provider: ModpackProvider,
    pub name: String,
    pub version: String,
    pub game_version: String,
    pub loader_kind: String,
    pub loader_version: String,
    pub file_count: u64,
    pub total_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledModpack {
    pub provider: ModpackProvider,
    pub pack_name: String,
    pub pack_version: String,
    pub game_version: String,
    pub loader_kind: String,
    pub managed_files: Vec<ManagedPackFile>,
    pub installed_at_unix_seconds: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagedPackFile {
    pub relative_path: String,
    pub sha512: String,
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModpackInstallReport {
    pub instance_id: String,
    pub pack_name: String,
    pub pack_version: String,
    pub installed_files: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModpackUpdateReport {
    pub pack_name: String,
    pub from_version: String,
    pub to_version: String,
    pub added_files: u64,
    pub replaced_files: u64,
    pub deleted_files: u64,
    pub kept_user_modified: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModpackJournal {
    schema_version: u16,
    operation: String,
    committed: Vec<String>,
    backups: Vec<(String, String)>,
}

#[derive(Clone)]
pub struct MciMirrorClient {
    client: Client,
    base_url: Url,
}

impl MciMirrorClient {
    pub fn new() -> Result<Self> {
        Self::with_base_url(MCI_MIRROR_BASE)
    }

    pub fn with_base_url(base_url: &str) -> Result<Self> {
        let mut base_url = Url::parse(base_url)
            .map_err(|error| CoreError::Content(format!("MCI Mirror 地址无效：{error}")))?;
        if !base_url.path().ends_with('/') {
            base_url.set_path(&format!("{}/", base_url.path()));
        }
        let localhost = matches!(base_url.host_str(), Some("127.0.0.1" | "localhost"));
        if base_url.scheme() != "https" && !(base_url.scheme() == "http" && localhost) {
            return Err(CoreError::Content("MCI Mirror 必须使用 https".to_owned()));
        }
        let user_agent = format!(
            "SakuraRed/MoyuMax/{} (github.com/SakuraRed/MoyuMax)",
            env!("CARGO_PKG_VERSION")
        );
        let client = crate::http_client_builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(20))
            .user_agent(user_agent)
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        Ok(Self { client, base_url })
    }

    /// 经 MCI Mirror 解析 CurseForge 文件下载信息。
    pub async fn curseforge_file(&self, project_id: u64, file_id: u64) -> Result<CurseForgeFile> {
        let url = self
            .base_url
            .join(&format!("curseforge/v1/mods/{project_id}/files/{file_id}"))
            .map_err(|error| CoreError::Content(format!("MCI 文件地址无效：{error}")))?;
        let response =
            self.client.get(url).send().await.map_err(|error| {
                CoreError::Content(format!("无法解析 CurseForge 文件：{error}"))
            })?;
        if !response.status().is_success() {
            return Err(CoreError::Content(format!(
                "MCI Mirror 返回 HTTP {}",
                response.status().as_u16()
            )));
        }
        let body: CurseForgeFileResponse = response.json().await?;
        let file = body.data;
        let sha1 = file
            .hashes
            .iter()
            .find(|hash| hash.algo == 1)
            .map(|hash| hash.value.clone())
            .ok_or_else(|| CoreError::Content("CurseForge 文件缺少 SHA-1".to_owned()))?;
        Ok(CurseForgeFile {
            url: file
                .download_url
                .ok_or_else(|| CoreError::Content("CurseForge 文件没有可用下载地址".to_owned()))?,
            file_name: file.file_name,
            size: file.file_length,
            sha1,
        })
    }
}

#[derive(Debug)]
pub struct CurseForgeFile {
    pub url: String,
    pub file_name: String,
    pub size: u64,
    pub sha1: String,
}

#[derive(Debug, Deserialize)]
struct CurseForgeFileResponse {
    data: CurseForgeFileData,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CurseForgeFileData {
    download_url: Option<String>,
    file_name: String,
    file_length: u64,
    hashes: Vec<CurseForgeFileHash>,
}

#[derive(Debug, Deserialize)]
struct CurseForgeFileHash {
    algo: u8,
    value: String,
}

/// 解析整合包压缩包为统一计划（自动识别 .mrpack 与 CF .zip）。
pub fn parse_modpack_archive(archive_path: &Path) -> Result<ModpackPlan> {
    let file = fs::File::open(archive_path)
        .map_err(|error| CoreError::Content(format!("无法打开整合包：{error}")))?;
    let mut archive = ZipArchive::new(file)
        .map_err(|error| CoreError::Content(format!("整合包不是有效的 ZIP：{error}")))?;
    let index_buffer = {
        match archive.by_name("modrinth.index.json") {
            Ok(mut entry) => {
                let mut buffer = Vec::new();
                std::io::Read::read_to_end(&mut entry, &mut buffer)?;
                Some(buffer)
            }
            Err(_) => None,
        }
    };
    if let Some(buffer) = index_buffer {
        return parse_modrinth_index(&buffer, &mut archive);
    }
    let manifest_buffer = {
        match archive.by_name("manifest.json") {
            Ok(mut entry) => {
                let mut buffer = Vec::new();
                std::io::Read::read_to_end(&mut entry, &mut buffer)?;
                Some(buffer)
            }
            Err(_) => None,
        }
    };
    if let Some(buffer) = manifest_buffer {
        return parse_curseforge_manifest(&buffer, &mut archive);
    }
    Err(CoreError::Content(
        "整合包缺少 modrinth.index.json 或 manifest.json".to_owned(),
    ))
}

fn parse_modrinth_index(index: &[u8], archive: &mut ZipArchive<fs::File>) -> Result<ModpackPlan> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct MrIndex {
        format_version: u16,
        game: String,
        version_id: String,
        name: String,
        files: Vec<MrFile>,
        dependencies: HashMap<String, String>,
    }
    #[derive(Deserialize)]
    struct MrFile {
        path: String,
        hashes: MrHashes,
        downloads: Vec<String>,
        #[serde(rename = "fileSize")]
        file_size: u64,
    }
    #[derive(Deserialize)]
    struct MrHashes {
        sha1: Option<String>,
        sha512: Option<String>,
    }
    let index: MrIndex = serde_json::from_slice(index)
        .map_err(|error| CoreError::Content(format!("modrinth.index.json 无效：{error}")))?;
    if index.format_version != 1 {
        return Err(CoreError::Content(format!(
            "不支持的 modrinth.index.json 版本：{}",
            index.format_version
        )));
    }
    if index.game != "minecraft" {
        return Err(CoreError::Content(
            "整合包目标游戏不是 minecraft".to_owned(),
        ));
    }
    if index.files.len() > MAX_PACK_FILES {
        return Err(CoreError::Content(
            "整合包文件数量超出可接受范围".to_owned(),
        ));
    }
    let game_version = required_dependency(&index.dependencies, "minecraft")?.to_owned();
    let (loader_kind, loader_version) = resolve_loader_dependency(&index.dependencies)?;
    let mut files = Vec::with_capacity(index.files.len());
    for file in &index.files {
        let relative_path = validate_pack_relative_path(&file.path)?;
        let url = file
            .downloads
            .first()
            .ok_or_else(|| CoreError::Content(format!("文件 {} 没有下载地址", file.path)))?;
        if file.hashes.sha1.is_none() && file.hashes.sha512.is_none() {
            return Err(CoreError::Content(format!(
                "文件 {} 缺少校验哈希",
                file.path
            )));
        }
        files.push(ModpackFile {
            relative_path,
            url: Some(url.clone()),
            urls: file.downloads.clone(),
            sha1: file.hashes.sha1.clone(),
            sha512: file.hashes.sha512.clone(),
            size: file.file_size,
            cf_project_id: None,
            cf_file_id: None,
        });
    }
    let overrides = collect_overrides(archive, "overrides/")?;
    Ok(ModpackPlan {
        provider: ModpackProvider::Modrinth,
        name: index.name,
        version: index.version_id,
        game_version,
        loader_kind,
        loader_version,
        files,
        overrides,
    })
}

fn parse_curseforge_manifest(
    manifest: &[u8],
    archive: &mut ZipArchive<fs::File>,
) -> Result<ModpackPlan> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct CfManifest {
        minecraft: CfMinecraft,
        name: String,
        version: String,
        files: Vec<CfFile>,
    }
    #[derive(Deserialize)]
    struct CfMinecraft {
        version: String,
        #[serde(rename = "modLoaders")]
        mod_loaders: Vec<CfModLoader>,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct CfModLoader {
        id: String,
        primary: bool,
    }
    #[derive(Deserialize)]
    struct CfFile {
        #[serde(rename = "projectID")]
        project_id: u64,
        #[serde(rename = "fileID")]
        file_id: u64,
        required: bool,
    }
    let manifest: CfManifest = serde_json::from_slice(manifest)
        .map_err(|error| CoreError::Content(format!("manifest.json 无效：{error}")))?;
    if manifest.files.is_empty() || manifest.files.len() > MAX_PACK_FILES {
        return Err(CoreError::Content(
            "整合包文件数量超出可接受范围".to_owned(),
        ));
    }
    let loader = manifest
        .minecraft
        .mod_loaders
        .iter()
        .find(|loader| loader.primary)
        .or_else(|| manifest.minecraft.mod_loaders.first())
        .ok_or_else(|| CoreError::Content("整合包没有声明加载器".to_owned()))?;
    let (loader_kind, loader_version) = loader
        .id
        .split_once('-')
        .ok_or_else(|| CoreError::Content(format!("加载器标识无效：{}", loader.id)))?;
    let loader_kind = normalize_loader_kind(loader_kind)?;
    let mut files = Vec::with_capacity(manifest.files.len());
    for file in &manifest.files {
        if !file.required {
            continue;
        }
        files.push(ModpackFile {
            relative_path: String::new(),
            url: None,
            urls: Vec::new(),
            sha1: None,
            sha512: None,
            size: 0,
            cf_project_id: Some(file.project_id),
            cf_file_id: Some(file.file_id),
        });
    }
    if files.is_empty() {
        return Err(CoreError::Content("整合包没有必需文件".to_owned()));
    }
    let overrides = collect_overrides(archive, "overrides/")?;
    Ok(ModpackPlan {
        provider: ModpackProvider::Curseforge,
        name: manifest.name,
        version: manifest.version,
        game_version: manifest.minecraft.version,
        loader_kind: loader_kind.to_owned(),
        loader_version: loader_version.to_owned(),
        files,
        overrides,
    })
}

fn required_dependency<'a>(
    dependencies: &'a HashMap<String, String>,
    key: &str,
) -> Result<&'a str> {
    dependencies
        .get(key)
        .map(String::as_str)
        .ok_or_else(|| CoreError::Content(format!("整合包缺少依赖声明：{key}")))
}

fn resolve_loader_dependency(dependencies: &HashMap<String, String>) -> Result<(String, String)> {
    for key in ["fabric-loader", "quilt-loader", "forge", "neoforge"] {
        if let Some(version) = dependencies.get(key) {
            let kind = match key {
                "fabric-loader" => "fabric",
                "quilt-loader" => "quilt",
                "forge" => "forge",
                _ => "neoforge",
            };
            return Ok((kind.to_owned(), version.clone()));
        }
    }
    Err(CoreError::Content(
        "整合包没有声明受支持的加载器".to_owned(),
    ))
}

fn normalize_loader_kind(kind: &str) -> Result<&str> {
    match kind {
        "fabric" | "quilt" | "forge" | "neoforge" => Ok(kind),
        _ => Err(CoreError::Content(format!("不支持的加载器：{kind}"))),
    }
}

pub(crate) fn validate_pack_relative_path(path: &str) -> Result<String> {
    let relative = Path::new(path);
    let mut components = Vec::new();
    for component in relative.components() {
        let Component::Normal(value) = component else {
            return Err(CoreError::Content(format!("整合包包含不安全路径：{path}")));
        };
        let value = value
            .to_str()
            .ok_or_else(|| CoreError::Content("整合包路径不是有效 Unicode".to_owned()))?;
        if value.contains(':') || value.contains('\\') {
            return Err(CoreError::Content(format!("整合包包含不安全路径：{path}")));
        }
        components.push(value.to_owned());
    }
    if components.is_empty() {
        return Err(CoreError::Content(format!("整合包路径为空：{path}")));
    }
    Ok(components.join("/"))
}

fn collect_overrides(
    archive: &mut ZipArchive<fs::File>,
    prefix: &str,
) -> Result<Vec<ModpackOverrideFile>> {
    let mut overrides = Vec::new();
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|error| CoreError::Content(format!("整合包无法读取：{error}")))?;
        let Some(name) = entry.enclosed_name().map(|path| path.to_path_buf()) else {
            return Err(CoreError::Content(
                "整合包 overrides 包含不安全路径".to_owned(),
            ));
        };
        let name_str = name.to_string_lossy().replace('\\', "/");
        if !name_str.starts_with(prefix) || entry.is_dir() {
            continue;
        }
        let relative = validate_pack_relative_path(&name_str[prefix.len()..])?;
        overrides.push(ModpackOverrideFile {
            relative_path: relative,
            zip_entry: name_str,
        });
    }
    Ok(overrides)
}

pub fn modpack_preview(plan: &ModpackPlan) -> ModpackPreview {
    ModpackPreview {
        provider: plan.provider,
        name: plan.name.clone(),
        version: plan.version.clone(),
        game_version: plan.game_version.clone(),
        loader_kind: plan.loader_kind.clone(),
        loader_version: plan.loader_version.clone(),
        file_count: plan.files.len() as u64,
        total_bytes: plan
            .files
            .iter()
            .fold(0_u64, |total, file| total.saturating_add(file.size)),
    }
}

impl AppService {
    pub fn installed_modpack(&self, instance_id: &str) -> Result<Option<InstalledModpack>> {
        let connection = self.connection()?;
        let row = connection
            .query_row(
                "
                SELECT provider, pack_name, pack_version, game_version, loader_kind,
                       managed_files_json, installed_at_unix_seconds
                FROM instance_modpacks WHERE instance_id = ?1
                ",
                params![instance_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, i64>(6)?,
                    ))
                },
            )
            .optional()?;
        let Some((provider, name, version, game_version, loader_kind, managed_json, installed_at)) =
            row
        else {
            return Ok(None);
        };
        let managed_files: Vec<ManagedPackFile> = serde_json::from_str(&managed_json)
            .map_err(|error| CoreError::InvalidStoredState(format!("整合包清单无效：{error}")))?;
        Ok(Some(InstalledModpack {
            provider: ModpackProvider::from_database(&provider)?,
            pack_name: name,
            pack_version: version,
            game_version,
            loader_kind,
            managed_files,
            installed_at_unix_seconds: installed_at,
        }))
    }

    /// 安装整合包文件到已就绪实例（游戏与加载器由调用方先行安装）。
    pub async fn install_modpack_files(
        &self,
        plan: &ModpackPlan,
        archive_path: &Path,
        instance_id: &str,
        mci: &MciMirrorClient,
        on_progress: &(dyn Fn(u64, u64, &str) + Sync),
    ) -> Result<ModpackInstallReport> {
        let instance = self.ready_instance(instance_id)?;
        if self.installed_modpack(instance_id)?.is_some() {
            return Err(CoreError::Content(
                "该实例已安装整合包，请使用更新入口".to_owned(),
            ));
        }
        if instance.game_version != plan.game_version || instance.loader_kind != plan.loader_kind {
            return Err(CoreError::Content(format!(
                "整合包要求 Minecraft {} 与 {}，与实例不匹配",
                plan.game_version, plan.loader_kind
            )));
        }
        let instance_root = PathBuf::from(&instance.root_directory);
        let operation_id = Uuid::new_v4().to_string();
        let staging = self
            .selected_data_directory()?
            .join(".staging")
            .join("modpack")
            .join(&operation_id);
        fs::create_dir_all(&staging)?;
        let artifacts = self
            .resolve_modpack_artifacts(plan, mci, on_progress)
            .await?;
        let result = self
            .download_and_commit_modpack(
                &instance_root,
                &staging,
                plan,
                archive_path,
                artifacts,
                "install",
                on_progress,
            )
            .await;
        if let Err(error) = result {
            let _ = rollback_modpack_journal(&instance_root, &staging);
            let _ = fs::remove_dir_all(&staging);
            return Err(error);
        }
        let managed = collect_managed_files(&instance_root, plan)?;
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute(
            "
            INSERT INTO instance_modpacks (
                instance_id, provider, pack_name, pack_version, game_version, loader_kind,
                managed_files_json, installed_at_unix_seconds
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            ",
            params![
                instance_id,
                plan.provider.database_value(),
                plan.name,
                plan.version,
                plan.game_version,
                plan.loader_kind,
                serde_json::to_string(&managed)?,
                unix_timestamp(),
            ],
        )?;
        if let Err(error) = transaction.commit() {
            let _ = rollback_modpack_journal(&instance_root, &staging);
            let _ = fs::remove_dir_all(&staging);
            return Err(error.into());
        }
        let _ = fs::remove_dir_all(&staging);
        Ok(ModpackInstallReport {
            instance_id: instance_id.to_owned(),
            pack_name: plan.name.clone(),
            pack_version: plan.version.clone(),
            installed_files: managed.len() as u64,
        })
    }

    /// 用同格式新版整合包更新实例；用户改动过的文件保留并汇总。
    pub async fn update_modpack(
        &self,
        plan: &ModpackPlan,
        archive_path: &Path,
        instance_id: &str,
        mci: &MciMirrorClient,
        on_progress: &(dyn Fn(u64, u64, &str) + Sync),
    ) -> Result<ModpackUpdateReport> {
        let instance = self.ready_instance(instance_id)?;
        let installed = self
            .installed_modpack(instance_id)?
            .ok_or_else(|| CoreError::Content("该实例不是由整合包安装的".to_owned()))?;
        if installed.provider != plan.provider {
            return Err(CoreError::Content(
                "更新必须使用同一来源格式的整合包".to_owned(),
            ));
        }
        if installed.game_version != plan.game_version || installed.loader_kind != plan.loader_kind
        {
            return Err(CoreError::Content(format!(
                "新版整合包要求 Minecraft {} 与 {}，与当前实例不一致",
                plan.game_version, plan.loader_kind
            )));
        }
        let instance_root = PathBuf::from(&instance.root_directory);
        let operation_id = Uuid::new_v4().to_string();
        let staging = self
            .selected_data_directory()?
            .join(".staging")
            .join("modpack")
            .join(&operation_id);
        fs::create_dir_all(&staging)?;

        let minecraft_root = instance_root.join(".minecraft");
        let managed_index: HashMap<String, &ManagedPackFile> = installed
            .managed_files
            .iter()
            .map(|file| (file.relative_path.clone(), file))
            .collect();
        let new_paths: HashSet<String> = plan
            .files
            .iter()
            .map(|file| file.relative_path.clone())
            .chain(
                plan.overrides
                    .iter()
                    .map(|entry| entry.relative_path.clone()),
            )
            .collect();
        let mut deleted_files = 0_u64;
        let mut kept_user_modified = Vec::new();
        // 删除：旧包有而新包没有，且当前内容未被用户改动。
        for managed in &installed.managed_files {
            if new_paths.contains(&managed.relative_path) {
                continue;
            }
            let target = minecraft_root.join(&managed.relative_path);
            if !target.exists() {
                continue;
            }
            let current_sha512 = file_sha512(&target)?;
            if current_sha512.eq_ignore_ascii_case(&managed.sha512) {
                fs::remove_file(&target)?;
                deleted_files += 1;
            } else {
                kept_user_modified.push(managed.relative_path.clone());
            }
        }
        // 替换/新增：跳过用户改动过的受管文件。
        let mut filtered_plan = plan.clone();
        let mut user_modified_paths = HashSet::new();
        for file in &mut filtered_plan.files {
            if let Some(managed) = managed_index.get(&file.relative_path) {
                let target = minecraft_root.join(&file.relative_path);
                if target.exists() && !file_sha512(&target)?.eq_ignore_ascii_case(&managed.sha512) {
                    user_modified_paths.insert(file.relative_path.clone());
                }
            }
        }
        filtered_plan
            .files
            .retain(|file| !user_modified_paths.contains(&file.relative_path));
        for path in &user_modified_paths {
            if !kept_user_modified.contains(path) {
                kept_user_modified.push(path.clone());
            }
        }
        kept_user_modified.sort();

        let artifacts = self
            .resolve_modpack_artifacts(&filtered_plan, mci, on_progress)
            .await?;
        let result = self
            .download_and_commit_modpack(
                &instance_root,
                &staging,
                &filtered_plan,
                archive_path,
                artifacts,
                "update",
                on_progress,
            )
            .await;
        if let Err(error) = result {
            let _ = rollback_modpack_journal(&instance_root, &staging);
            let _ = fs::remove_dir_all(&staging);
            return Err(error);
        }
        let managed = collect_managed_files(&instance_root, plan)?;
        let added = plan
            .files
            .iter()
            .filter(|file| !managed_index.contains_key(&file.relative_path))
            .count() as u64;
        let replaced = filtered_plan.files.len() as u64 - added;
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute(
            "
            UPDATE instance_modpacks
            SET pack_version = ?2, managed_files_json = ?3, installed_at_unix_seconds = ?4
            WHERE instance_id = ?1
            ",
            params![
                instance_id,
                plan.version,
                serde_json::to_string(&managed)?,
                unix_timestamp(),
            ],
        )?;
        if let Err(error) = transaction.commit() {
            let _ = rollback_modpack_journal(&instance_root, &staging);
            let _ = fs::remove_dir_all(&staging);
            return Err(error.into());
        }
        let _ = fs::remove_dir_all(&staging);
        Ok(ModpackUpdateReport {
            pack_name: plan.name.clone(),
            from_version: installed.pack_version.clone(),
            to_version: plan.version.clone(),
            added_files: added,
            replaced_files: replaced,
            deleted_files,
            kept_user_modified,
        })
    }

    #[doc(hidden)]
    pub async fn resolve_modpack_artifacts(
        &self,
        plan: &ModpackPlan,
        mci: &MciMirrorClient,
        on_progress: &(dyn Fn(u64, u64, &str) + Sync),
    ) -> Result<Vec<(String, ResolvedArtifact, Vec<DownloadCandidate>)>> {
        let policy = self.download_source_policy()?;
        let mut artifacts = Vec::with_capacity(plan.files.len() + plan.overrides.len());
        let total = plan.files.len() as u64;
        for (index, file) in plan.files.iter().enumerate() {
            let (candidates, size, sha1, sha512) = match plan.provider {
                ModpackProvider::Modrinth => (
                    Self::modpack_file_candidates(file, &policy)?,
                    file.size,
                    file.sha1.clone(),
                    file.sha512.clone(),
                ),
                ModpackProvider::Curseforge => {
                    let resolved = mci
                        .curseforge_file(
                            file.cf_project_id.ok_or_else(|| {
                                CoreError::Content("CurseForge 文件缺少 projectId".to_owned())
                            })?,
                            file.cf_file_id.ok_or_else(|| {
                                CoreError::Content("CurseForge 文件缺少 fileId".to_owned())
                            })?,
                        )
                        .await?;
                    on_progress(index as u64, total, &format!("解析 {}", resolved.file_name));
                    let candidates = match candidates_for(&resolved.url, &policy) {
                        SourceCandidates::Ready(list) => list,
                        SourceCandidates::CurseForgeOfficialUnavailable { mirror } => vec![mirror],
                        SourceCandidates::CustomUnsupported { reason } => {
                            return Err(CoreError::Content(reason));
                        }
                    };
                    (candidates, resolved.size, Some(resolved.sha1), None)
                }
            };
            let primary_url = candidates
                .first()
                .map(|candidate| candidate.url.clone())
                .ok_or_else(|| {
                    CoreError::Content(format!("文件 {} 没有可用下载来源", file.relative_path))
                })?;
            artifacts.push((
                file.relative_path.clone(),
                ResolvedArtifact {
                    kind: ArtifactKind::ContentMod,
                    relative_path: format!("pack/{}", file.relative_path.replace('/', "_")),
                    url: primary_url,
                    size,
                    sha1,
                    sha256: None,
                    sha512,
                },
                candidates,
            ));
        }
        Ok(artifacts)
    }

    /// 为 mrpack 文件构造有序下载候选:实测 forgecdn 直连严重限速(约 30 KB/s),
    /// 同一文件的 cdn.modrinth.com 快约 20 倍,因此把 Modrinth CDN 提前,其余
    /// 地址保持包内声明顺序;每个地址再按下载源策略展开镜像候选并按 URL 去重。
    #[doc(hidden)]
    pub fn modpack_file_candidates(
        file: &ModpackFile,
        policy: &crate::SourcePolicy,
    ) -> Result<Vec<DownloadCandidate>> {
        let mut ordered = file.urls.clone();
        if ordered.is_empty()
            && let Some(url) = &file.url
        {
            ordered.push(url.clone());
        }
        ordered.sort_by_key(|url| !url.contains("cdn.modrinth.com"));
        let mut seen = HashSet::new();
        let mut candidates = Vec::new();
        for url in ordered {
            let expanded = match candidates_for(&url, policy) {
                SourceCandidates::Ready(list) => list,
                SourceCandidates::CurseForgeOfficialUnavailable { mirror } => vec![mirror],
                // 自定义源不覆盖该域名时跳过这个备用地址,尝试下一个。
                SourceCandidates::CustomUnsupported { .. } => Vec::new(),
            };
            for candidate in expanded {
                if seen.insert(candidate.url.clone()) {
                    candidates.push(candidate);
                }
            }
        }
        if candidates.is_empty() {
            return Err(CoreError::Content(format!(
                "文件 {} 没有可用下载来源",
                file.relative_path
            )));
        }
        Ok(candidates)
    }

    #[allow(clippy::too_many_arguments)]
    async fn download_and_commit_modpack(
        &self,
        instance_root: &Path,
        staging: &Path,
        plan: &ModpackPlan,
        archive_path: &Path,
        artifacts: Vec<(String, ResolvedArtifact, Vec<DownloadCandidate>)>,
        operation: &str,
        on_progress: &(dyn Fn(u64, u64, &str) + Sync),
    ) -> Result<()> {
        // 与游戏安装一致使用用户配置的下载并发(此前写死 4,大整合包极慢)。
        let concurrency = self.download_concurrency()?;
        let downloader = crate::ArtifactDownloader::new(concurrency)?;
        let download_root = staging.join("downloads");
        fs::create_dir_all(&download_root)?;
        let shared_root = self.selected_data_directory()?.join("store");
        fs::create_dir_all(&shared_root)?;
        let total = artifacts.len() as u64;
        // 第一遍:全并发下载,失败文件收集起来;网络抖动窗口里
        // 单文件失败不应拖垮整个整合包。
        let outcomes = futures_util::stream::iter(artifacts.into_iter().enumerate().map(
            |(index, (relative, artifact, candidates))| {
                let downloader = downloader.clone();
                let download_root = download_root.clone();
                let shared_root = shared_root.clone();
                async move {
                    let result = downloader
                        .fetch_with_candidates(
                            &artifact,
                            &download_root,
                            &shared_root,
                            &candidates,
                            None,
                        )
                        .await;
                    on_progress(index as u64 + 1, total, &artifact.relative_path);
                    (relative, artifact, candidates, result)
                }
            },
        ))
        .buffer_unordered(concurrency)
        .collect::<Vec<_>>()
        .await;
        let mut downloads: Vec<(String, crate::FetchReport)> = Vec::new();
        let mut failed = Vec::new();
        for (relative, artifact, candidates, result) in outcomes {
            match result {
                Ok(report) => downloads.push((relative, report)),
                Err(error) => failed.push((relative, artifact, candidates, error)),
            }
        }
        // 第二遍:失败文件延迟后逐个重试,仍失败才整体报错。
        if !failed.is_empty() {
            on_progress(0, total, "重试下载失败的文件");
            tokio::time::sleep(Duration::from_secs(2)).await;
            for (relative, artifact, candidates, _) in failed {
                let report = downloader
                    .fetch_with_candidates(
                        &artifact,
                        &download_root,
                        &shared_root,
                        &candidates,
                        None,
                    )
                    .await
                    .map_err(|error| {
                        CoreError::Content(format!(
                            "文件 {relative} 下载失败（镜像与备用地址均已重试）：{error}"
                        ))
                    })?;
                downloads.push((relative, report));
            }
        }

        // overrides 解包到暂存（存在时）。
        if !plan.overrides.is_empty() {
            let file = fs::File::open(archive_path)?;
            let mut archive = ZipArchive::new(file)
                .map_err(|error| CoreError::Content(format!("整合包无法读取：{error}")))?;
            for entry_plan in &plan.overrides {
                let mut entry = archive.by_name(&entry_plan.zip_entry).map_err(|error| {
                    CoreError::Content(format!(
                        "整合包缺少 overrides 条目 {}：{error}",
                        entry_plan.zip_entry
                    ))
                })?;
                let target = staging.join("overrides").join(&entry_plan.relative_path);
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent)?;
                }
                let mut output = fs::File::create(&target)?;
                std::io::copy(&mut entry, &mut output)?;
            }
        }

        // 提交：同名异哈希先备份到实例快照区，再原子 rename；日志驱动回滚。
        let mut journal = ModpackJournal {
            schema_version: 1,
            operation: operation.to_owned(),
            committed: Vec::new(),
            backups: Vec::new(),
        };
        write_modpack_journal(staging, &journal)?;
        let minecraft_root = instance_root.join(".minecraft");
        let snapshot_root = instance_root
            .join(".moyumax")
            .join("snapshots")
            .join(format!(
                "modpack-{}",
                staging.file_name().unwrap_or_default().to_string_lossy()
            ));
        let mut entries: Vec<(PathBuf, String)> = downloads
            .into_iter()
            .map(|(relative, report)| (report.result.staged_file, relative))
            .collect();
        for entry in &plan.overrides {
            entries.push((
                staging.join("overrides").join(&entry.relative_path),
                entry.relative_path.clone(),
            ));
        }
        for (staged, relative) in &entries {
            let target = minecraft_root.join(relative);
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            if target.exists() {
                let staged_sha = file_sha512(staged)?;
                let target_sha = file_sha512(&target)?;
                if staged_sha != target_sha {
                    let backup = snapshot_root.join(relative);
                    if let Some(parent) = backup.parent() {
                        fs::create_dir_all(parent)?;
                    }
                    fs::rename(&target, &backup)?;
                    journal.backups.push((relative.clone(), relative.clone()));
                } else {
                    continue;
                }
            }
            fs::rename(staged, &target)?;
            journal.committed.push(relative.clone());
            write_modpack_journal(staging, &journal)?;
        }
        journal.operation = format!("{operation}-done");
        write_modpack_journal(staging, &journal)?;
        Ok(())
    }

    /// 重启收敛：回滚未完成的整合包提交。
    pub(crate) fn recover_interrupted_modpack_ops(&self) -> Result<()> {
        let staging_root = self
            .selected_data_directory()?
            .join(".staging")
            .join("modpack");
        if !staging_root.exists() {
            return Ok(());
        }
        let instances = self.list_instances()?;
        for entry in fs::read_dir(&staging_root)? {
            let operation_dir = entry?.path();
            let journal_path = operation_dir.join(MODPACK_JOURNAL_NAME);
            if !journal_path.is_file() {
                continue;
            }
            let journal: ModpackJournal = serde_json::from_slice(&fs::read(&journal_path)?)?;
            if journal.operation.ends_with("-done") {
                let _ = fs::remove_dir_all(&operation_dir);
                continue;
            }
            for instance in &instances {
                let _ =
                    rollback_modpack_journal(Path::new(&instance.root_directory), &operation_dir);
            }
            let _ = fs::remove_dir_all(&operation_dir);
        }
        Ok(())
    }
}

fn collect_managed_files(instance_root: &Path, plan: &ModpackPlan) -> Result<Vec<ManagedPackFile>> {
    let minecraft_root = instance_root.join(".minecraft");
    let mut managed = Vec::new();
    for file in &plan.files {
        let target = minecraft_root.join(&file.relative_path);
        if target.exists() {
            managed.push(ManagedPackFile {
                relative_path: file.relative_path.clone(),
                sha512: file_sha512(&target)?,
                size: fs::metadata(&target)?.len(),
            });
        }
    }
    for entry in &plan.overrides {
        let target = minecraft_root.join(&entry.relative_path);
        if target.exists() {
            managed.push(ManagedPackFile {
                relative_path: entry.relative_path.clone(),
                sha512: file_sha512(&target)?,
                size: fs::metadata(&target)?.len(),
            });
        }
    }
    Ok(managed)
}

fn write_modpack_journal(staging: &Path, journal: &ModpackJournal) -> Result<()> {
    fs::create_dir_all(staging)?;
    fs::write(
        staging.join(MODPACK_JOURNAL_NAME),
        serde_json::to_vec_pretty(journal)?,
    )?;
    Ok(())
}

fn rollback_modpack_journal(instance_root: &Path, staging: &Path) -> Result<()> {
    let journal_path = staging.join(MODPACK_JOURNAL_NAME);
    if !journal_path.is_file() {
        return Ok(());
    }
    let journal: ModpackJournal = serde_json::from_slice(&fs::read(&journal_path)?)?;
    if journal.operation.ends_with("-done") {
        return Ok(());
    }
    let minecraft_root = instance_root.join(".minecraft");
    for relative in journal.committed.iter().rev() {
        let _ = fs::remove_file(minecraft_root.join(relative));
    }
    let op = staging
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let snapshot_root = instance_root
        .join(".moyumax")
        .join("snapshots")
        .join(format!("modpack-{op}"));
    for (relative, backup_relative) in journal.backups.iter().rev() {
        let backup = snapshot_root.join(backup_relative);
        let target = minecraft_root.join(relative);
        if backup.exists() {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::rename(&backup, &target)?;
        }
    }
    if snapshot_root.exists() {
        let _ = fs::remove_dir_all(&snapshot_root);
    }
    Ok(())
}

fn file_sha512(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path)?;
    let mut buffer = vec![0_u8; 128 * 1024];
    let mut sha512 = Sha512::new();
    loop {
        let read = std::io::Read::read(&mut file, &mut buffer)?;
        if read == 0 {
            break;
        }
        sha512.update(&buffer[..read]);
    }
    Ok(encode_hex(sha512.finalize()))
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
