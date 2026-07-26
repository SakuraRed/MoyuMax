use std::path::PathBuf;

use moyumax_core::{
    AppService, GlobalLaunchPreference, LaunchOptions, auto_launch_options,
    total_physical_memory_mib,
};
use rusqlite::{Connection, params};
use tempfile::TempDir;

#[test]
fn m33_launch_mem_001_unset_follows_global_auto() {
    let fixture = LaunchOptionsFixture::new();
    fixture.register_instance("instance-a", "实例甲");

    assert_eq!(
        fixture
            .service
            .instance_launch_options("instance-a")
            .unwrap(),
        None,
        "实例未自定义时必须返回 None(跟随全局)"
    );
    assert_eq!(
        fixture
            .service
            .resolved_launch_options("instance-a")
            .unwrap(),
        auto_launch_options(total_physical_memory_mib()),
        "实例跟随全局且全局默认自动时,解析结果必须等于自动分配"
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

    assert_eq!(
        fixture
            .service
            .instance_launch_options("instance-a")
            .unwrap(),
        None,
        "拒绝后不得写入,仍应跟随全局"
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
        Some(options),
        "写入后必须立即回读一致"
    );

    let reopened = AppService::open(&fixture.database_path, &fixture.data_directory).unwrap();
    assert_eq!(
        reopened.instance_launch_options("instance-a").unwrap(),
        Some(options),
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
    let error = fixture
        .service
        .clear_instance_launch_options("missing-instance")
        .expect_err("未知实例清除必须报错");
    assert!(error.to_string().contains("实例不存在"), "{error}");
    let error = fixture
        .service
        .resolved_launch_options("missing-instance")
        .expect_err("未知实例解析必须报错");
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

#[test]
fn m33_launch_mem_006_clear_restores_follow_global() {
    let fixture = LaunchOptionsFixture::new();
    fixture.register_instance("instance-a", "实例甲");
    fixture
        .service
        .set_instance_launch_options(
            "instance-a",
            &LaunchOptions {
                minimum_memory_mib: 1_024,
                maximum_memory_mib: 8_192,
            },
        )
        .unwrap();

    fixture
        .service
        .clear_instance_launch_options("instance-a")
        .expect("清除应成功");
    assert_eq!(
        fixture
            .service
            .instance_launch_options("instance-a")
            .unwrap(),
        None,
        "清除后必须恢复跟随全局"
    );
    assert_eq!(
        fixture
            .service
            .resolved_launch_options("instance-a")
            .unwrap(),
        auto_launch_options(total_physical_memory_mib()),
        "清除后解析结果必须回落到自动分配"
    );
}

#[test]
fn m33_launch_mem_007_global_preference_defaults_to_auto() {
    let fixture = LaunchOptionsFixture::new();
    assert_eq!(
        fixture.service.global_launch_preference().unwrap(),
        GlobalLaunchPreference::Auto,
        "未设置时全局偏好必须默认为自动分配"
    );
}

#[test]
fn m33_launch_mem_008_global_custom_persists_and_survives_reopen() {
    let fixture = LaunchOptionsFixture::new();
    fixture.register_instance("instance-a", "实例甲");
    let preference = GlobalLaunchPreference::Custom {
        min_mib: 1_024,
        max_mib: 6_144,
    };

    fixture
        .service
        .set_global_launch_preference(&preference)
        .expect("合法全局自定义应写入");
    assert_eq!(
        fixture.service.global_launch_preference().unwrap(),
        preference,
        "写入后必须立即回读一致"
    );
    assert_eq!(
        fixture
            .service
            .resolved_launch_options("instance-a")
            .unwrap(),
        LaunchOptions {
            minimum_memory_mib: 1_024,
            maximum_memory_mib: 6_144,
        },
        "实例跟随全局时,全局自定义必须生效"
    );

    let reopened = AppService::open(&fixture.database_path, &fixture.data_directory).unwrap();
    assert_eq!(
        reopened.global_launch_preference().unwrap(),
        preference,
        "重新打开数据库后全局偏好必须保持"
    );
}

#[test]
fn m33_launch_mem_009_global_custom_invalid_rejected() {
    let fixture = LaunchOptionsFixture::new();
    for preference in [
        GlobalLaunchPreference::Custom {
            min_mib: 128,
            max_mib: 2_048,
        },
        GlobalLaunchPreference::Custom {
            min_mib: 4_096,
            max_mib: 2_048,
        },
        GlobalLaunchPreference::Custom {
            min_mib: 512,
            max_mib: 70_000,
        },
    ] {
        let error = fixture
            .service
            .set_global_launch_preference(&preference)
            .expect_err("非法全局自定义必须拒绝");
        assert!(error.to_string().contains("256 MiB"), "{error}");
    }
    assert_eq!(
        fixture.service.global_launch_preference().unwrap(),
        GlobalLaunchPreference::Auto,
        "拒绝后不得写入,仍应保持默认自动分配"
    );
}

#[test]
fn m33_launch_mem_010_resolution_chain_priority() {
    let fixture = LaunchOptionsFixture::new();
    fixture.register_instance("instance-a", "实例甲");
    let global_custom = GlobalLaunchPreference::Custom {
        min_mib: 1_024,
        max_mib: 6_144,
    };
    fixture
        .service
        .set_global_launch_preference(&global_custom)
        .unwrap();
    let instance_custom = LaunchOptions {
        minimum_memory_mib: 2_048,
        maximum_memory_mib: 8_192,
    };
    fixture
        .service
        .set_instance_launch_options("instance-a", &instance_custom)
        .unwrap();

    assert_eq!(
        fixture
            .service
            .resolved_launch_options("instance-a")
            .unwrap(),
        instance_custom,
        "实例自定义必须优先于全局自定义"
    );

    fixture
        .service
        .clear_instance_launch_options("instance-a")
        .unwrap();
    assert_eq!(
        fixture
            .service
            .resolved_launch_options("instance-a")
            .unwrap(),
        LaunchOptions {
            minimum_memory_mib: 1_024,
            maximum_memory_mib: 6_144,
        },
        "实例无自定义时全局自定义必须其次生效"
    );

    fixture
        .service
        .set_global_launch_preference(&GlobalLaunchPreference::Auto)
        .unwrap();
    assert_eq!(
        fixture
            .service
            .resolved_launch_options("instance-a")
            .unwrap(),
        auto_launch_options(total_physical_memory_mib()),
        "全局自动时必须回落到自动分配"
    );
}

#[test]
fn m33_launch_mem_011_auto_rule_boundaries() {
    let expected = |minimum_memory_mib, maximum_memory_mib| LaunchOptions {
        minimum_memory_mib,
        maximum_memory_mib,
    };
    assert_eq!(
        auto_launch_options(None),
        expected(512, 4_096),
        "取不到物理内存时回退 512/4096"
    );
    assert_eq!(
        auto_launch_options(Some(2_048)),
        expected(512, 2_048),
        "物理内存过小时 Xmx 夹到下限 2048"
    );
    assert_eq!(
        auto_launch_options(Some(8_192)),
        expected(512, 2_048),
        "物理内存的四分之一不足 2048 时夹到下限"
    );
    assert_eq!(
        auto_launch_options(Some(16_384)),
        expected(512, 4_096),
        "16 GiB 物理内存分配四分之一"
    );
    assert_eq!(
        auto_launch_options(Some(32_768)),
        expected(512, 8_192),
        "32 GiB 物理内存到达上限 8192"
    );
    assert_eq!(
        auto_launch_options(Some(1_048_576)),
        expected(512, 8_192),
        "物理内存过大时 Xmx 夹到上限 8192"
    );
    assert_eq!(
        auto_launch_options(Some(u64::MAX)),
        expected(512, 8_192),
        "极端值不得溢出,夹到上限 8192"
    );
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
