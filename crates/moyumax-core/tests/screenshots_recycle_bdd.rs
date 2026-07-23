use std::{fs, path::PathBuf};

use moyumax_core::{AppService, InstanceResourceKind, RecycleItemKind, RecycleItemState};
use rusqlite::{Connection, params};
use tempfile::TempDir;

#[test]
fn m18_shot_001_screenshot_listing_matches_the_file_system() {
    let fixture = RecycleFixture::new();
    fixture.write_screenshot("2026-07-21_23.12.05.png", b"shot-a");
    fixture.write_screenshot("2026-07-22_08.01.33.png", b"shot-b");
    fixture.write_screenshot("notes.txt", b"not-a-screenshot");

    let screenshots = fixture
        .service
        .list_instance_screenshots(&fixture.instance_id)
        .unwrap();

    assert_eq!(screenshots.len(), 2);
    assert_eq!(screenshots[0].file_name, "2026-07-22_08.01.33.png");
    assert_eq!(screenshots[1].file_name, "2026-07-21_23.12.05.png");
    assert_eq!(screenshots[0].size_bytes, 6);
    assert!(screenshots[0].taken_at_unix_seconds > 0);
}

#[test]
fn m18_shot_002_screenshot_delete_and_restore_roundtrip() {
    let fixture = RecycleFixture::new();
    fixture.write_screenshot("2026-07-22_08.01.33.png", b"shot-b");
    let original = fixture.screenshots_dir().join("2026-07-22_08.01.33.png");

    let item = fixture
        .service
        .delete_instance_screenshot(&fixture.instance_id, "2026-07-22_08.01.33.png")
        .unwrap();

    assert_eq!(item.kind, RecycleItemKind::Screenshot);
    assert_eq!(item.state, RecycleItemState::Ready);
    assert!(!original.exists());
    assert_eq!(
        fs::read(&item.recycled_path).unwrap(),
        b"shot-b",
        "截图必须完整进入回收站"
    );

    fixture.service.restore_recycled_entry(&item.id).unwrap();
    assert_eq!(fs::read(&original).unwrap(), b"shot-b");
    assert!(
        fixture.service.list_recycle_bin_items().unwrap().is_empty(),
        "恢复后回收站不应保留该项目"
    );
}

#[test]
fn m18_shot_003_restore_refuses_to_overwrite_occupied_path() {
    let fixture = RecycleFixture::new();
    fixture.write_screenshot("2026-07-22_08.01.33.png", b"shot-b");
    let original = fixture.screenshots_dir().join("2026-07-22_08.01.33.png");
    let item = fixture
        .service
        .delete_instance_screenshot(&fixture.instance_id, "2026-07-22_08.01.33.png")
        .unwrap();
    fs::write(&original, b"new-screenshot").unwrap();

    let error = fixture
        .service
        .restore_recycled_entry(&item.id)
        .expect_err("原位置被占用时必须拒绝恢复");

    assert!(error.to_string().contains("已被占用"));
    assert_eq!(fs::read(&original).unwrap(), b"new-screenshot");
    assert_eq!(fs::read(&item.recycled_path).unwrap(), b"shot-b");
}

#[test]
fn m18_res_001_resource_delete_restore_keeps_index_and_disabled_state() {
    let fixture = RecycleFixture::new();
    let source = fixture.directory.path().join("faithful.zip");
    fs::write(&source, b"pack-bytes").unwrap();
    let resource = fixture
        .service
        .import_instance_resource(
            &fixture.instance_id,
            InstanceResourceKind::ResourcePack,
            &source,
            None,
        )
        .unwrap();
    fixture
        .service
        .set_instance_resource_enabled(&resource.id, false)
        .unwrap();
    let disabled_path = fixture
        .instance_root
        .join(".minecraft/resourcepacks/faithful.zip.disabled");

    let item = fixture
        .service
        .delete_instance_resource(&resource.id)
        .unwrap();

    assert_eq!(item.kind, RecycleItemKind::Resource);
    assert!(!disabled_path.exists());
    assert!(
        fixture
            .service
            .list_instance_resources(&fixture.instance_id, None)
            .unwrap()
            .is_empty(),
        "删除后索引行必须移除"
    );
    assert!(item.payload.is_some(), "资源回收项目必须携带索引负载");

    fixture.service.restore_recycled_entry(&item.id).unwrap();

    assert!(
        disabled_path.exists(),
        "停用资源必须以 .disabled 名称回到原目录"
    );
    let restored = fixture
        .service
        .list_instance_resources(&fixture.instance_id, None)
        .unwrap();
    assert_eq!(restored.len(), 1);
    assert!(!restored[0].enabled, "恢复后索引必须保留停用状态");
    assert_eq!(restored[0].id, resource.id);
}

#[test]
fn m18_world_001_world_delete_and_restore_roundtrip() {
    let fixture = RecycleFixture::new();
    fixture.create_world(
        "alpha",
        &[("level.dat", b"level"), ("region/r.0.0.mca", b"chunk")],
    );

    let item = fixture
        .service
        .delete_instance_world(&fixture.instance_id, "alpha")
        .unwrap();

    assert_eq!(item.kind, RecycleItemKind::World);
    assert!(
        !fixture
            .instance_root
            .join(".minecraft/saves/alpha")
            .exists()
    );
    assert!(
        PathBuf::from(&item.recycled_path)
            .join("level.dat")
            .exists()
    );

    fixture.service.restore_recycled_entry(&item.id).unwrap();

    let restored = fixture.instance_root.join(".minecraft/saves/alpha");
    assert_eq!(fs::read(restored.join("level.dat")).unwrap(), b"level");
    assert_eq!(
        fs::read(restored.join("region/r.0.0.mca")).unwrap(),
        b"chunk"
    );
}

#[test]
fn m18_purge_001_entries_can_be_permanently_deleted() {
    let fixture = RecycleFixture::new();
    fixture.write_screenshot("2026-07-22_08.01.33.png", b"shot-b");
    let item = fixture
        .service
        .delete_instance_screenshot(&fixture.instance_id, "2026-07-22_08.01.33.png")
        .unwrap();

    let result = fixture.service.purge_recycle_bin_item(&item.id).unwrap();

    assert_eq!(result.removed_subjects, 1);
    assert!(!PathBuf::from(&item.recycled_path).exists());
    assert!(fixture.service.list_recycle_bin_items().unwrap().is_empty());
}

#[test]
fn m18_recover_001_interrupted_move_converges_on_restart() {
    let fixture = RecycleFixture::new();
    fixture.write_screenshot("2026-07-22_08.01.33.png", b"shot-b");
    let item = fixture
        .service
        .delete_instance_screenshot(&fixture.instance_id, "2026-07-22_08.01.33.png")
        .unwrap();
    // 模拟中断：文件已移走但状态停留在 moving。
    Connection::open(&fixture.database_path)
        .unwrap()
        .execute(
            "UPDATE recycle_bin_items SET state = 'moving' WHERE id = ?1",
            params![item.id],
        )
        .unwrap();

    let reopened = AppService::open(&fixture.database_path, &fixture.data_directory).unwrap();

    let items = reopened.list_recycle_bin_items().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].state, RecycleItemState::Ready);
    assert!(PathBuf::from(&item.recycled_path).exists());
    assert!(
        !fixture
            .screenshots_dir()
            .join("2026-07-22_08.01.33.png")
            .exists()
    );
}

struct RecycleFixture {
    directory: TempDir,
    database_path: PathBuf,
    data_directory: PathBuf,
    instance_id: String,
    instance_root: PathBuf,
    service: AppService,
}

impl RecycleFixture {
    fn new() -> Self {
        let directory = TempDir::new().unwrap();
        let database_path = directory.path().join("state.sqlite3");
        let data_directory = directory.path().join("data");
        let instance_id = "instance-id".to_owned();
        let instance_root = data_directory.join("instances").join(&instance_id);
        let service = AppService::open(&database_path, &data_directory).unwrap();
        service.skip_onboarding().unwrap();
        fs::create_dir_all(instance_root.join(".minecraft/screenshots")).unwrap();
        Connection::open(&database_path)
            .unwrap()
            .execute(
                "
                INSERT INTO instances (
                    id, name, game_version, loader_kind, loader_version,
                    root_directory, state, created_at_unix_seconds
                ) VALUES (?1, '回收测试', '26.2', 'fabric', '0.19.3', ?2, 'ready', 1)
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

    fn screenshots_dir(&self) -> PathBuf {
        self.instance_root.join(".minecraft/screenshots")
    }

    fn write_screenshot(&self, name: &str, bytes: &[u8]) {
        fs::create_dir_all(self.screenshots_dir()).unwrap();
        fs::write(self.screenshots_dir().join(name), bytes).unwrap();
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
