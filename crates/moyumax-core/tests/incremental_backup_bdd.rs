use std::{fs, path::PathBuf};

use moyumax_core::{AppService, BackupKind, BackupState, BackupTrigger};
use rusqlite::{Connection, params};
use tempfile::TempDir;
use zip::ZipArchive;

#[test]
fn m19_inc_001_scheduled_incremental_contains_only_changed_files_and_deletions() {
    let fixture = BackupFixture::new();
    fixture.write_world_file("alpha", "level.dat", b"level-v1");
    fixture.write_world_file("alpha", "keep.txt", b"keep");
    fixture.write_world_file("alpha", "old.txt", b"old");
    let full = fixture.manual_backup();
    // 变化：修改 level.dat、新增 new.txt、删除 old.txt；keep.txt 不变。
    fixture.write_world_file("alpha", "level.dat", b"level-v2-changed");
    fixture.write_world_file("alpha", "new.txt", b"new");
    fs::remove_file(fixture.instance_root.join(".minecraft/saves/alpha/old.txt")).unwrap();

    let incremental = fixture
        .service
        .create_scheduled_world_backup(&fixture.instance_id)
        .unwrap();

    assert_eq!(incremental.kind, BackupKind::Incremental);
    assert_eq!(incremental.trigger, BackupTrigger::Scheduled);
    assert_eq!(
        incremental.base_backup_id.as_deref(),
        Some(full.id.as_str())
    );
    let names = fixture.archive_names(&incremental);
    assert!(names.contains(&"manifest.json".to_owned()));
    assert!(names.contains(&"worlds/alpha/level.dat".to_owned()));
    assert!(names.contains(&"worlds/alpha/new.txt".to_owned()));
    assert!(
        !names.contains(&"worlds/alpha/keep.txt".to_owned()),
        "未变化文件不得进入增量备份"
    );
    let manifest = fixture.read_manifest(&incremental);
    assert_eq!(manifest["kind"], "incremental");
    assert_eq!(manifest["baseBackupId"], full.id.as_str());
    assert_eq!(manifest["deleted"], serde_json::json!(["alpha/old.txt"]));
    assert!(
        manifest["files"]
            .as_array()
            .unwrap()
            .iter()
            .any(|file| file["path"] == "alpha/keep.txt"),
        "清单必须包含完整文件索引供下一次增量对比"
    );
}

#[test]
fn m19_inc_002_scheduled_without_usable_base_falls_back_to_full() {
    let fixture = BackupFixture::new();
    fixture.write_world_file("alpha", "level.dat", b"level-v1");

    let backup = fixture
        .service
        .create_scheduled_world_backup(&fixture.instance_id)
        .unwrap();

    assert_eq!(backup.kind, BackupKind::Full);
    assert_eq!(backup.trigger, BackupTrigger::Scheduled);
    assert_eq!(backup.state, BackupState::Ready);
}

#[test]
fn m19_inc_003_rollback_to_incremental_matches_that_point_in_time() {
    let fixture = BackupFixture::new();
    fixture.write_world_file("alpha", "level.dat", b"level-v1");
    fixture.write_world_file("alpha", "keep.txt", b"keep");
    fixture.write_world_file("alpha", "old.txt", b"old");
    fixture.manual_backup();
    // 增量 1 时点：修改（尺寸变化确保差异检测）、新增、删除。
    fixture.write_world_file("alpha", "level.dat", b"level-v2-larger");
    fixture.write_world_file("alpha", "new.txt", b"new");
    fs::remove_file(fixture.instance_root.join(".minecraft/saves/alpha/old.txt")).unwrap();
    let inc1 = fixture
        .service
        .create_scheduled_world_backup(&fixture.instance_id)
        .unwrap();
    assert_eq!(inc1.kind, BackupKind::Incremental);
    // 增量 2 时点：进一步变化（回滚后不得出现）。
    fixture.write_world_file("alpha", "level.dat", b"level-v3");
    fixture.write_world_file("alpha", "later.txt", b"later");
    let inc2 = fixture
        .service
        .create_scheduled_world_backup(&fixture.instance_id)
        .unwrap();
    assert_eq!(inc2.base_backup_id.as_deref(), Some(inc1.id.as_str()));

    fixture.service.rollback_world_backup(&inc1.id).unwrap();

    let saves = fixture.instance_root.join(".minecraft/saves/alpha");
    assert_eq!(fs::read(saves.join("level.dat")).unwrap(), b"level-v2-larger");
    assert_eq!(fs::read(saves.join("keep.txt")).unwrap(), b"keep");
    assert_eq!(fs::read(saves.join("new.txt")).unwrap(), b"new");
    assert!(!saves.join("old.txt").exists(), "增量前删除的文件不得重现");
    assert!(
        !saves.join("later.txt").exists(),
        "增量后的新增不得提前出现"
    );
}

#[test]
fn m19_inc_004_no_changes_produces_no_new_backup() {
    let fixture = BackupFixture::new();
    fixture.write_world_file("alpha", "level.dat", b"level-v1");
    let full = fixture.manual_backup();

    let result = fixture
        .service
        .create_scheduled_world_backup(&fixture.instance_id)
        .unwrap();

    assert_eq!(result.id, full.id, "没有变化时不得产生新备份");
    assert_eq!(fixture.service.list_world_backups(None).unwrap().len(), 1);
}

#[test]
fn m19_inc_005_keep_count_prunes_without_dangling_chains() {
    let fixture = BackupFixture::new();
    fixture.service.set_world_backup_keep_count(2).unwrap();
    fixture.write_world_file("alpha", "level.dat", b"v1");
    let full1 = fixture.manual_backup();
    fixture.write_world_file("alpha", "level.dat", b"v2");
    let inc2 = fixture
        .service
        .create_scheduled_world_backup(&fixture.instance_id)
        .unwrap();
    fixture.write_world_file("alpha", "level.dat", b"v3");
    let full3 = fixture.manual_backup();
    fixture.write_world_file("alpha", "level.dat", b"v4");
    let full4 = fixture.manual_backup();

    let remaining = fixture.service.list_world_backups(None).unwrap();
    let remaining_ids = remaining
        .iter()
        .map(|backup| backup.id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(remaining.len(), 2, "清理必须收敛到保留数量");
    assert!(remaining_ids.contains(&full3.id.as_str()));
    assert!(remaining_ids.contains(&full4.id.as_str()));
    assert!(
        !remaining_ids.contains(&full1.id.as_str()) && !remaining_ids.contains(&inc2.id.as_str()),
        "最旧的链段应整体移除"
    );
    for backup in &remaining {
        if let Some(base_id) = &backup.base_backup_id {
            assert!(remaining_ids.contains(&base_id.as_str()), "恢复链不得悬空");
        }
    }
    assert!(
        !PathBuf::from(full1.archive_path.unwrap()).exists()
            && !PathBuf::from(inc2.archive_path.unwrap()).exists(),
        "被清理备份的归档文件必须删除"
    );
}

#[test]
fn m19_inc_006_settings_persist_and_validate_bounds() {
    let fixture = BackupFixture::new();
    assert_eq!(fixture.service.world_backup_interval_minutes().unwrap(), 30);
    assert_eq!(fixture.service.world_backup_keep_count().unwrap(), 20);

    fixture
        .service
        .set_world_backup_interval_minutes(5)
        .unwrap();
    fixture.service.set_world_backup_keep_count(3).unwrap();
    let reopened = AppService::open(&fixture.database_path, &fixture.data_directory).unwrap();
    assert_eq!(reopened.world_backup_interval_minutes().unwrap(), 5);
    assert_eq!(reopened.world_backup_keep_count().unwrap(), 3);

    assert!(
        reopened.set_world_backup_interval_minutes(1441).is_err(),
        "间隔超过 1440 分钟必须被拒绝"
    );
    assert!(reopened.set_world_backup_keep_count(0).is_err());
    assert!(reopened.set_world_backup_keep_count(101).is_err());
}

#[tokio::test]
async fn m19_inc_007_scheduler_exits_immediately_when_interval_disabled() {
    let fixture = BackupFixture::new();
    fixture
        .service
        .set_world_backup_interval_minutes(0)
        .unwrap();
    moyumax_core::spawn_scheduled_world_backups(fixture.service.clone(), "session-x".to_owned());
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    assert!(
        fixture.service.list_world_backups(None).unwrap().is_empty(),
        "间隔为 0 时调度不得产生任何备份"
    );
}

struct BackupFixture {
    _directory: TempDir,
    database_path: PathBuf,
    data_directory: PathBuf,
    instance_id: String,
    instance_root: PathBuf,
    service: AppService,
}

impl BackupFixture {
    fn new() -> Self {
        let directory = TempDir::new().unwrap();
        let database_path = directory.path().join("state.sqlite3");
        let data_directory = directory.path().join("data");
        let instance_id = "instance-id".to_owned();
        let instance_root = data_directory.join("instances").join(&instance_id);
        let service = AppService::open(&database_path, &data_directory).unwrap();
        service.skip_onboarding().unwrap();
        fs::create_dir_all(instance_root.join(".minecraft/saves")).unwrap();
        Connection::open(&database_path)
            .unwrap()
            .execute(
                "
                INSERT INTO instances (
                    id, name, game_version, loader_kind, loader_version,
                    root_directory, state, created_at_unix_seconds
                ) VALUES (?1, '备份测试', '26.2', 'fabric', '0.19.3', ?2, 'ready', 1)
                ",
                params![instance_id, instance_root.to_string_lossy()],
            )
            .unwrap();
        Self {
            _directory: directory,
            database_path,
            data_directory,
            instance_id,
            instance_root,
            service,
        }
    }

    fn write_world_file(&self, world: &str, name: &str, bytes: &[u8]) {
        let path = self
            .instance_root
            .join(".minecraft/saves")
            .join(world)
            .join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }

    fn manual_backup(&self) -> moyumax_core::WorldBackupSummary {
        self.service
            .create_world_backup(&self.instance_id, BackupTrigger::Manual, None)
            .unwrap()
    }

    fn archive_names(&self, backup: &moyumax_core::WorldBackupSummary) -> Vec<String> {
        let archive =
            ZipArchive::new(fs::File::open(backup.archive_path.as_ref().unwrap()).unwrap())
                .unwrap();
        (0..archive.len())
            .map(|index| archive.name_for_index(index).unwrap().to_owned())
            .collect()
    }

    fn read_manifest(&self, backup: &moyumax_core::WorldBackupSummary) -> serde_json::Value {
        let mut archive =
            ZipArchive::new(fs::File::open(backup.archive_path.as_ref().unwrap()).unwrap())
                .unwrap();
        let mut entry = archive.by_name("manifest.json").unwrap();
        let mut buffer = Vec::new();
        std::io::copy(&mut entry, &mut buffer).unwrap();
        serde_json::from_slice(&buffer).unwrap()
    }
}
