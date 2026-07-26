use std::{
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    sync::{Arc, Mutex},
    thread,
};

use moyumax_core::{ModrinthClient, ModrinthProjectType, ModrinthSearchIndex, ModrinthSearchQuery};
use sha1::{Digest, Sha1};
use tempfile::TempDir;

fn search_query(project_type: ModrinthProjectType) -> ModrinthSearchQuery {
    ModrinthSearchQuery {
        query: "tundra".to_owned(),
        game_version: if project_type == ModrinthProjectType::Mod {
            "26.2".to_owned()
        } else {
            String::new()
        },
        loader: if project_type == ModrinthProjectType::Mod {
            "fabric".to_owned()
        } else {
            String::new()
        },
        index: ModrinthSearchIndex::Relevance,
        offset: 0,
        limit: 20,
        project_type,
        category: String::new(),
    }
}

#[tokio::test]
async fn catalog_search_filters_facets_by_project_type() {
    let requests = Arc::new(Mutex::new(Vec::<String>::new()));
    let requests_seen = Arc::clone(&requests);
    let (base_url, _server) = spawn_api(
        move |_method, path, _body| {
            if path.starts_with("/search") {
                requests_seen.lock().unwrap().push(path.to_owned());
            }
            (
                200,
                serde_json::json!({
                    "hits": [], "offset": 0, "limit": 20, "total_hits": 0
                })
                .to_string(),
            )
        },
        None,
    );
    let client = ModrinthClient::with_base_url(&base_url).unwrap();

    client
        .search_mods(&search_query(ModrinthProjectType::Mod))
        .await
        .unwrap();
    client
        .search_projects(&search_query(ModrinthProjectType::Modpack))
        .await
        .unwrap();
    client
        .search_projects(&search_query(ModrinthProjectType::Shader))
        .await
        .unwrap();

    let recorded = requests.lock().unwrap().clone();
    assert_eq!(recorded.len(), 3);
    let mod_facets = decode_facets(&recorded[0]);
    assert!(mod_facets.contains("project_type:mod"), "{mod_facets}");
    assert!(mod_facets.contains("versions:26.2"), "{mod_facets}");
    assert!(mod_facets.contains("categories:fabric"), "{mod_facets}");
    let pack_facets = decode_facets(&recorded[1]);
    assert!(
        pack_facets.contains("project_type:modpack"),
        "{pack_facets}"
    );
    assert!(!pack_facets.contains("categories"), "{pack_facets}");
    let shader_facets = decode_facets(&recorded[2]);
    assert!(
        shader_facets.contains("project_type:shader"),
        "{shader_facets}"
    );
}

#[tokio::test]
async fn catalog_latest_file_prefers_release_and_validates_hashes() {
    let (base_url, _server) = spawn_api(
        |method, path, _body| {
            assert_eq!(method, "GET");
            assert!(path.starts_with("/project/ABCDEFGH/version"));
            (
                200,
                serde_json::json!([
                    {
                        "id": "beta1", "project_id": "ABCDEFGH", "version_number": "2.0.0-beta",
                        "game_versions": ["26.2"], "loaders": ["fabric"],
                        "version_type": "beta", "status": "listed",
                        "date_published": "2026-07-20T00:00:00Z", "dependencies": [],
                        "files": [{
                            "hashes": {"sha1": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb", "sha512": "b".repeat(128)},
                            "url": "https://cdn.example.com/beta.mrpack",
                            "filename": "beta.mrpack", "primary": true, "size": 10
                        }]
                    },
                    {
                        "id": "rel1", "project_id": "ABCDEFGH", "version_number": "1.0.0",
                        "game_versions": ["26.2"], "loaders": ["fabric"],
                        "version_type": "release", "status": "listed",
                        "date_published": "2026-07-10T00:00:00Z", "dependencies": [],
                        "files": [{
                            "hashes": {"sha1": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "sha512": "a".repeat(128)},
                            "url": "https://cdn.example.com/release.mrpack",
                            "filename": "release.mrpack", "primary": true, "size": 10
                        }]
                    }
                ])
                .to_string(),
            )
        },
        None,
    );
    let client = ModrinthClient::with_base_url(&base_url).unwrap();

    let file = client
        .latest_project_file("ABCDEFGH", None, None)
        .await
        .unwrap();

    assert_eq!(
        file.filename, "release.mrpack",
        "必须优先正式版而非时间最新"
    );
}

#[tokio::test]
async fn catalog_project_version_file_returns_selected_version_primary_file() {
    let (base_url, _server) = spawn_api(
        |method, path, _body| {
            assert_eq!(method, "GET");
            assert_eq!(path, "/version/VER12345");
            (
                200,
                serde_json::json!({
                    "id": "VER12345", "project_id": "P7dR8mSH", "version_number": "0.91.0+1.21.8",
                    "game_versions": ["1.21.8"], "loaders": ["fabric", "quilt"],
                    "version_type": "release", "status": "listed",
                    "date_published": "2026-07-01T00:00:00Z", "dependencies": [],
                    "files": [{
                        "hashes": {"sha1": "cccccccccccccccccccccccccccccccccccccccc", "sha512": "c".repeat(128)},
                        "url": "https://cdn.example.com/fabric-api.jar",
                        "filename": "fabric-api.jar", "primary": true, "size": 42
                    }]
                })
                .to_string(),
            )
        },
        None,
    );
    let client = ModrinthClient::with_base_url(&base_url).unwrap();

    let file = client.project_version_file("VER12345").await.unwrap();

    assert_eq!(file.filename, "fabric-api.jar");
    assert_eq!(file.size, 42);
    assert_eq!(file.sha1, "c".repeat(40));
}

#[tokio::test]
async fn catalog_download_verifies_sha1_before_commit() {
    let payload = b"modpack-bytes".to_vec();
    let good_sha1 = hex_encode(&Sha1::digest(&payload));
    let (base_url, _server) = spawn_api(
        move |_method, path, _body| {
            assert!(path.starts_with("/files/"));
            (200, String::from_utf8(payload.clone()).unwrap())
        },
        Some("application/octet-stream"),
    );
    let client = ModrinthClient::with_base_url(&base_url).unwrap();
    let directory = TempDir::new().unwrap();

    let file = moyumax_core::ModrinthVersionFile {
        url: format!("{base_url}files/pack.mrpack"),
        filename: "pack.mrpack".to_owned(),
        sha1: good_sha1,
        sha512: "a".repeat(128),
        size: 13,
    };
    let path = client
        .download_project_file(&file, directory.path())
        .await
        .unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"modpack-bytes");
    assert_eq!(path.file_name().unwrap(), "pack.mrpack");
    assert!(
        !directory.path().join("pack.mrpack.part").exists()
            && std::fs::read_dir(directory.path()).unwrap().count() == 1,
        "成功后不得残留临时文件"
    );

    let tampered = moyumax_core::ModrinthVersionFile {
        sha1: "0".repeat(40),
        ..file
    };
    let error = client
        .download_project_file(&tampered, directory.path())
        .await
        .expect_err("SHA-1 不符必须拒绝");
    assert!(error.to_string().contains("SHA-1"), "{error}");
    assert!(
        std::fs::read_dir(directory.path()).unwrap().count() == 1,
        "失败后不得留下临时文件"
    );
}

#[tokio::test]
async fn catalog_download_decodes_gzip_encoded_responses() {
    // CDN 以 gzip 内容编码返回时，客户端必须解码后再校验哈希。
    let gzipped: Vec<u8> = vec![
        31, 139, 8, 0, 201, 31, 99, 106, 2, 255, 203, 205, 79, 41, 72, 76, 206, 214, 77, 170, 44,
        73, 45, 6, 0, 98, 231, 109, 90, 13, 0, 0, 0,
    ];
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { break };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let _ = read_request(&mut reader);
            let mut stream = stream;
            stream
                .write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Encoding: gzip\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        gzipped.len()
                    )
                    .as_bytes(),
                )
                .unwrap();
            stream.write_all(&gzipped).unwrap();
            stream.flush().unwrap();
        }
    });
    let client = ModrinthClient::with_base_url(&format!("http://{address}/")).unwrap();
    let directory = TempDir::new().unwrap();
    let file = moyumax_core::ModrinthVersionFile {
        url: format!("http://{address}/files/pack.mrpack"),
        filename: "pack.mrpack".to_owned(),
        sha1: hex_encode(&Sha1::digest(b"modpack-bytes")),
        sha512: "a".repeat(128),
        size: 13,
    };

    let path = client
        .download_project_file(&file, directory.path())
        .await
        .expect("gzip 编码响应必须解码下载");

    assert_eq!(std::fs::read(&path).unwrap(), b"modpack-bytes");
    drop(server);
}

fn decode_facets(path: &str) -> String {
    let query = path.split('?').nth(1).unwrap_or("");
    query
        .split('&')
        .find_map(|pair| pair.strip_prefix("facets="))
        .map(|value| {
            value
                .replace("%22", "\"")
                .replace("%5B", "[")
                .replace("%5D", "]")
                .replace("%3A", ":")
                .replace("%2C", ",")
        })
        .unwrap_or_default()
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn spawn_api(
    handler: impl Fn(&str, &str, &str) -> (u16, String) + Send + Sync + 'static,
    content_type: Option<&'static str>,
) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let handler = Arc::new(handler);
    let content_type = content_type.unwrap_or("application/json");
    let server = thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { break };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let (method, path, body) = read_request(&mut reader);
            let (status, response_body) = handler(&method, &path, &body);
            let reason = if status == 200 { "OK" } else { "Error" };
            let mut stream = stream;
            write!(
                stream,
                "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response_body}",
                response_body.len()
            )
            .unwrap();
            stream.flush().unwrap();
        }
    });
    (format!("http://{address}/"), server)
}

fn read_request(reader: &mut BufReader<TcpStream>) -> (String, String, String) {
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).unwrap_or(0) == 0 {
        return (String::new(), String::new(), String::new());
    }
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_owned();
    let path = parts.next().unwrap_or_default().to_owned();
    let mut content_length = 0_usize;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
            break;
        }
        if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
            content_length = value.trim().parse().unwrap_or(0);
        }
    }
    let mut body = vec![0_u8; content_length];
    reader.read_exact(&mut body).ok();
    (method, path, String::from_utf8_lossy(&body).into_owned())
}
