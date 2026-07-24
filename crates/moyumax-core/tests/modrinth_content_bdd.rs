use std::{
    collections::HashMap,
    io::{BufRead, BufReader, Write},
    net::{TcpListener, TcpStream},
    sync::{Arc, Mutex},
    thread,
};

use moyumax_core::{
    ContentDependencyKind, ManagedInstanceSummary, ModrinthClient, ModrinthProjectType,
    ModrinthSearchIndex, ModrinthSearchQuery,
};

#[tokio::test]
async fn m5_search_001_filters_for_the_target_client_instance() {
    let server = FixtureApi::new(
        HashMap::from([(
            "/v2/search".to_owned(),
            serde_json::json!({
                "hits": [{
                    "project_id": "ROOT0001",
                    "slug": "continuity",
                    "title": "Continuity",
                    "description": "Connected textures",
                    "downloads": 42,
                    "client_side": "required",
                    "server_side": "optional"
                }],
                "offset": 0,
                "limit": 20,
                "total_hits": 1
            })
            .to_string(),
        )]),
        1,
    );
    let client = ModrinthClient::with_base_url(&server.base_url()).unwrap();

    let page = client
        .search_mods(&ModrinthSearchQuery {
            query: "continuity".to_owned(),
            game_version: "26.2".to_owned(),
            loader: "fabric".to_owned(),
            index: ModrinthSearchIndex::Relevance,
            offset: 0,
            limit: 20,
            project_type: ModrinthProjectType::Mod,
        })
        .await
        .unwrap();

    assert_eq!(page.total_hits, 1);
    assert_eq!(page.hits[0].project_id, "ROOT0001");
    let request = server.requests().remove(0);
    assert!(request.contains("GET /v2/search?"));
    assert!(request.contains("facets="));
    assert!(request.contains("project_type%3Amod"));
    assert!(request.contains("versions%3A26.2"));
    assert!(request.contains("categories%3Afabric"));
    assert!(request.contains("client_side%3Arequired"));
    assert!(
        request
            .to_ascii_lowercase()
            .contains("user-agent: sakurared/moyumax/")
    );
}

#[tokio::test]
async fn m5_dependency_001_required_closure_is_deduplicated_and_optional_waits() {
    let root_version = version(
        "ROOTVER1",
        "ROOT0001",
        "Continuity 3.0.1",
        "continuity.jar",
        vec![
            dependency(Some("DEP00001"), None, "required"),
            dependency(Some("OPT00001"), None, "optional"),
        ],
    );
    let dependency_version = version(
        "DEPVER01",
        "DEP00001",
        "Fabric API 1.0",
        "fabric-api.jar",
        vec![dependency(Some("ROOT0001"), None, "required")],
    );
    let server = FixtureApi::new(
        HashMap::from([
            (
                "/v2/project/ROOT0001".to_owned(),
                project("ROOT0001", "Continuity"),
            ),
            (
                "/v2/project/ROOT0001/version".to_owned(),
                serde_json::json!([root_version]).to_string(),
            ),
            (
                "/v2/project/DEP00001".to_owned(),
                project("DEP00001", "Fabric API"),
            ),
            (
                "/v2/project/DEP00001/version".to_owned(),
                serde_json::json!([dependency_version]).to_string(),
            ),
            (
                "/v2/project/OPT00001".to_owned(),
                project("OPT00001", "Mod Menu"),
            ),
        ]),
        5,
    );
    let client = ModrinthClient::with_base_url(&server.base_url()).unwrap();
    let instance = instance();

    let plan = client
        .resolve_mod_install_plan(&instance, "ROOT0001", &[])
        .await
        .unwrap();

    assert_eq!(plan.entries.len(), 2);
    assert_eq!(plan.entries[0].project_id, "DEP00001");
    assert_eq!(plan.entries[1].project_id, "ROOT0001");
    assert_eq!(plan.optional_dependencies.len(), 1);
    assert_eq!(
        plan.optional_dependencies[0].project_id.as_deref(),
        Some("OPT00001")
    );
    assert_eq!(
        plan.optional_dependencies[0].kind,
        ContentDependencyKind::Optional
    );
    assert!(
        plan.entries
            .iter()
            .all(|entry| entry.file.sha1.len() == 40 && entry.file.sha512.len() == 128)
    );
}

#[tokio::test]
async fn m5_dependency_001_only_explicitly_selected_optional_dependency_enters_entries() {
    let root_version = version(
        "ROOTVER1",
        "ROOT0001",
        "Optional root",
        "root.jar",
        vec![dependency(Some("OPT00001"), None, "optional")],
    );
    let optional_version = version(
        "OPTVER01",
        "OPT00001",
        "Mod Menu",
        "modmenu.jar",
        Vec::new(),
    );
    let server = FixtureApi::new(
        HashMap::from([
            (
                "/v2/project/ROOT0001".to_owned(),
                project("ROOT0001", "Optional root"),
            ),
            (
                "/v2/project/ROOT0001/version".to_owned(),
                serde_json::json!([root_version]).to_string(),
            ),
            (
                "/v2/project/OPT00001".to_owned(),
                project("OPT00001", "Mod Menu"),
            ),
            (
                "/v2/project/OPT00001/version".to_owned(),
                serde_json::json!([optional_version]).to_string(),
            ),
        ]),
        5,
    );

    let plan = ModrinthClient::with_base_url(&server.base_url())
        .unwrap()
        .resolve_mod_install_plan(&instance(), "ROOT0001", &["OPT00001".to_owned()])
        .await
        .unwrap();

    assert_eq!(
        plan.entries
            .iter()
            .map(|entry| entry.project_id.as_str())
            .collect::<Vec<_>>(),
        vec!["OPT00001", "ROOT0001"]
    );
    assert_eq!(plan.optional_dependencies.len(), 1);
}

#[tokio::test]
async fn m5_dependency_002_rejects_an_explicit_incompatible_version() {
    let root_version = version(
        "ROOTVER1",
        "ROOT0001",
        "Continuity 3.0.1",
        "continuity.jar",
        vec![dependency(Some("DEP00001"), Some("BADVER01"), "required")],
    );
    let mut incompatible = version(
        "BADVER01",
        "DEP00001",
        "Fabric API old",
        "fabric-api.jar",
        Vec::new(),
    );
    incompatible["game_versions"] = serde_json::json!(["1.21.8"]);
    let server = FixtureApi::new(
        HashMap::from([
            (
                "/v2/project/ROOT0001".to_owned(),
                project("ROOT0001", "Continuity"),
            ),
            (
                "/v2/project/ROOT0001/version".to_owned(),
                serde_json::json!([root_version]).to_string(),
            ),
            (
                "/v2/project/DEP00001".to_owned(),
                project("DEP00001", "Fabric API"),
            ),
            ("/v2/version/BADVER01".to_owned(), incompatible.to_string()),
        ]),
        4,
    );

    let error = ModrinthClient::with_base_url(&server.base_url())
        .unwrap()
        .resolve_mod_install_plan(&instance(), "ROOT0001", &[])
        .await
        .unwrap_err();

    assert!(error.to_string().contains("BADVER01"));
    assert!(error.to_string().contains("不兼容"));
}

#[tokio::test]
async fn m5_dependency_003_rejects_a_required_file_name_without_ids() {
    let root_version = version(
        "ROOTVER1",
        "ROOT0001",
        "Broken dependency",
        "root.jar",
        vec![serde_json::json!({
            "project_id": null,
            "version_id": null,
            "file_name": "unknown-library.jar",
            "dependency_type": "required"
        })],
    );
    let server = FixtureApi::new(
        HashMap::from([
            (
                "/v2/project/ROOT0001".to_owned(),
                project("ROOT0001", "Broken dependency"),
            ),
            (
                "/v2/project/ROOT0001/version".to_owned(),
                serde_json::json!([root_version]).to_string(),
            ),
        ]),
        2,
    );

    let error = ModrinthClient::with_base_url(&server.base_url())
        .unwrap()
        .resolve_mod_install_plan(&instance(), "ROOT0001", &[])
        .await
        .unwrap_err();

    assert!(error.to_string().contains("unknown-library.jar"));
    assert!(error.to_string().contains("无法安全解析"));
}

#[tokio::test]
async fn m5_dependency_002_rejects_conflicting_versions_of_one_project() {
    let root_version = version(
        "ROOTVER1",
        "ROOT0001",
        "Conflict root",
        "root.jar",
        vec![
            dependency(Some("DEP00001"), Some("DEPVER01"), "required"),
            dependency(Some("DEP00001"), Some("DEPVER02"), "required"),
        ],
    );
    let first = version(
        "DEPVER01",
        "DEP00001",
        "Dependency one",
        "dependency-one.jar",
        Vec::new(),
    );
    let second = version(
        "DEPVER02",
        "DEP00001",
        "Dependency two",
        "dependency-two.jar",
        Vec::new(),
    );
    let server = FixtureApi::new(
        HashMap::from([
            (
                "/v2/project/ROOT0001".to_owned(),
                project("ROOT0001", "Conflict root"),
            ),
            (
                "/v2/project/ROOT0001/version".to_owned(),
                serde_json::json!([root_version]).to_string(),
            ),
            (
                "/v2/project/DEP00001".to_owned(),
                project("DEP00001", "Dependency"),
            ),
            ("/v2/version/DEPVER01".to_owned(), first.to_string()),
            ("/v2/version/DEPVER02".to_owned(), second.to_string()),
        ]),
        4,
    );

    let error = ModrinthClient::with_base_url(&server.base_url())
        .unwrap()
        .resolve_mod_install_plan(&instance(), "ROOT0001", &[])
        .await
        .unwrap_err();

    assert!(error.to_string().contains("DEP00001"));
    assert!(error.to_string().contains("两个不同版本"));
}

#[tokio::test]
async fn m5_dependency_002_prefers_release_and_never_selects_sources() {
    let mut beta = version(
        "BETAVER1",
        "ROOT0001",
        "New beta",
        "new-beta.jar",
        Vec::new(),
    );
    beta["version_type"] = serde_json::json!("beta");
    beta["date_published"] = serde_json::json!("2026-07-22T00:00:00Z");
    let mut release = version(
        "RELVER01",
        "ROOT0001",
        "Stable release",
        "stable.jar",
        Vec::new(),
    );
    release["date_published"] = serde_json::json!("2026-07-01T00:00:00Z");
    release["files"] = serde_json::json!([
        {
            "hashes": hashes("3", "4"),
            "url": "https://cdn.modrinth.com/stable-sources.jar",
            "filename": "stable-sources.jar",
            "primary": true,
            "size": 512,
            "file_type": "sources-jar"
        },
        {
            "hashes": hashes("1", "2"),
            "url": "https://cdn.modrinth.com/stable.jar",
            "filename": "stable.jar",
            "primary": false,
            "size": 1024,
            "file_type": null
        }
    ]);
    let server = FixtureApi::new(
        HashMap::from([
            (
                "/v2/project/ROOT0001".to_owned(),
                project("ROOT0001", "Stable"),
            ),
            (
                "/v2/project/ROOT0001/version".to_owned(),
                serde_json::json!([beta, release]).to_string(),
            ),
        ]),
        2,
    );

    let plan = ModrinthClient::with_base_url(&server.base_url())
        .unwrap()
        .resolve_mod_install_plan(&instance(), "ROOT0001", &[])
        .await
        .unwrap();

    assert_eq!(plan.entries[0].version_id, "RELVER01");
    assert_eq!(plan.entries[0].file.filename, "stable.jar");
}

#[tokio::test]
async fn m5_search_001_reports_provider_retirement_rate_limit_and_server_errors() {
    for (status, expected) in [
        (410, "已退役"),
        (429, "达到限流"),
        (503, "HTTP 503 Service Unavailable"),
    ] {
        let server = FixtureApi::with_responses(
            HashMap::from([("/v2/search".to_owned(), FixtureResponse::new(status, "{}"))]),
            1,
        );
        let error = ModrinthClient::with_base_url(&server.base_url())
            .unwrap()
            .search_mods(&search_query())
            .await
            .unwrap_err();
        assert!(
            error.to_string().contains(expected),
            "status {status} returned unexpected error: {error}"
        );
    }
}

fn instance() -> ManagedInstanceSummary {
    ManagedInstanceSummary {
        id: "instance-id".to_owned(),
        name: "内容测试".to_owned(),
        game_version: "26.2".to_owned(),
        loader_kind: "fabric".to_owned(),
        loader_version: Some("0.19.3".to_owned()),
        root_directory: "D:\\MoyuMax\\instances\\instance-id".to_owned(),
        state: "ready".to_owned(),
    }
}

fn project(id: &str, title: &str) -> String {
    serde_json::json!({
        "id": id,
        "slug": title.to_ascii_lowercase().replace(' ', "-"),
        "title": title,
        "project_type": "mod",
        "client_side": "required",
        "server_side": "optional"
    })
    .to_string()
}

fn dependency(
    project_id: Option<&str>,
    version_id: Option<&str>,
    dependency_type: &str,
) -> serde_json::Value {
    serde_json::json!({
        "project_id": project_id,
        "version_id": version_id,
        "file_name": null,
        "dependency_type": dependency_type
    })
}

fn search_query() -> ModrinthSearchQuery {
    ModrinthSearchQuery {
        query: "continuity".to_owned(),
        game_version: "26.2".to_owned(),
        loader: "fabric".to_owned(),
        index: ModrinthSearchIndex::Relevance,
        offset: 0,
        limit: 20,
        project_type: ModrinthProjectType::Mod,
    }
}

fn hashes(sha1_digit: &str, sha512_digit: &str) -> serde_json::Value {
    serde_json::json!({
        "sha1": sha1_digit.repeat(40),
        "sha512": sha512_digit.repeat(128)
    })
}

fn version(
    id: &str,
    project_id: &str,
    name: &str,
    filename: &str,
    dependencies: Vec<serde_json::Value>,
) -> serde_json::Value {
    serde_json::json!({
        "id": id,
        "project_id": project_id,
        "name": name,
        "version_number": "1.0.0",
        "game_versions": ["26.2"],
        "loaders": ["fabric"],
        "version_type": "release",
        "status": "listed",
        "date_published": "2026-07-20T00:00:00Z",
        "dependencies": dependencies,
        "files": [{
            "hashes": {
                "sha1": "1111111111111111111111111111111111111111",
                "sha512": "22222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222"
            },
            "url": format!("https://cdn.modrinth.com/{filename}"),
            "filename": filename,
            "primary": true,
            "size": 1024,
            "file_type": null
        }]
    })
}

struct FixtureApi {
    address: std::net::SocketAddr,
    requests: Arc<Mutex<Vec<String>>>,
    _thread: thread::JoinHandle<()>,
}

#[derive(Debug)]
struct FixtureResponse {
    status: u16,
    reason: &'static str,
    body: String,
}

impl FixtureResponse {
    fn new(status: u16, body: &str) -> Self {
        let reason = match status {
            200 => "OK",
            410 => "Gone",
            429 => "Too Many Requests",
            503 => "Service Unavailable",
            _ => "Fixture Status",
        };
        Self {
            status,
            reason,
            body: body.to_owned(),
        }
    }
}

impl FixtureApi {
    fn new(responses: HashMap<String, String>, request_count: usize) -> Self {
        Self::with_responses(
            responses
                .into_iter()
                .map(|(route, body)| (route, FixtureResponse::new(200, &body)))
                .collect(),
            request_count,
        )
    }

    fn with_responses(responses: HashMap<String, FixtureResponse>, request_count: usize) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let captured = Arc::clone(&requests);
        let server_thread = thread::spawn(move || {
            for _ in 0..request_count {
                let (stream, _) = listener.accept().unwrap();
                serve(stream, &responses, &captured);
            }
        });
        Self {
            address,
            requests,
            _thread: server_thread,
        }
    }

    fn base_url(&self) -> String {
        format!("http://{}/v2/", self.address)
    }

    fn requests(&self) -> Vec<String> {
        self.requests.lock().unwrap().clone()
    }
}

fn serve(
    mut stream: TcpStream,
    responses: &HashMap<String, FixtureResponse>,
    requests: &Arc<Mutex<Vec<String>>>,
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
        .unwrap();
    let route = path.split('?').next().unwrap();
    let response = responses
        .get(route)
        .unwrap_or_else(|| panic!("unexpected route: {route}"));
    requests.lock().unwrap().push(request);
    write!(
        stream,
        "HTTP/1.1 {} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        response.status,
        response.reason,
        response.body.len(),
        response.body
    )
    .unwrap();
    stream.flush().unwrap();
}
