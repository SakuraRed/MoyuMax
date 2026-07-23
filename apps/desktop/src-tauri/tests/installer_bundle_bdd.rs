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

    let icons = config["bundle"]["icon"]
        .as_array()
        .expect("bundle icons must be declared");
    assert!(
        icons
            .iter()
            .any(|icon| icon.as_str().is_some_and(|path| path.ends_with("icon.ico"))),
        "正式 ICO 必须进入打包"
    );
    for icon in icons {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(icon.as_str().unwrap());
        assert!(path.is_file(), "打包图标缺失: {}", path.display());
    }
    let ico = fs::read(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("icons/icon.ico"))
        .expect("icon.ico should be readable");
    assert!(
        ico.len() > 6 && u16::from_le_bytes([ico[4], ico[5]]) >= 4,
        "icon.ico 应包含至少 4 个尺寸"
    );
    let tray = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("icons/tray-icon.png");
    assert!(tray.is_file(), "托盘正式图标缺失");

    let delete_data = config["bundle"]["windows"]["nsis"]["deleteAppDataOnUninstall"]
        .as_bool()
        .unwrap_or(false);
    assert!(
        !delete_data,
        "卸载不得删除应用数据（实例、存档、备份、账户与 JDK 默认保留）"
    );
}
