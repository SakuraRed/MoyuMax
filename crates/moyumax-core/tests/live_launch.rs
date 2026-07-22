use std::path::Path;

use moyumax_core::{
    AppService, LaunchAccount, LaunchOptions, LaunchSessionState, run_launch_execution,
};

#[tokio::test]
#[ignore = "启动真实 Minecraft 客户端并在确认主类与 native 初始化后主动停止"]
async fn installed_recommended_fabric_enters_minecraft_and_extracts_natives() {
    let root = std::env::var_os("MOYUMAX_LIVE_INSTALL_ROOT")
        .map(std::path::PathBuf::from)
        .expect("MOYUMAX_LIVE_INSTALL_ROOT 必须指向保留的真实安装目录");
    let data = root.join("data");
    let service = AppService::open(&root.join("state.sqlite3"), &data).unwrap();
    let instance = service
        .list_instances()
        .unwrap()
        .into_iter()
        .find(|instance| instance.state == "ready")
        .expect("真实安装目录应包含 ready 实例");
    let execution = service
        .create_launch_execution(
            &instance.id,
            &LaunchAccount::offline("MoyuMaxPlayer").unwrap(),
            &LaunchOptions {
                minimum_memory_mib: 512,
                maximum_memory_mib: 2_048,
            },
        )
        .unwrap();
    let stdout_path = std::path::PathBuf::from(&execution.session().stdout_path);
    let stderr_path = std::path::PathBuf::from(&execution.session().stderr_path);
    let natives = Path::new(&instance.root_directory).join("natives");
    let (stop_sender, stop_receiver) = tokio::sync::oneshot::channel();
    let monitor = tokio::spawn(async move {
        for _ in 0..180 {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            let stdout = tokio::fs::read_to_string(&stdout_path)
                .await
                .unwrap_or_default();
            let stderr = tokio::fs::read_to_string(&stderr_path)
                .await
                .unwrap_or_default();
            let output = format!("{stdout}\n{stderr}");
            let entered_minecraft = ["Loading Minecraft", "Fabric Loader", "LWJGL Version"]
                .iter()
                .any(|marker| output.contains(marker));
            if entered_minecraft && contains_dll(&natives) {
                let _ = stop_sender.send(());
                return true;
            }
        }
        let _ = stop_sender.send(());
        false
    });

    let completed = run_launch_execution(&service, execution, stop_receiver)
        .await
        .unwrap();
    let reached_runtime = monitor.await.unwrap();
    let stdout = std::fs::read_to_string(&completed.stdout_path).unwrap_or_default();
    let stderr = std::fs::read_to_string(&completed.stderr_path).unwrap_or_default();
    assert!(
        reached_runtime,
        "90 秒内未进入 Minecraft/native 初始化\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert_eq!(completed.state, LaunchSessionState::Stopped);
    assert!(contains_dll(
        Path::new(&instance.root_directory)
            .join("natives")
            .as_path()
    ));
}

fn contains_dll(directory: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return false;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() && contains_dll(&path) {
            return true;
        }
        if path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("dll"))
        {
            return true;
        }
    }
    false
}
