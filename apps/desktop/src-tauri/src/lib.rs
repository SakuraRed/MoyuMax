use std::path::PathBuf;

use moyumax_core::{AppService, BootstrapState, OnboardingSelection};
use tauri::{Manager, State};

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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let state_directory = app.path().app_local_data_dir()?;
            let database_path = state_directory.join("state.sqlite3");
            let default_data_directory = default_data_directory()?;
            let service = AppService::open(&database_path, &default_data_directory)?;
            app.manage(service);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_bootstrap_state,
            complete_onboarding,
            skip_onboarding
        ])
        .run(tauri::generate_context!())
        .expect("MoyuMax desktop runtime failed");
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
