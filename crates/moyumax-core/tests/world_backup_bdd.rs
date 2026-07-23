use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};

use moyumax_core::{AppService, BackupState, BackupTrigger, OnboardingSelection};
use rusqlite::{Connection, params};
use tempfile::TempDir;
use zip::ZipArchive;

struct Fixture {
    _temp: TempDir,
    database_path: PathBuf,
    data_directory: PathBuf,
    instance_directory: PathBuf,
    service: AppService,
}

impl Fixture {
    fn new(with_world: bool) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let database_path = temp.path().join("state/moyumax.sqlite");
        let data_directory = temp.path().join("data");
        let service = AppService::open(&database_path, &data_directory).unwrap();
        service
            .complete_onboarding(&OnboardingSelection::recommended(path_text(
                &data_directory,
            )))
            .unwrap();
        let instance_directory = data_directory.join("instances/instance-backup");
        fs::create_dir_all(instance_directory.join(".minecraft")).unwrap();
        if with_world {
            fs::create_dir_all(instance_directory.join(".minecraft/saves/First World/region"))
                .unwrap();
            fs::write(
                instance_directory.join(".minecraft/saves/First World/level.dat"),
                b"before-launch",
            )
            .unwrap();
            fs::write(
                instance_directory.join(".minecraft/saves/First World/region/r.0.0.mca"),
                b"region-data",
            )
            .unwrap();
        }
        fs::create_dir_all(data_directory.join("store/java/azul-zulu/21/bin")).unwrap();
        fs::write(
            data_directory.join("store/java/azul-zulu/21/bin/java.exe"),
            b"managed-java",
        )
        .unwrap();
        let connection = Connection::open(&database_path).unwrap();
        connection
            .pragma_update(None, "foreign_keys", true)
            .unwrap();
        connection
            .execute(
                "INSERT INTO managed_java_environments (id, distribution, full_version, architecture, home_directory, status) VALUES ('java-backup', 'azul-zulu', '21', 'x64', ?1, 'ready')",
                params![path_text(&data_directory.join("store/java/azul-zulu/21"))],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO instances (id, name, game_version, loader_kind, loader_version, root_directory, state, created_at_unix_seconds) VALUES ('instance-backup', '备份测试', '1.21.8', 'fabric', '0.16.14', ?1, 'ready', 1)",
                params![path_text(&instance_directory)],
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
fn m8_backup_001_pre_and_post_snapshots_are_atomic_and_keep_distinct_content() {
    let fixture = Fixture::new(true);

    let before = fixture
        .service
        .create_world_backup(
            "instance-backup",
            BackupTrigger::PreLaunch,
            Some("session-1"),
        )
        .unwrap();
    assert_eq!(before.state, BackupState::Ready);
    assert_eq!(before.world_count, 1);
    assert_eq!(
        zip_text(
            before.archive_path.as_deref().unwrap(),
            "worlds/First World/level.dat"
        ),
        b"before-launch"
    );
    assert!(
        !Path::new(&format!(
            "{}.partial",
            before.archive_path.as_deref().unwrap()
        ))
        .exists()
    );

    fs::write(
        fixture
            .instance_directory
            .join(".minecraft/saves/First World/level.dat"),
        b"after-exit",
    )
    .unwrap();
    let after = fixture
        .service
        .create_world_backup(
            "instance-backup",
            BackupTrigger::PostExit,
            Some("session-1"),
        )
        .unwrap();

    assert_eq!(after.state, BackupState::Ready);
    assert_eq!(
        zip_text(
            after.archive_path.as_deref().unwrap(),
            "worlds/First World/level.dat"
        ),
        b"after-exit"
    );
    let history = fixture
        .service
        .list_world_backups(Some("instance-backup"))
        .unwrap();
    assert_eq!(history.len(), 2);
    assert!(
        fixture
            .data_directory
            .join("store/java/azul-zulu/21/bin/java.exe")
            .is_file()
    );
}

#[test]
fn m8_backup_002_no_worlds_records_skipped_without_empty_archive() {
    let fixture = Fixture::new(false);

    let backup = fixture
        .service
        .create_world_backup(
            "instance-backup",
            BackupTrigger::PreLaunch,
            Some("session-empty"),
        )
        .unwrap();

    assert_eq!(backup.state, BackupState::Skipped);
    assert_eq!(backup.world_count, 0);
    assert_eq!(backup.source_bytes, 0);
    assert!(backup.archive_path.is_none());
    assert!(!fixture.data_directory.join("backups").exists());
}

#[test]
fn m8_backup_003_reopen_cleans_partial_archive_and_marks_interrupted_backup_failed() {
    let fixture = Fixture::new(true);
    let archive = fixture
        .data_directory
        .join("backups/instances/instance-backup/interrupted.zip");
    fs::create_dir_all(archive.parent().unwrap()).unwrap();
    let partial = PathBuf::from(format!("{}.partial", archive.to_string_lossy()));
    fs::write(&partial, b"incomplete-zip").unwrap();
    let connection = Connection::open(&fixture.database_path).unwrap();
    connection
        .execute(
            "INSERT INTO world_backups (id, instance_id, instance_name, launch_session_id, trigger, state, archive_path, world_count, source_bytes, archive_bytes, created_at_unix_seconds) VALUES ('backup-interrupted', 'instance-backup', '备份测试', 'session-interrupted', 'post_exit', 'staging', ?1, 1, 12, 0, 1)",
            params![path_text(&archive)],
        )
        .unwrap();
    drop(connection);

    let reopened = fixture.reopen();
    let backup = reopened
        .list_world_backups(Some("instance-backup"))
        .unwrap()
        .remove(0);

    assert_eq!(backup.state, BackupState::Failed);
    assert!(
        backup
            .error_summary
            .is_some_and(|message| message.contains("中断"))
    );
    assert!(!partial.exists());
    assert!(!archive.exists());
}

#[test]
fn m8_backup_004_default_retention_keeps_only_twenty_successful_archives() {
    let fixture = Fixture::new(true);
    let mut archive_paths = Vec::new();
    for index in 0..21 {
        fs::write(
            fixture
                .instance_directory
                .join(".minecraft/saves/First World/level.dat"),
            format!("snapshot-{index}"),
        )
        .unwrap();
        let backup = fixture
            .service
            .create_world_backup("instance-backup", BackupTrigger::Manual, None)
            .unwrap();
        archive_paths.push(PathBuf::from(backup.archive_path.unwrap()));
    }

    let history = fixture
        .service
        .list_world_backups(Some("instance-backup"))
        .unwrap();
    assert_eq!(
        history
            .iter()
            .filter(|backup| backup.state == BackupState::Ready)
            .count(),
        20
    );
    assert_eq!(
        archive_paths.iter().filter(|path| path.exists()).count(),
        20
    );
    assert!(fixture.data_directory.join("store").is_dir());
}

fn zip_text(path: &str, name: &str) -> Vec<u8> {
    let file = fs::File::open(path).unwrap();
    let mut archive = ZipArchive::new(file).unwrap();
    let mut entry = archive.by_name(name).unwrap();
    let mut content = Vec::new();
    entry.read_to_end(&mut content).unwrap();
    content
}

fn path_text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}
