//! CurseForge 官方 API 客户端 BDD 测试：搜索归一化、文件归一化、
//! download-url 兜底、无 Key 报错与设置持久化，全部走本地 mock server。

use std::{
    collections::HashMap,
    io::{BufRead, BufReader, Write},
    net::{TcpListener, TcpStream},
    sync::{Arc, Mutex},
    thread,
};

use moyumax_core::{
    AppService, CURSEFORGE_CLASS_MOD, CatalogProjectSource, CurseForgeClient,
    CurseforgeFileSummary, CurseforgeSearchQuery, CurseforgeSortField, CurseforgeSortOrder,
};
use sha1::{Digest, Sha1};
use tempfile::TempDir;

const TEST_KEY: &str = "fixture-key";

fn search_query() -> CurseforgeSearchQuery {
    CurseforgeSearchQuery {
        query: "sodium".to_owned(),
        class_id: CURSEFORGE_CLASS_MOD,
        game_version: Some("1.21.1".to_owned()),
        category_id: None,
        mod_loader: Some("fabric".to_owned()),
        sort_field: CurseforgeSortField::Popularity,
        sort_order: CurseforgeSortOrder::Desc,
        index: 0,
        page_size: 20,
    }
}

fn search_body() -> String {
    serde_json::json!({
        "data": [{
            "id": 360438,
            "gameId": 432,
            "name": "Sodium",
            "slug": "sodium",
            "summary": "A modern rendering engine",
            "downloadCount": 40123456,
            "dateModified": "2026-06-18T10:00:00Z",
            "logo": {"thumbnailUrl": "https://media.forgecdn.net/avatars/1.png", "url": null},
            "authors": [{"id": 1, "name": "jellysquid3", "url": "https://www.curseforge.com/members/1"}],
            "categories": [{"id": 421, "name": "Cosmetic", "slug": "cosmetic", "isClass": false}],
            "latestFilesIndexes": [
                {"gameVersion": "1.21.1", "fileId": 5500001, "filename": "sodium.jar"},
                {"gameVersion": "1.21.1", "fileId": 5500002, "filename": "sodium2.jar"},
                {"gameVersion": "1.20.1", "fileId": 5400001, "filename": "sodium-old.jar"}
            ]
        }],
        "pagination": {"index": 0, "pageSize": 20, "resultCount": 1, "totalCount": 1}
    })
    .to_string()
}

fn file_json(
    id: u64,
    display_name: &str,
    file_name: &str,
    release_type: u8,
    download_url: Option<&str>,
    game_versions: &[&str],
) -> serde_json::Value {
    serde_json::json!({
        "id": id,
        "displayName": display_name,
        "fileName": file_name,
        "releaseType": release_type,
        "fileDate": "2026-06-18T10:00:00Z",
        "fileLength": 123456,
        "downloadCount": 7890,
        "downloadUrl": download_url,
        "gameVersions": game_versions,
        "hashes": [
            {"algo": 1, "value": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"},
            {"algo": 2, "value": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"}
        ]
    })
}

#[tokio::test]
async fn cf_search_001_normalizes_hits_and_sends_key_and_filters() {
    let server = FixtureApi::new(vec![("/v1/mods/search", search_body(), 200)], 1);
    let client = CurseForgeClient::with_base_url(&server.base_url(), Some(TEST_KEY.to_owned()))
        .expect("客户端构造失败");

    let page = client.search(&search_query()).await.expect("搜索失败");

    assert_eq!(page.total_count, 1);
    let hit = &page.hits[0];
    assert_eq!(hit.project_id, "360438");
    assert_eq!(hit.title, "Sodium");
    assert_eq!(hit.slug, "sodium");
    assert_eq!(hit.author.as_deref(), Some("jellysquid3"));
    assert_eq!(hit.description, "A modern rendering engine");
    assert_eq!(
        hit.icon_url.as_deref(),
        Some("https://media.forgecdn.net/avatars/1.png")
    );
    assert_eq!(hit.downloads, 40123456);
    assert_eq!(hit.date_modified.as_deref(), Some("2026-06-18T10:00:00Z"));
    assert_eq!(hit.game_versions, vec!["1.21.1", "1.20.1"]);
    assert_eq!(hit.categories, vec!["Cosmetic"]);
    assert_eq!(hit.source, CatalogProjectSource::Curseforge);

    let request = server.requests().remove(0);
    let request_line = request.lines().next().unwrap_or_default();
    assert!(
        request_line.starts_with("GET /v1/mods/search?"),
        "{request_line}"
    );
    for expected in [
        "gameId=432",
        "classId=6",
        "searchFilter=sodium",
        "gameVersion=1.21.1",
        "modLoaderType=4",
        "sortField=2",
        "sortOrder=desc",
        "index=0",
        "pageSize=20",
    ] {
        assert!(
            request_line.contains(expected),
            "缺少 {expected}：{request_line}"
        );
    }
    assert!(
        request
            .to_ascii_lowercase()
            .contains("x-api-key: fixture-key"),
        "必须携带 x-api-key：{request}"
    );
}

#[tokio::test]
async fn cf_files_001_paginates_and_normalizes_versions_loaders_and_hashes() {
    let first_page = serde_json::json!({
        "data": [file_json(
            5500001,
            "Sodium 0.6.0",
            "sodium-fabric-0.6.0.jar",
            1,
            Some("https://edge.forgecdn.net/files/5500/1/sodium-fabric-0.6.0.jar"),
            &["1.21.1", "Fabric", "Java 21", "1.20.1", "Forge"]
        )],
        "pagination": {"index": 0, "pageSize": 50, "resultCount": 1, "totalCount": 2}
    })
    .to_string();
    let second_page = serde_json::json!({
        "data": [file_json(
            5400009,
            "",
            "sodium-fabric-0.5.9-beta.jar",
            2,
            None,
            &["1.20.1", "Quilt"]
        )],
        "pagination": {"index": 50, "pageSize": 50, "resultCount": 1, "totalCount": 2}
    })
    .to_string();
    let server = FixtureApi::new(
        vec![
            ("/v1/mods/360438/files", first_page, 200),
            ("/v1/mods/360438/files", second_page, 200),
        ],
        2,
    );
    let client = CurseForgeClient::with_base_url(&server.base_url(), Some(TEST_KEY.to_owned()))
        .expect("客户端构造失败");

    let files = client
        .project_files("360438", None, None)
        .await
        .expect("文件列表失败");

    assert_eq!(files.len(), 2, "分页必须取全");
    let release = &files[0];
    assert_eq!(release.id, "5500001");
    assert_eq!(release.version_number, "Sodium 0.6.0");
    assert_eq!(release.version_type, "release");
    assert_eq!(release.game_versions, vec!["1.21.1", "1.20.1"]);
    assert_eq!(release.loaders, vec!["fabric", "forge"]);
    assert_eq!(
        release.sha1.as_deref(),
        Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
    );
    assert_eq!(release.size, 123456);
    assert_eq!(
        release.download_url.as_deref(),
        Some("https://edge.forgecdn.net/files/5500/1/sodium-fabric-0.6.0.jar")
    );
    let beta = &files[1];
    assert_eq!(
        beta.version_number, "sodium-fabric-0.5.9-beta.jar",
        "displayName 为空回退 fileName"
    );
    assert_eq!(beta.version_type, "beta");
    assert_eq!(beta.loaders, vec!["quilt"]);
    assert!(
        beta.download_url.is_none(),
        "downloadUrl 为 null 必须如实保留"
    );
}

#[tokio::test]
async fn cf_download_url_001_direct_then_falls_back_to_edge_on_empty_body() {
    // 第一次：download-url 直接返回地址。
    let direct = FixtureApi::new(
        vec![(
            "/v1/mods/360438/files/5500001/download-url",
            serde_json::json!({"data": "https://edge.forgecdn.net/files/5500/1/sodium.jar"})
                .to_string(),
            200,
        )],
        1,
    );
    let client = CurseForgeClient::with_base_url(&direct.base_url(), Some(TEST_KEY.to_owned()))
        .expect("客户端构造失败");
    let url = client
        .file_download_url("360438", "5500001")
        .await
        .expect("download-url 失败");
    assert_eq!(url, "https://edge.forgecdn.net/files/5500/1/sodium.jar");

    // 第二次：download-url 返回 204 空体，走文件元数据 + edge 兜底。
    let fallback = FixtureApi::with_responses(
        vec![
            (
                "/v1/mods/360438/files/5500003/download-url",
                FixtureResponse::new(204, ""),
            ),
            (
                "/v1/mods/360438/files/5500003",
                FixtureResponse::new(
                    200,
                    &serde_json::json!({
                        "data": file_json(
                            5500003,
                            "Sodium 0.6.1",
                            "sodium-0.6.1.jar",
                            1,
                            None,
                            &["1.21.1", "Fabric"]
                        )
                    })
                    .to_string(),
                ),
            ),
        ],
        2,
    );
    let client = CurseForgeClient::with_base_url(&fallback.base_url(), Some(TEST_KEY.to_owned()))
        .expect("客户端构造失败");
    let url = client
        .file_download_url("360438", "5500003")
        .await
        .expect("edge 兜底失败");
    assert_eq!(
        url, "https://edge.forgecdn.net/files/5500/3/sodium-0.6.1.jar",
        "downloadUrl 为空必须按 /files/{{id/1000}}/{{id%1000}}/{{fileName}} 兜底"
    );
}

#[tokio::test]
async fn cf_key_001_missing_key_fails_with_guidance_before_any_request() {
    let server = FixtureApi::new(Vec::new(), 0);
    let client = CurseForgeClient::with_base_url(&server.base_url(), None).expect("客户端构造失败");
    let error = client
        .search(&search_query())
        .await
        .expect_err("无 Key 必须报错");
    let message = error.to_string();
    assert!(message.contains("未配置 CurseForge API Key"), "{message}");
    assert!(message.contains("设置"), "{message}");
    assert!(message.contains("MCI Mirror"), "{message}");
    assert!(
        server.requests().is_empty(),
        "无 Key 不得发出任何 HTTP 请求"
    );
}

#[tokio::test]
async fn cf_key_002_invalid_key_maps_to_friendly_error() {
    let server =
        FixtureApi::with_responses(vec![("/v1/games/432", FixtureResponse::new(403, "{}"))], 1);
    let client = CurseForgeClient::with_base_url(&server.base_url(), Some(TEST_KEY.to_owned()))
        .expect("客户端构造失败");
    let error = client.verify_key().await.expect_err("403 必须报错");
    assert!(error.to_string().contains("无效或已过期"), "{error}");
}

#[test]
fn cf_key_003_setting_persists_and_clears() {
    let directory = TempDir::new().unwrap();
    let service = AppService::open(
        &directory.path().join("state.sqlite3"),
        &directory.path().join("data"),
    )
    .unwrap();
    assert_eq!(service.curseforge_api_key().unwrap(), None);
    service
        .set_curseforge_api_key("  local-fixture-key  ")
        .unwrap();
    assert_eq!(
        service.curseforge_api_key().unwrap().as_deref(),
        Some("local-fixture-key")
    );
    let reopened = AppService::open(
        &directory.path().join("state.sqlite3"),
        &directory.path().join("data"),
    )
    .unwrap();
    assert_eq!(
        reopened.curseforge_api_key().unwrap().as_deref(),
        Some("local-fixture-key"),
        "重启后必须保留"
    );
    reopened.set_curseforge_api_key("   ").unwrap();
    assert_eq!(
        reopened.curseforge_api_key().unwrap(),
        None,
        "空白输入必须清除"
    );
}

#[tokio::test]
async fn cf_download_001_verifies_sha1_then_size_when_hash_absent() {
    let payload = b"curseforge-file-bytes".to_vec();
    let digest = encode_hex(Sha1::digest(&payload));
    let server = FixtureApi::new(
        vec![(
            "/cdn/file.jar",
            String::from_utf8_lossy(&payload).into_owned(),
            200,
        )],
        3,
    );
    let client = CurseForgeClient::with_base_url(&server.base_url(), Some(TEST_KEY.to_owned()))
        .expect("客户端构造失败");
    let directory = TempDir::new().unwrap();
    let good = CurseforgeFileSummary {
        id: "5500001".to_owned(),
        version_number: "1.0.0".to_owned(),
        version_type: "release".to_owned(),
        date_published: "2026-06-18T10:00:00Z".to_owned(),
        game_versions: vec!["1.21.1".to_owned()],
        loaders: vec!["fabric".to_owned()],
        downloads: 1,
        file_name: "file.jar".to_owned(),
        size: payload.len() as u64,
        sha1: Some(digest),
        download_url: Some(format!("{}/cdn/file.jar", server.origin())),
    };
    let path = client
        .download_file(&good, directory.path())
        .await
        .expect("SHA-1 正确必须下载成功");
    assert_eq!(std::fs::read(&path).unwrap(), payload);

    let bad = CurseforgeFileSummary {
        sha1: Some("0".repeat(40)),
        ..good.clone()
    };
    let error = client
        .download_file(&bad, directory.path())
        .await
        .expect_err("SHA-1 错误必须校验失败");
    assert!(error.to_string().contains("SHA-1"), "{error}");

    let size_only = CurseforgeFileSummary {
        sha1: None,
        size: payload.len() as u64 + 1,
        ..good
    };
    let error = client
        .download_file(&size_only, directory.path())
        .await
        .expect_err("无校验值时必须按大小校验");
    assert!(error.to_string().contains("大小不一致"), "{error}");
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

struct FixtureResponse {
    status: u16,
    body: String,
}

impl FixtureResponse {
    fn new(status: u16, body: &str) -> Self {
        Self {
            status,
            body: body.to_owned(),
        }
    }
}

/// 路径路由 mock server：同一路由多次命中按注册顺序轮流返回
/// （分页与重复下载测试用），超出注册数后重复最后一个响应。
struct FixtureApi {
    address: std::net::SocketAddr,
    requests: Arc<Mutex<Vec<String>>>,
    _thread: thread::JoinHandle<()>,
}

impl FixtureApi {
    fn new(responses: Vec<(&str, String, u16)>, request_count: usize) -> Self {
        let mut routes: HashMap<String, Vec<FixtureResponse>> = HashMap::new();
        for (route, body, status) in responses {
            routes
                .entry(route.to_owned())
                .or_default()
                .push(FixtureResponse::new(status, &body));
        }
        Self::serve(routes, request_count)
    }

    fn with_responses(responses: Vec<(&str, FixtureResponse)>, request_count: usize) -> Self {
        let mut routes: HashMap<String, Vec<FixtureResponse>> = HashMap::new();
        for (route, response) in responses {
            routes.entry(route.to_owned()).or_default().push(response);
        }
        Self::serve(routes, request_count)
    }

    fn serve(routes: HashMap<String, Vec<FixtureResponse>>, request_count: usize) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let captured = Arc::clone(&requests);
        let server_thread = thread::spawn(move || {
            let mut hits: HashMap<String, usize> = HashMap::new();
            for _ in 0..request_count {
                let Ok((stream, _)) = listener.accept() else {
                    break;
                };
                handle(stream, &routes, &captured, &mut hits);
            }
        });
        Self {
            address,
            requests,
            _thread: server_thread,
        }
    }

    fn origin(&self) -> String {
        format!("http://{}", self.address)
    }

    fn base_url(&self) -> String {
        format!("http://{}/v1/", self.address)
    }

    fn requests(&self) -> Vec<String> {
        self.requests.lock().unwrap().clone()
    }
}

fn handle(
    mut stream: TcpStream,
    routes: &HashMap<String, Vec<FixtureResponse>>,
    requests: &Arc<Mutex<Vec<String>>>,
    hits: &mut HashMap<String, usize>,
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
        .unwrap_or("/");
    let route = path.split('?').next().unwrap_or(path);
    let hit = hits.entry(route.to_owned()).or_insert(0);
    let candidates = routes
        .get(route)
        .unwrap_or_else(|| panic!("unexpected route: {route}"));
    let response = candidates
        .get(*hit)
        .or_else(|| candidates.last())
        .expect("路由必须至少注册一个响应");
    *hit += 1;
    requests.lock().unwrap().push(request);
    let reason = match response.status {
        200 => "OK",
        204 => "No Content",
        403 => "Forbidden",
        429 => "Too Many Requests",
        _ => "Fixture Status",
    };
    write!(
        stream,
        "HTTP/1.1 {} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        response.status,
        reason,
        response.body.len(),
        response.body
    )
    .unwrap();
    stream.flush().unwrap();
}
