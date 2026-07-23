use std::{
    collections::{HashMap, HashSet},
    ffi::OsString,
    fs,
    io::Write,
    path::{Component, Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
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
    sync::{Notify, Semaphore},
};
use zip::ZipArchive;

use crate::{
    AppService, ArtifactKind, CONTENT_COMMIT_JOURNAL_NAME, ContentCommitJournal,
    ContentCommitJournalEntry, ContentInstallStage, ContentInstallTask, ContentPlanEntry,
    CoreError, DownloadCandidate, InstallStage, InstallTask, InstalledContent,
    JavaEnvironmentStatus, JavaPlanAction, ManagedInstanceSummary, ResolvedArtifact,
    ResolvedLoader, Result, SourceCandidates, SourceChannel, SourcePolicy, TaskState,
    candidates_for, content_update_snapshot_directory,
};

/// 全局暂停全部任务时的下载中断信号。
///
/// 中断只在文件分段边界生效：响应流停止写入，已写入的 `.partial`
/// 保留，恢复后按既有续传逻辑先校验再继续。信号一次性生效，不可复位。
#[derive(Debug, Clone, Default)]
pub struct DownloadInterrupt {
    interrupted: Arc<AtomicBool>,
    notify: Arc<Notify>,
}

impl DownloadInterrupt {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn interrupt(&self) {
        self.interrupted.store(true, Ordering::Release);
        self.notify.notify_waiters();
    }

    #[must_use]
    pub fn is_interrupted(&self) -> bool {
        self.interrupted.load(Ordering::Acquire)
    }

    /// 等待中断信号。先注册等待再检查标志，避免错过已发出的中断。
    async fn wait(&self) {
        let notified = self.notify.notified();
        tokio::pin!(notified);
        notified.as_mut().enable();
        if self.is_interrupted() {
            return;
        }
        notified.await;
    }
}

/// 全局下载限速器：所有下载连接共用的令牌桶。
/// `rate = 0` 表示不限速;分段下载也经过同一令牌桶,不会系统性突破上限。
#[derive(Debug)]
pub struct RateLimiter {
    rate_bytes_per_sec: AtomicU64,
    state: tokio::sync::Mutex<(f64, Instant)>,
}

impl RateLimiter {
    #[must_use]
    pub fn new() -> Self {
        Self {
            rate_bytes_per_sec: AtomicU64::new(0),
            state: tokio::sync::Mutex::new((0.0, Instant::now())),
        }
    }

    pub fn set_rate(&self, bytes_per_sec: u64) {
        self.rate_bytes_per_sec
            .store(bytes_per_sec, Ordering::Release);
    }

    #[must_use]
    pub fn rate(&self) -> u64 {
        self.rate_bytes_per_sec.load(Ordering::Acquire)
    }

    /// 获取 bytes 额度;不限速时立即返回。暂停中断可打断等待。
    pub async fn acquire(&self, bytes: u64, interrupt: Option<&DownloadInterrupt>) -> Result<()> {
        let rate = self.rate();
        if rate == 0 {
            return Ok(());
        }
        let rate = rate as f64;
        let needed = bytes as f64;
        loop {
            if interrupt.is_some_and(DownloadInterrupt::is_interrupted) {
                return Err(CoreError::TaskPaused);
            }
            let wait = {
                let mut state = self.state.lock().await;
                let elapsed = state.1.elapsed().as_secs_f64();
                state.0 = (state.0 + elapsed * rate).min(rate);
                state.1 = Instant::now();
                if state.0 >= needed {
                    state.0 -= needed;
                    return Ok(());
                }
                (needed - state.0) / rate
            };
            let wait = Duration::from_secs_f64(wait.clamp(0.005, 1.0));
            match interrupt {
                Some(signal) => {
                    tokio::select! {
                        () = tokio::time::sleep(wait) => {}
                        () = signal.wait() => return Err(CoreError::TaskPaused),
                    }
                }
                None => tokio::time::sleep(wait).await,
            }
        }
    }
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

static GLOBAL_RATE_LIMITER: std::sync::OnceLock<Arc<RateLimiter>> = std::sync::OnceLock::new();

/// 全局共享限速器:桌面、执行器与未来 CLI 使用同一令牌桶。
pub fn global_rate_limiter() -> Arc<RateLimiter> {
    GLOBAL_RATE_LIMITER
        .get_or_init(|| Arc::new(RateLimiter::new()))
        .clone()
}

/// 压力感知并发控制器:按每连接吞吐与失败信号收缩有效连接数,
/// 恢复稳定后缓慢回升。压力信号只来自网络行为(吞吐与失败)。
#[derive(Debug)]
pub struct AdaptiveConcurrency {
    max: usize,
    current: AtomicUsize,
    in_flight: AtomicUsize,
    samples: std::sync::Mutex<std::collections::VecDeque<AdaptiveSample>>,
    healthy_streak: AtomicUsize,
}

#[derive(Debug, Clone, Copy)]
struct AdaptiveSample {
    bytes_per_sec: f64,
}

const ADAPTIVE_WINDOW: usize = 16;
const ADAPTIVE_SLOW_BPS: f64 = 256.0 * 1024.0;

impl AdaptiveConcurrency {
    #[must_use]
    pub fn new(max: usize) -> Self {
        Self {
            max,
            current: AtomicUsize::new(max.max(1)),
            in_flight: AtomicUsize::new(0),
            samples: std::sync::Mutex::new(std::collections::VecDeque::new()),
            healthy_streak: AtomicUsize::new(0),
        }
    }

    #[must_use]
    pub fn current_limit(&self) -> usize {
        self.current.load(Ordering::Acquire)
    }

    /// 等到有自适应额度;返回在途计数守卫。
    pub async fn acquire(
        &self,
        interrupt: Option<&DownloadInterrupt>,
    ) -> Result<AdaptivePermit<'_>> {
        loop {
            if interrupt.is_some_and(DownloadInterrupt::is_interrupted) {
                return Err(CoreError::TaskPaused);
            }
            let limit = self.current_limit();
            let in_flight = self.in_flight.load(Ordering::Acquire);
            if in_flight < limit {
                if self
                    .in_flight
                    .compare_exchange(
                        in_flight,
                        in_flight + 1,
                        Ordering::AcqRel,
                        Ordering::Acquire,
                    )
                    .is_ok()
                {
                    return Ok(AdaptivePermit { controller: self });
                }
                continue;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }

    fn release(&self) {
        self.in_flight.fetch_sub(1, Ordering::AcqRel);
    }

    /// 记录一次下载结果。失败或吞吐显著劣化时收缩,稳定健康时缓慢回升。
    pub fn record(&self, bytes_per_sec: f64, success: bool) {
        let mut shrink = false;
        let mut grow = false;
        {
            let mut samples = self
                .samples
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            samples.push_back(AdaptiveSample { bytes_per_sec });
            if samples.len() > ADAPTIVE_WINDOW {
                samples.pop_front();
            }
            if !success {
                shrink = true;
                self.healthy_streak.store(0, Ordering::Release);
            } else {
                let recent: Vec<f64> = samples
                    .iter()
                    .rev()
                    .take(4)
                    .map(|sample| sample.bytes_per_sec)
                    .collect();
                let slow = recent.len() >= 4
                    && recent
                        .iter()
                        .all(|bps| *bps > 0.0 && *bps < ADAPTIVE_SLOW_BPS);
                if slow {
                    shrink = true;
                    self.healthy_streak.store(0, Ordering::Release);
                } else {
                    let streak = self.healthy_streak.fetch_add(1, Ordering::AcqRel) + 1;
                    if streak >= 6 {
                        grow = true;
                        self.healthy_streak.store(0, Ordering::Release);
                    }
                }
            }
        }
        if shrink {
            let current = self.current.load(Ordering::Acquire);
            let next = (current / 2).max(1);
            self.current.store(next, Ordering::Release);
        } else if grow {
            let current = self.current.load(Ordering::Acquire);
            let next = (current + 1).min(self.max);
            self.current.store(next, Ordering::Release);
        }
    }
}

/// 在途额度守卫:释放时归还在途计数。
pub struct AdaptivePermit<'a> {
    controller: &'a AdaptiveConcurrency,
}

impl Drop for AdaptivePermit<'_> {
    fn drop(&mut self) {
        self.controller.release();
    }
}

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

/// 一次下载尝试记录：来源、URL 与结果，用于任务详情审计。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceAttempt {
    pub url: String,
    pub label: String,
    pub channel: SourceChannel,
    pub outcome: SourceAttemptOutcome,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SourceAttemptOutcome {
    Success,
    Failed { error: String },
}

/// 候选感知的下载报告：真实来源、尝试记录与分段状态。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FetchReport {
    pub result: DownloadResult,
    pub final_label: String,
    pub channel: SourceChannel,
    pub attempts: Vec<SourceAttempt>,
    pub segmented: bool,
    pub segment_count: u32,
    pub degraded_reason: Option<String>,
    pub reused_local: bool,
    pub effective_connections: usize,
}

/// 持久化在任务暂存区的分段清单，重启后据此复用有效分段。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SegmentManifest {
    url: String,
    total: u64,
    etag: Option<String>,
    last_modified: Option<String>,
    segments: Vec<SegmentState>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SegmentState {
    index: u32,
    start: u64,
    end: u64,
    completed: u64,
    done: bool,
}

/// 单文件进入分段下载的阈值与单文件分段数上限。
const SEGMENT_MIN_SIZE_BYTES: u64 = 8 * 1024 * 1024;
const SEGMENT_TARGET_BYTES: u64 = 16 * 1024 * 1024;
const SEGMENT_MAX_COUNT: u32 = 8;

fn segment_plan(total: u64, target_bytes: u64) -> Vec<(u64, u64)> {
    let count = total
        .div_ceil(target_bytes)
        .clamp(2, u64::from(SEGMENT_MAX_COUNT));
    let base = total.div_ceil(count);
    let mut plan = Vec::new();
    let mut start = 0_u64;
    while start < total {
        let end = (start + base).min(total);
        plan.push((start, end));
        start = end;
    }
    plan
}

fn segment_file(segments_dir: &Path, index: u32) -> PathBuf {
    segments_dir.join(format!("seg-{index:06}.part"))
}

fn manifest_path(segments_dir: &Path) -> PathBuf {
    segments_dir.join("manifest.json")
}

fn load_manifest(segments_dir: &Path) -> Option<SegmentManifest> {
    let payload = fs::read_to_string(manifest_path(segments_dir)).ok()?;
    serde_json::from_str(&payload).ok()
}

fn save_manifest(segments_dir: &Path, manifest: &SegmentManifest) -> Result<()> {
    fs::create_dir_all(segments_dir)?;
    let target = manifest_path(segments_dir);
    let temporary = segments_dir.join("manifest.json.tmp");
    fs::write(&temporary, serde_json::to_string(manifest)?)?;
    fs::rename(temporary, target)?;
    Ok(())
}

/// 校验本地清单与分段文件一致性；不一致的分段重置为未下载。
fn reconcile_manifest(segments_dir: &Path, manifest: &mut SegmentManifest) -> Result<()> {
    for segment in &mut manifest.segments {
        let path = segment_file(segments_dir, segment.index);
        let length = path.metadata().map(|meta| meta.len()).unwrap_or(0);
        let span = segment.end - segment.start;
        if segment.done && length != span {
            // 完成标记与文件长度不一致,按损坏处理并重新下载该分段。
            segment.done = false;
            segment.completed = 0;
            let _ = fs::remove_file(&path);
        } else if !segment.done {
            if length > span {
                let _ = fs::remove_file(&path);
                segment.completed = 0;
            } else {
                segment.completed = length;
            }
        }
    }
    Ok(())
}

#[derive(Debug)]
enum SegmentedFetch {
    Completed { segment_count: u32 },
    Degrade { reason: String },
}

#[derive(Debug, Clone)]
pub struct ArtifactDownloader {
    client: Client,
    permits: Arc<Semaphore>,
    segment_target_bytes: u64,
    rate_limiter: Arc<RateLimiter>,
    adaptive: Arc<AdaptiveConcurrency>,
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
            segment_target_bytes: SEGMENT_TARGET_BYTES,
            rate_limiter: global_rate_limiter(),
            adaptive: Arc::new(AdaptiveConcurrency::new(max_concurrent_downloads)),
        })
    }

    /// 覆盖单文件分段目标大小(下限 1 MiB)。供受控基准使用,生产保持默认值。
    #[must_use]
    pub fn with_segment_target_bytes(mut self, bytes: u64) -> Self {
        self.segment_target_bytes = bytes.max(1024 * 1024);
        self
    }

    /// 注入独立限速器,供限速测试隔离使用;生产共享全局限速器。
    #[doc(hidden)]
    #[must_use]
    pub fn with_rate_limiter(mut self, limiter: Arc<RateLimiter>) -> Self {
        self.rate_limiter = limiter;
        self
    }

    /// 当前压力感知有效连接数。
    #[must_use]
    pub fn effective_connections(&self) -> usize {
        self.adaptive.current_limit()
    }

    pub async fn fetch(
        &self,
        artifact: &ResolvedArtifact,
        staging_root: &Path,
        shared_root: &Path,
    ) -> Result<DownloadResult> {
        self.fetch_with_interrupt(artifact, staging_root, shared_root, None)
            .await
    }

    pub async fn fetch_with_interrupt(
        &self,
        artifact: &ResolvedArtifact,
        staging_root: &Path,
        shared_root: &Path,
        interrupt: Option<&DownloadInterrupt>,
    ) -> Result<DownloadResult> {
        let staged_file = safe_join(staging_root, &artifact.relative_path)?;
        let shared_file = safe_join(shared_root, &artifact.relative_path)?;
        if let Some(reused) = self
            .reuse_check(artifact, &staged_file, &shared_file)
            .await?
        {
            return Ok(reused);
        }
        self.download_single(artifact, &artifact.url, &staged_file, interrupt)
            .await
    }

    async fn reuse_check(
        &self,
        artifact: &ResolvedArtifact,
        staged_file: &Path,
        shared_file: &Path,
    ) -> Result<Option<DownloadResult>> {
        if file_matches(shared_file, artifact).await? {
            return Ok(Some(DownloadResult {
                staged_file: shared_file.to_path_buf(),
                disposition: DownloadDisposition::ReusedShared,
                bytes: artifact.size,
            }));
        }
        if file_matches(staged_file, artifact).await? {
            return Ok(Some(DownloadResult {
                staged_file: staged_file.to_path_buf(),
                disposition: DownloadDisposition::ReusedStaged,
                bytes: artifact.size,
            }));
        }
        Ok(None)
    }

    /// 按来源策略为工件生成候选并逐个尝试,记录每次尝试与真实来源。
    pub async fn fetch_with_policy(
        &self,
        artifact: &ResolvedArtifact,
        staging_root: &Path,
        shared_root: &Path,
        policy: &SourcePolicy,
        interrupt: Option<&DownloadInterrupt>,
    ) -> Result<FetchReport> {
        let candidates = match candidates_for(&artifact.url, policy) {
            SourceCandidates::Ready(candidates) => candidates,
            SourceCandidates::CurseForgeOfficialUnavailable { mirror } => {
                return Err(CoreError::Download(format!(
                    "CurseForge 官方源不可用,不会发起官方直连;请切换内置镜像优先,或使用 MCI Mirror 路径:{}",
                    mirror.url
                )));
            }
            SourceCandidates::CustomUnsupported { reason } => {
                return Err(CoreError::Download(reason));
            }
        };
        if candidates.is_empty() {
            return Err(CoreError::Download("没有可用的下载来源".to_owned()));
        }
        self.fetch_with_candidates(artifact, staging_root, shared_root, &candidates, interrupt)
            .await
    }

    /// 按给定顺序尝试候选来源。自定义源绝不回退,其余渠道按策略顺序回退并记录。
    pub async fn fetch_with_candidates(
        &self,
        artifact: &ResolvedArtifact,
        staging_root: &Path,
        shared_root: &Path,
        candidates: &[DownloadCandidate],
        interrupt: Option<&DownloadInterrupt>,
    ) -> Result<FetchReport> {
        let mut attempts: Vec<SourceAttempt> = Vec::new();
        let mut last_error: Option<CoreError> = None;
        for candidate in candidates {
            match self
                .fetch_from_candidate(artifact, staging_root, shared_root, candidate, interrupt)
                .await
            {
                Ok(mut report) => {
                    attempts.push(SourceAttempt {
                        url: candidate.url.clone(),
                        label: candidate.label.clone(),
                        channel: candidate.channel,
                        outcome: SourceAttemptOutcome::Success,
                    });
                    report.attempts = attempts;
                    return Ok(report);
                }
                Err(CoreError::TaskPaused) => return Err(CoreError::TaskPaused),
                Err(error) => {
                    // 自定义源绝不切换;内置镜像与官方源之间允许按策略回退并记录。
                    attempts.push(SourceAttempt {
                        url: candidate.url.clone(),
                        label: candidate.label.clone(),
                        channel: candidate.channel,
                        outcome: SourceAttemptOutcome::Failed {
                            error: error.to_string(),
                        },
                    });
                    last_error = Some(error);
                    if candidate.channel == SourceChannel::Custom {
                        break;
                    }
                }
            }
        }
        Err(last_error.unwrap_or_else(|| CoreError::Download("没有可用的下载来源".to_owned())))
    }

    async fn fetch_from_candidate(
        &self,
        artifact: &ResolvedArtifact,
        staging_root: &Path,
        shared_root: &Path,
        candidate: &DownloadCandidate,
        interrupt: Option<&DownloadInterrupt>,
    ) -> Result<FetchReport> {
        let staged_file = safe_join(staging_root, &artifact.relative_path)?;
        let shared_file = safe_join(shared_root, &artifact.relative_path)?;
        if let Some(reused) = self
            .reuse_check(artifact, &staged_file, &shared_file)
            .await?
        {
            return Ok(FetchReport {
                result: reused,
                final_label: "本地缓存复用".to_owned(),
                channel: candidate.channel,
                attempts: Vec::new(),
                segmented: false,
                segment_count: 0,
                degraded_reason: None,
                reused_local: true,
                effective_connections: self.effective_connections(),
            });
        }
        let has_trusted_hash =
            artifact.sha1.is_some() || artifact.sha256.is_some() || artifact.sha512.is_some();
        let eligible = artifact.size >= SEGMENT_MIN_SIZE_BYTES && has_trusted_hash;
        if eligible {
            match self
                .fetch_segmented(artifact, candidate, &staged_file, interrupt)
                .await?
            {
                SegmentedFetch::Completed { segment_count } => {
                    let partial_file = partial_path(&staged_file);
                    verify_file(&partial_file, artifact).await?;
                    replace_file(&partial_file, &staged_file).await?;
                    return Ok(FetchReport {
                        result: DownloadResult {
                            staged_file,
                            disposition: DownloadDisposition::Downloaded,
                            bytes: artifact.size,
                        },
                        final_label: candidate.label.clone(),
                        channel: candidate.channel,
                        attempts: Vec::new(),
                        segmented: true,
                        segment_count,
                        degraded_reason: None,
                        reused_local: false,
                        effective_connections: self.effective_connections(),
                    });
                }
                SegmentedFetch::Degrade { reason } => {
                    // 降级原因意味着分段不可用或不可信,清理后走单连接。
                    let _ = fs::remove_dir_all(staged_file.with_extension("segments"));
                    let result = self
                        .download_single(artifact, &candidate.url, &staged_file, interrupt)
                        .await?;
                    return Ok(FetchReport {
                        result,
                        final_label: candidate.label.clone(),
                        channel: candidate.channel,
                        attempts: Vec::new(),
                        segmented: false,
                        segment_count: 0,
                        degraded_reason: Some(reason),
                        reused_local: false,
                        effective_connections: self.effective_connections(),
                    });
                }
            }
        }
        let result = self
            .download_single(artifact, &candidate.url, &staged_file, interrupt)
            .await?;
        Ok(FetchReport {
            result,
            final_label: candidate.label.clone(),
            channel: candidate.channel,
            attempts: Vec::new(),
            segmented: false,
            segment_count: 0,
            degraded_reason: None,
            reused_local: false,
            effective_connections: self.effective_connections(),
        })
    }

    /// 大文件分段并行下载。Range 违规或对象证据变化时安全降级。
    async fn fetch_segmented(
        &self,
        artifact: &ResolvedArtifact,
        candidate: &DownloadCandidate,
        staged_file: &Path,
        interrupt: Option<&DownloadInterrupt>,
    ) -> Result<SegmentedFetch> {
        let segments_dir = staged_file.with_extension("segments");
        let total = artifact.size;
        // 既有清单:来源或总长度不一致时整体废弃,不保留任何旧分段。
        let mut manifest = match load_manifest(&segments_dir) {
            Some(mut existing) if existing.url == candidate.url && existing.total == total => {
                reconcile_manifest(&segments_dir, &mut existing)?;
                existing
            }
            Some(_) => {
                let _ = fs::remove_dir_all(&segments_dir);
                new_manifest(&candidate.url, total, self.segment_target_bytes)
            }
            None => new_manifest(&candidate.url, total, self.segment_target_bytes),
        };
        save_manifest(&segments_dir, &manifest)?;

        let pending: Vec<SegmentState> = manifest
            .segments
            .iter()
            .filter(|segment| !segment.done)
            .cloned()
            .collect();
        if pending.is_empty() {
            return merge_segments(&segments_dir, &manifest, &partial_path(staged_file)).await;
        }

        let outcomes = stream::iter(pending)
            .map(|segment| {
                let client = self.client.clone();
                let permits = Arc::clone(&self.permits);
                let rate_limiter = Arc::clone(&self.rate_limiter);
                let adaptive = Arc::clone(&self.adaptive);
                let segments_dir = segments_dir.clone();
                let expected_etag = manifest.etag.clone();
                let expected_last_modified = manifest.last_modified.clone();
                async move {
                    download_segment(
                        client,
                        permits,
                        rate_limiter,
                        adaptive,
                        &candidate.url,
                        &segment,
                        total,
                        &segments_dir,
                        expected_etag,
                        expected_last_modified,
                        interrupt,
                    )
                    .await
                }
            })
            .buffer_unordered(4)
            .collect::<Vec<_>>()
            .await;

        for outcome in outcomes {
            match outcome? {
                SegmentOutcome::Completed {
                    index,
                    etag,
                    last_modified,
                } => {
                    // 分段间对象证据必须一致,否则远端对象不可信,整体降级。
                    if let Some(etag) = &etag {
                        if let Some(expected) = &manifest.etag {
                            if expected != etag {
                                return Ok(SegmentedFetch::Degrade {
                                    reason: "分段间 ETag 不一致,远端对象已变化".to_owned(),
                                });
                            }
                        } else {
                            manifest.etag = Some(etag.clone());
                        }
                    }
                    if let Some(last_modified) = &last_modified {
                        if let Some(expected) = &manifest.last_modified {
                            if expected != last_modified {
                                return Ok(SegmentedFetch::Degrade {
                                    reason: "分段间 Last-Modified 不一致,远端对象已变化".to_owned(),
                                });
                            }
                        } else {
                            manifest.last_modified = Some(last_modified.clone());
                        }
                    }
                    let segment = manifest
                        .segments
                        .iter_mut()
                        .find(|entry| entry.index == index)
                        .ok_or_else(|| CoreError::Download("分段清单不一致".to_owned()))?;
                    segment.completed = segment.end - segment.start;
                    segment.done = true;
                    save_manifest(&segments_dir, &manifest)?;
                }
                SegmentOutcome::Degrade { reason } => {
                    return Ok(SegmentedFetch::Degrade { reason });
                }
            }
        }
        merge_segments(&segments_dir, &manifest, &partial_path(staged_file)).await
    }

    async fn download_single(
        &self,
        artifact: &ResolvedArtifact,
        url: &str,
        staged_file: &Path,
        interrupt: Option<&DownloadInterrupt>,
    ) -> Result<DownloadResult> {
        if interrupt.is_some_and(DownloadInterrupt::is_interrupted) {
            return Err(CoreError::TaskPaused);
        }
        let _permit = self
            .permits
            .acquire()
            .await
            .map_err(|_| CoreError::Download("下载协调器已经关闭".to_owned()))?;
        let _adaptive = self.adaptive.acquire(interrupt).await?;
        if let Some(parent) = staged_file.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let partial_file = partial_path(staged_file);
        let mut existing_length = file_length(&partial_file).await?;
        let mut forced_restart = false;
        if artifact.size > 0 && existing_length > artifact.size {
            truncate_file(&partial_file).await?;
            existing_length = 0;
            forced_restart = true;
        }
        if existing_length > 0 && file_matches(&partial_file, artifact).await? {
            replace_file(&partial_file, staged_file).await?;
            return Ok(DownloadResult {
                staged_file: staged_file.to_path_buf(),
                disposition: DownloadDisposition::ReusedStaged,
                bytes: artifact.size,
            });
        }

        let started = Instant::now();
        let download_start = existing_length;
        let result = async {
            let mut request = self.client.get(url);
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
                    .fetch_without_resume(artifact, url, &partial_file, staged_file, interrupt)
                    .await;
            } else {
                return Err(CoreError::Download(format!("{} 返回 HTTP {}", url, status)));
            };

            write_response(
                response,
                &partial_file,
                append,
                interrupt,
                &self.rate_limiter,
            )
            .await?;
            verify_file(&partial_file, artifact).await?;
            replace_file(&partial_file, staged_file).await?;
            Ok(DownloadResult {
                staged_file: staged_file.to_path_buf(),
                disposition,
                bytes: artifact.size,
            })
        }
        .await;
        match &result {
            Ok(_) => {
                let bytes = artifact.size.saturating_sub(download_start);
                let seconds = started.elapsed().as_secs_f64().max(0.001);
                self.adaptive.record(bytes as f64 / seconds, true);
            }
            Err(CoreError::TaskPaused) => {}
            Err(_) => {
                self.adaptive.record(0.0, false);
            }
        }
        result
    }

    async fn fetch_without_resume(
        &self,
        artifact: &ResolvedArtifact,
        url: &str,
        partial_file: &Path,
        staged_file: &Path,
        interrupt: Option<&DownloadInterrupt>,
    ) -> Result<DownloadResult> {
        if interrupt.is_some_and(DownloadInterrupt::is_interrupted) {
            return Err(CoreError::TaskPaused);
        }
        let response = self.client.get(url).send().await?;
        if response.status() != StatusCode::OK {
            return Err(CoreError::Download(format!(
                "{} 重新下载时返回 HTTP {}",
                url,
                response.status()
            )));
        }
        write_response(response, partial_file, false, interrupt, &self.rate_limiter).await?;
        verify_file(partial_file, artifact).await?;
        replace_file(partial_file, staged_file).await?;
        Ok(DownloadResult {
            staged_file: staged_file.to_path_buf(),
            disposition: DownloadDisposition::Restarted,
            bytes: artifact.size,
        })
    }
}

fn new_manifest(url: &str, total: u64, target_bytes: u64) -> SegmentManifest {
    SegmentManifest {
        url: url.to_owned(),
        total,
        etag: None,
        last_modified: None,
        segments: segment_plan(total, target_bytes)
            .into_iter()
            .enumerate()
            .map(|(index, (start, end))| SegmentState {
                index: index as u32,
                start,
                end,
                completed: 0,
                done: false,
            })
            .collect(),
    }
}

#[derive(Debug)]
enum SegmentOutcome {
    Completed {
        index: u32,
        etag: Option<String>,
        last_modified: Option<String>,
    },
    Degrade {
        reason: String,
    },
}

/// 下载单个分段。Range 违规返回 Degrade,硬错误返回 Err(交由候选回退处理)。
#[allow(clippy::too_many_arguments)]
async fn download_segment(
    client: Client,
    permits: Arc<Semaphore>,
    rate_limiter: Arc<RateLimiter>,
    adaptive: Arc<AdaptiveConcurrency>,
    url: &str,
    segment: &SegmentState,
    total: u64,
    segments_dir: &Path,
    expected_etag: Option<String>,
    expected_last_modified: Option<String>,
    interrupt: Option<&DownloadInterrupt>,
) -> Result<SegmentOutcome> {
    let span = segment.end - segment.start;
    let seg_path = segment_file(segments_dir, segment.index);
    let completed = tokio::fs::metadata(&seg_path)
        .await
        .map(|meta| meta.len())
        .unwrap_or(0);
    if completed >= span {
        return Ok(SegmentOutcome::Completed {
            index: segment.index,
            etag: None,
            last_modified: None,
        });
    }
    let _permit = permits
        .acquire()
        .await
        .map_err(|_| CoreError::Download("下载协调器已经关闭".to_owned()))?;
    let _adaptive = adaptive.acquire(interrupt).await?;
    let started = Instant::now();
    let download_start = completed;
    let range_start = segment.start + completed;
    let response = client
        .get(url)
        .header(
            header::RANGE,
            format!("bytes={range_start}-{}", segment.end - 1),
        )
        .send()
        .await?;
    let status = response.status();
    if status == StatusCode::OK {
        return Ok(SegmentOutcome::Degrade {
            reason: "来源忽略 Range 分段请求,已降级为单连接续传".to_owned(),
        });
    }
    if status == StatusCode::RANGE_NOT_SATISFIABLE {
        return Ok(SegmentOutcome::Degrade {
            reason: "分段范围被来源拒绝,已降级为单连接续传".to_owned(),
        });
    }
    if status != StatusCode::PARTIAL_CONTENT {
        adaptive.record(0.0, false);
        return Err(CoreError::Download(format!(
            "{url} 分段请求返回 HTTP {status}"
        )));
    }
    let (range_begin, range_total) = parse_content_range(response.headers())?;
    if range_begin != range_start || range_total != total {
        return Ok(SegmentOutcome::Degrade {
            reason: "分段响应范围与计划不一致,已降级为单连接续传".to_owned(),
        });
    }
    let etag = response
        .headers()
        .get(header::ETAG)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let last_modified = response
        .headers()
        .get(header::LAST_MODIFIED)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    if let (Some(expected), Some(actual)) = (&expected_etag, &etag)
        && expected != actual
    {
        return Ok(SegmentOutcome::Degrade {
            reason: "远端对象 ETag 已变化,废弃旧分段".to_owned(),
        });
    }
    if let (Some(expected), Some(actual)) = (&expected_last_modified, &last_modified)
        && expected != actual
    {
        return Ok(SegmentOutcome::Degrade {
            reason: "远端对象 Last-Modified 已变化,废弃旧分段".to_owned(),
        });
    }
    write_response(response, &seg_path, true, interrupt, &rate_limiter).await?;
    let bytes = (segment.end - segment.start).saturating_sub(download_start);
    let seconds = started.elapsed().as_secs_f64().max(0.001);
    adaptive.record(bytes as f64 / seconds, true);
    Ok(SegmentOutcome::Completed {
        index: segment.index,
        etag,
        last_modified,
    })
}

/// 合并全部分段到最终暂存文件并清理分段目录。合并后由调用方执行完整校验。
async fn merge_segments(
    segments_dir: &Path,
    manifest: &SegmentManifest,
    partial_file: &Path,
) -> Result<SegmentedFetch> {
    if manifest.segments.iter().any(|segment| !segment.done) {
        return Err(CoreError::Download("存在未完成分段,不能合并".to_owned()));
    }
    if let Some(parent) = partial_file.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let mut output = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(partial_file)
        .await?;
    let mut buffer = vec![0_u8; 256 * 1024];
    for segment in &manifest.segments {
        let mut input = File::open(segment_file(segments_dir, segment.index)).await?;
        loop {
            let read = input.read(&mut buffer).await?;
            if read == 0 {
                break;
            }
            output.write_all(&buffer[..read]).await?;
        }
    }
    output.flush().await?;
    output.sync_data().await?;
    drop(output);
    let merged_length = tokio::fs::metadata(partial_file).await?.len();
    if merged_length != manifest.total {
        return Err(CoreError::Download(format!(
            "合并长度 {merged_length} 与计划总长度 {} 不一致",
            manifest.total
        )));
    }
    let _ = tokio::fs::remove_dir_all(segments_dir).await;
    Ok(SegmentedFetch::Completed {
        segment_count: u32::try_from(manifest.segments.len()).unwrap_or(SEGMENT_MAX_COUNT),
    })
}

fn parse_content_range(headers: &header::HeaderMap) -> Result<(u64, u64)> {
    let value = headers
        .get(header::CONTENT_RANGE)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| CoreError::Download("分段响应缺少 Content-Range".to_owned()))?;
    let range = value
        .strip_prefix("bytes ")
        .ok_or_else(|| CoreError::Download(format!("Content-Range 格式无效:{value}")))?;
    let (span, total) = range
        .split_once('/')
        .ok_or_else(|| CoreError::Download(format!("Content-Range 格式无效:{value}")))?;
    let (start, _end) = span
        .split_once('-')
        .ok_or_else(|| CoreError::Download(format!("Content-Range 格式无效:{value}")))?;
    let start: u64 = start
        .parse()
        .map_err(|_| CoreError::Download(format!("Content-Range 起点无效:{value}")))?;
    let total: u64 = total
        .parse()
        .map_err(|_| CoreError::Download(format!("Content-Range 总长无效:{value}")))?;
    Ok((start, total))
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
        self.execute_task_with_interrupt(service, task_id, None)
            .await
    }

    pub async fn execute_task_with_interrupt(
        &self,
        service: &AppService,
        task_id: &str,
        interrupt: Option<DownloadInterrupt>,
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
        let result = self.execute_task_inner(service, &task, interrupt).await;
        if let Err(error) = &result {
            if matches!(error, CoreError::TaskPaused) {
                let _ = service.mark_content_task_paused(task_id, "global");
            } else {
                let _ = service.mark_content_task_failed(task_id, &error.to_string());
            }
        }
        result
    }

    async fn execute_task_inner(
        &self,
        service: &AppService,
        task: &ContentInstallTask,
        interrupt: Option<DownloadInterrupt>,
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
        let policy = service.download_source_policy()?;
        let downloads = self
            .download_batch(
                artifacts.clone(),
                ContentDownloadContext {
                    service,
                    task_id: &task.id,
                    staging_directory: &download_directory,
                    shared_directory: &shared_directory,
                    total,
                    interrupt: interrupt.as_ref(),
                    policy: &policy,
                },
            )
            .await?;
        if let Some(detail) = crate::TaskSourceDetail::summarize(
            &downloads
                .iter()
                .map(|(_, report)| report)
                .collect::<Vec<_>>(),
        ) {
            service.set_content_task_source_detail(&task.id, &detail)?;
        }
        service.set_content_task_phase(
            &task.id,
            TaskState::Running,
            ContentInstallStage::VerifyFiles,
            "所有模组文件已通过大小、SHA-1 和 SHA-512 校验",
        )?;

        for (artifact, report) in &downloads {
            commit_shared_file(artifact, &report.result, &shared_directory, &task.id).await?;
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
        let mut backups: Vec<(PathBuf, PathBuf)> = Vec::new();
        for (entry, artifact) in task.plan.entries.iter().zip(&artifacts) {
            let prepared = safe_join(&publish_directory, &entry.file.filename)?;
            let destination = safe_join(&mods_directory, &entry.file.filename)?;
            if tokio::fs::try_exists(&destination).await? {
                if file_matches(&destination, artifact).await? {
                    tokio::fs::remove_file(&prepared).await?;
                    continue;
                }
                if !task.plan.is_update {
                    let error = CoreError::Content(format!(
                        "模组文件冲突：{} 已存在且哈希不同",
                        entry.file.filename
                    ));
                    return rollback_content_error(error, &published, &backups).await;
                }
                // 更新：先把旧文件移入实例快照区，再发布新文件。
                let snapshot_directory =
                    content_update_snapshot_directory(Path::new(&task.target_directory), &task.id);
                if let Err(error) = tokio::fs::create_dir_all(&snapshot_directory).await {
                    return rollback_content_error(error.into(), &published, &backups).await;
                }
                let backup = safe_join(&snapshot_directory, &entry.file.filename)?;
                if let Err(error) = tokio::fs::rename(&destination, &backup).await {
                    return rollback_content_error(error.into(), &published, &backups).await;
                }
                backups.push((destination.clone(), backup));
            }
            if let Err(error) = tokio::fs::rename(&prepared, &destination).await {
                return rollback_content_error(error.into(), &published, &backups).await;
            }
            published.push(destination);
        }

        let installed = match service.publish_installed_content(task) {
            Ok(installed) => installed,
            Err(error) => return rollback_content_error(error, &published, &backups).await,
        };
        if task.plan.is_update {
            let snapshot_directory =
                content_update_snapshot_directory(Path::new(&task.target_directory), &task.id);
            if let Err(error) = tokio::fs::remove_dir_all(&snapshot_directory).await
                && error.kind() != std::io::ErrorKind::NotFound
            {
                let _ = service.note_content_cleanup_pending(&task.id, &error.to_string());
            }
        }
        if let Err(error) = tokio::fs::remove_dir_all(&staging_directory).await {
            let _ = service.note_content_cleanup_pending(&task.id, &error.to_string());
        }
        Ok(installed)
    }

    async fn download_batch(
        &self,
        artifacts: Vec<ResolvedArtifact>,
        context: ContentDownloadContext<'_>,
    ) -> Result<Vec<(ResolvedArtifact, FetchReport)>> {
        let completed = Arc::new(AtomicU64::new(0));
        let downloader = self.downloader.clone();
        stream::iter(artifacts)
            .map(|artifact| {
                let downloader = downloader.clone();
                let completed = Arc::clone(&completed);
                let service = context.service.clone();
                let task_id = context.task_id.to_owned();
                let staging_directory = context.staging_directory.to_path_buf();
                let shared_directory = context.shared_directory.to_path_buf();
                let total = context.total;
                let policy = context.policy.clone();
                async move {
                    let report = downloader
                        .fetch_with_policy(
                            &artifact,
                            &staging_directory,
                            &shared_directory,
                            &policy,
                            context.interrupt,
                        )
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
                    Ok::<_, CoreError>((artifact, report))
                }
            })
            .buffer_unordered(self.max_concurrent_downloads)
            .try_collect()
            .await
    }
}

struct ContentDownloadContext<'a> {
    service: &'a AppService,
    task_id: &'a str,
    staging_directory: &'a Path,
    shared_directory: &'a Path,
    total: u64,
    interrupt: Option<&'a DownloadInterrupt>,
    policy: &'a SourcePolicy,
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
    if task.plan.is_update {
        // 更新任务允许同名异哈希替换，冲突检查由提交阶段的快照替换处理。
        return Ok(mods_directory);
    }
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

async fn rollback_content_error<T>(
    error: CoreError,
    published: &[PathBuf],
    backups: &[(PathBuf, PathBuf)],
) -> Result<T> {
    let mut rollback_errors = Vec::new();
    for path in published.iter().rev() {
        if let Err(rollback_error) = tokio::fs::remove_file(path).await {
            rollback_errors.push(format!("{}：{rollback_error}", path.display()));
        }
    }
    for (destination, backup) in backups.iter().rev() {
        if let Err(rollback_error) = tokio::fs::rename(backup, destination).await {
            rollback_errors.push(format!(
                "恢复 {} 失败：{rollback_error}",
                destination.display()
            ));
            continue;
        }
        if let Some(parent) = backup.parent() {
            // 旧文件移回后清理空快照目录，不留半成品。
            let _ = tokio::fs::remove_dir(parent).await;
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

#[derive(Clone)]
pub struct InstallExecutor {
    downloader: ArtifactDownloader,
    max_concurrent_downloads: usize,
    asset_base_url: String,
    processor_runner: Option<Arc<crate::ProcessorRunner>>,
}

impl std::fmt::Debug for InstallExecutor {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("InstallExecutor")
            .field("max_concurrent_downloads", &self.max_concurrent_downloads)
            .finish_non_exhaustive()
    }
}

impl InstallExecutor {
    pub fn new(max_concurrent_downloads: usize) -> Result<Self> {
        Ok(Self {
            downloader: ArtifactDownloader::new(max_concurrent_downloads)?,
            max_concurrent_downloads,
            asset_base_url: "https://resources.download.minecraft.net".to_owned(),
            processor_runner: None,
        })
    }

    /// 注入确定性处理器运行器,供 BDD 测试使用;生产保持托管 Java 子进程。
    #[doc(hidden)]
    #[must_use]
    pub fn with_processor_runner(mut self, runner: Arc<crate::ProcessorRunner>) -> Self {
        self.processor_runner = Some(runner);
        self
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
        self.execute_task_with_interrupt(service, task_id, None)
            .await
    }

    pub async fn execute_task_with_interrupt(
        &self,
        service: &AppService,
        task_id: &str,
        interrupt: Option<DownloadInterrupt>,
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
        let result = self.execute_task_inner(service, &task, interrupt).await;
        if let Err(error) = &result {
            if matches!(error, CoreError::TaskPaused) {
                let _ = service.mark_install_task_paused(task_id, "global");
            } else {
                if let JavaPlanAction::Install { environment_id, .. } = &task.plan.java_action {
                    let is_ready = service
                        .list_managed_java()
                        .ok()
                        .and_then(|environments| {
                            environments
                                .into_iter()
                                .find(|environment| environment.id == *environment_id)
                        })
                        .is_some_and(|environment| {
                            environment.status == JavaEnvironmentStatus::Ready
                        });
                    if !is_ready {
                        let _ = service.mark_java_environment_status(
                            environment_id,
                            JavaEnvironmentStatus::Failed,
                        );
                    }
                }
                let _ = service.mark_task_failed(task_id, &error.to_string());
            }
        }
        result
    }

    async fn execute_task_inner(
        &self,
        service: &AppService,
        task: &InstallTask,
        interrupt: Option<DownloadInterrupt>,
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
        let policy = service.download_source_policy()?;
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
                    policy: policy.clone(),
                },
                interrupt.as_ref(),
            )
            .await?;
        let completed_base = downloads.iter().fold(0_u64, |total, (artifact, _)| {
            total.saturating_add(artifact.size)
        });

        let asset_index_path = downloads
            .iter()
            .find(|(artifact, _)| artifact.kind == ArtifactKind::AssetIndex)
            .map(|(_, report)| report.result.staged_file.clone())
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
                    policy,
                },
                interrupt.as_ref(),
            )
            .await?;
        downloads.extend(asset_downloads);
        if let Some(detail) = crate::TaskSourceDetail::summarize(
            &downloads
                .iter()
                .map(|(_, report)| report)
                .collect::<Vec<_>>(),
        ) {
            service.set_task_source_detail(&task.id, &detail)?;
        }

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
        if let Some((patched_artifact, patched_report)) = self
            .run_loader_processors(service, task, &java_home, &staging_directory)
            .await?
        {
            downloads.push((patched_artifact, patched_report));
        }
        let runtime = build_runtime_manifest(task, &java_home, &downloads);
        let staged_instance = prepare_instance_directory(task, &runtime, &staging_directory)?;
        extract_native_libraries(&downloads, &staged_instance, &staging_directory)?;
        for (artifact, report) in &downloads {
            if artifact.kind != ArtifactKind::JavaArchive {
                commit_shared_file(artifact, &report.result, &shared_directory, &task.id).await?;
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
        interrupt: Option<&DownloadInterrupt>,
    ) -> Result<Vec<(ResolvedArtifact, FetchReport)>> {
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
                let policy = context.policy.clone();
                async move {
                    let report = downloader
                        .fetch_with_policy(
                            &artifact,
                            &staging_root,
                            &shared_root,
                            &policy,
                            interrupt,
                        )
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
                    Ok::<_, CoreError>((artifact, report))
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
        let profile_json = match loader {
            ResolvedLoader::Vanilla => return Ok(Vec::new()),
            ResolvedLoader::Forge {
                install_profile,
                version_json,
                installer_url,
                installer_sha1,
                installer_size,
                ..
            }
            | ResolvedLoader::NeoForge {
                install_profile,
                version_json,
                installer_url,
                installer_sha1,
                installer_size,
                ..
            } => {
                let mut artifacts = version_json_library_artifacts(version_json)?;
                let profile: crate::InstallProfile =
                    serde_json::from_value(install_profile.clone())?;
                artifacts.extend(profile_library_artifacts(&profile)?);
                let file_name = installer_url
                    .rsplit('/')
                    .next()
                    .filter(|name| !name.is_empty())
                    .ok_or_else(|| {
                        CoreError::InvalidInstallRequest("安装器 URL 缺少文件名".to_owned())
                    })?;
                artifacts.push(ResolvedArtifact {
                    kind: ArtifactKind::LoaderLibrary,
                    relative_path: format!("minecraft/loader-installers/{file_name}"),
                    url: installer_url.clone(),
                    size: *installer_size,
                    sha1: Some(installer_sha1.clone()),
                    sha256: None,
                    sha512: None,
                });
                return Ok(artifacts);
            }
            ResolvedLoader::Fabric { profile, .. } | ResolvedLoader::Quilt { profile, .. } => {
                profile.clone()
            }
        };
        let profile: FabricProfile = serde_json::from_value(profile_json)?;
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

    /// Forge/NeoForge:按 install_profile 执行客户端处理器并产出校验过的客户端 JAR。
    /// 其他加载器返回 None。
    async fn run_loader_processors(
        &self,
        service: &AppService,
        task: &InstallTask,
        java_home: &str,
        staging_directory: &Path,
    ) -> Result<Option<(ResolvedArtifact, FetchReport)>> {
        let install_profile_json = match &task.plan.loader {
            ResolvedLoader::Forge {
                install_profile, ..
            }
            | ResolvedLoader::NeoForge {
                install_profile, ..
            } => install_profile.clone(),
            ResolvedLoader::Vanilla
            | ResolvedLoader::Fabric { .. }
            | ResolvedLoader::Quilt { .. } => {
                return Ok(None);
            }
        };
        service.set_task_phase(
            &task.id,
            TaskState::Running,
            InstallStage::ApplyLoader,
            "正在执行加载器安装器处理器",
        )?;
        let profile: crate::InstallProfile = serde_json::from_value(install_profile_json)?;
        let download_staging = staging_directory.join("downloads");
        let library_dir = download_staging.join("minecraft/libraries");
        let work_dir = staging_directory.join("loader-work");
        let installer_url = match &task.plan.loader {
            ResolvedLoader::Forge { installer_url, .. }
            | ResolvedLoader::NeoForge { installer_url, .. } => installer_url.clone(),
            _ => unreachable!(),
        };
        let file_name = installer_url
            .rsplit('/')
            .next()
            .ok_or_else(|| CoreError::InvalidInstallRequest("安装器 URL 缺少文件名".to_owned()))?;
        let installer_path = download_staging
            .join("minecraft/loader-installers")
            .join(file_name);
        if !installer_path.is_file() {
            return Err(CoreError::InvalidInstallRequest(
                "安装器未完成下载，无法执行处理器".to_owned(),
            ));
        }
        let shared_store = PathBuf::from(&task.plan.shared_store_directory);
        let minecraft_jar_relative =
            format!("minecraft/versions/{0}/{0}.jar", task.plan.game.version.id);
        let minecraft_jar = {
            let shared_candidate = shared_store.join(&minecraft_jar_relative);
            if shared_candidate.is_file() {
                shared_candidate
            } else {
                download_staging.join(&minecraft_jar_relative)
            }
        };
        let plan = crate::plan_loader_processors(
            &profile,
            &installer_path,
            &library_dir,
            &minecraft_jar,
            &task.plan.game.version.id,
            &work_dir,
        )?;
        let runner: Box<crate::ProcessorRunner> = match &self.processor_runner {
            Some(runner) => {
                let runner = Arc::clone(runner);
                Box::new(move |invocation, work| runner(invocation, work))
            }
            None => crate::java_processor_runner(Path::new(java_home).join("bin/java.exe")),
        };
        crate::run_loader_processors(&plan, runner.as_ref(), &work_dir)?;
        let patched_metadata = std::fs::metadata(&plan.patched_output)?;
        let patched_artifact = ResolvedArtifact {
            kind: ArtifactKind::LoaderLibrary,
            relative_path: format!(
                "minecraft/libraries/{}",
                plan.patched_coordinate.relative_path()
            ),
            url: String::new(),
            size: patched_metadata.len(),
            sha1: plan.patched_sha1.clone(),
            sha256: None,
            sha512: None,
        };
        let patched_report = FetchReport {
            result: DownloadResult {
                staged_file: plan.patched_output.clone(),
                disposition: DownloadDisposition::Downloaded,
                bytes: patched_metadata.len(),
            },
            final_label: "本地处理器产出".to_owned(),
            channel: crate::SourceChannel::Official,
            attempts: Vec::new(),
            segmented: false,
            segment_count: 0,
            degraded_reason: None,
            reused_local: true,
            effective_connections: 1,
        };
        Ok(Some((patched_artifact, patched_report)))
    }

    async fn prepare_java(
        &self,
        service: &AppService,
        task: &InstallTask,
        downloads: &[(ResolvedArtifact, FetchReport)],
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
                    .map(|(_, report)| report.result.staged_file.clone())
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
    policy: SourcePolicy,
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
    downloads: &[(ResolvedArtifact, FetchReport)],
    staged_instance: &Path,
    staging_directory: &Path,
) -> Result<()> {
    let natives_directory = staged_instance.join("natives");
    let mut extracted_dlls = 0_usize;
    let mut native_archives = 0_usize;
    for (index, (_, report)) in downloads
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
            &report.result.staged_file,
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
    downloads: &[(ResolvedArtifact, FetchReport)],
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
        ResolvedLoader::Fabric { profile, .. } | ResolvedLoader::Quilt { profile, .. } => (
            profile
                .get("mainClass")
                .and_then(serde_json::Value::as_str)
                .unwrap_or(&task.plan.game.main_class)
                .to_owned(),
            Some(profile.clone()),
        ),
        ResolvedLoader::Forge { version_json, .. }
        | ResolvedLoader::NeoForge { version_json, .. } => (
            version_json
                .get("mainClass")
                .and_then(serde_json::Value::as_str)
                .unwrap_or(&task.plan.game.main_class)
                .to_owned(),
            Some(version_json.clone()),
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
    Ok(crate::MavenCoordinate::parse(coordinate)?.relative_path())
}

/// 解析 version.json 中的游戏库下载工件。URL 为空的条目由处理器本地产出,跳过。
fn version_json_library_artifacts(
    version_json: &serde_json::Value,
) -> Result<Vec<ResolvedArtifact>> {
    let libraries = version_json
        .get("libraries")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| {
            CoreError::InvalidInstallRequest("version.json 缺少 libraries".to_owned())
        })?;
    let mut artifacts = Vec::with_capacity(libraries.len());
    for library in libraries {
        let name = library
            .get("name")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| {
                CoreError::InvalidInstallRequest("version.json 库缺少 name".to_owned())
            })?;
        let Some(download) = library.pointer("/downloads/artifact") else {
            continue;
        };
        let url = download
            .get("url")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        if url.is_empty() {
            continue;
        }
        let path = download
            .get("path")
            .and_then(serde_json::Value::as_str)
            .filter(|path| !path.is_empty())
            .map(str::to_owned)
            .map_or_else(|| maven_path(name), Ok)?;
        let sha1 = download
            .get("sha1")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned);
        if let Some(value) = &sha1 {
            validate_hash(value, 40, "加载器库 SHA-1")?;
        }
        let size = download
            .get("size")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        if size == 0 {
            return Err(CoreError::Metadata(format!(
                "加载器库 {name} 没有提供可验证大小"
            )));
        }
        artifacts.push(ResolvedArtifact {
            kind: ArtifactKind::LoaderLibrary,
            relative_path: format!("minecraft/libraries/{path}"),
            url: url.to_owned(),
            size,
            sha1,
            sha256: None,
            sha512: None,
        });
    }
    Ok(artifacts)
}

/// 解析 install_profile 中的处理器库下载工件。
fn profile_library_artifacts(profile: &crate::InstallProfile) -> Result<Vec<ResolvedArtifact>> {
    let mut artifacts = Vec::with_capacity(profile.libraries.len());
    for library in &profile.libraries {
        let relative = maven_path(&library.name)?;
        let Some(artifact) = library
            .downloads
            .as_ref()
            .and_then(|downloads| downloads.artifact.as_ref())
        else {
            continue;
        };
        if artifact.url.is_empty() {
            return Err(CoreError::InvalidInstallRequest(format!(
                "处理器库 {} 缺少下载 URL",
                library.name
            )));
        }
        if artifact.sha1.is_empty() {
            return Err(CoreError::InvalidInstallRequest(format!(
                "处理器库 {} 缺少 SHA-1",
                library.name
            )));
        }
        validate_hash(&artifact.sha1, 40, "处理器库 SHA-1")?;
        if artifact.size == 0 {
            return Err(CoreError::Metadata(format!(
                "处理器库 {} 没有提供可验证大小",
                library.name
            )));
        }
        artifacts.push(ResolvedArtifact {
            kind: ArtifactKind::LoaderLibrary,
            relative_path: format!("minecraft/libraries/{relative}"),
            url: artifact.url.clone(),
            size: artifact.size,
            sha1: Some(artifact.sha1.clone()),
            sha256: None,
            sha512: None,
        });
    }
    Ok(artifacts)
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
    interrupt: Option<&DownloadInterrupt>,
    rate_limiter: &RateLimiter,
) -> Result<()> {
    let mut output = OpenOptions::new()
        .create(true)
        .write(true)
        .append(append)
        .truncate(!append)
        .open(partial_file)
        .await?;
    let mut stream = response.bytes_stream();
    loop {
        let chunk = match interrupt {
            Some(signal) => {
                tokio::select! {
                    chunk = stream.next() => chunk,
                    () = signal.wait() => {
                        // 暂停中断:停止写入,已写入的 .partial 保留供恢复校验。
                        output.flush().await?;
                        output.sync_data().await?;
                        return Err(CoreError::TaskPaused);
                    }
                }
            }
            None => stream.next().await,
        };
        let Some(chunk) = chunk else { break };
        let chunk = chunk?;
        let chunk_len = u64::try_from(chunk.len()).unwrap_or(u64::MAX);
        output.write_all(&chunk).await?;
        // 全局限速:所有连接共用同一令牌桶,分段下载也不会系统性突破上限。
        rate_limiter.acquire(chunk_len, interrupt).await?;
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
