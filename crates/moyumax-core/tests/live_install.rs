use std::path::Path;

use moyumax_core::{
    AppService, InstallExecutor, InstallSelection, InstanceIsolation, LoaderChoice, MetadataClient,
    TaskState,
};

#[tokio::test]
#[ignore = "下载并校验当前 Minecraft、Fabric 与 Azul JDK，耗时且需要网络"]
async fn current_recommended_fabric_installs_to_a_ready_isolated_instance() {
    let parent = std::env::var_os("MOYUMAX_LIVE_TEST_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    std::fs::create_dir_all(&parent).unwrap();
    let directory = tempfile::TempDir::new_in(parent).unwrap();
    let database = directory.path().join("state.sqlite3");
    let data = directory.path().join("data");
    let service = AppService::open(&database, &data).unwrap();
    service.skip_onboarding().unwrap();

    let metadata = MetadataClient::new().unwrap();
    let catalog = metadata.fetch_version_catalog().await.unwrap();
    service.store_version_catalog(&catalog).unwrap();
    let game_version = catalog
        .versions
        .iter()
        .find(|version| version.recommended)
        .unwrap()
        .clone();
    let loader = metadata
        .compatible_fabric_loaders(&game_version.id)
        .await
        .unwrap()
        .into_iter()
        .find(|loader| loader.recommended)
        .unwrap();
    let request = metadata
        .resolve_install_request(&InstallSelection {
            instance_name: format!("{} Fabric 实机安装", game_version.id),
            game_version,
            loader: LoaderChoice::Fabric {
                version: loader.version,
            },
            isolation: InstanceIsolation::Full,
        })
        .await
        .unwrap();
    let task = service.enqueue_install_task(&request).unwrap();

    let result = InstallExecutor::new(8)
        .unwrap()
        .execute_task(&service, &task.id)
        .await;
    let instance = match result {
        Ok(instance) => instance,
        Err(error) => {
            let retained = directory.keep();
            panic!(
                "真实安装失败：{error}\n诊断数据已保留在 {}",
                retained.display()
            );
        }
    };

    assert_eq!(instance.state, "ready");
    assert!(
        Path::new(&instance.root_directory)
            .join(".moyumax/runtime.json")
            .is_file()
    );
    let runtime: serde_json::Value = serde_json::from_slice(
        &std::fs::read(Path::new(&instance.root_directory).join(".moyumax/runtime.json")).unwrap(),
    )
    .unwrap();
    let classpath = runtime["classpath"].as_array().unwrap();
    assert!(classpath.iter().any(|entry| {
        entry.as_str().is_some_and(|path| {
            path.ends_with("natives-windows.jar")
                || path.ends_with("natives-windows-x86_64.jar")
                || path.ends_with("natives-windows-amd64.jar")
        })
    }));
    assert!(!classpath.iter().any(|entry| {
        entry.as_str().is_some_and(|path| {
            path.contains("natives-windows-x86") || path.contains("natives-windows-arm64")
        })
    }));
    let java_environment = service
        .list_managed_java()
        .unwrap()
        .iter()
        .find(|environment| {
            environment.status == moyumax_core::JavaEnvironmentStatus::Ready
                && Path::new(&environment.home_directory)
                    .join("bin/java.exe")
                    .is_file()
        })
        .cloned()
        .expect("安装后应存在可执行的托管 Java");
    let java_version = std::process::Command::new(
        Path::new(&java_environment.home_directory).join("bin/java.exe"),
    )
    .arg("-version")
    .output()
    .expect("托管 Java 应能启动");
    assert!(
        java_version.status.success(),
        "托管 Java -version 失败：{}",
        String::from_utf8_lossy(&java_version.stderr)
    );
    assert_eq!(
        service.list_install_tasks().unwrap()[0].state,
        TaskState::Completed
    );
    assert!(!Path::new(&task.staging_directory).exists());
    if let Some(result_file) = std::env::var_os("MOYUMAX_LIVE_RESULT_FILE") {
        let result_file = std::path::PathBuf::from(result_file);
        if let Some(parent) = result_file.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(
            result_file,
            serde_json::to_vec_pretty(&serde_json::json!({
                "instanceId": instance.id,
                "gameVersion": instance.game_version,
                "loaderKind": instance.loader_kind,
                "loaderVersion": instance.loader_version,
                "javaVersion": java_environment.full_version,
                "state": instance.state,
                "taskState": "completed",
                "stagingCleaned": true
            }))
            .unwrap(),
        )
        .unwrap();
    }
    if std::env::var_os("MOYUMAX_LIVE_KEEP").is_some() {
        let retained = directory.keep();
        println!("真实安装成功并保留在 {}", retained.display());
    }
}
