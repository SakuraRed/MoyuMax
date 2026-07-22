use std::{
    collections::HashMap,
    fs,
    io::{BufRead, BufReader, Cursor, Write},
    net::{TcpListener, TcpStream},
    path::Path,
    sync::Arc,
    thread,
};

use moyumax_core::{
    AppService, ArtifactKind, GameReleaseType, GameVersionSummary, InstallExecutor, InstallStage,
    InstanceIsolation, JavaArchitecture, JavaDistribution, JavaEnvironmentStatus,
    ManagedJavaEnvironment, ResolvedArtifact, ResolvedGameVersion, ResolvedInstallRequest,
    ResolvedJavaPackage, ResolvedLoader, TaskState,
};
use serde_json::json;
use sha1::{Digest, Sha1};
use tempfile::TempDir;
use zip::{ZipWriter, write::SimpleFileOptions};

#[tokio::test]
async fn m3_execute_001_verified_files_publish_one_ready_instance_atomically() {
    let asset = b"asset-object".to_vec();
    let asset_hash = sha1(&asset);
    let asset_index = serde_json::to_vec(&json!({
        "objects": {
            "minecraft/sounds/example.ogg": {
                "hash": asset_hash,
                "size": asset.len()
            }
        }
    }))
    .unwrap();
    let version_json =
        br#"{"id":"test-release","mainClass":"net.minecraft.client.main.Main"}"#.to_vec();
    let client = b"fake-client-jar".to_vec();
    let native_archive = native_archive();
    let server = FixtureServer::new(HashMap::from([
        ("/version.json".to_owned(), version_json.clone()),
        ("/client.jar".to_owned(), client.clone()),
        ("/asset-index.json".to_owned(), asset_index.clone()),
        ("/native.jar".to_owned(), native_archive.clone()),
        (
            format!("/objects/{}/{asset_hash}", &asset_hash[..2]),
            asset.clone(),
        ),
    ]));
    let fixture = ExecutorFixture::new();
    fixture.register_ready_java();
    let request = fixture.request(
        &server,
        &version_json,
        &client,
        &asset_index,
        &native_archive,
        asset.len() as u64,
    );
    let task = fixture.service.enqueue_install_task(&request).unwrap();

    let instance = InstallExecutor::new(4)
        .unwrap()
        .with_asset_base_url(server.url("/objects"))
        .unwrap()
        .execute_task(&fixture.service, &task.id)
        .await
        .expect("verified task should publish an instance");

    assert_eq!(instance.state, "ready");
    assert!(Path::new(&instance.root_directory).is_dir());
    assert!(
        Path::new(&instance.root_directory)
            .join(".moyumax/install-plan.json")
            .is_file()
    );
    assert_eq!(
        fs::read(Path::new(&instance.root_directory).join("natives/fixture-native.dll")).unwrap(),
        b"native-dll"
    );
    let shared_asset = fixture
        .data_directory
        .join("store/minecraft/assets/objects")
        .join(&asset_hash[..2])
        .join(&asset_hash);
    assert_eq!(fs::read(shared_asset).unwrap(), asset);
    let tasks = fixture.service.list_install_tasks().unwrap();
    assert_eq!(tasks[0].state, TaskState::Completed);
    assert_eq!(
        tasks[0].current_stage,
        Some(InstallStage::CreateRollbackPoint)
    );
    assert_eq!(
        tasks[0].progress.completed_bytes,
        tasks[0].progress.total_bytes.unwrap()
    );
    assert_eq!(fixture.service.list_instances().unwrap(), vec![instance]);
}

#[tokio::test]
async fn m3_execute_008_modern_native_artifacts_select_only_windows_x64() {
    let asset = b"asset-object".to_vec();
    let asset_hash = sha1(&asset);
    let asset_index = serde_json::to_vec(&json!({
        "objects": {"fixture": {"hash": asset_hash, "size": asset.len()}}
    }))
    .unwrap();
    let version_json =
        br#"{"id":"test-release","mainClass":"net.minecraft.client.main.Main"}"#.to_vec();
    let client = b"fake-client-jar".to_vec();
    let x64_native = native_archive_named("lwjgl-x64.dll", b"x64");
    let x86_native = native_archive_named("lwjgl-x86.dll", b"x86");
    let arm64_native = native_archive_named("lwjgl-arm64.dll", b"arm64");
    let server = FixtureServer::new(HashMap::from([
        ("/version.json".to_owned(), version_json.clone()),
        ("/client.jar".to_owned(), client.clone()),
        ("/asset-index.json".to_owned(), asset_index.clone()),
        ("/native-x64.jar".to_owned(), x64_native.clone()),
        ("/native-x86.jar".to_owned(), x86_native.clone()),
        ("/native-arm64.jar".to_owned(), arm64_native.clone()),
        (
            format!("/objects/{}/{asset_hash}", &asset_hash[..2]),
            asset.clone(),
        ),
    ]));
    let fixture = ExecutorFixture::new();
    fixture.register_ready_java();
    let mut request = fixture.request(
        &server,
        &version_json,
        &client,
        &asset_index,
        &x64_native,
        asset.len() as u64,
    );
    request.game.metadata["libraries"] = json!([
        modern_native_library(&server, "natives-windows", "/native-x64.jar", &x64_native),
        modern_native_library(
            &server,
            "natives-windows-x86",
            "/native-x86.jar",
            &x86_native
        ),
        modern_native_library(
            &server,
            "natives-windows-arm64",
            "/native-arm64.jar",
            &arm64_native
        )
    ]);
    request.game.artifacts.extend([
        modern_native_artifact(&server, "natives-windows", "/native-x64.jar", &x64_native),
        modern_native_artifact(
            &server,
            "natives-windows-x86",
            "/native-x86.jar",
            &x86_native,
        ),
        modern_native_artifact(
            &server,
            "natives-windows-arm64",
            "/native-arm64.jar",
            &arm64_native,
        ),
    ]);
    let task = fixture.service.enqueue_install_task(&request).unwrap();

    let instance = InstallExecutor::new(4)
        .unwrap()
        .with_asset_base_url(server.url("/objects"))
        .unwrap()
        .execute_task(&fixture.service, &task.id)
        .await
        .expect("modern Windows x64 natives should produce a ready instance");
    let runtime: serde_json::Value = serde_json::from_slice(
        &fs::read(Path::new(&instance.root_directory).join(".moyumax/runtime.json")).unwrap(),
    )
    .unwrap();
    let classpath = runtime["classpath"].as_array().unwrap();
    assert!(classpath.iter().any(|entry| {
        entry
            .as_str()
            .is_some_and(|path| path.ends_with("lwjgl-3.4.1-natives-windows.jar"))
    }));
    assert!(!classpath.iter().any(|entry| {
        entry.as_str().is_some_and(|path| {
            path.contains("natives-windows-x86") || path.contains("natives-windows-arm64")
        })
    }));
    assert!(
        Path::new(&instance.root_directory)
            .join("natives")
            .read_dir()
            .unwrap()
            .next()
            .is_none(),
        "新版原生 JAR 应在游戏运行时按官方 JVM 参数自解包"
    );
}

struct ExecutorFixture {
    _directory: TempDir,
    data_directory: std::path::PathBuf,
    service: AppService,
}

impl ExecutorFixture {
    fn new() -> Self {
        let directory = TempDir::new().unwrap();
        let database = directory.path().join("state.sqlite3");
        let data_directory = directory.path().join("data");
        let service = AppService::open(&database, &data_directory).unwrap();
        service.skip_onboarding().unwrap();
        Self {
            _directory: directory,
            data_directory,
            service,
        }
    }

    fn register_ready_java(&self) {
        let home = self.data_directory.join("java-ready");
        fs::create_dir_all(home.join("bin")).unwrap();
        fs::write(home.join("bin/java.exe"), b"fixture").unwrap();
        self.service
            .register_managed_java(&ManagedJavaEnvironment {
                id: "ready-java".to_owned(),
                distribution: JavaDistribution::AzulZulu,
                full_version: "21.0.12+8".to_owned(),
                architecture: JavaArchitecture::X64,
                home_directory: home.to_string_lossy().into_owned(),
                status: JavaEnvironmentStatus::Ready,
            })
            .unwrap();
    }

    fn request(
        &self,
        server: &FixtureServer,
        version_json: &[u8],
        client: &[u8],
        asset_index: &[u8],
        native_archive: &[u8],
        asset_size: u64,
    ) -> ResolvedInstallRequest {
        let version = "test-release";
        ResolvedInstallRequest {
            instance_name: "真实执行器测试".to_owned(),
            game: ResolvedGameVersion {
                version: GameVersionSummary {
                    id: version.to_owned(),
                    release_type: GameReleaseType::Release,
                    release_time: "2026-07-22T00:00:00Z".to_owned(),
                    metadata_url: server.url("/version.json"),
                    metadata_sha1: sha1(version_json),
                    recommended: true,
                },
                java_major_version: 21,
                main_class: "net.minecraft.client.main.Main".to_owned(),
                metadata: json!({
                    "id": version,
                    "mainClass": "net.minecraft.client.main.Main",
                    "assetIndex": {"id": "test-assets"},
                    "arguments": {"game": [], "jvm": []},
                    "libraries": [{
                        "name": "org.lwjgl:lwjgl:3.3.3",
                        "rules": [{"action": "allow", "os": {"name": "windows"}}],
                        "natives": {"windows": "natives-windows"},
                        "downloads": {
                            "classifiers": {
                                "natives-windows": {
                                    "path": "org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3-natives-windows.jar",
                                    "url": server.url("/native.jar"),
                                    "sha1": sha1(native_archive),
                                    "size": native_archive.len()
                                }
                            }
                        }
                    }]
                }),
                artifacts: vec![
                    artifact(
                        ArtifactKind::VersionMetadata,
                        &format!("minecraft/versions/{version}/{version}.json"),
                        server.url("/version.json"),
                        version_json,
                    ),
                    artifact(
                        ArtifactKind::GameClient,
                        &format!("minecraft/versions/{version}/{version}.jar"),
                        server.url("/client.jar"),
                        client,
                    ),
                    artifact(
                        ArtifactKind::AssetIndex,
                        "minecraft/assets/indexes/test-assets.json",
                        server.url("/asset-index.json"),
                        asset_index,
                    ),
                ],
                asset_objects_total_bytes: asset_size,
            },
            loader: ResolvedLoader::Vanilla,
            java: ResolvedJavaPackage {
                distribution: JavaDistribution::AzulZulu,
                full_version: "21.0.12+8".to_owned(),
                architecture: JavaArchitecture::X64,
                package_uuid: "fixture-java".to_owned(),
                artifact: ResolvedArtifact {
                    kind: ArtifactKind::JavaArchive,
                    relative_path: "java/packages/unused.zip".to_owned(),
                    url: server.url("/unused-java.zip"),
                    size: 1,
                    sha1: None,
                    sha256: Some("00".repeat(32)),
                },
            },
            isolation: InstanceIsolation::Full,
        }
    }
}

fn artifact(
    kind: ArtifactKind,
    relative_path: &str,
    url: String,
    bytes: &[u8],
) -> ResolvedArtifact {
    ResolvedArtifact {
        kind,
        relative_path: relative_path.to_owned(),
        url,
        size: u64::try_from(bytes.len()).unwrap(),
        sha1: Some(sha1(bytes)),
        sha256: None,
    }
}

fn native_archive() -> Vec<u8> {
    native_archive_named("fixture-native.dll", b"native-dll")
}

fn native_archive_named(name: &str, contents: &[u8]) -> Vec<u8> {
    let cursor = Cursor::new(Vec::new());
    let mut writer = ZipWriter::new(cursor);
    writer
        .start_file(name, SimpleFileOptions::default())
        .unwrap();
    writer.write_all(contents).unwrap();
    writer.finish().unwrap().into_inner()
}

fn modern_native_library(
    server: &FixtureServer,
    classifier: &str,
    url: &str,
    archive: &[u8],
) -> serde_json::Value {
    json!({
        "name": format!("org.lwjgl:lwjgl:3.4.1:{classifier}"),
        "rules": [{"action": "allow", "os": {"name": "windows"}}],
        "downloads": {"artifact": {
            "path": format!("org/lwjgl/lwjgl/3.4.1/lwjgl-3.4.1-{classifier}.jar"),
            "url": server.url(url),
            "sha1": sha1(archive),
            "size": archive.len()
        }}
    })
}

fn modern_native_artifact(
    server: &FixtureServer,
    classifier: &str,
    url: &str,
    archive: &[u8],
) -> ResolvedArtifact {
    artifact(
        ArtifactKind::Library,
        &format!("minecraft/libraries/org/lwjgl/lwjgl/3.4.1/lwjgl-3.4.1-{classifier}.jar"),
        server.url(url),
        archive,
    )
}

fn sha1(bytes: &[u8]) -> String {
    let mut hasher = Sha1::new();
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
            for _ in 0..responses.len() {
                let (stream, _) = listener.accept().unwrap();
                serve_fixture(stream, &responses);
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

fn serve_fixture(mut stream: TcpStream, responses: &HashMap<String, Vec<u8>>) {
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
    let body = responses.get(path).unwrap();
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .unwrap();
    stream.write_all(body).unwrap();
    stream.flush().unwrap();
}
