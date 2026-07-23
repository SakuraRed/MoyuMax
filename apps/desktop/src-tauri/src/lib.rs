use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use moyumax_core::{
    AppService, ArtifactDownloader, BootstrapState, ContentExecutor, ContentInstallPlan,
    ContentInstallTask, CrashReportSummary, DiagnosticExportPreview, DiagnosticExportResult,
    DownloadInterrupt, ExitImpactSummary, FabricLoaderSummary, InstallExecutor, InstallSelection,
    InstallTask, InstalledContent, InstanceIsolation, JavaArchitecture, JavaDeleteOutcome,
    JavaDistribution, JavaEnvironmentSummary, LaunchAccount, LaunchExecution, LaunchOptions,
    LaunchSessionSummary, ManagedInstanceSummary, MetadataClient, ModrinthClient,
    ModrinthSearchPage, ModrinthSearchQuery, OnboardingSelection, RecoveryDecision, RecycleBinItem,
    RecyclePurgeResult, ResolvedInstallRequest, ResolvedLoader, ShellState, SourcePolicy,
    VersionCatalog, WindowCloseBehavior, WorldBackupSummary, run_launch_execution,
};
use serde::Serialize;
use tauri::{Emitter, Manager, State};
use tokio::sync::{Semaphore, oneshot};
use uuid::Uuid;

mod lifecycle;
mod tray;

use lifecycle::{
    CLOSE_REQUESTED_EVENT, PendingIntent, ShellCoordinator, WindowStartupKind,
    confirm_graceful_exit, minimize_to_tray, spawn_idle_destroy_task, spawn_smoke_driver,
};

#[derive(Debug, Default)]
struct InstallPreviewStore {
    requests: Mutex<HashMap<String, ResolvedInstallRequest>>,
}

#[derive(Debug, Default)]
struct ContentPreviewStore {
    plans: Mutex<HashMap<String, ContentInstallPlan>>,
}

#[derive(Debug, Default)]
struct DiagnosticPreviewStore {
    reports: Mutex<HashMap<String, String>>,
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
    fn new(scheduling_enabled: bool) -> Result<Self, String> {
        Ok(Self {
            install_executor: InstallExecutor::new(4).map_err(|error| error.to_string())?,
            content_executor: ContentExecutor::new(4).map_err(|error| error.to_string())?,
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
            .ok_or_else(|| format!("无法连接官方版本服务，且本地没有可用缓存：{network_error}")),
    }
}

#[tauri::command]
async fn get_fabric_loaders(
    metadata: State<'_, MetadataClient>,
    game_version: String,
) -> Result<Vec<FabricLoaderSummary>, String> {
    metadata
        .compatible_fabric_loaders(&game_version)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn get_quilt_loaders(
    metadata: State<'_, MetadataClient>,
    game_version: String,
) -> Result<Vec<FabricLoaderSummary>, String> {
    metadata
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
    instance_id: String,
) -> Result<LaunchSessionSummary, String> {
    let service = service.inner().clone();
    let coordinator = coordinator.inner().clone();
    let account = LaunchAccount::offline("MoyuMaxPlayer").map_err(|error| error.to_string())?;
    let preparation_service = service.clone();
    let execution = tauri::async_runtime::spawn_blocking(move || {
        preparation_service.create_launch_execution(
            &instance_id,
            &account,
            &LaunchOptions {
                minimum_memory_mib: 512,
                maximum_memory_mib: 2_048,
            },
        )
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
            let coordinator = TaskCoordinator::new(!tasks_paused).map_err(std::io::Error::other)?;
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
            app.manage(DiagnosticPreviewStore::default());
            app.manage(coordinator);
            app.manage(LaunchCoordinator::default());
            app.manage(Arc::clone(&shell));
            tray::setup_tray(app.handle(), Arc::clone(&shell))?;
            spawn_idle_destroy_task(app.handle().clone(), Arc::clone(&shell));
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
            get_download_speed_limit,
            set_download_speed_limit
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
        let coordinator = TaskCoordinator::new(true).unwrap();
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
        let coordinator = TaskCoordinator::new(false).unwrap();
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
