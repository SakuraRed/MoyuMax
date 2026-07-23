use std::{
    collections::HashMap,
    fs,
    io::{BufRead, BufReader, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

use moyumax_core::{
    AppService, ArtifactDownloader, ArtifactKind, ContentDependencyChoice, ContentExecutor,
    ContentFilePlan, ContentInstallPlan, ContentPlanEntry, CoreError, DownloadDisposition,
    DownloadInterrupt, GameReleaseType, GameVersionSummary, InstallExecutor, InstanceIsolation,
    JavaArchitecture, JavaDistribution, JavaEnvironmentStatus, ManagedJavaEnvironment,
    ResolvedArtifact, ResolvedGameVersion, ResolvedInstallRequest, ResolvedJavaPackage,
    ResolvedLoader, ShellState, TaskState, WindowCloseBehavior,
};
use rusqlite::{Connection, params};
use serde_json::json;
use sha1::{Digest, Sha1};
use sha2::Sha512;
use tempfile::TempDir;
use zip::{ZipWriter, write::SimpleFileOptions};

#[test]
fn m9_tray_001_close_behavior_defaults_to_ask_and_roundtrips() {
    let fixture = LifecycleFixture::new();
    assert_eq!(
        fixture.service.window_close_behavior().unwrap(),
        WindowCloseBehavior::Ask
    );

    fixture
        .service
        .set_window_close_behavior(WindowCloseBehavior::MinimizeToTray)
        .unwrap();
    assert_eq!(
        fixture.service.window_close_behavior().unwrap(),
        WindowCloseBehavior::MinimizeToTray
    );

    let reopened = fixture.reopen();
    assert_eq!(
        reopened.window_close_behavior().unwrap(),
        WindowCloseBehavior::MinimizeToTray
    );

    reopened
        .set_window_close_behavior(WindowCloseBehavior::Exit)
        .unwrap();
    assert_eq!(
        reopened.window_close_behavior().unwrap(),
        WindowCloseBehavior::Exit
    );

    Connection::open(&fixture.database_path)
        .unwrap()
        .execute(
            "INSERT OR REPLACE INTO app_settings (key, value) VALUES ('window_close_behavior', 'sideways')",
            [],
        )
        .unwrap();
    assert!(
        reopened.window_close_behavior().is_err(),
        "未知关闭行为必须报错而不是静默回退"
    );
}

#[test]
fn m9_tray_002_shell_state_persists_and_rejects_invalid() {
    let fixture = LifecycleFixture::new();
    assert!(fixture.service.shell_state().unwrap().is_none());

    let state = ShellState {
        page: "tasks".to_owned(),
        scroll_top: 320,
    };
    fixture.service.persist_shell_state(&state).unwrap();
    assert_eq!(fixture.service.shell_state().unwrap(), Some(state.clone()));

    let reopened = fixture.reopen();
    assert_eq!(reopened.shell_state().unwrap(), Some(state));

    for invalid_page in ["", "  ", "bad\npage"] {
        let invalid = ShellState {
            page: invalid_page.to_owned(),
            scroll_top: 0,
        };
        assert!(
            reopened.persist_shell_state(&invalid).is_err(),
            "非法页面标识 {invalid_page:?} 必须被拒绝"
        );
    }
}

#[test]
fn m9_task_002_paused_flag_survives_restart() {
    let fixture = LifecycleFixture::new();
    assert!(!fixture.service.tasks_paused().unwrap());

    fixture.service.set_tasks_paused(true).unwrap();
    assert!(fixture.service.tasks_paused().unwrap());

    let reopened = fixture.reopen();
    assert!(
        reopened.tasks_paused().unwrap(),
        "暂停标志必须在应用重启后保持"
    );

    reopened.set_tasks_paused(false).unwrap();
    assert!(!reopened.tasks_paused().unwrap());
}

#[tokio::test]
async fn m9_task_002_pause_interrupts_download_and_resume_continues_partial() {
    let body: Vec<u8> = (0..262_144_u32).map(|index| (index % 251) as u8).collect();
    let drip = 65_536_usize;
    let server = DripServer::new(HashMap::from([(
        "/artifact.bin".to_owned(),
        DripRoute {
            body: body.clone(),
            drip: Some(drip),
        },
    )]));
    let directory = TempDir::new().unwrap();
    let staging = directory.path().join("staging");
    let shared = directory.path().join("shared");
    let artifact = ResolvedArtifact {
        kind: ArtifactKind::GameClient,
        relative_path: "minecraft/versions/test/test.jar".to_owned(),
        url: server.url("/artifact.bin"),
        size: u64::try_from(body.len()).unwrap(),
        sha1: Some(digest_sha1(&body)),
        sha256: None,
        sha512: Some(digest_sha512(&body)),
    };
    let partial = partial_path(&staging.join(&artifact.relative_path));

    let downloader = ArtifactDownloader::new(1).unwrap();
    let interrupt = DownloadInterrupt::new();
    let handle = tokio::spawn({
        let downloader = downloader.clone();
        let interrupt = interrupt.clone();
        let artifact = artifact.clone();
        let staging = staging.clone();
        let shared = shared.clone();
        async move {
            downloader
                .fetch_with_interrupt(&artifact, &staging, &shared, Some(&interrupt))
                .await
        }
    });
    wait_until(|| partial.metadata().map_or(0, |meta| meta.len()) >= drip as u64).await;
    interrupt.interrupt();

    let error = handle
        .await
        .unwrap()
        .expect_err("中断的下载必须返回暂停错误");
    assert!(
        matches!(error, CoreError::TaskPaused),
        "期望 TaskPaused，实际 {error:?}"
    );
    assert_eq!(
        fs::read(&partial).unwrap(),
        body[..drip],
        "已写入的暂存分段必须原样保留"
    );

    let result = downloader
        .fetch(&artifact, &staging, &shared)
        .await
        .expect("恢复后应从中断分段继续下载");
    assert_eq!(result.disposition, DownloadDisposition::Resumed);
    assert_eq!(fs::read(result.staged_file).unwrap(), body);
    assert!(!partial.exists(), "完成后不应残留分段临时文件");
}

#[tokio::test]
async fn m9_task_002_content_task_pauses_instead_of_failing_and_resumes() {
    let fast = b"fabric-api-fixture".to_vec();
    let slow: Vec<u8> = (0..262_144_u32).map(|index| (index % 241) as u8).collect();
    let drip = 65_536_usize;
    let server = DripServer::new(HashMap::from([
        (
            "/fabric-api.jar".to_owned(),
            DripRoute {
                body: fast.clone(),
                drip: None,
            },
        ),
        (
            "/continuity.jar".to_owned(),
            DripRoute {
                body: slow.clone(),
                drip: Some(drip),
            },
        ),
    ]));
    let fixture = LifecycleFixture::new();
    fixture.insert_instance("instance-content", "内容暂停测试");
    let plan = fixture.content_plan("instance-content", &server, &fast, &slow);
    let task = fixture.service.enqueue_content_install_task(&plan).unwrap();

    let interrupt = DownloadInterrupt::new();
    let executor = ContentExecutor::new(2).unwrap();
    let handle = tokio::spawn({
        let service = fixture.service.clone();
        let interrupt = interrupt.clone();
        let task_id = task.id.clone();
        async move {
            executor
                .execute_task_with_interrupt(&service, &task_id, Some(interrupt))
                .await
        }
    });
    let slow_partial = content_partial_path(&task.staging_directory, &slow, "continuity.jar");
    wait_until(|| slow_partial.metadata().map_or(0, |meta| meta.len()) >= drip as u64).await;
    interrupt.interrupt();

    let error = handle.await.unwrap().expect_err("暂停中断必须返回暂停错误");
    assert!(matches!(error, CoreError::TaskPaused));
    let paused = fixture.service.list_content_install_tasks().unwrap();
    assert_eq!(
        paused[0].state,
        TaskState::Paused,
        "暂停中断后任务必须进入 paused 而不是 failed"
    );
    assert_eq!(
        fs::read(&slow_partial).unwrap(),
        slow[..drip],
        "中断点之前的分段必须保留"
    );

    let requeued = fixture.service.requeue_paused_content_tasks().unwrap();
    assert_eq!(requeued, vec![task.id.clone()]);
    assert_eq!(
        fixture.service.list_content_install_tasks().unwrap()[0].state,
        TaskState::Queued
    );

    let installed = ContentExecutor::new(2)
        .unwrap()
        .execute_task(&fixture.service, &task.id)
        .await
        .expect("恢复后应完成安装");
    assert_eq!(installed.len(), 2);
    assert_eq!(
        fs::read(
            fixture
                .mods_directory("instance-content")
                .join("continuity.jar")
        )
        .unwrap(),
        slow
    );
    assert_eq!(
        fixture.service.list_content_install_tasks().unwrap()[0].state,
        TaskState::Completed
    );
}

#[tokio::test]
async fn m9_task_002_install_task_pauses_and_completes_after_requeue() {
    let asset = b"asset-object".to_vec();
    let asset_hash = digest_sha1(&asset);
    let asset_index = serde_json::to_vec(&json!({
        "objects": {
            "minecraft/sounds/example.ogg": { "hash": asset_hash, "size": asset.len() }
        }
    }))
    .unwrap();
    let version_json =
        br#"{"id":"test-release","mainClass":"net.minecraft.client.main.Main"}"#.to_vec();
    let client: Vec<u8> = (0..262_144_u32).map(|index| (index % 233) as u8).collect();
    let drip = 65_536_usize;
    let native = native_archive();
    let server = DripServer::new(HashMap::from([
        (
            "/version.json".to_owned(),
            DripRoute {
                body: version_json.clone(),
                drip: None,
            },
        ),
        (
            "/client.jar".to_owned(),
            DripRoute {
                body: client.clone(),
                drip: Some(drip),
            },
        ),
        (
            "/asset-index.json".to_owned(),
            DripRoute {
                body: asset_index.clone(),
                drip: None,
            },
        ),
        (
            "/native.jar".to_owned(),
            DripRoute {
                body: native.clone(),
                drip: None,
            },
        ),
        (
            format!("/objects/{}/{asset_hash}", &asset_hash[..2]),
            DripRoute {
                body: asset.clone(),
                drip: None,
            },
        ),
    ]));
    let fixture = LifecycleFixture::new();
    fixture.register_ready_java();
    let request = fixture.install_request(
        &server,
        &version_json,
        &client,
        &asset_index,
        &native,
        asset.len() as u64,
    );
    let task = fixture.service.enqueue_install_task(&request).unwrap();

    let interrupt = DownloadInterrupt::new();
    let executor = InstallExecutor::new(4)
        .unwrap()
        .with_asset_base_url(server.url("/objects"))
        .unwrap();
    let handle = tokio::spawn({
        let service = fixture.service.clone();
        let interrupt = interrupt.clone();
        let task_id = task.id.clone();
        async move {
            executor
                .execute_task_with_interrupt(&service, &task_id, Some(interrupt))
                .await
        }
    });
    let client_partial = partial_path(
        &Path::new(&task.staging_directory)
            .join("downloads/minecraft/versions/test-release/test-release.jar"),
    );
    wait_until(|| client_partial.metadata().map_or(0, |meta| meta.len()) >= drip as u64).await;
    interrupt.interrupt();

    let error = handle.await.unwrap().expect_err("暂停中断必须返回暂停错误");
    assert!(matches!(error, CoreError::TaskPaused));
    let paused = fixture.service.list_install_tasks().unwrap();
    assert_eq!(paused[0].state, TaskState::Paused);

    // 重启后暂停任务保持 paused，不会伪装成待恢复中断。
    let reopened = fixture.reopen();
    assert_eq!(
        reopened.list_install_tasks().unwrap()[0].state,
        TaskState::Paused
    );

    let requeued = reopened.requeue_paused_install_tasks().unwrap();
    assert_eq!(requeued, vec![task.id.clone()]);

    let instance = InstallExecutor::new(4)
        .unwrap()
        .with_asset_base_url(server.url("/objects"))
        .unwrap()
        .execute_task(&reopened, &task.id)
        .await
        .expect("恢复后安装应完成");
    assert_eq!(instance.state, "ready");
    assert_eq!(
        fs::read(
            fixture
                .data_directory
                .join("store/minecraft/versions/test-release/test-release.jar")
        )
        .unwrap(),
        client
    );
    assert_eq!(
        reopened.list_install_tasks().unwrap()[0].state,
        TaskState::Completed
    );
}

#[test]
fn m9_task_002_paused_survives_restart_while_running_enters_recovery() {
    let fixture = LifecycleFixture::new();
    fixture.insert_instance("instance-recover", "恢复语义测试");
    let server = DripServer::new(HashMap::from([
        (
            "/fabric-api.jar".to_owned(),
            DripRoute {
                body: b"fabric-api".to_vec(),
                drip: None,
            },
        ),
        (
            "/continuity.jar".to_owned(),
            DripRoute {
                body: b"continuity".to_vec(),
                drip: None,
            },
        ),
    ]));
    let plan = fixture.content_plan("instance-recover", &server, b"fabric-api", b"continuity");
    let paused_task = fixture.service.enqueue_content_install_task(&plan).unwrap();
    fixture.set_task_running("content_install_tasks", &paused_task.id);
    fixture
        .service
        .mark_content_task_paused(&paused_task.id, "global")
        .unwrap();

    let mut running_plan =
        fixture.content_plan("instance-recover", &server, b"fabric-api", b"continuity");
    running_plan.entries[0].file.filename = "fabric-api-two.jar".to_owned();
    running_plan.entries[1].file.filename = "continuity-two.jar".to_owned();
    let running_task = fixture
        .service
        .enqueue_content_install_task(&running_plan)
        .unwrap();
    fixture.set_task_running("content_install_tasks", &running_task.id);

    let reopened = fixture.reopen();
    let states: HashMap<String, TaskState> = reopened
        .list_content_install_tasks()
        .unwrap()
        .into_iter()
        .map(|task| (task.id, task.state))
        .collect();
    assert_eq!(
        states[&paused_task.id],
        TaskState::Paused,
        "暂停任务重启后必须保持 paused"
    );
    assert_eq!(
        states[&running_task.id],
        TaskState::AwaitingRecovery,
        "重启时仍在 running 的任务必须进入 awaiting_recovery"
    );
}

#[test]
fn m9_tray_004_exit_impact_summarizes_sessions_and_tasks() {
    let fixture = LifecycleFixture::new();
    let impact = fixture.service.exit_impact_summary().unwrap();
    assert!(!impact.requires_confirmation());
    assert_eq!(impact.paused_tasks, 0);

    fixture.insert_instance("instance-impact", "影响测试");
    fixture.insert_launch_session("session-running", "instance-impact", "running");
    let server = DripServer::new(HashMap::from([
        (
            "/fabric-api.jar".to_owned(),
            DripRoute {
                body: b"fabric-api".to_vec(),
                drip: None,
            },
        ),
        (
            "/continuity.jar".to_owned(),
            DripRoute {
                body: b"continuity".to_vec(),
                drip: None,
            },
        ),
    ]));
    let plan = fixture.content_plan("instance-impact", &server, b"fabric-api", b"continuity");
    let task = fixture.service.enqueue_content_install_task(&plan).unwrap();

    let impact = fixture.service.exit_impact_summary().unwrap();
    assert_eq!(impact.running_sessions.len(), 1);
    assert_eq!(
        impact.running_sessions[0].instance_name,
        "影响测试".to_owned()
    );
    assert_eq!(impact.active_content_tasks, 1);
    assert!(impact.requires_confirmation());

    fixture.set_task_running("content_install_tasks", &task.id);
    fixture
        .service
        .mark_content_task_paused(&task.id, "global")
        .unwrap();
    Connection::open(&fixture.database_path)
        .unwrap()
        .execute(
            "UPDATE launch_sessions SET state = 'stopped', ended_at_unix_seconds = 2 WHERE id = 'session-running'",
            [],
        )
        .unwrap();

    let impact = fixture.service.exit_impact_summary().unwrap();
    assert_eq!(impact.paused_tasks, 1);
    assert!(
        !impact.requires_confirmation(),
        "只剩已暂停任务时退出不需要额外确认"
    );
}

#[test]
fn m9_tray_003_recent_instances_order_running_marker_and_recycle_exclusion() {
    let fixture = LifecycleFixture::new();
    fixture.insert_instance_at("instance-a", "实例甲", 1);
    fixture.insert_instance_at("instance-b", "实例乙", 2);
    fixture.insert_instance_at("instance-c", "实例丙", 3);
    fixture.insert_launch_session_at("session-b", "instance-b", "running", 10);
    fixture.insert_launch_session_at("session-c", "instance-c", "completed", 20);

    let recent = fixture.service.recent_instances(5).unwrap();
    let order: Vec<&str> = recent.iter().map(|entry| entry.id.as_str()).collect();
    assert_eq!(order, vec!["instance-c", "instance-b", "instance-a"]);
    assert!(!recent[0].is_running);
    assert!(recent[1].is_running, "运行中的实例必须带运行标记");
    assert_eq!(recent[2].last_started_at_unix_seconds, None);

    // 进入回收站的实例不再出现在托盘最近实例中。
    Connection::open(&fixture.database_path)
        .unwrap()
        .execute(
            "
            INSERT INTO recycle_bin_items (
                id, item_kind, subject_id, display_name, original_path, recycled_path,
                original_state, size_bytes, deleted_at_unix_seconds, expires_at_unix_seconds, state
            ) VALUES ('recycle-c', 'instance', 'instance-c', '实例丙', '/original', '/recycled',
                      'ready', 0, 30, 40, 'ready')
            ",
            [],
        )
        .unwrap();
    let recent = fixture.service.recent_instances(5).unwrap();
    let order: Vec<&str> = recent.iter().map(|entry| entry.id.as_str()).collect();
    assert_eq!(order, vec!["instance-b", "instance-a"]);

    let limited = fixture.service.recent_instances(1).unwrap();
    assert_eq!(limited.len(), 1);
    assert_eq!(limited[0].id, "instance-b");
}

struct LifecycleFixture {
    _directory: TempDir,
    database_path: PathBuf,
    data_directory: PathBuf,
    service: AppService,
}

impl LifecycleFixture {
    fn new() -> Self {
        let directory = TempDir::new().unwrap();
        let database_path = directory.path().join("state.sqlite3");
        let data_directory = directory.path().join("data");
        let service = AppService::open(&database_path, &data_directory).unwrap();
        service.skip_onboarding().unwrap();
        Self {
            _directory: directory,
            database_path,
            data_directory,
            service,
        }
    }

    fn reopen(&self) -> AppService {
        AppService::open(&self.database_path, &self.data_directory).unwrap()
    }

    fn insert_instance(&self, id: &str, name: &str) {
        self.insert_instance_at(id, name, 1);
    }

    fn insert_instance_at(&self, id: &str, name: &str, created_at: i64) {
        let root = self.data_directory.join("instances").join(id);
        fs::create_dir_all(root.join(".minecraft/mods")).unwrap();
        Connection::open(&self.database_path)
            .unwrap()
            .execute(
                "
                INSERT INTO instances (
                    id, name, game_version, loader_kind, loader_version,
                    root_directory, state, created_at_unix_seconds
                ) VALUES (?1, ?2, '26.2', 'fabric', '0.19.3', ?3, 'ready', ?4)
                ",
                params![id, name, root.to_string_lossy(), created_at],
            )
            .unwrap();
    }

    fn insert_launch_session(&self, id: &str, instance_id: &str, state: &str) {
        self.insert_launch_session_at(id, instance_id, state, 1);
    }

    fn insert_launch_session_at(&self, id: &str, instance_id: &str, state: &str, started_at: i64) {
        Connection::open(&self.database_path)
            .unwrap()
            .execute(
                "
                INSERT INTO launch_sessions (
                    id, instance_id, player_name, state, started_at_unix_seconds,
                    stdout_path, stderr_path
                ) VALUES (?1, ?2, 'Player', ?3, ?4, 'stdout.log', 'stderr.log')
                ",
                params![id, instance_id, state, started_at],
            )
            .unwrap();
    }

    fn set_task_running(&self, table: &str, task_id: &str) {
        let query = format!("UPDATE {table} SET state = 'running' WHERE id = ?1");
        Connection::open(&self.database_path)
            .unwrap()
            .execute(&query, params![task_id])
            .unwrap();
    }

    fn mods_directory(&self, instance_id: &str) -> PathBuf {
        self.data_directory
            .join("instances")
            .join(instance_id)
            .join(".minecraft/mods")
    }

    fn register_ready_java(&self) {
        let home = self.data_directory.join("java-ready");
        fs::create_dir_all(home.join("bin")).unwrap();
        fs::write(home.join("bin/java.exe"), b"fixture").unwrap();
        self.service
            .register_managed_java(&ManagedJavaEnvironment {
                id: "ready-java".to_owned(),
                distribution: JavaDistribution::AzulZulu,
                full_version: "21.0.12+8".to_owned(),
                architecture: JavaArchitecture::X64,
                home_directory: home.to_string_lossy().into_owned(),
                status: JavaEnvironmentStatus::Ready,
            })
            .unwrap();
    }

    fn content_plan(
        &self,
        instance_id: &str,
        server: &DripServer,
        fast: &[u8],
        slow: &[u8],
    ) -> ContentInstallPlan {
        ContentInstallPlan {
            schema_version: 1,
            instance_id: instance_id.to_owned(),
            instance_name: format!("内容测试-{instance_id}"),
            game_version: "26.2".to_owned(),
            loader: "fabric".to_owned(),
            root_project_id: "ROOT0001".to_owned(),
            entries: vec![
                content_entry(
                    "DEP00001",
                    "DEPVER01",
                    "Fabric API",
                    "fabric-api.jar",
                    server.url("/fabric-api.jar"),
                    fast,
                    Some("ROOT0001"),
                ),
                content_entry(
                    "ROOT0001",
                    "ROOTVER1",
                    "Continuity",
                    "continuity.jar",
                    server.url("/continuity.jar"),
                    slow,
                    None,
                ),
            ],
            optional_dependencies: Vec::<ContentDependencyChoice>::new(),
            incompatible_dependencies: Vec::<ContentDependencyChoice>::new(),
        }
    }

    fn install_request(
        &self,
        server: &DripServer,
        version_json: &[u8],
        client: &[u8],
        asset_index: &[u8],
        native_archive: &[u8],
        asset_size: u64,
    ) -> ResolvedInstallRequest {
        let version = "test-release";
        ResolvedInstallRequest {
            instance_name: "暂停恢复测试".to_owned(),
            game: ResolvedGameVersion {
                version: GameVersionSummary {
                    id: version.to_owned(),
                    release_type: GameReleaseType::Release,
                    release_time: "2026-07-22T00:00:00Z".to_owned(),
                    metadata_url: server.url("/version.json"),
                    metadata_sha1: digest_sha1(version_json),
                    recommended: true,
                },
                java_major_version: 21,
                main_class: "net.minecraft.client.main.Main".to_owned(),
                metadata: json!({
                    "id": version,
                    "mainClass": "net.minecraft.client.main.Main",
                    "assetIndex": {"id": "test-assets"},
                    "arguments": {"game": [], "jvm": []},
                    "libraries": [{
                        "name": "org.lwjgl:lwjgl:3.3.3",
                        "rules": [{"action": "allow", "os": {"name": "windows"}}],
                        "natives": {"windows": "natives-windows"},
                        "downloads": {
                            "classifiers": {
                                "natives-windows": {
                                    "path": "org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3-natives-windows.jar",
                                    "url": server.url("/native.jar"),
                                    "sha1": digest_sha1(native_archive),
                                    "size": native_archive.len()
                                }
                            }
                        }
                    }]
                }),
                artifacts: vec![
                    artifact(
                        ArtifactKind::VersionMetadata,
                        &format!("minecraft/versions/{version}/{version}.json"),
                        server.url("/version.json"),
                        version_json,
                    ),
                    artifact(
                        ArtifactKind::GameClient,
                        &format!("minecraft/versions/{version}/{version}.jar"),
                        server.url("/client.jar"),
                        client,
                    ),
                    artifact(
                        ArtifactKind::AssetIndex,
                        "minecraft/assets/indexes/test-assets.json",
                        server.url("/asset-index.json"),
                        asset_index,
                    ),
                ],
                asset_objects_total_bytes: asset_size,
            },
            loader: ResolvedLoader::Vanilla,
            java: ResolvedJavaPackage {
                distribution: JavaDistribution::AzulZulu,
                full_version: "21.0.12+8".to_owned(),
                architecture: JavaArchitecture::X64,
                package_uuid: "fixture-java".to_owned(),
                artifact: ResolvedArtifact {
                    kind: ArtifactKind::JavaArchive,
                    relative_path: "java/packages/unused.zip".to_owned(),
                    url: server.url("/unused-java.zip"),
                    size: 1,
                    sha1: None,
                    sha256: Some("00".repeat(32)),
                    sha512: None,
                },
            },
            isolation: InstanceIsolation::Full,
        }
    }
}

fn content_entry(
    project_id: &str,
    version_id: &str,
    project_title: &str,
    filename: &str,
    url: String,
    bytes: &[u8],
    required_by_project_id: Option<&str>,
) -> ContentPlanEntry {
    ContentPlanEntry {
        project_id: project_id.to_owned(),
        version_id: version_id.to_owned(),
        project_title: project_title.to_owned(),
        version_number: "1.0.0".to_owned(),
        required_by_project_id: required_by_project_id.map(str::to_owned),
        file: ContentFilePlan {
            url,
            filename: filename.to_owned(),
            size: u64::try_from(bytes.len()).unwrap(),
            sha1: digest_sha1(bytes),
            sha512: digest_sha512(bytes),
        },
    }
}

fn artifact(
    kind: ArtifactKind,
    relative_path: &str,
    url: String,
    bytes: &[u8],
) -> ResolvedArtifact {
    ResolvedArtifact {
        kind,
        relative_path: relative_path.to_owned(),
        url,
        size: u64::try_from(bytes.len()).unwrap(),
        sha1: Some(digest_sha1(bytes)),
        sha256: None,
        sha512: None,
    }
}

fn native_archive() -> Vec<u8> {
    let cursor = std::io::Cursor::new(Vec::new());
    let mut writer = ZipWriter::new(cursor);
    writer
        .start_file("fixture-native.dll", SimpleFileOptions::default())
        .unwrap();
    writer.write_all(b"native-dll").unwrap();
    writer.finish().unwrap().into_inner()
}

fn content_partial_path(staging_directory: &str, bytes: &[u8], filename: &str) -> PathBuf {
    let sha512 = digest_sha512(bytes);
    partial_path(
        &Path::new(staging_directory)
            .join("downloads")
            .join("content/modrinth")
            .join(&sha512[..2])
            .join(sha512)
            .join(filename),
    )
}

fn partial_path(staged_file: &Path) -> PathBuf {
    let extension = staged_file
        .extension()
        .and_then(|value| value.to_str())
        .map_or_else(|| "part".to_owned(), |value| format!("{value}.part"));
    staged_file.with_extension(extension)
}

async fn wait_until(condition: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !condition() {
        assert!(Instant::now() < deadline, "等待下载分段写入超时");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

fn digest_sha1(bytes: &[u8]) -> String {
    let mut hasher = Sha1::new();
    hasher.update(bytes);
    encode_hex(hasher.finalize())
}

fn digest_sha512(bytes: &[u8]) -> String {
    let mut hasher = Sha512::new();
    hasher.update(bytes);
    encode_hex(hasher.finalize())
}

fn encode_hex(bytes: impl AsRef<[u8]>) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let bytes = bytes.as_ref();
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}

struct DripRoute {
    body: Vec<u8>,
    drip: Option<usize>,
}

struct DripServer {
    address: std::net::SocketAddr,
    _thread: thread::JoinHandle<()>,
}

impl DripServer {
    fn new(routes: HashMap<String, DripRoute>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let routes = Arc::new(routes);
        let server_thread = thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                let routes = Arc::clone(&routes);
                thread::spawn(move || serve_drip(stream, &routes));
            }
        });
        Self {
            address,
            _thread: server_thread,
        }
    }

    fn url(&self, path: &str) -> String {
        format!("http://{}{path}", self.address)
    }
}

fn serve_drip(mut stream: TcpStream, routes: &HashMap<String, DripRoute>) {
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut request = String::new();
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
            break;
        }
        request.push_str(&line);
    }
    let path = request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("/");
    let Some(route) = routes.get(path) else {
        write!(
            stream,
            "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        )
        .unwrap();
        return;
    };
    let range_start = request.lines().find_map(|line| {
        line.strip_prefix("Range: bytes=")
            .or_else(|| line.strip_prefix("range: bytes="))
            .and_then(|value| value.trim_end_matches('-').parse::<usize>().ok())
    });
    if let Some(start) = range_start {
        let remaining = &route.body[start..];
        write!(
            stream,
            "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {}-{}/{}\r\nConnection: close\r\n\r\n",
            remaining.len(),
            start,
            route.body.len() - 1,
            route.body.len()
        )
        .unwrap();
        let _ = stream.write_all(remaining);
        let _ = stream.flush();
        return;
    }
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        route.body.len()
    )
    .unwrap();
    if let Some(drip) = route.drip {
        let _ = stream.write_all(&route.body[..drip]);
        let _ = stream.flush();
        // 模拟慢速来源:剩余字节拖延发送,给暂停中断留出窗口。
        thread::sleep(Duration::from_secs(10));
        let _ = stream.write_all(&route.body[drip..]);
        let _ = stream.flush();
    } else {
        let _ = stream.write_all(&route.body);
        let _ = stream.flush();
    }
}
