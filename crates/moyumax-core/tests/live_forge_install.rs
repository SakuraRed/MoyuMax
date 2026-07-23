use moyumax_core::{
    AppService, InstallExecutor, InstallSelection, InstanceIsolation, LoaderChoice, MetadataClient,
};
use tempfile::TempDir;

/// 真实 Forge 安装探针：联网下载真实元数据、安装器与处理器依赖,
/// 用真实托管 Java 执行真实处理器链。默认 ignored,发布候选前手动执行:
/// `cargo test -p moyumax-core --test live_forge_install -- --ignored --nocapture`
#[tokio::test]
#[ignore = "真实网络与真实处理器执行,发布候选前手动验证"]
async fn m12_live_forge_install_produces_ready_instance() {
    let directory = TempDir::new().unwrap();
    let service = AppService::open(
        &directory.path().join("state.sqlite3"),
        &directory.path().join("data"),
    )
    .unwrap();
    service.skip_onboarding().unwrap();
    let metadata = MetadataClient::new().unwrap();
    let catalog = metadata.fetch_version_catalog().await.unwrap();
    service.store_version_catalog(&catalog).unwrap();
    let version = catalog
        .versions
        .iter()
        .find(|version| version.recommended)
        .expect("目录应有推荐版本");
    let forge_versions = metadata
        .compatible_forge_versions(&version.id)
        .await
        .unwrap();
    let forge = forge_versions
        .iter()
        .find(|entry| entry.recommended)
        .expect("应有推荐 Forge 构建");
    let request = metadata
        .resolve_install_request(&InstallSelection {
            instance_name: "Forge 真实探针".to_owned(),
            game_version: version.clone(),
            loader: LoaderChoice::Forge {
                version: forge.version.clone(),
            },
            isolation: InstanceIsolation::Full,
        })
        .await
        .unwrap();
    let task = service.enqueue_install_task(&request).unwrap();

    let instance = InstallExecutor::new(4)
        .unwrap()
        .execute_task(&service, &task.id)
        .await
        .expect("真实 Forge 安装应完成");

    assert_eq!(instance.state, "ready");
    assert_eq!(instance.loader_kind, "forge");
    let runtime: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(
            std::path::Path::new(&instance.root_directory).join(".moyumax/runtime.json"),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        runtime["mainClass"],
        "net.minecraftforge.bootstrap.ForgeBootstrap"
    );
    let classpath = runtime["classpath"].as_array().unwrap();
    assert!(
        classpath
            .iter()
            .any(|entry| entry.as_str().unwrap().contains("client.jar")),
        "PATCHED 客户端 JAR 应在 classpath: {classpath:?}"
    );
}
