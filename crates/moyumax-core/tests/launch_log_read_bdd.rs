use std::fs;

use moyumax_core::{AppService, LAUNCH_LOG_READ_LIMIT_BYTES, Language, OnboardingSelection};
use rusqlite::{Connection, params};
use tempfile::TempDir;

#[test]
fn m4_log_001_incremental_read_follows_appended_output_per_channel() {
    let fixture = LaunchLogFixture::new();
    fixture.insert_session("session-a", "running");
    fixture.write_stdout("session-a", "[Init] 第一行\n[Init] 第二行\n");
    fixture.write_stderr("session-a", "[Warn] 一个警告\n");

    let first = fixture
        .service
        .read_launch_log("session-a", 0, 0)
        .expect("initial read should succeed");
    assert_eq!(first.session_id, "session-a");
    assert_eq!(
        first.state,
        moyumax_core::LaunchSessionState::Running,
        "读取结果应携带读取时刻的会话状态"
    );
    assert_eq!(first.stdout.content, "[Init] 第一行\n[Init] 第二行\n");
    assert_eq!(first.stderr.content, "[Warn] 一个警告\n");
    assert!(!first.stdout.truncated);
    assert!(!first.stderr.truncated);

    // 游戏继续追加输出后,按上次偏移只返回新增内容。
    fixture.append_stdout("session-a", "[Game] 世界已加载\n");
    fixture.append_stderr("session-a", "[Warn] 又一个警告\n");
    let second = fixture
        .service
        .read_launch_log(
            "session-a",
            first.stdout.next_offset,
            first.stderr.next_offset,
        )
        .expect("follow-up read should succeed");
    assert_eq!(second.stdout.content, "[Game] 世界已加载\n");
    assert_eq!(second.stderr.content, "[Warn] 又一个警告\n");
    assert!(
        second.stdout.next_offset > first.stdout.next_offset,
        "下次偏移应随文件长度前进"
    );

    // 没有新增内容时返回空串而不是重复内容。
    let third = fixture
        .service
        .read_launch_log(
            "session-a",
            second.stdout.next_offset,
            second.stderr.next_offset,
        )
        .expect("idle read should succeed");
    assert_eq!(third.stdout.content, "");
    assert_eq!(third.stderr.content, "");
    assert_eq!(third.stdout.next_offset, second.stdout.next_offset);
}

#[test]
fn m4_log_002_oversized_log_returns_tail_within_limit_on_char_boundary() {
    let fixture = LaunchLogFixture::new();
    fixture.insert_session("session-big", "completed");
    // 截断起点附近放多字节字符:若不做边界保护会产生 U+FFFD 替换符。
    let padding_len = usize::try_from(LAUNCH_LOG_READ_LIMIT_BYTES).unwrap() - 64;
    let content = format!(
        "{}{}{}",
        "A".repeat(padding_len),
        "中文日志行占据多字节字符\n",
        "B".repeat(128),
    );
    fixture.write_stdout("session-big", &content);
    fixture.write_stderr("session-big", "少量错误输出\n");

    let read = fixture
        .service
        .read_launch_log("session-big", 0, 0)
        .expect("tail read should succeed");
    assert!(read.stdout.truncated, "超过上限应标记截断");
    assert!(
        u64::try_from(read.stdout.content.len()).unwrap() <= LAUNCH_LOG_READ_LIMIT_BYTES,
        "返回内容不得超过单次读取上限"
    );
    assert!(
        !read.stdout.content.contains('\u{FFFD}'),
        "截断起点应回退到完整 UTF-8 字符边界"
    );
    assert!(
        read.stdout.content.ends_with(&"B".repeat(128)),
        "尾部内容应完整保留"
    );
    assert!(!read.stderr.truncated, "未超上限的通道不受影响");
    assert_eq!(read.stderr.content, "少量错误输出\n");
}

#[test]
fn m4_log_003_missing_log_files_read_as_empty() {
    let fixture = LaunchLogFixture::new();
    fixture.insert_session("session-pending", "starting");

    let read = fixture
        .service
        .read_launch_log("session-pending", 0, 0)
        .expect("missing files should read as empty");
    assert_eq!(read.stdout.content, "");
    assert_eq!(read.stderr.content, "");
    assert_eq!(read.stdout.next_offset, 0);
    assert!(!read.stdout.truncated);
}

#[test]
fn m4_log_004_unknown_session_and_out_of_range_offset_are_safe() {
    let fixture = LaunchLogFixture::new();
    let error = fixture
        .service
        .read_launch_log("no-such-session", 0, 0)
        .expect_err("unknown session should be rejected");
    assert!(
        error.to_string().contains("启动会话不存在"),
        "错误应能定位到会话不存在: {error}"
    );

    fixture.insert_session("session-short", "completed");
    fixture.write_stdout("session-short", "只有一行\n");
    // 偏移超过当前文件长度(例如日志被外部清理后重写)按文件末尾处理。
    let read = fixture
        .service
        .read_launch_log("session-short", u64::MAX, u64::MAX)
        .expect("out-of-range offset should clamp to file end");
    assert_eq!(read.stdout.content, "");
    assert_eq!(
        read.stdout.next_offset,
        u64::try_from("只有一行\n".len()).unwrap()
    );
    assert!(!read.stdout.truncated);
}

struct LaunchLogFixture {
    _temp: TempDir,
    service: AppService,
    database_path: std::path::PathBuf,
    logs_directory: std::path::PathBuf,
}

impl LaunchLogFixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let database_path = temp.path().join("state.sqlite3");
        let data_directory = temp.path().join("data");
        let instance_root = data_directory.join("instances/log-instance");
        let logs_directory = instance_root.join(".minecraft/logs/moyumax");
        fs::create_dir_all(&logs_directory).unwrap();
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
                    "log-instance",
                    "日志测试实例",
                    instance_root.to_string_lossy().as_ref(),
                ],
            )
            .unwrap();
        Self {
            _temp: temp,
            service,
            database_path,
            logs_directory,
        }
    }

    fn insert_session(&self, id: &str, state: &str) {
        let connection = Connection::open(&self.database_path).unwrap();
        connection
            .execute(
                "INSERT INTO launch_sessions (id, instance_id, player_name, state, started_at_unix_seconds, stdout_path, stderr_path) VALUES (?1, 'log-instance', 'LogPlayer', ?2, 1, ?3, ?4)",
                params![
                    id,
                    state,
                    self.logs_directory
                        .join(format!("{id}.stdout.log"))
                        .to_string_lossy()
                        .as_ref(),
                    self.logs_directory
                        .join(format!("{id}.stderr.log"))
                        .to_string_lossy()
                        .as_ref(),
                ],
            )
            .unwrap();
    }

    fn write_stdout(&self, session_id: &str, content: &str) {
        fs::write(
            self.logs_directory.join(format!("{session_id}.stdout.log")),
            content,
        )
        .unwrap();
    }

    fn append_stdout(&self, session_id: &str, content: &str) {
        use std::io::Write as _;
        let mut file = fs::OpenOptions::new()
            .append(true)
            .open(self.logs_directory.join(format!("{session_id}.stdout.log")))
            .unwrap();
        file.write_all(content.as_bytes()).unwrap();
    }

    fn write_stderr(&self, session_id: &str, content: &str) {
        fs::write(
            self.logs_directory.join(format!("{session_id}.stderr.log")),
            content,
        )
        .unwrap();
    }

    fn append_stderr(&self, session_id: &str, content: &str) {
        use std::io::Write as _;
        let mut file = fs::OpenOptions::new()
            .append(true)
            .open(self.logs_directory.join(format!("{session_id}.stderr.log")))
            .unwrap();
        file.write_all(content.as_bytes()).unwrap();
    }
}
