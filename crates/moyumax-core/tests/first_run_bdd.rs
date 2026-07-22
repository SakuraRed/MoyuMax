use std::path::Path;

use moyumax_core::{AppService, Language, OnboardingSelection};
use tempfile::tempdir;

#[test]
fn m1_first_run_001_new_database_requires_onboarding() {
    let fixture = tempdir().expect("create test directory");
    let database = fixture.path().join("state.sqlite3");
    let default_data = fixture.path().join("data");
    let service = AppService::open(&database, &default_data).expect("open state store");

    let state = service.bootstrap_state().expect("read bootstrap state");

    assert!(state.requires_onboarding);
    assert_eq!(state.default_data_directory, path_text(&default_data));
    assert!(!state.defaults.telemetry_enabled);
    assert!(state.defaults.update_checks_enabled);
    assert!(!state.defaults.nat_detection_enabled);
    assert!(state.defaults.instance_isolation_enabled);
}

#[test]
fn m1_first_run_002_completion_persists_across_reopen() {
    let fixture = tempdir().expect("create test directory");
    let database = fixture.path().join("state.sqlite3");
    let default_data = fixture.path().join("data");
    let custom_data = fixture.path().join("minecraft-data");
    let service = AppService::open(&database, &default_data).expect("open state store");
    let selection = OnboardingSelection {
        language: Language::En,
        data_directory: path_text(&custom_data),
        telemetry_enabled: false,
        update_checks_enabled: true,
        nat_detection_enabled: false,
        instance_isolation_enabled: true,
    };

    service
        .complete_onboarding(&selection)
        .expect("complete onboarding");
    drop(service);

    let reopened = AppService::open(&database, &default_data).expect("reopen state store");
    let state = reopened.bootstrap_state().expect("read persisted state");
    assert!(!state.requires_onboarding);
    assert_eq!(state.settings, Some(selection));
}

#[test]
fn m1_first_run_003_unc_path_is_rejected_without_partial_completion() {
    let fixture = tempdir().expect("create test directory");
    let database = fixture.path().join("state.sqlite3");
    let default_data = fixture.path().join("data");
    let service = AppService::open(&database, &default_data).expect("open state store");
    let selection = OnboardingSelection {
        data_directory: r"\\server\share\MoyuMax".to_owned(),
        ..OnboardingSelection::recommended(path_text(&default_data))
    };

    let error = service
        .complete_onboarding(&selection)
        .expect_err("UNC path must be rejected");

    assert!(error.to_string().contains("SMB"));
    assert!(
        service
            .bootstrap_state()
            .expect("read state after rejection")
            .requires_onboarding
    );
}

#[test]
fn m1_first_run_004_skip_uses_safe_defaults() {
    let fixture = tempdir().expect("create test directory");
    let database = fixture.path().join("state.sqlite3");
    let default_data = fixture.path().join("data");
    let service = AppService::open(&database, &default_data).expect("open state store");

    service.skip_onboarding().expect("skip onboarding");

    let state = service.bootstrap_state().expect("read skipped state");
    let settings = state.settings.expect("settings persisted");
    assert!(!state.requires_onboarding);
    assert_eq!(settings.language, Language::ZhCn);
    assert_eq!(settings.data_directory, path_text(&default_data));
    assert!(!settings.telemetry_enabled);
    assert!(!settings.nat_detection_enabled);
    assert!(settings.instance_isolation_enabled);
}

fn path_text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}
