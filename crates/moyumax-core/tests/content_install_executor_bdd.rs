use std::{
    collections::HashMap,
    fs,
    io::{BufRead, BufReader, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::Arc,
    thread,
};

use moyumax_core::{
    AppService, ContentDependencyChoice, ContentDependencyKind, ContentExecutor, ContentFilePlan,
    ContentInstallPlan, ContentInstallStage, ContentPlanEntry, TaskState,
};
use rusqlite::{Connection, params};
use sha1::{Digest, Sha1};
use sha2::Sha512;
use tempfile::TempDir;

#[tokio::test]
async fn m5_install_001_mods_and_required_dependencies_publish_and_index_atomically() {
    let dependency = b"fabric-api-fixture".to_vec();
    let root = b"continuity-fixture".to_vec();
    let server = FixtureServer::new(HashMap::from([
        ("/fabric-api.jar".to_owned(), dependency.clone()),
        ("/continuity.jar".to_owned(), root.clone()),
    ]));
    let fixture = ContentFixture::new();
    let plan = fixture.two_file_plan(&server, &dependency, &root);
    let task = fixture.service.enqueue_content_install_task(&plan).unwrap();

    let installed = ContentExecutor::new(2)
        .unwrap()
        .execute_task(&fixture.service, &task.id)
        .await
        .expect("verified content transaction should publish all files");

    assert_eq!(installed.len(), 2);
    assert_eq!(
        fs::read(fixture.mods_directory().join("fabric-api.jar")).unwrap(),
        dependency
    );
    assert_eq!(
        fs::read(fixture.mods_directory().join("continuity.jar")).unwrap(),
        root
    );
    assert!(!Path::new(&task.staging_directory).exists());
    let tasks = fixture.service.list_content_install_tasks().unwrap();
    assert_eq!(tasks[0].state, TaskState::Completed);
    assert_eq!(
        tasks[0].current_stage,
        Some(ContentInstallStage::IndexContent)
    );
    assert_eq!(
        tasks[0].progress.completed_bytes,
        tasks[0].progress.total_bytes.unwrap()
    );
    let local = fixture
        .service
        .list_installed_content(&fixture.instance_id)
        .unwrap();
    let mut installed_by_project = installed.clone();
    installed_by_project.sort_by(|left, right| left.project_id.cmp(&right.project_id));
    let mut local_by_project = local.clone();
    local_by_project.sort_by(|left, right| left.project_id.cmp(&right.project_id));
    assert_eq!(local_by_project, installed_by_project);
    assert!(local.iter().all(|entry| !entry.auto_update_enabled));
    assert_eq!(
        count_files(&fixture.data_directory.join("store/content")),
        2
    );
}

#[tokio::test]
async fn m5_install_002_same_name_with_different_hash_never_overwrites_user_file() {
    let dependency = b"fabric-api-fixture".to_vec();
    let root = b"continuity-fixture".to_vec();
    let server = FixtureServer::new(HashMap::from([
        ("/fabric-api.jar".to_owned(), dependency),
        ("/continuity.jar".to_owned(), root.clone()),
    ]));
    let fixture = ContentFixture::new();
    fs::write(
        fixture.mods_directory().join("continuity.jar"),
        b"user-owned-different-content",
    )
    .unwrap();
    let plan = fixture.two_file_plan(&server, b"fabric-api-fixture", &root);
    let task = fixture.service.enqueue_content_install_task(&plan).unwrap();

    let error = ContentExecutor::new(2)
        .unwrap()
        .execute_task(&fixture.service, &task.id)
        .await
        .expect_err("a same-name hash conflict must block the whole transaction");

    assert!(error.to_string().contains("continuity.jar"));
    assert!(error.to_string().contains("冲突"));
    assert_eq!(
        fs::read(fixture.mods_directory().join("continuity.jar")).unwrap(),
        b"user-owned-different-content"
    );
    assert!(!fixture.mods_directory().join("fabric-api.jar").exists());
    assert!(
        fixture
            .service
            .list_installed_content(&fixture.instance_id)
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        fixture.service.list_content_install_tasks().unwrap()[0].state,
        TaskState::Failed
    );
}

#[tokio::test]
async fn m5_install_003_database_failure_removes_every_newly_published_file() {
    let dependency = b"fabric-api-fixture".to_vec();
    let root = b"continuity-fixture".to_vec();
    let server = FixtureServer::new(HashMap::from([
        ("/fabric-api.jar".to_owned(), dependency.clone()),
        ("/continuity.jar".to_owned(), root.clone()),
    ]));
    let fixture = ContentFixture::new();
    let plan = fixture.two_file_plan(&server, &dependency, &root);
    let task = fixture.service.enqueue_content_install_task(&plan).unwrap();
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

    let error = ContentExecutor::new(2)
        .unwrap()
        .execute_task(&fixture.service, &task.id)
        .await
        .expect_err("database failure must roll back published files");

    assert!(error.to_string().contains("fixture index failure"));
    assert!(!fixture.mods_directory().join("fabric-api.jar").exists());
    assert!(!fixture.mods_directory().join("continuity.jar").exists());
    assert!(
        fixture
            .service
            .list_installed_content(&fixture.instance_id)
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        fixture.service.list_content_install_tasks().unwrap()[0].state,
        TaskState::Failed
    );
}

#[test]
fn m5_install_003_discarding_an_interrupted_commit_removes_only_journaled_new_files() {
    let dependency = b"fabric-api-fixture".to_vec();
    let root = b"continuity-fixture".to_vec();
    let server = FixtureServer::new(HashMap::new());
    let fixture = ContentFixture::new();
    let plan = fixture.two_file_plan(&server, &dependency, &root);
    let task = fixture.service.enqueue_content_install_task(&plan).unwrap();
    fs::write(fixture.mods_directory().join("fabric-api.jar"), &dependency).unwrap();
    fs::write(fixture.mods_directory().join("user-notes.txt"), b"keep me").unwrap();
    fs::write(
        Path::new(&task.staging_directory).join("commit-journal.json"),
        serde_json::to_vec(&serde_json::json!({
            "schemaVersion": 1,
            "entries": [
                {"fileName": "fabric-api.jar", "existedBefore": false},
                {"fileName": "continuity.jar", "existedBefore": false}
            ]
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
        reopened.list_content_install_tasks().unwrap()[0].state,
        TaskState::AwaitingRecovery
    );
    reopened
        .resolve_content_task_recovery(&task.id, moyumax_core::RecoveryDecision::Discard)
        .unwrap();

    assert!(!fixture.mods_directory().join("fabric-api.jar").exists());
    assert_eq!(
        fs::read(fixture.mods_directory().join("user-notes.txt")).unwrap(),
        b"keep me"
    );
    assert!(!Path::new(&task.staging_directory).exists());
    assert_eq!(
        reopened.list_content_install_tasks().unwrap()[0].state,
        TaskState::Cancelled
    );
}

#[test]
fn m5_dependency_003_incompatible_project_inside_the_plan_blocks_enqueue() {
    let server = FixtureServer::new(HashMap::new());
    let fixture = ContentFixture::new();
    let mut plan = fixture.two_file_plan(&server, b"fabric-api-fixture", b"continuity-fixture");
    plan.incompatible_dependencies
        .push(ContentDependencyChoice {
            project_id: Some("DEP00001".to_owned()),
            version_id: None,
            title: "Fabric API conflict".to_owned(),
            kind: ContentDependencyKind::Incompatible,
            required_by_project_id: "ROOT0001".to_owned(),
        });

    let error = fixture
        .service
        .enqueue_content_install_task(&plan)
        .expect_err("an incompatible project in the closure must block enqueue");

    assert!(error.to_string().contains("Fabric API conflict"));
    assert!(error.to_string().contains("冲突"));
    assert!(
        fixture
            .service
            .list_content_install_tasks()
            .unwrap()
            .is_empty()
    );
}

struct ContentFixture {
    _directory: TempDir,
    database_path: PathBuf,
    data_directory: PathBuf,
    instance_id: String,
    instance_root: PathBuf,
    service: AppService,
}

impl ContentFixture {
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
                ) VALUES (?1, '内容测试', '26.2', 'fabric', '0.19.3', ?2, 'ready', 1)
                ",
                params![instance_id, instance_root.to_string_lossy()],
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

    fn two_file_plan(
        &self,
        server: &FixtureServer,
        dependency: &[u8],
        root: &[u8],
    ) -> ContentInstallPlan {
        ContentInstallPlan {
            schema_version: 1,
            instance_id: self.instance_id.clone(),
            instance_name: "内容测试".to_owned(),
            game_version: "26.2".to_owned(),
            loader: "fabric".to_owned(),
            root_project_id: "ROOT0001".to_owned(),
            entries: vec![
                entry(
                    "DEP00001",
                    "DEPVER01",
                    "Fabric API",
                    "fabric-api.jar",
                    server.url("/fabric-api.jar"),
                    dependency,
                    Some("ROOT0001"),
                ),
                entry(
                    "ROOT0001",
                    "ROOTVER1",
                    "Continuity",
                    "continuity.jar",
                    server.url("/continuity.jar"),
                    root,
                    None,
                ),
            ],
            optional_dependencies: Vec::<ContentDependencyChoice>::new(),
            incompatible_dependencies: Vec::<ContentDependencyChoice>::new(),
        }
    }
}

fn entry(
    project_id: &str,
    version_id: &str,
    project_title: &str,
    filename: &str,
    url: String,
    bytes: &[u8],
    required_by_project_id: Option<&str>,
) -> ContentPlanEntry {
    ContentPlanEntry {
        project_id: project_id.to_owned(),
        version_id: version_id.to_owned(),
        project_title: project_title.to_owned(),
        version_number: "1.0.0".to_owned(),
        required_by_project_id: required_by_project_id.map(str::to_owned),
        file: ContentFilePlan {
            url,
            filename: filename.to_owned(),
            size: u64::try_from(bytes.len()).unwrap(),
            sha1: digest_sha1(bytes),
            sha512: digest_sha512(bytes),
        },
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

fn count_files(root: &Path) -> usize {
    if !root.exists() {
        return 0;
    }
    fs::read_dir(root)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .map(|path| {
            if path.is_dir() {
                count_files(&path)
            } else {
                usize::from(path.is_file())
            }
        })
        .sum()
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
            for _ in 0..responses.len() {
                let (stream, _) = listener.accept().unwrap();
                serve(stream, &responses);
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

fn serve(mut stream: TcpStream, responses: &HashMap<String, Vec<u8>>) {
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
