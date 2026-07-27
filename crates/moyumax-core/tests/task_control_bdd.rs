use std::{
    collections::HashMap,
    fs,
    io::{BufRead, BufReader, Write},
    net::{TcpListener, TcpStream},
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use moyumax_core::{
    AdaptiveConcurrency, AppService, ArtifactDownloader, ArtifactKind, ContentDependencyChoice,
    ContentExecutor, ContentFilePlan, ContentInstallPlan, ContentPlanEntry, RateLimiter,
    ResolvedArtifact, SourceAttemptOutcome, SourcePolicy, TaskState,
};
use rusqlite::{Connection, params};
use sha1::{Digest, Sha1};
use sha2::Sha512;
use tempfile::TempDir;

#[test]
fn m14_task_001_single_pause_resume_and_global_resume_interop() {
    let fixture = TaskFixture::new();
    fixture.insert_instance("instance-control", "控制测试");
    let task_a = fixture.enqueue_content_task("instance-control", "mod-a.jar");
    let task_b = fixture.enqueue_content_task("instance-control", "mod-b.jar");

    // 用户单独暂停 A(排队状态直接标记)。
    fixture
        .service
        .mark_content_task_paused(&task_a, "user")
        .unwrap();
    // 全局暂停打断 B(模拟执行中断后的标记)。
    fixture.set_task_running("content_install_tasks", &task_b);
    fixture
        .service
        .mark_content_task_paused(&task_b, "global")
        .unwrap();

    // 全局恢复只重入被全局暂停打断的任务,用户单独暂停的 A 保持暂停。
    let requeued = fixture.service.requeue_paused_content_tasks().unwrap();
    assert_eq!(requeued, vec![task_b.clone()]);
    let states = fixture.task_states();
    assert_eq!(states[&task_a], TaskState::Paused, "用户暂停不得被全局恢复");
    assert_eq!(states[&task_b], TaskState::Queued);

    // 单任务恢复 A。
    fixture
        .service
        .requeue_paused_content_task(&task_a)
        .unwrap();
    assert_eq!(fixture.task_states()[&task_a], TaskState::Queued);
}

#[test]
fn m14_task_002_priority_orders_queue_and_rejects_running() {
    let fixture = TaskFixture::new();
    fixture.insert_instance("instance-priority", "优先级测试");
    let low = fixture.enqueue_content_task("instance-priority", "low.jar");
    let high = fixture.enqueue_content_task("instance-priority", "high.jar");
    let mid = fixture.enqueue_content_task("instance-priority", "mid.jar");

    fixture.service.set_content_task_priority(&high, 5).unwrap();
    fixture.service.set_content_task_priority(&mid, 1).unwrap();

    let ordered = fixture.service.queued_content_tasks_by_priority().unwrap();
    assert_eq!(ordered, vec![high.clone(), mid.clone(), low.clone()]);

    fixture.set_task_running("content_install_tasks", &high);
    let error = fixture
        .service
        .set_content_task_priority(&high, 9)
        .expect_err("执行中任务不允许调整优先级");
    assert!(error.to_string().contains("优先级"));
}

#[tokio::test]
async fn m14_task_003_speed_limit_throttles_total_throughput() {
    let body: Vec<u8> = (0..2 * 1024 * 1024_u32)
        .map(|index| (index % 251) as u8)
        .collect();
    let server = ExecutorServer::new(HashMap::from([
        ("/a.jar".to_owned(), body.clone()),
        ("/b.jar".to_owned(), body.clone()),
    ]));
    let limiter = Arc::new(RateLimiter::new());
    limiter.set_rate(1024 * 1024);
    let downloader = ArtifactDownloader::new(4)
        .unwrap()
        .with_rate_limiter(Arc::clone(&limiter));
    let directory = TempDir::new().unwrap();
    let artifact_a = artifact(&server.url("/a.jar"), "a.jar", &body);
    let artifact_b = artifact(&server.url("/b.jar"), "b.jar", &body);
    let staging = directory.path().join("staging");
    let shared = directory.path().join("shared");

    let started = Instant::now();
    let (result_a, result_b) = tokio::join!(
        downloader.fetch_with_policy(
            &artifact_a,
            &staging,
            &shared,
            &SourcePolicy::MirrorFirst,
            None
        ),
        downloader.fetch_with_policy(
            &artifact_b,
            &staging,
            &shared,
            &SourcePolicy::MirrorFirst,
            None
        )
    );
    result_a.unwrap();
    result_b.unwrap();
    let elapsed = started.elapsed();

    // 总 4 MiB 在 1 MiB/s 全局限速下至少约 4 秒,证明多连接没有系统性突破上限。
    assert!(
        elapsed >= Duration::from_secs_f64(3.4),
        "限速必须约束总吞吐: 实际 {elapsed:?}"
    );
    assert!(
        elapsed < Duration::from_secs(12),
        "限速实现不应异常卡死: {elapsed:?}"
    );

    // 不限速时同文件应明显更快。
    let fast = ArtifactDownloader::new(4).unwrap();
    let fast_dir = TempDir::new().unwrap();
    let fast_start = Instant::now();
    fast.fetch_with_policy(
        &artifact_a,
        &fast_dir.path().join("staging"),
        &fast_dir.path().join("shared"),
        &SourcePolicy::MirrorFirst,
        None,
    )
    .await
    .unwrap();
    let fast_elapsed = fast_start.elapsed();
    assert!(
        fast_elapsed < elapsed,
        "不限速应快于限速: {fast_elapsed:?} vs {elapsed:?}"
    );
}

#[test]
fn m14_task_004_pressure_shrinks_and_recovers_connections() {
    let adaptive = AdaptiveConcurrency::new(8);
    assert_eq!(adaptive.current_limit(), 8);

    // 零星失败不收缩:交给瞬态重试与候选回退处理。
    adaptive.record(0.0, false);
    adaptive.record(0.0, false);
    assert_eq!(adaptive.current_limit(), 8, "零星失败不得收缩");
    // 连续第三次失败才减半。
    adaptive.record(0.0, false);
    assert_eq!(adaptive.current_limit(), 4, "连续失败应减半");
    // 成功重置失败计数;连续两次健康回升一格。
    adaptive.record(1.0, true);
    adaptive.record(0.0, false);
    adaptive.record(0.0, false);
    assert_eq!(adaptive.current_limit(), 4, "成功重置后零星失败仍不收缩");
    for _ in 0..2 {
        adaptive.record(1.0, true);
    }
    assert_eq!(adaptive.current_limit(), 5, "连续健康两次回升一格");

    // 慢速成功不得收缩,且能恢复到满并发。
    for _ in 0..6 {
        adaptive.record(30.0 * 1024.0, true);
    }
    assert_eq!(adaptive.current_limit(), 8, "慢速成功必须恢复到满并发");
}

#[tokio::test]
async fn m14_task_005_queued_user_pause_never_marks_failed() {
    let fixture = TaskFixture::new();
    fixture.insert_instance("instance-guard", "守卫测试");
    let task_id = fixture.enqueue_content_task("instance-guard", "guard.jar");

    // 用户暂停排队任务后再执行执行器:执行器拒绝启动而不是把任务标成失败。
    fixture
        .service
        .mark_content_task_paused(&task_id, "user")
        .unwrap();
    let error = ContentExecutor::new(2)
        .unwrap()
        .execute_task(&fixture.service, &task_id)
        .await
        .expect_err("暂停任务不能被误执行");
    assert!(error.to_string().contains("不能开始执行"));
    assert_eq!(fixture.task_states()[&task_id], TaskState::Paused);
}

struct TaskFixture {
    _directory: TempDir,
    database_path: PathBuf,
    service: AppService,
}

impl TaskFixture {
    fn new() -> Self {
        let directory = TempDir::new().unwrap();
        let database_path = directory.path().join("state.sqlite3");
        let data_directory = directory.path().join("data");
        let service = AppService::open(&database_path, &data_directory).unwrap();
        service.skip_onboarding().unwrap();
        Self {
            _directory: directory,
            database_path,
            service,
        }
    }

    fn data_directory(&self) -> PathBuf {
        self.database_path.parent().unwrap().join("data")
    }

    fn insert_instance(&self, id: &str, name: &str) {
        let root = self.service_root(id);
        fs::create_dir_all(root.join(".minecraft/mods")).unwrap();
        Connection::open(&self.database_path)
            .unwrap()
            .execute(
                "
                INSERT INTO instances (
                    id, name, game_version, loader_kind, loader_version,
                    root_directory, state, created_at_unix_seconds
                ) VALUES (?1, ?2, '26.2', 'fabric', '0.19.3', ?3, 'ready', 1)
                ",
                params![id, name, root.to_string_lossy()],
            )
            .unwrap();
    }

    fn service_root(&self, id: &str) -> PathBuf {
        self.database_path
            .parent()
            .unwrap()
            .join("data/instances")
            .join(id)
    }

    fn enqueue_content_task(&self, instance_id: &str, filename: &str) -> String {
        let bytes = b"mod-bytes".to_vec();
        let plan = ContentInstallPlan {
            schema_version: 1,
            instance_id: instance_id.to_owned(),
            instance_name: "控制测试".to_owned(),
            game_version: "26.2".to_owned(),
            loader: "fabric".to_owned(),
            root_project_id: "ROOT0001".to_owned(),
            entries: vec![ContentPlanEntry {
                project_id: "ROOT0001".to_owned(),
                version_id: "ROOTVER1".to_owned(),
                project_title: "控制模组".to_owned(),
                version_number: "1.0.0".to_owned(),
                required_by_project_id: None,
                file: ContentFilePlan {
                    url: "http://127.0.0.1:1/unused.jar".to_owned(),
                    filename: filename.to_owned(),
                    size: u64::try_from(bytes.len()).unwrap(),
                    sha1: digest_sha1(&bytes),
                    sha512: digest_sha512(&bytes),
                },
            }],
            optional_dependencies: Vec::<ContentDependencyChoice>::new(),
            incompatible_dependencies: Vec::<ContentDependencyChoice>::new(),
            is_update: false,
        };
        self.service.enqueue_content_install_task(&plan).unwrap().id
    }

    fn set_task_running(&self, table: &str, task_id: &str) {
        let query = format!("UPDATE {table} SET state = 'running' WHERE id = ?1");
        Connection::open(&self.database_path)
            .unwrap()
            .execute(&query, params![task_id])
            .unwrap();
    }

    fn set_task_state(&self, table: &str, task_id: &str, state: &str) {
        let query = format!("UPDATE {table} SET state = ?2 WHERE id = ?1");
        Connection::open(&self.database_path)
            .unwrap()
            .execute(&query, params![task_id, state])
            .unwrap();
    }

    fn task_state(&self, table: &str, task_id: &str) -> String {
        let query = format!("SELECT state FROM {table} WHERE id = ?1");
        Connection::open(&self.database_path)
            .unwrap()
            .query_row(&query, params![task_id], |row| row.get(0))
            .unwrap()
    }

    fn task_count(&self, table: &str, column: &str, task_id: &str) -> i64 {
        let query = format!("SELECT COUNT(*) FROM {table} WHERE {column} = ?1");
        Connection::open(&self.database_path)
            .unwrap()
            .query_row(&query, params![task_id], |row| row.get(0))
            .unwrap()
    }

    /// 直接写入一条安装任务记录（绕过计划解析，仅用于状态机与删除路径测试）。
    fn insert_install_task_row(&self, state: &str) -> String {
        let task_id = uuid::Uuid::new_v4().to_string();
        let staging = self
            .data_directory()
            .join(".staging")
            .join("install")
            .join(&task_id);
        fs::create_dir_all(&staging).unwrap();
        Connection::open(&self.database_path)
            .unwrap()
            .execute(
                "
                INSERT INTO install_tasks (
                    id, state, current_stage, plan_json, staging_directory,
                    target_directory, created_at_unix_seconds, updated_at_unix_seconds
                ) VALUES (?1, ?2, 'download_game_files', '{}', ?3, ?4, 1, 1)
                ",
                params![
                    task_id,
                    state,
                    staging.to_string_lossy(),
                    self.data_directory()
                        .join("instances")
                        .join(&task_id)
                        .to_string_lossy()
                ],
            )
            .unwrap();
        task_id
    }

    fn staging_directory(&self, kind: &str, task_id: &str) -> PathBuf {
        self.data_directory()
            .join(".staging")
            .join(kind)
            .join(task_id)
    }

    /// 单文件内容计划，URL 指向给定服务器，哈希按实际字节计算。
    fn single_file_plan(&self, url: String, bytes: &[u8]) -> ContentInstallPlan {
        ContentInstallPlan {
            schema_version: 1,
            instance_id: "instance-control".to_owned(),
            instance_name: "控制测试".to_owned(),
            game_version: "26.2".to_owned(),
            loader: "fabric".to_owned(),
            root_project_id: "ROOT0001".to_owned(),
            entries: vec![ContentPlanEntry {
                project_id: "ROOT0001".to_owned(),
                version_id: "ROOTVER1".to_owned(),
                project_title: "控制模组".to_owned(),
                version_number: "1.0.0".to_owned(),
                required_by_project_id: None,
                file: ContentFilePlan {
                    url,
                    filename: "slow-mod.jar".to_owned(),
                    size: u64::try_from(bytes.len()).unwrap(),
                    sha1: digest_sha1(bytes),
                    sha512: digest_sha512(bytes),
                },
            }],
            optional_dependencies: Vec::<ContentDependencyChoice>::new(),
            incompatible_dependencies: Vec::<ContentDependencyChoice>::new(),
            is_update: false,
        }
    }

    fn task_states(&self) -> HashMap<String, TaskState> {
        self.service
            .list_content_install_tasks()
            .unwrap()
            .into_iter()
            .map(|task| (task.id, task.state))
            .collect()
    }
}

fn artifact(url: &str, name: &str, expected: &[u8]) -> ResolvedArtifact {
    ResolvedArtifact {
        kind: ArtifactKind::GameClient,
        relative_path: format!("minecraft/versions/test/{name}"),
        url: url.to_owned(),
        size: u64::try_from(expected.len()).unwrap(),
        sha1: Some(digest_sha1(expected)),
        sha256: None,
        sha512: Some(digest_sha512(expected)),
    }
}

fn digest_sha1(bytes: &[u8]) -> String {
    let mut hasher = Sha1::new();
    hasher.update(bytes);
    encode_hex(hasher.finalize())
}

fn digest_sha512(bytes: &[u8]) -> String {
    let mut hasher = Sha512::new();
    hasher.update(bytes);
    encode_hex(hasher.finalize())
}

fn encode_hex(bytes: impl AsRef<[u8]>) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let bytes = bytes.as_ref();
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}

struct ExecutorServer {
    address: std::net::SocketAddr,
    _thread: thread::JoinHandle<()>,
}

impl ExecutorServer {
    fn new(responses: HashMap<String, Vec<u8>>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let responses = Arc::new(responses);
        let server_thread = thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                let responses = Arc::clone(&responses);
                thread::spawn(move || serve_bytes(stream, &responses));
            }
        });
        Self {
            address,
            _thread: server_thread,
        }
    }

    fn url(&self, path: &str) -> String {
        format!("http://{}{path}", self.address)
    }
}

fn serve_bytes(mut stream: TcpStream, responses: &HashMap<String, Vec<u8>>) {
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut request_line = String::new();
    let _ = reader.read_line(&mut request_line);
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
            break;
        }
    }
    let path = request_line.split_whitespace().nth(1).unwrap_or("/");
    let Some(body) = responses.get(path) else {
        write!(
            stream,
            "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        )
        .unwrap();
        return;
    };
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .unwrap();
    let _ = stream.write_all(body);
    let _ = stream.flush();
}

// ---------- M33: 单任务取消/删除、瞬态重试、下载并发设置 ----------

#[test]
fn m33_task_001_cancel_state_machine_and_terminal_rejection() {
    let fixture = TaskFixture::new();
    fixture.insert_instance("instance-control", "控制测试");

    // 内容任务:排队、暂停、运行中均可取消。
    let queued = fixture.enqueue_content_task("instance-control", "queued.jar");
    fixture.service.cancel_content_task(&queued).unwrap();
    assert_eq!(fixture.task_states()[&queued], TaskState::Cancelled);

    let paused = fixture.enqueue_content_task("instance-control", "paused.jar");
    fixture
        .service
        .mark_content_task_paused(&paused, "user")
        .unwrap();
    fixture.service.cancel_content_task(&paused).unwrap();
    assert_eq!(fixture.task_states()[&paused], TaskState::Cancelled);

    let running = fixture.enqueue_content_task("instance-control", "running.jar");
    fixture.set_task_running("content_install_tasks", &running);
    fixture.service.cancel_content_task(&running).unwrap();
    assert_eq!(fixture.task_states()[&running], TaskState::Cancelled);
    assert!(
        !fixture
            .service
            .queued_content_tasks_by_priority()
            .unwrap()
            .contains(&running),
        "已取消任务不得再被调度"
    );

    // 终态任务拒绝取消,状态保持。
    for (state, expected) in [
        ("failed", TaskState::Failed),
        ("completed", TaskState::Completed),
        ("cancelled", TaskState::Cancelled),
    ] {
        let terminal = fixture.enqueue_content_task("instance-control", &format!("{state}.jar"));
        fixture.set_task_state("content_install_tasks", &terminal, state);
        fixture
            .service
            .cancel_content_task(&terminal)
            .expect_err("终态任务不能取消");
        assert_eq!(fixture.task_states()[&terminal], expected);
    }

    // 安装任务:同一状态机。
    let install_queued = fixture.insert_install_task_row("queued");
    fixture
        .service
        .cancel_install_task(&install_queued)
        .unwrap();
    assert_eq!(
        fixture.task_state("install_tasks", &install_queued),
        "cancelled"
    );
    let install_paused = fixture.insert_install_task_row("paused");
    fixture
        .service
        .cancel_install_task(&install_paused)
        .unwrap();
    assert_eq!(
        fixture.task_state("install_tasks", &install_paused),
        "cancelled"
    );
    let install_running = fixture.insert_install_task_row("running");
    fixture
        .service
        .cancel_install_task(&install_running)
        .unwrap();
    assert_eq!(
        fixture.task_state("install_tasks", &install_running),
        "cancelled"
    );
    for state in ["failed", "completed", "cancelled"] {
        let terminal = fixture.insert_install_task_row(state);
        fixture
            .service
            .cancel_install_task(&terminal)
            .expect_err("终态安装任务不能取消");
        assert_eq!(fixture.task_state("install_tasks", &terminal), state);
    }
}

#[tokio::test]
async fn m33_task_002_cancelled_queued_task_is_never_executed() {
    let fixture = TaskFixture::new();
    fixture.insert_instance("instance-control", "控制测试");
    let task_id = fixture.enqueue_content_task("instance-control", "cancelled.jar");

    fixture.service.cancel_content_task(&task_id).unwrap();
    let error = ContentExecutor::new(2)
        .unwrap()
        .execute_task(&fixture.service, &task_id)
        .await
        .expect_err("已取消任务不能被误执行");
    assert!(error.to_string().contains("不能开始执行"));
    assert_eq!(fixture.task_states()[&task_id], TaskState::Cancelled);
}

#[tokio::test]
async fn m33_task_003_cancel_mid_run_is_not_overwritten_by_executor() {
    let fixture = TaskFixture::new();
    fixture.insert_instance("instance-control", "控制测试");
    // 慢速来源:分块发送,为执行中取消留出确定窗口。
    let body: Vec<u8> = (0..8 * 16 * 1024_u32)
        .map(|index| (index % 251) as u8)
        .collect();
    let server = SlowServer::new(body.clone(), Duration::from_millis(40));
    let plan = fixture.single_file_plan(server.url("/slow-mod.jar"), &body);
    let task = fixture.service.enqueue_content_install_task(&plan).unwrap();

    let service = fixture.service.clone();
    let task_id = task.id.clone();
    let handle = tokio::spawn(async move {
        ContentExecutor::new(2)
            .unwrap()
            .execute_task(&service, &task_id)
            .await
    });
    // 等执行器进入下载阶段再取消;无论取消落在哪个阶段,终态都必须是已取消。
    tokio::time::sleep(Duration::from_millis(100)).await;
    fixture.service.cancel_content_task(&task.id).unwrap();

    let result = handle.await.unwrap();
    result.expect_err("已取消任务不得报告执行成功");
    assert_eq!(
        fixture.task_states()[&task.id],
        TaskState::Cancelled,
        "执行器收尾不得覆盖已取消状态"
    );
    assert!(
        !fixture
            .data_directory()
            .join("instances/instance-control/.minecraft/mods/slow-mod.jar")
            .exists(),
        "已取消任务不得发布模组文件"
    );
}

#[test]
fn m33_task_004_delete_terminal_task_removes_record_and_staging() {
    let fixture = TaskFixture::new();
    fixture.insert_instance("instance-control", "控制测试");

    let task = fixture.enqueue_content_task("instance-control", "failed.jar");
    fixture.set_task_state("content_install_tasks", &task, "failed");
    let staging = fixture.staging_directory("content", &task);
    fs::write(staging.join("leftover.bin"), b"partial").unwrap();
    assert_eq!(
        fixture.task_count("content_task_progress", "task_id", &task),
        1
    );

    fixture.service.delete_content_task(&task).unwrap();

    assert_eq!(fixture.task_count("content_install_tasks", "id", &task), 0);
    assert_eq!(
        fixture.task_count("content_task_progress", "task_id", &task),
        0,
        "进度记录应随外键级联删除"
    );
    assert!(!staging.exists(), "暂存目录应被递归清理");

    // 已取消与已完成同样可删除。
    for state in ["cancelled", "completed"] {
        let done = fixture.enqueue_content_task("instance-control", &format!("{state}.jar"));
        fixture.set_task_state("content_install_tasks", &done, state);
        fixture.service.delete_content_task(&done).unwrap();
        assert_eq!(fixture.task_count("content_install_tasks", "id", &done), 0);
    }

    // 安装任务:失败记录删除并清理暂存。
    let install = fixture.insert_install_task_row("failed");
    let staging = fixture.staging_directory("install", &install);
    assert!(staging.exists());
    fixture.service.delete_install_task(&install).unwrap();
    assert_eq!(fixture.task_count("install_tasks", "id", &install), 0);
    assert!(!staging.exists());
}

#[test]
fn m33_task_005_delete_rejects_non_terminal_and_foreign_staging() {
    let fixture = TaskFixture::new();
    fixture.insert_instance("instance-control", "控制测试");

    // 非终态拒绝删除,记录保留。
    let queued = fixture.enqueue_content_task("instance-control", "queued.jar");
    fixture
        .service
        .delete_content_task(&queued)
        .expect_err("排队任务不能删除");
    assert_eq!(
        fixture.task_count("content_install_tasks", "id", &queued),
        1
    );

    // 路径穿越保护:暂存路径被篡改为受管区外,拒绝清理且记录保留。
    let failed = fixture.enqueue_content_task("instance-control", "failed.jar");
    fixture.set_task_state("content_install_tasks", &failed, "failed");
    let outside = fixture.data_directory().join("instances");
    Connection::open(&fixture.database_path)
        .unwrap()
        .execute(
            "UPDATE content_install_tasks SET staging_directory = ?2 WHERE id = ?1",
            params![failed, outside.to_string_lossy()],
        )
        .unwrap();
    let error = fixture
        .service
        .delete_content_task(&failed)
        .expect_err("受管区外暂存路径必须拒绝");
    assert!(error.to_string().contains("暂存路径"));
    assert_eq!(
        fixture.task_count("content_install_tasks", "id", &failed),
        1
    );
    assert!(outside.exists(), "受管区外目录不得被清理");

    // 安装任务同样拒绝非终态与区外路径。
    let running = fixture.insert_install_task_row("running");
    fixture
        .service
        .delete_install_task(&running)
        .expect_err("运行中任务不能删除");
    assert_eq!(fixture.task_count("install_tasks", "id", &running), 1);
    let tampered = fixture.insert_install_task_row("failed");
    Connection::open(&fixture.database_path)
        .unwrap()
        .execute(
            "UPDATE install_tasks SET staging_directory = ?2 WHERE id = ?1",
            params![tampered, outside.to_string_lossy()],
        )
        .unwrap();
    fixture
        .service
        .delete_install_task(&tampered)
        .expect_err("受管区外暂存路径必须拒绝");
    assert_eq!(fixture.task_count("install_tasks", "id", &tampered), 1);
}

#[test]
fn m33_task_006_download_concurrency_persists_and_validates() {
    let fixture = TaskFixture::new();
    assert_eq!(fixture.service.download_concurrency().unwrap(), 24);
    assert_eq!(
        fixture.service.download_concurrency().unwrap(),
        moyumax_core::DEFAULT_DOWNLOAD_CONCURRENCY
    );

    fixture.service.set_download_concurrency(16).unwrap();
    assert_eq!(fixture.service.download_concurrency().unwrap(), 16);
    fixture.service.set_download_concurrency(1).unwrap();
    fixture.service.set_download_concurrency(32).unwrap();
    fixture
        .service
        .set_download_concurrency(0)
        .expect_err("0 越界必须拒绝");
    fixture
        .service
        .set_download_concurrency(33)
        .expect_err("33 越界必须拒绝");
    assert_eq!(
        fixture.service.download_concurrency().unwrap(),
        32,
        "越界写入不得生效"
    );

    Connection::open(&fixture.database_path)
        .unwrap()
        .execute(
            "INSERT OR REPLACE INTO app_settings (key, value) VALUES ('download_concurrency', 'abc')",
            [],
        )
        .unwrap();
    fixture
        .service
        .download_concurrency()
        .expect_err("损坏设置必须如实报错");
}

#[tokio::test]
async fn m33_task_007_transient_server_error_retries_same_candidate() {
    let body: Vec<u8> = (0..300_000_u32).map(|index| (index % 253) as u8).collect();
    let server = FlakyServer::new(FlakyMode::ServerErrorThenOk, body.clone());
    let directory = TempDir::new().unwrap();
    let artifact = artifact(&server.url("/retry.jar"), "retry.jar", &body);

    let report = ArtifactDownloader::new(4)
        .unwrap()
        .fetch_with_policy(
            &artifact,
            &directory.path().join("staging"),
            &directory.path().join("shared"),
            &SourcePolicy::MirrorFirst,
            None,
        )
        .await
        .expect("首次 500 重试后应成功");

    assert_eq!(server.hits(), 2, "首次 500 应在同一候选重试一次");
    assert_eq!(report.attempts.len(), 2, "每次重试都要记录");
    match &report.attempts[0].outcome {
        SourceAttemptOutcome::Failed { error } => {
            assert!(error.contains("500"), "重试记录应保留原始错误:{error}");
        }
        other => panic!("首次尝试应记录失败:{other:?}"),
    }
    assert_eq!(report.attempts[1].outcome, SourceAttemptOutcome::Success);
}

#[tokio::test]
async fn m33_task_008_broken_stream_retries_same_candidate() {
    let body: Vec<u8> = (0..200_000_u32).map(|index| (index % 241) as u8).collect();
    // 断流:首次响应 Content-Length 大于实际写出字节。
    let server = FlakyServer::new(FlakyMode::TruncatedThenOk, body.clone());
    let directory = TempDir::new().unwrap();
    let artifact = artifact(&server.url("/stream.jar"), "stream.jar", &body);

    let report = ArtifactDownloader::new(4)
        .unwrap()
        .fetch_with_policy(
            &artifact,
            &directory.path().join("staging"),
            &directory.path().join("shared"),
            &SourcePolicy::MirrorFirst,
            None,
        )
        .await
        .expect("断流重试后应成功");

    assert_eq!(server.hits(), 2);
    assert_eq!(report.attempts.len(), 2);
    assert_eq!(report.attempts[1].outcome, SourceAttemptOutcome::Success);
}

#[tokio::test]
async fn m33_task_009_retry_budget_is_bounded() {
    let body: Vec<u8> = (0..100_000_u32).map(|index| (index % 239) as u8).collect();
    let server = FlakyServer::new(FlakyMode::AlwaysServerError, body.clone());
    let directory = TempDir::new().unwrap();
    let artifact = artifact(&server.url("/down.jar"), "down.jar", &body);

    let error = ArtifactDownloader::new(4)
        .unwrap()
        .fetch_with_policy(
            &artifact,
            &directory.path().join("staging"),
            &directory.path().join("shared"),
            &SourcePolicy::MirrorFirst,
            None,
        )
        .await
        .expect_err("持续 500 应在重试预算内放弃");

    assert!(error.to_string().contains("500"));
    assert_eq!(server.hits(), 4, "同一候选最多重试 3 次(共 4 次尝试)");
}

#[tokio::test]
async fn m33_task_010_hash_mismatch_and_client_error_never_retry() {
    let body: Vec<u8> = (0..100_000_u32).map(|index| (index % 239) as u8).collect();

    // 校验失败:来源返回长度相同但内容错误的字节,不得在同一候选重试。
    let server = FlakyServer::new(FlakyMode::WrongBody, body.clone());
    let directory = TempDir::new().unwrap();
    let tampered = artifact(&server.url("/tampered.jar"), "tampered.jar", &body);
    let error = ArtifactDownloader::new(4)
        .unwrap()
        .fetch_with_policy(
            &tampered,
            &directory.path().join("staging"),
            &directory.path().join("shared"),
            &SourcePolicy::MirrorFirst,
            None,
        )
        .await
        .expect_err("哈希不匹配必须失败");
    assert!(error.to_string().contains("SHA-1"));
    assert_eq!(server.hits(), 1, "校验失败不得重试");

    // 404 是确定性错误,同样不重试。
    let not_found = FlakyServer::new(FlakyMode::AlwaysNotFound, body.clone());
    let missing = artifact(&not_found.url("/missing.jar"), "missing.jar", &body);
    let error = ArtifactDownloader::new(4)
        .unwrap()
        .fetch_with_policy(
            &missing,
            &directory.path().join("staging2"),
            &directory.path().join("shared2"),
            &SourcePolicy::MirrorFirst,
            None,
        )
        .await
        .expect_err("404 必须失败");
    assert!(error.to_string().contains("404"));
    assert_eq!(not_found.hits(), 1, "HTTP 4xx 不得重试");
}

/// 慢速来源:分块发送并在块间睡眠,为执行中取消留出确定窗口。
struct SlowServer {
    address: std::net::SocketAddr,
    _thread: thread::JoinHandle<()>,
}

impl SlowServer {
    fn new(body: Vec<u8>, chunk_delay: Duration) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server_thread = thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                let body = body.clone();
                thread::spawn(move || serve_slow(stream, &body, chunk_delay));
            }
        });
        Self {
            address,
            _thread: server_thread,
        }
    }

    fn url(&self, path: &str) -> String {
        format!("http://{}{path}", self.address)
    }
}

fn serve_slow(mut stream: TcpStream, body: &[u8], chunk_delay: Duration) {
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut request_line = String::new();
    let _ = reader.read_line(&mut request_line);
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
            break;
        }
    }
    if write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .is_err()
    {
        return;
    }
    for chunk in body.chunks(16 * 1024) {
        if stream.write_all(chunk).is_err() {
            return;
        }
        let _ = stream.flush();
        thread::sleep(chunk_delay);
    }
}

/// 可按命中次数改变行为的来源桩,用于瞬态重试语义验证。
#[derive(Clone, Copy)]
enum FlakyMode {
    /// 首次 500,之后 200。
    ServerErrorThenOk,
    /// 首次声明超长 Content-Length 后提前关闭(断流),之后 200。
    TruncatedThenOk,
    /// 首次 429,之后 200。
    TooManyRequestsThenOk,
    /// 首次返回 HTML 挑战页(反爬),之后 200。
    HtmlThenOk,
    AlwaysNotFound,
    AlwaysServerError,
    /// 始终 200,但返回与预期等长的错误字节(校验失败)。
    WrongBody,
}

struct FlakyServer {
    address: std::net::SocketAddr,
    hits: Arc<AtomicUsize>,
    _thread: thread::JoinHandle<()>,
}

impl FlakyServer {
    fn new(mode: FlakyMode, body: Vec<u8>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let hits = Arc::new(AtomicUsize::new(0));
        let server_hits = Arc::clone(&hits);
        let server_thread = thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                let body = body.clone();
                let hits = Arc::clone(&server_hits);
                thread::spawn(move || serve_flaky(stream, mode, &body, &hits));
            }
        });
        Self {
            address,
            hits,
            _thread: server_thread,
        }
    }

    fn url(&self, path: &str) -> String {
        format!("http://{}{path}", self.address)
    }

    fn hits(&self) -> usize {
        self.hits.load(Ordering::SeqCst)
    }
}

fn serve_flaky(mut stream: TcpStream, mode: FlakyMode, body: &[u8], hits: &AtomicUsize) {
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut request_line = String::new();
    let _ = reader.read_line(&mut request_line);
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
            break;
        }
    }
    let hit = hits.fetch_add(1, Ordering::SeqCst) + 1;
    match mode {
        FlakyMode::AlwaysNotFound => {
            write!(
                stream,
                "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            )
            .unwrap();
        }
        FlakyMode::AlwaysServerError
        | FlakyMode::ServerErrorThenOk
        | FlakyMode::TruncatedThenOk
            if matches!(mode, FlakyMode::AlwaysServerError) || hit == 1 =>
        {
            match mode {
                FlakyMode::TruncatedThenOk => {
                    // 声明完整 Content-Length,只写出前半部分后关闭:客户端读到断流。
                    write!(
                        stream,
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    )
                    .unwrap();
                    let _ = stream.write_all(&body[..body.len() / 2]);
                    let _ = stream.flush();
                }
                _ => {
                    write!(
                        stream,
                        "HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    )
                    .unwrap();
                }
            }
        }
        FlakyMode::TooManyRequestsThenOk if hit == 1 => {
            write!(
                stream,
                "HTTP/1.1 429 Too Many Requests\r\nContent-Length: 17\r\nConnection: close\r\n\r\nToo Many Requests"
            )
            .unwrap();
        }
        FlakyMode::HtmlThenOk if hit == 1 => {
            let page = b"<script>function a(a){for(var b=0;b<1;b++){}}</script>";
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                page.len()
            )
            .unwrap();
            let _ = stream.write_all(page);
            let _ = stream.flush();
        }
        FlakyMode::WrongBody => {
            let wrong = vec![0xAB_u8; body.len()];
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                wrong.len()
            )
            .unwrap();
            let _ = stream.write_all(&wrong);
            let _ = stream.flush();
        }
        _ => {
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )
            .unwrap();
            let _ = stream.write_all(body);
            let _ = stream.flush();
        }
    }
}

#[tokio::test]
async fn m33_task_011_too_many_requests_retries_same_candidate() {
    let body = b"asset bytes after rate limit".to_vec();
    let server = FlakyServer::new(FlakyMode::TooManyRequestsThenOk, body.clone());
    let directory = TempDir::new().unwrap();
    let artifact = artifact(&server.url("/rate.bin"), "rate.bin", &body);

    let report = ArtifactDownloader::new(4)
        .unwrap()
        .fetch_with_policy(
            &artifact,
            &directory.path().join("staging"),
            &directory.path().join("shared"),
            &SourcePolicy::MirrorFirst,
            None,
        )
        .await
        .expect("429 属瞬态错误,重试后应成功");

    assert_eq!(server.hits(), 2, "首次 429 应在同一候选重试一次");
    assert_eq!(report.attempts.len(), 2);
    assert_eq!(report.attempts[1].outcome, SourceAttemptOutcome::Success);
}

#[tokio::test]
async fn m33_task_012_html_challenge_page_retries_then_succeeds() {
    let body = b"\x4f\x67\x67\x53 real binary object bytes".to_vec();
    let server = FlakyServer::new(FlakyMode::HtmlThenOk, body.clone());
    let directory = TempDir::new().unwrap();
    let artifact = artifact(&server.url("/object.bin"), "object.bin", &body);

    let report = ArtifactDownloader::new(4)
        .unwrap()
        .fetch_with_policy(
            &artifact,
            &directory.path().join("staging"),
            &directory.path().join("shared"),
            &SourcePolicy::MirrorFirst,
            None,
        )
        .await
        .expect("HTML 挑战页按瞬态重试后应拿到真文件");

    assert_eq!(server.hits(), 2, "挑战页应在同一候选重试一次");
    match &report.attempts[0].outcome {
        SourceAttemptOutcome::Failed { error } => {
            assert!(error.contains("网页"), "首次失败应说明返回了网页:{error}");
        }
        other => panic!("首次尝试应记录失败:{other:?}"),
    }
    assert_eq!(report.attempts[1].outcome, SourceAttemptOutcome::Success);
}

#[test]
fn m33_task_013_cancel_and_delete_release_java_environment_claim() {
    let fixture = TaskFixture::new();
    let env_id = "java-env-shared";
    let java_home = fixture
        .data_directory()
        .join("store/java/azul-zulu/21.0.12+8/x64");
    Connection::open(&fixture.database_path)
        .unwrap()
        .execute(
            "
            INSERT INTO managed_java_environments (
                id, distribution, full_version, architecture, home_directory, status
            ) VALUES (?1, 'azul-zulu', '21.0.12+8', 'x64', ?2, 'installing')
            ",
            params![env_id, java_home.to_string_lossy()],
        )
        .unwrap();
    let env_status = || {
        Connection::open(&fixture.database_path)
            .unwrap()
            .query_row(
                "SELECT status FROM managed_java_environments WHERE id = ?1",
                params![env_id],
                |row| row.get::<_, String>(0),
            )
            .unwrap()
    };

    // 有运行中的安装任务:活动安装者存在;取消后环境必须落为 failed。
    let task = fixture.insert_install_task_row("running");
    Connection::open(&fixture.database_path)
        .unwrap()
        .execute(
            "INSERT INTO install_task_java (task_id, environment_id, action) VALUES (?1, ?2, 'install')",
            params![task, env_id],
        )
        .unwrap();
    assert!(
        fixture.service.has_active_java_installer(env_id).unwrap(),
        "运行中的安装任务必须计为活动安装者"
    );
    fixture.service.cancel_install_task(&task).unwrap();
    assert_eq!(env_status(), "failed", "取消安装任务必须释放环境占用");
    assert!(
        !fixture.service.has_active_java_installer(env_id).unwrap(),
        "取消后不再有活动安装者"
    );

    // 删除(终态任务)同样释放环境占用——用户删除失败任务后环境不得成为孤儿。
    Connection::open(&fixture.database_path)
        .unwrap()
        .execute(
            "UPDATE managed_java_environments SET status = 'planned' WHERE id = ?1",
            params![env_id],
        )
        .unwrap();
    let task = fixture.insert_install_task_row("failed");
    Connection::open(&fixture.database_path)
        .unwrap()
        .execute(
            "INSERT INTO install_task_java (task_id, environment_id, action) VALUES (?1, ?2, 'install')",
            params![task, env_id],
        )
        .unwrap();
    fixture.service.delete_install_task(&task).unwrap();
    assert_eq!(env_status(), "failed", "删除安装任务必须释放环境占用");

    // ready 环境不受取消/删除影响(防御:绝不动已就绪环境)。
    Connection::open(&fixture.database_path)
        .unwrap()
        .execute(
            "UPDATE managed_java_environments SET status = 'ready' WHERE id = ?1",
            params![env_id],
        )
        .unwrap();
    let task = fixture.insert_install_task_row("failed");
    Connection::open(&fixture.database_path)
        .unwrap()
        .execute(
            "INSERT INTO install_task_java (task_id, environment_id, action) VALUES (?1, ?2, 'install')",
            params![task, env_id],
        )
        .unwrap();
    fixture.service.delete_install_task(&task).unwrap();
    assert_eq!(env_status(), "ready", "已就绪环境不得被改写");
}
