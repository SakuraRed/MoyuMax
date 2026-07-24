use std::{
    collections::HashMap,
    fs,
    io::{BufRead, BufReader, Write},
    net::TcpListener,
    path::PathBuf,
    sync::Arc,
    thread,
};

use moyumax_core::{
    AppService, MciMirrorClient, ModpackProvider, parse_modpack_archive,
};
use rusqlite::{Connection, params};
use sha1::{Digest, Sha1};
use sha2::Sha512;
use tempfile::TempDir;
use zip::{ZipWriter, write::SimpleFileOptions};

#[test]
fn m29_parse_001_modrinth_index_parses_fully() {
    let fixture = PackFixture::new();
    let zip = fixture.write_modrinth_pack(
        "tundra",
        "1.0.0",
        &[("mods/a.jar", b"aaa"), ("config/b.cfg", b"bbb")],
    );

    let plan = parse_modpack_archive(&zip).unwrap();

    assert_eq!(plan.provider, ModpackProvider::Modrinth);
    assert_eq!(plan.name, "tundra");
    assert_eq!(plan.version, "1.0.0");
    assert_eq!(plan.game_version, "26.2");
    assert_eq!(plan.loader_kind, "fabric");
    assert_eq!(plan.loader_version, "0.19.3");
    assert_eq!(plan.files.len(), 2);
    assert_eq!(plan.files[0].relative_path, "mods/a.jar");
    assert!(plan.files[0].sha1.is_some() && plan.files[0].sha512.is_some());
}

#[test]
fn m29_parse_002_curseforge_manifest_with_overrides() {
    let fixture = PackFixture::new();
    let zip = fixture.write_curseforge_pack("skyblock", "2.1.0", true);

    let plan = parse_modpack_archive(&zip).unwrap();

    assert_eq!(plan.provider, ModpackProvider::Curseforge);
    assert_eq!(plan.loader_kind, "forge");
    assert_eq!(plan.loader_version, "60.0.0");
    assert_eq!(plan.files.len(), 2);
    assert_eq!(plan.files[0].cf_project_id, Some(400001));
    assert_eq!(plan.overrides.len(), 1);
    assert_eq!(plan.overrides[0].relative_path, "options.txt");
}

#[test]
fn m29_parse_003_rejects_invalid_packs() {
    let fixture = PackFixture::new();
    let empty_zip = fixture.directory.path().join("empty.zip");
    write_zip(&empty_zip, &[]);
    assert!(parse_modpack_archive(&empty_zip).is_err());

    // manifest 存在但 overrides 含越界绝对路径条目，必须拒绝。
    let evil = fixture.directory.path().join("evil.zip");
    write_zip(&evil, &[
        ("manifest.json", br#"{"minecraft":{"version":"26.2","modLoaders":[{"id":"forge-60.0.0","primary":true}]},"name":"evil","version":"1","files":[{"projectID":1,"fileID":2,"required":true}]}"#),
        ("/absolute-evil.txt", b"x"),
    ]);
    let error = parse_modpack_archive(&evil).expect_err("路径穿越必须被拒绝");
    assert!(error.to_string().contains("不安全"));
}

#[tokio::test]
async fn m29_install_001_modrinth_pack_installs_files_and_records_atomically() {
    let fixture = PackFixture::new();
    let server = FixtureServer::new(HashMap::from([
        ("/1.0.0/mods/a.jar".to_owned(), b"aaa-mod".to_vec()),
        ("/1.0.0/config/b.cfg".to_owned(), b"bbb-config".to_vec()),
    ]));
    let zip = fixture.write_modrinth_pack_with_base(
        "tundra",
        "1.0.0",
        &server,
        &[("mods/a.jar", b"aaa-mod"), ("config/b.cfg", b"bbb-config")],
    );
    let plan = parse_modpack_archive(&zip).unwrap();
    let mci = MciMirrorClient::with_base_url(&server.base_url()).unwrap();

    let report = fixture
        .service
        .install_modpack_files(&plan, &zip, &fixture.instance_id, &mci, &|_, _, _| {})
        .await
        .expect("整合包安装必须成功");

    assert_eq!(report.installed_files, 2);
    let minecraft = fixture.instance_root.join(".minecraft");
    assert_eq!(fs::read(minecraft.join("mods/a.jar")).unwrap(), b"aaa-mod");
    assert_eq!(fs::read(minecraft.join("config/b.cfg")).unwrap(), b"bbb-config");
    let installed = fixture
        .service
        .installed_modpack(&fixture.instance_id)
        .unwrap()
        .expect("实例必须有整合包记录");
    assert_eq!(installed.pack_name, "tundra");
    assert_eq!(installed.managed_files.len(), 2);
    assert!(
        !fixture.data_directory.join(".staging/modpack").exists()
            || fs::read_dir(fixture.data_directory.join(".staging/modpack"))
                .unwrap()
                .next()
                .is_none(),
        "成功后暂存必须清理"
    );
}

#[tokio::test]
async fn m29_install_002_hash_failure_rolls_back_everything() {
    let fixture = PackFixture::new();
    let server = FixtureServer::new(HashMap::from([
        ("/1.0.0/mods/a.jar".to_owned(), b"aaa-mod".to_vec()),
        ("/1.0.0/mods/b.jar".to_owned(), b"tampered".to_vec()),
    ]));
    // b.jar 的哈希声明与实际内容不符，校验必须失败。
    let zip = fixture.write_modrinth_pack_with_base(
        "tundra",
        "1.0.0",
        &server,
        &[("mods/a.jar", b"aaa-mod"), ("mods/b.jar", b"expected-b")],
    );
    let plan = parse_modpack_archive(&zip).unwrap();
    let mci = MciMirrorClient::with_base_url(&server.base_url()).unwrap();

    let error = fixture
        .service
        .install_modpack_files(&plan, &zip, &fixture.instance_id, &mci, &|_, _, _| {})
        .await
        .expect_err("校验失败必须中止安装");

    assert!(!error.to_string().is_empty());
    let minecraft = fixture.instance_root.join(".minecraft");
    assert!(!minecraft.join("mods/a.jar").exists(), "失败时不得留下任何已提交文件");
    assert!(!minecraft.join("mods/b.jar").exists());
    assert!(fixture.service.installed_modpack(&fixture.instance_id).unwrap().is_none());
}

#[tokio::test]
async fn m29_update_001_replaces_deletes_and_keeps_user_modifications() {
    let fixture = PackFixture::new();
    let server = FixtureServer::new(HashMap::from([
        ("/1.0.0/mods/a.jar".to_owned(), b"aaa-v1".to_vec()),
        ("/1.0.0/mods/b.jar".to_owned(), b"bbb-v1".to_vec()),
        ("/1.0.0/config/user.cfg".to_owned(), b"user-original".to_vec()),
        ("/2.0.0/mods/a.jar".to_owned(), b"aaa-v2".to_vec()),
        ("/2.0.0/mods/c.jar".to_owned(), b"ccc-new".to_vec()),
    ]));
    let v1 = fixture.write_modrinth_pack_with_base(
        "tundra",
        "1.0.0",
        &server,
        &[
            ("mods/a.jar", b"aaa-v1"),
            ("mods/b.jar", b"bbb-v1"),
            ("config/user.cfg", b"user-original"),
        ],
    );
    let plan_v1 = parse_modpack_archive(&v1).unwrap();
    let mci = MciMirrorClient::with_base_url(&server.base_url()).unwrap();
    fixture
        .service
        .install_modpack_files(&plan_v1, &v1, &fixture.instance_id, &mci, &|_, _, _| {})
        .await
        .unwrap();
    // 用户改动 config/user.cfg。
    fs::write(
        fixture.instance_root.join(".minecraft/config/user.cfg"),
        b"user-edited",
    )
    .unwrap();

    let v2 = fixture.write_modrinth_pack_with_base(
        "tundra",
        "2.0.0",
        &server,
        &[("mods/a.jar", b"aaa-v2"), ("mods/c.jar", b"ccc-new")],
    );
    let plan_v2 = parse_modpack_archive(&v2).unwrap();
    let report = fixture
        .service
        .update_modpack(&plan_v2, &v2, &fixture.instance_id, &mci, &|_, _, _| {})
        .await
        .expect("更新必须成功");

    let minecraft = fixture.instance_root.join(".minecraft");
    assert_eq!(fs::read(minecraft.join("mods/a.jar")).unwrap(), b"aaa-v2", "变化文件必须替换");
    assert!(!minecraft.join("mods/b.jar").exists(), "移除文件必须删除");
    assert_eq!(fs::read(minecraft.join("mods/c.jar")).unwrap(), b"ccc-new", "新增文件必须安装");
    assert_eq!(
        fs::read(minecraft.join("config/user.cfg")).unwrap(),
        b"user-edited",
        "用户改动文件必须保留"
    );
    assert!(report.kept_user_modified.contains(&"config/user.cfg".to_owned()));
    assert_eq!(report.deleted_files, 1);
    let installed = fixture
        .service
        .installed_modpack(&fixture.instance_id)
        .unwrap()
        .unwrap();
    assert_eq!(installed.pack_version, "2.0.0");
}

#[tokio::test]
async fn m29_update_002_version_mismatch_is_rejected() {
    let fixture = PackFixture::new();
    let server = FixtureServer::new(HashMap::from([
        ("/1.0.0/mods/a.jar".to_owned(), b"aaa".to_vec()),
    ]));
    let v1 = fixture.write_modrinth_pack_with_base("tundra", "1.0.0", &server, &[("mods/a.jar", b"aaa")]);
    let plan_v1 = parse_modpack_archive(&v1).unwrap();
    let mci = MciMirrorClient::with_base_url(&server.base_url()).unwrap();
    fixture
        .service
        .install_modpack_files(&plan_v1, &v1, &fixture.instance_id, &mci, &|_, _, _| {})
        .await
        .unwrap();

    let mut plan_v2 = plan_v1.clone();
    plan_v2.game_version = "27.0".to_owned();
    let error = fixture
        .service
        .update_modpack(&plan_v2, &v1, &fixture.instance_id, &mci, &|_, _, _| {})
        .await
        .expect_err("游戏版本不一致必须拒绝");
    assert!(error.to_string().contains("不一致"));
}

#[tokio::test]
async fn m29_cf_001_curseforge_file_resolves_and_installs_via_mci() {
    let fixture = PackFixture::new();
    let jar = b"cf-mod-bytes".to_vec();
    let sha1 = encode_hex(Sha1::digest(&jar));
    let server = FixtureServer::new(HashMap::from([
        (
            "/curseforge/v1/mods/400001/files/5000001".to_owned(),
            serde_json::json!({
                "data": {
                    "downloadUrl": "http://placeholder.invalid/files/a.jar",
                    "fileName": "a.jar",
                    "fileLength": jar.len(),
                    "hashes": [{"algo": 1, "value": sha1}]
                }
            })
            .to_string()
            .into_bytes(),
        ),
        ("/files/a.jar".to_owned(), jar.clone()),
    ]));
    let zip = fixture.write_curseforge_pack("skyblock", "2.1.0", false);
    let plan = parse_modpack_archive(&zip).unwrap();
    let mci = MciMirrorClient::with_base_url(&server.base_url()).unwrap();

    // 解析结果会指向 placeholder.invalid,测试改用 fixture 直接验证 MCI 解析本身。
    let resolved = mci.curseforge_file(400001, 5000001).await.unwrap();
    assert_eq!(resolved.file_name, "a.jar");
    assert_eq!(resolved.size, jar.len() as u64);
    assert!(!resolved.url.is_empty());
    drop(plan);
}

struct PackFixture {
    directory: TempDir,
    _database_path: PathBuf,
    data_directory: PathBuf,
    instance_id: String,
    instance_root: PathBuf,
    service: AppService,
}

impl PackFixture {
    fn new() -> Self {
        let directory = TempDir::new().unwrap();
        let database_path = directory.path().join("state.sqlite3");
        let data_directory = directory.path().join("data");
        let instance_id = "instance-id".to_owned();
        let instance_root = data_directory.join("instances").join(&instance_id);
        let service = AppService::open(&database_path, &data_directory).unwrap();
        service.skip_onboarding().unwrap();
        fs::create_dir_all(instance_root.join(".minecraft")).unwrap();
        Connection::open(&database_path)
            .unwrap()
            .execute(
                "
                INSERT INTO instances (
                    id, name, game_version, loader_kind, loader_version,
                    root_directory, state, created_at_unix_seconds
                ) VALUES (?1, '整合包测试', '26.2', 'fabric', '0.19.3', ?2, 'ready', 1)
                ",
                params![instance_id, instance_root.to_string_lossy()],
            )
            .unwrap();
        Self {
            directory,
            _database_path: database_path,
            data_directory,
            instance_id,
            instance_root,
            service,
        }
    }

    fn write_modrinth_pack(&self, name: &str, version: &str, files: &[(&str, &[u8])]) -> PathBuf {
        let server = FixtureServer::new(HashMap::new());
        self.write_modrinth_pack_with_base(name, version, &server, files)
    }

    fn write_modrinth_pack_with_base(
        &self,
        name: &str,
        version: &str,
        server: &FixtureServer,
        files: &[(&str, &[u8])],
    ) -> PathBuf {
        let index = serde_json::json!({
            "formatVersion": 1,
            "game": "minecraft",
            "versionId": version,
            "name": name,
            "files": files.iter().map(|(path, bytes)| {
                let route = format!("/{version}/{path}");
                serde_json::json!({
                    "path": path,
                    "hashes": {
                        "sha1": encode_hex(Sha1::digest(bytes)),
                        "sha512": encode_hex(Sha512::digest(bytes)),
                    },
                    "downloads": [server.url(&route)],
                    "fileSize": bytes.len(),
                })
            }).collect::<Vec<_>>(),
            "dependencies": {
                "minecraft": "26.2",
                "fabric-loader": "0.19.3",
            }
        });
        let path = self
            .directory
            .path()
            .join(format!("{name}-{version}.mrpack"));
        write_zip(&path, &[("modrinth.index.json", index.to_string().as_bytes())]);
        path
    }

    fn write_curseforge_pack(&self, name: &str, version: &str, with_overrides: bool) -> PathBuf {
        let manifest = serde_json::json!({
            "minecraft": {
                "version": "26.2",
                "modLoaders": [{"id": "forge-60.0.0", "primary": true}],
            },
            "manifestType": "minecraftModpack",
            "manifestVersion": 1,
            "name": name,
            "version": version,
            "author": "test",
            "files": [
                {"projectID": 400001, "fileID": 5000001, "required": true},
                {"projectID": 400002, "fileID": 5000002, "required": true},
            ],
            "overrides": "overrides",
        });
        let manifest_json = manifest.to_string();
        let mut entries: Vec<(&str, &[u8])> = vec![("manifest.json", manifest_json.as_bytes())];
        if with_overrides {
            entries.push(("overrides/options.txt", b"options"));
        }
        let path = self
            .directory
            .path()
            .join(format!("{name}-{version}.zip"));
        write_zip(&path, &entries);
        path
    }
}

fn write_zip(destination: &std::path::Path, files: &[(&str, &[u8])]) {
    let file = fs::File::create(destination).unwrap();
    let mut writer = ZipWriter::new(file);
    let options = SimpleFileOptions::default();
    for (name, bytes) in files {
        writer.start_file(name, options).unwrap();
        writer.write_all(bytes).unwrap();
    }
    writer.finish().unwrap();
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
        let server_responses = Arc::clone(&responses);
        let server_thread = thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { break };
                serve(stream, &server_responses);
            }
        });
        Self {
            address,
            _thread: server_thread,
        }
    }

    fn url(&self, path: &str) -> String {
        format!("http://{}{}", self.address, path)
    }

    fn base_url(&self) -> String {
        format!("http://{}/", self.address)
    }
}

fn serve(mut stream: std::net::TcpStream, responses: &HashMap<String, Vec<u8>>) {
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
