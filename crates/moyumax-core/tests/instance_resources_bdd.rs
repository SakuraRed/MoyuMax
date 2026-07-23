use std::{
    fs,
    path::{Path, PathBuf},
};

use moyumax_core::{AppService, InstanceResourceKind};
use rusqlite::{Connection, params};
use sha2::{Digest, Sha256};
use tempfile::TempDir;

#[test]
fn m16_res_001_import_resource_pack_publishes_file_and_index_atomically() {
    let fixture = ResourceFixture::new();
    let source = fixture.write_source("jei-items.zip", b"resource-pack-bytes");

    let resource = fixture
        .service
        .import_instance_resource(
            &fixture.instance_id,
            InstanceResourceKind::ResourcePack,
            &source,
            None,
        )
        .expect("资源包导入必须成功");

    let target = fixture
        .instance_root
        .join(".minecraft/resourcepacks/jei-items.zip");
    assert_eq!(fs::read(&target).unwrap(), b"resource-pack-bytes");
    assert!(resource.enabled);
    assert_eq!(resource.display_name, "jei-items");
    assert_eq!(
        resource.relative_path,
        ".minecraft/resourcepacks/jei-items.zip"
    );
    assert_eq!(resource.sha256, sha256_hex(b"resource-pack-bytes"));
    let listed = fixture
        .service
        .list_instance_resources(&fixture.instance_id, None)
        .unwrap();
    assert_eq!(listed, vec![resource]);
    assert!(
        staging_is_empty(&fixture.data_directory),
        "导入成功后暂存必须清理"
    );
}

#[test]
fn m16_res_002_same_name_import_is_rejected_without_overwrite() {
    let fixture = ResourceFixture::new();
    fixture
        .service
        .import_instance_resource(
            &fixture.instance_id,
            InstanceResourceKind::ResourcePack,
            &fixture.write_source("pack.zip", b"original-pack"),
            None,
        )
        .unwrap();
    let target = fixture
        .instance_root
        .join(".minecraft/resourcepacks/pack.zip");
    let attacker = fixture.write_source("pack.zip", b"attacker-pack");

    let error = fixture
        .service
        .import_instance_resource(
            &fixture.instance_id,
            InstanceResourceKind::ResourcePack,
            &attacker,
            None,
        )
        .expect_err("同名导入必须被拒绝");

    assert!(error.to_string().contains("已拒绝导入"));
    assert_eq!(fs::read(&target).unwrap(), b"original-pack");
    assert_eq!(
        fixture
            .service
            .list_instance_resources(&fixture.instance_id, None)
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn m16_res_003_index_failure_removes_the_published_file() {
    let fixture = ResourceFixture::new();
    let source = fixture.write_source("complementary.zip", b"shader-bytes");
    Connection::open(&fixture.database_path)
        .unwrap()
        .execute_batch(
            "
            CREATE TRIGGER fixture_reject_instance_resources
            BEFORE INSERT ON instance_resources
            BEGIN
                SELECT RAISE(ABORT, 'fixture resource index failure');
            END;
            ",
        )
        .unwrap();

    let error = fixture
        .service
        .import_instance_resource(
            &fixture.instance_id,
            InstanceResourceKind::Shader,
            &source,
            None,
        )
        .expect_err("索引写入失败必须中止导入");

    assert!(error.to_string().contains("fixture resource index failure"));
    assert!(
        !fixture
            .instance_root
            .join(".minecraft/shaderpacks/complementary.zip")
            .exists(),
        "索引失败时实例目录不得留下文件"
    );
    assert!(
        fixture
            .service
            .list_instance_resources(&fixture.instance_id, None)
            .unwrap()
            .is_empty()
    );
    assert!(staging_is_empty(&fixture.data_directory));
}

fn staging_is_empty(data_directory: &Path) -> bool {
    let staging = data_directory.join(".staging/resources");
    !staging.exists() || fs::read_dir(&staging).unwrap().next().is_none()
}

#[test]
fn m16_res_004_enable_disable_toggles_disk_suffix_with_compensation() {
    let fixture = ResourceFixture::new();
    let resource = fixture
        .service
        .import_instance_resource(
            &fixture.instance_id,
            InstanceResourceKind::ResourcePack,
            &fixture.write_source("pack.zip", b"pack-bytes"),
            None,
        )
        .unwrap();
    let enabled_path = fixture
        .instance_root
        .join(".minecraft/resourcepacks/pack.zip");
    let disabled_path = fixture
        .instance_root
        .join(".minecraft/resourcepacks/pack.zip.disabled");

    let disabled = fixture
        .service
        .set_instance_resource_enabled(&resource.id, false)
        .unwrap();
    assert!(!disabled.enabled);
    assert!(!enabled_path.exists());
    assert!(disabled_path.exists());

    let enabled = fixture
        .service
        .set_instance_resource_enabled(&resource.id, true)
        .unwrap();
    assert!(enabled.enabled);
    assert!(enabled_path.exists());
    assert!(!disabled_path.exists());

    // 索引更新失败时文件名必须回滚，状态不得漂移。
    Connection::open(&fixture.database_path)
        .unwrap()
        .execute_batch(
            "
            CREATE TRIGGER fixture_reject_resource_update
            BEFORE UPDATE ON instance_resources
            BEGIN
                SELECT RAISE(ABORT, 'fixture update failure');
            END;
            ",
        )
        .unwrap();
    let error = fixture
        .service
        .set_instance_resource_enabled(&resource.id, false)
        .expect_err("索引失败必须中止切换");
    assert!(error.to_string().contains("fixture update failure"));
    assert!(enabled_path.exists());
    assert!(!disabled_path.exists());
    let listed = fixture
        .service
        .list_instance_resources(&fixture.instance_id, None)
        .unwrap();
    assert!(listed[0].enabled, "索引状态必须与文件系统一致");
}

#[test]
fn m16_res_005_datapack_requires_and_targets_only_the_selected_world() {
    let fixture = ResourceFixture::new();
    fixture.create_world("world-a");
    fixture.create_world("world-b");
    let source = fixture.write_source("tweaks.zip", b"datapack-bytes");

    let error = fixture
        .service
        .import_instance_resource(
            &fixture.instance_id,
            InstanceResourceKind::Datapack,
            &source,
            None,
        )
        .expect_err("未选择世界不得导入数据包");
    assert!(error.to_string().contains("选择目标世界"));
    let error = fixture
        .service
        .import_instance_resource(
            &fixture.instance_id,
            InstanceResourceKind::Datapack,
            &source,
            Some("missing-world"),
        )
        .expect_err("不存在的世界不得作为目标");
    assert!(error.to_string().contains("不存在"));

    let resource = fixture
        .service
        .import_instance_resource(
            &fixture.instance_id,
            InstanceResourceKind::Datapack,
            &source,
            Some("world-b"),
        )
        .unwrap();

    assert_eq!(resource.world_name.as_deref(), Some("world-b"));
    assert!(
        fixture
            .instance_root
            .join(".minecraft/saves/world-b/datapacks/tweaks.zip")
            .exists()
    );
    assert!(
        !fixture
            .instance_root
            .join(".minecraft/saves/world-a/datapacks/tweaks.zip")
            .exists(),
        "数据包不得进入未选择的世界"
    );
    let worlds = fixture
        .service
        .list_instance_worlds(&fixture.instance_id)
        .unwrap();
    assert_eq!(worlds, vec!["world-a".to_owned(), "world-b".to_owned()]);
}

#[test]
fn m16_res_006_instances_are_fully_isolated() {
    let fixture = ResourceFixture::new();
    let second_root = fixture.create_instance("second-instance");
    fixture
        .service
        .import_instance_resource(
            &fixture.instance_id,
            InstanceResourceKind::ResourcePack,
            &fixture.write_source("pack.zip", b"pack-bytes"),
            None,
        )
        .unwrap();

    assert!(
        fixture
            .service
            .list_instance_resources("second-instance", None)
            .unwrap()
            .is_empty(),
        "实例乙的清单不得出现实例甲的资源"
    );
    assert!(
        !second_root
            .join(".minecraft/resourcepacks/pack.zip")
            .exists(),
        "实例乙的目录不得出现实例甲的文件"
    );
}

#[test]
fn m16_res_007_kind_filter_and_filename_validation() {
    let fixture = ResourceFixture::new();
    fixture
        .service
        .import_instance_resource(
            &fixture.instance_id,
            InstanceResourceKind::Shader,
            &fixture.write_source("bsl.zip", b"shader-bytes"),
            None,
        )
        .unwrap();
    fixture
        .service
        .import_instance_resource(
            &fixture.instance_id,
            InstanceResourceKind::ResourcePack,
            &fixture.write_source("faithful.jar", b"pack-jar-bytes"),
            None,
        )
        .unwrap();

    let shaders = fixture
        .service
        .list_instance_resources(&fixture.instance_id, Some(InstanceResourceKind::Shader))
        .unwrap();
    assert_eq!(shaders.len(), 1);
    assert_eq!(shaders[0].kind, InstanceResourceKind::Shader);
    assert_eq!(
        fixture
            .service
            .list_instance_resources(&fixture.instance_id, None)
            .unwrap()
            .len(),
        2
    );
    let error = fixture
        .service
        .import_instance_resource(
            &fixture.instance_id,
            InstanceResourceKind::ResourcePack,
            &fixture.write_source("notes.txt", b"not-a-pack"),
            None,
        )
        .expect_err("非 ZIP/JAR 文件必须被拒绝");
    assert!(error.to_string().contains("ZIP/JAR"));
    let error = fixture
        .service
        .import_instance_resource(
            &fixture.instance_id,
            InstanceResourceKind::ResourcePack,
            Path::new("D:\\definitely\\missing\\pack.zip"),
            None,
        )
        .expect_err("缺失文件必须被拒绝");
    assert!(error.to_string().contains("找不到"));
}

struct ResourceFixture {
    directory: TempDir,
    database_path: PathBuf,
    data_directory: PathBuf,
    instance_id: String,
    instance_root: PathBuf,
    service: AppService,
}

impl ResourceFixture {
    fn new() -> Self {
        let directory = TempDir::new().unwrap();
        let database_path = directory.path().join("state.sqlite3");
        let data_directory = directory.path().join("data");
        let instance_id = "instance-id".to_owned();
        let instance_root = data_directory.join("instances").join(&instance_id);
        let service = AppService::open(&database_path, &data_directory).unwrap();
        service.skip_onboarding().unwrap();
        fs::create_dir_all(instance_root.join(".minecraft")).unwrap();
        Connection::open(&database_path)
            .unwrap()
            .execute(
                "
                INSERT INTO instances (
                    id, name, game_version, loader_kind, loader_version,
                    root_directory, state, created_at_unix_seconds
                ) VALUES (?1, '资源测试', '26.2', 'fabric', '0.19.3', ?2, 'ready', 1)
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
        fs::create_dir_all(instance_root.join(".minecraft")).unwrap();
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

    fn create_world(&self, name: &str) {
        let world = self.instance_root.join(".minecraft/saves").join(name);
        fs::create_dir_all(&world).unwrap();
        fs::write(world.join("level.dat"), b"level").unwrap();
    }

    fn write_source(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let source_dir = self.directory.path().join("sources");
        fs::create_dir_all(&source_dir).unwrap();
        let path = source_dir.join(name);
        fs::write(&path, bytes).unwrap();
        path
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    let mut encoded = String::with_capacity(digest.len() * 2);
    for byte in digest {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}
