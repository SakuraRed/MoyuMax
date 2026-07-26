use std::{
    collections::HashMap,
    fs,
    io::{BufRead, BufReader, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    thread,
};

use moyumax_core::{
    AppService, ArtifactKind, GameReleaseType, GameVersionSummary, InstallExecutor,
    InstanceIsolation, JavaArchitecture, JavaDistribution, JavaEnvironmentStatus, LaunchAccount,
    LaunchOptions, ManagedJavaEnvironment, MavenCoordinate, MetadataClient, ProcessorInvocation,
    ResolvedArtifact, ResolvedGameVersion, ResolvedInstallRequest, ResolvedJavaPackage,
    ResolvedLoader, TaskState, prepare_launch_from_runtime, read_install_profile,
};
use serde_json::{Value, json};
use sha1::{Digest, Sha1};
use tempfile::TempDir;
use zip::{ZipWriter, write::SimpleFileOptions};

#[test]
fn m12_loader_001_maven_coordinate_parsing() {
    let plain = MavenCoordinate::parse("net.minecraftforge:installertools:1.4.3").unwrap();
    assert_eq!(
        plain.relative_path(),
        "net/minecraftforge/installertools/1.4.3/installertools-1.4.3.jar"
    );

    let classified = MavenCoordinate::parse("net.neoforged:AutoRenamingTool:2.0.11:all").unwrap();
    assert_eq!(
        classified.relative_path(),
        "net/neoforged/AutoRenamingTool/2.0.11/AutoRenamingTool-2.0.11-all.jar"
    );

    let zipped = MavenCoordinate::parse("net.neoforged:neoform:1.21.8:mappings@txt").unwrap();
    assert_eq!(zipped.extension, "txt");
    assert_eq!(
        zipped.relative_path(),
        "net/neoforged/neoform/1.21.8/neoform-1.21.8-mappings.txt"
    );

    assert!(MavenCoordinate::parse("only:two").is_err());
    assert!(MavenCoordinate::parse("a:bad artifact:1.0").is_err());
}

#[test]
fn m12_loader_001_unsupported_spec_is_rejected() {
    let installer = build_installer_zip(
        &json!({
            "spec": 0, "profile": "forge", "version": "legacy",
            "minecraft": "test-release", "json": "/version.json",
            "data": {}, "processors": [], "libraries": []
        }),
        &json!({"id": "legacy", "mainClass": "x", "libraries": []}),
        &[],
    );
    let error =
        read_install_profile(std::io::Cursor::new(installer)).expect_err("非 spec-1 必须拒绝");
    assert!(error.to_string().contains("spec"));
}

#[tokio::test]
async fn m12_loader_002_forge_processors_run_to_verified_instance() {
    let fixture = ForgeFixture::new(PatchOutcome::Correct);
    let task = fixture
        .service
        .enqueue_install_task(&fixture.request)
        .unwrap();
    let invocations = Arc::new(Mutex::new(Vec::new()));
    let runner = fake_runner(Arc::clone(&invocations), fixture.patched_bytes.clone());

    let instance = InstallExecutor::new(4)
        .unwrap()
        .with_asset_base_url(fixture.server.url("/objects"))
        .unwrap()
        .with_processor_runner(runner)
        .execute_task(&fixture.service, &task.id)
        .await
        .expect("Forge 处理器链应完成安装");

    assert_eq!(instance.state, "ready");
    assert_eq!(instance.loader_kind, "forge");
    assert_eq!(instance.loader_version.as_deref(), Some("58.1.20"));

    let calls = invocations.lock().unwrap();
    assert_eq!(calls.len(), 2, "只执行客户端处理器,跳过 server 侧");
    assert!(calls[0].args.iter().any(|arg| arg == "DOWNLOAD_MOJMAPS"));
    assert_eq!(calls[1].main_class, "test.BinaryPatcher");
    assert!(
        calls[1].args.iter().any(|arg| {
            arg.replace(char::from(92), "/")
                .ends_with("net/minecraft/client/test-release/client-test-release-official.jar")
        }),
        "MC_OFF 占位符应解析到受管库路径: {:?}",
        calls[1].args
    );
    assert!(
        calls[1].args.iter().any(|arg| arg.contains("client.lzma")),
        "BINPATCH 应从安装器解出到工作目录"
    );
    assert!(
        calls[1].args.iter().all(|arg| !arg.contains("${")),
        "不允许残留占位符: {:?}",
        calls[1].args
    );

    let patched = fixture
        .data_directory
        .join("store/minecraft/libraries/net/minecraftforge/forge/test-release-58.1.20/forge-test-release-58.1.20-client.jar");
    assert_eq!(fs::read(patched).unwrap(), fixture.patched_bytes);
    let runtime: Value = serde_json::from_str(
        &fs::read_to_string(Path::new(&instance.root_directory).join(".moyumax/runtime.json"))
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        runtime["mainClass"],
        "net.minecraftforge.bootstrap.ForgeBootstrap"
    );
    let classpath = runtime["classpath"].as_array().unwrap();
    assert!(
        classpath.iter().any(|entry| entry
            .as_str()
            .unwrap()
            .contains("forge-test-release-58.1.20-client")),
        "PATCHED 必须进入 classpath: {classpath:?}"
    );
    assert_eq!(
        fixture.service.list_install_tasks().unwrap()[0].state,
        TaskState::Completed
    );
}

#[tokio::test]
async fn m12_loader_002_patched_sha_mismatch_rolls_back() {
    let fixture = ForgeFixture::new(PatchOutcome::Corrupted);
    let task = fixture
        .service
        .enqueue_install_task(&fixture.request)
        .unwrap();
    let invocations = Arc::new(Mutex::new(Vec::new()));
    let runner = fake_runner(Arc::clone(&invocations), fixture.patched_bytes.clone());

    let error = InstallExecutor::new(4)
        .unwrap()
        .with_asset_base_url(fixture.server.url("/objects"))
        .unwrap()
        .with_processor_runner(runner)
        .execute_task(&fixture.service, &task.id)
        .await
        .expect_err("PATCHED 校验失败必须回滚");

    assert!(error.to_string().contains("SHA-1"), "应为校验错误: {error}");
    assert!(fixture.service.list_instances().unwrap().is_empty());
    assert!(
        !fixture
            .data_directory
            .join("store/minecraft/libraries/net/minecraftforge")
            .exists(),
        "失败的补丁不得进入共享存储"
    );
    assert_eq!(
        fixture.service.list_install_tasks().unwrap()[0].state,
        TaskState::Failed
    );
}

#[tokio::test]
async fn m12_loader_003_neoforge_processors_expand_module_placeholders() {
    let fixture = ForgeFixture::new(PatchOutcome::Correct);
    let task = fixture
        .service
        .enqueue_install_task(&fixture.neoforge_request())
        .unwrap();
    let invocations = Arc::new(Mutex::new(Vec::new()));
    let runner = fake_runner(Arc::clone(&invocations), fixture.patched_bytes.clone());

    let instance = InstallExecutor::new(4)
        .unwrap()
        .with_asset_base_url(fixture.server.url("/objects"))
        .unwrap()
        .with_processor_runner(runner)
        .execute_task(&fixture.service, &task.id)
        .await
        .expect("NeoForge 处理器链应完成安装");

    assert_eq!(instance.loader_kind, "neoforge");
    let calls = invocations.lock().unwrap();
    assert_eq!(calls.len(), 6, "NeoForge 客户端链共 6 个处理器");
    assert!(calls[3].args.iter().any(|arg| arg == "--slim"));
    let runtime: Value = serde_json::from_str(
        &fs::read_to_string(Path::new(&instance.root_directory).join(".moyumax/runtime.json"))
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        runtime["mainClass"],
        "cpw.mods.bootstraplauncher.BootstrapLauncher"
    );

    // 启动展开:library_directory 与 classpath_separator 必须被替换。
    let account = LaunchAccount::offline("LocalPlayer").unwrap();
    let prepared =
        prepare_launch_from_runtime(&instance, &runtime, &account, &LaunchOptions::default())
            .expect("NeoForge 运行时清单应展开为启动命令");
    assert!(
        prepared
            .arguments()
            .iter()
            .all(|argument| !argument.contains("${")),
        "不允许残留占位符: {:?}",
        prepared.arguments()
    );
    assert!(
        prepared
            .arguments()
            .iter()
            .any(|argument| argument.contains("minecraft") && argument.contains("libraries")),
        "library_directory 应展开到受管库目录"
    );
}

#[tokio::test]
async fn m12_loader_004_resolve_forge_loader_with_fixture_server() {
    let installer = build_installer_zip(
        &json!({
            "spec": 1, "profile": "forge", "version": "test-release-forge-58.1.20",
            "minecraft": "test-release", "json": "/version.json",
            "data": {}, "processors": [], "libraries": []
        }),
        &json!({"id": "test-release-forge-58.1.20", "mainClass": "net.minecraftforge.bootstrap.ForgeBootstrap", "libraries": []}),
        &[],
    );
    let installer_sha1 = digest_sha1(&installer);
    let server = ExecutorServer::new(HashMap::from([
        (
            "/forge/minecraft/test-release".to_owned(),
            serde_json::to_vec(&json!([
                {"version": "58.1.19", "build": 58010019},
                {"version": "58.1.20", "build": 58010020},
            ]))
            .unwrap(),
        ),
        (
            "/net/minecraftforge/forge/test-release-58.1.20/forge-test-release-58.1.20-installer.jar"
                .to_owned(),
            installer.clone(),
        ),
    ]));
    let client = MetadataClient::new()
        .unwrap()
        .with_bmclapi_base(server.url(""))
        .with_forge_maven_base(server.url(""));

    let versions = client
        .compatible_forge_versions("test-release")
        .await
        .unwrap();
    assert_eq!(versions.len(), 2);
    assert!(versions[1].recommended, "最新构建应推荐");
    assert!(!versions[0].recommended);

    let ResolvedLoader::Forge {
        version,
        installer_sha1: sha1,
        installer_size,
        version_json,
        ..
    } = client
        .resolve_forge_loader("test-release", "58.1.20")
        .await
        .expect("兼容 Forge 应解析成功")
    else {
        panic!("应解析为 Forge");
    };
    assert_eq!(version, "58.1.20");
    assert_eq!(sha1, installer_sha1);
    assert_eq!(installer_size, installer.len() as u64);
    assert_eq!(
        version_json["mainClass"],
        "net.minecraftforge.bootstrap.ForgeBootstrap"
    );

    let error = client
        .resolve_forge_loader("test-release", "58.0.0")
        .await
        .expect_err("不在兼容列表的版本必须拒绝");
    assert!(error.to_string().contains("兼容列表"));
}

#[tokio::test]
async fn m12_loader_005_resolve_neoforge_loader_with_fixture_server() {
    let installer = build_installer_zip(
        &json!({
            "spec": 1, "profile": "NeoForge", "version": "neoforge-21.8.54",
            "minecraft": "test-release", "json": "/version.json",
            "data": {}, "processors": [], "libraries": []
        }),
        &json!({"id": "neoforge-21.8.54", "mainClass": "cpw.mods.bootstraplauncher.BootstrapLauncher", "libraries": []}),
        &[],
    );
    let installer_sha1 = digest_sha1(&installer);
    // BMCLAPI 的 installerPath 带 /maven 前缀(相对镜像根);客户端必须剥掉
    // 再拼上游 maven 根,fixture 按剥离后的真实上游路径提供文件。
    let server = ExecutorServer::new(HashMap::from([
        (
            "/neoforge/list/test-release".to_owned(),
            serde_json::to_vec(&json!([
                {"version": "21.8.53", "installerPath": "/maven/net/neoforged/neoforge/21.8.53/neoforge-21.8.53-installer.jar"},
                {"version": "21.8.54", "installerPath": "/maven/net/neoforged/neoforge/21.8.54/neoforge-21.8.54-installer.jar"},
            ]))
            .unwrap(),
        ),
        (
            "/net/neoforged/neoforge/21.8.54/neoforge-21.8.54-installer.jar".to_owned(),
            installer.clone(),
        ),
    ]));
    let client = MetadataClient::new()
        .unwrap()
        .with_bmclapi_base(server.url(""))
        .with_neoforge_maven_base(server.url(""));

    let versions = client
        .compatible_neoforge_versions("test-release")
        .await
        .unwrap();
    assert_eq!(versions.len(), 2);
    assert!(versions[1].recommended);

    let ResolvedLoader::NeoForge {
        version,
        installer_url,
        installer_sha1: sha1,
        ..
    } = client
        .resolve_neoforge_loader("test-release", "21.8.54")
        .await
        .expect("兼容 NeoForge 应解析成功")
    else {
        panic!("应解析为 NeoForge");
    };
    assert_eq!(version, "21.8.54");
    assert_eq!(sha1, installer_sha1);
    assert!(installer_url.ends_with("neoforge-21.8.54-installer.jar"));
    assert!(
        !installer_url.contains("/maven/"),
        "上游 URL 不得包含镜像专用的 /maven 段:{installer_url}"
    );
}

enum PatchOutcome {
    Correct,
    Corrupted,
}

struct ForgeFixture {
    _directory: TempDir,
    data_directory: PathBuf,
    service: AppService,
    server: ExecutorServer,
    request: ResolvedInstallRequest,
    patched_bytes: Vec<u8>,
}

impl ForgeFixture {
    fn new(outcome: PatchOutcome) -> Self {
        let patched_bytes = match outcome {
            PatchOutcome::Correct => b"patched-client-jar-correct".to_vec(),
            PatchOutcome::Corrupted => b"patched-client-jar-corrupted".to_vec(),
        };
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
        let universal = b"forge-universal".to_vec();
        let tool_jar = processor_jar("test.InstallerTools");
        let patcher_jar = processor_jar("test.BinaryPatcher");
        let server_tool_jar = processor_jar("test.ServerTool");
        let (listener, base) = ExecutorServer::bind();
        let reference_patched = b"patched-client-jar-correct".to_vec();
        let profile = forge_profile(
            &base,
            &tool_jar,
            &patcher_jar,
            &server_tool_jar,
            &reference_patched,
        );
        let loader_version_json = forge_version_json(&base, &universal);
        let installer = build_installer_zip(
            &json!({
                "spec": profile["spec"].clone(),
                "profile": profile["profile"].clone(),
                "version": profile["version"].clone(),
                "minecraft": profile["minecraft"].clone(),
                "json": profile["json"].clone(),
                "data": profile["data"].clone(),
                "processors": profile["processors"].clone(),
                "libraries": profile["libraries"].clone(),
            }),
            &loader_version_json,
            &[("data/client.lzma", b"binpatch-lzma")],
        );
        let installer_sha1 = digest_sha1(&installer);
        let server = ExecutorServer::with_listener(
            listener,
            HashMap::from([
                ("/version.json".to_owned(), version_json.clone()),
                ("/client.jar".to_owned(), client.clone()),
                ("/asset-index.json".to_owned(), asset_index.clone()),
                ("/native.jar".to_owned(), native.clone()),
                (
                    format!("/objects/{}/{asset_hash}", &asset_hash[..2]),
                    asset.clone(),
                ),
                (
                    "/net/minecraftforge/forge/test-release-58.1.20/forge-test-release-58.1.20-universal.jar"
                        .to_owned(),
                    universal.clone(),
                ),
                (
                    "/net/minecraftforge/installertools/1.4.3/installertools-1.4.3.jar".to_owned(),
                    tool_jar.clone(),
                ),
                (
                    "/net/minecraftforge/binarypatcher/1.2.0/binarypatcher-1.2.0.jar".to_owned(),
                    patcher_jar.clone(),
                ),
                (
                    "/net/minecraftforge/servertool/1.0.0/servertool-1.0.0.jar".to_owned(),
                    server_tool_jar.clone(),
                ),
                (
                    "/net/minecraftforge/forge/test-release-58.1.20/forge-test-release-58.1.20-installer.jar"
                        .to_owned(),
                    installer.clone(),
                ),
            ]),
        );
        let directory = TempDir::new().unwrap();
        let database = directory.path().join("state.sqlite3");
        let data_directory = directory.path().join("data");
        let service = AppService::open(&database, &data_directory).unwrap();
        service.skip_onboarding().unwrap();
        register_ready_java(&service, &data_directory);
        let request = build_request(
            &server,
            &version_json,
            &client,
            &asset_index,
            &native,
            asset.len() as u64,
            &installer,
            &installer_sha1,
            profile,
            loader_version_json,
        );
        Self {
            _directory: directory,
            data_directory,
            service,
            server,
            request,
            patched_bytes,
        }
    }

    fn neoforge_request(&self) -> ResolvedInstallRequest {
        let ResolvedLoader::Forge {
            installer_url,
            installer_sha1,
            installer_size,
            ..
        } = &self.request.loader
        else {
            panic!("fixture loader must be forge");
        };
        let base = self.server.url("");
        let tool_jar = fs::read(
            self.data_directory
                .join("store/minecraft/libraries/net/minecraftforge/installertools/1.4.3/installertools-1.4.3.jar"),
        )
        .unwrap_or_else(|_| processor_jar("test.InstallerTools"));
        let mut request = self.request.clone();
        request.loader = ResolvedLoader::NeoForge {
            version: "21.8.54".to_owned(),
            installer_url: installer_url.clone(),
            installer_sha1: installer_sha1.clone(),
            installer_size: *installer_size,
            install_profile: neoforge_profile(&base, &tool_jar),
            version_json: neoforge_version_json(&base, &tool_jar),
        };
        request
    }
}

#[allow(clippy::too_many_arguments)]
fn build_request(
    server: &ExecutorServer,
    version_json: &[u8],
    client: &[u8],
    asset_index: &[u8],
    native_archive: &[u8],
    asset_size: u64,
    installer: &[u8],
    installer_sha1: &str,
    profile: Value,
    loader_version_json: Value,
) -> ResolvedInstallRequest {
    let version = "test-release";
    ResolvedInstallRequest {
        instance_name: "Forge 安装测试".to_owned(),
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
        loader: ResolvedLoader::Forge {
            version: "58.1.20".to_owned(),
            installer_url: server.url(
                "/net/minecraftforge/forge/test-release-58.1.20/forge-test-release-58.1.20-installer.jar",
            ),
            installer_sha1: installer_sha1.to_owned(),
            installer_size: installer.len() as u64,
            install_profile: profile,
            version_json: loader_version_json,
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

fn forge_profile(
    base: &str,
    tool_jar: &[u8],
    patcher_jar: &[u8],
    server_tool_jar: &[u8],
    patched: &[u8],
) -> Value {
    json!({
        "spec": 1,
        "profile": "forge",
        "version": "test-release-forge-58.1.20",
        "minecraft": "test-release",
        "json": "/version.json",
        "data": {
            "MOJMAPS": {"client": "[net.minecraft:client:test-release:mappings@txt]"},
            "MC_OFF": {"client": "[net.minecraft:client:test-release:official]"},
            "BINPATCH": {"client": "/data/client.lzma"},
            "PATCHED": {"client": "[net.minecraftforge:forge:test-release-58.1.20:client]"},
            "PATCHED_SHA": {"client": format!("'{}'", digest_sha1(patched))}
        },
        "processors": [
            {
                "jar": "net.minecraftforge:installertools:1.4.3",
                "classpath": [],
                "args": ["--task", "DOWNLOAD_MOJMAPS", "--version", "test-release", "--side", "{SIDE}", "--output", "{MOJMAPS}"]
            },
            {
                "sides": ["server"],
                "jar": "net.minecraftforge:servertool:1.0.0",
                "classpath": [],
                "args": ["--server-only"]
            },
            {
                "sides": ["client"],
                "jar": "net.minecraftforge:binarypatcher:1.2.0",
                "classpath": [],
                "args": ["--clean", "{MC_OFF}", "--output", "{PATCHED}", "--apply", "{BINPATCH}", "--data", "--unpatched"]
            }
        ],
        "libraries": [
            profile_library(base, "net.minecraftforge:installertools:1.4.3", tool_jar),
            profile_library(base, "net.minecraftforge:binarypatcher:1.2.0", patcher_jar),
            profile_library(base, "net.minecraftforge:servertool:1.0.0", server_tool_jar)
        ]
    })
}

fn forge_version_json(base: &str, universal: &[u8]) -> Value {
    json!({
        "id": "test-release-forge-58.1.20",
        "mainClass": "net.minecraftforge.bootstrap.ForgeBootstrap",
        "arguments": {"game": ["--launchTarget", "forge_client"], "jvm": []},
        "libraries": [
            {
                "name": "net.minecraftforge:forge:test-release-58.1.20:universal",
                "downloads": {"artifact": {
                    "path": "net/minecraftforge/forge/test-release-58.1.20/forge-test-release-58.1.20-universal.jar",
                    "url": format!("{}/net/minecraftforge/forge/test-release-58.1.20/forge-test-release-58.1.20-universal.jar", base.trim_end_matches('/')),
                    "sha1": digest_sha1(universal),
                    "size": universal.len()
                }}
            },
            {
                "name": "net.minecraftforge:forge:test-release-58.1.20:client",
                "downloads": {"artifact": {
                    "path": "net/minecraftforge/forge/test-release-58.1.20/forge-test-release-58.1.20-client.jar",
                    "url": "",
                    "sha1": "",
                    "size": 0
                }}
            }
        ]
    })
}

fn neoforge_profile(base: &str, tool_jar: &[u8]) -> Value {
    json!({
        "spec": 1,
        "profile": "NeoForge",
        "version": "neoforge-21.8.54",
        "minecraft": "test-release",
        "json": "/version.json",
        "data": {
            "MAPPINGS": {"client": "[net.neoforged:neoform:test-release:mappings@txt]"},
            "MOJMAPS": {"client": "[net.minecraft:client:test-release:mappings@txt]"},
            "MERGED_MAPPINGS": {"client": "[net.neoforged:neoform:test-release:mappings-merged@txt]"},
            "BINPATCH": {"client": "/data/client.lzma"},
            "MC_SLIM": {"client": "[net.minecraft:client:test-release:slim]"},
            "MC_EXTRA": {"client": "[net.minecraft:client:test-release:extra]"},
            "MC_SRG": {"client": "[net.minecraft:client:test-release:srg]"},
            "PATCHED": {"client": "[net.neoforged:neoforge:21.8.54:client]"}
        },
        "processors": [
            {"jar": "net.minecraftforge:installertools:1.4.3", "classpath": [],
             "args": ["--task", "MCP_DATA", "--input", "[net.neoforged:neoform:test-release@zip]", "--output", "{MAPPINGS}", "--key", "mappings"]},
            {"jar": "net.minecraftforge:installertools:1.4.3", "classpath": [],
             "args": ["--task", "DOWNLOAD_MOJMAPS", "--version", "test-release", "--side", "{SIDE}", "--output", "{MOJMAPS}"]},
            {"jar": "net.minecraftforge:installertools:1.4.3", "classpath": [],
             "args": ["--task", "MERGE_MAPPING", "--merge", "{MAPPINGS}", "--base", "{MOJMAPS}", "--output", "{MERGED_MAPPINGS}", "--reverse-base"]},
            {"sides": ["client"], "jar": "net.minecraftforge:installertools:1.4.3", "classpath": [],
             "args": ["--input", "{MINECRAFT_JAR}", "--slim", "{MC_SLIM}", "--extra", "{MC_EXTRA}", "--srg", "{MERGED_MAPPINGS}"]},
            {"jar": "net.minecraftforge:installertools:1.4.3", "classpath": [],
             "args": ["--input", "{MC_SLIM}", "--output", "{MC_SRG}", "--names", "{MERGED_MAPPINGS}"]},
            {"jar": "net.minecraftforge:installertools:1.4.3", "classpath": [],
             "args": ["--clean", "{MC_SRG}", "--output", "{PATCHED}", "--apply", "{BINPATCH}"]}
        ],
        "libraries": [profile_library(base, "net.minecraftforge:installertools:1.4.3", tool_jar)]
    })
}

fn neoforge_version_json(base: &str, tool_jar: &[u8]) -> Value {
    json!({
        "id": "neoforge-21.8.54",
        "mainClass": "cpw.mods.bootstraplauncher.BootstrapLauncher",
        "arguments": {
            "game": ["--launchTarget", "neoforgeclient"],
            "jvm": ["-p", "${library_directory}/bootstraplauncher.jar${classpath_separator}${library_directory}/securejarhandler.jar"]
        },
        "libraries": [{
            "name": "net.neoforged.fancymodloader:bootstraplauncher:9.0.18",
            "downloads": {"artifact": {
                "path": "net/neoforged/fancymodloader/bootstraplauncher/9.0.18/bootstraplauncher-9.0.18.jar",
                "url": format!("{}/net/minecraftforge/installertools/1.4.3/installertools-1.4.3.jar", base.trim_end_matches('/')),
                "sha1": digest_sha1(tool_jar),
                "size": tool_jar.len()
            }}
        }]
    })
}

fn profile_library(base: &str, name: &str, bytes: &[u8]) -> Value {
    let coordinate = MavenCoordinate::parse(name).unwrap();
    json!({
        "name": name,
        "downloads": {"artifact": {
            "path": coordinate.relative_path(),
            "url": format!("{}/{}", base.trim_end_matches('/'), coordinate.relative_path()),
            "sha1": digest_sha1(bytes),
            "size": bytes.len()
        }}
    })
}

fn fake_runner(
    invocations: Arc<Mutex<Vec<ProcessorInvocation>>>,
    patched_bytes: Vec<u8>,
) -> Arc<moyumax_core::ProcessorRunner> {
    Arc::new(move |invocation: &ProcessorInvocation, work_dir: &Path| {
        invocations.lock().unwrap().push(invocation.clone());
        if let Some(position) = invocation.args.iter().position(|arg| arg == "--output") {
            let target = invocation
                .args
                .get(position + 1)
                .expect("--output 缺少目标");
            let target_path = PathBuf::from(target);
            fs::create_dir_all(target_path.parent().unwrap()).unwrap();
            let payload = if target.ends_with(".txt") || target.contains("mappings") {
                b"mappings-text".to_vec()
            } else if target.contains("slim")
                || target.contains("extra")
                || target.contains("srg")
                || target.contains("official")
            {
                b"intermediate-jar".to_vec()
            } else {
                patched_bytes.clone()
            };
            fs::write(&target_path, payload).unwrap();
        }
        let _ = work_dir;
        Ok(())
    })
}

fn build_installer_zip(profile: &Value, version_json: &Value, data: &[(&str, &[u8])]) -> Vec<u8> {
    let cursor = std::io::Cursor::new(Vec::new());
    let mut writer = ZipWriter::new(cursor);
    let options = SimpleFileOptions::default();
    writer.start_file("install_profile.json", options).unwrap();
    writer
        .write_all(serde_json::to_string(profile).unwrap().as_bytes())
        .unwrap();
    writer.start_file("version.json", options).unwrap();
    writer
        .write_all(serde_json::to_string(version_json).unwrap().as_bytes())
        .unwrap();
    for (path, bytes) in data {
        writer.start_file(path, options).unwrap();
        writer.write_all(bytes).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

fn processor_jar(main_class: &str) -> Vec<u8> {
    let cursor = std::io::Cursor::new(Vec::new());
    let mut writer = ZipWriter::new(cursor);
    writer
        .start_file("META-INF/MANIFEST.MF", SimpleFileOptions::default())
        .unwrap();
    writer
        .write_all(format!("Manifest-Version: 1.0\r\nMain-Class: {main_class}\r\n\r\n").as_bytes())
        .unwrap();
    writer.finish().unwrap().into_inner()
}

fn register_ready_java(service: &AppService, data_directory: &Path) {
    let home = data_directory.join("java-ready");
    fs::create_dir_all(home.join("bin")).unwrap();
    fs::write(home.join("bin/java.exe"), b"fixture").unwrap();
    service
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
        Self::with_listener(listener, responses)
    }

    fn bind() -> (TcpListener, String) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        (listener, base)
    }

    fn with_listener(listener: TcpListener, responses: HashMap<String, Vec<u8>>) -> Self {
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
