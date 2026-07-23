use serde_json::Value;
use std::{fs, path::PathBuf};

#[test]
fn windows_preview_build_is_a_current_user_nsis_installer() {
    let config_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tauri.conf.json");
    let config: Value = serde_json::from_slice(
        &fs::read(&config_path).expect("tauri.conf.json should be readable"),
    )
    .expect("tauri.conf.json should contain valid JSON");

    assert_eq!(config["bundle"]["active"], true);
    assert_eq!(config["bundle"]["targets"], serde_json::json!(["nsis"]));
    assert_eq!(
        config["bundle"]["windows"]["nsis"]["installMode"],
        "currentUser"
    );
    assert!(
        config["version"]
            .as_str()
            .is_some_and(|version| version.contains("preview")),
        "unsigned review builds must remain visibly marked as preview"
    );
}
