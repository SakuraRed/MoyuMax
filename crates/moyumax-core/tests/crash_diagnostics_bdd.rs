use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};

use moyumax_core::{AppService, CrashCauseKind, CrashEvidenceKind, Language, OnboardingSelection};
use rusqlite::{Connection, params};
use tempfile::TempDir;
use zip::ZipArchive;

#[test]
fn m6_crash_001_failed_session_generates_one_local_report_with_discovered_evidence() {
    let fixture = DiagnosticsFixture::new();
    fixture.insert_session(
        "failed-session",
        "failed",
        Some(1),
        Some("游戏进程退出码：1"),
    );
    fixture.write_failure_evidence("failed-session");

    let report = fixture
        .service
        .create_crash_report_for_session("failed-session")
        .unwrap()
        .expect("失败会话应生成报告");

    assert_eq!(report.launch_session_id, "failed-session");
    assert_eq!(report.cause, CrashCauseKind::OutOfMemory);
    assert!(report.summary.contains("内存"));
    assert!(report.evidence.iter().any(|item| {
        item.kind == CrashEvidenceKind::GameOutput && item.bundle_name == "game/last-output.log"
    }));
    assert!(report.evidence.iter().any(|item| {
        item.kind == CrashEvidenceKind::GameLog && item.bundle_name == "game/latest.log"
    }));
    assert!(report.evidence.iter().any(|item| {
        item.kind == CrashEvidenceKind::GameLog && item.bundle_name == "game/debug.log"
    }));
    assert!(report.evidence.iter().any(|item| {
        item.kind == CrashEvidenceKind::GameCrashReport
            && item.bundle_name.starts_with("game/crash-reports/")
    }));
    assert!(report.evidence.iter().any(|item| {
        item.kind == CrashEvidenceKind::NativeCrash
            && item.bundle_name.starts_with("game/crash-reports/")
    }));
    assert!(report.evidence.iter().any(|item| {
        item.kind == CrashEvidenceKind::LauncherLog && item.bundle_name == "moyumax/launcher.log"
    }));
    assert!(report.evidence.iter().any(|item| {
        item.kind == CrashEvidenceKind::LaunchScript
            && item.bundle_name == "moyumax/launch-redacted.cmd.txt"
    }));

    let same_report = fixture
        .service
        .create_crash_report_for_session("failed-session")
        .unwrap()
        .expect("重复生成应返回原报告");
    assert_eq!(same_report.id, report.id);
    assert_eq!(fixture.service.list_crash_reports().unwrap().len(), 1);

    let report_json = fs::read_to_string(
        fixture
            .data_directory
            .join("diagnostics/reports")
            .join(&report.id)
            .join("report.json"),
    )
    .unwrap();
    assert!(!report_json.contains(&fixture.instance_root.to_string_lossy().to_string()));
    assert!(!report_json.contains("PrivatePlayer"));
    assert!(!report_json.contains("super-secret-token"));
}

#[test]
fn m6_crash_002_normal_and_user_stopped_sessions_do_not_generate_reports() {
    let fixture = DiagnosticsFixture::new();
    fixture.insert_session("completed-session", "completed", Some(0), None);
    fixture.insert_session("stopped-session", "stopped", Some(1), None);

    assert!(
        fixture
            .service
            .create_crash_report_for_session("completed-session")
            .unwrap()
            .is_none()
    );
    assert!(
        fixture
            .service
            .create_crash_report_for_session("stopped-session")
            .unwrap()
            .is_none()
    );
    assert!(fixture.service.list_crash_reports().unwrap().is_empty());
}

#[test]
fn m6_privacy_001_preview_precedes_atomic_zip_export_and_sensitive_values_are_redacted() {
    let fixture = DiagnosticsFixture::new();
    fixture.insert_session(
        "privacy-session",
        "failed",
        Some(1),
        Some("游戏进程退出码：1"),
    );
    fixture.write_failure_evidence("privacy-session");
    let report = fixture
        .service
        .create_crash_report_for_session("privacy-session")
        .unwrap()
        .unwrap();

    let preview = fixture
        .service
        .preview_diagnostic_export(&report.id)
        .unwrap();
    assert!(
        preview
            .files
            .iter()
            .any(|file| file.bundle_name == "manifest.json")
    );
    assert!(
        preview
            .files
            .iter()
            .any(|file| file.bundle_name == "game/last-output.log")
    );
    assert!(preview.redactions.iter().any(|item| item.contains("玩家")));
    assert!(
        preview
            .redactions
            .iter()
            .any(|item| item.contains("用户目录"))
    );
    assert!(
        preview
            .redactions
            .iter()
            .any(|item| item.contains("服务器"))
    );
    assert!(preview.redactions.iter().any(|item| item.contains("凭据")));
    let exports = fixture.data_directory.join("diagnostics/exports");
    assert_eq!(file_count(&exports, "zip"), 0, "预览不能写出 ZIP");

    let result = fixture
        .service
        .export_diagnostic_bundle(&report.id)
        .unwrap();
    assert!(Path::new(&result.archive_path).is_file());
    assert_eq!(file_count(&exports, "partial"), 0);
    assert_eq!(result.file_count, preview.files.len());

    let archive_file = fs::File::open(&result.archive_path).unwrap();
    let mut archive = ZipArchive::new(archive_file).unwrap();
    let mut combined = String::new();
    let mut names = Vec::new();
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).unwrap();
        names.push(entry.name().to_owned());
        entry.read_to_string(&mut combined).unwrap();
    }
    assert!(names.contains(&"manifest.json".to_owned()));
    assert!(names.contains(&"game/last-output.log".to_owned()));
    for sensitive in [
        "PrivatePlayer",
        "super-secret-token",
        "my-local-password",
        "203.0.113.7",
        "mc.example.test:25565",
        "123e4567-e89b-12d3-a456-426614174000",
        fixture.instance_root.to_string_lossy().as_ref(),
    ] {
        assert!(
            !combined.contains(sensitive),
            "诊断包泄露敏感值：{sensitive}"
        );
    }
    assert!(combined.contains("<redacted"));
}

#[test]
fn m6_crash_003_reopen_marks_running_session_interrupted_and_backfills_report() {
    let fixture = DiagnosticsFixture::new();
    fixture.insert_session("interrupted-session", "running", None, None);
    drop(fixture.service);

    let reopened = AppService::open(&fixture.database_path, &fixture.data_directory).unwrap();
    let session = reopened
        .list_launch_sessions()
        .unwrap()
        .into_iter()
        .find(|session| session.id == "interrupted-session")
        .unwrap();
    assert_eq!(format!("{:?}", session.state), "Interrupted");
    let reports = reopened.list_crash_reports().unwrap();
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].launch_session_id, "interrupted-session");
    assert_eq!(reports[0].cause, CrashCauseKind::LauncherInterrupted);
}

#[test]
fn m6_crash_004_local_rules_distinguish_supported_causes_without_guessing() {
    for (session_id, evidence, expected) in [
        (
            "mod-conflict",
            "org.spongepowered.asm.mixin.throwables.MixinApplyError: Mixin apply failed",
            CrashCauseKind::ModConflict,
        ),
        (
            "java-runtime",
            "java.lang.UnsupportedClassVersionError: class file version is unsupported",
            CrashCauseKind::JavaRuntime,
        ),
        (
            "native-crash",
            "EXCEPTION_ACCESS_VIOLATION\nProblematic frame: nvoglv64.dll",
            CrashCauseKind::NativeCrash,
        ),
        (
            "unknown-crash",
            "The game stopped for an unclassified reason",
            CrashCauseKind::Unknown,
        ),
    ] {
        let fixture = DiagnosticsFixture::new();
        fixture.insert_session(session_id, "failed", Some(1), Some("游戏进程退出码：1"));
        let stdout = fixture
            .working_directory
            .join("logs/moyumax")
            .join(format!("{session_id}.stdout.log"));
        fs::write(stdout, evidence).unwrap();
        let report = fixture
            .service
            .create_crash_report_for_session(session_id)
            .unwrap()
            .unwrap();
        assert_eq!(report.cause, expected, "会话 {session_id} 分类错误");
    }
}

struct DiagnosticsFixture {
    _temp: TempDir,
    service: AppService,
    database_path: PathBuf,
    data_directory: PathBuf,
    instance_root: PathBuf,
    working_directory: PathBuf,
}

impl DiagnosticsFixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let database_path = temp.path().join("state.sqlite3");
        let data_directory = temp.path().join("data");
        let instance_root = data_directory.join("instances/diagnostics-instance");
        let working_directory = instance_root.join(".minecraft");
        fs::create_dir_all(working_directory.join("logs/moyumax")).unwrap();
        fs::create_dir_all(working_directory.join("crash-reports")).unwrap();
        let service = AppService::open(&database_path, &data_directory).unwrap();
        service
            .complete_onboarding(&OnboardingSelection {
                language: Language::ZhCn,
                data_directory: data_directory.to_string_lossy().into_owned(),
                telemetry_enabled: false,
                update_checks_enabled: true,
                nat_detection_enabled: false,
                instance_isolation_enabled: true,
            })
            .unwrap();
        let connection = Connection::open(&database_path).unwrap();
        connection
            .execute(
                "INSERT INTO instances (id, name, game_version, loader_kind, loader_version, root_directory, state, created_at_unix_seconds) VALUES (?1, ?2, '1.21.8', 'fabric', '0.16.14', ?3, 'ready', 1)",
                params![
                    "diagnostics-instance",
                    "诊断测试实例",
                    instance_root.to_string_lossy().as_ref(),
                ],
            )
            .unwrap();
        Self {
            _temp: temp,
            service,
            database_path,
            data_directory,
            instance_root,
            working_directory,
        }
    }

    fn insert_session(
        &self,
        id: &str,
        state: &str,
        exit_code: Option<i32>,
        error_summary: Option<&str>,
    ) {
        let logs = self.working_directory.join("logs/moyumax");
        let connection = Connection::open(&self.database_path).unwrap();
        connection
            .execute(
                "INSERT INTO launch_sessions (id, instance_id, player_name, state, started_at_unix_seconds, ended_at_unix_seconds, exit_code, stdout_path, stderr_path, error_summary) VALUES (?1, 'diagnostics-instance', 'PrivatePlayer', ?2, 1, 2, ?3, ?4, ?5, ?6)",
                params![
                    id,
                    state,
                    exit_code,
                    logs.join(format!("{id}.stdout.log")).to_string_lossy().as_ref(),
                    logs.join(format!("{id}.stderr.log")).to_string_lossy().as_ref(),
                    error_summary,
                ],
            )
            .unwrap();
    }

    fn write_failure_evidence(&self, session_id: &str) {
        let logs = self.working_directory.join("logs/moyumax");
        let sensitive = format!(
            "player=PrivatePlayer\npath={}\nserver=mc.example.test:25565\nip=203.0.113.7\naccess_token=super-secret-token\npassword=my-local-password\nuuid=123e4567-e89b-12d3-a456-426614174000\nAuthorization: Bearer super-secret-token\n",
            self.instance_root.display(),
        );
        fs::write(
            logs.join(format!("{session_id}.stdout.log")),
            format!("java.lang.OutOfMemoryError: Java heap space\n{sensitive}"),
        )
        .unwrap();
        fs::write(
            logs.join(format!("{session_id}.stderr.log")),
            "Process terminated with exit code 1\n",
        )
        .unwrap();
        fs::write(self.working_directory.join("logs/latest.log"), &sensitive).unwrap();
        fs::write(self.working_directory.join("logs/debug.log"), &sensitive).unwrap();
        fs::write(
            self.working_directory.join("crash-reports/crash-test.txt"),
            format!("---- Minecraft Crash Report ----\n{sensitive}"),
        )
        .unwrap();
        fs::write(
            self.working_directory.join("hs_err_pid123.log"),
            format!("EXCEPTION_ACCESS_VIOLATION\nProblematic frame\n{sensitive}"),
        )
        .unwrap();
        fs::write(
            logs.join(format!("{session_id}.launcher.log")),
            "完整性检查通过\n游戏进程异常退出\n",
        )
        .unwrap();
        fs::write(
            logs.join(format!("{session_id}.launch-redacted.cmd.txt")),
            "java --username <redacted-player> --accessToken <redacted-credential>\n",
        )
        .unwrap();
    }
}

fn file_count(directory: &Path, extension: &str) -> usize {
    fs::read_dir(directory)
        .map(|entries| {
            entries
                .filter_map(std::result::Result::ok)
                .filter(|entry| {
                    entry.path().extension().and_then(|value| value.to_str()) == Some(extension)
                })
                .count()
        })
        .unwrap_or(0)
}
