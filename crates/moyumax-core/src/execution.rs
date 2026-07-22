use std::{
    collections::{HashMap, HashSet},
    ffi::OsString,
    fs,
    io::Write,
    path::{Component, Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use futures_util::{StreamExt, TryStreamExt, stream};
use regex::Regex;
use reqwest::{Client, StatusCode, Url, header};
use serde::{Deserialize, Serialize};
use sha1::{Digest as Sha1Digest, Sha1};
use sha2::{Digest as Sha2Digest, Sha256, Sha512};
use tokio::{
    fs::{File, OpenOptions},
    io::{AsyncReadExt, AsyncWriteExt},
    sync::Semaphore,
};
use zip::ZipArchive;

use crate::{
    AppService, ArtifactKind, CONTENT_COMMIT_JOURNAL_NAME, ContentCommitJournal,
    ContentCommitJournalEntry, ContentInstallStage, ContentInstallTask, ContentPlanEntry,
    CoreError, InstallStage, InstallTask, InstalledContent, JavaEnvironmentStatus, JavaPlanAction,
    ManagedInstanceSummary, ResolvedArtifact, ResolvedLoader, Result, TaskState,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DownloadDisposition {
    Downloaded,
    Resumed,
    Restarted,
    ReusedShared,
    ReusedStaged,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadResult {
    pub staged_file: PathBuf,
    pub disposition: DownloadDisposition,
    pub bytes: u64,
}

#[derive(Debug, Clone)]
pub struct ArtifactDownloader {
    client: Client,
    permits: Arc<Semaphore>,
}

impl ArtifactDownloader {
    pub fn new(max_concurrent_downloads: usize) -> Result<Self> {
        if !(1..=32).contains(&max_concurrent_downloads) {
            return Err(CoreError::InvalidInstallRequest(
                "下载并发上限必须在 1 到 32 之间".to_owned(),
            ));
        }
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .user_agent(concat!("MoyuMax/", env!("CARGO_PKG_VERSION")))
            .build()?;
        Ok(Self {
            client,
            permits: Arc::new(Semaphore::new(max_concurrent_downloads)),
        })
    }

    pub async fn fetch(
        &self,
        artifact: &ResolvedArtifact,
        staging_root: &Path,
        shared_root: &Path,
    ) -> Result<DownloadResult> {
        let _permit = self
            .permits
            .acquire()
            .await
            .map_err(|_| CoreError::Download("下载协调器已经关闭".to_owned()))?;
        let staged_file = safe_join(staging_root, &artifact.relative_path)?;
        let shared_file = safe_join(shared_root, &artifact.relative_path)?;

        if file_matches(&shared_file, artifact).await? {
            return Ok(DownloadResult {
                staged_file: shared_file,
                disposition: DownloadDisposition::ReusedShared,
                bytes: artifact.size,
            });
        }
        if file_matches(&staged_file, artifact).await? {
            return Ok(DownloadResult {
                staged_file,
                disposition: DownloadDisposition::ReusedStaged,
                bytes: artifact.size,
            });
        }
        if let Some(parent) = staged_file.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let partial_file = partial_path(&staged_file);
        let mut existing_length = file_length(&partial_file).await?;
        let mut forced_restart = false;
        if artifact.size > 0 && existing_length > artifact.size {
            truncate_file(&partial_file).await?;
            existing_length = 0;
            forced_restart = true;
        }
        if existing_length > 0 && file_matches(&partial_file, artifact).await? {
            replace_file(&partial_file, &staged_file).await?;
            return Ok(DownloadResult {
                staged_file,
                disposition: DownloadDisposition::ReusedStaged,
                bytes: artifact.size,
            });
        }

        let mut request = self.client.get(&artifact.url);
        if existing_length > 0 {
            request = request.header(header::RANGE, format!("bytes={existing_length}-"));
        }
        let response = request.send().await?;
        let status = response.status();
        let (append, disposition) = if status == StatusCode::PARTIAL_CONTENT {
            validate_content_range(response.headers(), existing_length)?;
            (
                true,
                if existing_length > 0 {
                    DownloadDisposition::Resumed
                } else {
                    DownloadDisposition::Downloaded
                },
            )
        } else if status == StatusCode::OK {
            (
                false,
                if existing_length > 0 || forced_restart {
                    DownloadDisposition::Restarted
                } else {
                    DownloadDisposition::Downloaded
                },
            )
        } else if status == StatusCode::RANGE_NOT_SATISFIABLE && existing_length > 0 {
            truncate_file(&partial_file).await?;
            return self
                .fetch_without_resume(artifact, &partial_file, &staged_file)
                .await;
        } else {
            return Err(CoreError::Download(format!(
                "{} 返回 HTTP {}",
                artifact.url, status
            )));
        };

        write_response(response, &partial_file, append).await?;
        verify_file(&partial_file, artifact).await?;
        replace_file(&partial_file, &staged_file).await?;
        Ok(DownloadResult {
            staged_file,
            disposition,
            bytes: artifact.size,
        })
    }

    async fn fetch_without_resume(
        &self,
        artifact: &ResolvedArtifact,
        partial_file: &Path,
        staged_file: &Path,
    ) -> Result<DownloadResult> {
        let response = self.client.get(&artifact.url).send().await?;
        if response.status() != StatusCode::OK {
            return Err(CoreError::Download(format!(
                "{} 重新下载时返回 HTTP {}",
                artifact.url,
                response.status()
            )));
        }
        write_response(response, partial_file, false).await?;
        verify_file(partial_file, artifact).await?;
        replace_file(partial_file, staged_file).await?;
        Ok(DownloadResult {
            staged_file: staged_file.to_path_buf(),
            disposition: DownloadDisposition::Restarted,
            bytes: artifact.size,
        })
    }
}

#[derive(Debug, Clone)]
pub struct ContentExecutor {
    downloader: ArtifactDownloader,
    max_concurrent_downloads: usize,
}

impl ContentExecutor {
    pub fn new(max_concurrent_downloads: usize) -> Result<Self> {
        Ok(Self {
            downloader: ArtifactDownloader::new(max_concurrent_downloads)?,
            max_concurrent_downloads,
        })
    }

    pub async fn execute_task(
        &self,
        service: &AppService,
        task_id: &str,
    ) -> Result<Vec<InstalledContent>> {
        let task = service.content_install_task(task_id)?;
        if task.state != TaskState::Queued {
            return Err(CoreError::Content(format!(
                "内容任务当前状态 {:?} 不能开始执行",
                task.state
            )));
        }
        service.set_content_task_phase(
            task_id,
            TaskState::Running,
            ContentInstallStage::Prepare,
            "正在检查实例与文件冲突",
        )?;
        let result = self.execute_task_inner(service, &task).await;
        if let Err(error) = &result {
            let _ = service.mark_content_task_failed(task_id, &error.to_string());
        }
        result
    }

    async fn execute_task_inner(
        &self,
        service: &AppService,
        task: &ContentInstallTask,
    ) -> Result<Vec<InstalledContent>> {
        service.content_task_instance(task)?;
        let artifacts = task
            .plan
            .entries
            .iter()
            .map(content_artifact)
            .collect::<Result<Vec<_>>>()?;
        let mods_directory = preflight_content_destinations(task, &artifacts).await?;
        let total = artifacts
            .iter()
            .fold(0_u64, |sum, artifact| sum.saturating_add(artifact.size));
        service.set_content_task_progress(&task.id, 0, total, "等待下载模组文件")?;
        service.set_content_task_phase(
            &task.id,
            TaskState::Running,
            ContentInstallStage::DownloadFiles,
            "正在下载模组及必需依赖",
        )?;

        let staging_directory = PathBuf::from(&task.staging_directory);
        let download_directory = staging_directory.join("downloads");
        let shared_directory = PathBuf::from(&task.shared_store_directory);
        let downloads = self
            .download_batch(
                service,
                &task.id,
                artifacts.clone(),
                &download_directory,
                &shared_directory,
                total,
            )
            .await?;
        service.set_content_task_phase(
            &task.id,
            TaskState::Running,
            ContentInstallStage::VerifyFiles,
            "所有模组文件已通过大小、SHA-1 和 SHA-512 校验",
        )?;

        for (artifact, result) in &downloads {
            commit_shared_file(artifact, result, &shared_directory, &task.id).await?;
        }
        let publish_directory = staging_directory.join("publish");
        if tokio::fs::try_exists(&publish_directory).await? {
            tokio::fs::remove_dir_all(&publish_directory).await?;
        }
        tokio::fs::create_dir_all(&publish_directory).await?;
        for (entry, artifact) in task.plan.entries.iter().zip(&artifacts) {
            let source = safe_join(&shared_directory, &artifact.relative_path)?;
            let destination = safe_join(&publish_directory, &entry.file.filename)?;
            tokio::fs::copy(&source, &destination).await?;
            verify_file(&destination, artifact).await?;
        }

        service.set_content_task_phase(
            &task.id,
            TaskState::Committing,
            ContentInstallStage::CommitFiles,
            "正在原子发布模组文件与本地索引",
        )?;
        preflight_content_destinations(task, &artifacts).await?;
        tokio::fs::create_dir_all(&mods_directory).await?;
        let journal = ContentCommitJournal {
            schema_version: 1,
            entries: task
                .plan
                .entries
                .iter()
                .map(|entry| {
                    Ok(ContentCommitJournalEntry {
                        file_name: entry.file.filename.clone(),
                        existed_before: std::fs::exists(safe_join(
                            &mods_directory,
                            &entry.file.filename,
                        )?)?,
                    })
                })
                .collect::<Result<Vec<_>>>()?,
        };
        write_content_commit_journal(&staging_directory, &journal).await?;
        let mut published = Vec::new();
        for (entry, artifact) in task.plan.entries.iter().zip(&artifacts) {
            let prepared = safe_join(&publish_directory, &entry.file.filename)?;
            let destination = safe_join(&mods_directory, &entry.file.filename)?;
            if tokio::fs::try_exists(&destination).await? {
                if !file_matches(&destination, artifact).await? {
                    let error = CoreError::Content(format!(
                        "模组文件冲突：{} 已存在且哈希不同",
                        entry.file.filename
                    ));
                    return rollback_content_error(error, &published).await;
                }
                tokio::fs::remove_file(&prepared).await?;
                continue;
            }
            if let Err(error) = tokio::fs::rename(&prepared, &destination).await {
                return rollback_content_error(error.into(), &published).await;
            }
            published.push(destination);
        }

        let installed = match service.publish_installed_content(task) {
            Ok(installed) => installed,
            Err(error) => return rollback_content_error(error, &published).await,
        };
        if let Err(error) = tokio::fs::remove_dir_all(&staging_directory).await {
            let _ = service.note_content_cleanup_pending(&task.id, &error.to_string());
        }
        Ok(installed)
    }

    async fn download_batch(
        &self,
        service: &AppService,
        task_id: &str,
        artifacts: Vec<ResolvedArtifact>,
        staging_directory: &Path,
        shared_directory: &Path,
        total: u64,
    ) -> Result<Vec<(ResolvedArtifact, DownloadResult)>> {
        let completed = Arc::new(AtomicU64::new(0));
        let downloader = self.downloader.clone();
        stream::iter(artifacts)
            .map(|artifact| {
                let downloader = downloader.clone();
                let completed = Arc::clone(&completed);
                let service = service.clone();
                let task_id = task_id.to_owned();
                let staging_directory = staging_directory.to_path_buf();
                let shared_directory = shared_directory.to_path_buf();
                async move {
                    let result = downloader
                        .fetch(&artifact, &staging_directory, &shared_directory)
                        .await?;
                    let finished = completed
                        .fetch_add(artifact.size, Ordering::AcqRel)
                        .saturating_add(artifact.size);
                    service.set_content_task_progress(
                        &task_id,
                        finished,
                        total,
                        &artifact.relative_path,
                    )?;
                    Ok::<_, CoreError>((artifact, result))
                }
            })
            .buffer_unordered(self.max_concurrent_downloads)
            .try_collect()
            .await
    }
}

fn content_artifact(entry: &ContentPlanEntry) -> Result<ResolvedArtifact> {
    validate_hash(&entry.file.sha1, 40, "模组文件 SHA-1")?;
    validate_hash(&entry.file.sha512, 128, "模组文件 SHA-512")?;
    let prefix = entry.file.sha512.get(..2).ok_or_else(|| {
        CoreError::Content(format!("模组文件 {} 缺少 SHA-512", entry.file.filename))
    })?;
    Ok(ResolvedArtifact {
        kind: ArtifactKind::ContentMod,
        relative_path: format!(
            "content/modrinth/{prefix}/{}/{}",
            entry.file.sha512, entry.file.filename
        ),
        url: entry.file.url.clone(),
        size: entry.file.size,
        sha1: Some(entry.file.sha1.clone()),
        sha256: None,
        sha512: Some(entry.file.sha512.clone()),
    })
}

async fn preflight_content_destinations(
    task: &ContentInstallTask,
    artifacts: &[ResolvedArtifact],
) -> Result<PathBuf> {
    let root = PathBuf::from(&task.target_directory);
    if !tokio::fs::try_exists(&root).await? {
        return Err(CoreError::Content("目标实例目录不存在".to_owned()));
    }
    let mods_directory = root.join(".minecraft").join("mods");
    for (entry, artifact) in task.plan.entries.iter().zip(artifacts) {
        let destination = safe_join(&mods_directory, &entry.file.filename)?;
        if tokio::fs::try_exists(&destination).await?
            && !file_matches(&destination, artifact).await?
        {
            return Err(CoreError::Content(format!(
                "模组文件冲突：{} 已存在且哈希不同",
                entry.file.filename
            )));
        }
    }
    Ok(mods_directory)
}

async fn rollback_content_error<T>(error: CoreError, published: &[PathBuf]) -> Result<T> {
    let mut rollback_errors = Vec::new();
    for path in published.iter().rev() {
        if let Err(rollback_error) = tokio::fs::remove_file(path).await {
            rollback_errors.push(format!("{}：{rollback_error}", path.display()));
        }
    }
    if rollback_errors.is_empty() {
        Err(error)
    } else {
        Err(CoreError::Content(format!(
            "{error}；补偿回滚失败：{}",
            rollback_errors.join("；")
        )))
    }
}

async fn write_content_commit_journal(
    staging_directory: &Path,
    journal: &ContentCommitJournal,
) -> Result<()> {
    tokio::fs::create_dir_all(staging_directory).await?;
    let path = staging_directory.join(CONTENT_COMMIT_JOURNAL_NAME);
    let payload = serde_json::to_vec_pretty(journal)?;
    let mut file = File::create(path).await?;
    file.write_all(&payload).await?;
    file.flush().await?;
    file.sync_all().await?;
    Ok(())
}

#[derive(Debug, Clone)]
pub struct InstallExecutor {
    downloader: ArtifactDownloader,
    max_concurrent_downloads: usize,
    asset_base_url: String,
}

impl InstallExecutor {
    pub fn new(max_concurrent_downloads: usize) -> Result<Self> {
        Ok(Self {
            downloader: ArtifactDownloader::new(max_concurrent_downloads)?,
            max_concurrent_downloads,
            asset_base_url: "https://resources.download.minecraft.net".to_owned(),
        })
    }

    pub fn with_asset_base_url(mut self, asset_base_url: String) -> Result<Self> {
        let url = Url::parse(&asset_base_url)
            .map_err(|_| CoreError::InvalidInstallRequest("资源对象来源 URL 无效".to_owned()))?;
        if !matches!(url.scheme(), "https" | "http") || url.host_str().is_none() {
            return Err(CoreError::InvalidInstallRequest(
                "资源对象来源必须是带主机名的 HTTP(S) URL".to_owned(),
            ));
        }
        self.asset_base_url = asset_base_url.trim_end_matches('/').to_owned();
        Ok(self)
    }

    pub async fn execute_task(
        &self,
        service: &AppService,
        task_id: &str,
    ) -> Result<ManagedInstanceSummary> {
        let task = service.install_task(task_id)?;
        if task.state != TaskState::Queued {
            return Err(CoreError::InvalidInstallRequest(format!(
                "任务当前状态 {:?} 不能开始执行",
                task.state
            )));
        }
        service.set_task_phase(
            task_id,
            TaskState::Running,
            InstallStage::Prepare,
            "正在准备安装清单",
        )?;
        let result = self.execute_task_inner(service, &task).await;
        if let Err(error) = &result {
            if let JavaPlanAction::Install { environment_id, .. } = &task.plan.java_action {
                let is_ready = service
                    .list_managed_java()
                    .ok()
                    .and_then(|environments| {
                        environments
                            .into_iter()
                            .find(|environment| environment.id == *environment_id)
                    })
                    .is_some_and(|environment| environment.status == JavaEnvironmentStatus::Ready);
                if !is_ready {
                    let _ = service.mark_java_environment_status(
                        environment_id,
                        JavaEnvironmentStatus::Failed,
                    );
                }
            }
            let _ = service.mark_task_failed(task_id, &error.to_string());
        }
        result
    }

    async fn execute_task_inner(
        &self,
        service: &AppService,
        task: &InstallTask,
    ) -> Result<ManagedInstanceSummary> {
        let staging_directory = PathBuf::from(&task.staging_directory);
        let download_staging = staging_directory.join("downloads");
        let shared_directory = PathBuf::from(&task.plan.shared_store_directory);
        let platform = platform_artifacts(&task.plan.game.metadata)?;
        let mut initial_artifacts = task.plan.game.artifacts.clone();
        initial_artifacts.retain(|artifact| {
            artifact.kind != ArtifactKind::Library
                || platform.allowed_libraries.contains(&artifact.relative_path)
        });
        initial_artifacts.extend(platform.native_artifacts);
        initial_artifacts.extend(self.resolve_loader_artifacts(&task.plan.loader).await?);
        if let JavaPlanAction::Install { package, .. } = &task.plan.java_action {
            initial_artifacts.push(package.clone());
        }
        validate_artifact_paths(&initial_artifacts)?;

        let base_total = initial_artifacts
            .iter()
            .fold(0_u64, |total, artifact| total.saturating_add(artifact.size));
        let estimated_total = base_total.saturating_add(task.plan.game.asset_objects_total_bytes);
        service.set_task_progress(&task.id, 0, Some(estimated_total), Some("等待下载游戏文件"))?;
        service.set_task_phase(
            &task.id,
            TaskState::Running,
            InstallStage::DownloadGameFiles,
            "正在下载游戏文件",
        )?;

        let mut downloads = self
            .download_batch(
                initial_artifacts,
                DownloadBatchContext {
                    service: service.clone(),
                    task_id: task.id.clone(),
                    staging_root: download_staging.clone(),
                    shared_root: shared_directory.clone(),
                    completed_before: 0,
                    total: estimated_total,
                },
            )
            .await?;
        let completed_base = downloads.iter().fold(0_u64, |total, (artifact, _)| {
            total.saturating_add(artifact.size)
        });

        let asset_index_path = downloads
            .iter()
            .find(|(artifact, _)| artifact.kind == ArtifactKind::AssetIndex)
            .map(|(_, result)| result.staged_file.clone())
            .ok_or_else(|| CoreError::InvalidInstallRequest("安装计划缺少资源索引".to_owned()))?;
        let asset_artifacts = self.asset_artifacts(&asset_index_path).await?;
        let actual_asset_total = asset_artifacts
            .iter()
            .fold(0_u64, |total, artifact| total.saturating_add(artifact.size));
        let actual_total = completed_base.saturating_add(actual_asset_total);
        service.set_task_progress(
            &task.id,
            completed_base,
            Some(actual_total),
            Some("正在下载资源对象"),
        )?;
        let asset_downloads = self
            .download_batch(
                asset_artifacts,
                DownloadBatchContext {
                    service: service.clone(),
                    task_id: task.id.clone(),
                    staging_root: download_staging,
                    shared_root: shared_directory.clone(),
                    completed_before: completed_base,
                    total: actual_total,
                },
            )
            .await?;
        downloads.extend(asset_downloads);

        service.set_task_phase(
            &task.id,
            TaskState::Running,
            InstallStage::VerifyFiles,
            "所有下载文件已通过校验",
        )?;
        let (java_environment_id, java_home) = self
            .prepare_java(service, task, &downloads, &staging_directory)
            .await?;

        service.set_task_phase(
            &task.id,
            TaskState::Running,
            InstallStage::ApplyLoader,
            "正在准备实例运行时",
        )?;
        let runtime = build_runtime_manifest(task, &java_home, &downloads);
        let staged_instance = prepare_instance_directory(task, &runtime, &staging_directory)?;
        extract_native_libraries(&downloads, &staged_instance, &staging_directory)?;
        for (artifact, result) in &downloads {
            if artifact.kind != ArtifactKind::JavaArchive {
                commit_shared_file(artifact, result, &shared_directory, &task.id).await?;
            }
        }
        let target_directory = PathBuf::from(&task.target_directory);
        service.set_task_phase(
            &task.id,
            TaskState::Committing,
            InstallStage::CommitChanges,
            "正在原子提交实例",
        )?;
        if target_directory.exists() {
            return Err(CoreError::InvalidInstallRequest(
                "实例目标位置已经存在，已拒绝覆盖".to_owned(),
            ));
        }
        if let Some(parent) = target_directory.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::rename(&staged_instance, &target_directory)?;
        let published = service.publish_ready_instance(task, &java_environment_id, &runtime);
        let instance = match published {
            Ok(instance) => instance,
            Err(error) => {
                let _ = fs::rename(&target_directory, &staged_instance);
                return Err(error);
            }
        };
        let _ = fs::remove_dir_all(&staging_directory);
        Ok(instance)
    }

    async fn download_batch(
        &self,
        artifacts: Vec<ResolvedArtifact>,
        context: DownloadBatchContext,
    ) -> Result<Vec<(ResolvedArtifact, DownloadResult)>> {
        let completed = Arc::new(AtomicU64::new(context.completed_before));
        let downloader = self.downloader.clone();
        stream::iter(artifacts)
            .map(|artifact| {
                let downloader = downloader.clone();
                let completed = Arc::clone(&completed);
                let service = context.service.clone();
                let task_id = context.task_id.clone();
                let staging_root = context.staging_root.clone();
                let shared_root = context.shared_root.clone();
                let total = context.total;
                async move {
                    let result = downloader
                        .fetch(&artifact, &staging_root, &shared_root)
                        .await?;
                    let finished = completed
                        .fetch_add(artifact.size, Ordering::AcqRel)
                        .saturating_add(artifact.size);
                    service.set_task_progress(
                        &task_id,
                        finished,
                        Some(total),
                        Some(&artifact.relative_path),
                    )?;
                    Ok::<_, CoreError>((artifact, result))
                }
            })
            .buffer_unordered(self.max_concurrent_downloads)
            .try_collect()
            .await
    }

    async fn asset_artifacts(&self, index_path: &Path) -> Result<Vec<ResolvedArtifact>> {
        let payload = tokio::fs::read(index_path).await?;
        let index: AssetIndex = serde_json::from_slice(&payload)?;
        let mut seen = HashSet::new();
        let mut artifacts = Vec::new();
        for object in index.objects.into_values() {
            validate_hash(&object.hash, 40, "资源对象 SHA-1")?;
            if !seen.insert(object.hash.clone()) {
                continue;
            }
            let prefix = &object.hash[..2];
            artifacts.push(ResolvedArtifact {
                kind: ArtifactKind::AssetObject,
                relative_path: format!("minecraft/assets/objects/{prefix}/{}", object.hash),
                url: format!("{}/{prefix}/{}", self.asset_base_url, object.hash),
                size: object.size,
                sha1: Some(object.hash),
                sha256: None,
                sha512: None,
            });
        }
        Ok(artifacts)
    }

    async fn resolve_loader_artifacts(
        &self,
        loader: &ResolvedLoader,
    ) -> Result<Vec<ResolvedArtifact>> {
        let ResolvedLoader::Fabric { profile, .. } = loader else {
            return Ok(Vec::new());
        };
        let profile: FabricProfile = serde_json::from_value(profile.clone())?;
        let mut artifacts = Vec::with_capacity(profile.libraries.len());
        for library in profile.libraries {
            let relative = maven_path(&library.name)?;
            let base = library
                .url
                .unwrap_or_else(|| "https://libraries.minecraft.net/".to_owned());
            let base = base.trim_end_matches('/');
            let url = format!("{base}/{relative}");
            let sha1 = if let Some(value) = library.sha1 {
                validate_hash(&value, 40, "Fabric 库 SHA-1")?;
                value
            } else {
                let value = self
                    .downloader
                    .client
                    .get(format!("{url}.sha1"))
                    .send()
                    .await?
                    .error_for_status()?
                    .text()
                    .await?;
                let value = value.trim().to_owned();
                validate_hash(&value, 40, "Fabric Maven SHA-1")?;
                value
            };
            let size = if let Some(size) = library.size {
                size
            } else {
                self.downloader
                    .client
                    .head(&url)
                    .send()
                    .await?
                    .error_for_status()?
                    .content_length()
                    .ok_or_else(|| {
                        CoreError::Metadata(format!(
                            "Fabric 库 {} 没有提供可验证大小",
                            library.name
                        ))
                    })?
            };
            artifacts.push(ResolvedArtifact {
                kind: ArtifactKind::LoaderLibrary,
                relative_path: format!("minecraft/libraries/{relative}"),
                url,
                size,
                sha1: Some(sha1),
                sha256: library.sha256,
                sha512: None,
            });
        }
        Ok(artifacts)
    }

    async fn prepare_java(
        &self,
        service: &AppService,
        task: &InstallTask,
        downloads: &[(ResolvedArtifact, DownloadResult)],
        staging_directory: &Path,
    ) -> Result<(String, String)> {
        service.set_task_phase(
            &task.id,
            TaskState::Running,
            InstallStage::InstallGameEnvironment,
            "正在安装游戏环境",
        )?;
        match &task.plan.java_action {
            JavaPlanAction::Reuse {
                environment_id,
                home_directory,
            } => Ok((environment_id.clone(), home_directory.clone())),
            JavaPlanAction::AwaitExistingInstall {
                environment_id,
                target_directory,
            } => {
                let environment = service
                    .list_managed_java()?
                    .into_iter()
                    .find(|environment| environment.id == *environment_id)
                    .ok_or_else(|| {
                        CoreError::InvalidStoredState("等待的 Java 环境不存在".to_owned())
                    })?;
                if environment.status != JavaEnvironmentStatus::Ready {
                    return Err(CoreError::InvalidStoredState(
                        "共享 Java 环境尚未安装完成，请稍后重试".to_owned(),
                    ));
                }
                Ok((environment_id.clone(), target_directory.clone()))
            }
            JavaPlanAction::Install {
                environment_id,
                target_directory,
                ..
            } => {
                service.mark_java_environment_status(
                    environment_id,
                    JavaEnvironmentStatus::Installing,
                )?;
                let archive = downloads
                    .iter()
                    .find(|(artifact, _)| artifact.kind == ArtifactKind::JavaArchive)
                    .map(|(_, result)| result.staged_file.clone())
                    .ok_or_else(|| {
                        CoreError::InvalidStoredState("Java 安装包没有完成下载".to_owned())
                    })?;
                let extraction = staging_directory.join("java-environment");
                if extraction.exists() {
                    fs::remove_dir_all(&extraction)?;
                }
                let archive_for_worker = archive.clone();
                let extraction_for_worker = extraction.clone();
                tokio::task::spawn_blocking(move || {
                    extract_zip_safely(
                        &archive_for_worker,
                        &extraction_for_worker,
                        ArchiveLimits::java_default(),
                    )
                })
                .await
                .map_err(|error| CoreError::Archive(format!("Java 解包线程失败：{error}")))??;
                let java_executable = extraction.join("bin/java.exe");
                if !java_executable.is_file() {
                    return Err(CoreError::Archive(
                        "Azul JDK 解包后缺少 bin/java.exe".to_owned(),
                    ));
                }
                let target = PathBuf::from(target_directory);
                if target.exists() {
                    if !target.join("bin/java.exe").is_file() {
                        return Err(CoreError::Archive(
                            "托管 Java 目标已存在但不完整".to_owned(),
                        ));
                    }
                    fs::remove_dir_all(&extraction)?;
                } else {
                    if let Some(parent) = target.parent() {
                        fs::create_dir_all(parent)?;
                    }
                    fs::rename(&extraction, &target)?;
                }
                service
                    .mark_java_environment_status(environment_id, JavaEnvironmentStatus::Ready)?;
                Ok((environment_id.clone(), target_directory.clone()))
            }
        }
    }
}

struct DownloadBatchContext {
    service: AppService,
    task_id: String,
    staging_root: PathBuf,
    shared_root: PathBuf,
    completed_before: u64,
    total: u64,
}

#[derive(Debug, Deserialize)]
struct AssetIndex {
    objects: HashMap<String, AssetObject>,
}

#[derive(Debug, Deserialize)]
struct AssetObject {
    hash: String,
    size: u64,
}

#[derive(Debug, Deserialize)]
struct FabricProfile {
    #[serde(default)]
    libraries: Vec<FabricLibrary>,
}

#[derive(Debug, Deserialize)]
struct FabricLibrary {
    name: String,
    url: Option<String>,
    sha1: Option<String>,
    sha256: Option<String>,
    size: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RuntimeMetadata {
    #[serde(default)]
    libraries: Vec<RuntimeLibrary>,
}

#[derive(Debug, Deserialize)]
struct RuntimeLibrary {
    name: String,
    downloads: Option<RuntimeLibraryDownloads>,
    #[serde(default)]
    natives: HashMap<String, String>,
    #[serde(default)]
    rules: Vec<RuntimeRule>,
}

#[derive(Debug, Deserialize)]
struct RuntimeLibraryDownloads {
    artifact: Option<RuntimeLibraryArtifact>,
    #[serde(default)]
    classifiers: HashMap<String, RuntimeLibraryArtifact>,
}

#[derive(Debug, Deserialize)]
struct RuntimeLibraryArtifact {
    path: String,
    url: String,
    sha1: String,
    size: u64,
}

#[derive(Debug, Deserialize)]
struct RuntimeRule {
    action: String,
    os: Option<RuntimeRuleOs>,
    #[serde(default)]
    features: HashMap<String, bool>,
}

#[derive(Debug, Deserialize)]
struct RuntimeRuleOs {
    name: Option<String>,
    arch: Option<String>,
    version: Option<String>,
}

struct PlatformArtifacts {
    allowed_libraries: HashSet<String>,
    native_artifacts: Vec<ResolvedArtifact>,
}

fn platform_artifacts(metadata: &serde_json::Value) -> Result<PlatformArtifacts> {
    let metadata: RuntimeMetadata = serde_json::from_value(metadata.clone())?;
    let mut allowed_libraries = HashSet::new();
    let mut native_artifacts = Vec::new();
    let mut has_modern_windows_native = false;
    for library in metadata.libraries {
        if !library_allowed(&library.rules)? {
            continue;
        }
        let Some(downloads) = library.downloads else {
            continue;
        };
        if let Some(is_windows_x64) = modern_windows_native_classifier(&library.name)? {
            if !is_windows_x64 {
                continue;
            }
            let native = downloads.artifact.ok_or_else(|| {
                CoreError::Metadata(format!("原生库 {} 缺少下载 artifact", library.name))
            })?;
            validate_hash(&native.sha1, 40, "原生库 SHA-1")?;
            allowed_libraries.insert(format!("minecraft/libraries/{}", native.path));
            has_modern_windows_native = true;
            continue;
        }
        if let Some(artifact) = downloads.artifact {
            allowed_libraries.insert(format!("minecraft/libraries/{}", artifact.path));
        }
        let Some(classifier) = library.natives.get("windows") else {
            continue;
        };
        let classifier = classifier.replace("${arch}", "64");
        let native = downloads.classifiers.get(&classifier).ok_or_else(|| {
            CoreError::Metadata(format!("原生库 {} 缺少分类器 {classifier}", library.name))
        })?;
        validate_hash(&native.sha1, 40, "原生库 SHA-1")?;
        native_artifacts.push(ResolvedArtifact {
            kind: ArtifactKind::NativeLibrary,
            relative_path: format!("minecraft/libraries/{}", native.path),
            url: native.url.clone(),
            size: native.size,
            sha1: Some(native.sha1.clone()),
            sha256: None,
            sha512: None,
        });
    }
    if native_artifacts.is_empty() && !has_modern_windows_native {
        return Err(CoreError::Metadata(
            "Minecraft 元数据没有提供 Windows x64 原生库".to_owned(),
        ));
    }
    Ok(PlatformArtifacts {
        allowed_libraries,
        native_artifacts,
    })
}

fn modern_windows_native_classifier(coordinate: &str) -> Result<Option<bool>> {
    let coordinate = coordinate
        .rsplit_once('@')
        .map_or(coordinate, |(value, _)| value);
    let Some(classifier) = coordinate.split(':').nth(3) else {
        return Ok(None);
    };
    if !classifier.starts_with("natives-windows") {
        return Ok(None);
    }
    match classifier {
        "natives-windows" | "natives-windows-x86_64" | "natives-windows-amd64" => Ok(Some(true)),
        "natives-windows-x86" | "natives-windows-arm64" | "natives-windows-aarch64" => {
            Ok(Some(false))
        }
        _ => Err(CoreError::Metadata(format!(
            "无法识别 Windows 原生库架构分类器：{classifier}"
        ))),
    }
}

fn library_allowed(rules: &[RuntimeRule]) -> Result<bool> {
    if rules.is_empty() {
        return Ok(true);
    }
    let mut allowed = false;
    for rule in rules {
        if rule_applies(rule)? {
            allowed = rule.action == "allow";
        }
    }
    Ok(allowed)
}

fn rule_applies(rule: &RuntimeRule) -> Result<bool> {
    if let Some(os) = &rule.os {
        if os.name.as_deref().is_some_and(|name| name != "windows") {
            return Ok(false);
        }
        if let Some(arch) = &os.arch {
            let expression = Regex::new(&format!("^(?:{arch})$"))
                .map_err(|error| CoreError::Metadata(format!("原生库架构规则无效：{error}")))?;
            if !expression.is_match("x86_64") && !expression.is_match("amd64") {
                return Ok(false);
            }
        }
        if let Some(version) = &os.version {
            let expression = Regex::new(&format!("^(?:{version})$"))
                .map_err(|error| CoreError::Metadata(format!("原生库系统版本规则无效：{error}")))?;
            if !expression.is_match("10.0") {
                return Ok(false);
            }
        }
    }
    for expected in rule.features.values() {
        if *expected {
            return Ok(false);
        }
    }
    Ok(true)
}

fn extract_native_libraries(
    downloads: &[(ResolvedArtifact, DownloadResult)],
    staged_instance: &Path,
    staging_directory: &Path,
) -> Result<()> {
    let natives_directory = staged_instance.join("natives");
    let mut extracted_dlls = 0_usize;
    let mut native_archives = 0_usize;
    for (index, (_, result)) in downloads
        .iter()
        .filter(|(artifact, _)| artifact.kind == ArtifactKind::NativeLibrary)
        .enumerate()
    {
        native_archives = native_archives.saturating_add(1);
        let temporary = staging_directory.join(format!("native-extract-{index}"));
        if temporary.exists() {
            fs::remove_dir_all(&temporary)?;
        }
        extract_zip_safely(
            &result.staged_file,
            &temporary,
            ArchiveLimits::natives_default(),
        )?;
        extracted_dlls =
            extracted_dlls.saturating_add(copy_native_dlls(&temporary, &natives_directory)?);
        fs::remove_dir_all(temporary)?;
    }
    if native_archives > 0 && extracted_dlls == 0 {
        return Err(CoreError::Archive(
            "Windows x64 原生库没有解包出任何 DLL".to_owned(),
        ));
    }
    Ok(())
}

fn copy_native_dlls(source: &Path, destination: &Path) -> Result<usize> {
    let mut copied = 0_usize;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            copied = copied.saturating_add(copy_native_dlls(&path, destination)?);
            continue;
        }
        if path
            .extension()
            .and_then(|value| value.to_str())
            .is_none_or(|value| !value.eq_ignore_ascii_case("dll"))
        {
            continue;
        }
        let target = destination.join(entry.file_name());
        if target.exists() {
            if fs::read(&target)? != fs::read(&path)? {
                return Err(CoreError::Archive(format!(
                    "原生库文件名冲突：{}",
                    target.display()
                )));
            }
        } else {
            fs::copy(path, target)?;
        }
        copied = copied.saturating_add(1);
    }
    Ok(copied)
}

fn validate_artifact_paths(artifacts: &[ResolvedArtifact]) -> Result<()> {
    let mut paths = HashSet::new();
    for artifact in artifacts {
        safe_join(Path::new("."), &artifact.relative_path)?;
        if !paths.insert(&artifact.relative_path) {
            return Err(CoreError::InvalidInstallRequest(format!(
                "安装计划包含重复目标路径：{}",
                artifact.relative_path
            )));
        }
    }
    Ok(())
}

async fn commit_shared_file(
    artifact: &ResolvedArtifact,
    result: &DownloadResult,
    shared_root: &Path,
    task_id: &str,
) -> Result<()> {
    let destination = safe_join(shared_root, &artifact.relative_path)?;
    if result.disposition == DownloadDisposition::ReusedShared {
        return Ok(());
    }
    if file_matches(&destination, artifact).await? {
        if result.staged_file != destination {
            let _ = tokio::fs::remove_file(&result.staged_file).await;
        }
        return Ok(());
    }
    if let Some(parent) = destination.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let backup = destination.with_extension(format!("moyumax-backup-{task_id}"));
    let had_destination = tokio::fs::try_exists(&destination).await?;
    if had_destination {
        if tokio::fs::try_exists(&backup).await? {
            tokio::fs::remove_file(&backup).await?;
        }
        tokio::fs::rename(&destination, &backup).await?;
    }
    if let Err(error) = tokio::fs::rename(&result.staged_file, &destination).await {
        if had_destination {
            let _ = tokio::fs::rename(&backup, &destination).await;
        }
        return Err(error.into());
    }
    if had_destination {
        let _ = tokio::fs::remove_file(&backup).await;
    }
    Ok(())
}

fn prepare_instance_directory(
    task: &InstallTask,
    runtime: &serde_json::Value,
    staging_directory: &Path,
) -> Result<PathBuf> {
    let instance = staging_directory.join("instance");
    if instance.exists() {
        fs::remove_dir_all(&instance)?;
    }
    for relative in [
        ".minecraft/saves",
        ".minecraft/mods",
        ".minecraft/config",
        ".minecraft/resourcepacks",
        ".minecraft/shaderpacks",
        ".minecraft/screenshots",
        ".minecraft/logs",
        ".moyumax/snapshots",
        "natives",
    ] {
        fs::create_dir_all(instance.join(relative))?;
    }
    write_json(&instance.join(".moyumax/install-plan.json"), &task.plan)?;
    write_json(&instance.join(".moyumax/runtime.json"), runtime)?;
    write_json(
        &instance.join(".moyumax/snapshots/initial.json"),
        &serde_json::json!({
            "schemaVersion": 1,
            "kind": "initial-install",
            "taskId": task.id,
        }),
    )?;
    Ok(instance)
}

fn build_runtime_manifest(
    task: &InstallTask,
    java_home: &str,
    downloads: &[(ResolvedArtifact, DownloadResult)],
) -> serde_json::Value {
    let classpath: Vec<&str> = downloads
        .iter()
        .filter(|(artifact, _)| {
            matches!(
                artifact.kind,
                ArtifactKind::GameClient | ArtifactKind::Library | ArtifactKind::LoaderLibrary
            )
        })
        .map(|(artifact, _)| artifact.relative_path.as_str())
        .collect();
    let (main_class, loader_profile) = match &task.plan.loader {
        ResolvedLoader::Vanilla => (task.plan.game.main_class.clone(), None),
        ResolvedLoader::Fabric { profile, .. } => (
            profile
                .get("mainClass")
                .and_then(serde_json::Value::as_str)
                .unwrap_or(&task.plan.game.main_class)
                .to_owned(),
            Some(profile.clone()),
        ),
    };
    serde_json::json!({
        "schemaVersion": 1,
        "gameVersion": task.plan.game.version.id,
        "mainClass": main_class,
        "javaHome": java_home,
        "sharedStore": task.plan.shared_store_directory,
        "workingDirectory": ".minecraft",
        "nativesDirectory": "natives",
        "classpath": classpath,
        "gameMetadata": task.plan.game.metadata,
        "loaderProfile": loader_profile,
        "isolation": task.plan.isolation,
    })
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| CoreError::InvalidStoredState("JSON 目标缺少父目录".to_owned()))?;
    fs::create_dir_all(parent)?;
    let temporary = path.with_extension("json.tmp");
    let payload = serde_json::to_vec_pretty(value)?;
    fs::write(&temporary, payload)?;
    fs::rename(temporary, path)?;
    Ok(())
}

fn validate_hash(value: &str, length: usize, label: &str) -> Result<()> {
    if value.len() != length || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(CoreError::Metadata(format!("{label} 格式无效")));
    }
    Ok(())
}

fn maven_path(coordinate: &str) -> Result<String> {
    let (coordinate, extension) = coordinate
        .rsplit_once('@')
        .map_or((coordinate, "jar"), |(value, extension)| (value, extension));
    let parts: Vec<&str> = coordinate.split(':').collect();
    if !(3..=4).contains(&parts.len())
        || parts.iter().any(|part| {
            part.is_empty()
                || !part.chars().all(|character| {
                    character.is_ascii_alphanumeric() || ".-_+".contains(character)
                })
        })
        || extension.is_empty()
        || !extension
            .chars()
            .all(|character| character.is_ascii_alphanumeric())
    {
        return Err(CoreError::Metadata(format!(
            "Fabric Maven 坐标无效：{coordinate}"
        )));
    }
    let group = parts[0].replace('.', "/");
    let artifact = parts[1];
    let version = parts[2];
    let classifier = parts
        .get(3)
        .map_or(String::new(), |value| format!("-{value}"));
    Ok(format!(
        "{group}/{artifact}/{version}/{artifact}-{version}{classifier}.{extension}"
    ))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArchiveLimits {
    pub max_entries: usize,
    pub max_uncompressed_bytes: u64,
    pub max_expansion_ratio: u64,
}

impl ArchiveLimits {
    #[must_use]
    pub const fn java_default() -> Self {
        Self {
            max_entries: 100_000,
            max_uncompressed_bytes: 2 * 1024 * 1024 * 1024,
            max_expansion_ratio: 40,
        }
    }

    #[must_use]
    pub const fn natives_default() -> Self {
        Self {
            max_entries: 10_000,
            max_uncompressed_bytes: 512 * 1024 * 1024,
            max_expansion_ratio: 80,
        }
    }
}

pub fn extract_zip_safely(
    archive_path: &Path,
    target_directory: &Path,
    limits: ArchiveLimits,
) -> Result<()> {
    if target_directory.exists() {
        return Err(CoreError::Archive(
            "解包目标已经存在，已拒绝覆盖".to_owned(),
        ));
    }
    let archive_size = fs::metadata(archive_path)?.len().max(1);
    let archive_file = fs::File::open(archive_path)?;
    let mut archive = ZipArchive::new(archive_file)
        .map_err(|error| CoreError::Archive(format!("无法读取 ZIP：{error}")))?;
    if archive.len() > limits.max_entries {
        return Err(CoreError::Archive(format!(
            "ZIP 条目数 {} 超过上限 {}",
            archive.len(),
            limits.max_entries
        )));
    }

    let mut total_uncompressed = 0_u64;
    let mut common_root: Option<OsString> = None;
    let mut can_strip_root = true;
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|error| CoreError::Archive(format!("无法读取 ZIP 条目：{error}")))?;
        let path = entry.enclosed_name().ok_or_else(|| {
            CoreError::Archive(format!("ZIP 条目包含不安全路径：{}", entry.name()))
        })?;
        if entry
            .unix_mode()
            .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err(CoreError::Archive(format!(
                "ZIP 条目是符号链接，已拒绝：{}",
                entry.name()
            )));
        }
        total_uncompressed = total_uncompressed.saturating_add(entry.size());
        if total_uncompressed > limits.max_uncompressed_bytes
            || total_uncompressed > archive_size.saturating_mul(limits.max_expansion_ratio)
        {
            return Err(CoreError::Archive("ZIP 展开大小超过安全上限".to_owned()));
        }
        let mut components = path.components();
        let first = components.next().and_then(|component| match component {
            Component::Normal(value) => Some(value.to_os_string()),
            _ => None,
        });
        if let Some(first) = first {
            if let Some(root) = &common_root {
                if root != &first {
                    can_strip_root = false;
                }
            } else {
                common_root = Some(first);
            }
            if components.next().is_none() && !entry.is_dir() {
                can_strip_root = false;
            }
        }
    }

    fs::create_dir_all(target_directory)?;
    let extraction = extract_validated_archive(
        &mut archive,
        target_directory,
        can_strip_root.then_some(common_root).flatten().as_deref(),
    );
    if extraction.is_err() {
        let _ = fs::remove_dir_all(target_directory);
    }
    extraction
}

fn extract_validated_archive(
    archive: &mut ZipArchive<fs::File>,
    target_directory: &Path,
    strip_root: Option<&std::ffi::OsStr>,
) -> Result<()> {
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| CoreError::Archive(format!("无法读取 ZIP 条目：{error}")))?;
        let enclosed = entry.enclosed_name().ok_or_else(|| {
            CoreError::Archive(format!("ZIP 条目包含不安全路径：{}", entry.name()))
        })?;
        let relative = if let Some(root) = strip_root {
            enclosed
                .strip_prefix(root)
                .map_err(|_| CoreError::Archive("ZIP 顶层目录结构在校验后发生变化".to_owned()))?
        } else {
            enclosed.as_path()
        };
        if relative.as_os_str().is_empty() {
            continue;
        }
        let destination = safe_join_path(target_directory, relative)?;
        if entry.is_dir() {
            fs::create_dir_all(&destination)?;
            continue;
        }
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut output = fs::File::create(&destination)?;
        std::io::copy(&mut entry, &mut output)?;
        output.flush()?;
        #[cfg(unix)]
        if let Some(mode) = entry.unix_mode() {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&destination, fs::Permissions::from_mode(mode & 0o777))?;
        }
    }
    Ok(())
}

async fn write_response(
    response: reqwest::Response,
    partial_file: &Path,
    append: bool,
) -> Result<()> {
    let mut output = OpenOptions::new()
        .create(true)
        .write(true)
        .append(append)
        .truncate(!append)
        .open(partial_file)
        .await?;
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        output.write_all(&chunk?).await?;
    }
    output.flush().await?;
    output.sync_data().await?;
    Ok(())
}

async fn verify_file(path: &Path, artifact: &ResolvedArtifact) -> Result<()> {
    let metadata = tokio::fs::metadata(path).await?;
    if artifact.size > 0 && metadata.len() != artifact.size {
        return Err(CoreError::Download(format!(
            "{} 大小不匹配：预期 {}，实际 {}",
            artifact.relative_path,
            artifact.size,
            metadata.len()
        )));
    }

    let mut file = File::open(path).await?;
    let mut buffer = vec![0_u8; 128 * 1024];
    let mut sha1 = artifact.sha1.as_ref().map(|_| Sha1::new());
    let mut sha256 = artifact.sha256.as_ref().map(|_| Sha256::new());
    let mut sha512 = artifact.sha512.as_ref().map(|_| Sha512::new());
    loop {
        let read = file.read(&mut buffer).await?;
        if read == 0 {
            break;
        }
        if let Some(hasher) = &mut sha1 {
            Sha1Digest::update(hasher, &buffer[..read]);
        }
        if let Some(hasher) = &mut sha256 {
            Sha2Digest::update(hasher, &buffer[..read]);
        }
        if let Some(hasher) = &mut sha512 {
            Sha2Digest::update(hasher, &buffer[..read]);
        }
    }
    if let (Some(expected), Some(hasher)) = (&artifact.sha1, sha1) {
        let actual = encode_hex(Sha1Digest::finalize(hasher));
        if !actual.eq_ignore_ascii_case(expected) {
            return Err(CoreError::Download(format!(
                "{} 的 SHA-1 不匹配",
                artifact.relative_path
            )));
        }
    }
    if let (Some(expected), Some(hasher)) = (&artifact.sha256, sha256) {
        let actual = encode_hex(Sha2Digest::finalize(hasher));
        if !actual.eq_ignore_ascii_case(expected) {
            return Err(CoreError::Download(format!(
                "{} 的 SHA-256 不匹配",
                artifact.relative_path
            )));
        }
    }
    if let (Some(expected), Some(hasher)) = (&artifact.sha512, sha512) {
        let actual = encode_hex(Sha2Digest::finalize(hasher));
        if !actual.eq_ignore_ascii_case(expected) {
            return Err(CoreError::Download(format!(
                "{} 的 SHA-512 不匹配",
                artifact.relative_path
            )));
        }
    }
    if artifact.sha1.is_none() && artifact.sha256.is_none() && artifact.sha512.is_none() {
        return Err(CoreError::Download(format!(
            "{} 缺少可信校验值",
            artifact.relative_path
        )));
    }
    Ok(())
}

async fn file_matches(path: &Path, artifact: &ResolvedArtifact) -> Result<bool> {
    match tokio::fs::metadata(path).await {
        Ok(metadata) if metadata.is_file() => match verify_file(path, artifact).await {
            Ok(()) => Ok(true),
            Err(CoreError::Download(_)) => Ok(false),
            Err(error) => Err(error),
        },
        Ok(_) => Ok(false),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

async fn file_length(path: &Path) -> Result<u64> {
    match tokio::fs::metadata(path).await {
        Ok(metadata) if metadata.is_file() => Ok(metadata.len()),
        Ok(_) => Err(CoreError::Download(format!(
            "未完成下载路径不是文件：{}",
            path.display()
        ))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(0),
        Err(error) => Err(error.into()),
    }
}

async fn truncate_file(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(path)
        .await?;
    Ok(())
}

async fn replace_file(source: &Path, destination: &Path) -> Result<()> {
    if let Some(parent) = destination.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    if tokio::fs::try_exists(destination).await? {
        tokio::fs::remove_file(destination).await?;
    }
    tokio::fs::rename(source, destination).await?;
    Ok(())
}

fn validate_content_range(headers: &header::HeaderMap, expected_start: u64) -> Result<()> {
    let value = headers
        .get(header::CONTENT_RANGE)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| CoreError::Download("206 响应缺少 Content-Range".to_owned()))?;
    let expected_prefix = format!("bytes {expected_start}-");
    if !value.starts_with(&expected_prefix) {
        return Err(CoreError::Download(format!(
            "Content-Range 与本地长度不一致：{value}"
        )));
    }
    Ok(())
}

fn safe_join(root: &Path, relative: &str) -> Result<PathBuf> {
    safe_join_path(root, Path::new(relative))
}

fn safe_join_path(root: &Path, relative: &Path) -> Result<PathBuf> {
    if relative.as_os_str().is_empty() {
        return Err(CoreError::Download("下载目标相对路径为空".to_owned()));
    }
    let mut destination = root.to_path_buf();
    for component in relative.components() {
        match component {
            Component::Normal(value) => destination.push(value),
            _ => {
                return Err(CoreError::Download(format!(
                    "目标路径包含不安全组件：{}",
                    relative.display()
                )));
            }
        }
    }
    Ok(destination)
}

fn partial_path(staged_file: &Path) -> PathBuf {
    let extension = staged_file
        .extension()
        .and_then(|value| value.to_str())
        .map_or_else(|| "part".to_owned(), |value| format!("{value}.part"));
    staged_file.with_extension(extension)
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
