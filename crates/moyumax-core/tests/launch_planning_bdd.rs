use std::{fs, path::Path};

use moyumax_core::{
    AppService, LaunchAccount, LaunchOptions, LaunchSessionState, ManagedInstanceSummary,
    prepare_launch_from_runtime, run_launch_execution,
};
use rusqlite::{Connection, params};
use serde_json::json;
use tempfile::TempDir;

#[test]
fn m4_launch_001_ready_instance_expands_a_complete_isolated_command() {
    let fixture = LaunchFixture::new();
    let account = LaunchAccount::offline("LocalPlayer").unwrap();
    let repeated = LaunchAccount::offline("LocalPlayer").unwrap();

    let prepared = prepare_launch_from_runtime(
        &fixture.instance,
        &fixture.runtime,
        &account,
        &LaunchOptions {
            minimum_memory_mib: 512,
            maximum_memory_mib: 2_048,
        },
    )
    .expect("a complete runtime should produce a launch command");

    assert_eq!(account.player_uuid(), repeated.player_uuid());
    assert_eq!(
        prepared.executable(),
        fixture.java_home.join("bin/java.exe")
    );
    assert_eq!(
        prepared.working_directory(),
        fixture.root.join(".minecraft")
    );
    assert!(
        prepared
            .arguments()
            .iter()
            .all(|argument| !argument.contains("${"))
    );
    assert!(
        prepared
            .arguments()
            .iter()
            .any(|argument| argument == "-Xms512M")
    );
    assert!(
        prepared
            .arguments()
            .iter()
            .any(|argument| argument == "-Xmx2048M")
    );
    assert!(
        prepared
            .arguments()
            .iter()
            .any(|argument| argument == "-XX:+UseZGC")
    );
    assert!(
        prepared
            .arguments()
            .iter()
            .any(|argument| argument == "-DFabricFixture=true")
    );
    assert!(
        !prepared
            .arguments()
            .iter()
            .any(|argument| argument == "--demo")
    );
    let classpath = prepared
        .arguments()
        .iter()
        .find(|argument| {
            argument.contains("example-natives-windows.jar") && argument.contains("26.2.jar")
        })
        .unwrap_or_else(|| panic!("missing classpath argument: {:?}", prepared.arguments()));
    assert!(
        classpath.rfind("26.2.jar") > classpath.rfind("example-natives-windows.jar"),
        "game client should be last: {classpath}"
    );
    assert!(
        prepared
            .arguments()
            .windows(2)
            .any(|pair| pair == ["--username", "LocalPlayer"])
    );
    assert!(
        prepared
            .arguments()
            .iter()
            .any(|argument| argument.contains("client.xml"))
    );
}

#[test]
fn m4_launch_001_windows_x86_64_native_classifier_is_retained() {
    let mut fixture = LaunchFixture::new();
    let native = fixture
        .shared
        .join("minecraft/libraries/example-natives-windows-x86_64.jar");
    fs::write(&native, b"fixture").unwrap();
    fixture.runtime["classpath"][1] =
        json!("minecraft/libraries/example-natives-windows-x86_64.jar");

    let prepared = prepare_launch_from_runtime(
        &fixture.instance,
        &fixture.runtime,
        &LaunchAccount::offline("LocalPlayer").unwrap(),
        &LaunchOptions::default(),
    )
    .expect("Windows x86_64 native classifier should be supported");

    assert!(
        prepared
            .arguments()
            .iter()
            .any(|argument| argument.contains("example-natives-windows-x86_64.jar"))
    );
}

#[test]
fn m4_launch_001_windows_x86_native_classifier_is_rejected() {
    let mut fixture = LaunchFixture::new();
    let native = fixture
        .shared
        .join("minecraft/libraries/example-natives-windows-x86.jar");
    fs::write(&native, b"fixture").unwrap();
    fixture.runtime["classpath"][1] = json!("minecraft/libraries/example-natives-windows-x86.jar");

    let error = prepare_launch_from_runtime(
        &fixture.instance,
        &fixture.runtime,
        &LaunchAccount::offline("LocalPlayer").unwrap(),
        &LaunchOptions::default(),
    )
    .unwrap_err();

    assert!(error.to_string().contains("错误架构原生库"));
}

#[test]
fn m4_launch_002_missing_classpath_file_is_rejected_before_spawn() {
    let fixture = LaunchFixture::new();
    fs::remove_file(fixture.shared.join("minecraft/libraries/example.jar")).unwrap();

    let error = prepare_launch_from_runtime(
        &fixture.instance,
        &fixture.runtime,
        &LaunchAccount::offline("LocalPlayer").unwrap(),
        &LaunchOptions::default(),
    )
    .unwrap_err();

    assert!(error.to_string().contains("example.jar"));
}

#[test]
fn m4_launch_002_missing_java_assets_or_log_configuration_is_specific() {
    for (missing, expected) in [
        ("java", "托管 Java"),
        ("assets", "资源索引"),
        ("logging", "日志配置"),
    ] {
        let fixture = LaunchFixture::new();
        let path = match missing {
            "java" => fixture.java_home.join("bin/java.exe"),
            "assets" => fixture.shared.join("minecraft/assets/indexes/32.json"),
            "logging" => fixture
                .shared
                .join("minecraft/assets/log_configs/client.xml"),
            _ => unreachable!(),
        };
        fs::remove_file(path).unwrap();

        let error = prepare_launch_from_runtime(
            &fixture.instance,
            &fixture.runtime,
            &LaunchAccount::offline("LocalPlayer").unwrap(),
            &LaunchOptions::default(),
        )
        .unwrap_err();

        assert!(
            error.to_string().contains(expected),
            "missing {missing} should mention {expected}: {error}"
        );
    }
}

#[test]
fn m4_launch_005_duplicate_running_instance_is_rejected() {
    let fixture = LaunchFixture::new();
    let service = fixture.register_with_service();
    let account = LaunchAccount::offline("LocalPlayer").unwrap();
    let first = service
        .create_launch_execution(&fixture.instance.id, &account, &LaunchOptions::default())
        .unwrap();

    let error = service
        .create_launch_execution(&fixture.instance.id, &account, &LaunchOptions::default())
        .unwrap_err();

    assert!(error.to_string().contains("已经在运行"));
    assert_eq!(service.list_launch_sessions().unwrap().len(), 1);
    assert_eq!(first.session().state, LaunchSessionState::Starting);
}

#[test]
fn m4_launch_010_runtime_cannot_replace_indexed_managed_java() {
    let mut fixture = LaunchFixture::new();
    let unindexed_java = fixture._directory.path().join("unindexed-java");
    fs::create_dir_all(unindexed_java.join("bin")).unwrap();
    fs::write(unindexed_java.join("bin/java.exe"), b"fixture").unwrap();
    fixture.runtime["javaHome"] = json!(unindexed_java);
    let service = fixture.register_with_service();

    let error = service
        .create_launch_execution(
            &fixture.instance.id,
            &LaunchAccount::offline("LocalPlayer").unwrap(),
            &LaunchOptions::default(),
        )
        .unwrap_err();

    assert!(error.to_string().contains("受管 Java 环境索引不一致"));
}

#[test]
fn m4_launch_010_runtime_cannot_replace_indexed_shared_store() {
    let mut fixture = LaunchFixture::new();
    let unindexed_store = fixture._directory.path().join("unindexed-store");
    fs::create_dir_all(&unindexed_store).unwrap();
    fixture.runtime["sharedStore"] = json!(unindexed_store);
    let service = fixture.register_with_service();

    let error = service
        .create_launch_execution(
            &fixture.instance.id,
            &LaunchAccount::offline("LocalPlayer").unwrap(),
            &LaunchOptions::default(),
        )
        .unwrap_err();

    assert!(error.to_string().contains("受管共享存储索引不一致"));
}

#[tokio::test]
async fn m4_launch_006_explicit_stop_persists_stopped_state_and_logs() {
    let fixture = LaunchFixture::new();
    fixture.use_current_test_binary_as_java();
    let service = fixture.register_with_service();
    let execution = service
        .create_launch_execution(
            &fixture.instance.id,
            &LaunchAccount::offline("LocalPlayer").unwrap(),
            &LaunchOptions::default(),
        )
        .unwrap();
    let (stop_sender, stop_receiver) = tokio::sync::oneshot::channel();
    stop_sender.send(()).unwrap();

    let completed = run_launch_execution(&service, execution, stop_receiver)
        .await
        .unwrap();

    assert_eq!(completed.state, LaunchSessionState::Stopped);
    assert!(completed.ended_at_unix_seconds.is_some());
    assert!(Path::new(&completed.stdout_path).is_file());
    assert!(Path::new(&completed.stderr_path).is_file());
}

#[test]
fn m4_launch_007_reopen_marks_orphaned_session_interrupted() {
    let fixture = LaunchFixture::new();
    let service = fixture.register_with_service();
    let execution = service
        .create_launch_execution(
            &fixture.instance.id,
            &LaunchAccount::offline("LocalPlayer").unwrap(),
            &LaunchOptions::default(),
        )
        .unwrap();
    let session_id = execution.session().id.clone();
    drop(execution);
    drop(service);

    let reopened = AppService::open(&fixture.database_path(), &fixture.shared).unwrap();
    let session = reopened
        .list_launch_sessions()
        .unwrap()
        .into_iter()
        .find(|session| session.id == session_id)
        .unwrap();

    assert_eq!(session.state, LaunchSessionState::Interrupted);
    assert!(session.ended_at_unix_seconds.is_some());
    assert!(
        session
            .error_summary
            .is_some_and(|summary| summary.contains("启动器退出"))
    );
}

#[test]
fn m4_launch_008_prepared_launch_debug_output_redacts_arguments() {
    let fixture = LaunchFixture::new();
    let prepared = prepare_launch_from_runtime(
        &fixture.instance,
        &fixture.runtime,
        &LaunchAccount::offline("LocalPlayer").unwrap(),
        &LaunchOptions::default(),
    )
    .unwrap();

    let debug = format!("{prepared:?}");
    assert!(debug.contains("<redacted>"));
    assert!(!debug.contains("--accessToken"));
    assert!(!debug.contains("LocalPlayer"));
}

#[tokio::test]
async fn m4_launch_009_closed_stop_channel_is_not_a_stop_request() {
    let fixture = LaunchFixture::new();
    fixture.use_current_test_binary_as_java();
    let service = fixture.register_with_service();
    let execution = service
        .create_launch_execution(
            &fixture.instance.id,
            &LaunchAccount::offline("LocalPlayer").unwrap(),
            &LaunchOptions::default(),
        )
        .unwrap();
    let (stop_sender, stop_receiver) = tokio::sync::oneshot::channel();
    drop(stop_sender);

    let completed = run_launch_execution(&service, execution, stop_receiver)
        .await
        .unwrap();

    assert_ne!(completed.state, LaunchSessionState::Stopped);
    assert!(matches!(
        completed.state,
        LaunchSessionState::Completed | LaunchSessionState::Failed
    ));
}

#[tokio::test]
async fn m4_launch_003_nonzero_process_exit_persists_logs_and_failure_state() {
    let fixture = LaunchFixture::new();
    fs::copy(
        std::env::current_exe().unwrap(),
        fixture.java_home.join("bin/java.exe"),
    )
    .unwrap();
    let service = fixture.register_with_service();
    let execution = service
        .create_launch_execution(
            &fixture.instance.id,
            &LaunchAccount::offline("LocalPlayer").unwrap(),
            &LaunchOptions::default(),
        )
        .unwrap();
    let session_id = execution.session().id.clone();
    let (_stop, stop_receiver) = tokio::sync::oneshot::channel();

    let completed = run_launch_execution(&service, execution, stop_receiver)
        .await
        .unwrap();

    assert_eq!(completed.state, LaunchSessionState::Failed);
    assert!(completed.exit_code.is_some_and(|code| code != 0));
    assert!(completed.ended_at_unix_seconds.is_some());
    assert!(Path::new(&completed.stdout_path).is_file());
    assert!(Path::new(&completed.stderr_path).is_file());
    assert_eq!(service.list_launch_sessions().unwrap()[0].id, session_id);
}

#[tokio::test]
async fn m4_launch_011_log_initialization_failure_is_persisted() {
    let fixture = LaunchFixture::new();
    let service = fixture.register_with_service();
    let execution = service
        .create_launch_execution(
            &fixture.instance.id,
            &LaunchAccount::offline("LocalPlayer").unwrap(),
            &LaunchOptions::default(),
        )
        .unwrap();
    fs::write(fixture.root.join(".minecraft/logs"), b"not a directory").unwrap();
    let (_stop_sender, stop_receiver) = tokio::sync::oneshot::channel();

    let error = run_launch_execution(&service, execution, stop_receiver)
        .await
        .unwrap_err();
    let session = service.list_launch_sessions().unwrap().remove(0);

    assert!(error.to_string().contains("logs"));
    assert_eq!(session.state, LaunchSessionState::Failed);
    assert!(session.ended_at_unix_seconds.is_some());
    assert!(session.error_summary.is_some());
}

struct LaunchFixture {
    _directory: TempDir,
    root: std::path::PathBuf,
    shared: std::path::PathBuf,
    java_home: std::path::PathBuf,
    instance: ManagedInstanceSummary,
    runtime: serde_json::Value,
}

impl LaunchFixture {
    fn new() -> Self {
        let directory = TempDir::new().unwrap();
        let root = directory.path().join("instance");
        let shared = directory.path().join("store");
        let java_home = directory.path().join("java");
        for directory in [
            root.join(".minecraft"),
            root.join("natives"),
            java_home.join("bin"),
            shared.join("minecraft/libraries"),
            shared.join("minecraft/versions/26.2"),
            shared.join("minecraft/assets/indexes"),
            shared.join("minecraft/assets/log_configs"),
        ] {
            fs::create_dir_all(directory).unwrap();
        }
        for file in [
            java_home.join("bin/java.exe"),
            shared.join("minecraft/libraries/example.jar"),
            shared.join("minecraft/libraries/example-natives-windows.jar"),
            shared.join("minecraft/versions/26.2/26.2.jar"),
            shared.join("minecraft/assets/indexes/32.json"),
            shared.join("minecraft/assets/log_configs/client.xml"),
        ] {
            fs::write(file, b"fixture").unwrap();
        }
        let instance = ManagedInstanceSummary {
            id: "instance-id".to_owned(),
            name: "启动测试".to_owned(),
            game_version: "26.2".to_owned(),
            loader_kind: "fabric".to_owned(),
            loader_version: Some("0.19.3".to_owned()),
            root_directory: root.to_string_lossy().into_owned(),
            state: "ready".to_owned(),
        };
        let runtime = runtime_manifest(&root, &shared, &java_home);
        Self {
            _directory: directory,
            root,
            shared,
            java_home,
            instance,
            runtime,
        }
    }

    fn register_with_service(&self) -> AppService {
        let database = self.database_path();
        let service = AppService::open(&database, self._directory.path()).unwrap();
        service.skip_onboarding().unwrap();
        fs::create_dir_all(self.root.join(".moyumax")).unwrap();
        fs::write(
            self.root.join(".moyumax/runtime.json"),
            serde_json::to_vec_pretty(&self.runtime).unwrap(),
        )
        .unwrap();
        let connection = Connection::open(database).unwrap();
        connection
            .execute(
                "INSERT INTO managed_java_environments (id, distribution, full_version, architecture, home_directory, status) VALUES (?1, 'azul-zulu', '25.0.4+7', 'x64', ?2, 'ready')",
                params!["java-id", self.java_home.to_string_lossy()],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO instances (id, name, game_version, loader_kind, loader_version, root_directory, state, created_at_unix_seconds) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'ready', 1)",
                params![
                    self.instance.id,
                    self.instance.name,
                    self.instance.game_version,
                    self.instance.loader_kind,
                    self.instance.loader_version,
                    self.instance.root_directory,
                ],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO instance_runtime (instance_id, java_environment_id, plan_json, runtime_json) VALUES (?1, 'java-id', '{}', ?2)",
                params![self.instance.id, serde_json::to_string(&self.runtime).unwrap()],
            )
            .unwrap();
        service
    }

    fn database_path(&self) -> std::path::PathBuf {
        self._directory.path().join("state.sqlite3")
    }

    fn use_current_test_binary_as_java(&self) {
        fs::copy(
            std::env::current_exe().unwrap(),
            self.java_home.join("bin/java.exe"),
        )
        .unwrap();
    }
}

fn runtime_manifest(root: &Path, shared: &Path, java_home: &Path) -> serde_json::Value {
    json!({
        "schemaVersion": 1,
        "gameVersion": "26.2",
        "mainClass": "net.fabricmc.loader.impl.launch.knot.KnotClient",
        "javaHome": java_home,
        "sharedStore": shared,
        "workingDirectory": ".minecraft",
        "nativesDirectory": "natives",
        "classpath": [
            "minecraft/versions/26.2/26.2.jar",
            "minecraft/libraries/example-natives-windows.jar",
            "minecraft/libraries/example.jar"
        ],
        "gameMetadata": {
            "id": "26.2",
            "type": "release",
            "assetIndex": {"id": "32"},
            "arguments": {
                "default-user-jvm": [
                    ["-Xms2G", "-Xmx4G", "-XX:+AlwaysPreTouch"],
                    {
                        "rules": [{"action": "allow", "os": {"name": "windows", "versionRange": {"min": "10.0.17134"}}}],
                        "value": "-XX:+UseZGC"
                    }
                ],
                "jvm": [
                    "-Djava.library.path=${natives_directory}/java",
                    "-Dminecraft.launcher.brand=${launcher_name}",
                    "-Dminecraft.launcher.version=${launcher_version}",
                    "-cp",
                    "${classpath}"
                ],
                "game": [
                    "--username", "${auth_player_name}",
                    "--version", "${version_name}",
                    "--gameDir", "${game_directory}",
                    "--assetsDir", "${assets_root}",
                    "--assetIndex", "${assets_index_name}",
                    "--uuid", "${auth_uuid}",
                    "--accessToken", "${auth_access_token}",
                    {"rules": [{"action": "allow", "features": {"is_demo_user": true}}], "value": "--demo"}
                ]
            },
            "logging": {"client": {"argument": "-Dlog4j.configurationFile=${path}", "file": {"id": "client.xml"}}}
        },
        "loaderProfile": {
            "id": "fabric-loader-0.19.3-26.2",
            "mainClass": "net.fabricmc.loader.impl.launch.knot.KnotClient",
            "arguments": {"jvm": ["-DFabricFixture=true"], "game": []}
        },
        "isolation": "full",
        "fixtureRoot": root
    })
}
