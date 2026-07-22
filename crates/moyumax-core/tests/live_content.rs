use std::{fs, path::Path, time::Duration};

use moyumax_core::{
    AppService, ContentExecutor, LaunchAccount, LaunchOptions, LaunchSessionState, ModrinthClient,
    ModrinthSearchIndex, ModrinthSearchQuery, OnboardingSelection, run_launch_execution,
};
use rusqlite::{Connection, params};
use tempfile::TempDir;

const CONTINUITY_PROJECT_ID: &str = "1IjD5062";
const FABRIC_API_PROJECT_ID: &str = "P7dR8mSH";

#[tokio::test]
#[ignore = "复制保留实例，联网解析并安装 Continuity 与 Fabric API，再真实启动 Minecraft"]
async fn current_continuity_dependency_closure_installs_and_launches_from_a_clone() {
    let baseline = std::env::var_os("MOYUMAX_LIVE_INSTALL_ROOT")
        .map(std::path::PathBuf::from)
        .expect("MOYUMAX_LIVE_INSTALL_ROOT 必须指向保留的真实安装目录");
    let baseline_instance = only_directory(&baseline.join("data/instances"));
    let fixture = TempDir::new().unwrap();
    let database_path = fixture.path().join("state.sqlite3");
    let data_directory = fixture.path().join("data");
    let instance_id = baseline_instance
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let cloned_instance = data_directory.join("instances").join(&instance_id);
    fs::create_dir_all(cloned_instance.parent().unwrap()).unwrap();
    fs::copy(baseline.join("state.sqlite3"), &database_path).unwrap();
    copy_directory(&baseline_instance, &cloned_instance);

    let service = AppService::open(&database_path, &data_directory).unwrap();
    service
        .complete_onboarding(&OnboardingSelection::recommended(
            data_directory.to_string_lossy().into_owned(),
        ))
        .unwrap();
    Connection::open(&database_path)
        .unwrap()
        .execute(
            "UPDATE instances SET root_directory = ?2 WHERE id = ?1",
            params![instance_id, cloned_instance.to_string_lossy()],
        )
        .unwrap();
    let instance = service
        .list_instances()
        .unwrap()
        .into_iter()
        .find(|instance| instance.id == instance_id)
        .unwrap();
    assert_eq!(instance.game_version, "26.2");
    assert_eq!(instance.loader_kind, "fabric");
    assert!(
        fs::read_dir(cloned_instance.join(".minecraft/mods"))
            .unwrap()
            .next()
            .is_none(),
        "真实内容验收必须从不含模组的实例副本开始"
    );

    let modrinth = ModrinthClient::new().unwrap();
    let search = modrinth
        .search_mods(&ModrinthSearchQuery {
            query: "Continuity".to_owned(),
            game_version: instance.game_version.clone(),
            loader: instance.loader_kind.clone(),
            index: ModrinthSearchIndex::Relevance,
            offset: 0,
            limit: 20,
        })
        .await
        .unwrap();
    assert!(
        search
            .hits
            .iter()
            .any(|project| project.project_id == CONTINUITY_PROJECT_ID)
    );
    let plan = modrinth
        .resolve_mod_install_plan(&instance, CONTINUITY_PROJECT_ID, &[])
        .await
        .unwrap();
    assert!(
        plan.entries
            .iter()
            .any(|entry| entry.project_id == CONTINUITY_PROJECT_ID)
    );
    assert!(
        plan.entries
            .iter()
            .any(|entry| entry.project_id == FABRIC_API_PROJECT_ID)
    );
    assert!(plan.entries.iter().all(|entry| {
        entry.file.sha1.len() == 40 && entry.file.sha512.len() == 128 && entry.file.size > 0
    }));

    let task = service.enqueue_content_install_task(&plan).unwrap();
    let installed = ContentExecutor::new(4)
        .unwrap()
        .execute_task(&service, &task.id)
        .await
        .unwrap();
    assert_eq!(installed.len(), plan.entries.len());
    assert!(!Path::new(&task.staging_directory).exists());
    for entry in &installed {
        assert!(cloned_instance.join(&entry.relative_path).is_file());
    }

    service
        .complete_onboarding(&OnboardingSelection::recommended(
            baseline.join("data").to_string_lossy().into_owned(),
        ))
        .unwrap();

    let execution = service
        .create_launch_execution(
            &instance_id,
            &LaunchAccount::offline("MoyuMaxPlayer").unwrap(),
            &LaunchOptions {
                minimum_memory_mib: 512,
                maximum_memory_mib: 2_048,
            },
        )
        .unwrap();
    let stdout_path = std::path::PathBuf::from(&execution.session().stdout_path);
    let stderr_path = std::path::PathBuf::from(&execution.session().stderr_path);
    let (stop_sender, stop_receiver) = tokio::sync::oneshot::channel();
    let monitor = tokio::spawn(async move {
        for _ in 0..180 {
            tokio::time::sleep(Duration::from_millis(500)).await;
            let stdout = tokio::fs::read_to_string(&stdout_path)
                .await
                .unwrap_or_default();
            let stderr = tokio::fs::read_to_string(&stderr_path)
                .await
                .unwrap_or_default();
            let output = format!("{stdout}\n{stderr}").to_ascii_lowercase();
            if output.contains("continuity") && output.contains("fabric-api") {
                let _ = stop_sender.send(());
                return true;
            }
        }
        let _ = stop_sender.send(());
        false
    });
    let session = run_launch_execution(&service, execution, stop_receiver)
        .await
        .unwrap();
    let discovered = monitor.await.unwrap();
    let stdout = fs::read_to_string(&session.stdout_path).unwrap_or_default();
    let stderr = fs::read_to_string(&session.stderr_path).unwrap_or_default();
    assert!(
        discovered,
        "90 秒内 Fabric Loader 未同时发现 Continuity 与 Fabric API\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert_eq!(session.state, LaunchSessionState::Stopped);

    assert!(
        !baseline_instance
            .join(".minecraft/mods/continuity.jar")
            .exists()
    );
    assert!(
        fs::read_dir(baseline_instance.join(".minecraft/mods"))
            .unwrap()
            .next()
            .is_none(),
        "保留的里程碑 4 实例不得被真实内容测试污染"
    );
}

fn only_directory(parent: &Path) -> std::path::PathBuf {
    let directories = fs::read_dir(parent)
        .unwrap()
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.is_dir())
        .collect::<Vec<_>>();
    assert_eq!(directories.len(), 1, "保留基线必须只有一个实例目录");
    directories.into_iter().next().unwrap()
}

fn copy_directory(source: &Path, destination: &Path) {
    fs::create_dir_all(destination).unwrap();
    for entry in fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        if source_path.is_dir() {
            copy_directory(&source_path, &destination_path);
        } else {
            fs::copy(source_path, destination_path).unwrap();
        }
    }
}
