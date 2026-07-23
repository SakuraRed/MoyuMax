use std::{
    collections::HashMap,
    fs,
    io::{BufRead, BufReader, Write},
    net::{TcpListener, TcpStream},
    path::PathBuf,
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

use moyumax_core::{
    AdaptiveConcurrency, AppService, ArtifactDownloader, ArtifactKind, ContentDependencyChoice,
    ContentExecutor, ContentFilePlan, ContentInstallPlan, ContentPlanEntry, RateLimiter,
    ResolvedArtifact, SourcePolicy, TaskState,
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

    adaptive.record(0.0, false);
    assert_eq!(adaptive.current_limit(), 4, "失败应减半");
    adaptive.record(0.0, false);
    assert_eq!(adaptive.current_limit(), 2);
    adaptive.record(0.0, false);
    assert_eq!(adaptive.current_limit(), 1, "收缩下限为 1");
    adaptive.record(0.0, false);
    assert_eq!(adaptive.current_limit(), 1);

    for _ in 0..6 {
        adaptive.record(8.0 * 1024.0 * 1024.0, true);
    }
    assert_eq!(adaptive.current_limit(), 2, "稳定后缓慢回升一格");
    for _ in 0..6 {
        adaptive.record(8.0 * 1024.0 * 1024.0, true);
    }
    assert_eq!(adaptive.current_limit(), 3);

    // 吞吐持续劣化也应收缩。
    for _ in 0..4 {
        adaptive.record(64.0 * 1024.0, true);
    }
    assert!(adaptive.current_limit() <= 2, "持续低速应收缩");
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
