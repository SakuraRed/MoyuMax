use std::{
    collections::HashMap,
    fs,
    io::{BufRead, BufReader, Write},
    net::{TcpListener, TcpStream},
    path::PathBuf,
    sync::{Arc, Mutex},
    thread,
};

use moyumax_core::{
    AppService, ContentExecutor, ContentFilePlan, ContentInstallPlan, ContentPlanEntry,
    ModrinthClient, RecoveryDecision, TaskState,
};
use rusqlite::{Connection, params};
use sha1::{Digest, Sha1};
use sha2::Sha512;
use tempfile::TempDir;

#[test]
fn m15_loader_001_quilt_forge_and_neoforge_instances_accept_modrinth_plans() {
    for (loader_kind, loader_version) in [
        ("quilt", "0.29.0"),
        ("forge", "60.0.0"),
        ("neoforge", "21.0.0"),
    ] {
        let server = FixtureServer::new(HashMap::new());
        let fixture = UpdateFixture::new(loader_kind, loader_version);
        let plan = fixture.single_file_plan(
            &server,
            "ROOT0001",
            "ROOTVER1",
            "continuity.jar",
            b"mod-bytes",
        );

        fixture
            .service
            .enqueue_content_install_task(&plan)
            .unwrap_or_else(|error| panic!("{loader_kind} 实例应接受 Modrinth 计划：{error}"));
    }
}

#[test]
fn m15_loader_001_unsupported_loader_is_still_rejected() {
    let server = FixtureServer::new(HashMap::new());
    let fixture = UpdateFixture::new("vanilla", "");
    let plan = fixture.single_file_plan(
        &server,
        "ROOT0001",
        "ROOTVER1",
        "continuity.jar",
        b"mod-bytes",
    );

    let error = fixture
        .service
        .enqueue_content_install_task(&plan)
        .expect_err("vanilla 实例不应接受 Modrinth 模组计划");

    assert!(error.to_string().contains("不支持"));
}

#[tokio::test]
async fn m15_update_001_check_updates_only_reads_metadata_and_never_downloads() {
    let old_bytes = b"continuity-v1".to_vec();
    let files = FixtureServer::new(HashMap::from([(
        "/continuity-v1.jar".to_owned(),
        old_bytes.clone(),
    )]));
    let fixture = UpdateFixture::new("fabric", "0.19.3");
    fixture
        .install_mod(
            &files,
            "ROOT0001",
            "ROOTVER1",
            "1.0.0",
            "continuity.jar",
            &old_bytes,
        )
        .await;
    let api = FixtureApi::new(
        HashMap::from([(
            "/v2/project/ROOT0001/version".to_owned(),
            serde_json::json!([
                version_json(
                    "ROOTVER2",
                    "ROOT0001",
                    "2.0.0",
                    "fabric",
                    &files,
                    "/continuity-v2.jar",
                    "continuity.jar",
                    b"continuity-v2",
                    "2026-07-22T00:00:00Z"
                ),
                version_json(
                    "ROOTVER1",
                    "ROOT0001",
                    "1.0.0",
                    "fabric",
                    &files,
                    "/continuity-v1.jar",
                    "continuity.jar",
                    &old_bytes,
                    "2026-07-01T00:00:00Z"
                ),
            ])
            .to_string(),
        )]),
        1,
    );
    let modrinth = ModrinthClient::with_base_url(&api.base_url()).unwrap();

    assert!(
        !fixture
            .service
            .instance_content_auto_update(&fixture.instance_id)
            .unwrap(),
        "内容自动更新必须默认关闭"
    );
    let updates = fixture
        .service
        .check_content_updates(&modrinth, &fixture.instance_id)
        .await
        .unwrap();

    assert_eq!(updates.len(), 1);
    assert_eq!(updates[0].project_id, "ROOT0001");
    assert_eq!(updates[0].current_version_id, "ROOTVER1");
    assert_eq!(updates[0].latest_version_id, "ROOTVER2");
    assert_eq!(updates[0].latest_version_number, "2.0.0");
    assert_eq!(
        fixture.service.list_content_install_tasks().unwrap().len(),
        1,
        "更新检查不得产生新任务"
    );
    assert_eq!(
        fs::read(fixture.mods_directory().join("continuity.jar")).unwrap(),
        old_bytes,
        "更新检查不得修改实例文件"
    );
    assert_eq!(api.requests().len(), 1, "更新检查只能读取版本元数据");
}

#[tokio::test]
async fn m15_update_002_update_replaces_old_file_and_rewrites_index_atomically() {
    let old_bytes = b"continuity-v1".to_vec();
    let new_bytes = b"continuity-v2".to_vec();
    let files = FixtureServer::new(HashMap::from([
        ("/continuity-v1.jar".to_owned(), old_bytes.clone()),
        ("/continuity-v2.jar".to_owned(), new_bytes.clone()),
    ]));
    let fixture = UpdateFixture::new("fabric", "0.19.3");
    fixture
        .install_mod(
            &files,
            "ROOT0001",
            "ROOTVER1",
            "1.0.0",
            "continuity.jar",
            &old_bytes,
        )
        .await;
    Connection::open(&fixture.database_path)
        .unwrap()
        .execute(
            "UPDATE installed_content SET enabled = 0, auto_update_enabled = 1 WHERE project_id = 'ROOT0001'",
            [],
        )
        .unwrap();
    let api = FixtureApi::new(
        HashMap::from([(
            "/v2/project/ROOT0001/version".to_owned(),
            versions_payload(&files, &old_bytes, &new_bytes),
        )]),
        3,
    );
    let modrinth = ModrinthClient::with_base_url(&api.base_url()).unwrap();

    let updates = fixture
        .service
        .check_content_updates(&modrinth, &fixture.instance_id)
        .await
        .unwrap();
    assert_eq!(updates.len(), 1);
    let task = fixture
        .service
        .plan_content_update(&modrinth, &fixture.instance_id, &["ROOT0001".to_owned()])
        .await
        .unwrap();
    assert!(task.plan.is_update);
    ContentExecutor::new(1)
        .unwrap()
        .execute_task(&fixture.service, &task.id)
        .await
        .expect("更新事务应发布新文件并重写索引");

    assert_eq!(
        fs::read(fixture.mods_directory().join("continuity.jar")).unwrap(),
        new_bytes
    );
    assert!(
        !fixture.snapshot_directory(&task.id).exists(),
        "成功后恢复点应被清理"
    );
    let installed = fixture
        .service
        .list_installed_content(&fixture.instance_id)
        .unwrap();
    assert_eq!(installed.len(), 1, "更新后同一项目只能有一行索引");
    assert_eq!(installed[0].version_id, "ROOTVER2");
    assert_eq!(installed[0].version_number, "2.0.0");
    assert!(!installed[0].enabled, "更新必须保留逐项启用标志");
    assert!(
        installed[0].auto_update_enabled,
        "更新必须保留逐项自动更新标志"
    );
    assert_eq!(
        content_task_state(&fixture.service, &task.id),
        TaskState::Completed
    );
    let remaining = fixture
        .service
        .check_content_updates(&modrinth, &fixture.instance_id)
        .await
        .unwrap();
    assert!(remaining.is_empty(), "更新后不应再提示同一版本");
}

#[tokio::test]
async fn m15_update_003_database_failure_restores_the_previous_file() {
    let old_bytes = b"continuity-v1".to_vec();
    let new_bytes = b"continuity-v2".to_vec();
    let files = FixtureServer::new(HashMap::from([
        ("/continuity-v1.jar".to_owned(), old_bytes.clone()),
        ("/continuity-v2.jar".to_owned(), new_bytes.clone()),
    ]));
    let fixture = UpdateFixture::new("fabric", "0.19.3");
    fixture
        .install_mod(
            &files,
            "ROOT0001",
            "ROOTVER1",
            "1.0.0",
            "continuity.jar",
            &old_bytes,
        )
        .await;
    let api = FixtureApi::new(
        HashMap::from([(
            "/v2/project/ROOT0001/version".to_owned(),
            versions_payload(&files, &old_bytes, &new_bytes),
        )]),
        1,
    );
    let modrinth = ModrinthClient::with_base_url(&api.base_url()).unwrap();
    let task = fixture
        .service
        .plan_content_update(&modrinth, &fixture.instance_id, &["ROOT0001".to_owned()])
        .await
        .unwrap();
    Connection::open(&fixture.database_path)
        .unwrap()
        .execute_batch(
            "
            CREATE TRIGGER fixture_reject_installed_content
            BEFORE INSERT ON installed_content
            BEGIN
                SELECT RAISE(ABORT, 'fixture index failure');
            END;
            ",
        )
        .unwrap();

    let error = ContentExecutor::new(1)
        .unwrap()
        .execute_task(&fixture.service, &task.id)
        .await
        .expect_err("索引写入失败必须回滚到更新前状态");

    assert!(error.to_string().contains("fixture index failure"));
    assert_eq!(
        fs::read(fixture.mods_directory().join("continuity.jar")).unwrap(),
        old_bytes,
        "失败时必须从恢复点放回旧文件"
    );
    assert!(
        !fixture.snapshot_directory(&task.id).exists(),
        "回滚后不得留下半成品快照"
    );
    let installed = fixture
        .service
        .list_installed_content(&fixture.instance_id)
        .unwrap();
    assert_eq!(installed.len(), 1);
    assert_eq!(installed[0].version_id, "ROOTVER1");
    assert_eq!(
        content_task_state(&fixture.service, &task.id),
        TaskState::Failed
    );
}

#[tokio::test]
async fn m15_update_004_restart_restores_previous_files_before_recovery_confirmation() {
    let old_bytes = b"continuity-v1".to_vec();
    let new_bytes = b"continuity-v2".to_vec();
    let files = FixtureServer::new(HashMap::from([(
        "/continuity-v1.jar".to_owned(),
        old_bytes.clone(),
    )]));
    let fixture = UpdateFixture::new("fabric", "0.19.3");
    fixture
        .install_mod(
            &files,
            "ROOT0001",
            "ROOTVER1",
            "1.0.0",
            "continuity.jar",
            &old_bytes,
        )
        .await;
    let api = FixtureApi::new(
        HashMap::from([(
            "/v2/project/ROOT0001/version".to_owned(),
            versions_payload(&files, &old_bytes, &new_bytes),
        )]),
        1,
    );
    let modrinth = ModrinthClient::with_base_url(&api.base_url()).unwrap();
    let task = fixture
        .service
        .plan_content_update(&modrinth, &fixture.instance_id, &["ROOT0001".to_owned()])
        .await
        .unwrap();
    // 模拟提交阶段中断：旧文件已在恢复点，新文件已落位，任务停留在 committing。
    let snapshot_directory = fixture.snapshot_directory(&task.id);
    fs::create_dir_all(&snapshot_directory).unwrap();
    fs::rename(
        fixture.mods_directory().join("continuity.jar"),
        snapshot_directory.join("continuity.jar"),
    )
    .unwrap();
    fs::write(fixture.mods_directory().join("continuity.jar"), &new_bytes).unwrap();
    fs::write(
        PathBuf::from(&task.staging_directory).join("commit-journal.json"),
        serde_json::to_vec(&serde_json::json!({
            "schemaVersion": 1,
            "entries": [{"fileName": "continuity.jar", "existedBefore": true}]
        }))
        .unwrap(),
    )
    .unwrap();
    Connection::open(&fixture.database_path)
        .unwrap()
        .execute(
            "UPDATE content_install_tasks SET state = 'committing', current_stage = 'commit_files' WHERE id = ?1",
            params![task.id],
        )
        .unwrap();

    let reopened = AppService::open(&fixture.database_path, &fixture.data_directory).unwrap();

    assert_eq!(
        fs::read(fixture.mods_directory().join("continuity.jar")).unwrap(),
        old_bytes,
        "重启后必须先把恢复点中的旧文件放回实例"
    );
    assert!(
        !snapshot_directory.exists(),
        "恢复点文件放回后应清理快照目录"
    );
    assert_eq!(
        content_task_state(&reopened, &task.id),
        TaskState::AwaitingRecovery,
        "任务必须进入恢复确认而不是伪装完成"
    );
    reopened
        .resolve_content_task_recovery(&task.id, RecoveryDecision::Discard)
        .unwrap();
    assert_eq!(
        fs::read(fixture.mods_directory().join("continuity.jar")).unwrap(),
        old_bytes
    );
    assert!(!PathBuf::from(&task.staging_directory).exists());
    assert_eq!(
        content_task_state(&reopened, &task.id),
        TaskState::Cancelled
    );
}

#[tokio::test]
async fn m15_update_005_auto_update_flag_persists_and_batch_update_builds_one_task() {
    let old_bytes = b"continuity-v1".to_vec();
    let files = FixtureServer::new(HashMap::from([(
        "/continuity-v1.jar".to_owned(),
        old_bytes.clone(),
    )]));
    let fixture = UpdateFixture::new("fabric", "0.19.3");
    fixture
        .install_mod(
            &files,
            "ROOT0001",
            "ROOTVER1",
            "1.0.0",
            "continuity.jar",
            &old_bytes,
        )
        .await;
    fixture
        .install_mod(
            &files,
            "BBB00002",
            "BBBVER01",
            "1.0.0",
            "other-mod.jar",
            &old_bytes,
        )
        .await;
    fixture
        .service
        .set_instance_content_auto_update(&fixture.instance_id, true)
        .unwrap();
    let reopened = AppService::open(&fixture.database_path, &fixture.data_directory).unwrap();
    assert!(
        reopened
            .instance_content_auto_update(&fixture.instance_id)
            .unwrap(),
        "按实例自动更新开关必须持久化"
    );

    let api = FixtureApi::new(
        HashMap::from([
            (
                "/v2/project/ROOT0001/version".to_owned(),
                serde_json::json!([version_json(
                    "ROOTVER2",
                    "ROOT0001",
                    "2.0.0",
                    "fabric",
                    &files,
                    "/continuity-v1.jar",
                    "continuity.jar",
                    &old_bytes,
                    "2026-07-22T00:00:00Z",
                )])
                .to_string(),
            ),
            (
                "/v2/project/BBB00002/version".to_owned(),
                serde_json::json!([version_json(
                    "BBBVER02",
                    "BBB00002",
                    "2.0.0",
                    "fabric",
                    &files,
                    "/continuity-v1.jar",
                    "other-mod.jar",
                    &old_bytes,
                    "2026-07-22T00:00:00Z",
                )])
                .to_string(),
            ),
        ]),
        2,
    );
    let modrinth = ModrinthClient::with_base_url(&api.base_url()).unwrap();

    let task = reopened
        .plan_content_update(
            &modrinth,
            &fixture.instance_id,
            &["ROOT0001".to_owned(), "BBB00002".to_owned()],
        )
        .await
        .expect("开启自动更新后应提供用户明确触发的全部更新入口");

    assert!(task.plan.is_update);
    assert_eq!(task.plan.entries.len(), 2);
    assert_eq!(task.state, TaskState::Queued);
    let error = reopened
        .plan_content_update(&modrinth, &fixture.instance_id, &[])
        .await
        .expect_err("空选择必须被拒绝");
    assert!(error.to_string().contains("没有选择"));
    let error = reopened
        .plan_content_update(&modrinth, &fixture.instance_id, &["MISSING0".to_owned()])
        .await
        .expect_err("未安装项目不能更新");
    assert!(error.to_string().contains("未安装"));
}

struct UpdateFixture {
    _directory: TempDir,
    database_path: PathBuf,
    data_directory: PathBuf,
    instance_id: String,
    instance_root: PathBuf,
    service: AppService,
}

impl UpdateFixture {
    fn new(loader_kind: &str, loader_version: &str) -> Self {
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
                ) VALUES (?1, '更新测试', '26.2', ?2, ?3, ?4, 'ready', 1)
                ",
                params![
                    instance_id,
                    loader_kind,
                    if loader_version.is_empty() {
                        None
                    } else {
                        Some(loader_version)
                    },
                    instance_root.to_string_lossy()
                ],
            )
            .unwrap();
        Self {
            _directory: directory,
            database_path,
            data_directory,
            instance_id,
            instance_root,
            service,
        }
    }

    fn mods_directory(&self) -> PathBuf {
        self.instance_root.join(".minecraft/mods")
    }

    fn snapshot_directory(&self, task_id: &str) -> PathBuf {
        self.instance_root
            .join(".moyumax")
            .join("snapshots")
            .join(format!("content-update-{task_id}"))
    }

    fn single_file_plan(
        &self,
        server: &FixtureServer,
        project_id: &str,
        version_id: &str,
        filename: &str,
        bytes: &[u8],
    ) -> ContentInstallPlan {
        let loader_kind: String = Connection::open(&self.database_path)
            .unwrap()
            .query_row(
                "SELECT loader_kind FROM instances WHERE id = ?1",
                params![self.instance_id],
                |row| row.get(0),
            )
            .unwrap();
        ContentInstallPlan {
            schema_version: 1,
            instance_id: self.instance_id.clone(),
            instance_name: "更新测试".to_owned(),
            game_version: "26.2".to_owned(),
            loader: loader_kind,
            root_project_id: project_id.to_owned(),
            entries: vec![plan_entry(
                project_id,
                version_id,
                "1.0.0",
                filename,
                server.url("/continuity-v1.jar"),
                bytes,
            )],
            optional_dependencies: Vec::new(),
            incompatible_dependencies: Vec::new(),
            is_update: false,
        }
    }

    async fn install_mod(
        &self,
        server: &FixtureServer,
        project_id: &str,
        version_id: &str,
        version_number: &str,
        filename: &str,
        bytes: &[u8],
    ) {
        let mut plan = self.single_file_plan(server, project_id, version_id, filename, bytes);
        plan.entries[0].version_number = version_number.to_owned();
        let task = self.service.enqueue_content_install_task(&plan).unwrap();
        ContentExecutor::new(1)
            .unwrap()
            .execute_task(&self.service, &task.id)
            .await
            .expect("预置安装必须成功");
    }
}

fn plan_entry(
    project_id: &str,
    version_id: &str,
    version_number: &str,
    filename: &str,
    url: String,
    bytes: &[u8],
) -> ContentPlanEntry {
    ContentPlanEntry {
        project_id: project_id.to_owned(),
        version_id: version_id.to_owned(),
        project_title: format!("项目 {project_id}"),
        version_number: version_number.to_owned(),
        required_by_project_id: None,
        file: ContentFilePlan {
            url,
            filename: filename.to_owned(),
            size: u64::try_from(bytes.len()).unwrap(),
            sha1: digest_sha1(bytes),
            sha512: digest_sha512(bytes),
        },
    }
}

fn versions_payload(files: &FixtureServer, old_bytes: &[u8], new_bytes: &[u8]) -> String {
    serde_json::json!([
        version_json(
            "ROOTVER2",
            "ROOT0001",
            "2.0.0",
            "fabric",
            files,
            "/continuity-v2.jar",
            "continuity.jar",
            new_bytes,
            "2026-07-22T00:00:00Z"
        ),
        version_json(
            "ROOTVER1",
            "ROOT0001",
            "1.0.0",
            "fabric",
            files,
            "/continuity-v1.jar",
            "continuity.jar",
            old_bytes,
            "2026-07-01T00:00:00Z"
        ),
    ])
    .to_string()
}

#[allow(clippy::too_many_arguments)]
fn version_json(
    id: &str,
    project_id: &str,
    version_number: &str,
    loader: &str,
    files: &FixtureServer,
    path: &str,
    filename: &str,
    bytes: &[u8],
    date_published: &str,
) -> serde_json::Value {
    serde_json::json!({
        "id": id,
        "project_id": project_id,
        "name": format!("{project_id} {version_number}"),
        "version_number": version_number,
        "game_versions": ["26.2"],
        "loaders": [loader],
        "version_type": "release",
        "status": "listed",
        "date_published": date_published,
        "dependencies": [],
        "files": [{
            "hashes": {
                "sha1": digest_sha1(bytes),
                "sha512": digest_sha512(bytes)
            },
            "url": files.url(path),
            "filename": filename,
            "primary": true,
            "size": bytes.len(),
            "file_type": null
        }]
    })
}

fn content_task_state(service: &AppService, task_id: &str) -> TaskState {
    service
        .list_content_install_tasks()
        .unwrap()
        .into_iter()
        .find(|task| task.id == task_id)
        .unwrap()
        .state
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

struct FixtureServer {
    address: std::net::SocketAddr,
    _thread: thread::JoinHandle<()>,
}

impl FixtureServer {
    fn new(responses: HashMap<String, Vec<u8>>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let responses = Arc::new(responses);
        let server_thread = thread::spawn(move || {
            for stream in listener.incoming() {
                match stream {
                    Ok(stream) => serve_file(stream, &responses),
                    Err(_) => break,
                }
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

fn serve_file(mut stream: TcpStream, responses: &HashMap<String, Vec<u8>>) {
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut request_line = String::new();
    reader.read_line(&mut request_line).unwrap();
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
            break;
        }
    }
    let path = request_line.split_whitespace().nth(1).unwrap();
    let body = responses
        .get(path)
        .unwrap_or_else(|| panic!("unexpected fixture route: {path}"));
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .unwrap();
    stream.write_all(body).unwrap();
    stream.flush().unwrap();
}

struct FixtureApi {
    address: std::net::SocketAddr,
    requests: Arc<Mutex<Vec<String>>>,
    _thread: thread::JoinHandle<()>,
}

impl FixtureApi {
    fn new(responses: HashMap<String, String>, request_count: usize) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let captured = Arc::clone(&requests);
        let server_thread = thread::spawn(move || {
            for _ in 0..request_count {
                let (stream, _) = listener.accept().unwrap();
                serve_metadata(stream, &responses, &captured);
            }
        });
        Self {
            address,
            requests,
            _thread: server_thread,
        }
    }

    fn base_url(&self) -> String {
        format!("http://{}/v2/", self.address)
    }

    fn requests(&self) -> Vec<String> {
        self.requests.lock().unwrap().clone()
    }
}

fn serve_metadata(
    mut stream: TcpStream,
    responses: &HashMap<String, String>,
    requests: &Arc<Mutex<Vec<String>>>,
) {
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut request = String::new();
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
            break;
        }
        request.push_str(&line);
    }
    let path = request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap();
    let route = path.split('?').next().unwrap();
    let response = responses
        .get(route)
        .unwrap_or_else(|| panic!("unexpected route: {route}"));
    requests.lock().unwrap().push(request);
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        response.len(),
        response
    )
    .unwrap();
    stream.flush().unwrap();
}
