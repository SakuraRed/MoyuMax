use std::{
    collections::HashMap,
    fs,
    io::{BufRead, BufReader, Write},
    net::{TcpListener, TcpStream},
    path::PathBuf,
    sync::Arc,
    thread,
};

use moyumax_core::{
    AppService, ArtifactDownloader, ArtifactKind, JavaArchitecture, JavaDeleteOutcome,
    JavaDistribution, JavaEnvironmentStatus, LaunchAccount, LaunchOptions, ManagedJavaEnvironment,
    ResolvedArtifact, ResolvedJavaPackage,
};
use rusqlite::{Connection, params};
use serde_json::json;
use sha2::{Digest as Sha2Digest, Sha256};
use tempfile::TempDir;
use zip::{ZipWriter, write::SimpleFileOptions};

#[test]
fn m13_java_001_list_environments_with_references_and_health() {
    let fixture = JavaFixture::new();
    fixture.register_java("env-21", "21.0.12+8", JavaEnvironmentStatus::Ready);
    fixture.register_instance("instance-a", "实例甲", "env-21", 21);

    let environments = fixture.service.list_java_environments().unwrap();
    assert_eq!(environments.len(), 1);
    let environment = &environments[0];
    assert_eq!(environment.full_version, "21.0.12+8");
    assert_eq!(environment.architecture, JavaArchitecture::X64);
    assert!(environment.healthy, "bin/java.exe 存在应健康");
    assert!(environment.size_bytes > 0);
    assert_eq!(environment.referencing_instances.len(), 1);
    assert_eq!(environment.referencing_instances[0].name, "实例甲");
}

#[test]
fn m13_java_002_delete_referenced_requires_confirmation_then_tombstone() {
    let fixture = JavaFixture::new();
    fixture.register_java("env-21", "21.0.12+8", JavaEnvironmentStatus::Ready);
    fixture.register_instance("instance-a", "实例甲", "env-21", 21);
    fixture.register_instance("instance-b", "实例乙", "env-21", 21);
    let home = fixture.java_home("env-21");

    let outcome = fixture
        .service
        .delete_java_environment("env-21", false)
        .unwrap();
    let JavaDeleteOutcome::RequiresConfirmation { instances } = outcome else {
        panic!("有引用时必须要求确认: {outcome:?}");
    };
    assert_eq!(instances.len(), 2);
    assert!(home.join("bin/java.exe").exists(), "确认前不得删除文件");

    let outcome = fixture
        .service
        .delete_java_environment("env-21", true)
        .unwrap();
    let JavaDeleteOutcome::Deleted { files_removed } = outcome else {
        panic!("确认后应删除: {outcome:?}");
    };
    assert!(files_removed);
    assert!(!home.exists(), "环境文件必须移除");
    let deleted = fixture.service.list_deleted_java_environments().unwrap();
    assert_eq!(deleted.len(), 1);
    assert_eq!(
        deleted[0].referencing_instances.len(),
        2,
        "墓碑必须保留引用记录"
    );
    assert!(
        fixture.service.list_java_environments().unwrap().is_empty(),
        "已删除环境不出现在清单"
    );
    let error = fixture
        .service
        .delete_java_environment("env-21", true)
        .expect_err("重复删除必须报错");
    assert!(error.to_string().contains("已经被删除"));
}

#[test]
fn m13_java_002_deleted_env_blocks_launch_with_recovery_hint() {
    let fixture = JavaFixture::new();
    fixture.register_java("env-21", "21.0.12+8", JavaEnvironmentStatus::Ready);
    fixture.register_instance("instance-a", "实例甲", "env-21", 21);
    fixture
        .service
        .delete_java_environment("env-21", true)
        .unwrap();

    let account = LaunchAccount::offline("LocalPlayer").unwrap();
    let error = fixture
        .service
        .create_launch_execution("instance-a", &account, &LaunchOptions::default())
        .expect_err("环境被删除后启动必须拒绝");

    let message = error.to_string();
    assert!(message.contains("已被删除"), "{message}");
    assert!(message.contains("恢复"), "应指引恢复: {message}");
    assert!(message.contains("不会自动装回"), "不得静默重装: {message}");
    assert!(
        fixture
            .service
            .list_deleted_java_environments()
            .unwrap()
            .len()
            == 1,
        "拒绝后不得偷偷恢复"
    );
}

#[test]
fn m13_java_003_interrupted_deletion_converges_on_reopen() {
    let fixture = JavaFixture::new();
    fixture.register_java("env-21", "21.0.12+8", JavaEnvironmentStatus::Ready);
    let home = fixture.java_home("env-21");
    Connection::open(&fixture.database_path)
        .unwrap()
        .execute(
            "UPDATE managed_java_environments SET status = 'deleted' WHERE id = 'env-21'",
            [],
        )
        .unwrap();
    assert!(home.exists(), "中断时文件仍在");

    let reopened = fixture.reopen();
    assert!(!home.exists(), "重启后必须完成清理");
    assert_eq!(reopened.list_deleted_java_environments().unwrap().len(), 1);
}

#[test]
fn m13_java_004_set_instance_java_environment_major_check_and_sync() {
    let fixture = JavaFixture::new();
    fixture.register_java("env-21", "21.0.12+8", JavaEnvironmentStatus::Ready);
    fixture.register_java("env-17", "17.0.16+8", JavaEnvironmentStatus::Ready);
    fixture.register_instance("instance-a", "实例甲", "env-21", 21);

    let error = fixture
        .service
        .set_instance_java_environment("instance-a", "env-17")
        .expect_err("主版本不一致必须拒绝");
    assert!(error.to_string().contains("主版本不一致"), "{error}");

    fixture.register_java("env-21b", "21.0.13+9", JavaEnvironmentStatus::Ready);
    fixture
        .service
        .set_instance_java_environment("instance-a", "env-21b")
        .expect("兼容环境应指派成功");
    let home = fixture.java_home("env-21b");
    let runtime_json = fixture.read_instance_runtime_json("instance-a");
    assert_eq!(runtime_json["javaHome"], json!(home.to_string_lossy()));
    let disk: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(
            fixture
                .instance_root("instance-a")
                .join(".moyumax/runtime.json"),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        disk["javaHome"], runtime_json["javaHome"],
        "数据库与磁盘必须一致"
    );

    let account = LaunchAccount::offline("LocalPlayer").unwrap();
    fixture
        .service
        .create_launch_execution("instance-a", &account, &LaunchOptions::default())
        .expect("指派后可启动");
}

#[tokio::test]
async fn m13_java_005_restore_replaces_tombstone_and_rewires_references() {
    let fixture = JavaFixture::new();
    fixture.register_java("env-21", "21.0.12+8", JavaEnvironmentStatus::Ready);
    fixture.register_instance("instance-a", "实例甲", "env-21", 21);
    fixture
        .service
        .delete_java_environment("env-21", true)
        .unwrap();
    let jdk_zip = jdk_zip_bytes();
    let server = ExecutorServer::new(HashMap::from([(
        "/zulu-jdk.zip".to_owned(),
        jdk_zip.clone(),
    )]));
    let package = ResolvedJavaPackage {
        distribution: JavaDistribution::AzulZulu,
        full_version: "21.0.13+9".to_owned(),
        architecture: JavaArchitecture::X64,
        package_uuid: "fixture-package".to_owned(),
        artifact: ResolvedArtifact {
            kind: ArtifactKind::JavaArchive,
            relative_path: "java/packages/zulu-jdk.zip".to_owned(),
            url: server.url("/zulu-jdk.zip"),
            size: u64::try_from(jdk_zip.len()).unwrap(),
            sha1: None,
            sha256: Some(digest_sha256(&jdk_zip)),
            sha512: None,
        },
    };
    let downloader = ArtifactDownloader::new(2).unwrap();

    let restored = fixture
        .service
        .restore_java_environment(&package, &downloader, "env-21")
        .await
        .expect("恢复应完成");

    assert_eq!(restored.full_version, "21.0.13+9");
    assert_eq!(restored.status, JavaEnvironmentStatus::Ready);
    assert!(restored.healthy);
    assert!(
        fixture
            .service
            .list_deleted_java_environments()
            .unwrap()
            .is_empty(),
        "恢复后旧墓碑必须移除"
    );
    assert_eq!(
        restored.referencing_instances.len(),
        1,
        "实例引用必须指向恢复环境"
    );
    let runtime_json = fixture.read_instance_runtime_json("instance-a");
    let new_home = PathBuf::from(&restored.home_directory);
    assert_eq!(runtime_json["javaHome"], json!(new_home.to_string_lossy()));
    assert!(
        new_home.join("bin/java.exe").is_file(),
        "解包后必须有 java.exe"
    );

    let account = LaunchAccount::offline("LocalPlayer").unwrap();
    fixture
        .service
        .create_launch_execution("instance-a", &account, &LaunchOptions::default())
        .expect("恢复后可启动");
}

struct JavaFixture {
    _directory: TempDir,
    database_path: PathBuf,
    data_directory: PathBuf,
    service: AppService,
}

impl JavaFixture {
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

    fn reopen(&self) -> AppService {
        AppService::open(&self.database_path, &self.data_directory).unwrap()
    }

    fn java_home(&self, id: &str) -> PathBuf {
        self.data_directory.join("store/java/zulu").join(id)
    }

    fn register_java(&self, id: &str, full_version: &str, status: JavaEnvironmentStatus) {
        let home = self.java_home(id);
        fs::create_dir_all(home.join("bin")).unwrap();
        fs::write(home.join("bin/java.exe"), b"fixture").unwrap();
        fs::write(home.join("release"), full_version).unwrap();
        self.service
            .register_managed_java(&ManagedJavaEnvironment {
                id: id.to_owned(),
                distribution: JavaDistribution::AzulZulu,
                full_version: full_version.to_owned(),
                architecture: JavaArchitecture::X64,
                home_directory: home.to_string_lossy().into_owned(),
                status,
            })
            .unwrap();
    }

    fn instance_root(&self, id: &str) -> PathBuf {
        self.data_directory.join("instances").join(id)
    }

    fn register_instance(&self, id: &str, name: &str, environment_id: &str, java_major: u16) {
        let root = self.instance_root(id);
        fs::create_dir_all(root.join(".moyumax")).unwrap();
        fs::create_dir_all(root.join(".minecraft/saves")).unwrap();
        fs::create_dir_all(root.join("natives")).unwrap();
        let home = self.java_home(environment_id);
        let runtime = json!({
            "schemaVersion": 1,
            "gameVersion": "26.2",
            "mainClass": "net.minecraft.client.main.Main",
            "javaHome": home.to_string_lossy(),
            "sharedStore": self.data_directory.join("store"),
            "workingDirectory": ".minecraft",
            "nativesDirectory": "natives",
            "classpath": [
                "minecraft/versions/26.2/26.2.jar",
                "minecraft/libraries/example-natives-windows.jar"
            ],
            "gameMetadata": {"id": "26.2", "assetIndex": {"id": "32"}, "arguments": {"jvm": ["-cp", "${classpath}"], "game": ["--version", "${version_name}"]}},
            "isolation": "full"
        });
        fs::write(
            root.join(".moyumax/runtime.json"),
            serde_json::to_vec_pretty(&runtime).unwrap(),
        )
        .unwrap();
        let natives_jar = self
            .data_directory
            .join("store/minecraft/libraries/example-natives-windows.jar");
        fs::create_dir_all(natives_jar.parent().unwrap()).unwrap();
        fs::write(&natives_jar, b"fixture").unwrap();
        let assets_index = self
            .data_directory
            .join("store/minecraft/assets/indexes/32.json");
        fs::create_dir_all(assets_index.parent().unwrap()).unwrap();
        fs::write(&assets_index, b"{}").unwrap();
        let version_jar = self
            .data_directory
            .join("store/minecraft/versions/26.2/26.2.jar");
        fs::create_dir_all(version_jar.parent().unwrap()).unwrap();
        fs::write(&version_jar, b"fixture").unwrap();
        let connection = Connection::open(&self.database_path).unwrap();
        connection
            .execute(
                "INSERT INTO instances (id, name, game_version, loader_kind, loader_version, root_directory, state, created_at_unix_seconds) VALUES (?1, ?2, '26.2', 'fabric', '0.19.3', ?3, 'ready', 1)",
                params![id, name, root.to_string_lossy()],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO instance_runtime (instance_id, java_environment_id, plan_json, runtime_json) VALUES (?1, ?2, ?3, ?4)",
                params![
                    id,
                    environment_id,
                    json!({"game": {"javaMajorVersion": java_major}}).to_string(),
                    runtime.to_string()
                ],
            )
            .unwrap();
    }

    fn read_instance_runtime_json(&self, instance_id: &str) -> serde_json::Value {
        let text: String = Connection::open(&self.database_path)
            .unwrap()
            .query_row(
                "SELECT runtime_json FROM instance_runtime WHERE instance_id = ?1",
                params![instance_id],
                |row| row.get(0),
            )
            .unwrap();
        serde_json::from_str(&text).unwrap()
    }
}

fn jdk_zip_bytes() -> Vec<u8> {
    let cursor = std::io::Cursor::new(Vec::new());
    let mut writer = ZipWriter::new(cursor);
    let options = SimpleFileOptions::default();
    writer.start_file("bin/java.exe", options).unwrap();
    writer.write_all(b"fake-java").unwrap();
    writer.start_file("release", options).unwrap();
    writer.write_all(b"JAVA_VERSION=21.0.13").unwrap();
    writer.finish().unwrap().into_inner()
}

fn digest_sha256(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    sha2::Digest::update(&mut hasher, bytes);
    encode_hex(sha2::Digest::finalize(hasher))
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
    base: String,
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
            base: format!("http://{address}"),
            _thread: server_thread,
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base)
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
