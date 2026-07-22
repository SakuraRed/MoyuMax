use std::{
    fs,
    path::{Path, PathBuf},
};

use moyumax_core::{
    AppService, JavaArchitecture, JavaDistribution, OnboardingSelection, RecycleItemKind,
    RecycleItemState,
};
use rusqlite::{Connection, params};
use tempfile::TempDir;

const RETENTION_SECONDS: i64 = 30 * 24 * 60 * 60;

struct Fixture {
    _temp: TempDir,
    database_path: PathBuf,
    data_directory: PathBuf,
    instance_directory: PathBuf,
    service: AppService,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let database_path = temp.path().join("state/moyumax.sqlite");
        let data_directory = temp.path().join("data");
        let service = AppService::open(&database_path, &data_directory).unwrap();
        service
            .complete_onboarding(&OnboardingSelection::recommended(path_text(
                &data_directory,
            )))
            .unwrap();

        let instance_directory = data_directory.join("instances/instance-1");
        fs::create_dir_all(instance_directory.join(".minecraft/saves/First World")).unwrap();
        fs::write(
            instance_directory.join(".minecraft/saves/First World/level.dat"),
            b"world-data",
        )
        .unwrap();
        fs::create_dir_all(instance_directory.join(".minecraft/mods")).unwrap();
        fs::write(
            instance_directory.join(".minecraft/mods/example.jar"),
            b"mod-data",
        )
        .unwrap();
        fs::create_dir_all(data_directory.join("store/java/azul-zulu/21.0.12+8/bin")).unwrap();
        fs::write(
            data_directory.join("store/java/azul-zulu/21.0.12+8/bin/java.exe"),
            b"managed-java",
        )
        .unwrap();

        let connection = Connection::open(&database_path).unwrap();
        connection
            .pragma_update(None, "foreign_keys", true)
            .unwrap();
        connection
            .execute(
                "INSERT INTO managed_java_environments (id, distribution, full_version, architecture, home_directory, status) VALUES ('java-1', 'azul-zulu', '21.0.12+8', 'x64', ?1, 'ready')",
                params![path_text(&data_directory.join("store/java/azul-zulu/21.0.12+8"))],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO instances (id, name, game_version, loader_kind, loader_version, root_directory, state, created_at_unix_seconds) VALUES ('instance-1', '生存世界', '1.21.8', 'fabric', '0.16.14', ?1, 'ready', 1)",
                params![path_text(&instance_directory)],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO instance_runtime (instance_id, java_environment_id, plan_json, runtime_json) VALUES ('instance-1', 'java-1', '{}', '{}')",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO installed_content (id, instance_id, provider, project_id, version_id, project_title, version_number, file_name, relative_path, size, sha1, sha512, installed_at_unix_seconds) VALUES ('content-1', 'instance-1', 'modrinth', 'project-1', 'version-1', '示例模组', '1.0.0', 'example.jar', '.minecraft/mods/example.jar', 8, ?1, ?2, 1)",
                params!["1".repeat(40), "2".repeat(128)],
            )
            .unwrap();

        Self {
            _temp: temp,
            database_path,
            data_directory,
            instance_directory,
            service,
        }
    }

    fn reopen(&self) -> AppService {
        AppService::open(&self.database_path, &self.data_directory).unwrap()
    }
}

#[test]
fn m7_recycle_001_deleted_instance_restores_with_world_and_java_intact() {
    let fixture = Fixture::new();

    let item = fixture.service.recycle_instance("instance-1").unwrap();

    assert_eq!(item.kind, RecycleItemKind::Instance);
    assert_eq!(item.state, RecycleItemState::Ready);
    assert_eq!(
        item.expires_at_unix_seconds - item.deleted_at_unix_seconds,
        RETENTION_SECONDS
    );
    assert!(item.size_bytes >= b"world-data".len() as u64);
    assert!(fixture.service.list_instances().unwrap().is_empty());
    assert!(!fixture.instance_directory.exists());
    assert!(
        Path::new(&item.recycled_path)
            .join(".minecraft/saves/First World/level.dat")
            .is_file()
    );
    assert_eq!(fixture.service.list_managed_java().unwrap().len(), 1);
    assert_eq!(
        fixture
            .service
            .list_installed_content("instance-1")
            .unwrap()
            .len(),
        1
    );

    let restored = fixture.service.restore_recycle_bin_item(&item.id).unwrap();

    assert_eq!(restored.id, "instance-1");
    assert_eq!(restored.state, "ready");
    assert_eq!(
        fs::read(
            fixture
                .instance_directory
                .join(".minecraft/saves/First World/level.dat")
        )
        .unwrap(),
        b"world-data",
    );
    assert!(fixture.service.list_recycle_bin_items().unwrap().is_empty());
    let java = fixture.service.list_managed_java().unwrap();
    assert_eq!(java.len(), 1);
    assert_eq!(java[0].distribution, JavaDistribution::AzulZulu);
    assert_eq!(java[0].architecture, JavaArchitecture::X64);
    assert!(
        Path::new(&java[0].home_directory)
            .join("bin/java.exe")
            .is_file()
    );
    assert_eq!(
        fixture
            .service
            .list_installed_content("instance-1")
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn m7_recycle_002_running_instance_is_rejected_without_moving_data() {
    let fixture = Fixture::new();
    let connection = Connection::open(&fixture.database_path).unwrap();
    connection
        .pragma_update(None, "foreign_keys", true)
        .unwrap();
    connection
        .execute(
            "INSERT INTO launch_sessions (id, instance_id, player_name, state, started_at_unix_seconds, stdout_path, stderr_path) VALUES ('session-1', 'instance-1', 'Player', 'running', 1, 'stdout.log', 'stderr.log')",
            [],
        )
        .unwrap();

    let error = fixture.service.recycle_instance("instance-1").unwrap_err();

    assert!(error.to_string().contains("先停止游戏"));
    assert!(fixture.instance_directory.is_dir());
    assert_eq!(fixture.service.list_instances().unwrap().len(), 1);
    assert!(fixture.service.list_recycle_bin_items().unwrap().is_empty());
}

#[test]
fn m7_recycle_003_restore_conflict_never_overwrites_either_copy() {
    let fixture = Fixture::new();
    let item = fixture.service.recycle_instance("instance-1").unwrap();
    fs::create_dir_all(&fixture.instance_directory).unwrap();
    fs::write(fixture.instance_directory.join("foreign.txt"), b"keep-me").unwrap();

    let error = fixture
        .service
        .restore_recycle_bin_item(&item.id)
        .unwrap_err();

    assert!(error.to_string().contains("原位置已被占用"));
    assert_eq!(
        fs::read(fixture.instance_directory.join("foreign.txt")).unwrap(),
        b"keep-me"
    );
    assert!(
        Path::new(&item.recycled_path)
            .join(".minecraft/saves/First World/level.dat")
            .is_file()
    );
    assert_eq!(
        fixture.service.list_recycle_bin_items().unwrap(),
        vec![item]
    );
}

#[test]
fn m7_recycle_004_permanent_delete_removes_only_instance_owned_data() {
    let fixture = Fixture::new();
    let item = fixture.service.recycle_instance("instance-1").unwrap();

    let result = fixture.service.purge_recycle_bin_item(&item.id).unwrap();

    assert_eq!(result.item_id, item.id);
    assert_eq!(result.removed_subjects, 1);
    assert_eq!(result.released_bytes, item.size_bytes);
    assert!(!Path::new(&item.recycled_path).exists());
    assert!(fixture.service.list_instances().unwrap().is_empty());
    assert!(fixture.service.list_recycle_bin_items().unwrap().is_empty());
    let java = fixture.service.list_managed_java().unwrap();
    assert_eq!(java.len(), 1);
    assert!(
        Path::new(&java[0].home_directory)
            .join("bin/java.exe")
            .is_file()
    );
    assert!(fixture.data_directory.join("store").is_dir());
    assert!(
        fixture
            .service
            .list_installed_content("instance-1")
            .unwrap()
            .is_empty()
    );
}

#[test]
fn m7_recycle_005_reopen_finishes_a_persisted_move_once() {
    let fixture = Fixture::new();
    let recycled_directory = fixture
        .data_directory
        .join(".recycle/instances/recycle-interrupted");
    let connection = Connection::open(&fixture.database_path).unwrap();
    connection
        .pragma_update(None, "foreign_keys", true)
        .unwrap();
    connection
        .execute(
            "INSERT INTO recycle_bin_items (id, item_kind, subject_id, display_name, original_path, recycled_path, original_state, size_bytes, deleted_at_unix_seconds, expires_at_unix_seconds, state) VALUES ('recycle-interrupted', 'instance', 'instance-1', '生存世界', ?1, ?2, 'ready', 10, 10, ?3, 'moving')",
            params![path_text(&fixture.instance_directory), path_text(&recycled_directory), 10 + RETENTION_SECONDS],
        )
        .unwrap();
    connection
        .execute(
            "UPDATE instances SET state = 'recycling' WHERE id = 'instance-1'",
            [],
        )
        .unwrap();
    drop(connection);

    let reopened = fixture.reopen();

    assert!(reopened.list_instances().unwrap().is_empty());
    let items = reopened.list_recycle_bin_items().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].state, RecycleItemState::Ready);
    assert!(!fixture.instance_directory.exists());
    assert!(
        recycled_directory
            .join(".minecraft/saves/First World/level.dat")
            .is_file()
    );
}

#[test]
fn m7_recycle_006_reopen_finishes_a_restore_after_the_file_move() {
    let fixture = Fixture::new();
    let item = fixture.service.recycle_instance("instance-1").unwrap();
    let recycled_directory = PathBuf::from(&item.recycled_path);
    let connection = Connection::open(&fixture.database_path).unwrap();
    connection
        .pragma_update(None, "foreign_keys", true)
        .unwrap();
    connection
        .execute(
            "UPDATE recycle_bin_items SET state = 'restoring' WHERE id = ?1",
            params![item.id],
        )
        .unwrap();
    connection
        .execute(
            "UPDATE instances SET state = 'restoring' WHERE id = 'instance-1'",
            [],
        )
        .unwrap();
    fs::rename(&recycled_directory, &fixture.instance_directory).unwrap();
    drop(connection);

    let reopened = fixture.reopen();

    assert!(reopened.list_recycle_bin_items().unwrap().is_empty());
    let instances = reopened.list_instances().unwrap();
    assert_eq!(instances.len(), 1);
    assert_eq!(instances[0].state, "ready");
    assert!(
        fixture
            .instance_directory
            .join(".minecraft/saves/First World/level.dat")
            .is_file()
    );
    assert_eq!(
        reopened.list_installed_content("instance-1").unwrap().len(),
        1
    );
}

#[test]
fn m7_recycle_007_reopen_finishes_a_persisted_permanent_delete() {
    let fixture = Fixture::new();
    let item = fixture.service.recycle_instance("instance-1").unwrap();
    let connection = Connection::open(&fixture.database_path).unwrap();
    connection
        .pragma_update(None, "foreign_keys", true)
        .unwrap();
    connection
        .execute(
            "UPDATE recycle_bin_items SET state = 'purging' WHERE id = ?1",
            params![item.id],
        )
        .unwrap();
    connection
        .execute(
            "UPDATE instances SET state = 'purging' WHERE id = 'instance-1'",
            [],
        )
        .unwrap();
    drop(connection);

    let reopened = fixture.reopen();

    assert!(reopened.list_recycle_bin_items().unwrap().is_empty());
    assert!(reopened.list_instances().unwrap().is_empty());
    assert!(!Path::new(&item.recycled_path).exists());
    assert_eq!(reopened.list_managed_java().unwrap().len(), 1);
    assert!(fixture.data_directory.join("store/java").is_dir());
}

fn path_text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}
