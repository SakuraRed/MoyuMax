use std::{
    collections::{HashMap, HashSet, VecDeque},
    fs,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    time::Duration,
};

use futures_util::StreamExt;
use reqwest::{Client, StatusCode, Url};
use rusqlite::{OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sha1::{Digest, Sha1};
use sha2::Sha512;
use uuid::Uuid;

use crate::{
    AppService, CoreError, ManagedInstanceSummary, RecoveryDecision, Result, TaskProgress,
    TaskState, unix_timestamp,
};

const MODRINTH_API_BASE: &str = "https://api.modrinth.com/v2/";
pub(crate) const CONTENT_COMMIT_JOURNAL_NAME: &str = "commit-journal.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ModrinthSearchIndex {
    Relevance,
    Downloads,
    Follows,
    Newest,
    Updated,
}

impl ModrinthSearchIndex {
    const fn api_value(self) -> &'static str {
        match self {
            Self::Relevance => "relevance",
            Self::Downloads => "downloads",
            Self::Follows => "follows",
            Self::Newest => "newest",
            Self::Updated => "updated",
        }
    }
}

/// Modrinth 项目类型；搜索与目录浏览按此过滤。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ModrinthProjectType {
    #[default]
    Mod,
    Modpack,
    Shader,
    Resourcepack,
}

impl ModrinthProjectType {
    fn api_value(self) -> &'static str {
        match self {
            Self::Mod => "mod",
            Self::Modpack => "modpack",
            Self::Shader => "shader",
            Self::Resourcepack => "resourcepack",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModrinthSearchQuery {
    pub query: String,
    pub game_version: String,
    pub loader: String,
    pub index: ModrinthSearchIndex,
    pub offset: u32,
    pub limit: u8,
    /// 搜索的项目类型；缺省为模组（兼容 M5 调用方）。
    #[serde(default)]
    pub project_type: ModrinthProjectType,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModrinthProjectSummary {
    #[serde(alias = "project_id")]
    pub project_id: String,
    pub slug: String,
    pub title: String,
    pub description: String,
    pub downloads: u64,
    #[serde(alias = "client_side")]
    pub client_side: String,
    #[serde(alias = "server_side")]
    pub server_side: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModrinthSearchPage {
    pub hits: Vec<ModrinthProjectSummary>,
    pub offset: u32,
    pub limit: u32,
    #[serde(alias = "total_hits")]
    pub total_hits: u64,
}

/// 在线项目版本的主文件（整合包/光影/资源包安装用）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModrinthVersionFile {
    pub url: String,
    pub filename: String,
    pub sha1: String,
    pub sha512: String,
    pub size: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ContentDependencyKind {
    Required,
    Optional,
    Incompatible,
    Embedded,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentDependencyChoice {
    pub project_id: Option<String>,
    pub version_id: Option<String>,
    pub title: String,
    pub kind: ContentDependencyKind,
    pub required_by_project_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentFilePlan {
    pub url: String,
    pub filename: String,
    pub size: u64,
    pub sha1: String,
    pub sha512: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentPlanEntry {
    pub project_id: String,
    pub version_id: String,
    pub project_title: String,
    pub version_number: String,
    pub required_by_project_id: Option<String>,
    pub file: ContentFilePlan,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentInstallPlan {
    pub schema_version: u16,
    pub instance_id: String,
    pub instance_name: String,
    pub game_version: String,
    pub loader: String,
    pub root_project_id: String,
    pub entries: Vec<ContentPlanEntry>,
    pub optional_dependencies: Vec<ContentDependencyChoice>,
    pub incompatible_dependencies: Vec<ContentDependencyChoice>,
    /// 更新计划：允许同名异哈希替换,替换前把旧文件移入实例快照区。
    #[serde(default)]
    pub is_update: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ContentInstallStage {
    Prepare,
    DownloadFiles,
    VerifyFiles,
    CommitFiles,
    IndexContent,
}

impl ContentInstallStage {
    const fn database_value(self) -> &'static str {
        match self {
            Self::Prepare => "prepare",
            Self::DownloadFiles => "download_files",
            Self::VerifyFiles => "verify_files",
            Self::CommitFiles => "commit_files",
            Self::IndexContent => "index_content",
        }
    }

    fn from_database(value: &str) -> Result<Self> {
        match value {
            "prepare" => Ok(Self::Prepare),
            "download_files" => Ok(Self::DownloadFiles),
            "verify_files" => Ok(Self::VerifyFiles),
            "commit_files" => Ok(Self::CommitFiles),
            "index_content" => Ok(Self::IndexContent),
            _ => Err(CoreError::InvalidStoredState(format!(
                "未知内容安装阶段：{value}"
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentInstallTask {
    pub id: String,
    pub state: TaskState,
    pub current_stage: Option<ContentInstallStage>,
    pub plan: ContentInstallPlan,
    pub staging_directory: String,
    pub target_directory: String,
    pub shared_store_directory: String,
    pub created_at_unix_seconds: i64,
    pub updated_at_unix_seconds: i64,
    #[serde(default)]
    pub priority: i64,
    #[serde(default)]
    pub paused_by: Option<String>,
    pub progress: TaskProgress,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ContentProvider {
    Modrinth,
}

impl ContentProvider {
    const fn database_value(self) -> &'static str {
        match self {
            Self::Modrinth => "modrinth",
        }
    }

    fn from_database(value: &str) -> Result<Self> {
        match value {
            "modrinth" => Ok(Self::Modrinth),
            _ => Err(CoreError::InvalidStoredState(format!(
                "未知内容来源：{value}"
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledContent {
    pub id: String,
    pub instance_id: String,
    pub provider: ContentProvider,
    pub project_id: String,
    pub version_id: String,
    pub project_title: String,
    pub version_number: String,
    pub file_name: String,
    pub relative_path: String,
    pub size: u64,
    pub sha1: String,
    pub sha512: String,
    pub enabled: bool,
    pub auto_update_enabled: bool,
    pub installed_at_unix_seconds: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentUpdateInfo {
    pub project_id: String,
    pub project_title: String,
    pub current_version_id: String,
    pub current_version_number: String,
    pub latest_version_id: String,
    pub latest_version_number: String,
    pub file: ContentFilePlan,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ContentCommitJournal {
    pub schema_version: u16,
    pub entries: Vec<ContentCommitJournalEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ContentCommitJournalEntry {
    pub file_name: String,
    pub existed_before: bool,
}

#[derive(Debug, Clone)]
pub struct ModrinthClient {
    client: Client,
    base_url: Url,
}

impl ModrinthClient {
    pub fn new() -> Result<Self> {
        Self::with_base_url(MODRINTH_API_BASE)
    }

    pub fn with_base_url(base_url: &str) -> Result<Self> {
        let base_url = Url::parse(base_url)
            .map_err(|error| CoreError::Content(format!("Modrinth API 地址无效：{error}")))?;
        if !matches!(base_url.scheme(), "https" | "http")
            || base_url.host_str().is_none()
            || !base_url.path().ends_with('/')
        {
            return Err(CoreError::Content(
                "Modrinth API 地址必须是带主机名且以斜杠结尾的 HTTP(S) URL".to_owned(),
            ));
        }
        let user_agent = format!(
            "SakuraRed/MoyuMax/{} (github.com/SakuraRed/MoyuMax)",
            env!("CARGO_PKG_VERSION")
        );
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .user_agent(user_agent)
            .build()?;
        Ok(Self { client, base_url })
    }

    pub async fn search_mods(&self, query: &ModrinthSearchQuery) -> Result<ModrinthSearchPage> {
        self.search_projects(query).await
    }

    /// 按项目类型搜索在线目录。模组沿用版本/加载器/客户端侧过滤；
    /// 整合包、光影、资源包在提供游戏版本时附加版本过滤，否则全目录浏览。
    pub async fn search_projects(&self, query: &ModrinthSearchQuery) -> Result<ModrinthSearchPage> {
        validate_search_query(query)?;
        let mut facet_groups: Vec<Vec<String>> = vec![vec![format!(
            "project_type:{}",
            query.project_type.api_value()
        )]];
        if query.project_type == ModrinthProjectType::Mod {
            facet_groups.push(vec![format!("versions:{}", query.game_version)]);
            facet_groups.push(vec![format!("categories:{}", query.loader)]);
            facet_groups.push(vec![
                "client_side:required".to_owned(),
                "client_side:optional".to_owned(),
            ]);
        } else if !query.game_version.trim().is_empty() {
            facet_groups.push(vec![format!("versions:{}", query.game_version)]);
        }
        let facets = serde_json::to_string(&facet_groups)?;
        let mut url = self.endpoint("search")?;
        url.query_pairs_mut()
            .append_pair("query", query.query.trim())
            .append_pair("facets", &facets)
            .append_pair("index", query.index.api_value())
            .append_pair("offset", &query.offset.to_string())
            .append_pair("limit", &query.limit.to_string());
        self.get_json(url).await
    }

    /// 项目最新版本的主文件（在线整合包/光影/资源包安装用）。
    /// 提供游戏版本或加载器时按实例约束过滤；优先正式版。
    pub async fn latest_project_file(
        &self,
        project_id: &str,
        game_version: Option<&str>,
        loader: Option<&str>,
    ) -> Result<ModrinthVersionFile> {
        validate_identifier(project_id, "Modrinth 项目 ID")?;
        let mut url = self.endpoint(&format!("project/{project_id}/version"))?;
        {
            let mut pairs = url.query_pairs_mut();
            if let Some(game_version) = game_version.filter(|value| !value.trim().is_empty()) {
                pairs.append_pair(
                    "game_versions",
                    &serde_json::to_string(&vec![game_version])?,
                );
            }
            if let Some(loader) = loader.filter(|value| !value.trim().is_empty()) {
                pairs.append_pair("loaders", &serde_json::to_string(&vec![loader])?);
            }
        }
        let mut versions: Vec<ModrinthVersion> = self.get_json(url).await?;
        versions.sort_by_key(|version| version.version_type.priority());
        let version = versions
            .iter()
            .find(|version| version.status == ModrinthVersionStatus::Listed)
            .or(versions.first())
            .ok_or_else(|| CoreError::Content("该项目没有匹配当前条件的版本".to_owned()))?;
        let file = version
            .files
            .iter()
            .find(|file| file.primary)
            .or(version.files.first())
            .ok_or_else(|| CoreError::Content("该版本没有可下载文件".to_owned()))?;
        validate_hash(&file.hashes.sha1, 40, "SHA-1")?;
        validate_hash(&file.hashes.sha512, 128, "SHA-512")?;
        Ok(ModrinthVersionFile {
            url: file.url.clone(),
            filename: file.filename.clone(),
            sha1: file.hashes.sha1.clone(),
            sha512: file.hashes.sha512.clone(),
            size: file.size,
        })
    }

    /// 流式下载项目文件到目标目录，SHA-1 校验后原子改名返回最终路径。
    pub async fn download_project_file(
        &self,
        file: &ModrinthVersionFile,
        destination_directory: &Path,
    ) -> Result<PathBuf> {
        if file.filename.is_empty()
            || file.filename.contains(['/', '\\'])
            || file.filename == "."
            || file.filename == ".."
        {
            return Err(CoreError::Content("服务端返回的文件名无效".to_owned()));
        }
        fs::create_dir_all(destination_directory)?;
        let staging = destination_directory.join(format!("{}.part", Uuid::new_v4().simple()));
        let target = destination_directory.join(&file.filename);
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

    async fn download_to_staging(&self, file: &ModrinthVersionFile, staging: &Path) -> Result<()> {
        let response = self
            .client
            .get(&file.url)
            .send()
            .await
            .map_err(|error| CoreError::Content(format!("无法下载项目文件：{error}")))?;
        if !response.status().is_success() {
            return Err(CoreError::Content(format!(
                "项目文件下载失败（HTTP {}）",
                response.status()
            )));
        }
        let mut hasher = Sha1::new();
        let mut writer = std::io::BufWriter::new(fs::File::create(staging)?);
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk =
                chunk.map_err(|error| CoreError::Content(format!("项目文件下载中断：{error}")))?;
            hasher.update(&chunk);
            writer.write_all(&chunk)?;
        }
        writer.flush()?;
        let digest = content_encode_hex(hasher.finalize());
        if !digest.eq_ignore_ascii_case(&file.sha1) {
            return Err(CoreError::Content(
                "项目文件 SHA-1 校验失败，已拒绝使用".to_owned(),
            ));
        }
        Ok(())
    }

    pub async fn resolve_mod_install_plan(
        &self,
        instance: &ManagedInstanceSummary,
        root_project_id: &str,
        selected_optional_projects: &[String],
    ) -> Result<ContentInstallPlan> {
        validate_target_instance(instance)?;
        validate_identifier(root_project_id, "Modrinth 项目 ID")?;
        let selected_optional = selected_optional_projects
            .iter()
            .map(|project_id| {
                validate_identifier(project_id, "可选依赖项目 ID")?;
                Ok(project_id.clone())
            })
            .collect::<Result<HashSet<_>>>()?;
        let mut queue = VecDeque::from([DependencyRequest {
            project_id: root_project_id.to_owned(),
            version_id: None,
            required_by_project_id: None,
        }]);
        let mut nodes = HashMap::<String, ResolvedNode>::new();
        let mut optional_dependencies = HashMap::<String, ContentDependencyChoice>::new();
        let mut incompatible_dependencies = HashMap::<String, ContentDependencyChoice>::new();

        while let Some(request) = queue.pop_front() {
            if let Some(existing) = nodes.get(&request.project_id) {
                if request
                    .version_id
                    .as_ref()
                    .is_some_and(|version_id| version_id != &existing.entry.version_id)
                {
                    return Err(CoreError::Content(format!(
                        "项目 {} 被依赖闭包要求两个不同版本",
                        request.project_id
                    )));
                }
                continue;
            }
            let project = self.get_project(&request.project_id).await?;
            validate_project(&project)?;
            let version = if let Some(version_id) = &request.version_id {
                let version = self.get_version(version_id).await?;
                if version.project_id != request.project_id {
                    return Err(CoreError::Content(format!(
                        "版本 {version_id} 不属于项目 {}",
                        request.project_id
                    )));
                }
                validate_compatible_version(&version, instance)?;
                version
            } else {
                self.latest_compatible_version(&request.project_id, instance)
                    .await?
            };
            let file = select_primary_file(&version)?;
            let mut required_projects = Vec::new();
            for dependency in &version.dependencies {
                match dependency.dependency_type {
                    ContentDependencyKind::Required => {
                        let dependency_request = self
                            .dependency_request(dependency, &version.project_id)
                            .await?;
                        required_projects.push(dependency_request.project_id.clone());
                        queue.push_back(dependency_request);
                    }
                    ContentDependencyKind::Optional => {
                        let choice = self
                            .dependency_choice(dependency, &version.project_id)
                            .await?;
                        if let Some(project_id) = &choice.project_id {
                            if selected_optional.contains(project_id) {
                                required_projects.push(project_id.clone());
                                queue.push_back(DependencyRequest {
                                    project_id: project_id.clone(),
                                    version_id: choice.version_id.clone(),
                                    required_by_project_id: Some(version.project_id.clone()),
                                });
                            }
                            optional_dependencies.insert(project_id.clone(), choice);
                        }
                    }
                    ContentDependencyKind::Incompatible => {
                        let choice = self
                            .dependency_choice(dependency, &version.project_id)
                            .await?;
                        let key = choice
                            .project_id
                            .clone()
                            .or_else(|| choice.version_id.clone())
                            .unwrap_or_else(|| choice.title.clone());
                        incompatible_dependencies.insert(key, choice);
                    }
                    ContentDependencyKind::Embedded => {}
                }
            }
            required_projects.sort();
            required_projects.dedup();
            nodes.insert(
                request.project_id.clone(),
                ResolvedNode {
                    entry: ContentPlanEntry {
                        project_id: version.project_id,
                        version_id: version.id,
                        project_title: project.title,
                        version_number: version.version_number,
                        required_by_project_id: request.required_by_project_id,
                        file,
                    },
                    required_projects,
                },
            );
        }

        let mut order = Vec::new();
        let mut temporary = HashSet::new();
        let mut permanent = HashSet::new();
        visit_dependency(
            root_project_id,
            &nodes,
            &mut temporary,
            &mut permanent,
            &mut order,
        )?;
        let entries = order
            .into_iter()
            .map(|project_id| {
                nodes
                    .get(&project_id)
                    .map(|node| node.entry.clone())
                    .ok_or_else(|| CoreError::Content(format!("依赖闭包缺少项目 {project_id}")))
            })
            .collect::<Result<Vec<_>>>()?;
        let mut optional_dependencies = optional_dependencies.into_values().collect::<Vec<_>>();
        optional_dependencies.sort_by(|left, right| left.title.cmp(&right.title));
        let mut incompatible_dependencies =
            incompatible_dependencies.into_values().collect::<Vec<_>>();
        incompatible_dependencies.sort_by(|left, right| left.title.cmp(&right.title));

        Ok(ContentInstallPlan {
            schema_version: 1,
            instance_id: instance.id.clone(),
            instance_name: instance.name.clone(),
            game_version: instance.game_version.clone(),
            loader: instance.loader_kind.clone(),
            root_project_id: root_project_id.to_owned(),
            entries,
            optional_dependencies,
            incompatible_dependencies,
            is_update: false,
        })
    }

    async fn latest_compatible_version(
        &self,
        project_id: &str,
        instance: &ManagedInstanceSummary,
    ) -> Result<ModrinthVersion> {
        let mut url = self.endpoint(&format!("project/{project_id}/version"))?;
        url.query_pairs_mut()
            .append_pair("loaders", &serde_json::to_string(&[&instance.loader_kind])?)
            .append_pair(
                "game_versions",
                &serde_json::to_string(&[&instance.game_version])?,
            )
            .append_pair("include_changelog", "false");
        let versions: Vec<ModrinthVersion> = self.get_json(url).await?;
        select_compatible_version(versions, instance).ok_or_else(|| {
            CoreError::Content(format!(
                "项目 {project_id} 没有兼容 Minecraft {} {} 的已列出版本",
                instance.game_version, instance.loader_kind
            ))
        })
    }

    async fn get_project(&self, project_id: &str) -> Result<ModrinthProject> {
        validate_identifier(project_id, "Modrinth 项目 ID")?;
        self.get_json(self.endpoint(&format!("project/{project_id}"))?)
            .await
    }

    async fn get_version(&self, version_id: &str) -> Result<ModrinthVersion> {
        validate_identifier(version_id, "Modrinth 版本 ID")?;
        self.get_json(self.endpoint(&format!("version/{version_id}"))?)
            .await
    }

    async fn dependency_request(
        &self,
        dependency: &ModrinthDependency,
        required_by_project_id: &str,
    ) -> Result<DependencyRequest> {
        let (project_id, version_id) = self.dependency_identity(dependency).await?;
        Ok(DependencyRequest {
            project_id,
            version_id,
            required_by_project_id: Some(required_by_project_id.to_owned()),
        })
    }

    async fn dependency_choice(
        &self,
        dependency: &ModrinthDependency,
        required_by_project_id: &str,
    ) -> Result<ContentDependencyChoice> {
        let (project_id, version_id) = self.dependency_identity(dependency).await?;
        let project = self.get_project(&project_id).await?;
        Ok(ContentDependencyChoice {
            project_id: Some(project_id),
            version_id,
            title: project.title,
            kind: dependency.dependency_type,
            required_by_project_id: required_by_project_id.to_owned(),
        })
    }

    async fn dependency_identity(
        &self,
        dependency: &ModrinthDependency,
    ) -> Result<(String, Option<String>)> {
        if let Some(project_id) = &dependency.project_id {
            validate_identifier(project_id, "依赖项目 ID")?;
            if let Some(version_id) = &dependency.version_id {
                validate_identifier(version_id, "依赖版本 ID")?;
            }
            return Ok((project_id.clone(), dependency.version_id.clone()));
        }
        if let Some(version_id) = &dependency.version_id {
            let version = self.get_version(version_id).await?;
            return Ok((version.project_id, Some(version_id.clone())));
        }
        Err(CoreError::Content(format!(
            "依赖 {} 只有文件名且没有项目或版本 ID，无法安全解析",
            dependency.file_name.as_deref().unwrap_or("<unknown>")
        )))
    }

    fn endpoint(&self, path: &str) -> Result<Url> {
        self.base_url
            .join(path)
            .map_err(|error| CoreError::Content(format!("Modrinth API 路径无效：{error}")))
    }

    async fn get_json<T: DeserializeOwned>(&self, url: Url) -> Result<T> {
        let response = self.client.get(url.clone()).send().await?;
        match response.status() {
            status if status.is_success() => response.json().await.map_err(CoreError::from),
            StatusCode::GONE => Err(CoreError::Content(
                "Modrinth API v2 已退役，需要更新 MoyuMax provider".to_owned(),
            )),
            StatusCode::TOO_MANY_REQUESTS => Err(CoreError::Content(
                "Modrinth 请求达到限流，请稍后重试".to_owned(),
            )),
            status => Err(CoreError::Content(format!(
                "Modrinth 请求 {} 返回 HTTP {status}",
                url.path()
            ))),
        }
    }
}

impl AppService {
    pub fn enqueue_content_install_task(
        &self,
        plan: &ContentInstallPlan,
    ) -> Result<ContentInstallTask> {
        validate_content_install_plan(plan)?;
        let instance = self
            .list_instances()?
            .into_iter()
            .find(|instance| instance.id == plan.instance_id)
            .ok_or_else(|| CoreError::Content("目标实例不存在".to_owned()))?;
        validate_content_instance_snapshot(plan, &instance)?;

        let installed = self.list_installed_content(&plan.instance_id)?;
        let installed_projects = installed
            .iter()
            .map(|entry| entry.project_id.as_str())
            .collect::<HashSet<_>>();
        if !plan.is_update
            && let Some(entry) = plan
                .entries
                .iter()
                .find(|entry| installed_projects.contains(entry.project_id.as_str()))
        {
            return Err(CoreError::Content(format!(
                "项目 {} 已安装；不会静默覆盖或更新模组",
                entry.project_title
            )));
        }
        let planned_projects = plan
            .entries
            .iter()
            .map(|entry| entry.project_id.as_str())
            .collect::<HashSet<_>>();
        let planned_versions = plan
            .entries
            .iter()
            .map(|entry| entry.version_id.as_str())
            .collect::<HashSet<_>>();
        if let Some(conflict) = plan.incompatible_dependencies.iter().find(|choice| {
            choice.project_id.as_deref().is_some_and(|project_id| {
                planned_projects.contains(project_id) || installed_projects.contains(project_id)
            }) || choice
                .version_id
                .as_deref()
                .is_some_and(|version_id| planned_versions.contains(version_id))
        }) {
            return Err(CoreError::Content(format!(
                "依赖闭包与不兼容项目 {} 冲突",
                conflict.title
            )));
        }

        let task_id = Uuid::new_v4().to_string();
        let data_directory = self.selected_data_directory()?;
        let staging_directory = data_directory
            .join(".staging")
            .join("content")
            .join(&task_id);
        let shared_store_directory = data_directory.join("store");
        fs::create_dir_all(&staging_directory)?;
        fs::create_dir_all(&shared_store_directory)?;
        let result = self.insert_content_install_task(
            &task_id,
            plan,
            &staging_directory,
            Path::new(&instance.root_directory),
            &shared_store_directory,
        );
        if result.is_err() {
            let _ = fs::remove_dir_all(&staging_directory);
        }
        result
    }

    /// 实例的内容自动更新开关（默认关闭）。
    pub fn instance_content_auto_update(&self, instance_id: &str) -> Result<bool> {
        let connection = self.connection()?;
        let enabled: Option<bool> = connection
            .query_row(
                "SELECT content_auto_update_enabled FROM instances WHERE id = ?1",
                params![instance_id],
                |row| row.get(0),
            )
            .optional()?;
        enabled.ok_or_else(|| CoreError::Content("实例不存在".to_owned()))
    }

    /// 开关实例的内容自动更新。开启后更新仍需用户明确触发,不做静默修改。
    pub fn set_instance_content_auto_update(&self, instance_id: &str, enabled: bool) -> Result<()> {
        let changed = self.connection()?.execute(
            "UPDATE instances SET content_auto_update_enabled = ?2 WHERE id = ?1",
            params![instance_id, enabled],
        )?;
        if changed == 0 {
            return Err(CoreError::Content("实例不存在".to_owned()));
        }
        Ok(())
    }

    /// 检查实例已安装内容的可用更新。只解析元数据,不下载任何文件。
    pub async fn check_content_updates(
        &self,
        modrinth: &ModrinthClient,
        instance_id: &str,
    ) -> Result<Vec<ContentUpdateInfo>> {
        let instance = self
            .list_instances()?
            .into_iter()
            .find(|instance| instance.id == instance_id)
            .ok_or_else(|| CoreError::Content("目标实例不存在".to_owned()))?;
        validate_target_instance(&instance)?;
        let installed = self.list_installed_content(instance_id)?;
        let mut updates = Vec::new();
        for entry in &installed {
            let Ok(latest) = modrinth
                .latest_compatible_version(&entry.project_id, &instance)
                .await
            else {
                continue;
            };
            if latest.id != entry.version_id {
                updates.push(ContentUpdateInfo {
                    project_id: entry.project_id.clone(),
                    project_title: entry.project_title.clone(),
                    current_version_id: entry.version_id.clone(),
                    current_version_number: entry.version_number.clone(),
                    latest_version_id: latest.id.clone(),
                    latest_version_number: latest.version_number.clone(),
                    file: select_primary_file(&latest)?,
                });
            }
        }
        updates.sort_by(|left, right| left.project_title.cmp(&right.project_title));
        Ok(updates)
    }

    /// 为选定项目生成更新计划（带恢复点语义）并入队。
    pub async fn plan_content_update(
        &self,
        modrinth: &ModrinthClient,
        instance_id: &str,
        project_ids: &[String],
    ) -> Result<ContentInstallTask> {
        if project_ids.is_empty() {
            return Err(CoreError::Content("没有选择要更新的项目".to_owned()));
        }
        let instance = self
            .list_instances()?
            .into_iter()
            .find(|instance| instance.id == instance_id)
            .ok_or_else(|| CoreError::Content("目标实例不存在".to_owned()))?;
        validate_target_instance(&instance)?;
        let installed = self.list_installed_content(instance_id)?;
        let mut entries = Vec::with_capacity(project_ids.len());
        for project_id in project_ids {
            validate_identifier(project_id, "内容项目 ID")?;
            let installed_entry = installed
                .iter()
                .find(|entry| &entry.project_id == project_id)
                .ok_or_else(|| CoreError::Content(format!("项目 {project_id} 未安装,不能更新")))?;
            let latest = modrinth
                .latest_compatible_version(project_id, &instance)
                .await?;
            if latest.id == installed_entry.version_id {
                continue;
            }
            entries.push(ContentPlanEntry {
                project_id: project_id.clone(),
                version_id: latest.id.clone(),
                project_title: installed_entry.project_title.clone(),
                version_number: latest.version_number.clone(),
                required_by_project_id: None,
                file: select_primary_file(&latest)?,
            });
        }
        if entries.is_empty() {
            return Err(CoreError::Content("所选项目已经是最新兼容版本".to_owned()));
        }
        let root_project_id = entries[0].project_id.clone();
        let plan = ContentInstallPlan {
            schema_version: 1,
            instance_id: instance.id.clone(),
            instance_name: instance.name.clone(),
            game_version: instance.game_version.clone(),
            loader: instance.loader_kind.clone(),
            root_project_id,
            entries,
            optional_dependencies: Vec::new(),
            incompatible_dependencies: Vec::new(),
            is_update: true,
        };
        self.enqueue_content_install_task(&plan)
    }

    pub fn list_content_install_tasks(&self) -> Result<Vec<ContentInstallTask>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "
            SELECT task.id, task.state, task.current_stage, task.plan_json,
                   task.staging_directory, task.target_directory,
                   task.shared_store_directory, task.created_at_unix_seconds,
                   task.updated_at_unix_seconds, COALESCE(progress.completed_bytes, 0),
                   progress.total_bytes, progress.current_item, progress.error_summary,
                   progress.source_detail, task.priority, task.paused_by
            FROM content_install_tasks AS task
            LEFT JOIN content_task_progress AS progress ON progress.task_id = task.id
            ORDER BY task.created_at_unix_seconds, task.id
            ",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, i64>(7)?,
                row.get::<_, i64>(8)?,
                row.get::<_, i64>(9)?,
                row.get::<_, Option<i64>>(10)?,
                row.get::<_, Option<String>>(11)?,
                row.get::<_, Option<String>>(12)?,
                row.get::<_, Option<String>>(13)?,
                row.get::<_, i64>(14)?,
                row.get::<_, Option<String>>(15)?,
            ))
        })?;
        rows.map(|row| {
            let (
                id,
                state,
                stage,
                plan,
                staging_directory,
                target_directory,
                shared_store_directory,
                created_at_unix_seconds,
                updated_at_unix_seconds,
                completed_bytes,
                total_bytes,
                current_item,
                error_summary,
                source_detail,
                priority,
                paused_by,
            ) = row?;
            Ok(ContentInstallTask {
                id,
                state: TaskState::from_database(&state)?,
                current_stage: stage
                    .as_deref()
                    .map(ContentInstallStage::from_database)
                    .transpose()?,
                plan: serde_json::from_str(&plan)?,
                staging_directory,
                target_directory,
                shared_store_directory,
                created_at_unix_seconds,
                updated_at_unix_seconds,
                priority,
                paused_by,
                progress: TaskProgress {
                    completed_bytes: content_sqlite_unsigned(
                        completed_bytes,
                        "内容任务已完成字节数",
                    )?,
                    total_bytes: total_bytes
                        .map(|value| content_sqlite_unsigned(value, "内容任务总字节数"))
                        .transpose()?,
                    current_item,
                    error_summary,
                    source_detail: source_detail
                        .map(|json| serde_json::from_str(&json))
                        .transpose()?,
                },
            })
        })
        .collect()
    }

    pub fn list_installed_content(&self, instance_id: &str) -> Result<Vec<InstalledContent>> {
        if instance_id.trim().is_empty() || instance_id.chars().any(char::is_control) {
            return Err(CoreError::Content("实例 ID 格式无效".to_owned()));
        }
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "
            SELECT id, instance_id, provider, project_id, version_id,
                   project_title, version_number, file_name, relative_path,
                   size, sha1, sha512, enabled, auto_update_enabled,
                   installed_at_unix_seconds
            FROM installed_content
            WHERE instance_id = ?1
            ORDER BY project_title, project_id
            ",
        )?;
        let rows = statement.query_map(params![instance_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, String>(8)?,
                row.get::<_, i64>(9)?,
                row.get::<_, String>(10)?,
                row.get::<_, String>(11)?,
                row.get::<_, bool>(12)?,
                row.get::<_, bool>(13)?,
                row.get::<_, i64>(14)?,
            ))
        })?;
        rows.map(|row| {
            let (
                id,
                instance_id,
                provider,
                project_id,
                version_id,
                project_title,
                version_number,
                file_name,
                relative_path,
                size,
                sha1,
                sha512,
                enabled,
                auto_update_enabled,
                installed_at_unix_seconds,
            ) = row?;
            Ok(InstalledContent {
                id,
                instance_id,
                provider: ContentProvider::from_database(&provider)?,
                project_id,
                version_id,
                project_title,
                version_number,
                file_name,
                relative_path,
                size: content_sqlite_unsigned(size, "已安装内容大小")?,
                sha1,
                sha512,
                enabled,
                auto_update_enabled,
                installed_at_unix_seconds,
            })
        })
        .collect()
    }

    pub fn recover_interrupted_content_tasks(&self) -> Result<()> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "
            SELECT id, plan_json, target_directory
            FROM content_install_tasks
            WHERE state IN ('running', 'committing')
            ",
        )?;
        let interrupted = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        drop(statement);
        for (task_id, plan_json, target_directory) in interrupted {
            if let Ok(plan) = serde_json::from_str::<ContentInstallPlan>(&plan_json)
                && plan.is_update
            {
                // 更新中断：先把恢复点中的旧文件放回实例，再进入恢复确认。
                let _ =
                    restore_content_update_snapshot(&plan, Path::new(&target_directory), &task_id);
            }
            connection.execute(
                "
                UPDATE content_install_tasks
                SET state = 'awaiting_recovery', updated_at_unix_seconds = ?1
                WHERE id = ?2 AND state IN ('running', 'committing')
                ",
                params![unix_timestamp(), task_id],
            )?;
        }
        let mut statement = connection.prepare(
            "
            SELECT id, staging_directory, target_directory
            FROM content_install_tasks
            WHERE state = 'completed'
            ",
        )?;
        let completed = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        drop(statement);
        drop(connection);
        let staging_root = self
            .selected_data_directory()?
            .join(".staging")
            .join("content");
        for (task_id, staging_directory, target_directory) in completed {
            let snapshot_directory =
                content_update_snapshot_directory(Path::new(&target_directory), &task_id);
            if snapshot_directory.exists() {
                let _ = fs::remove_dir_all(&snapshot_directory);
            }
            let expected = staging_root.join(&task_id);
            if Path::new(&staging_directory) == expected
                && expected.exists()
                && fs::remove_dir_all(&expected).is_ok()
            {
                let _ = self.connection()?.execute(
                    "UPDATE content_task_progress SET current_item = '内容安装完成' WHERE task_id = ?1",
                    params![task_id],
                );
            }
        }
        Ok(())
    }

    pub fn retry_failed_content_task(&self, task_id: &str) -> Result<()> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let changed = transaction.execute(
            "
            UPDATE content_install_tasks
            SET state = 'queued', current_stage = 'prepare',
                updated_at_unix_seconds = ?2
            WHERE id = ?1 AND state = 'failed'
            ",
            params![task_id, unix_timestamp()],
        )?;
        if changed == 0 {
            return Err(CoreError::Content(
                "内容任务不存在或当前状态不能重试".to_owned(),
            ));
        }
        transaction.execute(
            "
            UPDATE content_task_progress
            SET current_item = '等待重试执行', error_summary = NULL
            WHERE task_id = ?1
            ",
            params![task_id],
        )?;
        transaction.commit()?;
        Ok(())
    }

    /// 下载被中断后把内容任务标记为可恢复的暂停状态。
    pub fn mark_content_task_paused(&self, task_id: &str, paused_by: &str) -> Result<()> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let changed = transaction.execute(
            "
            UPDATE content_install_tasks
            SET state = 'paused', paused_by = ?3, updated_at_unix_seconds = ?2
            WHERE id = ?1 AND state IN ('queued', 'running')
            ",
            params![task_id, unix_timestamp(), paused_by],
        )?;
        if changed == 0 {
            return Err(CoreError::Content(
                "内容任务不存在或当前状态不能暂停".to_owned(),
            ));
        }
        transaction.execute(
            "
            UPDATE content_task_progress
            SET current_item = '已暂停，可在恢复后继续', error_summary = NULL
            WHERE task_id = ?1
            ",
            params![task_id],
        )?;
        transaction.commit()?;
        Ok(())
    }

    /// 全局恢复：只把被全局暂停打断的内容任务重新入队，按优先级与创建时间返回。
    pub fn requeue_paused_content_tasks(&self) -> Result<Vec<String>> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut statement = transaction.prepare(
            "
            SELECT id FROM content_install_tasks
            WHERE state = 'paused' AND paused_by = 'global'
            ORDER BY priority DESC, created_at_unix_seconds, id
            ",
        )?;
        let task_ids = statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        drop(statement);
        if task_ids.is_empty() {
            return Ok(Vec::new());
        }
        transaction.execute(
            "
            UPDATE content_task_progress
            SET current_item = '等待恢复执行', error_summary = NULL
            WHERE task_id IN (SELECT id FROM content_install_tasks WHERE state = 'paused' AND paused_by = 'global')
            ",
            [],
        )?;
        transaction.execute(
            "
            UPDATE content_install_tasks
            SET state = 'queued', paused_by = NULL, current_stage = 'prepare', updated_at_unix_seconds = ?1
            WHERE state = 'paused' AND paused_by = 'global'
            ",
            params![unix_timestamp()],
        )?;
        transaction.commit()?;
        Ok(task_ids)
    }

    /// 单任务恢复：任意暂停来源的内容任务重新入队。
    pub fn requeue_paused_content_task(&self, task_id: &str) -> Result<()> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let changed = transaction.execute(
            "
            UPDATE content_install_tasks
            SET state = 'queued', paused_by = NULL, current_stage = 'prepare', updated_at_unix_seconds = ?2
            WHERE id = ?1 AND state = 'paused'
            ",
            params![task_id, unix_timestamp()],
        )?;
        if changed == 0 {
            return Err(CoreError::Content(
                "内容任务不存在或当前不是暂停状态".to_owned(),
            ));
        }
        transaction.execute(
            "
            UPDATE content_task_progress
            SET current_item = '等待恢复执行', error_summary = NULL
            WHERE task_id = ?1
            ",
            params![task_id],
        )?;
        transaction.commit()?;
        Ok(())
    }

    /// 调整排队内容任务的优先级。
    pub fn set_content_task_priority(&self, task_id: &str, priority: i64) -> Result<()> {
        let changed = self.connection()?.execute(
            "
            UPDATE content_install_tasks
            SET priority = ?2, updated_at_unix_seconds = ?3
            WHERE id = ?1 AND state IN ('queued', 'paused')
            ",
            params![task_id, priority, unix_timestamp()],
        )?;
        if changed == 0 {
            return Err(CoreError::Content(
                "内容任务不存在或当前状态不能调整优先级".to_owned(),
            ));
        }
        Ok(())
    }

    /// 共享执行槽的内容候选：按优先级与创建时间返回排队任务。
    pub fn queued_content_tasks_by_priority(&self) -> Result<Vec<String>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "
            SELECT id FROM content_install_tasks
            WHERE state = 'queued'
            ORDER BY priority DESC, created_at_unix_seconds, id
            ",
        )?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn resolve_content_task_recovery(
        &self,
        task_id: &str,
        decision: RecoveryDecision,
    ) -> Result<()> {
        Uuid::parse_str(task_id)
            .map_err(|_| CoreError::Content("内容任务 ID 格式无效".to_owned()))?;
        let expected_staging = self
            .selected_data_directory()?
            .join(".staging")
            .join("content")
            .join(task_id);
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let (state, current_stage, plan_json, staging_directory, target_directory) = transaction
            .query_row(
                "
                SELECT state, current_stage, plan_json, staging_directory, target_directory
                FROM content_install_tasks WHERE id = ?1
                ",
                params![task_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                    ))
                },
            )
            .optional()?
            .ok_or_else(|| CoreError::Content("内容安装任务不存在".to_owned()))?;
        if TaskState::from_database(&state)? != TaskState::AwaitingRecovery {
            return Err(CoreError::Content("内容任务当前不需要恢复确认".to_owned()));
        }
        if decision == RecoveryDecision::Discard {
            if Path::new(&staging_directory) != expected_staging {
                return Err(CoreError::InvalidStoredState(
                    "内容任务暂存路径不在受管区域中，已拒绝清理".to_owned(),
                ));
            }
            let plan: ContentInstallPlan = serde_json::from_str(&plan_json)?;
            let stage = current_stage
                .as_deref()
                .map(ContentInstallStage::from_database)
                .transpose()?;
            rollback_interrupted_content_commit(
                &plan,
                Path::new(&target_directory),
                &expected_staging,
                stage,
                task_id,
            )?;
            let snapshot_directory =
                content_update_snapshot_directory(Path::new(&target_directory), task_id);
            if snapshot_directory.exists() {
                fs::remove_dir_all(&snapshot_directory)?;
            }
            if expected_staging.exists() {
                fs::remove_dir_all(&expected_staging)?;
            }
        }
        let state = match decision {
            RecoveryDecision::Resume => TaskState::Queued,
            RecoveryDecision::Discard => TaskState::Cancelled,
        };
        let changed = transaction.execute(
            "
            UPDATE content_install_tasks
            SET state = ?2, updated_at_unix_seconds = ?3
            WHERE id = ?1 AND state = 'awaiting_recovery'
            ",
            params![task_id, state.database_value(), unix_timestamp()],
        )?;
        if changed == 0 {
            return Err(CoreError::Content(
                "内容任务恢复状态已经变化，请刷新后重试".to_owned(),
            ));
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn content_install_task(&self, task_id: &str) -> Result<ContentInstallTask> {
        self.list_content_install_tasks()?
            .into_iter()
            .find(|task| task.id == task_id)
            .ok_or_else(|| CoreError::Content("内容安装任务不存在".to_owned()))
    }

    pub(crate) fn content_task_instance(
        &self,
        task: &ContentInstallTask,
    ) -> Result<ManagedInstanceSummary> {
        validate_content_install_plan(&task.plan)?;
        let instance = self
            .list_instances()?
            .into_iter()
            .find(|instance| instance.id == task.plan.instance_id)
            .ok_or_else(|| CoreError::Content("内容任务引用的实例不存在".to_owned()))?;
        validate_content_instance_snapshot(&task.plan, &instance)?;
        if Path::new(&instance.root_directory) != Path::new(&task.target_directory) {
            return Err(CoreError::InvalidStoredState(
                "内容任务目标目录与实例索引不一致".to_owned(),
            ));
        }
        Ok(instance)
    }

    pub(crate) fn set_content_task_phase(
        &self,
        task_id: &str,
        state: TaskState,
        stage: ContentInstallStage,
        current_item: &str,
    ) -> Result<()> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let changed = transaction.execute(
            "
            UPDATE content_install_tasks
            SET state = ?2, current_stage = ?3, updated_at_unix_seconds = ?4
            WHERE id = ?1
            ",
            params![
                task_id,
                state.database_value(),
                stage.database_value(),
                unix_timestamp()
            ],
        )?;
        if changed == 0 {
            return Err(CoreError::Content("内容安装任务不存在".to_owned()));
        }
        transaction.execute(
            "
            UPDATE content_task_progress
            SET current_item = ?2, error_summary = NULL
            WHERE task_id = ?1
            ",
            params![task_id, current_item],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub(crate) fn set_content_task_progress(
        &self,
        task_id: &str,
        completed_bytes: u64,
        total_bytes: u64,
        current_item: &str,
    ) -> Result<()> {
        self.connection()?.execute(
            "
            UPDATE content_task_progress
            SET completed_bytes = ?2, total_bytes = ?3,
                current_item = ?4, error_summary = NULL
            WHERE task_id = ?1
            ",
            params![
                task_id,
                content_sqlite_integer(completed_bytes, "内容任务已完成字节数")?,
                content_sqlite_integer(total_bytes, "内容任务总字节数")?,
                current_item,
            ],
        )?;
        Ok(())
    }

    pub(crate) fn set_content_task_source_detail(
        &self,
        task_id: &str,
        detail: &crate::TaskSourceDetail,
    ) -> Result<()> {
        let serialized = serde_json::to_string(detail)?;
        self.connection()?.execute(
            "UPDATE content_task_progress SET source_detail = ?2 WHERE task_id = ?1",
            params![task_id, serialized],
        )?;
        Ok(())
    }

    pub(crate) fn mark_content_task_failed(&self, task_id: &str, error: &str) -> Result<()> {
        let summary = error.chars().take(4_000).collect::<String>();
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute(
            "
            UPDATE content_install_tasks
            SET state = 'failed', updated_at_unix_seconds = ?2
            WHERE id = ?1 AND state <> 'completed'
            ",
            params![task_id, unix_timestamp()],
        )?;
        transaction.execute(
            "
            UPDATE content_task_progress
            SET current_item = '等待重试', error_summary = ?2
            WHERE task_id = ?1
            ",
            params![task_id, summary],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub(crate) fn note_content_cleanup_pending(&self, task_id: &str, error: &str) -> Result<()> {
        let summary = error.chars().take(1_000).collect::<String>();
        self.connection()?.execute(
            "
            UPDATE content_task_progress
            SET current_item = ?2
            WHERE task_id = ?1
            ",
            params![
                task_id,
                format!("内容已安装；临时文件将在稍后清理：{summary}")
            ],
        )?;
        Ok(())
    }

    pub(crate) fn publish_installed_content(
        &self,
        task: &ContentInstallTask,
    ) -> Result<Vec<InstalledContent>> {
        let installed_at = unix_timestamp();
        let entries = task
            .plan
            .entries
            .iter()
            .map(|entry| InstalledContent {
                id: Uuid::new_v4().to_string(),
                instance_id: task.plan.instance_id.clone(),
                provider: ContentProvider::Modrinth,
                project_id: entry.project_id.clone(),
                version_id: entry.version_id.clone(),
                project_title: entry.project_title.clone(),
                version_number: entry.version_number.clone(),
                file_name: entry.file.filename.clone(),
                relative_path: format!(".minecraft/mods/{}", entry.file.filename),
                size: entry.file.size,
                sha1: entry.file.sha1.clone(),
                sha512: entry.file.sha512.clone(),
                enabled: true,
                auto_update_enabled: false,
                installed_at_unix_seconds: installed_at,
            })
            .collect::<Vec<_>>();
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut entries = entries;
        if task.plan.is_update {
            // 更新：同一事务内替换旧索引行，保留逐项启用与自动更新标志。
            for entry in &mut entries {
                let preserved = transaction
                    .query_row(
                        "
                        SELECT enabled, auto_update_enabled FROM installed_content
                        WHERE instance_id = ?1 AND provider = ?2 AND project_id = ?3
                        ",
                        params![
                            entry.instance_id,
                            entry.provider.database_value(),
                            entry.project_id
                        ],
                        |row| Ok((row.get::<_, bool>(0)?, row.get::<_, bool>(1)?)),
                    )
                    .optional()?;
                if let Some((enabled, auto_update_enabled)) = preserved {
                    entry.enabled = enabled;
                    entry.auto_update_enabled = auto_update_enabled;
                }
                transaction.execute(
                    "
                    DELETE FROM installed_content
                    WHERE instance_id = ?1 AND provider = ?2 AND project_id = ?3
                    ",
                    params![
                        entry.instance_id,
                        entry.provider.database_value(),
                        entry.project_id
                    ],
                )?;
            }
        }
        for entry in &entries {
            transaction.execute(
                "
                INSERT INTO installed_content (
                    id, instance_id, provider, project_id, version_id,
                    project_title, version_number, file_name, relative_path,
                    size, sha1, sha512, enabled, auto_update_enabled,
                    installed_at_unix_seconds
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?14, ?15, ?13)
                ",
                params![
                    entry.id,
                    entry.instance_id,
                    entry.provider.database_value(),
                    entry.project_id,
                    entry.version_id,
                    entry.project_title,
                    entry.version_number,
                    entry.file_name,
                    entry.relative_path,
                    content_sqlite_integer(entry.size, "已安装内容大小")?,
                    entry.sha1,
                    entry.sha512,
                    entry.installed_at_unix_seconds,
                    entry.enabled,
                    entry.auto_update_enabled,
                ],
            )?;
        }
        let changed = transaction.execute(
            "
            UPDATE content_install_tasks
            SET state = 'completed', current_stage = 'index_content',
                updated_at_unix_seconds = ?2
            WHERE id = ?1 AND state = 'committing'
            ",
            params![task.id, unix_timestamp()],
        )?;
        if changed == 0 {
            return Err(CoreError::Content(
                "内容任务提交状态已经变化，拒绝写入索引".to_owned(),
            ));
        }
        transaction.execute(
            "
            UPDATE content_task_progress
            SET completed_bytes = COALESCE(total_bytes, completed_bytes),
                current_item = '内容安装完成', error_summary = NULL
            WHERE task_id = ?1
            ",
            params![task.id],
        )?;
        transaction.commit()?;
        Ok(entries)
    }

    fn insert_content_install_task(
        &self,
        task_id: &str,
        plan: &ContentInstallPlan,
        staging_directory: &Path,
        target_directory: &Path,
        shared_store_directory: &Path,
    ) -> Result<ContentInstallTask> {
        let now = unix_timestamp();
        let total_bytes = plan
            .entries
            .iter()
            .fold(0_u64, |total, entry| total.saturating_add(entry.file.size));
        let task = ContentInstallTask {
            id: task_id.to_owned(),
            state: TaskState::Queued,
            current_stage: Some(ContentInstallStage::Prepare),
            plan: plan.clone(),
            staging_directory: content_path_text(staging_directory),
            target_directory: content_path_text(target_directory),
            shared_store_directory: content_path_text(shared_store_directory),
            created_at_unix_seconds: now,
            updated_at_unix_seconds: now,
            priority: 0,
            paused_by: None,
            progress: TaskProgress {
                completed_bytes: 0,
                total_bytes: Some(total_bytes),
                current_item: Some("等待执行".to_owned()),
                error_summary: None,
                source_detail: None,
            },
        };
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute(
            "
            INSERT INTO content_install_tasks (
                id, instance_id, state, current_stage, plan_json,
                staging_directory, target_directory, shared_store_directory,
                created_at_unix_seconds, updated_at_unix_seconds
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
            ",
            params![
                task.id,
                task.plan.instance_id,
                task.state.database_value(),
                task.current_stage.map(ContentInstallStage::database_value),
                serde_json::to_string(&task.plan)?,
                task.staging_directory,
                task.target_directory,
                task.shared_store_directory,
                task.created_at_unix_seconds,
                task.updated_at_unix_seconds,
            ],
        )?;
        transaction.execute(
            "
            INSERT INTO content_task_progress (
                task_id, completed_bytes, total_bytes, current_item, error_summary
            ) VALUES (?1, 0, ?2, ?3, NULL)
            ",
            params![
                task.id,
                content_sqlite_integer(total_bytes, "内容任务总字节数")?,
                task.progress.current_item,
            ],
        )?;
        transaction.commit()?;
        Ok(task)
    }
}

#[derive(Debug, Clone, Deserialize)]
struct ModrinthProject {
    id: String,
    title: String,
    project_type: String,
    client_side: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ModrinthVersion {
    id: String,
    project_id: String,
    version_number: String,
    game_versions: Vec<String>,
    loaders: Vec<String>,
    version_type: ModrinthVersionType,
    status: ModrinthVersionStatus,
    date_published: String,
    dependencies: Vec<ModrinthDependency>,
    files: Vec<ModrinthFile>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
enum ModrinthVersionType {
    Release,
    Beta,
    Alpha,
}

impl ModrinthVersionType {
    const fn priority(self) -> u8 {
        match self {
            Self::Release => 0,
            Self::Beta => 1,
            Self::Alpha => 2,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
enum ModrinthVersionStatus {
    Listed,
    Archived,
    Draft,
    Unlisted,
    Scheduled,
    Unknown,
}

#[derive(Debug, Clone, Deserialize)]
struct ModrinthDependency {
    version_id: Option<String>,
    project_id: Option<String>,
    file_name: Option<String>,
    dependency_type: ContentDependencyKind,
}

#[derive(Debug, Clone, Deserialize)]
struct ModrinthFile {
    hashes: ModrinthHashes,
    url: String,
    filename: String,
    primary: bool,
    size: u64,
    file_type: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct ModrinthHashes {
    sha1: String,
    sha512: String,
}

#[derive(Debug)]
struct DependencyRequest {
    project_id: String,
    version_id: Option<String>,
    required_by_project_id: Option<String>,
}

#[derive(Debug)]
struct ResolvedNode {
    entry: ContentPlanEntry,
    required_projects: Vec<String>,
}

fn validate_content_install_plan(plan: &ContentInstallPlan) -> Result<()> {
    if plan.schema_version != 1 {
        return Err(CoreError::Content(format!(
            "不支持的内容安装计划版本：{}",
            plan.schema_version
        )));
    }
    if plan.instance_id.trim().is_empty() || plan.instance_id.chars().any(char::is_control) {
        return Err(CoreError::Content("内容安装计划缺少有效实例 ID".to_owned()));
    }
    validate_instance_term(&plan.game_version, "内容计划游戏版本")?;
    validate_instance_term(&plan.loader, "内容计划加载器")?;
    if !is_supported_mod_loader(&plan.loader) {
        return Err(CoreError::Content(format!(
            "加载器 {} 不支持 Modrinth 模组安装",
            plan.loader
        )));
    }
    validate_identifier(&plan.root_project_id, "根项目 ID")?;
    if plan.entries.is_empty() {
        return Err(CoreError::Content("内容安装计划没有文件".to_owned()));
    }

    let mut projects = HashSet::new();
    let mut filenames = HashSet::new();
    let mut positions = HashMap::new();
    for (index, entry) in plan.entries.iter().enumerate() {
        validate_identifier(&entry.project_id, "内容项目 ID")?;
        validate_identifier(&entry.version_id, "内容版本 ID")?;
        if entry.project_title.trim().is_empty()
            || entry.project_title.chars().any(char::is_control)
            || entry.version_number.trim().is_empty()
            || entry.version_number.chars().any(char::is_control)
        {
            return Err(CoreError::Content(format!(
                "项目 {} 的显示信息无效",
                entry.project_id
            )));
        }
        if !projects.insert(entry.project_id.as_str()) {
            return Err(CoreError::Content(format!(
                "内容安装计划重复包含项目 {}",
                entry.project_id
            )));
        }
        let folded_filename = entry.file.filename.to_lowercase();
        if !filenames.insert(folded_filename) {
            return Err(CoreError::Content(format!(
                "内容安装计划包含重复文件名 {}",
                entry.file.filename
            )));
        }
        validate_filename(&entry.file.filename)?;
        if entry.file.size == 0 {
            return Err(CoreError::Content(format!(
                "模组文件 {} 的大小不能为零",
                entry.file.filename
            )));
        }
        validate_hash(&entry.file.sha1, 40, "SHA-1")?;
        validate_hash(&entry.file.sha512, 128, "SHA-512")?;
        let url = Url::parse(&entry.file.url)
            .map_err(|error| CoreError::Content(format!("模组文件 URL 无效：{error}")))?;
        if !matches!(url.scheme(), "https" | "http") || url.host_str().is_none() {
            return Err(CoreError::Content(format!(
                "模组文件 {} 的 URL 不是有效 HTTP(S) 地址",
                entry.file.filename
            )));
        }
        if let Some(required_by) = &entry.required_by_project_id {
            validate_identifier(required_by, "引用项目 ID")?;
        }
        positions.insert(entry.project_id.as_str(), index);
    }
    if !projects.contains(plan.root_project_id.as_str()) {
        return Err(CoreError::Content("内容安装计划缺少根项目".to_owned()));
    }
    for (index, entry) in plan.entries.iter().enumerate() {
        if let Some(required_by) = entry.required_by_project_id.as_deref() {
            let required_by_position = positions.get(required_by).ok_or_else(|| {
                CoreError::Content(format!(
                    "项目 {} 引用了计划外项目 {required_by}",
                    entry.project_id
                ))
            })?;
            if index >= *required_by_position {
                return Err(CoreError::Content(format!(
                    "依赖项目 {} 必须排在引用者 {required_by} 之前",
                    entry.project_id
                )));
            }
        }
    }
    for choice in plan
        .optional_dependencies
        .iter()
        .chain(&plan.incompatible_dependencies)
    {
        if choice.project_id.is_none() && choice.version_id.is_none() {
            return Err(CoreError::Content(format!(
                "依赖 {} 缺少项目或版本 ID",
                choice.title
            )));
        }
        if let Some(project_id) = &choice.project_id {
            validate_identifier(project_id, "依赖项目 ID")?;
        }
        if let Some(version_id) = &choice.version_id {
            validate_identifier(version_id, "依赖版本 ID")?;
        }
        validate_identifier(&choice.required_by_project_id, "依赖引用项目 ID")?;
    }
    Ok(())
}

fn validate_content_instance_snapshot(
    plan: &ContentInstallPlan,
    instance: &ManagedInstanceSummary,
) -> Result<()> {
    if instance.state != "ready" {
        return Err(CoreError::Content(format!(
            "实例 {} 当前状态不是 ready",
            instance.name
        )));
    }
    if instance.game_version != plan.game_version || instance.loader_kind != plan.loader {
        return Err(CoreError::Content(format!(
            "内容计划目标 {} {} 与实例当前版本 {} {} 不一致",
            plan.game_version, plan.loader, instance.game_version, instance.loader_kind
        )));
    }
    Ok(())
}

fn content_path_text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn content_sqlite_integer(value: u64, label: &str) -> Result<i64> {
    i64::try_from(value)
        .map_err(|_| CoreError::InvalidStoredState(format!("{label}超过 SQLite 可表示范围")))
}

fn content_sqlite_unsigned(value: i64, label: &str) -> Result<u64> {
    u64::try_from(value).map_err(|_| CoreError::InvalidStoredState(format!("{label}不能是负数")))
}

fn rollback_interrupted_content_commit(
    plan: &ContentInstallPlan,
    target_directory: &Path,
    staging_directory: &Path,
    current_stage: Option<ContentInstallStage>,
    task_id: &str,
) -> Result<()> {
    validate_content_install_plan(plan)?;
    let journal_path = staging_directory.join(CONTENT_COMMIT_JOURNAL_NAME);
    if !journal_path.exists() {
        if current_stage == Some(ContentInstallStage::CommitFiles) && !plan.is_update {
            let mods_directory = target_directory.join(".minecraft").join("mods");
            if plan
                .entries
                .iter()
                .any(|entry| mods_directory.join(&entry.file.filename).exists())
            {
                return Err(CoreError::Content(
                    "内容提交阶段中断且提交日志缺失；为避免删除用户文件，已拒绝放弃，请选择继续任务"
                        .to_owned(),
                ));
            }
        }
        return Ok(());
    }
    let journal: ContentCommitJournal = serde_json::from_slice(&fs::read(&journal_path)?)?;
    if journal.schema_version != 1 || journal.entries.len() != plan.entries.len() {
        return Err(CoreError::InvalidStoredState(
            "内容提交日志版本或条目数量无效".to_owned(),
        ));
    }
    let plan_files = plan
        .entries
        .iter()
        .map(|entry| entry.file.filename.to_lowercase())
        .collect::<HashSet<_>>();
    let journal_files = journal
        .entries
        .iter()
        .map(|entry| entry.file_name.to_lowercase())
        .collect::<HashSet<_>>();
    if plan_files != journal_files || journal_files.len() != journal.entries.len() {
        return Err(CoreError::InvalidStoredState(
            "内容提交日志与安装计划不一致".to_owned(),
        ));
    }

    let mods_directory = target_directory.join(".minecraft").join("mods");
    for journal_entry in journal.entries.iter().filter(|entry| !entry.existed_before) {
        validate_filename(&journal_entry.file_name)?;
        let plan_entry = plan
            .entries
            .iter()
            .find(|entry| {
                entry
                    .file
                    .filename
                    .eq_ignore_ascii_case(&journal_entry.file_name)
            })
            .ok_or_else(|| {
                CoreError::InvalidStoredState(format!(
                    "提交日志包含计划外文件 {}",
                    journal_entry.file_name
                ))
            })?;
        let destination = mods_directory.join(&plan_entry.file.filename);
        if !destination.exists() {
            continue;
        }
        if !content_file_matches_sync(&destination, &plan_entry.file)? {
            return Err(CoreError::Content(format!(
                "已发布文件 {} 在中断后发生变化；为避免删除用户文件，已停止回滚",
                plan_entry.file.filename
            )));
        }
        fs::remove_file(&destination)?;
    }
    if plan.is_update {
        // 放弃更新任务：把恢复点中的旧文件移回实例。
        restore_content_update_snapshot(plan, target_directory, task_id)?;
    }
    Ok(())
}

/// 更新任务的恢复点目录（实例内快照区）。
pub(crate) fn content_update_snapshot_directory(target_directory: &Path, task_id: &str) -> PathBuf {
    target_directory
        .join(".moyumax")
        .join("snapshots")
        .join(format!("content-update-{task_id}"))
}

/// 把更新恢复点中的旧文件移回实例 mods 目录。
/// 目标处只允许覆盖与计划哈希一致的半成品新文件；不一致时报错，避免删除用户文件。
fn restore_content_update_snapshot(
    plan: &ContentInstallPlan,
    target_directory: &Path,
    task_id: &str,
) -> Result<()> {
    let snapshot_directory = content_update_snapshot_directory(target_directory, task_id);
    if !snapshot_directory.exists() {
        return Ok(());
    }
    let mods_directory = target_directory.join(".minecraft").join("mods");
    for entry in &plan.entries {
        validate_filename(&entry.file.filename)?;
        let backup = snapshot_directory.join(&entry.file.filename);
        if !backup.exists() {
            continue;
        }
        let destination = mods_directory.join(&entry.file.filename);
        if destination.exists() {
            if !content_file_matches_sync(&destination, &entry.file)? {
                return Err(CoreError::Content(format!(
                    "已发布文件 {} 在中断后发生变化；为避免删除用户文件，已停止恢复",
                    entry.file.filename
                )));
            }
            fs::remove_file(&destination)?;
        }
        fs::rename(&backup, &destination)?;
    }
    if fs::read_dir(&snapshot_directory)?.next().is_none() {
        fs::remove_dir(&snapshot_directory)?;
    }
    Ok(())
}

fn content_file_matches_sync(path: &Path, expected: &ContentFilePlan) -> Result<bool> {
    let metadata = match fs::metadata(path) {
        Ok(metadata) if metadata.is_file() => metadata,
        Ok(_) => return Ok(false),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error.into()),
    };
    if metadata.len() != expected.size {
        return Ok(false);
    }
    let mut file = fs::File::open(path)?;
    let mut buffer = vec![0_u8; 128 * 1024];
    let mut sha1 = Sha1::new();
    let mut sha512 = Sha512::new();
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        sha1.update(&buffer[..read]);
        sha512.update(&buffer[..read]);
    }
    let actual_sha1 = content_encode_hex(sha1.finalize());
    let actual_sha512 = content_encode_hex(sha512.finalize());
    Ok(actual_sha1.eq_ignore_ascii_case(&expected.sha1)
        && actual_sha512.eq_ignore_ascii_case(&expected.sha512))
}

fn content_encode_hex(bytes: impl AsRef<[u8]>) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let bytes = bytes.as_ref();
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}

fn validate_search_query(query: &ModrinthSearchQuery) -> Result<()> {
    if query.query.trim().is_empty() {
        return Err(CoreError::Content("请输入 Modrinth 搜索词".to_owned()));
    }
    // 模组必须有版本/加载器约束；目录浏览（整合包/光影/资源包）可留空，
    // 留空时不附加对应过滤。
    if query.project_type == ModrinthProjectType::Mod || !query.game_version.trim().is_empty() {
        validate_instance_term(&query.game_version, "游戏版本")?;
    }
    if query.project_type == ModrinthProjectType::Mod || !query.loader.trim().is_empty() {
        validate_instance_term(&query.loader, "加载器")?;
    }
    if !(1..=100).contains(&query.limit) {
        return Err(CoreError::Content(
            "Modrinth 单页结果数量必须在 1 到 100 之间".to_owned(),
        ));
    }
    Ok(())
}

fn validate_target_instance(instance: &ManagedInstanceSummary) -> Result<()> {
    if instance.state != "ready" {
        return Err(CoreError::Content(format!(
            "实例 {} 当前状态不是 ready",
            instance.name
        )));
    }
    if !is_supported_mod_loader(&instance.loader_kind) {
        return Err(CoreError::Content(format!(
            "加载器 {} 不支持 Modrinth 模组安装",
            instance.loader_kind
        )));
    }
    validate_instance_term(&instance.game_version, "实例游戏版本")
}

fn is_supported_mod_loader(loader_kind: &str) -> bool {
    matches!(loader_kind, "fabric" | "quilt" | "forge" | "neoforge")
}

fn validate_instance_term(value: &str, label: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
    {
        return Err(CoreError::Content(format!("{label}格式无效")));
    }
    Ok(())
}

fn validate_identifier(value: &str, label: &str) -> Result<()> {
    if value.len() != 8 || !value.bytes().all(|byte| byte.is_ascii_alphanumeric()) {
        return Err(CoreError::Content(format!("{label}格式无效：{value}")));
    }
    Ok(())
}

fn validate_project(project: &ModrinthProject) -> Result<()> {
    validate_identifier(&project.id, "Modrinth 项目 ID")?;
    if project.project_type != "mod" {
        return Err(CoreError::Content(format!(
            "项目 {} 不是模组",
            project.title
        )));
    }
    if project.client_side == "unsupported" {
        return Err(CoreError::Content(format!(
            "项目 {} 不支持客户端",
            project.title
        )));
    }
    Ok(())
}

fn validate_compatible_version(
    version: &ModrinthVersion,
    instance: &ManagedInstanceSummary,
) -> Result<()> {
    if version.status != ModrinthVersionStatus::Listed
        || !version
            .game_versions
            .iter()
            .any(|value| value == &instance.game_version)
        || !version
            .loaders
            .iter()
            .any(|value| value == &instance.loader_kind)
    {
        return Err(CoreError::Content(format!(
            "版本 {} 不兼容 Minecraft {} {}",
            version.id, instance.game_version, instance.loader_kind
        )));
    }
    Ok(())
}

fn select_compatible_version(
    versions: Vec<ModrinthVersion>,
    instance: &ManagedInstanceSummary,
) -> Option<ModrinthVersion> {
    let mut versions = versions
        .into_iter()
        .filter(|version| validate_compatible_version(version, instance).is_ok())
        .collect::<Vec<_>>();
    versions.sort_by(|left, right| {
        left.version_type
            .priority()
            .cmp(&right.version_type.priority())
            .then_with(|| right.date_published.cmp(&left.date_published))
            .then_with(|| left.id.cmp(&right.id))
    });
    versions.into_iter().next()
}

fn select_primary_file(version: &ModrinthVersion) -> Result<ContentFilePlan> {
    let mut primary = version
        .files
        .iter()
        .filter(|file| file.primary && file.file_type.is_none())
        .collect::<Vec<_>>();
    let selected = if primary.len() == 1 {
        primary.remove(0)
    } else {
        let untyped = version
            .files
            .iter()
            .filter(|file| file.file_type.is_none())
            .collect::<Vec<_>>();
        if untyped.len() != 1 {
            return Err(CoreError::Content(format!(
                "版本 {} 无法自动确定唯一主文件",
                version.id
            )));
        }
        untyped[0]
    };
    validate_filename(&selected.filename)?;
    validate_hash(&selected.hashes.sha1, 40, "SHA-1")?;
    validate_hash(&selected.hashes.sha512, 128, "SHA-512")?;
    let url = Url::parse(&selected.url)
        .map_err(|error| CoreError::Content(format!("模组文件 URL 无效：{error}")))?;
    if !matches!(url.scheme(), "https" | "http") || url.host_str().is_none() {
        return Err(CoreError::Content(
            "模组文件 URL 必须是带主机名的 HTTP(S) 地址".to_owned(),
        ));
    }
    Ok(ContentFilePlan {
        url: selected.url.clone(),
        filename: selected.filename.clone(),
        size: selected.size,
        sha1: selected.hashes.sha1.clone(),
        sha512: selected.hashes.sha512.clone(),
    })
}

fn validate_filename(filename: &str) -> Result<()> {
    let path = Path::new(filename);
    if filename.is_empty()
        || filename.len() > 240
        || path.components().count() != 1
        || !matches!(path.components().next(), Some(Component::Normal(_)))
        || !filename.to_ascii_lowercase().ends_with(".jar")
    {
        return Err(CoreError::Content(format!(
            "模组文件名不安全或不是 JAR：{filename}"
        )));
    }
    Ok(())
}

fn validate_hash(value: &str, length: usize, label: &str) -> Result<()> {
    if value.len() != length || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(CoreError::Content(format!("模组文件 {label} 格式无效")));
    }
    Ok(())
}

fn visit_dependency(
    project_id: &str,
    nodes: &HashMap<String, ResolvedNode>,
    temporary: &mut HashSet<String>,
    permanent: &mut HashSet<String>,
    order: &mut Vec<String>,
) -> Result<()> {
    if permanent.contains(project_id) {
        return Ok(());
    }
    if !temporary.insert(project_id.to_owned()) {
        return Ok(());
    }
    let node = nodes
        .get(project_id)
        .ok_or_else(|| CoreError::Content(format!("依赖闭包缺少项目 {project_id}")))?;
    for dependency in &node.required_projects {
        visit_dependency(dependency, nodes, temporary, permanent, order)?;
    }
    temporary.remove(project_id);
    if permanent.insert(project_id.to_owned()) {
        order.push(project_id.to_owned());
    }
    Ok(())
}
