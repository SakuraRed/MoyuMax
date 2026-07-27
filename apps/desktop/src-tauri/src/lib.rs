use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use moyumax_core::{
    AccountSummary, AppService, ArtifactDownloader, BootstrapState, ContentExecutor,
    ContentInstallPlan, ContentInstallTask, ContentUpdateInfo, CrashReportSummary,
    DiagnosticExportPreview, DiagnosticExportResult, DownloadInterrupt, ExitImpactSummary,
    ExportModpackOptions, ExportModpackReport, FabricLoaderSummary, GlobalLaunchPreference,
    InstallExecutor, InstallSelection, InstallTask, InstalledContent, InstalledModpack,
    InstanceIsolation, InstanceResource, InstanceResourceKind, InstanceScreenshot,
    InstanceServerEntry, InstanceWorldInfo, JavaArchitecture, JavaDeleteOutcome, JavaDistribution,
    JavaEnvironmentSummary, LaunchExecution, LaunchLogRead, LaunchOptions, LaunchSessionSummary,
    LoaderChoice, ManagedInstanceSummary, MciMirrorClient, MetadataClient, MicrosoftAuthClient,
    MicrosoftLoginCancel, MinecraftServerStatus, ModpackInstallReport, ModpackUpdateReport,
    ModrinthClient, ModrinthSearchPage, ModrinthSearchQuery, ModrinthVersionSummary,
    OnboardingSelection, ProxyPreference, RecoveryDecision, RecycleBinItem, RecyclePurgeResult,
    ReleaseInfo, ResolvedInstallRequest, ResolvedLoader, ShellState, SourcePolicy, ThemePack,
    UiBackground, UpdateClient, VersionCatalog, WindowCloseBehavior, WorldBackupSummary,
    YggdrasilClient, auto_launch_options, min_version_block, run_launch_execution,
    total_physical_memory_mib,
};
use serde::Serialize;
use tauri::{Emitter, Manager, State};
use tokio::sync::{Semaphore, oneshot};
use uuid::Uuid;

mod cli;
mod lifecycle;
mod tray;

use lifecycle::{
    CLOSE_REQUESTED_EVENT, PendingIntent, ShellCoordinator, WindowStartupKind,
    confirm_graceful_exit, minimize_to_tray, spawn_idle_destroy_task, spawn_smoke_driver,
};

pub use cli::run_cli;
pub use cli::{EXIT_DISABLED, EXIT_OK, EXIT_RUNTIME, EXIT_USAGE, execute as execute_cli_command};

#[derive(Debug, Default)]
struct InstallPreviewStore {
    requests: Mutex<HashMap<String, ResolvedInstallRequest>>,
}

#[derive(Debug, Default)]
struct ContentPreviewStore {
    plans: Mutex<HashMap<String, ContentInstallPlan>>,
}

#[derive(Debug, Default)]
struct ModpackPreviewStore {
    plans: Mutex<HashMap<String, (moyumax_core::ModpackPlan, PathBuf)>>,
}

#[derive(Debug, Default)]
struct DiagnosticPreviewStore {
    reports: Mutex<HashMap<String, String>>,
}

/// Microsoft 设备码登录的进行中状态；同一时间只允许一轮。
/// 内部句柄共享，Clone 后可安全 move 进后台轮询任务。
#[derive(Debug, Clone, Default)]
struct MicrosoftLoginState {
    running: Arc<Mutex<bool>>,
    cancel: MicrosoftLoginCancel,
}

/// 联机协调器：EasyTier 子进程的生命周期。
#[derive(Debug, Default)]
struct NetplayCoordinator {
    room: Mutex<Option<NetplayRoomProcess>>,
}

/// 整合包安装协调器:跟踪哪些实例的整合包文件正在安装中,
/// 防止用户在文件未落齐时启动游戏(表现为"只有两个 mod")。
#[derive(Debug, Default)]
struct ModpackInstallCoordinator {
    installing: Mutex<std::collections::HashSet<String>>,
}

impl ModpackInstallCoordinator {
    fn begin(&self, instance_id: &str) {
        if let Ok(mut guard) = self.installing.lock() {
            guard.insert(instance_id.to_owned());
        }
    }

    fn finish(&self, instance_id: &str) {
        if let Ok(mut guard) = self.installing.lock() {
            guard.remove(instance_id);
        }
    }

    fn is_installing(&self, instance_id: &str) -> bool {
        self.installing
            .lock()
            .map(|guard| guard.contains(instance_id))
            .unwrap_or(false)
    }
}

#[derive(Debug)]
struct NetplayRoomProcess {
    config: moyumax_core::NetplayRoomConfig,
    virtual_ip: String,
    child: std::process::Child,
    /// EasyTier RPC 门户端口（本机回环），用于 node info / port-forward 管理。
    rpc_port: u16,
    /// easytier-cli.exe 绝对路径（与 core 同目录）。
    cli_path: PathBuf,
    /// 主机侧侦测到的 MC「对局域网开放」端口。
    mc_lan_port: Option<u16>,
    /// 客机侧已建立的本机回环转发端口（指向主机虚拟 IP:MC 端口）。
    forwarded_local_port: Option<u16>,
    /// MC 局域网组播监听停止标记（仅主机）。
    lan_stop: Option<Arc<AtomicBool>>,
}

/// 联机房间的非敏感视图（不携带密码）。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct NetplayRoomView {
    network_name: String,
    virtual_ip: String,
    is_host: bool,
    mc_lan_port: Option<u16>,
    forwarded_local_port: Option<u16>,
}

/// NAT 检测报告视图。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct NatReportView {
    mapped_address: String,
    behind_nat: bool,
    impact: String,
}

/// `netplay-download-progress` 事件负载（EasyTier 首次下载进度）。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct NetplayDownloadProgress {
    current: u64,
    total: u64,
}

#[derive(Debug, Clone)]
struct TaskCoordinator {
    install_executor: InstallExecutor,
    content_executor: ContentExecutor,
    task_permits: Arc<Semaphore>,
    scheduling_enabled: Arc<AtomicBool>,
    current_interrupt: Arc<Mutex<Option<DownloadInterrupt>>>,
    current_task: Arc<Mutex<Option<(String, TaskKind)>>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
enum TaskKind {
    Install,
    Content,
}

impl TaskCoordinator {
    fn new(scheduling_enabled: bool, download_concurrency: usize) -> Result<Self, String> {
        Ok(Self {
            install_executor: InstallExecutor::new(download_concurrency)
                .map_err(|error| error.to_string())?,
            content_executor: ContentExecutor::new(download_concurrency)
                .map_err(|error| error.to_string())?,
            task_permits: Arc::new(Semaphore::new(1)),
            scheduling_enabled: Arc::new(AtomicBool::new(scheduling_enabled)),
            current_interrupt: Arc::new(Mutex::new(None)),
            current_task: Arc::new(Mutex::new(None)),
        })
    }

    fn scheduling_enabled(&self) -> bool {
        self.scheduling_enabled.load(Ordering::Acquire)
    }

    fn submit_install(&self, service: AppService, task_id: String) {
        if !self.scheduling_enabled() {
            return;
        }
        let coordinator = self.clone();
        tauri::async_runtime::spawn(async move {
            let Ok(_permit) = coordinator.task_permits.acquire().await else {
                return;
            };
            if !coordinator.scheduling_enabled() {
                return;
            }
            // 提交到执行之间用户可能暂停了该任务,只有排队状态才执行。
            let executable = service
                .install_task(&task_id)
                .map(|task| task.state == moyumax_core::TaskState::Queued)
                .unwrap_or(false);
            if !executable {
                return;
            }
            let interrupt = DownloadInterrupt::new();
            if let Ok(mut current) = coordinator.current_interrupt.lock() {
                *current = Some(interrupt.clone());
            }
            if let Ok(mut current) = coordinator.current_task.lock() {
                *current = Some((task_id.clone(), TaskKind::Install));
            }
            let _ = coordinator
                .install_executor
                .execute_task_with_interrupt(&service, &task_id, Some(interrupt))
                .await;
            if let Ok(mut current) = coordinator.current_interrupt.lock() {
                *current = None;
            }
            if let Ok(mut current) = coordinator.current_task.lock() {
                *current = None;
            }
        });
    }

    fn submit_content(&self, service: AppService, task_id: String) {
        if !self.scheduling_enabled() {
            return;
        }
        let coordinator = self.clone();
        tauri::async_runtime::spawn(async move {
            let Ok(_permit) = coordinator.task_permits.acquire().await else {
                return;
            };
            if !coordinator.scheduling_enabled() {
                return;
            }
            let executable = service
                .content_install_task(&task_id)
                .map(|task| task.state == moyumax_core::TaskState::Queued)
                .unwrap_or(false);
            if !executable {
                return;
            }
            let interrupt = DownloadInterrupt::new();
            if let Ok(mut current) = coordinator.current_interrupt.lock() {
                *current = Some(interrupt.clone());
            }
            if let Ok(mut current) = coordinator.current_task.lock() {
                *current = Some((task_id.clone(), TaskKind::Content));
            }
            let _ = coordinator
                .content_executor
                .execute_task_with_interrupt(&service, &task_id, Some(interrupt))
                .await;
            if let Ok(mut current) = coordinator.current_interrupt.lock() {
                *current = None;
            }
            if let Ok(mut current) = coordinator.current_task.lock() {
                *current = None;
            }
        });
    }

    /// 暂停全部任务：持久化暂停标志、停止调度、在分段边界中断执行中的下载。
    fn pause_all(&self, service: &AppService) -> Result<(), String> {
        service
            .set_tasks_paused(true)
            .map_err(|error| error.to_string())?;
        self.interrupt_running_for_exit();
        Ok(())
    }

    /// 恢复全部任务：清除暂停标志，重新入队被全局暂停打断的任务并按优先级调度排队任务。
    fn resume_all(&self, service: &AppService) -> Result<(), String> {
        service
            .set_tasks_paused(false)
            .map_err(|error| error.to_string())?;
        self.scheduling_enabled.store(true, Ordering::Release);
        let install_ids = service
            .requeue_paused_install_tasks()
            .map_err(|error| error.to_string())?;
        let content_ids = service
            .requeue_paused_content_tasks()
            .map_err(|error| error.to_string())?;
        for task_id in install_ids {
            self.submit_install(service.clone(), task_id);
        }
        for task_id in content_ids {
            self.submit_content(service.clone(), task_id);
        }
        for task_id in service
            .queued_install_tasks_by_priority()
            .map_err(|error| error.to_string())?
        {
            self.submit_install(service.clone(), task_id);
        }
        for task_id in service
            .queued_content_tasks_by_priority()
            .map_err(|error| error.to_string())?
        {
            self.submit_content(service.clone(), task_id);
        }
        Ok(())
    }

    /// 单任务暂停：先以 `user` 来源标记暂停（排队任务直接退出调度）,
    /// 再中断执行中的下载;执行器完成前的中断标记不会覆盖 `user` 归因。
    fn pause_task(
        &self,
        service: &AppService,
        task_id: &str,
        kind: TaskKind,
    ) -> Result<(), String> {
        match kind {
            TaskKind::Install => {
                service
                    .mark_install_task_paused(task_id, "user")
                    .map_err(|error| error.to_string())?;
            }
            TaskKind::Content => {
                service
                    .mark_content_task_paused(task_id, "user")
                    .map_err(|error| error.to_string())?;
            }
        }
        let is_current = self
            .current_task
            .lock()
            .map(|current| {
                current
                    .as_ref()
                    .is_some_and(|(id, k)| id == task_id && *k == kind)
            })
            .unwrap_or(false);
        if is_current
            && let Ok(current) = self.current_interrupt.lock()
            && let Some(interrupt) = current.as_ref()
        {
            interrupt.interrupt();
        }
        Ok(())
    }

    /// 单任务恢复：重新入队并按全局调度继续。
    fn resume_task(
        &self,
        service: &AppService,
        task_id: &str,
        kind: TaskKind,
    ) -> Result<(), String> {
        match kind {
            TaskKind::Install => {
                service
                    .requeue_paused_install_task(task_id)
                    .map_err(|error| error.to_string())?;
                self.submit_install(service.clone(), task_id.to_owned());
            }
            TaskKind::Content => {
                service
                    .requeue_paused_content_task(task_id)
                    .map_err(|error| error.to_string())?;
                self.submit_content(service.clone(), task_id.to_owned());
            }
        }
        Ok(())
    }

    /// 单任务取消：先以事务标记已取消（取消优先，执行器收尾不会覆盖）,
    /// 若该任务正在当前执行槽中运行，则触发中断让下载在分段边界停止。
    fn cancel_task(
        &self,
        service: &AppService,
        task_id: &str,
        kind: TaskKind,
    ) -> Result<(), String> {
        match kind {
            TaskKind::Install => service
                .cancel_install_task(task_id)
                .map_err(|error| error.to_string())?,
            TaskKind::Content => service
                .cancel_content_task(task_id)
                .map_err(|error| error.to_string())?,
        }
        let is_current = self
            .current_task
            .lock()
            .map(|current| {
                current
                    .as_ref()
                    .is_some_and(|(id, k)| id == task_id && *k == kind)
            })
            .unwrap_or(false);
        if is_current
            && let Ok(current) = self.current_interrupt.lock()
            && let Some(interrupt) = current.as_ref()
        {
            interrupt.interrupt();
        }
        Ok(())
    }

    /// 停止调度并在分段边界中断执行中的下载。退出路径使用：
    /// 被中断的任务进入可恢复的暂停状态，不修改持久化暂停标志。
    fn interrupt_running_for_exit(&self) {
        self.scheduling_enabled.store(false, Ordering::Release);
        if let Ok(current) = self.current_interrupt.lock()
            && let Some(interrupt) = current.as_ref()
        {
            interrupt.interrupt();
        }
    }
}

#[derive(Debug, Clone, Default)]
struct LaunchCoordinator {
    stop_requests: Arc<Mutex<HashMap<String, oneshot::Sender<()>>>>,
}

impl LaunchCoordinator {
    fn register_stop_request(
        &self,
        instance_id: &str,
        sender: oneshot::Sender<()>,
    ) -> Result<(), String> {
        let mut requests = self
            .stop_requests
            .lock()
            .map_err(|_| "游戏进程协调器状态锁已损坏，请重启 MoyuMax".to_owned())?;
        if requests.contains_key(instance_id) {
            return Err("该实例已经在运行".to_owned());
        }
        requests.insert(instance_id.to_owned(), sender);
        Ok(())
    }

    fn submit(
        &self,
        service: AppService,
        execution: LaunchExecution,
    ) -> Result<LaunchSessionSummary, String> {
        let session = execution.session().clone();
        let instance_id = session.instance_id.clone();
        let (stop_sender, stop_receiver) = oneshot::channel();
        self.register_stop_request(&instance_id, stop_sender)?;
        moyumax_core::spawn_scheduled_world_backups(service.clone(), session.id.clone());
        let coordinator = self.clone();
        tauri::async_runtime::spawn(async move {
            let _ = run_launch_execution(&service, execution, stop_receiver).await;
            if let Ok(mut requests) = coordinator.stop_requests.lock() {
                requests.remove(&instance_id);
            }
        });
        Ok(session)
    }

    fn request_stop(&self, instance_id: &str) -> Result<(), String> {
        let sender = self
            .stop_requests
            .lock()
            .map_err(|_| "游戏进程协调器状态锁已损坏，请重启 MoyuMax".to_owned())?
            .remove(instance_id)
            .ok_or_else(|| "该实例当前没有可停止的游戏进程".to_owned())?;
        sender
            .send(())
            .map_err(|_| "游戏进程已经结束，正在刷新会话状态".to_owned())
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct InstallPreview {
    id: String,
    instance_name: String,
    game_version: String,
    loader_name: String,
    loader_version: Option<String>,
    java_distribution: JavaDistribution,
    java_version: String,
    java_architecture: JavaArchitecture,
    isolation: InstanceIsolation,
    estimated_download_bytes: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ContentInstallPreview {
    id: String,
    plan: ContentInstallPlan,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct WorldBackupSettings {
    interval_minutes: u64,
    keep_count: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UiPreferences {
    theme: String,
    language: String,
    motion: String,
    contrast: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ModpackPreviewResponse {
    id: String,
    preview: moyumax_core::ModpackPreview,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ModpackProgressEvent {
    stage: String,
    current: u64,
    total: u64,
    item: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DiagnosticExportPreviewResponse {
    id: String,
    #[serde(flatten)]
    preview: DiagnosticExportPreview,
}

#[tauri::command]
fn get_bootstrap_state(
    service: State<'_, AppService>,
    shell: State<'_, Arc<ShellCoordinator>>,
) -> Result<BootstrapState, String> {
    shell.note_bootstrap_ipc();
    service.bootstrap_state().map_err(|error| error.to_string())
}

#[tauri::command]
fn complete_onboarding(
    service: State<'_, AppService>,
    selection: OnboardingSelection,
) -> Result<(), String> {
    service
        .complete_onboarding(&selection)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn skip_onboarding(service: State<'_, AppService>) -> Result<(), String> {
    service.skip_onboarding().map_err(|error| error.to_string())
}

#[tauri::command]
async fn get_game_version_catalog(
    service: State<'_, AppService>,
    metadata: State<'_, MetadataClient>,
) -> Result<VersionCatalog, String> {
    let metadata = metadata
        .inner()
        .clone()
        .with_source_policy(service.download_source_policy().unwrap_or_default());
    match metadata.fetch_version_catalog().await {
        Ok(catalog) => {
            service
                .store_version_catalog(&catalog)
                .map_err(|error| error.to_string())?;
            Ok(catalog)
        }
        Err(network_error) => service
            .cached_version_catalog()
            .map_err(|error| error.to_string())?
            .ok_or_else(|| format!("无法连接版本服务，且本地没有可用缓存：{network_error}")),
    }
}

#[tauri::command]
async fn get_fabric_loaders(
    service: State<'_, AppService>,
    metadata: State<'_, MetadataClient>,
    game_version: String,
) -> Result<Vec<FabricLoaderSummary>, String> {
    metadata
        .inner()
        .clone()
        .with_source_policy(service.download_source_policy().unwrap_or_default())
        .compatible_fabric_loaders(&game_version)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn get_quilt_loaders(
    service: State<'_, AppService>,
    metadata: State<'_, MetadataClient>,
    game_version: String,
) -> Result<Vec<FabricLoaderSummary>, String> {
    metadata
        .inner()
        .clone()
        .with_source_policy(service.download_source_policy().unwrap_or_default())
        .compatible_quilt_loaders(&game_version)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn get_forge_versions(
    metadata: State<'_, MetadataClient>,
    game_version: String,
) -> Result<Vec<FabricLoaderSummary>, String> {
    metadata
        .compatible_forge_versions(&game_version)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn get_neoforge_versions(
    metadata: State<'_, MetadataClient>,
    game_version: String,
) -> Result<Vec<FabricLoaderSummary>, String> {
    metadata
        .compatible_neoforge_versions(&game_version)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn preview_install(
    service: State<'_, AppService>,
    metadata: State<'_, MetadataClient>,
    previews: State<'_, InstallPreviewStore>,
    selection: InstallSelection,
) -> Result<InstallPreview, String> {
    let catalog = service
        .cached_version_catalog()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "本地没有可信的官方版本目录，请重新加载新建实例页".to_owned())?;
    let trusted_version = catalog
        .versions
        .into_iter()
        .find(|version| version.id == selection.game_version.id)
        .ok_or_else(|| "所选 Minecraft 版本不在已缓存的官方目录中".to_owned())?;
    let trusted_selection = InstallSelection {
        game_version: trusted_version,
        ..selection
    };
    let metadata = metadata
        .inner()
        .clone()
        .with_source_policy(service.download_source_policy().unwrap_or_default());
    let request = metadata
        .resolve_install_request(&trusted_selection)
        .await
        .map_err(|error| error.to_string())?;
    let id = Uuid::new_v4().to_string();
    let preview = build_install_preview(id.clone(), &request);
    let mut store = previews
        .requests
        .lock()
        .map_err(|_| "安装预览状态锁已损坏，请重启 MoyuMax".to_owned())?;
    if store.len() >= 8 {
        store.clear();
    }
    store.insert(id, request);
    Ok(preview)
}

#[tauri::command]
fn confirm_install_preview(
    service: State<'_, AppService>,
    previews: State<'_, InstallPreviewStore>,
    coordinator: State<'_, TaskCoordinator>,
    preview_id: String,
) -> Result<InstallTask, String> {
    let request = previews
        .requests
        .lock()
        .map_err(|_| "安装预览状态锁已损坏，请重启 MoyuMax".to_owned())?
        .remove(&preview_id)
        .ok_or_else(|| "安装预览已失效，请返回重新确认".to_owned())?;
    let task = service
        .enqueue_install_task(&request)
        .map_err(|error| error.to_string())?;
    coordinator.submit_install(service.inner().clone(), task.id.clone());
    Ok(task)
}

#[tauri::command]
fn get_install_tasks(service: State<'_, AppService>) -> Result<Vec<InstallTask>, String> {
    service
        .list_install_tasks()
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn resolve_install_task_recovery(
    service: State<'_, AppService>,
    coordinator: State<'_, TaskCoordinator>,
    task_id: String,
    decision: RecoveryDecision,
) -> Result<(), String> {
    service
        .resolve_install_task_recovery(&task_id, decision)
        .map_err(|error| error.to_string())?;
    if decision == RecoveryDecision::Resume {
        coordinator.submit_install(service.inner().clone(), task_id);
    }
    Ok(())
}

#[tauri::command]
fn retry_install_task(
    service: State<'_, AppService>,
    coordinator: State<'_, TaskCoordinator>,
    task_id: String,
) -> Result<(), String> {
    service
        .retry_failed_install_task(&task_id)
        .map_err(|error| error.to_string())?;
    coordinator.submit_install(service.inner().clone(), task_id);
    Ok(())
}

#[tauri::command]
async fn search_modrinth_mods(
    modrinth: State<'_, ModrinthClient>,
    query: ModrinthSearchQuery,
) -> Result<ModrinthSearchPage, String> {
    modrinth
        .search_mods(&query)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn preview_modrinth_install(
    service: State<'_, AppService>,
    modrinth: State<'_, ModrinthClient>,
    previews: State<'_, ContentPreviewStore>,
    instance_id: String,
    project_id: String,
    selected_optional_projects: Vec<String>,
) -> Result<ContentInstallPreview, String> {
    let instance = service
        .list_instances()
        .map_err(|error| error.to_string())?
        .into_iter()
        .find(|instance| instance.id == instance_id)
        .ok_or_else(|| "目标实例不存在，请刷新实例列表".to_owned())?;
    let plan = modrinth
        .resolve_mod_install_plan(&instance, &project_id, &selected_optional_projects)
        .await
        .map_err(|error| error.to_string())?;
    let id = Uuid::new_v4().to_string();
    let mut store = previews
        .plans
        .lock()
        .map_err(|_| "内容安装预览状态锁已损坏，请重启 MoyuMax".to_owned())?;
    if store.len() >= 16 {
        store.clear();
    }
    store.insert(id.clone(), plan.clone());
    Ok(ContentInstallPreview { id, plan })
}

#[tauri::command]
fn confirm_content_preview(
    service: State<'_, AppService>,
    previews: State<'_, ContentPreviewStore>,
    coordinator: State<'_, TaskCoordinator>,
    preview_id: String,
) -> Result<ContentInstallTask, String> {
    let plan = previews
        .plans
        .lock()
        .map_err(|_| "内容安装预览状态锁已损坏，请重启 MoyuMax".to_owned())?
        .remove(&preview_id)
        .ok_or_else(|| "内容安装预览已失效，请重新确认依赖".to_owned())?;
    let task = service
        .enqueue_content_install_task(&plan)
        .map_err(|error| error.to_string())?;
    coordinator.submit_content(service.inner().clone(), task.id.clone());
    Ok(task)
}

#[tauri::command]
fn get_content_install_tasks(
    service: State<'_, AppService>,
) -> Result<Vec<ContentInstallTask>, String> {
    service
        .list_content_install_tasks()
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn get_installed_content(
    service: State<'_, AppService>,
    instance_id: String,
) -> Result<Vec<InstalledContent>, String> {
    service
        .list_installed_content(&instance_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn set_installed_content_enabled(
    service: State<'_, AppService>,
    content_id: String,
    enabled: bool,
) -> Result<InstalledContent, String> {
    service
        .set_installed_content_enabled(&content_id, enabled)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn get_instance_launch_options(
    service: State<'_, AppService>,
    instance_id: String,
) -> Result<Option<LaunchOptions>, String> {
    service
        .instance_launch_options(&instance_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn set_instance_launch_options(
    service: State<'_, AppService>,
    instance_id: String,
    options: LaunchOptions,
) -> Result<(), String> {
    service
        .set_instance_launch_options(&instance_id, &options)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn clear_instance_launch_options(
    service: State<'_, AppService>,
    instance_id: String,
) -> Result<(), String> {
    service
        .clear_instance_launch_options(&instance_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn get_global_launch_preference(
    service: State<'_, AppService>,
) -> Result<GlobalLaunchPreference, String> {
    service
        .global_launch_preference()
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn set_global_launch_preference(
    service: State<'_, AppService>,
    preference: GlobalLaunchPreference,
) -> Result<(), String> {
    service
        .set_global_launch_preference(&preference)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn get_auto_launch_options() -> LaunchOptions {
    auto_launch_options(total_physical_memory_mib())
}

#[tauri::command]
async fn check_content_updates(
    service: State<'_, AppService>,
    modrinth: State<'_, ModrinthClient>,
    instance_id: String,
) -> Result<Vec<ContentUpdateInfo>, String> {
    service
        .check_content_updates(&modrinth, &instance_id)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn plan_content_update(
    service: State<'_, AppService>,
    modrinth: State<'_, ModrinthClient>,
    coordinator: State<'_, TaskCoordinator>,
    instance_id: String,
    project_ids: Vec<String>,
) -> Result<ContentInstallTask, String> {
    let task = service
        .plan_content_update(&modrinth, &instance_id, &project_ids)
        .await
        .map_err(|error| error.to_string())?;
    coordinator.submit_content(service.inner().clone(), task.id.clone());
    Ok(task)
}

#[tauri::command]
fn get_instance_content_auto_update(
    service: State<'_, AppService>,
    instance_id: String,
) -> Result<bool, String> {
    service
        .instance_content_auto_update(&instance_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn set_instance_content_auto_update(
    service: State<'_, AppService>,
    instance_id: String,
    enabled: bool,
) -> Result<(), String> {
    service
        .set_instance_content_auto_update(&instance_id, enabled)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn list_instance_worlds(
    service: State<'_, AppService>,
    instance_id: String,
) -> Result<Vec<String>, String> {
    service
        .list_instance_worlds(&instance_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn list_instance_resources(
    service: State<'_, AppService>,
    instance_id: String,
) -> Result<Vec<InstanceResource>, String> {
    service
        .list_instance_resources(&instance_id, None)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn import_instance_resource(
    service: State<'_, AppService>,
    instance_id: String,
    kind: InstanceResourceKind,
    source_path: String,
    world_name: Option<String>,
) -> Result<InstanceResource, String> {
    service
        .import_instance_resource(
            &instance_id,
            kind,
            std::path::Path::new(&source_path),
            world_name.as_deref(),
        )
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn set_instance_resource_enabled(
    service: State<'_, AppService>,
    resource_id: String,
    enabled: bool,
) -> Result<InstanceResource, String> {
    service
        .set_instance_resource_enabled(&resource_id, enabled)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn list_instance_world_details(
    service: State<'_, AppService>,
    instance_id: String,
) -> Result<Vec<InstanceWorldInfo>, String> {
    service
        .list_instance_world_details(&instance_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn export_instance_world(
    service: State<'_, AppService>,
    instance_id: String,
    world_name: String,
    destination: String,
) -> Result<u64, String> {
    service
        .export_instance_world(
            &instance_id,
            &world_name,
            std::path::Path::new(&destination),
        )
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn export_instance_modpack(
    service: State<'_, AppService>,
    instance_id: String,
    options: ExportModpackOptions,
    destination_path: String,
) -> Result<ExportModpackReport, String> {
    service
        .export_instance_modpack(
            &instance_id,
            std::path::Path::new(&destination_path),
            &options,
        )
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn import_instance_world(
    service: State<'_, AppService>,
    instance_id: String,
    source_path: String,
) -> Result<InstanceWorldInfo, String> {
    service
        .import_instance_world(&instance_id, std::path::Path::new(&source_path))
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn rollback_world_backup(
    service: State<'_, AppService>,
    backup_id: String,
) -> Result<WorldBackupSummary, String> {
    service
        .rollback_world_backup(&backup_id)
        .map_err(|error| error.to_string())
}

/// 手动创建一个备份（触发类型 manual）。
#[tauri::command]
fn create_manual_world_backup(
    service: State<'_, AppService>,
    instance_id: String,
) -> Result<WorldBackupSummary, String> {
    service
        .create_world_backup(&instance_id, moyumax_core::BackupTrigger::Manual, None)
        .map_err(|error| error.to_string())
}

/// 手动删除一个备份（记录与归档随事务删除）。
#[tauri::command]
fn delete_world_backup(service: State<'_, AppService>, backup_id: String) -> Result<(), String> {
    service
        .delete_world_backup(&backup_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn list_instance_screenshots(
    service: State<'_, AppService>,
    instance_id: String,
) -> Result<Vec<InstanceScreenshot>, String> {
    service
        .list_instance_screenshots(&instance_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn read_instance_screenshot(
    service: State<'_, AppService>,
    instance_id: String,
    file_name: String,
) -> Result<Vec<u8>, String> {
    service
        .read_instance_screenshot(&instance_id, &file_name)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn open_screenshot_location(
    service: State<'_, AppService>,
    instance_id: String,
    file_name: String,
) -> Result<(), String> {
    let path = service
        .instance_screenshot_path(&instance_id, &file_name)
        .map_err(|error| error.to_string())?;
    std::process::Command::new("explorer.exe")
        .arg("/select,")
        .arg(&path)
        .spawn()
        .map_err(|error| format!("无法打开截图位置：{error}"))?;
    Ok(())
}

#[tauri::command]
fn delete_instance_screenshot(
    service: State<'_, AppService>,
    instance_id: String,
    file_name: String,
) -> Result<RecycleBinItem, String> {
    service
        .delete_instance_screenshot(&instance_id, &file_name)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn delete_instance_resource(
    service: State<'_, AppService>,
    resource_id: String,
) -> Result<RecycleBinItem, String> {
    service
        .delete_instance_resource(&resource_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn delete_instance_world(
    service: State<'_, AppService>,
    instance_id: String,
    world_name: String,
) -> Result<RecycleBinItem, String> {
    service
        .delete_instance_world(&instance_id, &world_name)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn list_instance_servers(
    service: State<'_, AppService>,
    instance_id: String,
) -> Result<Vec<InstanceServerEntry>, String> {
    service
        .list_instance_servers(&instance_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn add_instance_server(
    service: State<'_, AppService>,
    instance_id: String,
    name: String,
    address: String,
) -> Result<Vec<InstanceServerEntry>, String> {
    service
        .add_instance_server(&instance_id, &name, &address)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn remove_instance_server(
    service: State<'_, AppService>,
    instance_id: String,
    index: u32,
) -> Result<Vec<InstanceServerEntry>, String> {
    service
        .remove_instance_server(&instance_id, index)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn update_instance_server(
    service: State<'_, AppService>,
    instance_id: String,
    index: u32,
    name: String,
    address: String,
) -> Result<Vec<InstanceServerEntry>, String> {
    service
        .update_instance_server(&instance_id, index, &name, &address)
        .map_err(|error| error.to_string())
}

/// 探测服务器状态;同步阻塞 IO,在 Tauri 命令线程执行,前端自行限制并发。
#[tauri::command]
fn ping_minecraft_server(
    service: State<'_, AppService>,
    address: String,
) -> Result<MinecraftServerStatus, String> {
    service
        .ping_minecraft_server(&address)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn restore_recycled_entry(
    service: State<'_, AppService>,
    item_id: String,
) -> Result<RecycleBinItem, String> {
    service
        .restore_recycled_entry(&item_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn list_accounts(service: State<'_, AppService>) -> Result<Vec<AccountSummary>, String> {
    service.list_accounts().map_err(|error| error.to_string())
}

#[tauri::command]
fn add_offline_account(
    service: State<'_, AppService>,
    username: String,
) -> Result<AccountSummary, String> {
    service
        .add_offline_account(&username)
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn add_authlib_account(
    service: State<'_, AppService>,
    server_url: String,
    username: String,
    password: String,
) -> Result<AccountSummary, String> {
    let client = YggdrasilClient::with_base_url(&server_url).map_err(|error| error.to_string())?;
    service
        .add_authlib_account(&client, &server_url, &username, &password)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn set_default_account(service: State<'_, AppService>, account_id: String) -> Result<(), String> {
    service
        .set_default_account(&account_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn remove_account(service: State<'_, AppService>, account_id: String) -> Result<(), String> {
    service
        .remove_account(&account_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn refresh_account_session(
    service: State<'_, AppService>,
    account_id: String,
) -> Result<AccountSummary, String> {
    service
        .refresh_account_session(&account_id)
        .await
        .map_err(|error| error.to_string())
}

/// 设备码登录的非敏感展示信息（用户码与验证地址）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DeviceCodeInfo {
    user_code: String,
    verification_uri: String,
    expires_in_seconds: u64,
}

/// `microsoft-device-login` 事件负载（绝不携带令牌）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct MicrosoftLoginEvent {
    state: String,
    account: Option<AccountSummary>,
    message: Option<String>,
}

#[tauri::command]
async fn start_microsoft_device_login(
    app: tauri::AppHandle,
    service: State<'_, AppService>,
    login_state: State<'_, MicrosoftLoginState>,
) -> Result<DeviceCodeInfo, String> {
    {
        let running = login_state
            .running
            .lock()
            .map_err(|_| "登录状态不可用".to_owned())?;
        if *running {
            return Err("已有 Microsoft 登录正在进行".to_owned());
        }
    }
    let client = MicrosoftAuthClient::production().map_err(|error| error.to_string())?;
    let grant = client
        .begin_device_code()
        .await
        .map_err(|error| error.to_string())?;
    let info = DeviceCodeInfo {
        user_code: grant.user_code.clone(),
        verification_uri: grant.verification_uri.clone(),
        expires_in_seconds: grant.expires_in_seconds,
    };
    login_state.cancel.reset();
    {
        let mut running = login_state
            .running
            .lock()
            .map_err(|_| "登录状态不可用".to_owned())?;
        *running = true;
    }
    let cancel = login_state.cancel.clone();
    let service = service.inner().clone();
    let login_state = login_state.inner().clone();
    tauri::async_runtime::spawn(async move {
        let result = service
            .complete_microsoft_device_login(&client, &grant, &cancel)
            .await;
        let event = match result {
            Ok(account) => MicrosoftLoginEvent {
                state: "completed".to_owned(),
                account: Some(account),
                message: None,
            },
            Err(moyumax_core::CoreError::AccountLoginCancelled(message)) => MicrosoftLoginEvent {
                state: "cancelled".to_owned(),
                account: None,
                message: Some(message),
            },
            Err(error) => MicrosoftLoginEvent {
                state: "failed".to_owned(),
                account: None,
                message: Some(error.to_string()),
            },
        };
        if let Ok(mut running) = login_state.running.lock() {
            *running = false;
        }
        let _ = app.emit("microsoft-device-login", event);
    });
    Ok(info)
}

#[tauri::command]
fn cancel_microsoft_device_login(
    login_state: State<'_, MicrosoftLoginState>,
) -> Result<(), String> {
    login_state.cancel.cancel();
    Ok(())
}

/// 在系统浏览器打开外部链接；仅允许 https，拒绝其他协议。
#[tauri::command]
fn open_external_url(url: String) -> Result<(), String> {
    if !url.starts_with("https://") {
        return Err("只允许打开 https 链接".to_owned());
    }
    std::process::Command::new("rundll32")
        .args(["url.dll,FileProtocolHandler", &url])
        .spawn()
        .map_err(|error| format!("无法打开系统浏览器：{error}"))?;
    Ok(())
}

/// 创建或加入 EasyTier 联机房间；首次使用先下载校验 EasyTier。
#[tauri::command]
async fn start_netplay_room(
    app: tauri::AppHandle,
    service: State<'_, AppService>,
    coordinator: State<'_, NetplayCoordinator>,
    network_name: String,
    network_secret: String,
    is_host: bool,
) -> Result<NetplayRoomView, String> {
    let config = moyumax_core::NetplayRoomConfig {
        network_name: moyumax_core::validate_room_name(&network_name)
            .map_err(|error| error.to_string())?,
        network_secret: moyumax_core::validate_room_secret(&network_secret)
            .map_err(|error| error.to_string())?,
        is_host,
    };
    {
        let room = coordinator
            .room
            .lock()
            .map_err(|_| "联机状态不可用".to_owned())?;
        if room.is_some() {
            return Err("已在联机房间中，请先离开当前房间".to_owned());
        }
    }
    let tools_dir = service
        .selected_data_directory()
        .map_err(|error| error.to_string())?
        .join("tools");
    let http = moyumax_core::netplay_http_client().map_err(|error| error.to_string())?;
    let progress_app = app.clone();
    let binary = moyumax_core::ensure_easytier_binary(&http, &tools_dir, &move |current, total| {
        let _ = progress_app.emit(
            "netplay-download-progress",
            NetplayDownloadProgress { current, total },
        );
    })
    .await
    .map_err(|error| error.to_string())?;
    let rpc_port =
        moyumax_core::find_free_tcp_port().ok_or_else(|| "无法分配本机空闲端口".to_owned())?;
    let args = moyumax_core::easytier_args(&config, rpc_port);
    let child = std::process::Command::new(&binary)
        .args(&args)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|error| format!("无法启动 EasyTier：{error}"))?;
    let virtual_ip = if is_host {
        "10.144.144.1".to_owned()
    } else {
        "自动分配中…".to_owned()
    };
    let view = NetplayRoomView {
        network_name: config.network_name.clone(),
        virtual_ip: virtual_ip.clone(),
        is_host,
        mc_lan_port: None,
        forwarded_local_port: None,
    };
    let network_name = config.network_name.clone();
    {
        let mut room = coordinator
            .room
            .lock()
            .map_err(|_| "联机状态不可用".to_owned())?;
        *room = Some(NetplayRoomProcess {
            config,
            virtual_ip,
            child,
            rpc_port,
            cli_path: binary.with_file_name("easytier-cli.exe"),
            mc_lan_port: None,
            forwarded_local_port: None,
            lan_stop: None,
        });
    }
    if is_host {
        // 主机：监听 MC「对局域网开放」组播公告，自动记录端口号。
        let stop = Arc::new(AtomicBool::new(false));
        let listen_app = app.clone();
        let listener = moyumax_core::listen_mc_lan_announcements(stop.clone(), move |port| {
            let state = listen_app.state::<NetplayCoordinator>();
            if let Ok(mut room) = state.room.lock()
                && let Some(process) = room.as_mut()
            {
                process.mc_lan_port = Some(port);
            }
        });
        match listener {
            Ok(_handle) => {
                let mut room = coordinator
                    .room
                    .lock()
                    .map_err(|_| "联机状态不可用".to_owned())?;
                if let Some(process) = room.as_mut() {
                    process.lan_stop = Some(stop);
                }
            }
            Err(error) => {
                eprintln!("MC 局域网公告监听启动失败（不影响联机）：{error}");
            }
        }
    } else {
        // 客机：后台轮询 node info，取回 DHCP 实际分配的虚拟 IP。
        let poll_app = app.clone();
        let cli = binary.with_file_name("easytier-cli.exe");
        tauri::async_runtime::spawn(async move {
            for _ in 0..30 {
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                let cli_path = cli.clone();
                let output = tokio::task::spawn_blocking(move || {
                    std::process::Command::new(cli_path)
                        .args([
                            "-p",
                            &format!("127.0.0.1:{rpc_port}"),
                            "-o",
                            "json",
                            "node",
                            "info",
                        ])
                        .output()
                })
                .await;
                let Ok(Ok(output)) = output else { continue };
                let Ok(payload) = serde_json::from_slice::<serde_json::Value>(&output.stdout)
                else {
                    continue;
                };
                let Some(ip) = moyumax_core::parse_easytier_node_ipv4(&payload) else {
                    continue;
                };
                let state = poll_app.state::<NetplayCoordinator>();
                if let Ok(mut room) = state.room.lock()
                    && let Some(process) = room.as_mut()
                    && process.config.network_name == network_name
                    && !process.config.is_host
                {
                    process.virtual_ip = ip;
                }
                return;
            }
        });
    }
    Ok(view)
}

/// 客机把本机回环端口转发到主机虚拟 IP 的 MC 端口；返回本机端口供游戏内直连。
#[tauri::command]
async fn set_netplay_forward(
    coordinator: State<'_, NetplayCoordinator>,
    mc_port: u16,
) -> Result<u16, String> {
    let (cli_path, rpc_port) = {
        let room = coordinator
            .room
            .lock()
            .map_err(|_| "联机状态不可用".to_owned())?;
        let Some(process) = room.as_ref() else {
            return Err("当前不在联机房间中".to_owned());
        };
        if process.config.is_host {
            return Err("主机无需端口转发，直接告诉队友你的局域网端口即可".to_owned());
        }
        (process.cli_path.clone(), process.rpc_port)
    };
    let local_port =
        moyumax_core::find_free_tcp_port().ok_or_else(|| "无法分配本机空闲端口".to_owned())?;
    let target = format!("10.144.144.1:{mc_port}");
    for protocol in ["tcp", "udp"] {
        let cli = cli_path.clone();
        let bind = format!("127.0.0.1:{local_port}");
        let destination = target.clone();
        let output = tokio::task::spawn_blocking(move || {
            std::process::Command::new(cli)
                .args([
                    "-p",
                    &format!("127.0.0.1:{rpc_port}"),
                    "port-forward",
                    "add",
                    protocol,
                    &bind,
                    &destination,
                ])
                .output()
        })
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| format!("无法执行端口转发：{error}"))?;
        if !output.status.success() {
            let detail = String::from_utf8_lossy(&output.stderr);
            return Err(format!("端口转发失败（{protocol}）：{}", detail.trim()));
        }
    }
    let mut room = coordinator
        .room
        .lock()
        .map_err(|_| "联机状态不可用".to_owned())?;
    let Some(process) = room.as_mut() else {
        return Err("当前不在联机房间中".to_owned());
    };
    process.forwarded_local_port = Some(local_port);
    Ok(local_port)
}

/// 离开当前联机房间并终止 EasyTier 进程。
#[tauri::command]
fn stop_netplay_room(coordinator: State<'_, NetplayCoordinator>) -> Result<(), String> {
    let mut room = coordinator
        .room
        .lock()
        .map_err(|_| "联机状态不可用".to_owned())?;
    if let Some(mut process) = room.take() {
        if let Some(stop) = process.lan_stop.take() {
            stop.store(true, Ordering::Relaxed);
        }
        let _ = process.child.kill();
        let _ = process.child.wait();
    }
    Ok(())
}

/// 当前联机房间状态。
#[tauri::command]
fn get_netplay_status(
    coordinator: State<'_, NetplayCoordinator>,
) -> Result<Option<NetplayRoomView>, String> {
    let mut room = coordinator
        .room
        .lock()
        .map_err(|_| "联机状态不可用".to_owned())?;
    if let Some(process) = room.as_mut() {
        // 进程已退出时视为已离开房间。
        if let Ok(Some(_)) = process.child.try_wait() {
            if let Some(stop) = process.lan_stop.take() {
                stop.store(true, Ordering::Relaxed);
            }
            *room = None;
            return Ok(None);
        }
        return Ok(Some(NetplayRoomView {
            network_name: process.config.network_name.clone(),
            virtual_ip: process.virtual_ip.clone(),
            is_host: process.config.is_host,
            mc_lan_port: process.mc_lan_port,
            forwarded_local_port: process.forwarded_local_port,
        }));
    }
    Ok(None)
}

/// 当前房间成员列表（经 easytier-cli peer 解析；不在房间时返回空列表）。
#[tauri::command]
async fn list_netplay_peers(
    coordinator: State<'_, NetplayCoordinator>,
) -> Result<Vec<moyumax_core::EasyTierPeerView>, String> {
    let (cli_path, rpc_port) = {
        let room = coordinator
            .room
            .lock()
            .map_err(|_| "联机状态不可用".to_owned())?;
        let Some(process) = room.as_ref() else {
            return Ok(Vec::new());
        };
        (process.cli_path.clone(), process.rpc_port)
    };
    let output = tokio::task::spawn_blocking(move || {
        std::process::Command::new(cli_path)
            .args(["-p", &format!("127.0.0.1:{rpc_port}"), "-o", "json", "peer"])
            .output()
    })
    .await
    .map_err(|error| error.to_string())?
    .map_err(|error| format!("无法查询房间成员：{error}"))?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr);
        return Err(format!("房间成员查询失败：{}", detail.trim()));
    }
    let payload: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("房间成员信息无法解析：{error}"))?;
    Ok(moyumax_core::parse_easytier_peers(&payload))
}

/// 简化 NAT 检测（STUN，仅手动触发）。
#[tauri::command]
async fn detect_nat_type() -> Result<NatReportView, String> {
    let report = tokio::task::spawn_blocking(moyumax_core::detect_nat)
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())?;
    Ok(NatReportView {
        mapped_address: report.mapped_address,
        behind_nat: report.behind_nat,
        impact: report.impact.to_owned(),
    })
}

#[tauri::command]
fn get_ui_preferences(service: State<'_, AppService>) -> Result<UiPreferences, String> {
    Ok(UiPreferences {
        theme: service.ui_theme().map_err(|error| error.to_string())?,
        language: service.ui_language().map_err(|error| error.to_string())?,
        motion: service.ui_motion().map_err(|error| error.to_string())?,
        contrast: service.ui_contrast().map_err(|error| error.to_string())?,
    })
}

#[tauri::command]
fn set_ui_theme(service: State<'_, AppService>, theme: String) -> Result<(), String> {
    service
        .set_ui_theme(&theme)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn set_ui_language(service: State<'_, AppService>, language: String) -> Result<(), String> {
    service
        .set_ui_language(&language)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn set_ui_motion(service: State<'_, AppService>, motion: String) -> Result<(), String> {
    service
        .set_ui_motion(&motion)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn set_ui_contrast(service: State<'_, AppService>, contrast: String) -> Result<(), String> {
    service
        .set_ui_contrast(&contrast)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn get_cli_enabled(service: State<'_, AppService>) -> Result<bool, String> {
    service.cli_enabled().map_err(|error| error.to_string())
}

#[tauri::command]
fn set_cli_enabled(service: State<'_, AppService>, enabled: bool) -> Result<(), String> {
    service
        .set_cli_enabled(enabled)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn get_update_checks_enabled(service: State<'_, AppService>) -> Result<bool, String> {
    service
        .update_checks_enabled()
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn set_update_checks_enabled(service: State<'_, AppService>, enabled: bool) -> Result<(), String> {
    service
        .set_update_checks_enabled(enabled)
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn check_for_updates(service: State<'_, AppService>) -> Result<Option<ReleaseInfo>, String> {
    if !service
        .update_checks_enabled()
        .map_err(|error| error.to_string())?
    {
        return Err("更新提示已关闭；可在设置中重新开启".to_owned());
    }
    let client = UpdateClient::new().map_err(|error| error.to_string())?;
    client
        .check_latest(env!("CARGO_PKG_VERSION"))
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn download_update_installer(
    service: State<'_, AppService>,
    release: ReleaseInfo,
) -> Result<String, String> {
    if let Some(block) = min_version_block(env!("CARGO_PKG_VERSION"), &release) {
        return Err(block);
    }
    let asset = release
        .installer
        .ok_or_else(|| "该发布没有 Windows 安装包资产".to_owned())?;
    let client = UpdateClient::new().map_err(|error| error.to_string())?;
    let directory = service
        .selected_data_directory()
        .map_err(|error| error.to_string())?
        .join("updates")
        .join(&release.tag);
    let path = client
        .download_installer(&asset, &directory)
        .await
        .map_err(|error| error.to_string())?;
    Ok(path.to_string_lossy().into_owned())
}

#[tauri::command]
fn open_update_location(path: String) -> Result<(), String> {
    let path = std::path::PathBuf::from(&path);
    if !path.is_file() {
        return Err("安装包位置不存在或已删除".to_owned());
    }
    std::process::Command::new("explorer.exe")
        .arg("/select,")
        .arg(&path)
        .spawn()
        .map_err(|error| format!("无法打开安装包位置：{error}"))?;
    Ok(())
}

#[tauri::command]
fn get_ui_background(service: State<'_, AppService>) -> Result<UiBackground, String> {
    service.ui_background().map_err(|error| error.to_string())
}

#[tauri::command]
fn set_ui_background(
    service: State<'_, AppService>,
    background: UiBackground,
) -> Result<(), String> {
    service
        .set_ui_background(&background)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn import_background_image(
    service: State<'_, AppService>,
    source_path: String,
) -> Result<UiBackground, String> {
    service
        .import_background_image(std::path::Path::new(&source_path))
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn import_theme_pack(
    service: State<'_, AppService>,
    source_path: String,
) -> Result<ThemePack, String> {
    let source = std::fs::read_to_string(&source_path)
        .map_err(|error| format!("无法读取主题包：{error}"))?;
    let pack = moyumax_core::parse_theme_pack(&source).map_err(|error| error.to_string())?;
    service
        .set_ui_background(&UiBackground::ThemePack { pack: pack.clone() })
        .map_err(|error| error.to_string())?;
    Ok(pack)
}

#[tauri::command]
fn read_background_image(
    service: State<'_, AppService>,
) -> Result<Option<(String, Vec<u8>)>, String> {
    service
        .read_background_image()
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn import_modpack_preview(
    previews: State<'_, ModpackPreviewStore>,
    source_path: String,
) -> Result<ModpackPreviewResponse, String> {
    let path = PathBuf::from(&source_path);
    if !path.is_file() {
        return Err("整合包文件不存在".to_owned());
    }
    let plan = moyumax_core::parse_modpack_archive(&path).map_err(|error| error.to_string())?;
    let preview = moyumax_core::modpack_preview(&plan);
    let id = Uuid::new_v4().to_string();
    let mut store = previews
        .plans
        .lock()
        .map_err(|_| "整合包预览状态锁已损坏，请重启 MoyuMax".to_owned())?;
    if store.len() >= 8 {
        store.clear();
    }
    store.insert(id.clone(), (plan, path));
    Ok(ModpackPreviewResponse { id, preview })
}

#[tauri::command]
async fn install_modpack(
    app: tauri::AppHandle,
    service: State<'_, AppService>,
    metadata: State<'_, MetadataClient>,
    previews: State<'_, ModpackPreviewStore>,
    coordinator: State<'_, TaskCoordinator>,
    modpack_guard: State<'_, ModpackInstallCoordinator>,
    preview_id: String,
) -> Result<ModpackInstallReport, String> {
    let (plan, archive_path) = previews
        .plans
        .lock()
        .map_err(|_| "整合包预览状态锁已损坏，请重启 MoyuMax".to_owned())?
        .remove(&preview_id)
        .ok_or_else(|| "整合包预览已失效，请重新选择文件".to_owned())?;
    let online_staging = archive_path.parent().and_then(|uuid_dir| {
        uuid_dir
            .parent()
            .filter(|parent| parent.ends_with("online-modpacks"))
            .map(|_| uuid_dir.to_path_buf())
    });
    let instance = create_modpack_instance(&app, &service, &metadata, &coordinator, &plan).await;
    let instance = match instance {
        Ok(instance) => instance,
        Err(error) => {
            if let Some(staging) = online_staging {
                let _ = std::fs::remove_dir_all(staging);
            }
            return Err(error);
        }
    };
    emit_modpack_progress(
        &app,
        "files",
        0,
        plan.files.len() as u64,
        "准备下载整合包文件",
    );
    modpack_guard.begin(&instance.id);
    let mci = MciMirrorClient::new().map_err(|error| error.to_string())?;
    let progress_app = app.clone();
    let guard_result = service
        .install_modpack_files(
            &plan,
            &archive_path,
            &instance.id,
            &mci,
            &|current, total, item| {
                emit_modpack_progress(&progress_app, "files", current, total, item);
            },
        )
        .await;
    modpack_guard.finish(&instance.id);
    if let Some(staging) = online_staging {
        let _ = std::fs::remove_dir_all(staging);
    }
    guard_result.map_err(|error| error.to_string())
}

#[tauri::command]
async fn update_modpack(
    app: tauri::AppHandle,
    service: State<'_, AppService>,
    modpack_guard: State<'_, ModpackInstallCoordinator>,
    instance_id: String,
    source_path: String,
) -> Result<ModpackUpdateReport, String> {
    let archive_path = PathBuf::from(&source_path);
    if !archive_path.is_file() {
        return Err("整合包文件不存在".to_owned());
    }
    let plan =
        moyumax_core::parse_modpack_archive(&archive_path).map_err(|error| error.to_string())?;
    modpack_guard.begin(&instance_id);
    let mci = MciMirrorClient::new().map_err(|error| error.to_string())?;
    let progress_app = app.clone();
    let result = service
        .update_modpack(
            &plan,
            &archive_path,
            &instance_id,
            &mci,
            &|current, total, item| {
                emit_modpack_progress(&progress_app, "files", current, total, item);
            },
        )
        .await;
    modpack_guard.finish(&instance_id);
    result.map_err(|error| error.to_string())
}

/// 该实例的整合包文件是否正在安装中。
#[tauri::command]
fn is_modpack_installing(
    modpack_guard: State<'_, ModpackInstallCoordinator>,
    instance_id: String,
) -> Result<bool, String> {
    Ok(modpack_guard.is_installing(&instance_id))
}

#[tauri::command]
fn get_instance_modpack(
    service: State<'_, AppService>,
    instance_id: String,
) -> Result<Option<InstalledModpack>, String> {
    service
        .installed_modpack(&instance_id)
        .map_err(|error| error.to_string())
}

/// 在线整合包预览：下载 mrpack 到暂存目录（SHA-1 校验）后解析预览；
/// 确认安装复用 `install_modpack`，暂存文件在安装结束后清理。
#[tauri::command]
async fn preview_online_modpack(
    service: State<'_, AppService>,
    previews: State<'_, ModpackPreviewStore>,
    project_id: String,
) -> Result<ModpackPreviewResponse, String> {
    let client = ModrinthClient::new().map_err(|error| error.to_string())?;
    let file = client
        .latest_project_file(&project_id, None, None)
        .await
        .map_err(|error| error.to_string())?;
    let staging_directory = service
        .selected_data_directory()
        .map_err(|error| error.to_string())?
        .join(".staging")
        .join("online-modpacks")
        .join(Uuid::new_v4().simple().to_string());
    let archive_path = client
        .download_project_file(&file, &staging_directory)
        .await
        .map_err(|error| error.to_string())?;
    let plan = match moyumax_core::parse_modpack_archive(&archive_path) {
        Ok(plan) => plan,
        Err(error) => {
            let _ = std::fs::remove_dir_all(&staging_directory);
            return Err(error.to_string());
        }
    };
    let preview = moyumax_core::modpack_preview(&plan);
    let id = Uuid::new_v4().to_string();
    let mut store = previews
        .plans
        .lock()
        .map_err(|_| "整合包预览状态锁已损坏，请重启 MoyuMax".to_owned())?;
    if store.len() >= 8 {
        store.clear();
    }
    store.insert(id.clone(), (plan, archive_path));
    Ok(ModpackPreviewResponse { id, preview })
}

/// 项目版本列表（自由下载对话框的版本选择）。
#[tauri::command]
async fn list_modrinth_versions(
    project_id: String,
    game_version: Option<String>,
    loader: Option<String>,
) -> Result<Vec<ModrinthVersionSummary>, String> {
    let client = ModrinthClient::new().map_err(|error| error.to_string())?;
    client
        .project_versions(&project_id, game_version.as_deref(), loader.as_deref())
        .await
        .map_err(|error| error.to_string())
}

/// 自由下载：指定版本主文件下载到目标目录并按自定义文件名保存。
#[tauri::command]
async fn download_modrinth_file(
    version_id: String,
    target_dir: String,
    file_name: String,
) -> Result<String, String> {
    let target = PathBuf::from(&target_dir);
    if target_dir.trim().is_empty() || !target.is_dir() {
        return Err("保存目录不存在".to_owned());
    }
    let client = ModrinthClient::new().map_err(|error| error.to_string())?;
    let path = client
        .download_version_file(&version_id, &target, &file_name)
        .await
        .map_err(|error| error.to_string())?;
    Ok(path.to_string_lossy().into_owned())
}

/// 在线资源安装：资源包/光影/模组按实例约束解析文件（指定版本优先，
/// 否则取最新兼容；模组额外按实例加载器过滤），下载校验后走
/// M16 本地导入事务（同名拒绝、实例隔离、中断收敛）。
#[tauri::command]
async fn install_online_resource(
    service: State<'_, AppService>,
    instance_id: String,
    kind: InstanceResourceKind,
    project_id: String,
    version_id: Option<String>,
) -> Result<InstanceResource, String> {
    if matches!(kind, InstanceResourceKind::Datapack) {
        return Err("数据包必须选择目标世界，请走本地导入".to_owned());
    }
    let instances = service
        .list_instances()
        .map_err(|error| error.to_string())?;
    let instance = instances
        .iter()
        .find(|candidate| candidate.id == instance_id)
        .ok_or_else(|| "目标实例不存在".to_owned())?;
    let client = ModrinthClient::new().map_err(|error| error.to_string())?;
    let loader_filter = match kind {
        InstanceResourceKind::Mod if instance.loader_kind != "vanilla" => {
            Some(instance.loader_kind.as_str())
        }
        _ => None,
    };
    let file = match version_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(selected) => client
            .project_version_file(selected)
            .await
            .map_err(|error| error.to_string())?,
        None => client
            .latest_project_file(&project_id, Some(&instance.game_version), loader_filter)
            .await
            .map_err(|error| error.to_string())?,
    };
    let staging_directory = service
        .selected_data_directory()
        .map_err(|error| error.to_string())?
        .join(".staging")
        .join("online-resources")
        .join(Uuid::new_v4().simple().to_string());
    let downloaded = client
        .download_project_file(&file, &staging_directory)
        .await
        .map_err(|error| error.to_string())?;
    let result = service.import_instance_resource(&instance_id, kind, &downloaded, None);
    let _ = std::fs::remove_dir_all(&staging_directory);
    result.map_err(|error| error.to_string())
}

async fn create_modpack_instance(
    app: &tauri::AppHandle,
    service: &AppService,
    metadata: &MetadataClient,
    coordinator: &TaskCoordinator,
    plan: &moyumax_core::ModpackPlan,
) -> Result<ManagedInstanceSummary, String> {
    let catalog = service
        .cached_version_catalog()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "本地没有可信的官方版本目录，请先打开一次新建实例页".to_owned())?;
    let trusted_version = catalog
        .versions
        .into_iter()
        .find(|version| version.id == plan.game_version)
        .ok_or_else(|| {
            format!(
                "整合包要求的 Minecraft {} 不在已缓存的官方目录中",
                plan.game_version
            )
        })?;
    let loader = match plan.loader_kind.as_str() {
        "fabric" => LoaderChoice::Fabric {
            version: plan.loader_version.clone(),
        },
        "quilt" => LoaderChoice::Quilt {
            version: plan.loader_version.clone(),
        },
        "forge" => LoaderChoice::Forge {
            version: plan.loader_version.clone(),
        },
        "neoforge" => LoaderChoice::NeoForge {
            version: plan.loader_version.clone(),
        },
        other => return Err(format!("不支持的加载器：{other}")),
    };
    let selection = InstallSelection {
        instance_name: format!("{} {}", plan.name, plan.version),
        game_version: trusted_version,
        loader,
        isolation: InstanceIsolation::Full,
    };
    emit_modpack_progress(app, "game", 0, 1, "解析游戏安装计划");
    let metadata = metadata
        .clone()
        .with_source_policy(service.download_source_policy().unwrap_or_default());
    let request = metadata
        .resolve_install_request(&selection)
        .await
        .map_err(|error| error.to_string())?;
    let task = service
        .enqueue_install_task(&request)
        .map_err(|error| error.to_string())?;
    coordinator.submit_install(service.clone(), task.id.clone());
    // 不做硬性总时长死线:慢网下载可以很久;改为停滞检测——进度字节
    // 在窗口内没有任何推进才判卡死(处理器 CPU 阶段无字节推进属正常,
    // 窗口放宽到 15 分钟),报错必须带上阶段与当前项。
    const STALL_LIMIT: std::time::Duration = std::time::Duration::from_secs(15 * 60);
    let mut last_progress: Option<(u64, std::time::Instant)> = None;
    loop {
        let tasks = service
            .list_install_tasks()
            .map_err(|error| error.to_string())?;
        let current = tasks
            .iter()
            .find(|candidate| candidate.id == task.id)
            .ok_or_else(|| "游戏安装任务丢失".to_owned())?;
        match current.state {
            moyumax_core::TaskState::Completed => break,
            moyumax_core::TaskState::Failed => {
                let summary = current
                    .progress
                    .error_summary
                    .clone()
                    .unwrap_or_else(|| "游戏安装失败".to_owned());
                return Err(format!("游戏安装失败：{summary}"));
            }
            moyumax_core::TaskState::Cancelled => {
                return Err("游戏安装任务已取消".to_owned());
            }
            _ => {
                let completed = current.progress.completed_bytes;
                let now = std::time::Instant::now();
                match last_progress {
                    Some((previous, since)) if previous == completed => {
                        if now.duration_since(since) > STALL_LIMIT {
                            let stage = current
                                .current_stage
                                .map(|stage| format!("{stage:?}"))
                                .unwrap_or_else(|| "未知".to_owned());
                            let item = current
                                .progress
                                .current_item
                                .clone()
                                .unwrap_or_else(|| "无当前项".to_owned());
                            // 判卡死后取消任务,避免遗留 running 僵尸占用恢复队列。
                            let _ = service.cancel_install_task(&task.id);
                            return Err(format!(
                                "游戏安装停滞超过 15 分钟（阶段 {stage}，当前项 {item}，进度 {completed} 字节）"
                            ));
                        }
                    }
                    _ => last_progress = Some((completed, now)),
                }
                emit_modpack_progress(
                    app,
                    "game",
                    completed,
                    current.progress.total_bytes.unwrap_or(0),
                    &current
                        .progress
                        .current_item
                        .clone()
                        .unwrap_or_else(|| "正在安装游戏".to_owned()),
                );
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }
        }
    }
    service
        .list_instances()
        .map_err(|error| error.to_string())?
        .into_iter()
        .find(|instance| instance.name == selection.instance_name)
        .ok_or_else(|| "游戏安装完成但实例未登记".to_owned())
}

fn emit_modpack_progress(
    app: &tauri::AppHandle,
    stage: &str,
    current: u64,
    total: u64,
    item: &str,
) {
    let _ = app.emit(
        "modpack-progress",
        ModpackProgressEvent {
            stage: stage.to_owned(),
            current,
            total,
            item: item.to_owned(),
        },
    );
}

#[tauri::command]
fn get_world_backup_settings(
    service: State<'_, AppService>,
) -> Result<WorldBackupSettings, String> {
    Ok(WorldBackupSettings {
        interval_minutes: service
            .world_backup_interval_minutes()
            .map_err(|error| error.to_string())?,
        keep_count: service
            .world_backup_keep_count()
            .map_err(|error| error.to_string())?,
    })
}

#[tauri::command]
fn set_world_backup_interval_minutes(
    service: State<'_, AppService>,
    minutes: u64,
) -> Result<(), String> {
    service
        .set_world_backup_interval_minutes(minutes)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn set_world_backup_keep_count(service: State<'_, AppService>, count: u64) -> Result<(), String> {
    service
        .set_world_backup_keep_count(count)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn retry_content_task(
    service: State<'_, AppService>,
    coordinator: State<'_, TaskCoordinator>,
    task_id: String,
) -> Result<(), String> {
    service
        .retry_failed_content_task(&task_id)
        .map_err(|error| error.to_string())?;
    coordinator.submit_content(service.inner().clone(), task_id);
    Ok(())
}

#[tauri::command]
fn resolve_content_task_recovery(
    service: State<'_, AppService>,
    coordinator: State<'_, TaskCoordinator>,
    task_id: String,
    decision: RecoveryDecision,
) -> Result<(), String> {
    service
        .resolve_content_task_recovery(&task_id, decision)
        .map_err(|error| error.to_string())?;
    if decision == RecoveryDecision::Resume {
        coordinator.submit_content(service.inner().clone(), task_id);
    }
    Ok(())
}

#[tauri::command]
fn list_instances(service: State<'_, AppService>) -> Result<Vec<ManagedInstanceSummary>, String> {
    service.list_instances().map_err(|error| error.to_string())
}

#[tauri::command]
fn list_recycle_bin_items(service: State<'_, AppService>) -> Result<Vec<RecycleBinItem>, String> {
    service
        .list_recycle_bin_items()
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn recycle_instance(
    service: State<'_, AppService>,
    instance_id: String,
) -> Result<RecycleBinItem, String> {
    let service = service.inner().clone();
    tauri::async_runtime::spawn_blocking(move || service.recycle_instance(&instance_id))
        .await
        .map_err(|error| format!("后台回收操作中断：{error}"))?
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn restore_recycle_bin_item(
    service: State<'_, AppService>,
    item_id: String,
) -> Result<ManagedInstanceSummary, String> {
    let service = service.inner().clone();
    tauri::async_runtime::spawn_blocking(move || service.restore_recycle_bin_item(&item_id))
        .await
        .map_err(|error| format!("后台恢复操作中断：{error}"))?
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn purge_recycle_bin_item(
    service: State<'_, AppService>,
    item_id: String,
) -> Result<RecyclePurgeResult, String> {
    let service = service.inner().clone();
    tauri::async_runtime::spawn_blocking(move || service.purge_recycle_bin_item(&item_id))
        .await
        .map_err(|error| format!("后台永久删除操作中断：{error}"))?
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn list_world_backups(
    service: State<'_, AppService>,
    instance_id: Option<String>,
) -> Result<Vec<WorldBackupSummary>, String> {
    service
        .list_world_backups(instance_id.as_deref())
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn start_instance(
    service: State<'_, AppService>,
    coordinator: State<'_, LaunchCoordinator>,
    modpack_guard: State<'_, ModpackInstallCoordinator>,
    instance_id: String,
) -> Result<LaunchSessionSummary, String> {
    if modpack_guard.is_installing(&instance_id) {
        return Err("整合包文件仍在安装中，请等待安装完成后再启动".to_owned());
    }
    let service = service.inner().clone();
    let coordinator = coordinator.inner().clone();
    let account = service
        .account_launch_identity(None)
        .await
        .map_err(|error| error.to_string())?;
    let preparation_service = service.clone();
    let execution = tauri::async_runtime::spawn_blocking(move || {
        // 启动内存解析链:实例自定义 → 全局自定义 → 自动分配。
        let options = preparation_service.resolved_launch_options(&instance_id)?;
        preparation_service.create_launch_execution(&instance_id, &account, &options)
    })
    .await
    .map_err(|error| format!("后台启动检查中断：{error}"))?
    .map_err(|error| error.to_string())?;
    coordinator.submit(service, execution)
}

#[tauri::command]
fn stop_instance(
    coordinator: State<'_, LaunchCoordinator>,
    instance_id: String,
) -> Result<(), String> {
    coordinator.request_stop(&instance_id)
}

#[tauri::command]
fn list_launch_sessions(
    service: State<'_, AppService>,
) -> Result<Vec<LaunchSessionSummary>, String> {
    service
        .list_launch_sessions()
        .map_err(|error| error.to_string())
}

/// 游戏日志副页的尾部跟随读取：按双通道字节偏移返回增量内容与最新会话状态。
#[tauri::command]
fn read_launch_log(
    service: State<'_, AppService>,
    session_id: String,
    stdout_offset: u64,
    stderr_offset: u64,
) -> Result<LaunchLogRead, String> {
    service
        .read_launch_log(&session_id, stdout_offset, stderr_offset)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn open_launch_log_location(
    service: State<'_, AppService>,
    session_id: String,
) -> Result<(), String> {
    let session = service
        .list_launch_sessions()
        .map_err(|error| error.to_string())?
        .into_iter()
        .find(|session| session.id == session_id)
        .ok_or_else(|| "启动会话不存在".to_owned())?;
    let path = std::path::Path::new(&session.stdout_path);
    if !path.is_file() {
        return Err("日志文件不存在或已删除".to_owned());
    }
    std::process::Command::new("explorer.exe")
        .arg("/select,")
        .arg(path)
        .spawn()
        .map_err(|error| format!("无法打开日志位置：{error}"))?;
    Ok(())
}

#[tauri::command]
fn list_crash_reports(service: State<'_, AppService>) -> Result<Vec<CrashReportSummary>, String> {
    service
        .list_crash_reports()
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn preview_diagnostic_export(
    service: State<'_, AppService>,
    previews: State<'_, DiagnosticPreviewStore>,
    report_id: String,
) -> Result<DiagnosticExportPreviewResponse, String> {
    let preview = service
        .preview_diagnostic_export(&report_id)
        .map_err(|error| error.to_string())?;
    let id = Uuid::new_v4().to_string();
    let mut reports = previews
        .reports
        .lock()
        .map_err(|_| "诊断导出预览状态锁已损坏，请重启 MoyuMax".to_owned())?;
    if reports.len() >= 32 {
        reports.clear();
    }
    reports.insert(id.clone(), report_id);
    Ok(DiagnosticExportPreviewResponse { id, preview })
}

#[tauri::command]
async fn confirm_diagnostic_export(
    service: State<'_, AppService>,
    previews: State<'_, DiagnosticPreviewStore>,
    preview_id: String,
) -> Result<DiagnosticExportResult, String> {
    let report_id = previews
        .reports
        .lock()
        .map_err(|_| "诊断导出预览状态锁已损坏，请重启 MoyuMax".to_owned())?
        .remove(&preview_id)
        .ok_or_else(|| "诊断导出预览已失效，请重新查看文件清单".to_owned())?;
    let service = service.inner().clone();
    let export_report_id = report_id.clone();
    let export = match tauri::async_runtime::spawn_blocking(move || {
        service.export_diagnostic_bundle(&export_report_id)
    })
    .await
    {
        Ok(result) => result.map_err(|error| error.to_string()),
        Err(error) => Err(format!("后台诊断导出中断：{error}")),
    };
    if export.is_err() {
        previews
            .reports
            .lock()
            .map_err(|_| "诊断导出预览状态锁已损坏，请重启 MoyuMax".to_owned())?
            .insert(preview_id, report_id);
    }
    export
}

#[tauri::command]
fn get_window_close_behavior(
    service: State<'_, AppService>,
) -> Result<WindowCloseBehavior, String> {
    service
        .window_close_behavior()
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn set_window_close_behavior(
    service: State<'_, AppService>,
    behavior: WindowCloseBehavior,
) -> Result<(), String> {
    service
        .set_window_close_behavior(behavior)
        .map_err(|error| error.to_string())
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct WindowCloseResolution {
    action: WindowCloseAction,
    remember: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
enum WindowCloseAction {
    Minimize,
    Exit,
}

#[tauri::command]
async fn resolve_window_close(
    app: tauri::AppHandle,
    service: State<'_, AppService>,
    shell: State<'_, Arc<ShellCoordinator>>,
    resolution: WindowCloseResolution,
) -> Result<(), String> {
    if resolution.remember {
        let behavior = match resolution.action {
            WindowCloseAction::Minimize => WindowCloseBehavior::MinimizeToTray,
            WindowCloseAction::Exit => WindowCloseBehavior::Exit,
        };
        service
            .set_window_close_behavior(behavior)
            .map_err(|error| error.to_string())?;
    }
    match resolution.action {
        WindowCloseAction::Minimize => minimize_to_tray(&app, &shell),
        WindowCloseAction::Exit => {
            let impact = service
                .exit_impact_summary()
                .map_err(|error| error.to_string())?;
            if impact.requires_confirmation() {
                return Err("退出前需要确认运行中游戏与活动任务的影响".to_owned());
            }
            let service = service.inner().clone();
            let tasks = app.state::<TaskCoordinator>().inner().clone();
            let launches = app.state::<LaunchCoordinator>().inner().clone();
            confirm_graceful_exit(app, service, tasks, launches, Arc::clone(&shell)).await
        }
    }
}

#[tauri::command]
fn get_exit_impact(service: State<'_, AppService>) -> Result<ExitImpactSummary, String> {
    service
        .exit_impact_summary()
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn confirm_exit(
    app: tauri::AppHandle,
    service: State<'_, AppService>,
    tasks: State<'_, TaskCoordinator>,
    launches: State<'_, LaunchCoordinator>,
    shell: State<'_, Arc<ShellCoordinator>>,
) -> Result<(), String> {
    confirm_graceful_exit(
        app,
        service.inner().clone(),
        tasks.inner().clone(),
        launches.inner().clone(),
        Arc::clone(&shell),
    )
    .await
}

#[tauri::command]
fn force_exit(app: tauri::AppHandle, shell: State<'_, Arc<ShellCoordinator>>) {
    shell.begin_exit();
    app.exit(0);
}

#[tauri::command]
fn get_shell_state(service: State<'_, AppService>) -> Result<Option<ShellState>, String> {
    service.shell_state().map_err(|error| error.to_string())
}

#[tauri::command]
fn persist_shell_state(service: State<'_, AppService>, state: ShellState) -> Result<(), String> {
    service
        .persist_shell_state(&state)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn get_window_startup_kind(shell: State<'_, Arc<ShellCoordinator>>) -> WindowStartupKind {
    shell.startup_kind()
}

#[tauri::command]
fn take_pending_intent(shell: State<'_, Arc<ShellCoordinator>>) -> Option<PendingIntent> {
    shell.take_pending_intent()
}

#[tauri::command]
fn get_download_source_policy(service: State<'_, AppService>) -> Result<SourcePolicy, String> {
    service
        .download_source_policy()
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn set_download_source_policy(
    service: State<'_, AppService>,
    policy: SourcePolicy,
) -> Result<(), String> {
    service
        .set_download_source_policy(&policy)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn get_tasks_paused(service: State<'_, AppService>) -> Result<bool, String> {
    service.tasks_paused().map_err(|error| error.to_string())
}

#[tauri::command]
fn pause_all_tasks(
    service: State<'_, AppService>,
    tasks: State<'_, TaskCoordinator>,
) -> Result<(), String> {
    tasks.pause_all(&service)
}

#[tauri::command]
fn resume_all_tasks(
    service: State<'_, AppService>,
    tasks: State<'_, TaskCoordinator>,
) -> Result<(), String> {
    tasks.resume_all(&service)
}

#[tauri::command]
fn pause_task(
    service: State<'_, AppService>,
    tasks: State<'_, TaskCoordinator>,
    task_id: String,
    kind: TaskKind,
) -> Result<(), String> {
    tasks.pause_task(&service, &task_id, kind)
}

#[tauri::command]
fn resume_task(
    service: State<'_, AppService>,
    tasks: State<'_, TaskCoordinator>,
    task_id: String,
    kind: TaskKind,
) -> Result<(), String> {
    tasks.resume_task(&service, &task_id, kind)
}

#[tauri::command]
fn set_task_priority(
    service: State<'_, AppService>,
    task_id: String,
    kind: TaskKind,
    priority: i64,
) -> Result<(), String> {
    match kind {
        TaskKind::Install => service
            .set_install_task_priority(&task_id, priority)
            .map_err(|error| error.to_string()),
        TaskKind::Content => service
            .set_content_task_priority(&task_id, priority)
            .map_err(|error| error.to_string()),
    }
}

#[tauri::command]
fn cancel_install_task(
    service: State<'_, AppService>,
    tasks: State<'_, TaskCoordinator>,
    task_id: String,
) -> Result<(), String> {
    tasks.cancel_task(&service, &task_id, TaskKind::Install)
}

#[tauri::command]
fn cancel_content_task(
    service: State<'_, AppService>,
    tasks: State<'_, TaskCoordinator>,
    task_id: String,
) -> Result<(), String> {
    tasks.cancel_task(&service, &task_id, TaskKind::Content)
}

#[tauri::command]
fn delete_install_task(service: State<'_, AppService>, task_id: String) -> Result<(), String> {
    service
        .delete_install_task(&task_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn delete_content_task(service: State<'_, AppService>, task_id: String) -> Result<(), String> {
    service
        .delete_content_task(&task_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn get_download_concurrency(service: State<'_, AppService>) -> Result<usize, String> {
    service
        .download_concurrency()
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn set_download_concurrency(
    service: State<'_, AppService>,
    connections: usize,
) -> Result<(), String> {
    service
        .set_download_concurrency(connections)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn get_download_speed_limit(service: State<'_, AppService>) -> Result<u64, String> {
    service
        .download_speed_limit()
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn set_download_speed_limit(
    service: State<'_, AppService>,
    bytes_per_sec: u64,
) -> Result<(), String> {
    service
        .set_download_speed_limit(bytes_per_sec)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn get_proxy_preference(service: State<'_, AppService>) -> Result<ProxyPreference, String> {
    service
        .proxy_preference()
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn set_proxy_preference(
    service: State<'_, AppService>,
    metadata: State<'_, MetadataClient>,
    modrinth: State<'_, ModrinthClient>,
    preference: ProxyPreference,
) -> Result<(), String> {
    service
        .set_proxy_preference(&preference)
        .map_err(|error| error.to_string())?;
    // 启动期构造的客户端热重载,代理设置免重启生效。
    metadata.reload_http_client();
    modrinth.reload_http_client();
    Ok(())
}

#[tauri::command]
fn list_java_environments(
    service: State<'_, AppService>,
) -> Result<Vec<JavaEnvironmentSummary>, String> {
    service
        .list_java_environments()
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn list_deleted_java_environments(
    service: State<'_, AppService>,
) -> Result<Vec<JavaEnvironmentSummary>, String> {
    service
        .list_deleted_java_environments()
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn delete_java_environment(
    service: State<'_, AppService>,
    environment_id: String,
    force: bool,
) -> Result<JavaDeleteOutcome, String> {
    service
        .delete_java_environment(&environment_id, force)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn verify_java_environment(
    service: State<'_, AppService>,
    environment_id: String,
) -> Result<bool, String> {
    service
        .verify_java_environment(&environment_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn restore_java_environment(
    service: State<'_, AppService>,
    metadata: State<'_, MetadataClient>,
    environment_id: String,
) -> Result<JavaEnvironmentSummary, String> {
    let package = service
        .resolve_java_restore_package(&metadata, &environment_id)
        .await
        .map_err(|error| error.to_string())?;
    let downloader = ArtifactDownloader::new(4).map_err(|error| error.to_string())?;
    service
        .restore_java_environment(&package, &downloader, &environment_id)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn set_instance_java_environment(
    service: State<'_, AppService>,
    instance_id: String,
    environment_id: String,
) -> Result<(), String> {
    service
        .set_instance_java_environment(&instance_id, &environment_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn open_java_location(
    service: State<'_, AppService>,
    environment_id: String,
) -> Result<(), String> {
    let home = service
        .list_java_environments()
        .map_err(|error| error.to_string())?
        .into_iter()
        .find(|environment| environment.id == environment_id)
        .map(|environment| environment.home_directory)
        .or_else(|| {
            service
                .list_deleted_java_environments()
                .ok()?
                .into_iter()
                .find(|environment| environment.id == environment_id)
                .map(|environment| environment.home_directory)
        })
        .ok_or_else(|| "Java 环境不存在".to_owned())?;
    let path = std::path::Path::new(&home);
    if !path.is_dir() {
        return Err("环境位置不存在或已删除".to_owned());
    }
    std::process::Command::new("explorer.exe")
        .arg(path)
        .spawn()
        .map_err(|error| format!("无法打开环境位置：{error}"))?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .setup(|app| {
            let state_directory = std::env::var_os("MOYUMAX_STATE_DIR")
                .map(PathBuf::from)
                .map_or_else(|| app.path().app_local_data_dir(), Ok)?;
            let database_path = state_directory.join("state.sqlite3");
            let default_data_directory = default_data_directory()?;
            let service = AppService::open(&database_path, &default_data_directory)?;
            let metadata = MetadataClient::new()?;
            let modrinth = ModrinthClient::new()?;
            let smoke_enabled = std::env::var_os("MOYUMAX_SMOKE").is_some();
            let shell = Arc::new(ShellCoordinator::new(smoke_enabled, &state_directory));
            let tasks_paused = service.tasks_paused()?;
            let speed_limit = service.download_speed_limit()?;
            moyumax_core::global_rate_limiter().set_rate(speed_limit);
            let download_concurrency = service.download_concurrency()?;
            let coordinator = TaskCoordinator::new(!tasks_paused, download_concurrency)
                .map_err(std::io::Error::other)?;
            for task_id in service.queued_install_tasks_by_priority()? {
                coordinator.submit_install(service.clone(), task_id);
            }
            for task_id in service.queued_content_tasks_by_priority()? {
                coordinator.submit_content(service.clone(), task_id);
            }
            app.manage(service);
            app.manage(metadata);
            app.manage(modrinth);
            app.manage(InstallPreviewStore::default());
            app.manage(ContentPreviewStore::default());
            app.manage(ModpackPreviewStore::default());
            app.manage(DiagnosticPreviewStore::default());
            app.manage(MicrosoftLoginState::default());
            app.manage(NetplayCoordinator::default());
            app.manage(ModpackInstallCoordinator::default());
            app.manage(coordinator);
            app.manage(LaunchCoordinator::default());
            app.manage(Arc::clone(&shell));
            tray::setup_tray(app.handle(), Arc::clone(&shell))?;
            spawn_idle_destroy_task(app.handle().clone(), Arc::clone(&shell));
            shell.trace("window_shown");
            if smoke_enabled {
                spawn_smoke_driver(app.handle().clone(), shell);
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            let shell = window.app_handle().state::<Arc<ShellCoordinator>>();
            match event {
                tauri::WindowEvent::CloseRequested { api, .. } => {
                    if shell.is_exiting() || shell.is_minimizing() {
                        return;
                    }
                    api.prevent_close();
                    let _ = window.emit(CLOSE_REQUESTED_EVENT, ());
                }
                tauri::WindowEvent::Destroyed => {
                    shell.on_window_destroyed();
                }
                _ => {}
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_bootstrap_state,
            complete_onboarding,
            skip_onboarding,
            get_game_version_catalog,
            get_fabric_loaders,
            get_quilt_loaders,
            get_forge_versions,
            get_neoforge_versions,
            preview_install,
            confirm_install_preview,
            get_install_tasks,
            resolve_install_task_recovery,
            retry_install_task,
            search_modrinth_mods,
            preview_modrinth_install,
            confirm_content_preview,
            get_content_install_tasks,
            get_installed_content,
            set_installed_content_enabled,
            get_instance_launch_options,
            set_instance_launch_options,
            clear_instance_launch_options,
            get_global_launch_preference,
            set_global_launch_preference,
            get_auto_launch_options,
            check_content_updates,
            plan_content_update,
            get_instance_content_auto_update,
            set_instance_content_auto_update,
            list_instance_worlds,
            list_instance_resources,
            import_instance_resource,
            set_instance_resource_enabled,
            list_instance_world_details,
            export_instance_world,
            export_instance_modpack,
            import_instance_world,
            rollback_world_backup,
            create_manual_world_backup,
            delete_world_backup,
            list_instance_screenshots,
            read_instance_screenshot,
            open_screenshot_location,
            delete_instance_screenshot,
            delete_instance_resource,
            delete_instance_world,
            list_instance_servers,
            add_instance_server,
            remove_instance_server,
            update_instance_server,
            ping_minecraft_server,
            restore_recycled_entry,
            get_world_backup_settings,
            set_world_backup_interval_minutes,
            set_world_backup_keep_count,
            list_accounts,
            add_offline_account,
            add_authlib_account,
            set_default_account,
            remove_account,
            refresh_account_session,
            start_microsoft_device_login,
            cancel_microsoft_device_login,
            open_external_url,
            start_netplay_room,
            stop_netplay_room,
            get_netplay_status,
            list_netplay_peers,
            set_netplay_forward,
            detect_nat_type,
            get_ui_preferences,
            set_ui_theme,
            set_ui_language,
            set_ui_motion,
            set_ui_contrast,
            get_cli_enabled,
            set_cli_enabled,
            get_update_checks_enabled,
            set_update_checks_enabled,
            check_for_updates,
            download_update_installer,
            open_update_location,
            get_ui_background,
            set_ui_background,
            import_background_image,
            import_theme_pack,
            read_background_image,
            import_modpack_preview,
            install_modpack,
            update_modpack,
            is_modpack_installing,
            get_instance_modpack,
            preview_online_modpack,
            install_online_resource,
            list_modrinth_versions,
            download_modrinth_file,
            retry_content_task,
            resolve_content_task_recovery,
            list_instances,
            list_recycle_bin_items,
            recycle_instance,
            restore_recycle_bin_item,
            purge_recycle_bin_item,
            list_world_backups,
            start_instance,
            stop_instance,
            list_launch_sessions,
            read_launch_log,
            open_launch_log_location,
            list_crash_reports,
            preview_diagnostic_export,
            confirm_diagnostic_export,
            get_window_close_behavior,
            set_window_close_behavior,
            resolve_window_close,
            get_exit_impact,
            confirm_exit,
            force_exit,
            get_shell_state,
            persist_shell_state,
            get_window_startup_kind,
            take_pending_intent,
            list_java_environments,
            list_deleted_java_environments,
            delete_java_environment,
            verify_java_environment,
            restore_java_environment,
            set_instance_java_environment,
            open_java_location,
            get_download_source_policy,
            set_download_source_policy,
            get_tasks_paused,
            pause_all_tasks,
            resume_all_tasks,
            pause_task,
            resume_task,
            set_task_priority,
            cancel_install_task,
            cancel_content_task,
            delete_install_task,
            delete_content_task,
            get_download_concurrency,
            set_download_concurrency,
            get_download_speed_limit,
            set_download_speed_limit,
            get_proxy_preference,
            set_proxy_preference
        ])
        .build(tauri::generate_context!())
        .expect("MoyuMax desktop runtime failed")
        .run(|app, event| {
            if let tauri::RunEvent::ExitRequested { api, .. } = event {
                let shell = app.state::<Arc<ShellCoordinator>>();
                if !shell.is_exiting() {
                    // 窗口销毁后进程保持托盘常驻,只有显式退出才结束。
                    api.prevent_exit();
                }
            }
        });
}

fn build_install_preview(id: String, request: &ResolvedInstallRequest) -> InstallPreview {
    let (loader_name, loader_version) = match &request.loader {
        ResolvedLoader::Vanilla => ("原版".to_owned(), None),
        ResolvedLoader::Fabric { version, .. } => ("Fabric".to_owned(), Some(version.clone())),
        ResolvedLoader::Quilt { version, .. } => ("Quilt".to_owned(), Some(version.clone())),
        ResolvedLoader::Forge { version, .. } => ("Forge".to_owned(), Some(version.clone())),
        ResolvedLoader::NeoForge { version, .. } => ("NeoForge".to_owned(), Some(version.clone())),
    };
    let game_bytes = request
        .game
        .artifacts
        .iter()
        .fold(0_u64, |total, artifact| total.saturating_add(artifact.size))
        .saturating_add(request.game.asset_objects_total_bytes);
    InstallPreview {
        id,
        instance_name: request.instance_name.clone(),
        game_version: request.game.version.id.clone(),
        loader_name,
        loader_version,
        java_distribution: request.java.distribution,
        java_version: request.java.full_version.clone(),
        java_architecture: request.java.architecture,
        isolation: request.isolation,
        estimated_download_bytes: game_bytes.saturating_add(request.java.artifact.size),
    }
}

fn default_data_directory() -> Result<PathBuf, std::io::Error> {
    if let Some(configured) = std::env::var_os("MOYUMAX_DATA_DIR") {
        return Ok(PathBuf::from(configured));
    }

    let executable = std::env::current_exe()?;
    let executable_directory = executable
        .parent()
        .ok_or_else(|| std::io::Error::other("executable directory is unavailable"))?;
    Ok(executable_directory.join("data"))
}

#[cfg(test)]
mod tests {
    use super::{LaunchCoordinator, TaskCoordinator};

    #[tokio::test]
    async fn install_and_content_tasks_share_one_background_slot() {
        let coordinator = TaskCoordinator::new(true, 4).unwrap();
        assert_eq!(coordinator.task_permits.available_permits(), 1);

        let permit = coordinator
            .task_permits
            .clone()
            .acquire_owned()
            .await
            .unwrap();
        assert_eq!(coordinator.task_permits.available_permits(), 0);

        drop(permit);
        assert_eq!(coordinator.task_permits.available_permits(), 1);
    }

    #[test]
    fn paused_coordinator_keeps_scheduling_disabled() {
        let coordinator = TaskCoordinator::new(false, 4).unwrap();
        assert!(!coordinator.scheduling_enabled());
        coordinator.interrupt_running_for_exit();
        assert!(!coordinator.scheduling_enabled());
    }

    #[tokio::test]
    async fn launch_coordinator_routes_one_explicit_stop_request() {
        let coordinator = LaunchCoordinator::default();
        let (sender, receiver) = tokio::sync::oneshot::channel();
        coordinator
            .register_stop_request("instance-id", sender)
            .unwrap();

        coordinator.request_stop("instance-id").unwrap();

        assert!(receiver.await.is_ok());
        assert!(coordinator.request_stop("instance-id").is_err());
    }
}
