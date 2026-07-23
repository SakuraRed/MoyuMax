use std::fs;

use moyumax_core::AppService;
use moyumax_desktop_lib::{EXIT_DISABLED, EXIT_OK, EXIT_USAGE, execute_cli_command};
use rusqlite::{Connection, params};
use tempfile::TempDir;

#[test]
fn m24_cli_001_disabled_cli_rejects_every_command() {
    let fixture = CliFixture::new(false);

    for argv in [
        vec!["instances".to_owned(), "list".to_owned()],
        vec!["tasks".to_owned(), "pause-all".to_owned()],
    ] {
        let outcome = execute_cli_command(&fixture.service, &argv);
        assert_eq!(outcome.exit_code, EXIT_DISABLED);
        assert_eq!(outcome.envelope["ok"], false);
        assert_eq!(outcome.envelope["error"]["code"], "cli_disabled");
        assert_eq!(outcome.envelope["schemaVersion"], 1);
        assert!(
            outcome.envelope["error"]["message"]
                .as_str()
                .unwrap()
                .contains("开发者")
        );
    }
}

#[test]
fn m24_cli_002_instances_list_outputs_versioned_json() {
    let fixture = CliFixture::new(true);
    fixture.create_instance("instance-id", "整合测试");

    let outcome = execute_cli_command(
        &fixture.service,
        &["instances".to_owned(), "list".to_owned()],
    );

    assert_eq!(outcome.exit_code, EXIT_OK);
    assert_eq!(outcome.envelope["ok"], true);
    assert_eq!(outcome.envelope["schemaVersion"], 1);
    assert_eq!(
        outcome.envelope["command"],
        serde_json::json!(["instances", "list"])
    );
    let instances = outcome.envelope["data"].as_array().unwrap();
    assert_eq!(instances.len(), 1);
    assert_eq!(instances[0]["name"], "整合测试");
}

#[test]
fn m24_cli_003_pause_all_supports_dry_run_without_persisting() {
    let fixture = CliFixture::new(true);

    let dry = execute_cli_command(
        &fixture.service,
        &[
            "tasks".to_owned(),
            "pause-all".to_owned(),
            "--dry-run".to_owned(),
        ],
    );
    assert_eq!(dry.exit_code, EXIT_OK);
    assert_eq!(dry.envelope["data"]["dryRun"], true);
    assert!(!fixture.service.tasks_paused().unwrap(), "dry-run 不得落盘");

    let real = execute_cli_command(
        &fixture.service,
        &["tasks".to_owned(), "pause-all".to_owned()],
    );
    assert_eq!(real.exit_code, EXIT_OK);
    assert!(fixture.service.tasks_paused().unwrap());
    let resume = execute_cli_command(
        &fixture.service,
        &["tasks".to_owned(), "resume-all".to_owned()],
    );
    assert_eq!(resume.exit_code, EXIT_OK);
    assert!(!fixture.service.tasks_paused().unwrap());
}

#[test]
fn m24_cli_004_backup_create_dry_run_and_real() {
    let fixture = CliFixture::new(true);
    let root = fixture.create_instance("instance-id", "整合测试");
    let world = root.join(".minecraft/saves/alpha");
    fs::create_dir_all(&world).unwrap();
    fs::write(world.join("level.dat"), b"level").unwrap();

    let dry = execute_cli_command(
        &fixture.service,
        &[
            "backups".to_owned(),
            "create".to_owned(),
            "--instance".to_owned(),
            "instance-id".to_owned(),
            "--dry-run".to_owned(),
        ],
    );
    assert_eq!(dry.exit_code, EXIT_OK);
    assert_eq!(dry.envelope["data"]["worlds"], serde_json::json!(["alpha"]));
    assert!(fixture.service.list_world_backups(None).unwrap().is_empty());

    let real = execute_cli_command(
        &fixture.service,
        &[
            "backups".to_owned(),
            "create".to_owned(),
            "--instance".to_owned(),
            "instance-id".to_owned(),
        ],
    );
    assert_eq!(real.exit_code, EXIT_OK);
    assert_eq!(real.envelope["data"]["state"], "ready");
    assert_eq!(fixture.service.list_world_backups(None).unwrap().len(), 1);

    let missing = execute_cli_command(
        &fixture.service,
        &["backups".to_owned(), "create".to_owned()],
    );
    assert_eq!(missing.exit_code, EXIT_USAGE);
}

#[test]
fn m24_cli_005_unknown_command_returns_usage() {
    let fixture = CliFixture::new(true);

    let outcome = execute_cli_command(&fixture.service, &["wat".to_owned()]);

    assert_eq!(outcome.exit_code, EXIT_USAGE);
    assert_eq!(outcome.envelope["ok"], false);
    assert!(
        outcome.envelope["error"]["message"]
            .as_str()
            .unwrap()
            .contains("instances list")
    );
}

struct CliFixture {
    _directory: TempDir,
    database_path: std::path::PathBuf,
    data_directory: std::path::PathBuf,
    service: AppService,
}

impl CliFixture {
    fn new(cli_enabled: bool) -> Self {
        let directory = TempDir::new().unwrap();
        let database_path = directory.path().join("state.sqlite3");
        let data_directory = directory.path().join("data");
        let service = AppService::open(&database_path, &data_directory).unwrap();
        service.skip_onboarding().unwrap();
        service.set_cli_enabled(cli_enabled).unwrap();
        Self {
            _directory: directory,
            database_path,
            data_directory,
            service,
        }
    }

    fn create_instance(&self, instance_id: &str, name: &str) -> std::path::PathBuf {
        let instance_root = self.data_directory.join("instances").join(instance_id);
        fs::create_dir_all(&instance_root).unwrap();
        Connection::open(&self.database_path)
            .unwrap()
            .execute(
                "
                INSERT INTO instances (
                    id, name, game_version, loader_kind, loader_version,
                    root_directory, state, created_at_unix_seconds
                ) VALUES (?1, ?2, '26.2', 'fabric', '0.19.3', ?3, 'ready', 1)
                ",
                params![instance_id, name, instance_root.to_string_lossy()],
            )
            .unwrap();
        instance_root
    }
}
