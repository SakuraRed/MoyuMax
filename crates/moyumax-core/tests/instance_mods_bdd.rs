use std::fs;

use moyumax_core::AppService;
use rusqlite::{Connection, params};
use tempfile::TempDir;

struct Fixture {
    _directory: TempDir,
    data_directory: std::path::PathBuf,
    instance_id: String,
    instance_root: std::path::PathBuf,
    service: AppService,
}

impl Fixture {
    fn new() -> Self {
        let directory = TempDir::new().unwrap();
        let database_path = directory.path().join("state.sqlite3");
        let data_directory = directory.path().join("data");
        let instance_id = "instance-id".to_owned();
        let instance_root = data_directory.join("instances").join(&instance_id);
        let service = AppService::open(&database_path, &data_directory).unwrap();
        service.skip_onboarding().unwrap();
        fs::create_dir_all(instance_root.join(".minecraft/mods")).unwrap();
        Connection::open(&database_path)
            .unwrap()
            .execute(
                "
                INSERT INTO instances (
                    id, name, game_version, loader_kind, loader_version,
                    root_directory, state, created_at_unix_seconds
                ) VALUES (?1, '模组测试', '26.2', 'fabric', '0.19.3', ?2, 'ready', 1)
                ",
                params![instance_id, instance_root.to_string_lossy()],
            )
            .unwrap();
        Self {
            _directory: directory,
            data_directory,
            instance_id,
            instance_root,
            service,
        }
    }

    fn write_mod(&self, name: &str, bytes: &[u8]) {
        fs::write(self.instance_root.join(".minecraft/mods").join(name), bytes).unwrap();
    }

    fn record_content(&self) {
        let connection =
            Connection::open(self.data_directory.parent().unwrap().join("state.sqlite3")).unwrap();
        connection
            .execute(
                "
                INSERT INTO installed_content (
                    id, instance_id, provider, project_id, version_id,
                    project_title, version_number, file_name, relative_path,
                    size, sha1, sha512, enabled, auto_update_enabled,
                    installed_at_unix_seconds
                ) VALUES (
                    'content-sodium', ?1, 'modrinth', 'PROJNA', 'VER0001',
                    'Sodium', '0.6.13', 'sodium.jar', '.minecraft/mods/sodium.jar',
                    5, 'sha1', 'sha512', 1, 0, 1
                )
                ",
                params![self.instance_id],
            )
            .unwrap();
    }
}

#[test]
fn mods_listing_scans_directory_and_merges_records() {
    let fixture = Fixture::new();
    fixture.write_mod("jei.jar", b"jei-bytes");
    fixture.write_mod("sodium.jar", b"sod!!");
    fixture.write_mod("lithium.jar.disabled", b"lithium");
    fixture.write_mod("readme.txt", b"not-a-mod");
    fixture.record_content();

    let entries = fixture.service.list_instance_mods(&fixture.instance_id).unwrap();
    assert_eq!(entries.len(), 3, "只统计 jar 与 jar.disabled:{entries:?}");

    let jei = entries.iter().find(|entry| entry.file_name == "jei.jar").unwrap();
    assert!(jei.enabled);
    assert!(jei.content.is_none(), "未收录文件不带元数据");
    assert_eq!(jei.size_bytes, 9);

    let sodium = entries
        .iter()
        .find(|entry| entry.file_name == "sodium.jar")
        .unwrap();
    let content = sodium.content.as_ref().expect("收录记录应带出元数据");
    assert_eq!(content.project_title, "Sodium");
    assert_eq!(content.version_number, "0.6.13");

    let lithium = entries
        .iter()
        .find(|entry| entry.file_name == "lithium.jar.disabled")
        .unwrap();
    assert!(!lithium.enabled, ".disabled 后缀即禁用态");
}

#[test]
fn toggle_renames_file_and_syncs_index_flag() {
    let fixture = Fixture::new();
    fixture.write_mod("jei.jar", b"jei-bytes");
    fixture.write_mod("sodium.jar", b"sod!!");
    fixture.record_content();

    let disabled = fixture
        .service
        .set_instance_mod_enabled(&fixture.instance_id, "mods/jei.jar", false)
        .unwrap();
    assert_eq!(disabled.file_name, "jei.jar.disabled");
    assert!(!disabled.enabled);
    assert!(
        fixture
            .instance_root
            .join(".minecraft/mods/jei.jar.disabled")
            .is_file()
    );
    assert!(!fixture.instance_root.join(".minecraft/mods/jei.jar").exists());

    let sodium = fixture
        .service
        .set_instance_mod_enabled(&fixture.instance_id, "mods/sodium.jar", false)
        .unwrap();
    assert_eq!(sodium.relative_path, "mods/sodium.jar.disabled");
    let content = sodium.content.expect("索引记录应保持关联");
    assert!(!content.enabled, "索引标志同步为禁用");
    assert_eq!(content.relative_path, ".minecraft/mods/sodium.jar.disabled");

    let enabled = fixture
        .service
        .set_instance_mod_enabled(&fixture.instance_id, "mods/jei.jar.disabled", true)
        .unwrap();
    assert_eq!(enabled.file_name, "jei.jar");
    assert!(enabled.enabled);
    assert!(fixture.instance_root.join(".minecraft/mods/jei.jar").is_file());
}

#[test]
fn toggle_rejects_paths_outside_mods() {
    let fixture = Fixture::new();
    assert!(
        fixture
            .service
            .set_instance_mod_enabled(&fixture.instance_id, "../evil.jar", false)
            .is_err()
    );
    assert!(
        fixture
            .service
            .set_instance_mod_enabled(&fixture.instance_id, "config/evil.jar", false)
            .is_err()
    );
}
