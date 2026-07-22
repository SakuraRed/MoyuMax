use std::{collections::HashMap, path::PathBuf, sync::Mutex};

use moyumax_core::{
    AppService, BootstrapState, FabricLoaderSummary, InstallSelection, InstallTask,
    InstanceIsolation, JavaArchitecture, JavaDistribution, MetadataClient, OnboardingSelection,
    RecoveryDecision, ResolvedInstallRequest, ResolvedLoader, VersionCatalog,
};
use serde::Serialize;
use tauri::{Manager, State};
use uuid::Uuid;

#[derive(Debug, Default)]
struct InstallPreviewStore {
    requests: Mutex<HashMap<String, ResolvedInstallRequest>>,
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
    preview_id: String,
) -> Result<InstallTask, String> {
    let request = previews
        .requests
        .lock()
        .map_err(|_| "安装预览状态锁已损坏，请重启 MoyuMax".to_owned())?
        .remove(&preview_id)
        .ok_or_else(|| "安装预览已失效，请返回重新确认".to_owned())?;
    service
        .enqueue_install_task(&request)
        .map_err(|error| error.to_string())
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
    task_id: String,
    decision: RecoveryDecision,
) -> Result<(), String> {
    service
        .resolve_install_task_recovery(&task_id, decision)
        .map_err(|error| error.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let state_directory = app.path().app_local_data_dir()?;
            let database_path = state_directory.join("state.sqlite3");
            let default_data_directory = default_data_directory()?;
            let service = AppService::open(&database_path, &default_data_directory)?;
            let metadata = MetadataClient::new()?;
            app.manage(service);
            app.manage(metadata);
            app.manage(InstallPreviewStore::default());
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
            resolve_install_task_recovery
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
