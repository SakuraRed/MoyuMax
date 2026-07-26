use std::path::PathBuf;

use moyumax_core::{AppService, LaunchOptions};
use rusqlite::{Connection, params};
use tempfile::TempDir;

#[test]
fn m33_launch_mem_001_unset_falls_back_to_default() {
    let fixture = LaunchOptionsFixture::new();
    fixture.register_instance("instance-a", "实例甲");

    let options = fixture
        .service
        .instance_launch_options("instance-a")
        .unwrap();
    assert_eq!(options.minimum_memory_mib, 512, "未设置时最小内存回退默认");
    assert_eq!(
        options.maximum_memory_mib, 2_048,
        "未设置时最大内存回退默认"
    );
}

#[test]
fn m33_launch_mem_002_invalid_values_rejected_without_write() {
    let fixture = LaunchOptionsFixture::new();
    fixture.register_instance("instance-a", "实例甲");

    for options in [
        LaunchOptions {
            minimum_memory_mib: 128,
            maximum_memory_mib: 2_048,
        },
        LaunchOptions {
            minimum_memory_mib: 4_096,
            maximum_memory_mib: 2_048,
        },
        LaunchOptions {
            minimum_memory_mib: 512,
            maximum_memory_mib: 70_000,
        },
    ] {
        let error = fixture
            .service
            .set_instance_launch_options("instance-a", &options)
            .expect_err("非法内存配置必须拒绝");
        assert!(error.to_string().contains("256 MiB"), "{error}");
    }

    let persisted = fixture
        .service
        .instance_launch_options("instance-a")
        .unwrap();
    assert_eq!(
        persisted,
        LaunchOptions {
            minimum_memory_mib: 512,
            maximum_memory_mib: 2_048,
        },
        "拒绝后不得写入,仍应回退默认"
    );
}

#[test]
fn m33_launch_mem_003_set_persists_and_survives_reopen() {
    let fixture = LaunchOptionsFixture::new();
    fixture.register_instance("instance-a", "实例甲");
    let options = LaunchOptions {
        minimum_memory_mib: 1_024,
        maximum_memory_mib: 8_192,
    };

    fixture
        .service
        .set_instance_launch_options("instance-a", &options)
        .expect("合法配置应写入");
    assert_eq!(
        fixture
            .service
            .instance_launch_options("instance-a")
            .unwrap(),
        options,
        "写入后必须立即回读一致"
    );

    let reopened = AppService::open(&fixture.database_path, &fixture.data_directory).unwrap();
    assert_eq!(
        reopened.instance_launch_options("instance-a").unwrap(),
        options,
        "重新打开数据库后配置必须保持"
    );
}

#[test]
fn m33_launch_mem_004_unknown_instance_rejected() {
    let fixture = LaunchOptionsFixture::new();
    let error = fixture
        .service
        .instance_launch_options("missing-instance")
        .expect_err("未知实例读取必须报错");
    assert!(error.to_string().contains("实例不存在"), "{error}");
    let error = fixture
        .service
        .set_instance_launch_options(
            "missing-instance",
            &LaunchOptions {
                minimum_memory_mib: 512,
                maximum_memory_mib: 2_048,
            },
        )
        .expect_err("未知实例写入必须报错");
    assert!(error.to_string().contains("实例不存在"), "{error}");
}

#[test]
fn m33_launch_mem_005_content_enable_toggle_persists() {
    let fixture = LaunchOptionsFixture::new();
    fixture.register_instance("instance-a", "实例甲");
    fixture.register_content("content-1", "instance-a", true);

    let updated = fixture
        .service
        .set_installed_content_enabled("content-1", false)
        .expect("停用应成功");
    assert!(!updated.enabled, "停用后回读必须为 false");
    let listed = fixture
        .service
        .list_installed_content("instance-a")
        .unwrap();
    assert_eq!(listed.len(), 1);
    assert!(!listed[0].enabled, "清单必须反映停用状态");

    let reopened = AppService::open(&fixture.database_path, &fixture.data_directory).unwrap();
    let updated = reopened
        .set_installed_content_enabled("content-1", true)
        .expect("重新打开后启用应成功");
    assert!(updated.enabled);

    let error = fixture
        .service
        .set_installed_content_enabled("missing-content", true)
        .expect_err("未知内容必须报错");
    assert!(error.to_string().contains("内容项不存在"), "{error}");
}

struct LaunchOptionsFixture {
    _directory: TempDir,
    database_path: PathBuf,
    data_directory: PathBuf,
    service: AppService,
}

impl LaunchOptionsFixture {
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

    fn register_instance(&self, id: &str, name: &str) {
        let root = self.data_directory.join("instances").join(id);
        std::fs::create_dir_all(&root).unwrap();
        Connection::open(&self.database_path)
            .unwrap()
            .execute(
                "INSERT INTO instances (id, name, game_version, loader_kind, loader_version, root_directory, state, created_at_unix_seconds) VALUES (?1, ?2, '26.2', 'fabric', '0.19.3', ?3, 'ready', 1)",
                params![id, name, root.to_string_lossy()],
            )
            .unwrap();
    }

    fn register_content(&self, id: &str, instance_id: &str, enabled: bool) {
        Connection::open(&self.database_path)
            .unwrap()
            .execute(
                "INSERT INTO installed_content (id, instance_id, provider, project_id, version_id, project_title, version_number, file_name, relative_path, size, sha1, sha512, enabled, auto_update_enabled, installed_at_unix_seconds) VALUES (?1, ?2, 'modrinth', 'P1', 'V1', '示例模组', '1.0.0', 'example.jar', '.minecraft/mods/example.jar', 1024, '1', '2', ?3, 0, 1)",
                params![id, instance_id, enabled],
            )
            .unwrap();
    }
}
