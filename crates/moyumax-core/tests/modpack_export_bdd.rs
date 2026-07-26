//! M30 实例导出整合包（Modrinth mrpack）BDD：index 结构、引用与本体分流、
//! 选项开关、自家导入闭环读回、原子写入与校验。

use std::{
    fs,
    path::{Path, PathBuf},
};

use moyumax_core::{AppService, ExportModpackOptions, ModpackProvider, parse_modpack_archive};
use rusqlite::{Connection, params};
use sha1::{Digest, Sha1};
use sha2::Sha512;
use tempfile::TempDir;
use zip::ZipArchive;

const JAR_BYTES: &[u8] = b"jei-mod-bytes";

#[test]
fn m30_export_001_index_structure_matches_mrpack_spec() {
    let fixture = ExportFixture::new();
    let destination = fixture.destination("tundra-1.0.0.mrpack");

    let report = fixture
        .service
        .export_instance_modpack(&fixture.instance_id, &destination, &fixture.options())
        .expect("导出必须成功");

    assert_eq!(report.pack_name, "tundra");
    assert_eq!(report.pack_version, "1.0.0");
    let index = read_index(&destination);
    assert_eq!(index["formatVersion"], 1);
    assert_eq!(index["game"], "minecraft");
    assert_eq!(index["name"], "tundra");
    assert_eq!(index["versionId"], "1.0.0");
    assert_eq!(index["dependencies"]["minecraft"], "26.2");
    assert_eq!(index["dependencies"]["fabric-loader"], "0.19.3");
    let files = index["files"].as_array().expect("files 必须是数组");
    assert_eq!(files.len(), 1);
    let file = &files[0];
    assert_eq!(file["path"], "mods/jei.jar");
    assert_eq!(file["hashes"]["sha1"], encode_hex(Sha1::digest(JAR_BYTES)));
    assert_eq!(
        file["hashes"]["sha512"],
        encode_hex(Sha512::digest(JAR_BYTES))
    );
    assert_eq!(file["fileSize"], JAR_BYTES.len() as u64);
    assert_eq!(
        file["downloads"][0],
        "https://cdn.modrinth.com/data/AABBCCDD/versions/VVVVVVVV/jei.jar"
    );
    assert_eq!(file["env"]["client"], "required");
}

#[test]
fn m30_export_002_references_and_bundled_split_by_available_records() {
    let fixture = ExportFixture::new();
    let destination = fixture.destination("tundra-1.0.0.mrpack");

    let report = fixture
        .service
        .export_instance_modpack(&fixture.instance_id, &destination, &fixture.options())
        .expect("导出必须成功");

    // 有完整本地记录（projectId/versionId/哈希/大小）的内容走 files 引用；
    // 其余（手动导入的 Mod、配置、资源包、光影）诚实降级为本体打包。
    assert_eq!(report.referenced_files, 1);
    assert_eq!(report.bundled_files, 5);
    let entries = zip_entry_names(&destination);
    assert!(entries.contains(&"overrides/mods/manual.jar".to_owned()));
    assert!(entries.contains(&"overrides/config/jei.json".to_owned()));
    assert!(entries.contains(&"overrides/options.txt".to_owned()));
    assert!(entries.contains(&"overrides/resourcepacks/faithful.zip".to_owned()));
    assert!(entries.contains(&"overrides/shaderpacks/seus.zip".to_owned()));
    // 已引用的 jei.jar 不得重复打入 overrides；默认关闭的项不进包。
    assert!(!entries.contains(&"overrides/mods/jei.jar".to_owned()));
    assert!(!entries.iter().any(|name| name.contains("servers.dat")));
    assert!(!entries.iter().any(|name| name.contains("screenshots")));
}

#[test]
fn m30_export_003_option_toggles_control_overrides() {
    let fixture = ExportFixture::new();
    let destination = fixture.destination("tundra-1.0.0.mrpack");
    let options = ExportModpackOptions {
        include_config: false,
        include_resource_packs: false,
        include_shaders: false,
        include_servers: true,
        include_screenshots: true,
        ..fixture.options()
    };

    let report = fixture
        .service
        .export_instance_modpack(&fixture.instance_id, &destination, &options)
        .expect("导出必须成功");

    assert_eq!(report.bundled_files, 3);
    let entries = zip_entry_names(&destination);
    assert!(entries.contains(&"overrides/mods/manual.jar".to_owned()));
    assert!(entries.contains(&"overrides/servers.dat".to_owned()));
    assert!(entries.contains(&"overrides/screenshots/shot.png".to_owned()));
    assert!(!entries.iter().any(|name| name.contains("config/")));
    assert!(!entries.contains(&"overrides/options.txt".to_owned()));
    assert!(!entries.iter().any(|name| name.contains("resourcepacks")));
    assert!(!entries.iter().any(|name| name.contains("shaderpacks")));
}

#[test]
fn m30_export_004_own_importer_reads_back_exported_pack() {
    let fixture = ExportFixture::new();
    let destination = fixture.destination("tundra-1.0.0.mrpack");
    fixture
        .service
        .export_instance_modpack(&fixture.instance_id, &destination, &fixture.options())
        .expect("导出必须成功");

    // 闭环：产物必须能被自家 mrpack 导入解析管线读回。
    let plan = parse_modpack_archive(&destination).expect("自家导入必须能读回产物");
    assert_eq!(plan.provider, ModpackProvider::Modrinth);
    assert_eq!(plan.name, "tundra");
    assert_eq!(plan.version, "1.0.0");
    assert_eq!(plan.game_version, "26.2");
    assert_eq!(plan.loader_kind, "fabric");
    assert_eq!(plan.loader_version, "0.19.3");
    assert_eq!(plan.files.len(), 1);
    assert_eq!(plan.files[0].relative_path, "mods/jei.jar");
    assert_eq!(
        plan.files[0].url.as_deref(),
        Some("https://cdn.modrinth.com/data/AABBCCDD/versions/VVVVVVVV/jei.jar")
    );
    assert!(plan.files[0].sha1.is_some() && plan.files[0].sha512.is_some());
    assert_eq!(plan.files[0].size, JAR_BYTES.len() as u64);
    let override_paths: Vec<&str> = plan
        .overrides
        .iter()
        .map(|entry| entry.relative_path.as_str())
        .collect();
    for expected in [
        "mods/manual.jar",
        "config/jei.json",
        "options.txt",
        "resourcepacks/faithful.zip",
        "shaderpacks/seus.zip",
    ] {
        assert!(
            override_paths.contains(&expected),
            "缺少 overrides：{expected}"
        );
    }
    // overrides 内容必须与磁盘一致。
    let file = fs::File::open(&destination).unwrap();
    let mut archive = ZipArchive::new(file).unwrap();
    let mut entry = archive.by_name("overrides/options.txt").unwrap();
    let mut buffer = Vec::new();
    std::io::Read::read_to_end(&mut entry, &mut buffer).unwrap();
    assert_eq!(buffer, b"options-content");
}

#[test]
fn m30_export_005_validation_and_atomic_write() {
    let fixture = ExportFixture::new();

    // 目标扩展名必须是 .mrpack。
    let wrong_ext = fixture.directory.path().join("pack.zip");
    assert!(
        fixture
            .service
            .export_instance_modpack(&fixture.instance_id, &wrong_ext, &fixture.options())
            .is_err()
    );
    // 目标不能在实例目录内部。
    let inside = fixture.instance_root.join("pack.mrpack");
    assert!(
        fixture
            .service
            .export_instance_modpack(&fixture.instance_id, &inside, &fixture.options())
            .is_err()
    );
    // 名称与版本不能为空。
    let blank_name = ExportModpackOptions {
        name: "   ".to_owned(),
        ..fixture.options()
    };
    assert!(
        fixture
            .service
            .export_instance_modpack(
                &fixture.instance_id,
                &fixture.destination("pack.mrpack"),
                &blank_name,
            )
            .is_err()
    );
    // 无受支持加载器的实例拒绝导出（保证自家导入闭环）。
    fixture.insert_instance("vanilla-id", "vanilla", None);
    assert!(
        fixture
            .service
            .export_instance_modpack(
                "vanilla-id",
                &fixture.destination("vanilla.mrpack"),
                &fixture.options(),
            )
            .is_err()
    );
    // 成功导出后目标目录不得残留暂存文件。
    let destination = fixture.destination("tundra-1.0.0.mrpack");
    fixture
        .service
        .export_instance_modpack(&fixture.instance_id, &destination, &fixture.options())
        .expect("导出必须成功");
    let leftovers: Vec<_> = fs::read_dir(destination.parent().unwrap())
        .unwrap()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_name().to_string_lossy().contains("moyu-partial"))
        .collect();
    assert!(leftovers.is_empty(), "暂存文件必须清理");
}

#[test]
fn m30_export_006_pack_without_references_still_roundtrips() {
    let directory = TempDir::new().unwrap();
    let database_path = directory.path().join("state.sqlite3");
    let data_directory = directory.path().join("data");
    let instance_root = data_directory.join("instances").join("empty-id");
    let service = AppService::open(&database_path, &data_directory).unwrap();
    service.skip_onboarding().unwrap();
    fs::create_dir_all(instance_root.join(".minecraft/config")).unwrap();
    fs::write(instance_root.join(".minecraft/options.txt"), b"opts").unwrap();
    fs::write(instance_root.join(".minecraft/config/a.cfg"), b"cfg").unwrap();
    Connection::open(&database_path)
        .unwrap()
        .execute(
            "
            INSERT INTO instances (
                id, name, game_version, loader_kind, loader_version,
                root_directory, state, created_at_unix_seconds
            ) VALUES ('empty-id', '空包', '26.2', 'fabric', '0.19.3', ?1, 'ready', 1)
            ",
            params![instance_root.to_string_lossy()],
        )
        .unwrap();
    let destination = directory.path().join("empty-1.0.0.mrpack");
    let options = ExportModpackOptions {
        name: "empty".to_owned(),
        version: "1.0.0".to_owned(),
        include_config: true,
        include_resource_packs: true,
        include_shaders: true,
        include_servers: false,
        include_screenshots: false,
    };

    let report = service
        .export_instance_modpack("empty-id", &destination, &options)
        .expect("纯配置包必须能导出");
    assert_eq!(report.referenced_files, 0);
    assert_eq!(report.bundled_files, 2);

    // 没有 files 引用的包自家导入也必须能解析（按 overrides 安装）。
    let plan = parse_modpack_archive(&destination).expect("纯 overrides 包必须能读回");
    assert!(plan.files.is_empty());
    assert_eq!(plan.overrides.len(), 2);
}

struct ExportFixture {
    directory: TempDir,
    _database_path: PathBuf,
    instance_id: String,
    instance_root: PathBuf,
    service: AppService,
}

impl ExportFixture {
    fn new() -> Self {
        let directory = TempDir::new().unwrap();
        let database_path = directory.path().join("state.sqlite3");
        let data_directory = directory.path().join("data");
        let instance_id = "instance-id".to_owned();
        let instance_root = data_directory.join("instances").join(&instance_id);
        let service = AppService::open(&database_path, &data_directory).unwrap();
        service.skip_onboarding().unwrap();
        let minecraft = instance_root.join(".minecraft");
        for sub in [
            "mods",
            "config",
            "resourcepacks",
            "shaderpacks",
            "screenshots",
        ] {
            fs::create_dir_all(minecraft.join(sub)).unwrap();
        }
        fs::write(minecraft.join("mods/jei.jar"), JAR_BYTES).unwrap();
        fs::write(minecraft.join("mods/manual.jar"), b"manual-mod").unwrap();
        fs::write(minecraft.join("config/jei.json"), b"{}").unwrap();
        fs::write(minecraft.join("options.txt"), b"options-content").unwrap();
        fs::write(minecraft.join("resourcepacks/faithful.zip"), b"pack").unwrap();
        fs::write(minecraft.join("shaderpacks/seus.zip"), b"shader").unwrap();
        fs::write(minecraft.join("screenshots/shot.png"), b"png").unwrap();
        fs::write(minecraft.join("servers.dat"), b"servers").unwrap();
        let fixture = Self {
            directory,
            _database_path: database_path,
            instance_id,
            instance_root,
            service,
        };
        fixture.insert_instance("instance-id", "fabric", Some("0.19.3"));
        fixture.insert_content();
        fixture
    }

    fn insert_instance(&self, id: &str, loader_kind: &str, loader_version: Option<&str>) {
        let root = if id == self.instance_id {
            self.instance_root.clone()
        } else {
            let root = self.instance_root.parent().unwrap().join(id);
            fs::create_dir_all(root.join(".minecraft")).unwrap();
            root
        };
        Connection::open(&self._database_path)
            .unwrap()
            .execute(
                "
                INSERT INTO instances (
                    id, name, game_version, loader_kind, loader_version,
                    root_directory, state, created_at_unix_seconds
                ) VALUES (?1, '导出测试', '26.2', ?2, ?3, ?4, 'ready', 1)
                ",
                params![id, loader_kind, loader_version, root.to_string_lossy()],
            )
            .unwrap();
    }

    fn insert_content(&self) {
        Connection::open(&self._database_path)
            .unwrap()
            .execute(
                "
                INSERT INTO installed_content (
                    id, instance_id, provider, project_id, version_id,
                    project_title, version_number, file_name, relative_path,
                    size, sha1, sha512, enabled, auto_update_enabled,
                    installed_at_unix_seconds
                ) VALUES (
                    'content-1', ?1, 'modrinth', 'AABBCCDD', 'VVVVVVVV',
                    'JEI', '1.0.0', 'jei.jar', '.minecraft/mods/jei.jar',
                    ?2, ?3, ?4, 1, 0, 1
                )
                ",
                params![
                    self.instance_id,
                    JAR_BYTES.len() as i64,
                    encode_hex(Sha1::digest(JAR_BYTES)),
                    encode_hex(Sha512::digest(JAR_BYTES)),
                ],
            )
            .unwrap();
    }

    fn options(&self) -> ExportModpackOptions {
        ExportModpackOptions {
            name: "tundra".to_owned(),
            version: "1.0.0".to_owned(),
            include_config: true,
            include_resource_packs: true,
            include_shaders: true,
            include_servers: false,
            include_screenshots: false,
        }
    }

    fn destination(&self, file_name: &str) -> PathBuf {
        let directory = self.directory.path().join("exports");
        fs::create_dir_all(&directory).unwrap();
        directory.join(file_name)
    }
}

fn read_index(destination: &Path) -> serde_json::Value {
    let file = fs::File::open(destination).unwrap();
    let mut archive = ZipArchive::new(file).unwrap();
    let mut entry = archive.by_name("modrinth.index.json").unwrap();
    let mut buffer = Vec::new();
    std::io::Read::read_to_end(&mut entry, &mut buffer).unwrap();
    serde_json::from_slice(&buffer).unwrap()
}

fn zip_entry_names(destination: &Path) -> Vec<String> {
    let file = fs::File::open(destination).unwrap();
    let mut archive = ZipArchive::new(file).unwrap();
    (0..archive.len())
        .map(|index| archive.by_index(index).unwrap().name().to_owned())
        .collect()
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
