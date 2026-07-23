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
    AppService, ArtifactKind, GameReleaseType, GameVersionSummary, InstallExecutor,
    InstanceIsolation, JavaArchitecture, JavaDistribution, JavaEnvironmentStatus, LaunchAccount,
    LaunchOptions, ManagedInstanceSummary, ManagedJavaEnvironment, MetadataClient,
    ResolvedArtifact, ResolvedGameVersion, ResolvedInstallRequest, ResolvedJavaPackage,
    ResolvedLoader, prepare_launch_from_runtime,
};
use serde_json::json;
use sha1::{Digest, Sha1};
use tempfile::TempDir;
use zip::{ZipWriter, write::SimpleFileOptions};

#[tokio::test]
async fn m11_quilt_001_quilt_loaders_mark_first_stable_as_recommended() {
    let server = QuiltMetaServer::new(HashMap::from([(
        "/versions/loader/1.21.8".to_owned(),
        serde_json::to_vec(&json!([
            {"loader": {"version": "0.30.1-beta.1"}},
            {"loader": {"version": "0.30.0"}},
            {"loader": {"version": "0.29.2-beta.3"}},
        ]))
        .unwrap(),
    )]));
    let client = MetadataClient::new()
        .unwrap()
        .with_quilt_meta_base(server.base.clone());

    let loaders = client.compatible_quilt_loaders("1.21.8").await.unwrap();

    assert_eq!(loaders.len(), 3);
    assert!(!loaders[0].stable, "beta 后缀不得标记稳定");
    assert!(loaders[1].stable, "无 `-` 后缀应标记稳定");
    assert!(loaders[1].recommended, "推荐应落在第一个稳定项");
    assert!(!loaders[0].recommended && !loaders[2].recommended);
}

#[tokio::test]
async fn m11_quilt_001_resolve_quilt_loader_fetches_profile_with_sha256() {
    let profile_payload = serde_json::to_vec(&json!({
        "id": "quilt-loader-0.30.0-1.21.8",
        "mainClass": "org.quiltmc.loader.impl.launch.knot.KnotClient",
        "libraries": [],
        "arguments": {"game": []},
    }))
    .unwrap();
    let server = QuiltMetaServer::new(HashMap::from([
        (
            "/versions/loader/1.21.8".to_owned(),
            serde_json::to_vec(&json!([
                {"loader": {"version": "0.30.1-beta.1"}},
                {"loader": {"version": "0.30.0"}},
            ]))
            .unwrap(),
        ),
        (
            "/versions/loader/1.21.8/0.30.0/profile/json".to_owned(),
            profile_payload.clone(),
        ),
    ]));
    let client = MetadataClient::new()
        .unwrap()
        .with_quilt_meta_base(server.base.clone());

    let ResolvedLoader::Quilt {
        version,
        stable,
        profile_url,
        profile_sha256,
        profile,
    } = client
        .resolve_quilt_loader("1.21.8", "0.30.0")
        .await
        .expect("兼容版本应解析成功")
    else {
        panic!("应解析为 Quilt");
    };

    assert_eq!(version, "0.30.0");
    assert!(stable);
    assert!(profile_url.ends_with("/versions/loader/1.21.8/0.30.0/profile/json"));
    assert_eq!(profile_sha256, digest_sha256(&profile_payload));
    assert_eq!(
        profile["mainClass"],
        "org.quiltmc.loader.impl.launch.knot.KnotClient"
    );

    let error = client
        .resolve_quilt_loader("1.21.8", "0.0.0-does-not-exist")
        .await
        .expect_err("不在兼容列表中的版本必须被拒绝");
    assert!(error.to_string().contains("兼容列表"));
}

#[tokio::test]
async fn m11_quilt_002_install_publishes_quilt_instance() {
    let asset = b"asset-object".to_vec();
    let asset_hash = digest_sha1(&asset);
    let asset_index = serde_json::to_vec(&json!({
        "objects": {
            "minecraft/sounds/example.ogg": { "hash": asset_hash, "size": asset.len() }
        }
    }))
    .unwrap();
    let version_json =
        br#"{"id":"test-release","mainClass":"net.minecraft.client.main.Main"}"#.to_vec();
    let client = b"fake-client-jar".to_vec();
    let native = native_archive();
    let quilt_library = b"quilt-loader-library".to_vec();
    let mixin_library = b"sponge-mixin-library".to_vec();
    let server = ExecutorServer::new(HashMap::from([
        ("/version.json".to_owned(), version_json.clone()),
        ("/client.jar".to_owned(), client.clone()),
        ("/asset-index.json".to_owned(), asset_index.clone()),
        ("/native.jar".to_owned(), native.clone()),
        (
            format!("/objects/{}/{asset_hash}", &asset_hash[..2]),
            asset.clone(),
        ),
        (
            "/org/quiltmc/quilt-loader/0.30.0/quilt-loader-0.30.0.jar".to_owned(),
            quilt_library.clone(),
        ),
        (
            "/net/fabricmc/sponge-mixin/0.17.2/sponge-mixin-0.17.2.jar".to_owned(),
            mixin_library.clone(),
        ),
    ]));
    let fixture = ExecutorFixture::new();
    fixture.register_ready_java();
    let request = fixture.quilt_request(
        &server,
        &version_json,
        &client,
        &asset_index,
        &native,
        asset.len() as u64,
        &quilt_library,
        &mixin_library,
    );
    let task = fixture.service.enqueue_install_task(&request).unwrap();

    let instance = InstallExecutor::new(4)
        .unwrap()
        .with_asset_base_url(server.url("/objects"))
        .unwrap()
        .execute_task(&fixture.service, &task.id)
        .await
        .expect("Quilt 安装应原子提交");

    assert_eq!(instance.state, "ready");
    assert_eq!(instance.loader_kind, "quilt");
    assert_eq!(instance.loader_version.as_deref(), Some("0.30.0"));
    let runtime: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(Path::new(&instance.root_directory).join(".moyumax/runtime.json"))
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        runtime["mainClass"],
        "org.quiltmc.loader.impl.launch.knot.KnotClient"
    );
    let classpath = runtime["classpath"].as_array().unwrap();
    assert!(
        classpath
            .iter()
            .any(|entry| entry.as_str().unwrap().contains("quilt-loader")),
        "classpath 应包含 Quilt 库: {classpath:?}"
    );
    assert_eq!(
        fs::read(fixture.data_directory.join(
            "store/minecraft/libraries/org/quiltmc/quilt-loader/0.30.0/quilt-loader-0.30.0.jar"
        ))
        .unwrap(),
        quilt_library
    );
}

#[test]
fn m11_quilt_003_launch_uses_quilt_mainclass_and_loader_args() {
    let fixture = LaunchFixture::new();
    let account = LaunchAccount::offline("LocalPlayer").unwrap();
    let prepared = prepare_launch_from_runtime(
        &fixture.instance,
        &fixture.runtime,
        &account,
        &LaunchOptions::default(),
    )
    .expect("Quilt 运行时清单应展开为启动命令");

    assert!(
        prepared
            .arguments()
            .iter()
            .any(|argument| argument == "org.quiltmc.loader.impl.launch.knot.KnotClient"),
        "应使用 Quilt mainClass: {:?}",
        prepared.arguments()
    );
    assert!(
        prepared
            .arguments()
            .iter()
            .any(|argument| argument == "-DQuiltFixture=true"),
        "应包含 Quilt loader JVM 参数"
    );
    assert!(
        prepared
            .arguments()
            .windows(2)
            .any(|pair| pair == ["--version", "quilt-loader-0.30.0-26.2"]),
        "version_name 应取 loaderProfile.id"
    );
}

#[tokio::test]
async fn m11_quilt_001_incompatible_choice_is_rejected_before_download() {
    let server = QuiltMetaServer::new(HashMap::from([(
        "/versions/loader/1.21.8".to_owned(),
        serde_json::to_vec(&json!([{"loader": {"version": "0.30.0"}}])).unwrap(),
    )]));
    let client = MetadataClient::new()
        .unwrap()
        .with_quilt_meta_base(server.base.clone());
    let error = client
        .resolve_quilt_loader("1.21.8", "0.29.0")
        .await
        .expect_err("不在兼容列表的版本必须拒绝");
    assert!(error.to_string().contains("兼容列表"));
    assert_eq!(server.request_count(), 1, "拒绝前只允许查询兼容列表");
}

struct QuiltMetaServer {
    base: String,
    requests: Arc<std::sync::Mutex<usize>>,
    _thread: thread::JoinHandle<()>,
}

impl QuiltMetaServer {
    fn new(routes: HashMap<String, Vec<u8>>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let routes = Arc::new(routes);
        let requests = Arc::new(std::sync::Mutex::new(0_usize));
        let counter = Arc::clone(&requests);
        let server_thread = thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                let routes = Arc::clone(&routes);
                let counter = Arc::clone(&counter);
                thread::spawn(move || {
                    *counter.lock().unwrap() += 1;
                    serve_json(stream, &routes);
                });
            }
        });
        Self {
            base: format!("http://{address}"),
            requests,
            _thread: server_thread,
        }
    }

    fn request_count(&self) -> usize {
        *self.requests.lock().unwrap()
    }
}

fn serve_json(mut stream: TcpStream, routes: &HashMap<String, Vec<u8>>) {
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
    let Some(body) = routes.get(path) else {
        write!(
            stream,
            "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        )
        .unwrap();
        return;
    };
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .unwrap();
    let _ = stream.write_all(body);
    let _ = stream.flush();
}

struct ExecutorFixture {
    _directory: TempDir,
    data_directory: PathBuf,
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

    #[allow(clippy::too_many_arguments)]
    fn quilt_request(
        &self,
        server: &ExecutorServer,
        version_json: &[u8],
        client: &[u8],
        asset_index: &[u8],
        native_archive: &[u8],
        asset_size: u64,
        quilt_library: &[u8],
        mixin_library: &[u8],
    ) -> ResolvedInstallRequest {
        let version = "test-release";
        ResolvedInstallRequest {
            instance_name: "Quilt 安装测试".to_owned(),
            game: ResolvedGameVersion {
                version: GameVersionSummary {
                    id: version.to_owned(),
                    release_type: GameReleaseType::Release,
                    release_time: "2026-07-22T00:00:00Z".to_owned(),
                    metadata_url: server.url("/version.json"),
                    metadata_sha1: digest_sha1(version_json),
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
                                    "sha1": digest_sha1(native_archive),
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
            loader: ResolvedLoader::Quilt {
                version: "0.30.0".to_owned(),
                stable: true,
                profile_url: server.url("/quilt-profile.json"),
                profile_sha256: "00".repeat(32),
                profile: json!({
                    "id": "quilt-loader-0.30.0-test-release",
                    "mainClass": "org.quiltmc.loader.impl.launch.knot.KnotClient",
                    "arguments": {"game": []},
                    "libraries": [
                        {
                            "name": "org.quiltmc:quilt-loader:0.30.0",
                            "url": format!("{}/", server.url("")),
                            "sha1": digest_sha1(quilt_library),
                            "size": quilt_library.len()
                        },
                        {
                            "name": "net.fabricmc:sponge-mixin:0.17.2",
                            "url": format!("{}/", server.url("")),
                            "sha1": digest_sha1(mixin_library),
                            "size": mixin_library.len()
                        }
                    ]
                }),
            },
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
                    sha512: None,
                },
            },
            isolation: InstanceIsolation::Full,
        }
    }
}

struct LaunchFixture {
    _directory: TempDir,
    instance: ManagedInstanceSummary,
    runtime: serde_json::Value,
}

impl LaunchFixture {
    fn new() -> Self {
        let directory = TempDir::new().unwrap();
        let root = directory.path().join("instances/instance-id");
        let shared = directory.path().join("store");
        let java_home = directory.path().join("java");
        for directory in [
            root.join(".minecraft/saves/Test World"),
            root.join("natives"),
            java_home.join("bin"),
            shared.join("minecraft/libraries"),
            shared.join("minecraft/versions/26.2"),
            shared.join("minecraft/assets/indexes"),
            shared.join("minecraft/assets/log_configs"),
        ] {
            fs::create_dir_all(directory).unwrap();
        }
        for file in [
            java_home.join("bin/java.exe"),
            shared.join("minecraft/libraries/example.jar"),
            shared.join("minecraft/libraries/example-natives-windows.jar"),
            shared.join("minecraft/versions/26.2/26.2.jar"),
            shared.join("minecraft/assets/indexes/32.json"),
            shared.join("minecraft/assets/log_configs/client.xml"),
        ] {
            fs::write(file, b"fixture").unwrap();
        }
        let instance = ManagedInstanceSummary {
            id: "instance-id".to_owned(),
            name: "Quilt 启动测试".to_owned(),
            game_version: "26.2".to_owned(),
            loader_kind: "quilt".to_owned(),
            loader_version: Some("0.30.0".to_owned()),
            root_directory: root.to_string_lossy().into_owned(),
            state: "ready".to_owned(),
        };
        let runtime = json!({
            "schemaVersion": 1,
            "gameVersion": "26.2",
            "mainClass": "org.quiltmc.loader.impl.launch.knot.KnotClient",
            "javaHome": java_home,
            "sharedStore": shared,
            "workingDirectory": ".minecraft",
            "nativesDirectory": "natives",
            "classpath": [
                "minecraft/versions/26.2/26.2.jar",
                "minecraft/libraries/example-natives-windows.jar",
                "minecraft/libraries/example.jar"
            ],
            "gameMetadata": {
                "id": "26.2",
                "type": "release",
                "assetIndex": {"id": "32"},
                "arguments": {
                    "default-user-jvm": [],
                    "jvm": ["-cp", "${classpath}"],
                    "game": [
                        "--username", "${auth_player_name}",
                        "--version", "${version_name}",
                        "--gameDir", "${game_directory}",
                        "--assetsDir", "${assets_root}",
                        "--assetIndex", "${assets_index_name}"
                    ]
                },
                "logging": {"client": {"argument": "-Dlog4j.configurationFile=${path}", "file": {"id": "client.xml"}}}
            },
            "loaderProfile": {
                "id": "quilt-loader-0.30.0-26.2",
                "mainClass": "org.quiltmc.loader.impl.launch.knot.KnotClient",
                "arguments": {"jvm": ["-DQuiltFixture=true"], "game": []}
            },
            "isolation": "full",
            "fixtureRoot": root
        });
        Self {
            _directory: directory,
            instance,
            runtime,
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
        sha1: Some(digest_sha1(bytes)),
        sha256: None,
        sha512: None,
    }
}

fn native_archive() -> Vec<u8> {
    let cursor = std::io::Cursor::new(Vec::new());
    let mut writer = ZipWriter::new(cursor);
    writer
        .start_file("fixture-native.dll", SimpleFileOptions::default())
        .unwrap();
    writer.write_all(b"native-dll").unwrap();
    writer.finish().unwrap().into_inner()
}

fn digest_sha1(bytes: &[u8]) -> String {
    let mut hasher = Sha1::new();
    hasher.update(bytes);
    encode_hex(hasher.finalize())
}

fn digest_sha256(bytes: &[u8]) -> String {
    let mut hasher = sha2::Sha256::new();
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
