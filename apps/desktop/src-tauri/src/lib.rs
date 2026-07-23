use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use moyumax_core::{
    AppService, BootstrapState, ContentExecutor, ContentInstallPlan, ContentInstallTask,
    CrashReportSummary, DiagnosticExportPreview, DiagnosticExportResult, FabricLoaderSummary,
    InstallExecutor, InstallSelection, InstallTask, InstalledContent, InstanceIsolation,
    JavaArchitecture, JavaDistribution, LaunchAccount, LaunchExecution, LaunchOptions,
    LaunchSessionSummary, ManagedInstanceSummary, MetadataClient, ModrinthClient,
    ModrinthSearchPage, ModrinthSearchQuery, OnboardingSelection, RecoveryDecision, RecycleBinItem,
    RecyclePurgeResult, ResolvedInstallRequest, ResolvedLoader, TaskState, VersionCatalog,
    WorldBackupSummary, run_launch_execution,
};
use serde::Serialize;
use tauri::{Manager, State};
use tokio::sync::{Semaphore, oneshot};
use uuid::Uuid;

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
}

impl TaskCoordinator {
    fn new() -> Result<Self, String> {
        Ok(Self {
            install_executor: InstallExecutor::new(4).map_err(|error| error.to_string())?,
            content_executor: ContentExecutor::new(4).map_err(|error| error.to_string())?,
            task_permits: Arc::new(Semaphore::new(1)),
        })
    }

    fn submit_install(&self, service: AppService, task_id: String) {
        let coordinator = self.clone();
        tauri::async_runtime::spawn(async move {
            let Ok(_permit) = coordinator.task_permits.acquire().await else {
                return;
            };
            let _ = coordinator
                .install_executor
                .execute_task(&service, &task_id)
                .await;
        });
    }

    fn submit_content(&self, service: AppService, task_id: String) {
        let coordinator = self.clone();
        tauri::async_runtime::spawn(async move {
            let Ok(_permit) = coordinator.task_permits.acquire().await else {
                return;
            };
            let _ = coordinator
                .content_executor
                .execute_task(&service, &task_id)
                .await;
        });
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
fn get_bootstrap_state(service: State<'_, AppService>) -> Result<BootstrapState, String> {
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
            let coordinator = TaskCoordinator::new().map_err(std::io::Error::other)?;
            for task in service.list_install_tasks()? {
                if task.state == TaskState::Queued {
                    coordinator.submit_install(service.clone(), task.id);
                }
            }
            for task in service.list_content_install_tasks()? {
                if task.state == TaskState::Queued {
                    coordinator.submit_content(service.clone(), task.id);
                }
            }
            app.manage(service);
            app.manage(metadata);
            app.manage(modrinth);
            app.manage(InstallPreviewStore::default());
            app.manage(ContentPreviewStore::default());
            app.manage(DiagnosticPreviewStore::default());
            app.manage(coordinator);
            app.manage(LaunchCoordinator::default());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_bootstrap_state,
            complete_onboarding,
            skip_onboarding,
            get_game_version_catalog,
            get_fabric_loaders,
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
            confirm_diagnostic_export
        ])
        .run(tauri::generate_context!())
        .expect("MoyuMax desktop runtime failed");
}

fn build_install_preview(id: String, request: &ResolvedInstallRequest) -> InstallPreview {
    let (loader_name, loader_version) = match &request.loader {
        ResolvedLoader::Vanilla => ("原版".to_owned(), None),
        ResolvedLoader::Fabric { version, .. } => ("Fabric".to_owned(), Some(version.clone())),
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
        let coordinator = TaskCoordinator::new().unwrap();
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
