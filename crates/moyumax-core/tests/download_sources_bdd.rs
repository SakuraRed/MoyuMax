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
    AppService, ArtifactDownloader, ArtifactKind, ContentDependencyChoice, ContentExecutor,
    ContentFilePlan, ContentInstallPlan, ContentPlanEntry, DownloadCandidate, ResolvedArtifact,
    SourceCandidates, SourceChannel, SourcePolicy, TaskState, candidates_for,
};
use rusqlite::{Connection, params};
use sha1::{Digest, Sha1};
use sha2::Sha512;
use tempfile::TempDir;

#[test]
fn m10_source_001_policy_defaults_to_mirror_first_and_persists() {
    let fixture = SourceFixture::new();
    assert_eq!(
        fixture.service.download_source_policy().unwrap(),
        SourcePolicy::MirrorFirst
    );

    let custom = SourcePolicy::Custom {
        minecraft_base: Some("https://mc.example.test".to_owned()),
        modrinth_base: None,
    };
    fixture.service.set_download_source_policy(&custom).unwrap();
    let reopened = fixture.reopen();
    assert_eq!(reopened.download_source_policy().unwrap(), custom);
}

#[test]
fn m10_source_001_mirror_first_routes_to_mci_and_bmclapi() {
    let cases = [
        (
            "https://cdn.modrinth.com/data/ABC/versions/x/mod.jar",
            "https://mod.mcimirror.top/data/ABC/versions/x/mod.jar",
        ),
        (
            "https://api.modrinth.com/v2/search",
            "https://mod.mcimirror.top/modrinth/v2/search",
        ),
        (
            "https://api.curseforge.com/v1/mods/123",
            "https://mod.mcimirror.top/curseforge/v1/mods/123",
        ),
        (
            "https://piston-meta.mojang.com/mc/game/version_manifest.json",
            "https://bmclapi2.bangbang93.com/mc/game/version_manifest.json",
        ),
        (
            "https://piston-data.mojang.com/v1/objects/abc/client.jar",
            "https://bmclapi2.bangbang93.com/v1/objects/abc/client.jar",
        ),
        (
            "https://resources.download.minecraft.net/00/0123",
            "https://bmclapi2.bangbang93.com/assets/00/0123",
        ),
        (
            "https://libraries.minecraft.net/org/lwjgl/lwjgl.jar",
            "https://bmclapi2.bangbang93.com/maven/org/lwjgl/lwjgl.jar",
        ),
        (
            "https://meta.fabricmc.net/v2/versions/loader",
            "https://bmclapi2.bangbang93.com/fabric-meta/v2/versions/loader",
        ),
        (
            "https://maven.fabricmc.net/net/fabricmc/loader.jar",
            "https://bmclapi2.bangbang93.com/maven/net/fabricmc/loader.jar",
        ),
        (
            "https://meta.quiltmc.org/v3/versions",
            "https://bmclapi2.bangbang93.com/quilt-meta/v3/versions",
        ),
        (
            "https://maven.neoforged.net/releases/net/neoforged/neoforge/x.jar",
            "https://bmclapi2.bangbang93.com/maven/net/neoforged/neoforge/x.jar",
        ),
        (
            "https://files.minecraftforge.net/maven/net/minecraftforge/forge/x.jar",
            "https://bmclapi2.bangbang93.com/maven/net/minecraftforge/forge/x.jar",
        ),
        (
            "https://edge.forgecdn.net/files/1/2/mod.jar",
            "https://mod.mcimirror.top/files/1/2/mod.jar",
        ),
        (
            "https://mediafilez.forgecdn.net/files/8183/529/mod.jar",
            "https://mod.mcimirror.top/files/8183/529/mod.jar",
        ),
    ];
    for (official, mirror) in cases {
        let SourceCandidates::Ready(candidates) =
            candidates_for(official, &SourcePolicy::MirrorFirst)
        else {
            panic!("镜像优先必须产出候选: {official}");
        };
        assert!(
            candidates.len() >= 2,
            "镜像优先应给出镜像与官方回退: {official}"
        );
        assert_eq!(candidates[0].url, mirror, "镜像映射不符: {official}");
        assert_eq!(candidates[0].channel, SourceChannel::Mirror);
        assert_eq!(candidates[1].url, official, "官方回退必须保留原始 URL");
        assert_eq!(candidates[1].channel, SourceChannel::Official);
    }
}

#[test]
fn m10_source_001_official_first_prefers_official_then_mirror() {
    let SourceCandidates::Ready(candidates) = candidates_for(
        "https://cdn.modrinth.com/data/ABC/mod.jar",
        &SourcePolicy::OfficialFirst,
    ) else {
        panic!("官方优先必须产出候选");
    };
    assert_eq!(
        candidates[0].url,
        "https://cdn.modrinth.com/data/ABC/mod.jar"
    );
    assert_eq!(candidates[0].channel, SourceChannel::Official);
    assert_eq!(candidates[0].label, "Modrinth 官方");
    assert_eq!(
        candidates[1].url,
        "https://mod.mcimirror.top/data/ABC/mod.jar"
    );

    let SourceCandidates::Ready(azul) = candidates_for(
        "https://cdn.azul.com/zulu/bin/zulu21.zip",
        &SourcePolicy::OfficialFirst,
    ) else {
        panic!("Azul 域名必须产出候选");
    };
    assert_eq!(azul.len(), 1, "无镜像域名只保留官方链路");
    assert_eq!(azul[0].channel, SourceChannel::Official);
}

#[test]
fn m10_source_003_curseforge_official_first_is_unavailable_and_never_connects() {
    let candidates = candidates_for(
        "https://edge.forgecdn.net/files/1/2/mod.jar",
        &SourcePolicy::OfficialFirst,
    );
    let SourceCandidates::CurseForgeOfficialUnavailable { mirror } = candidates else {
        panic!("CurseForge 官方优先必须标记官方不可用: {candidates:?}");
    };
    assert_eq!(mirror.channel, SourceChannel::Mirror);
    assert_eq!(mirror.label, "MCI Mirror");

    // 经执行器请求时:不得发起官方直连,错误必须说明不可用并给出镜像一个可执行路径。
    let body = b"curseforge-file".to_vec();
    let artifact = ResolvedArtifact {
        kind: ArtifactKind::ContentMod,
        relative_path: "content/curseforge/mod.jar".to_owned(),
        url: "https://edge.forgecdn.net/files/1/2/mod.jar".to_owned(),
        size: u64::try_from(body.len()).unwrap(),
        sha1: Some(digest_sha1(&body)),
        sha256: None,
        sha512: Some(digest_sha512(&body)),
    };
    let directory = TempDir::new().unwrap();
    let error = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(ArtifactDownloader::new(1).unwrap().fetch_with_policy(
            &artifact,
            &directory.path().join("staging"),
            &directory.path().join("shared"),
            &SourcePolicy::OfficialFirst,
            None,
        ))
        .expect_err("CurseForge 官方优先必须失败且不发起直连");
    let message = error.to_string();
    assert!(message.contains("CurseForge 官方源不可用"), "{message}");
    assert!(
        message.contains("mcimirror.top"),
        "应提供 MCI Mirror 路径: {message}"
    );
}

#[tokio::test]
async fn m10_source_002_custom_source_never_switches() {
    let body = b"custom-source-body".to_vec();
    let server = FixtureServer::new(HashMap::from([("/mc/game/x.jar".to_owned(), body.clone())]));
    let policy = SourcePolicy::Custom {
        minecraft_base: Some(server.base.clone()),
        modrinth_base: None,
    };
    let artifact = ResolvedArtifact {
        kind: ArtifactKind::GameClient,
        relative_path: "minecraft/versions/test/x.jar".to_owned(),
        url: "https://piston-meta.mojang.com/mc/game/x.jar".to_owned(),
        size: u64::try_from(body.len()).unwrap(),
        sha1: Some(digest_sha1(&body)),
        sha256: None,
        sha512: Some(digest_sha512(&body)),
    };
    let directory = TempDir::new().unwrap();
    let report = ArtifactDownloader::new(1)
        .unwrap()
        .fetch_with_policy(
            &artifact,
            &directory.path().join("staging"),
            &directory.path().join("shared"),
            &policy,
            None,
        )
        .await
        .expect("自定义源应完成下载");
    assert_eq!(report.channel, SourceChannel::Custom);
    assert_eq!(report.final_label, "自定义源");

    // 自定义源失败时不得切换到官方或镜像:只有一个失败尝试记录。
    let dead_policy = SourcePolicy::Custom {
        minecraft_base: Some("http://127.0.0.1:1".to_owned()),
        modrinth_base: None,
    };
    let missing_dir = TempDir::new().unwrap();
    let error = ArtifactDownloader::new(1)
        .unwrap()
        .fetch_with_policy(
            &artifact,
            &missing_dir.path().join("staging"),
            &missing_dir.path().join("shared"),
            &dead_policy,
            None,
        )
        .await
        .expect_err("自定义源失败必须直接失败,不得切换");
    assert!(error.to_string().contains("127.0.0.1"));

    // 未配置基址的域名:明确报错且不得切换。
    let SourceCandidates::CustomUnsupported { reason } = candidates_for(
        "https://cdn.modrinth.com/data/ABC/mod.jar",
        &SourcePolicy::Custom {
            minecraft_base: Some("https://mc.example.test".to_owned()),
            modrinth_base: None,
        },
    ) else {
        panic!("未配置域名必须报 CustomUnsupported");
    };
    assert!(reason.contains("自定义源"));
}

#[tokio::test]
async fn m10_source_001_mirror_failure_falls_back_to_official_with_record() {
    let body = b"official-body".to_vec();
    let server = FixtureServer::new(HashMap::from([("/mod.jar".to_owned(), body.clone())]));
    let artifact = ResolvedArtifact {
        kind: ArtifactKind::ContentMod,
        relative_path: "content/modrinth/mod.jar".to_owned(),
        url: "https://cdn.modrinth.com/data/ABC/mod.jar".to_owned(),
        size: u64::try_from(body.len()).unwrap(),
        sha1: Some(digest_sha1(&body)),
        sha256: None,
        sha512: Some(digest_sha512(&body)),
    };
    let candidates = vec![
        DownloadCandidate {
            url: "http://127.0.0.1:1/mirror.jar".to_owned(),
            channel: SourceChannel::Mirror,
            label: "MCI Mirror".to_owned(),
        },
        DownloadCandidate {
            url: server.url("/mod.jar"),
            channel: SourceChannel::Official,
            label: "Modrinth 官方".to_owned(),
        },
    ];
    let directory = TempDir::new().unwrap();
    let report = ArtifactDownloader::new(1)
        .unwrap()
        .fetch_with_candidates(
            &artifact,
            &directory.path().join("staging"),
            &directory.path().join("shared"),
            &candidates,
            None,
        )
        .await
        .expect("镜像失败后应回退官方源");

    assert_eq!(report.final_label, "Modrinth 官方");
    // 镜像连接失败属于瞬态网络错误:同一候选先重试 3 次(逐次记录),再回退官方源。
    assert_eq!(report.attempts.len(), 5);
    for attempt in &report.attempts[..4] {
        assert_eq!(attempt.label, "MCI Mirror");
        assert!(
            matches!(
                attempt.outcome,
                moyumax_core::SourceAttemptOutcome::Failed { .. }
            ),
            "镜像的每次失败都必须记录"
        );
    }
    assert_eq!(report.attempts[4].label, "Modrinth 官方");
    assert_eq!(
        report.attempts[4].outcome,
        moyumax_core::SourceAttemptOutcome::Success
    );
}

#[tokio::test]
async fn m10_source_001_content_task_records_source_detail() {
    let fast = b"fabric-api-fixture".to_vec();
    let root = b"continuity-fixture".to_vec();
    let server = FixtureServer::new(HashMap::from([
        ("/fabric-api.jar".to_owned(), fast.clone()),
        ("/continuity.jar".to_owned(), root.clone()),
    ]));
    let fixture = SourceFixture::new();
    fixture.insert_instance("instance-source", "来源测试");
    let plan = ContentInstallPlan {
        schema_version: 1,
        instance_id: "instance-source".to_owned(),
        instance_name: "来源测试".to_owned(),
        game_version: "26.2".to_owned(),
        loader: "fabric".to_owned(),
        root_project_id: "ROOT0001".to_owned(),
        entries: vec![
            content_entry(
                "DEP00001",
                "DEPVER01",
                "Fabric API",
                "fabric-api.jar",
                server.url("/fabric-api.jar"),
                &fast,
                Some("ROOT0001"),
            ),
            content_entry(
                "ROOT0001",
                "ROOTVER1",
                "Continuity",
                "continuity.jar",
                server.url("/continuity.jar"),
                &root,
                None,
            ),
        ],
        optional_dependencies: Vec::<ContentDependencyChoice>::new(),
        incompatible_dependencies: Vec::<ContentDependencyChoice>::new(),
        is_update: false,
    };
    // 默认镜像优先策略下,fixture 域名(Other)只保留官方候选,
    // 任务完成后必须在进度中持久化真实来源与尝试记录。
    let task = fixture.service.enqueue_content_install_task(&plan).unwrap();
    ContentExecutor::new(2)
        .unwrap()
        .execute_task(&fixture.service, &task.id)
        .await
        .expect("内容任务应完成");

    let tasks = fixture.service.list_content_install_tasks().unwrap();
    assert_eq!(tasks[0].state, TaskState::Completed);
    let detail = tasks[0]
        .progress
        .source_detail
        .as_ref()
        .expect("任务必须记录来源详情");
    assert_eq!(detail.final_label, "官方源");
    assert_eq!(detail.channel, SourceChannel::Official);
    assert!(
        detail
            .attempts
            .iter()
            .all(|attempt| matches!(attempt.outcome, moyumax_core::SourceAttemptOutcome::Success))
    );
}

struct SourceFixture {
    _directory: TempDir,
    database_path: PathBuf,
    data_directory: PathBuf,
    service: AppService,
}

impl SourceFixture {
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

    fn insert_instance(&self, id: &str, name: &str) {
        let root = self.data_directory.join("instances").join(id);
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
}

fn content_entry(
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

struct FixtureServer {
    base: String,
    _thread: thread::JoinHandle<()>,
}

impl FixtureServer {
    fn new(responses: HashMap<String, Vec<u8>>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let responses = Arc::new(responses);
        let server_thread = thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                let responses = Arc::clone(&responses);
                thread::spawn(move || serve(stream, &responses));
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

fn serve(mut stream: TcpStream, responses: &HashMap<String, Vec<u8>>) {
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
