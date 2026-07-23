use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

use moyumax_core::{AppService, BackupState, BackupTrigger};
use rusqlite::{Connection, params};
use tempfile::TempDir;
use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

#[test]
fn m17_world_001_world_details_match_the_file_system() {
    let fixture = WorldFixture::new();
    fixture.create_world(
        "alpha",
        &[
            ("level.dat", b"level-alpha"),
            ("region/r.0.0.mca", b"chunk"),
        ],
    );
    fixture.create_world("beta", &[("level.dat", b"level-beta")]);

    let worlds = fixture
        .service
        .list_instance_world_details(&fixture.instance_id)
        .unwrap();

    assert_eq!(worlds.len(), 2);
    assert_eq!(worlds[0].name, "alpha");
    assert_eq!(worlds[1].name, "beta");
    assert!(worlds[0].size_bytes >= 16);
    assert!(worlds[0].last_played_unix_seconds.is_some());
}

#[test]
fn m17_world_002_exported_world_can_be_imported_into_another_instance() {
    let fixture = WorldFixture::new();
    fixture.create_world(
        "alpha",
        &[
            ("level.dat", b"level-alpha"),
            ("data/scoreboard.dat", b"score"),
        ],
    );
    let second_root = fixture.create_instance("second-instance");
    let export_path = fixture.directory.path().join("alpha.zip");

    let bytes = fixture
        .service
        .export_instance_world(&fixture.instance_id, "alpha", &export_path)
        .unwrap();

    assert!(bytes > 0);
    let listing = ZipArchive::new(fs::File::open(&export_path).unwrap()).unwrap();
    let names = (0..listing.len())
        .map(|index| listing.name_for_index(index).unwrap().to_owned())
        .collect::<Vec<_>>();
    assert!(names.contains(&"alpha/level.dat".to_owned()));
    assert!(names.contains(&"alpha/data/scoreboard.dat".to_owned()));

    let imported = fixture
        .service
        .import_instance_world("second-instance", &export_path)
        .unwrap();

    assert_eq!(imported.name, "alpha");
    let target = second_root.join(".minecraft/saves/alpha");
    assert_eq!(fs::read(target.join("level.dat")).unwrap(), b"level-alpha");
    assert_eq!(
        fs::read(target.join("data/scoreboard.dat")).unwrap(),
        b"score"
    );
}

#[test]
fn m17_world_003_same_name_import_is_rejected_without_overwrite() {
    let fixture = WorldFixture::new();
    fixture.create_world("alpha", &[("level.dat", b"original")]);
    let source = fixture.directory.path().join("alpha.zip");
    write_zip(&source, &[("alpha/level.dat", b"attacker")]);

    let error = fixture
        .service
        .import_instance_world(&fixture.instance_id, &source)
        .expect_err("同名世界必须拒绝导入");

    assert!(error.to_string().contains("已拒绝导入"));
    assert_eq!(
        fs::read(
            fixture
                .instance_root
                .join(".minecraft/saves/alpha/level.dat")
        )
        .unwrap(),
        b"original"
    );
}

#[test]
fn m17_world_004_rollback_restores_saves_and_keeps_a_recovery_point() {
    let fixture = WorldFixture::new();
    fixture.create_world("alpha", &[("level.dat", b"level-alpha")]);
    let backup = fixture
        .service
        .create_world_backup(&fixture.instance_id, BackupTrigger::Manual, None)
        .unwrap();
    assert_eq!(backup.state, BackupState::Ready);
    // 备份后继续游玩：修改既有世界并新增世界。
    fs::write(
        fixture
            .instance_root
            .join(".minecraft/saves/alpha/level.dat"),
        b"level-alpha-day2",
    )
    .unwrap();
    fixture.create_world("beta", &[("level.dat", b"level-beta")]);

    let recovery = fixture.service.rollback_world_backup(&backup.id).unwrap();

    assert_eq!(
        fs::read(
            fixture
                .instance_root
                .join(".minecraft/saves/alpha/level.dat")
        )
        .unwrap(),
        b"level-alpha",
        "回滚后 saves 必须回到备份时状态"
    );
    assert!(
        !fixture.instance_root.join(".minecraft/saves/beta").exists(),
        "备份后新增的世界应随回滚移除"
    );
    assert_eq!(recovery.state, BackupState::Ready);
    assert_eq!(recovery.world_count, 2, "恢复点必须捕获回滚前的两个世界");
    let backups = fixture.service.list_world_backups(None).unwrap();
    assert_eq!(backups.len(), 2);
    assert!(
        !fixture
            .instance_root
            .join(".moyumax/rollback-old-saves")
            .exists(),
        "交换完成后旧 saves 必须清理"
    );
}

#[test]
fn m17_world_005_rollback_aborts_when_recovery_point_fails() {
    let fixture = WorldFixture::new();
    fixture.create_world("alpha", &[("level.dat", b"level-alpha")]);
    let backup = fixture
        .service
        .create_world_backup(&fixture.instance_id, BackupTrigger::Manual, None)
        .unwrap();
    fs::write(
        fixture
            .instance_root
            .join(".minecraft/saves/alpha/level.dat"),
        b"level-alpha-day2",
    )
    .unwrap();
    Connection::open(&fixture.database_path)
        .unwrap()
        .execute_batch(
            "
            CREATE TRIGGER fixture_reject_world_backups
            BEFORE INSERT ON world_backups
            BEGIN
                SELECT RAISE(ABORT, 'fixture backup failure');
            END;
            ",
        )
        .unwrap();

    let error = fixture
        .service
        .rollback_world_backup(&backup.id)
        .expect_err("恢复点失败必须中止回滚");

    assert!(error.to_string().contains("fixture backup failure"));
    assert_eq!(
        fs::read(
            fixture
                .instance_root
                .join(".minecraft/saves/alpha/level.dat")
        )
        .unwrap(),
        b"level-alpha-day2",
        "中止回滚时 saves 必须保持不变"
    );
}

#[test]
fn m17_world_006_interrupted_rollback_swap_is_converged_on_restart() {
    let fixture = WorldFixture::new();
    fixture.create_world("alpha", &[("level.dat", b"original")]);
    // 模拟交换中段退出：saves 已改名，暂存未落位。
    let staging = fixture.instance_root.join(".moyumax/rollback-staging");
    fs::create_dir_all(&staging).unwrap();
    fs::write(staging.join("placeholder.txt"), b"half").unwrap();
    fs::rename(
        fixture.instance_root.join(".minecraft/saves"),
        fixture.instance_root.join(".moyumax/rollback-old-saves"),
    )
    .unwrap();

    let reopened = AppService::open(&fixture.database_path, &fixture.data_directory).unwrap();
    drop(reopened);

    assert_eq!(
        fs::read(
            fixture
                .instance_root
                .join(".minecraft/saves/alpha/level.dat")
        )
        .unwrap(),
        b"original",
        "重启后必须恢复原 saves"
    );
    assert!(!staging.exists(), "中断暂存必须清理");
}

#[test]
fn m17_world_007_unsafe_zip_entries_are_rejected() {
    let fixture = WorldFixture::new();
    let evil = fixture.directory.path().join("evil.zip");
    write_zip(
        &evil,
        &[("../escape.txt", b"evil"), ("level.dat", b"level")],
    );

    let error = fixture
        .service
        .import_instance_world(&fixture.instance_id, &evil)
        .expect_err("路径穿越条目必须被拒绝");

    assert!(error.to_string().contains("不安全"));
    assert!(
        !fixture.directory.path().join("escape.txt").exists(),
        "不得有任何文件被解压到目标之外"
    );
}

#[test]
fn m17_world_008_root_level_dat_layout_is_accepted() {
    let fixture = WorldFixture::new();
    let source = fixture.directory.path().join("skyblock.zip");
    write_zip(
        &source,
        &[("level.dat", b"level-root"), ("region/r.0.0.mca", b"chunk")],
    );

    let imported = fixture
        .service
        .import_instance_world(&fixture.instance_id, &source)
        .unwrap();

    assert_eq!(imported.name, "skyblock");
    assert_eq!(
        fs::read(
            fixture
                .instance_root
                .join(".minecraft/saves/skyblock/level.dat")
        )
        .unwrap(),
        b"level-root"
    );
}

struct WorldFixture {
    directory: TempDir,
    database_path: PathBuf,
    data_directory: PathBuf,
    instance_id: String,
    instance_root: PathBuf,
    service: AppService,
}

impl WorldFixture {
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
                ) VALUES (?1, '世界测试', '26.2', 'fabric', '0.19.3', ?2, 'ready', 1)
                ",
                params![instance_id, instance_root.to_string_lossy()],
            )
            .unwrap();
        Self {
            directory,
            database_path,
            data_directory,
            instance_id,
            instance_root,
            service,
        }
    }

    fn create_instance(&self, instance_id: &str) -> PathBuf {
        let instance_root = self.data_directory.join("instances").join(instance_id);
        fs::create_dir_all(instance_root.join(".minecraft/saves")).unwrap();
        Connection::open(&self.database_path)
            .unwrap()
            .execute(
                "
                INSERT INTO instances (
                    id, name, game_version, loader_kind, loader_version,
                    root_directory, state, created_at_unix_seconds
                ) VALUES (?1, '第二实例', '26.2', 'quilt', '0.29.0', ?2, 'ready', 1)
                ",
                params![instance_id, instance_root.to_string_lossy()],
            )
            .unwrap();
        instance_root
    }

    fn create_world(&self, name: &str, files: &[(&str, &[u8])]) {
        let world = self.instance_root.join(".minecraft/saves").join(name);
        for (relative, bytes) in files {
            let path = world.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, bytes).unwrap();
        }
    }
}

fn write_zip(destination: &Path, files: &[(&str, &[u8])]) {
    let file = fs::File::create(destination).unwrap();
    let mut writer = ZipWriter::new(file);
    let options = SimpleFileOptions::default();
    for (name, bytes) in files {
        writer.start_file(name, options).unwrap();
        writer.write_all(bytes).unwrap();
    }
    writer.finish().unwrap();
}
