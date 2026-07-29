//! CurseForge 官方 API 客户端（api.curseforge.com/v1）。
//!
//! 用户在设置 → 来源 中配置自己的 CurseForge API Key（只存本机
//! app_settings，不上传、不写入日志）；配置后资源中心可直接浏览、搜索与
//! 安装 CurseForge 官方源内容。未配置时所有调用如实报错并给出去向提示，
//! CurseForge 内容仍可由 MCI Mirror 内置镜像链路（整合包 manifest 解析等）
//! 提供。Key 只经 `x-api-key` 请求头发往 api.curseforge.com，下载 CDN
//! （edge.forgecdn.net）不携带 Key。

use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::Duration,
};

use futures_util::StreamExt;
use reqwest::{Client, StatusCode, Url};
use rusqlite::params;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sha1::{Digest, Sha1};
use uuid::Uuid;

use crate::{AppService, CoreError, Result, read_setting, write_setting};

const CURSEFORGE_API_BASE: &str = "https://api.curseforge.com/v1/";
/// Minecraft 在 CurseForge 的固定游戏 ID（官方文档示例口径）。
const CURSEFORGE_GAME_ID: u32 = 432;
const SETTING_CURSEFORGE_API_KEY: &str = "curseforge_api_key";
/// ForgeCDN 下载兜底：部分文件 `downloadUrl` 为空（作者关闭第三方分发），
/// 按 PCL-CE `HandleCurseForgeDownloadUrls` 的规则
/// `/files/{fileId / 1000}/{fileId % 1000}/{fileName}` 拼装 edge 地址。
const FORGECDN_EDGE_BASE: &str = "https://edge.forgecdn.net";

/// CurseForge 内容分类 classId，口径以官方 REST 文档 Get Categories 与
/// Prism Launcher 集成为准（Mods=6，Resource Packs=12，Modpacks=4471，
/// Shaders=6552）。
pub const CURSEFORGE_CLASS_MOD: u32 = 6;
pub const CURSEFORGE_CLASS_RESOURCE_PACK: u32 = 12;
pub const CURSEFORGE_CLASS_MODPACK: u32 = 4471;
pub const CURSEFORGE_CLASS_SHADER: u32 = 6552;

/// 目录来源：资源中心统一结构里标注项目来自哪个平台。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CatalogProjectSource {
    Modrinth,
    Curseforge,
}

/// 统一目录项目摘要（CurseForge 数字 ID 转字符串，与 Modrinth 摘要同形）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogProjectSummary {
    pub project_id: String,
    pub title: String,
    pub slug: String,
    pub author: Option<String>,
    pub description: String,
    pub icon_url: Option<String>,
    pub downloads: u64,
    pub date_modified: Option<String>,
    pub game_versions: Vec<String>,
    pub categories: Vec<String>,
    pub source: CatalogProjectSource,
}

/// 统一目录搜索分页。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogSearchPage {
    pub hits: Vec<CatalogProjectSummary>,
    pub index: u32,
    pub page_size: u32,
    pub total_count: u64,
}

/// CurseForge 搜索排序字段（ModsSearchSortField 枚举值）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum CurseforgeSortField {
    Featured,
    #[default]
    Popularity,
    LastUpdated,
    Name,
    TotalDownloads,
}

impl CurseforgeSortField {
    const fn api_value(self) -> u8 {
        match self {
            Self::Featured => 1,
            Self::Popularity => 2,
            Self::LastUpdated => 3,
            Self::Name => 4,
            Self::TotalDownloads => 6,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum CurseforgeSortOrder {
    Asc,
    #[default]
    Desc,
}

impl CurseforgeSortOrder {
    const fn api_value(self) -> &'static str {
        match self {
            Self::Asc => "asc",
            Self::Desc => "desc",
        }
    }
}

/// CurseForge 目录搜索条件。`mod_loader` 按官方文档必须与游戏版本搭配，
/// 仅在提供游戏版本时下发。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CurseforgeSearchQuery {
    #[serde(default)]
    pub query: String,
    pub class_id: u32,
    #[serde(default)]
    pub game_version: Option<String>,
    #[serde(default)]
    pub category_id: Option<u32>,
    #[serde(default)]
    pub mod_loader: Option<String>,
    #[serde(default)]
    pub sort_field: CurseforgeSortField,
    #[serde(default)]
    pub sort_order: CurseforgeSortOrder,
    #[serde(default)]
    pub index: u32,
    #[serde(default = "default_page_size")]
    pub page_size: u32,
}

const fn default_page_size() -> u32 {
    20
}

/// CurseForge 文件归一化摘要（与 Modrinth 版本摘要同形，额外携带下载与
/// 校验信息；`sha1` 为 None 表示来源未提供校验值，下载按大小校验）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CurseforgeFileSummary {
    pub id: String,
    pub version_number: String,
    /// release / beta / alpha（releaseType 1/2/3）。
    pub version_type: String,
    pub date_published: String,
    pub game_versions: Vec<String>,
    pub loaders: Vec<String>,
    pub downloads: u64,
    pub file_name: String,
    pub size: u64,
    pub sha1: Option<String>,
    /// 官方返回的下载地址；None 时按 ForgeCDN edge 规则兜底。
    pub download_url: Option<String>,
}

/// CurseForge 内容分类（Get Categories 返回，id 由 API 下发，不硬编码）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CurseforgeCategory {
    pub id: u32,
    pub name: String,
    pub slug: String,
}

/// CurseForge 官方 API 客户端；每次调用按需携带 `x-api-key`。
#[derive(Debug, Clone)]
pub struct CurseForgeClient {
    client: Client,
    base_url: Url,
    api_key: Option<String>,
}

impl CurseForgeClient {
    pub fn new(api_key: Option<String>) -> Result<Self> {
        Self::with_base_url(CURSEFORGE_API_BASE, api_key)
    }

    pub fn with_base_url(base_url: &str, api_key: Option<String>) -> Result<Self> {
        let mut base_url = Url::parse(base_url)
            .map_err(|error| CoreError::Content(format!("CurseForge API 地址无效：{error}")))?;
        if !base_url.path().ends_with('/') {
            base_url.set_path(&format!("{}/", base_url.path()));
        }
        let localhost = matches!(base_url.host_str(), Some("127.0.0.1" | "localhost"));
        if base_url.scheme() != "https" && !(base_url.scheme() == "http" && localhost) {
            return Err(CoreError::Content(
                "CurseForge API 地址必须使用 https".to_owned(),
            ));
        }
        let user_agent = format!(
            "SakuraRed/MoyuMax/{} (github.com/SakuraRed/MoyuMax)",
            env!("CARGO_PKG_VERSION")
        );
        let client = crate::http_client_builder()
            .connect_timeout(Duration::from_secs(10))
            // 同一客户端承担大体积整合包/文件下载，总时限放宽到 10 分钟。
            .timeout(Duration::from_secs(600))
            .user_agent(user_agent)
            .build()
            .map_err(CoreError::from)?;
        let api_key = api_key
            .map(|key| key.trim().to_owned())
            .filter(|key| !key.is_empty());
        Ok(Self {
            client,
            base_url,
            api_key,
        })
    }

    /// Key 缺失时所有调用统一报错并给出去向提示。
    fn require_key(&self) -> Result<&str> {
        self.api_key.as_deref().ok_or_else(|| {
            CoreError::Content(
                "未配置 CurseForge API Key：请在设置 → 来源 中配置；未配置时 CurseForge 内容经 MCI Mirror 内置镜像提供"
                    .to_owned(),
            )
        })
    }

    /// 目录搜索：GET /mods/search，gameId=432 固定。
    pub async fn search(&self, query: &CurseforgeSearchQuery) -> Result<CatalogSearchPage> {
        if query.page_size == 0 || query.page_size > 50 {
            return Err(CoreError::Content(
                "CurseForge 搜索分页大小必须在 1-50 之间".to_owned(),
            ));
        }
        let mut url = self.endpoint("mods/search")?;
        {
            let mut pairs = url.query_pairs_mut();
            pairs.append_pair("gameId", &CURSEFORGE_GAME_ID.to_string());
            pairs.append_pair("classId", &query.class_id.to_string());
            if !query.query.trim().is_empty() {
                pairs.append_pair("searchFilter", query.query.trim());
            }
            let game_version = query
                .game_version
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty());
            if let Some(game_version) = game_version {
                pairs.append_pair("gameVersion", game_version);
            }
            if let Some(category_id) = query.category_id {
                pairs.append_pair("categoryId", &category_id.to_string());
            }
            // 官方文档：modLoaderType 必须与 gameVersion 搭配。
            if game_version.is_some()
                && let Some(loader) = query.mod_loader.as_deref().and_then(mod_loader_type)
            {
                pairs.append_pair("modLoaderType", &loader.to_string());
            }
            pairs.append_pair("sortField", &query.sort_field.api_value().to_string());
            pairs.append_pair("sortOrder", query.sort_order.api_value());
            pairs.append_pair("index", &query.index.to_string());
            pairs.append_pair("pageSize", &query.page_size.to_string());
        }
        let page: CfSearchPage = self.get_json(url).await?;
        Ok(CatalogSearchPage {
            hits: page.data.into_iter().map(normalize_mod).collect(),
            index: page.pagination.index,
            page_size: page.pagination.page_size,
            total_count: page.pagination.total_count,
        })
    }

    /// 项目全部文件：GET /mods/{id}/files 分页全取后归一化。
    pub async fn project_files(
        &self,
        project_id: &str,
        game_version: Option<&str>,
        loader: Option<&str>,
    ) -> Result<Vec<CurseforgeFileSummary>> {
        let project_id = numeric_id(project_id, "CurseForge 项目 ID")?;
        let mut files = Vec::new();
        let mut index = 0_u32;
        loop {
            let mut url = self.endpoint(&format!("mods/{project_id}/files"))?;
            {
                let mut pairs = url.query_pairs_mut();
                if let Some(game_version) = game_version.filter(|value| !value.trim().is_empty()) {
                    pairs.append_pair("gameVersion", game_version);
                }
                if let Some(loader_type) = loader.and_then(mod_loader_type) {
                    pairs.append_pair("modLoaderType", &loader_type.to_string());
                }
                pairs.append_pair("index", &index.to_string());
                pairs.append_pair("pageSize", "50");
            }
            let page: CfFilesPage = self.get_json(url).await?;
            let fetched = page.data.len() as u64;
            files.extend(page.data.into_iter().map(normalize_file));
            let total = page.pagination.total_count;
            index += 50;
            // 官方上限 index + pageSize <= 10000；空页防护死循环。
            if files.len() as u64 >= total || fetched == 0 || index >= 10_000 {
                break;
            }
        }
        Ok(files)
    }

    /// 单个文件元数据：GET /mods/{id}/files/{fileId}。
    pub async fn file_summary(
        &self,
        project_id: &str,
        file_id: &str,
    ) -> Result<CurseforgeFileSummary> {
        let project_id = numeric_id(project_id, "CurseForge 项目 ID")?;
        let file_id = numeric_id(file_id, "CurseForge 文件 ID")?;
        let url = self.endpoint(&format!("mods/{project_id}/files/{file_id}"))?;
        let body: CfFileResponse = self.get_json(url).await?;
        Ok(normalize_file(body.data))
    }

    /// 文件下载地址：GET /mods/{id}/files/{fileId}/download-url；
    /// 返回 204/空体或空串时按 ForgeCDN edge 规则兜底。
    pub async fn file_download_url(&self, project_id: &str, file_id: &str) -> Result<String> {
        let key = self.require_key()?;
        let project = numeric_id(project_id, "CurseForge 项目 ID")?;
        let file = numeric_id(file_id, "CurseForge 文件 ID")?;
        let url = self.endpoint(&format!("mods/{project}/files/{file}/download-url"))?;
        let response = self
            .client
            .get(url)
            .header("x-api-key", key)
            .send()
            .await
            .map_err(|error| CoreError::Content(format!("无法连接 CurseForge：{error}")))?;
        if !response.status().is_success() {
            return Err(map_http_error(response.status(), "download-url"));
        }
        // 204/空体不能走 .json()，先读文本再尝试解析。
        let text = response
            .text()
            .await
            .map_err(|error| CoreError::Content(format!("无法读取 CurseForge 响应：{error}")))?;
        let direct = serde_json::from_str::<CfDownloadUrlResponse>(&text)
            .ok()
            .map(|body| body.data)
            .filter(|url| !url.trim().is_empty());
        if let Some(url) = direct {
            return Ok(url);
        }
        let summary = self.file_summary(project_id, file_id).await?;
        resolved_download_url(&summary)
    }

    /// 项目详情：GET /mods/{id}。
    pub async fn project_summary(&self, project_id: &str) -> Result<CatalogProjectSummary> {
        let project_id = numeric_id(project_id, "CurseForge 项目 ID")?;
        let url = self.endpoint(&format!("mods/{project_id}"))?;
        let body: CfModResponse = self.get_json(url).await?;
        Ok(normalize_mod(body.data))
    }

    /// 分类列表：GET /categories?gameId=432&classId=…（id 由 API 下发）。
    pub async fn categories(&self, class_id: u32) -> Result<Vec<CurseforgeCategory>> {
        let mut url = self.endpoint("categories")?;
        url.query_pairs_mut()
            .append_pair("gameId", &CURSEFORGE_GAME_ID.to_string())
            .append_pair("classId", &class_id.to_string());
        let body: CfCategoriesResponse = self.get_json(url).await?;
        Ok(body
            .data
            .into_iter()
            .filter(|category| !category.is_class)
            .map(|category| CurseforgeCategory {
                id: category.id,
                name: category.name,
                slug: category.slug,
            })
            .collect())
    }

    /// 最新可下载文件：按发布日期倒序后正式版优先（在线安装默认选版）。
    pub async fn latest_project_file(
        &self,
        project_id: &str,
        game_version: Option<&str>,
        loader: Option<&str>,
    ) -> Result<CurseforgeFileSummary> {
        let mut files = self.project_files(project_id, game_version, loader).await?;
        files.sort_by(|left, right| right.date_published.cmp(&left.date_published));
        files
            .iter()
            .find(|file| file.version_type == "release")
            .or(files.first())
            .cloned()
            .ok_or_else(|| CoreError::Content("该项目没有匹配当前条件的文件".to_owned()))
    }

    /// 验证 Key 有效：GET /games/432，成功返回游戏名。
    pub async fn verify_key(&self) -> Result<String> {
        let url = self.endpoint(&format!("games/{CURSEFORGE_GAME_ID}"))?;
        let body: CfGameResponse = self.get_json(url).await?;
        Ok(body.data.name)
    }

    /// 流式下载文件到目标目录：有 SHA-1 按 SHA-1 校验，否则按大小校验，
    /// 成功后原子改名返回最终路径。
    pub async fn download_file(
        &self,
        file: &CurseforgeFileSummary,
        destination_directory: &Path,
    ) -> Result<PathBuf> {
        if file.file_name.is_empty()
            || file.file_name.contains(['/', '\\'])
            || file.file_name == "."
            || file.file_name == ".."
        {
            return Err(CoreError::Content("服务端返回的文件名无效".to_owned()));
        }
        fs::create_dir_all(destination_directory)?;
        let staging = destination_directory.join(format!("{}.part", Uuid::new_v4().simple()));
        let target = destination_directory.join(&file.file_name);
        let outcome = self.download_to_staging(file, &staging).await;
        match outcome {
            Ok(()) => {
                fs::rename(&staging, &target)?;
                Ok(target)
            }
            Err(error) => {
                let _ = fs::remove_file(&staging);
                Err(error)
            }
        }
    }

    /// 自由下载：按自定义文件名保存（同名拒绝、不覆盖），校验规则同
    /// `download_file`。
    pub async fn download_file_as(
        &self,
        file: &CurseforgeFileSummary,
        target_dir: &Path,
        file_name: &str,
    ) -> Result<PathBuf> {
        let trimmed = file_name.trim();
        if trimmed.is_empty()
            || trimmed.contains(['/', '\\'])
            || trimmed == "."
            || trimmed == ".."
            || trimmed.bytes().any(|byte| byte < 0x20)
        {
            return Err(CoreError::Content("保存文件名无效".to_owned()));
        }
        let target = target_dir.join(trimmed);
        if target.exists() {
            return Err(CoreError::Content(format!(
                "同名文件 {trimmed} 已存在，已拒绝下载且未覆盖"
            )));
        }
        let staging_dir = target_dir.join(format!(".moyumax-dl-{}", Uuid::new_v4().simple()));
        let download_outcome = async {
            fs::create_dir_all(&staging_dir)?;
            self.download_file(file, &staging_dir).await
        }
        .await;
        let downloaded = match download_outcome {
            Ok(path) => path,
            Err(error) => {
                let _ = fs::remove_dir_all(&staging_dir);
                return Err(error);
            }
        };
        if target.exists() {
            let _ = fs::remove_dir_all(&staging_dir);
            return Err(CoreError::Content(format!(
                "同名文件 {trimmed} 已存在，已拒绝下载且未覆盖"
            )));
        }
        fs::rename(&downloaded, &target)?;
        let _ = fs::remove_dir_all(&staging_dir);
        Ok(target)
    }

    async fn download_to_staging(
        &self,
        file: &CurseforgeFileSummary,
        staging: &Path,
    ) -> Result<()> {
        let url = resolved_download_url(file)?;
        // CDN 不需要也不应携带 API Key。
        let response =
            self.client.get(&url).send().await.map_err(|error| {
                CoreError::Content(format!("无法下载 CurseForge 文件：{error}"))
            })?;
        if !response.status().is_success() {
            return Err(CoreError::Content(format!(
                "CurseForge 文件下载失败（HTTP {}）",
                response.status()
            )));
        }
        let mut hasher = Sha1::new();
        let mut written = 0_u64;
        let mut writer = std::io::BufWriter::new(fs::File::create(staging)?);
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk
                .map_err(|error| CoreError::Content(format!("CurseForge 文件下载中断：{error}")))?;
            hasher.update(&chunk);
            writer.write_all(&chunk)?;
            written += chunk.len() as u64;
        }
        writer.flush()?;
        if let Some(expected) = &file.sha1 {
            let digest = encode_hex(hasher.finalize());
            if !digest.eq_ignore_ascii_case(expected) {
                return Err(CoreError::Content(
                    "CurseForge 文件 SHA-1 校验失败，已拒绝使用".to_owned(),
                ));
            }
        } else if file.size > 0 && written != file.size {
            // 来源未提供校验值时如实按大小校验。
            return Err(CoreError::Content(format!(
                "CurseForge 文件大小不一致：期望 {} 字节，实际 {} 字节",
                file.size, written
            )));
        }
        Ok(())
    }

    fn endpoint(&self, path: &str) -> Result<Url> {
        self.base_url
            .join(path)
            .map_err(|error| CoreError::Content(format!("CurseForge API 路径无效：{error}")))
    }

    async fn get_json<T: DeserializeOwned>(&self, url: Url) -> Result<T> {
        let key = self.require_key()?;
        let response = self
            .client
            .get(url.clone())
            .header("x-api-key", key)
            .send()
            .await
            .map_err(|error| CoreError::Content(format!("无法连接 CurseForge：{error}")))?;
        let status = response.status();
        if status.is_success() {
            return response.json().await.map_err(CoreError::from);
        }
        Err(map_http_error(status, url.path()))
    }
}

impl AppService {
    /// 本机保存的 CurseForge API Key（仅本机使用，不上传、不写入日志）。
    pub fn curseforge_api_key(&self) -> Result<Option<String>> {
        let connection = self.connection()?;
        Ok(read_setting(&connection, SETTING_CURSEFORGE_API_KEY)?
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty()))
    }

    /// 保存 Key；空白输入视为清除。明文存本机 app_settings，界面打码展示。
    pub fn set_curseforge_api_key(&self, key: &str) -> Result<()> {
        let connection = self.connection()?;
        let trimmed = key.trim();
        if trimmed.is_empty() {
            connection.execute(
                "DELETE FROM app_settings WHERE key = ?1",
                params![SETTING_CURSEFORGE_API_KEY],
            )?;
        } else {
            write_setting(&connection, SETTING_CURSEFORGE_API_KEY, trimmed)?;
        }
        Ok(())
    }

    /// 用当前保存的 Key 调 GET /games/432 验证有效性；成功返回游戏名。
    pub async fn test_curseforge_key(&self) -> Result<String> {
        let client = CurseForgeClient::new(self.curseforge_api_key()?)?;
        client.verify_key().await
    }
}

fn map_http_error(status: StatusCode, path: &str) -> CoreError {
    match status {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => CoreError::Content(
            "CurseForge API Key 无效或已过期，请在设置 → 来源 中重新配置".to_owned(),
        ),
        StatusCode::TOO_MANY_REQUESTS => {
            CoreError::Content("CurseForge 请求达到限流，请稍后重试".to_owned())
        }
        StatusCode::NOT_FOUND => CoreError::Content("CurseForge 资源不存在（HTTP 404）".to_owned()),
        status => CoreError::Content(format!("CurseForge 请求 {path} 返回 HTTP {status}")),
    }
}

fn numeric_id(value: &str, label: &str) -> Result<u64> {
    value
        .trim()
        .parse::<u64>()
        .map_err(|_| CoreError::Content(format!("{label}必须是数字：{value}")))
}

/// ForgeCDN edge 兜底地址（downloadUrl 为空时）。
fn edge_download_url(file_id: u64, file_name: &str) -> String {
    format!(
        "{FORGECDN_EDGE_BASE}/files/{}/{}/{}",
        file_id / 1000,
        file_id % 1000,
        file_name
    )
}

fn resolved_download_url(file: &CurseforgeFileSummary) -> Result<String> {
    if let Some(url) = &file.download_url
        && !url.trim().is_empty()
    {
        return Ok(url.clone());
    }
    let file_id = file.id.parse::<u64>().ok().filter(|id| *id > 0);
    match (file_id, file.file_name.is_empty()) {
        (Some(file_id), false) => Ok(edge_download_url(file_id, &file.file_name)),
        _ => Err(CoreError::Content(
            "CurseForge 文件没有可用下载地址".to_owned(),
        )),
    }
}

/// ModLoaderType 枚举：Forge=1，Cauldron=2，LiteLoader=3，Fabric=4，
/// Quilt=5，NeoForge=6；不认识的加载器不下发过滤参数。
fn mod_loader_type(loader: &str) -> Option<u8> {
    match loader.trim().to_ascii_lowercase().as_str() {
        "forge" => Some(1),
        "cauldron" => Some(2),
        "liteloader" => Some(3),
        "fabric" => Some(4),
        "quilt" => Some(5),
        "neoforge" => Some(6),
        _ => None,
    }
}

/// CF 文件的 gameVersions 数组混装 MC 版本与加载器名：forge/fabric/
/// quilt/neoforge 识别为加载器，其余含 "." 的按 MC 版本，剩下的
/// （Java 21、Client/Server、无点快照号等）忽略。
fn split_game_versions(values: &[String]) -> (Vec<String>, Vec<String>) {
    let mut versions = Vec::new();
    let mut loaders = Vec::new();
    for value in values {
        let lowered = value.to_ascii_lowercase();
        if matches!(lowered.as_str(), "forge" | "fabric" | "quilt" | "neoforge") {
            if !loaders.contains(&lowered) {
                loaders.push(lowered);
            }
        } else if value.contains('.') && !versions.contains(value) {
            versions.push(value.clone());
        }
    }
    (versions, loaders)
}

fn normalize_mod(data: CfMod) -> CatalogProjectSummary {
    let mut game_versions = Vec::new();
    for index in &data.latest_files_indexes {
        if !index.game_version.is_empty() && !game_versions.contains(&index.game_version) {
            game_versions.push(index.game_version.clone());
        }
    }
    CatalogProjectSummary {
        project_id: data.id.to_string(),
        title: data.name,
        slug: data.slug,
        author: data.authors.first().map(|author| author.name.clone()),
        description: data.summary,
        icon_url: data
            .logo
            .and_then(|logo| logo.thumbnail_url.or(logo.url))
            .filter(|url| !url.trim().is_empty()),
        downloads: data.download_count,
        date_modified: Some(data.date_modified).filter(|value| !value.is_empty()),
        game_versions,
        categories: data
            .categories
            .into_iter()
            .map(|category| category.name)
            .collect(),
        source: CatalogProjectSource::Curseforge,
    }
}

fn normalize_file(data: CfFile) -> CurseforgeFileSummary {
    let sha1 = data
        .hashes
        .iter()
        .find(|hash| hash.algo == 1)
        .map(|hash| hash.value.clone())
        .filter(|value| !value.is_empty());
    let (game_versions, loaders) = split_game_versions(&data.game_versions);
    CurseforgeFileSummary {
        id: data.id.to_string(),
        version_number: if data.display_name.trim().is_empty() {
            data.file_name.clone()
        } else {
            data.display_name
        },
        version_type: match data.release_type {
            1 => "release",
            2 => "beta",
            3 => "alpha",
            // 文档只定义 1/2/3；未知值按正式版处理，避免界面误标预发布。
            _ => "release",
        }
        .to_owned(),
        date_published: data.file_date,
        game_versions,
        loaders,
        downloads: data.download_count,
        file_name: data.file_name,
        size: data.file_length,
        sha1,
        download_url: data.download_url.filter(|url| !url.trim().is_empty()),
    }
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

#[derive(Debug, Deserialize)]
struct CfPagination {
    index: u32,
    #[serde(rename = "pageSize")]
    page_size: u32,
    #[serde(rename = "totalCount")]
    total_count: u64,
}

#[derive(Debug, Deserialize)]
struct CfSearchPage {
    data: Vec<CfMod>,
    pagination: CfPagination,
}

#[derive(Debug, Deserialize)]
struct CfFilesPage {
    data: Vec<CfFile>,
    pagination: CfPagination,
}

#[derive(Debug, Deserialize)]
struct CfModResponse {
    data: CfMod,
}

#[derive(Debug, Deserialize)]
struct CfFileResponse {
    data: CfFile,
}

#[derive(Debug, Deserialize)]
struct CfGameResponse {
    data: CfGame,
}

#[derive(Debug, Deserialize)]
struct CfGame {
    name: String,
}

#[derive(Debug, Deserialize)]
struct CfCategoriesResponse {
    data: Vec<CfCategory>,
}

#[derive(Debug, Deserialize)]
struct CfCategory {
    id: u32,
    name: String,
    slug: String,
    #[serde(rename = "isClass", default)]
    is_class: bool,
}

#[derive(Debug, Deserialize)]
struct CfDownloadUrlResponse {
    data: String,
}

#[derive(Debug, Deserialize)]
struct CfMod {
    id: u64,
    name: String,
    slug: String,
    #[serde(default)]
    summary: String,
    #[serde(rename = "downloadCount", default)]
    download_count: u64,
    #[serde(rename = "dateModified", default)]
    date_modified: String,
    #[serde(default)]
    logo: Option<CfModLogo>,
    #[serde(default)]
    authors: Vec<CfAuthor>,
    #[serde(default)]
    categories: Vec<CfCategoryName>,
    #[serde(rename = "latestFilesIndexes", default)]
    latest_files_indexes: Vec<CfFileIndex>,
}

#[derive(Debug, Deserialize)]
struct CfModLogo {
    #[serde(rename = "thumbnailUrl")]
    thumbnail_url: Option<String>,
    url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CfAuthor {
    name: String,
}

#[derive(Debug, Deserialize)]
struct CfCategoryName {
    name: String,
}

#[derive(Debug, Deserialize)]
struct CfFileIndex {
    #[serde(rename = "gameVersion")]
    game_version: String,
}

#[derive(Debug, Deserialize)]
struct CfFile {
    id: u64,
    #[serde(rename = "displayName", default)]
    display_name: String,
    #[serde(rename = "fileName")]
    file_name: String,
    #[serde(rename = "releaseType")]
    release_type: u8,
    #[serde(rename = "fileDate", default)]
    file_date: String,
    #[serde(rename = "fileLength", default)]
    file_length: u64,
    #[serde(rename = "downloadCount", default)]
    download_count: u64,
    #[serde(rename = "downloadUrl")]
    download_url: Option<String>,
    #[serde(rename = "gameVersions", default)]
    game_versions: Vec<String>,
    #[serde(default)]
    hashes: Vec<CfFileHash>,
}

#[derive(Debug, Deserialize)]
struct CfFileHash {
    algo: u8,
    value: String,
}
