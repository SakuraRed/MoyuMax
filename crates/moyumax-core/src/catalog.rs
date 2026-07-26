use std::time::Duration;

use reqwest::{Client, Url, header::CONTENT_LENGTH};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha1::{Digest as Sha1Digest, Sha1};
use sha2::{Digest as Sha2Digest, Sha256};

use crate::{
    AppService, ArtifactKind, CoreError, InstallProfile, InstallSelection, JavaArchitecture,
    JavaDistribution, LoaderChoice, ResolvedArtifact, ResolvedGameVersion, ResolvedInstallRequest,
    ResolvedJavaPackage, ResolvedLoader, Result, read_install_profile, unix_timestamp,
};

const VERSION_MANIFEST_CACHE_KEY: &str = "mojang-version-manifest-v2";
const VERSION_MANIFEST_URL: &str =
    "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
const FABRIC_META_BASE_URL: &str = "https://meta.fabricmc.net/v2";
const QUILT_META_BASE_URL: &str = "https://meta.quiltmc.org/v3";
const AZUL_META_BASE_URL: &str = "https://api.azul.com/metadata/v1/zulu";
const BMCLAPI_BASE_URL: &str = "https://bmclapi2.bangbang93.com";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CatalogSource {
    Network,
    Cache,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GameReleaseType {
    Release,
    Snapshot,
    OldBeta,
    OldAlpha,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameVersionSummary {
    pub id: String,
    pub release_type: GameReleaseType,
    pub release_time: String,
    pub metadata_url: String,
    pub metadata_sha1: String,
    pub recommended: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionCatalog {
    pub latest_release: String,
    pub latest_snapshot: String,
    pub versions: Vec<GameVersionSummary>,
    pub fetched_at_unix_seconds: i64,
    pub source: CatalogSource,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FabricLoaderSummary {
    pub version: String,
    pub stable: bool,
    pub recommended: bool,
}

#[derive(Debug, Deserialize)]
struct RawVersionManifest {
    latest: RawLatestVersions,
    versions: Vec<RawVersionSummary>,
}

#[derive(Debug, Deserialize)]
struct RawLatestVersions {
    release: String,
    snapshot: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawVersionSummary {
    id: String,
    #[serde(rename = "type")]
    release_type: String,
    url: String,
    sha1: String,
    release_time: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawVersionMetadata {
    id: String,
    main_class: String,
    java_version: Option<RawJavaVersion>,
    downloads: RawGameDownloads,
    asset_index: RawDownload,
    libraries: Vec<RawLibrary>,
    logging: Option<RawLogging>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawJavaVersion {
    major_version: u16,
}

#[derive(Debug, Deserialize)]
struct RawGameDownloads {
    client: RawDownload,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawDownload {
    url: String,
    sha1: String,
    size: u64,
    id: Option<String>,
    total_size: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct RawLibrary {
    downloads: Option<RawLibraryDownloads>,
}

#[derive(Debug, Deserialize)]
struct RawLibraryDownloads {
    artifact: Option<RawLibraryArtifact>,
}

#[derive(Debug, Deserialize)]
struct RawLibraryArtifact {
    path: String,
    url: String,
    sha1: String,
    size: u64,
}

#[derive(Debug, Deserialize)]
struct RawLogging {
    client: RawLoggingClient,
}

#[derive(Debug, Deserialize)]
struct RawLoggingClient {
    file: RawLoggingFile,
}

#[derive(Debug, Deserialize)]
struct RawLoggingFile {
    id: String,
    url: String,
    sha1: String,
    size: u64,
}

#[derive(Debug, Deserialize)]
struct RawFabricLoaderEntry {
    loader: RawFabricLoader,
}

#[derive(Debug, Deserialize)]
struct RawFabricLoader {
    version: String,
    stable: bool,
}

#[derive(Debug, Deserialize)]
struct RawQuiltLoaderEntry {
    loader: RawQuiltLoader,
}

#[derive(Debug, Deserialize)]
struct RawQuiltLoader {
    version: String,
}

#[derive(Debug, Deserialize)]
struct RawForgeVersion {
    version: String,
}

#[derive(Debug, Deserialize)]
struct RawNeoForgeVersion {
    version: String,
    #[serde(rename = "installerPath")]
    installer_path: String,
}

/// 校验安装器声明的目标与所选一致,防止装错对象。
fn validate_installer_target(
    profile: &InstallProfile,
    game_version: &str,
    loader_version: &str,
    loader_name: &str,
) -> Result<()> {
    if !profile.minecraft.is_empty() && profile.minecraft != game_version {
        return Err(CoreError::InvalidInstallRequest(format!(
            "{loader_name} 安装器目标 Minecraft {} 与所选 {game_version} 不一致",
            profile.minecraft
        )));
    }
    if !profile.version.is_empty() && !profile.version.contains(loader_version) {
        return Err(CoreError::InvalidInstallRequest(format!(
            "{loader_name} 安装器版本 {} 与所选 {loader_version} 不一致",
            profile.version
        )));
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
struct RawAzulPackageSummary {
    package_uuid: String,
}

#[derive(Debug, Deserialize)]
struct RawAzulPackageDetails {
    package_uuid: String,
    java_version: Vec<u16>,
    openjdk_build_number: u16,
    download_url: String,
    name: String,
    sha256_hash: String,
    size: u64,
    hw_bitness: u16,
}

pub fn parse_mojang_version_manifest(payload: &[u8], fetched_at: i64) -> Result<VersionCatalog> {
    let manifest: RawVersionManifest = serde_json::from_slice(payload)?;
    if manifest.latest.release.trim().is_empty() || manifest.versions.is_empty() {
        return Err(CoreError::Metadata(
            "官方版本目录没有提供推荐稳定版或版本条目".to_owned(),
        ));
    }

    let versions = manifest
        .versions
        .into_iter()
        .map(|version| GameVersionSummary {
            recommended: version.id == manifest.latest.release,
            id: version.id,
            release_type: release_type(&version.release_type),
            release_time: version.release_time,
            metadata_url: version.url,
            metadata_sha1: version.sha1,
        })
        .collect();

    Ok(VersionCatalog {
        latest_release: manifest.latest.release,
        latest_snapshot: manifest.latest.snapshot,
        versions,
        fetched_at_unix_seconds: fetched_at,
        source: CatalogSource::Network,
    })
}

impl AppService {
    pub fn store_version_catalog(&self, catalog: &VersionCatalog) -> Result<()> {
        let payload = serde_json::to_string(catalog)?;
        self.connection()?.execute(
            "
            INSERT INTO metadata_cache (cache_key, payload_json, updated_at_unix_seconds)
            VALUES (?1, ?2, ?3)
            ON CONFLICT(cache_key) DO UPDATE SET
                payload_json = excluded.payload_json,
                updated_at_unix_seconds = excluded.updated_at_unix_seconds
            ",
            params![
                VERSION_MANIFEST_CACHE_KEY,
                payload,
                catalog.fetched_at_unix_seconds
            ],
        )?;
        Ok(())
    }

    pub fn cached_version_catalog(&self) -> Result<Option<VersionCatalog>> {
        let serialized = self
            .connection()?
            .query_row(
                "SELECT payload_json FROM metadata_cache WHERE cache_key = ?1",
                params![VERSION_MANIFEST_CACHE_KEY],
                |row| row.get::<_, String>(0),
            )
            .optional()?;

        serialized
            .map(|payload| {
                let mut catalog: VersionCatalog = serde_json::from_str(&payload)?;
                catalog.source = CatalogSource::Cache;
                Ok(catalog)
            })
            .transpose()
    }
}

#[derive(Debug, Clone)]
pub struct MetadataClient {
    client: Client,
    quilt_meta_base: String,
    bmclapi_base: String,
    forge_maven_base: String,
    neoforge_maven_base: String,
}

impl MetadataClient {
    pub fn new() -> Result<Self> {
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(15))
            .user_agent(concat!("MoyuMax/", env!("CARGO_PKG_VERSION")))
            .build()?;
        Ok(Self {
            client,
            quilt_meta_base: QUILT_META_BASE_URL.to_owned(),
            bmclapi_base: BMCLAPI_BASE_URL.to_owned(),
            forge_maven_base: "https://maven.minecraftforge.net".to_owned(),
            neoforge_maven_base: "https://maven.neoforged.net/releases".to_owned(),
        })
    }

    /// 覆盖 Quilt 元数据基址,供本地契约测试使用。生产保持官方默认值。
    #[doc(hidden)]
    #[must_use]
    pub fn with_quilt_meta_base(mut self, base: String) -> Self {
        self.quilt_meta_base = base.trim_end_matches('/').to_owned();
        self
    }

    /// 覆盖 BMCLAPI 基址,供本地契约测试使用。生产保持官方默认值。
    #[doc(hidden)]
    #[must_use]
    pub fn with_bmclapi_base(mut self, base: String) -> Self {
        self.bmclapi_base = base.trim_end_matches('/').to_owned();
        self
    }

    /// 覆盖 Forge Maven 基址,供本地契约测试使用。生产保持官方默认值。
    #[doc(hidden)]
    #[must_use]
    pub fn with_forge_maven_base(mut self, base: String) -> Self {
        self.forge_maven_base = base.trim_end_matches('/').to_owned();
        self
    }

    /// 覆盖 NeoForge Maven 基址,供本地契约测试使用。生产保持官方默认值。
    #[doc(hidden)]
    #[must_use]
    pub fn with_neoforge_maven_base(mut self, base: String) -> Self {
        self.neoforge_maven_base = base.trim_end_matches('/').to_owned();
        self
    }

    pub async fn fetch_version_catalog(&self) -> Result<VersionCatalog> {
        let payload = self
            .client
            .get(VERSION_MANIFEST_URL)
            .send()
            .await?
            .error_for_status()?
            .bytes()
            .await?;
        parse_mojang_version_manifest(&payload, unix_timestamp())
    }

    pub async fn compatible_fabric_loaders(
        &self,
        game_version: &str,
    ) -> Result<Vec<FabricLoaderSummary>> {
        validate_metadata_component(game_version, "Minecraft 版本")?;
        let url = format!("{FABRIC_META_BASE_URL}/versions/loader/{game_version}");
        let entries = self
            .client
            .get(url)
            .send()
            .await?
            .error_for_status()?
            .json::<Vec<RawFabricLoaderEntry>>()
            .await?;
        let recommended_index = entries.iter().position(|entry| entry.loader.stable);
        Ok(entries
            .into_iter()
            .enumerate()
            .map(|(index, entry)| FabricLoaderSummary {
                version: entry.loader.version,
                stable: entry.loader.stable,
                recommended: Some(index) == recommended_index,
            })
            .collect())
    }

    /// Quilt 版本列表。Quilt 元数据不直接给出稳定标记,
    /// 版本号不含 `-`(如 0.30.0)视为稳定,推荐第一个稳定项。
    pub async fn compatible_quilt_loaders(
        &self,
        game_version: &str,
    ) -> Result<Vec<FabricLoaderSummary>> {
        validate_metadata_component(game_version, "Minecraft 版本")?;
        let url = format!("{}/versions/loader/{game_version}", self.quilt_meta_base);
        let entries = self
            .client
            .get(url)
            .send()
            .await?
            .error_for_status()?
            .json::<Vec<RawQuiltLoaderEntry>>()
            .await?;
        let recommended_index = entries
            .iter()
            .position(|entry| !entry.loader.version.contains('-'));
        Ok(entries
            .into_iter()
            .enumerate()
            .map(|(index, entry)| {
                let stable = !entry.loader.version.contains('-');
                FabricLoaderSummary {
                    version: entry.loader.version,
                    stable,
                    recommended: Some(index) == recommended_index,
                }
            })
            .collect())
    }

    pub async fn resolve_install_request(
        &self,
        selection: &InstallSelection,
    ) -> Result<ResolvedInstallRequest> {
        let game = self.resolve_game_version(&selection.game_version).await?;
        let loader = match &selection.loader {
            LoaderChoice::Vanilla => ResolvedLoader::Vanilla,
            LoaderChoice::Fabric { version } => {
                self.resolve_fabric_loader(&game.version.id, version)
                    .await?
            }
            LoaderChoice::Quilt { version } => {
                self.resolve_quilt_loader(&game.version.id, version).await?
            }
            LoaderChoice::Forge { version } => {
                self.resolve_forge_loader(&game.version.id, version).await?
            }
            LoaderChoice::NeoForge { version } => {
                self.resolve_neoforge_loader(&game.version.id, version)
                    .await?
            }
        };
        let java = self.resolve_zulu_jdk(game.java_major_version).await?;
        Ok(ResolvedInstallRequest {
            instance_name: selection.instance_name.clone(),
            game,
            loader,
            java,
            isolation: selection.isolation,
        })
    }

    async fn resolve_game_version(
        &self,
        version: &GameVersionSummary,
    ) -> Result<ResolvedGameVersion> {
        validate_https_url(&version.metadata_url)?;
        let payload = self
            .client
            .get(&version.metadata_url)
            .send()
            .await?
            .error_for_status()?
            .bytes()
            .await?;
        let mut sha1 = Sha1::new();
        Sha1Digest::update(&mut sha1, &payload);
        let actual_sha1 = encode_hex(Sha1Digest::finalize(sha1));
        if !actual_sha1.eq_ignore_ascii_case(&version.metadata_sha1) {
            return Err(CoreError::Metadata(format!(
                "Minecraft {} 元数据的 SHA-1 与官方目录不一致",
                version.id
            )));
        }

        let raw: RawVersionMetadata = serde_json::from_slice(&payload)?;
        if raw.id != version.id {
            return Err(CoreError::Metadata(format!(
                "版本目录请求 {}，但详情返回 {}",
                version.id, raw.id
            )));
        }
        let metadata: Value = serde_json::from_slice(&payload)?;
        let mut artifacts = Vec::new();
        artifacts.push(ResolvedArtifact {
            kind: ArtifactKind::VersionMetadata,
            relative_path: format!("minecraft/versions/{0}/{0}.json", version.id),
            url: version.metadata_url.clone(),
            size: u64::try_from(payload.len()).unwrap_or(u64::MAX),
            sha1: Some(actual_sha1),
            sha256: None,
            sha512: None,
        });
        artifacts.push(game_download_artifact(
            ArtifactKind::GameClient,
            format!("minecraft/versions/{0}/{0}.jar", version.id),
            raw.downloads.client,
        ));
        let asset_objects_total_bytes = raw.asset_index.total_size.unwrap_or(0);
        let asset_index_id = raw
            .asset_index
            .id
            .clone()
            .unwrap_or_else(|| version.id.clone());
        artifacts.push(game_download_artifact(
            ArtifactKind::AssetIndex,
            format!("minecraft/assets/indexes/{asset_index_id}.json"),
            raw.asset_index,
        ));
        for library in raw.libraries {
            if let Some(artifact) = library.downloads.and_then(|value| value.artifact) {
                artifacts.push(ResolvedArtifact {
                    kind: ArtifactKind::Library,
                    relative_path: format!("minecraft/libraries/{}", artifact.path),
                    url: artifact.url,
                    size: artifact.size,
                    sha1: Some(artifact.sha1),
                    sha256: None,
                    sha512: None,
                });
            }
        }
        if let Some(logging) = raw.logging {
            artifacts.push(ResolvedArtifact {
                kind: ArtifactKind::LoggingConfiguration,
                relative_path: format!("minecraft/assets/log_configs/{}", logging.client.file.id),
                url: logging.client.file.url,
                size: logging.client.file.size,
                sha1: Some(logging.client.file.sha1),
                sha256: None,
                sha512: None,
            });
        }

        Ok(ResolvedGameVersion {
            version: version.clone(),
            java_major_version: raw
                .java_version
                .map_or_else(|| infer_java_major(&version.id), |java| java.major_version),
            main_class: raw.main_class,
            metadata,
            artifacts,
            asset_objects_total_bytes,
        })
    }

    pub async fn resolve_fabric_loader(
        &self,
        game_version: &str,
        loader_version: &str,
    ) -> Result<ResolvedLoader> {
        validate_metadata_component(game_version, "Minecraft 版本")?;
        validate_metadata_component(loader_version, "Fabric Loader 版本")?;
        let compatible = self.compatible_fabric_loaders(game_version).await?;
        let selected = compatible
            .into_iter()
            .find(|candidate| candidate.version == loader_version)
            .ok_or_else(|| {
                CoreError::InvalidInstallRequest(format!(
                    "Fabric Loader {loader_version} 不在 Minecraft {game_version} 的兼容列表中"
                ))
            })?;
        let profile_url = format!(
            "{FABRIC_META_BASE_URL}/versions/loader/{game_version}/{loader_version}/profile/json"
        );
        let payload = self
            .client
            .get(&profile_url)
            .send()
            .await?
            .error_for_status()?
            .bytes()
            .await?;
        let mut sha256 = Sha256::new();
        Sha2Digest::update(&mut sha256, &payload);
        let profile_sha256 = encode_hex(Sha2Digest::finalize(sha256));
        let profile = serde_json::from_slice(&payload)?;
        Ok(ResolvedLoader::Fabric {
            version: selected.version,
            stable: selected.stable,
            profile_url,
            profile_sha256,
            profile,
        })
    }

    pub async fn resolve_quilt_loader(
        &self,
        game_version: &str,
        loader_version: &str,
    ) -> Result<ResolvedLoader> {
        validate_metadata_component(game_version, "Minecraft 版本")?;
        validate_metadata_component(loader_version, "Quilt Loader 版本")?;
        let compatible = self.compatible_quilt_loaders(game_version).await?;
        let selected = compatible
            .into_iter()
            .find(|candidate| candidate.version == loader_version)
            .ok_or_else(|| {
                CoreError::InvalidInstallRequest(format!(
                    "Quilt Loader {loader_version} 不在 Minecraft {game_version} 的兼容列表中"
                ))
            })?;
        let profile_url = format!(
            "{}/versions/loader/{game_version}/{loader_version}/profile/json",
            self.quilt_meta_base
        );
        let payload = self
            .client
            .get(&profile_url)
            .send()
            .await?
            .error_for_status()?
            .bytes()
            .await?;
        let mut sha256 = Sha256::new();
        Sha2Digest::update(&mut sha256, &payload);
        let profile_sha256 = encode_hex(Sha2Digest::finalize(sha256));
        let profile = serde_json::from_slice(&payload)?;
        Ok(ResolvedLoader::Quilt {
            version: selected.version,
            stable: selected.stable,
            profile_url,
            profile_sha256,
            profile,
        })
    }

    /// Forge 版本列表：BMCLAPI 按构建升序返回，推荐最新构建。
    pub async fn compatible_forge_versions(
        &self,
        game_version: &str,
    ) -> Result<Vec<FabricLoaderSummary>> {
        validate_metadata_component(game_version, "Minecraft 版本")?;
        let url = format!("{}/forge/minecraft/{game_version}", self.bmclapi_base);
        let entries = self
            .client
            .get(url)
            .send()
            .await?
            .error_for_status()?
            .json::<Vec<RawForgeVersion>>()
            .await?;
        let recommended_index = entries.len().checked_sub(1);
        Ok(entries
            .into_iter()
            .enumerate()
            .map(|(index, entry)| FabricLoaderSummary {
                version: entry.version,
                stable: true,
                recommended: Some(index) == recommended_index,
            })
            .collect())
    }

    /// NeoForge 版本列表：BMCLAPI 按版本升序返回，推荐最新版本。
    pub async fn compatible_neoforge_versions(
        &self,
        game_version: &str,
    ) -> Result<Vec<FabricLoaderSummary>> {
        validate_metadata_component(game_version, "Minecraft 版本")?;
        let url = format!("{}/neoforge/list/{game_version}", self.bmclapi_base);
        let entries = self
            .client
            .get(url)
            .send()
            .await?
            .error_for_status()?
            .json::<Vec<RawNeoForgeVersion>>()
            .await?;
        let recommended_index = entries.len().checked_sub(1);
        Ok(entries
            .into_iter()
            .enumerate()
            .map(|(index, entry)| FabricLoaderSummary {
                version: entry.version,
                stable: true,
                recommended: Some(index) == recommended_index,
            })
            .collect())
    }

    /// 解析 Forge 加载器：校验兼容列表后下载安装器并读取 spec-1 profile。
    pub async fn resolve_forge_loader(
        &self,
        game_version: &str,
        loader_version: &str,
    ) -> Result<ResolvedLoader> {
        validate_metadata_component(game_version, "Minecraft 版本")?;
        validate_metadata_component(loader_version, "Forge 版本")?;
        let compatible = self.compatible_forge_versions(game_version).await?;
        let selected = compatible
            .into_iter()
            .find(|candidate| candidate.version == loader_version)
            .ok_or_else(|| {
                CoreError::InvalidInstallRequest(format!(
                    "Forge {loader_version} 不在 Minecraft {game_version} 的兼容列表中"
                ))
            })?;
        let installer_url = format!(
            "{}/net/minecraftforge/forge/{game_version}-{loader_version}/forge-{game_version}-{loader_version}-installer.jar",
            self.forge_maven_base
        );
        let (profile, version_json, installer_sha1, installer_size) =
            self.download_installer(&installer_url).await?;
        validate_installer_target(&profile, game_version, &selected.version, "Forge")?;
        Ok(ResolvedLoader::Forge {
            version: selected.version,
            installer_url,
            installer_sha1,
            installer_size,
            install_profile: serde_json::to_value(profile)?,
            version_json,
        })
    }

    /// 解析 NeoForge 加载器：兼容列表提供安装器路径，下载后读取 spec-1 profile。
    pub async fn resolve_neoforge_loader(
        &self,
        game_version: &str,
        loader_version: &str,
    ) -> Result<ResolvedLoader> {
        validate_metadata_component(game_version, "Minecraft 版本")?;
        validate_metadata_component(loader_version, "NeoForge 版本")?;
        let url = format!("{}/neoforge/list/{game_version}", self.bmclapi_base);
        let entries = self
            .client
            .get(url)
            .send()
            .await?
            .error_for_status()?
            .json::<Vec<RawNeoForgeVersion>>()
            .await?;
        let selected = entries
            .into_iter()
            .find(|entry| entry.version == loader_version)
            .ok_or_else(|| {
                CoreError::InvalidInstallRequest(format!(
                    "NeoForge {loader_version} 不在 Minecraft {game_version} 的兼容列表中"
                ))
            })?;
        // BMCLAPI 的 installerPath 以 `/maven` 开头(相对镜像根),上游
        // maven.neoforged.net/releases 下没有这一段,直接拼接必然 404
        // (实测 21.1.233);镜像路由会把 /releases 重写回 /maven,两侧都正确。
        let installer_path = selected
            .installer_path
            .strip_prefix("/maven")
            .unwrap_or(&selected.installer_path);
        let installer_url = format!("{}{}", self.neoforge_maven_base, installer_path);
        let (profile, version_json, installer_sha1, installer_size) =
            self.download_installer(&installer_url).await?;
        validate_installer_target(&profile, game_version, &selected.version, "NeoForge")?;
        Ok(ResolvedLoader::NeoForge {
            version: selected.version,
            installer_url,
            installer_sha1,
            installer_size,
            install_profile: serde_json::to_value(profile)?,
            version_json,
        })
    }

    /// 下载安装器并在内存中解析 spec-1 profile,返回 profile、version.json 与校验信息。
    async fn download_installer(
        &self,
        url: &str,
    ) -> Result<(InstallProfile, serde_json::Value, String, u64)> {
        let payload = self
            .client
            .get(url)
            .send()
            .await?
            .error_for_status()?
            .bytes()
            .await?;
        let mut hasher = Sha1::new();
        Sha1Digest::update(&mut hasher, &payload);
        let installer_sha1 = encode_hex(Sha1Digest::finalize(hasher));
        let installer_size = payload.len() as u64;
        let (profile, version_json) = read_install_profile(std::io::Cursor::new(payload))?;
        Ok((profile, version_json, installer_sha1, installer_size))
    }

    pub async fn resolve_zulu_jdk(&self, java_major: u16) -> Result<ResolvedJavaPackage> {
        let list_url = format!(
            "{AZUL_META_BASE_URL}/packages/?java_version={java_major}&os=windows&arch=x86&archive_type=zip&java_package_type=jdk&release_status=ga&availability_types=CA&latest=true&page_size=1"
        );
        let packages = self
            .client
            .get(list_url)
            .send()
            .await?
            .error_for_status()?
            .json::<Vec<RawAzulPackageSummary>>()
            .await?;
        let package = packages.first().ok_or_else(|| {
            CoreError::Metadata(format!(
                "Azul 没有返回适用于 Windows x64 的 Java {java_major} JDK"
            ))
        })?;
        validate_metadata_component(&package.package_uuid, "Azul package UUID")?;
        let details_url = format!("{AZUL_META_BASE_URL}/packages/{}", package.package_uuid);
        let details = self
            .client
            .get(details_url)
            .send()
            .await?
            .error_for_status()?
            .json::<RawAzulPackageDetails>()
            .await?;
        if details.hw_bitness != 64 || details.java_version.first() != Some(&java_major) {
            return Err(CoreError::Metadata(
                "Azul 返回的 JDK 架构或 Java 主版本与请求不一致".to_owned(),
            ));
        }
        if details.size == 0 {
            return Err(CoreError::Metadata(
                "Azul 返回的 JDK 元数据大小无效".to_owned(),
            ));
        }
        let download_size = self
            .remote_content_length(&details.download_url, "Azul JDK")
            .await?;
        let full_version =
            format_java_version(&details.java_version, details.openjdk_build_number)?;
        Ok(ResolvedJavaPackage {
            distribution: JavaDistribution::AzulZulu,
            full_version,
            architecture: JavaArchitecture::X64,
            package_uuid: details.package_uuid,
            artifact: ResolvedArtifact {
                kind: ArtifactKind::JavaArchive,
                relative_path: format!("java/packages/{}", details.name),
                url: details.download_url,
                size: download_size,
                sha1: None,
                sha256: Some(details.sha256_hash),
                sha512: None,
            },
        })
    }

    async fn remote_content_length(&self, url: &str, artifact_name: &str) -> Result<u64> {
        validate_https_url(url)?;
        let response = self.client.head(url).send().await?.error_for_status()?;
        let value = response.headers().get(CONTENT_LENGTH).ok_or_else(|| {
            CoreError::Metadata(format!("{artifact_name} 下载端没有提供 Content-Length"))
        })?;
        let size = value
            .to_str()
            .map_err(|_| CoreError::Metadata(format!("{artifact_name} Content-Length 不是 ASCII")))?
            .parse::<u64>()
            .map_err(|_| {
                CoreError::Metadata(format!("{artifact_name} Content-Length 不是有效整数"))
            })?;
        if size == 0 {
            return Err(CoreError::Metadata(format!(
                "{artifact_name} Content-Length 不能为 0"
            )));
        }
        Ok(size)
    }
}

fn release_type(value: &str) -> GameReleaseType {
    match value {
        "release" => GameReleaseType::Release,
        "snapshot" => GameReleaseType::Snapshot,
        "old_beta" => GameReleaseType::OldBeta,
        "old_alpha" => GameReleaseType::OldAlpha,
        _ => GameReleaseType::Unknown,
    }
}

fn game_download_artifact(
    kind: ArtifactKind,
    relative_path: String,
    download: RawDownload,
) -> ResolvedArtifact {
    ResolvedArtifact {
        kind,
        relative_path,
        url: download.url,
        size: download.size,
        sha1: Some(download.sha1),
        sha256: None,
        sha512: None,
    }
}

fn infer_java_major(version: &str) -> u16 {
    let mut components = version
        .split('.')
        .filter_map(|part| part.parse::<u16>().ok());
    let major = components.next().unwrap_or(1);
    let minor = components.next().unwrap_or(0);
    let patch = components.next().unwrap_or(0);
    if major > 1 || minor > 20 || (minor == 20 && patch >= 5) {
        21
    } else if minor >= 18 {
        17
    } else if minor == 17 {
        16
    } else {
        8
    }
}

fn validate_metadata_component(value: &str, label: &str) -> Result<()> {
    if value.is_empty()
        || !value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || ".-_+".contains(character))
    {
        return Err(CoreError::InvalidInstallRequest(format!(
            "{label}包含不受支持的字符"
        )));
    }
    Ok(())
}

fn validate_https_url(value: &str) -> Result<()> {
    let url =
        Url::parse(value).map_err(|_| CoreError::Metadata("元数据地址不是有效 URL".to_owned()))?;
    if url.scheme() != "https" || url.host_str().is_none() {
        return Err(CoreError::Metadata(
            "元数据地址必须使用带主机名的 HTTPS URL".to_owned(),
        ));
    }
    Ok(())
}

fn format_java_version(components: &[u16], build: u16) -> Result<String> {
    if components.len() < 3 {
        return Err(CoreError::Metadata(
            "Azul 返回的 Java 完整版本格式无效".to_owned(),
        ));
    }
    Ok(format!(
        "{}.{}.{}+{build}",
        components[0], components[1], components[2]
    ))
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
