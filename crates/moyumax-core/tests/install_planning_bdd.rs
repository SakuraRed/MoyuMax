use std::path::Path;

use moyumax_core::{
    AppService, ArtifactKind, CatalogSource, GameReleaseType, GameVersionSummary, InstallStage,
    InstanceIsolation, JavaArchitecture, JavaDistribution, JavaEnvironmentStatus, JavaPlanAction,
    ManagedJavaEnvironment, RecoveryDecision, ResolvedArtifact, ResolvedGameVersion,
    ResolvedInstallRequest, ResolvedJavaPackage, ResolvedLoader, TaskState, VersionCatalog,
    parse_mojang_version_manifest,
};
use serde_json::json;
use tempfile::TempDir;

#[test]
fn m2_install_001_default_plan_contains_game_java_and_persistent_stages() {
    let fixture = TestFixture::new();
    let task = fixture
        .service
        .enqueue_install_task(&resolved_request("第一个 Fabric 实例"))
        .expect("default install task should be created");

    assert!(matches!(
        task.plan.java_action,
        JavaPlanAction::Install { .. }
    ));
    assert_eq!(
        task.plan.stages,
        vec![
            InstallStage::Prepare,
            InstallStage::DownloadGameFiles,
            InstallStage::VerifyFiles,
            InstallStage::InstallGameEnvironment,
            InstallStage::ApplyLoader,
            InstallStage::CommitChanges,
            InstallStage::CreateRollbackPoint,
        ]
    );
    assert!(Path::new(&task.staging_directory).is_dir());
    assert!(Path::new(&task.target_directory).starts_with(&fixture.data_directory));

    let reopened = fixture.reopen();
    assert_eq!(reopened.list_install_tasks().unwrap(), vec![task]);
}

#[test]
fn m2_install_002_ready_java_build_is_reused_without_duplicate_install() {
    let fixture = TestFixture::new();
    let managed = ready_managed_java(&fixture);
    fixture
        .service
        .register_managed_java(&managed)
        .expect("managed Java should be registered");

    let task = fixture
        .service
        .enqueue_install_task(&resolved_request("复用 Java 的实例"))
        .expect("install task should reuse Java");

    assert_eq!(
        task.plan.java_action,
        JavaPlanAction::Reuse {
            environment_id: managed.id,
            home_directory: managed.home_directory,
        }
    );
    assert_eq!(fixture.service.list_managed_java().unwrap().len(), 1);
}

#[test]
fn m2_install_003_running_task_requires_recovery_confirmation_after_reopen() {
    let fixture = TestFixture::new();
    let task = fixture
        .service
        .enqueue_install_task(&resolved_request("待恢复实例"))
        .unwrap();
    fixture.service.mark_install_task_running(&task.id).unwrap();

    let reopened = fixture.reopen();
    let recovered = reopened.list_install_tasks().unwrap();
    assert_eq!(recovered.len(), 1);
    assert_eq!(recovered[0].state, TaskState::AwaitingRecovery);
}

#[test]
fn m2_install_004_staging_does_not_publish_an_installing_instance() {
    let fixture = TestFixture::new();
    let task = fixture
        .service
        .enqueue_install_task(&resolved_request("未提交实例"))
        .unwrap();

    assert!(Path::new(&task.staging_directory).exists());
    assert!(fixture.service.list_instances().unwrap().is_empty());
    assert!(!Path::new(&task.target_directory).exists());
}

#[test]
fn m2_install_005_cached_version_catalog_survives_offline_reopen() {
    let fixture = TestFixture::new();
    let manifest = br#"{
      "latest": {"release": "1.21.8", "snapshot": "25w30a"},
      "versions": [
        {
          "id": "1.21.8",
          "type": "release",
          "url": "https://piston-meta.mojang.com/v1/packages/release.json",
          "sha1": "1111111111111111111111111111111111111111",
          "time": "2026-07-17T12:00:00+00:00",
          "releaseTime": "2026-07-17T12:00:00+00:00"
        },
        {
          "id": "25w30a",
          "type": "snapshot",
          "url": "https://piston-meta.mojang.com/v1/packages/snapshot.json",
          "sha1": "2222222222222222222222222222222222222222",
          "time": "2026-07-18T12:00:00+00:00",
          "releaseTime": "2026-07-18T12:00:00+00:00"
        }
      ]
    }"#;
    let catalog = parse_mojang_version_manifest(manifest, 1_753_000_000).unwrap();
    fixture.service.store_version_catalog(&catalog).unwrap();

    let cached = fixture
        .reopen()
        .cached_version_catalog()
        .unwrap()
        .expect("catalog should be available offline");

    assert_eq!(cached.source, CatalogSource::Cache);
    assert_eq!(cached.latest_release, "1.21.8");
    assert!(cached.versions[0].recommended);
}

#[test]
fn m2_install_006_declining_recovery_only_cleans_task_staging() {
    let fixture = TestFixture::new();
    fixture
        .service
        .register_managed_java(&ready_managed_java(&fixture))
        .unwrap();
    let task = fixture
        .service
        .enqueue_install_task(&resolved_request("取消恢复实例"))
        .unwrap();
    fixture.service.mark_install_task_running(&task.id).unwrap();
    let reopened = fixture.reopen();

    reopened
        .resolve_install_task_recovery(&task.id, RecoveryDecision::Discard)
        .unwrap();

    assert!(!Path::new(&task.staging_directory).exists());
    let tasks = reopened.list_install_tasks().unwrap();
    assert_eq!(tasks[0].state, TaskState::Cancelled);
    assert!(reopened.list_instances().unwrap().is_empty());
    assert_eq!(reopened.list_managed_java().unwrap().len(), 1);
}

struct TestFixture {
    _directory: TempDir,
    database_path: std::path::PathBuf,
    data_directory: std::path::PathBuf,
    service: AppService,
}

impl TestFixture {
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
}

fn resolved_request(instance_name: &str) -> ResolvedInstallRequest {
    ResolvedInstallRequest {
        instance_name: instance_name.to_owned(),
        game: ResolvedGameVersion {
            version: GameVersionSummary {
                id: "1.21.8".to_owned(),
                release_type: GameReleaseType::Release,
                release_time: "2026-07-17T12:00:00+00:00".to_owned(),
                metadata_url: "https://piston-meta.mojang.com/v1/packages/release.json".to_owned(),
                metadata_sha1: "1111111111111111111111111111111111111111".to_owned(),
                recommended: true,
            },
            java_major_version: 21,
            main_class: "net.minecraft.client.main.Main".to_owned(),
            metadata: json!({"id": "1.21.8", "javaVersion": {"majorVersion": 21}}),
            artifacts: vec![ResolvedArtifact {
                kind: ArtifactKind::GameClient,
                relative_path: "minecraft/versions/1.21.8/1.21.8.jar".to_owned(),
                url: "https://piston-data.mojang.com/v1/objects/client.jar".to_owned(),
                size: 30_000_000,
                sha1: Some("3333333333333333333333333333333333333333".to_owned()),
                sha256: None,
                sha512: None,
            }],
            asset_objects_total_bytes: 0,
        },
        loader: ResolvedLoader::Fabric {
            version: "0.16.14".to_owned(),
            stable: true,
            profile_url: "https://meta.fabricmc.net/v2/versions/loader/1.21.8/0.16.14/profile/json"
                .to_owned(),
            profile_sha256: "4444444444444444444444444444444444444444444444444444444444444444"
                .to_owned(),
            profile: json!({"id": "fabric-loader-0.16.14-1.21.8"}),
        },
        java: resolved_java(),
        isolation: InstanceIsolation::Full,
    }
}

fn resolved_java() -> ResolvedJavaPackage {
    ResolvedJavaPackage {
        distribution: JavaDistribution::AzulZulu,
        full_version: "21.0.12+8".to_owned(),
        architecture: JavaArchitecture::X64,
        package_uuid: "b712429b-594d-4b19-a765-b80c76f0170e".to_owned(),
        artifact: ResolvedArtifact {
            kind: ArtifactKind::JavaArchive,
            relative_path: "java/packages/zulu21.0.12+8-win_x64.zip".to_owned(),
            url: "https://cdn.azul.com/zulu/bin/zulu21.52.15-ca-jdk21.0.12-win_x64.zip".to_owned(),
            size: 210_632_000,
            sha1: None,
            sha256: Some(
                "3c06e6693fd6fa725b985e66798a6a8293c75f52b793490754ad3c54d3d8b5a6".to_owned(),
            ),
            sha512: None,
        },
    }
}

fn ready_managed_java(fixture: &TestFixture) -> ManagedJavaEnvironment {
    let java = resolved_java();
    ManagedJavaEnvironment {
        id: "java-zulu-21-test".to_owned(),
        distribution: java.distribution,
        full_version: java.full_version,
        architecture: java.architecture,
        home_directory: fixture
            .data_directory
            .join("java/azul-zulu/21.0.12+8/x64")
            .to_string_lossy()
            .into_owned(),
        status: JavaEnvironmentStatus::Ready,
    }
}

#[allow(dead_code)]
fn catalog_shape_example() -> VersionCatalog {
    VersionCatalog {
        latest_release: "1.21.8".to_owned(),
        latest_snapshot: "25w30a".to_owned(),
        versions: vec![],
        fetched_at_unix_seconds: 0,
        source: CatalogSource::Network,
    }
}

#[test]
fn m2_install_006_dedupe_artifacts_merges_overlap_and_rejects_conflicts() {
    use moyumax_core::dedupe_artifacts;

    // NeoForge install_profile 与 version.json 的库列表重叠场景(实测
    // commons-lang3 3.14.0):同路径同哈希应去重而不是报"重复目标路径"。
    let artifact = |sha1: Option<&str>| ResolvedArtifact {
        kind: ArtifactKind::Library,
        relative_path:
            "minecraft/libraries/org/apache/commons/commons-lang3/3.14.0/commons-lang3-3.14.0.jar"
                .to_owned(),
        url: "https://example.com/commons-lang3.jar".to_owned(),
        size: 600_000,
        sha1: sha1.map(str::to_owned),
        sha256: None,
        sha512: None,
    };

    let merged = dedupe_artifacts(vec![
        artifact(Some("a".repeat(40).as_str())),
        artifact(Some("a".repeat(40).as_str())),
    ])
    .expect("同路径同哈希必须去重");
    assert_eq!(merged.len(), 1);

    // 无哈希在前、有哈希在后:保留带哈希的记录。
    let merged = dedupe_artifacts(vec![
        artifact(None),
        artifact(Some("b".repeat(40).as_str())),
    ])
    .expect("同路径互补哈希必须去重");
    assert_eq!(merged.len(), 1);
    assert_eq!(merged[0].sha1.as_deref(), Some("b".repeat(40).as_str()));

    // 同路径不同哈希是真冲突,必须拒绝。
    assert!(
        dedupe_artifacts(vec![
            artifact(Some("a".repeat(40).as_str())),
            artifact(Some("b".repeat(40).as_str())),
        ])
        .is_err()
    );
}
