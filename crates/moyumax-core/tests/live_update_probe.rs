//! 实机验证自动更新渠道:以预览版身份检查 GitHub 最新发行版,
//! 必须检出更新且携带安装器;以当前版本身份检查必须返回无更新;
//! 安装器下载须通过 GitHub 提供的 SHA-256 与大小校验。

use moyumax_core::UpdateClient;

#[tokio::test]
#[ignore = "访问真实 GitHub Releases,验证自动更新渠道端到端有效"]
async fn live_update_channel_detects_v010_for_preview_installs() {
    let client = UpdateClient::new().unwrap();
    let info = client
        .check_latest("0.1.0-preview.1")
        .await
        .unwrap()
        .expect("预览版必须能检测到 v0.1.0 更新");
    println!(
        "检测到更新:{} {} 资产数 {:?}",
        info.tag,
        info.name,
        info.installer.as_ref().map(|a| a.name.clone())
    );
    let installer = info.installer.expect("发行版必须携带 -setup.exe 安装器");
    assert!(installer.name.ends_with("-setup.exe"));
    assert!(installer.size > 1_000_000);
    assert!(
        installer
            .sha256
            .as_deref()
            .is_some_and(|digest| digest.len() == 64),
        "GitHub 资产应提供 SHA-256 摘要:{:?}",
        installer.sha256
    );

    let directory = std::env::temp_dir().join("moyumax-update-probe");
    let path = client
        .download_installer(&installer, &directory)
        .await
        .expect("安装器下载与校验必须成功");
    assert!(path.is_file());
    println!("安装器校验通过:{}", path.display());
    let _ = std::fs::remove_dir_all(&directory);

    let current = client.check_latest("0.1.0").await.unwrap();
    assert!(current.is_none(), "当前版本已是最新,不应再提示更新");
}
