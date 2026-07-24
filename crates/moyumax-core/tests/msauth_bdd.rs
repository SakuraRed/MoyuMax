use std::{
    collections::VecDeque,
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
    time::Instant,
};

use moyumax_core::{
    AccountKind, AccountSessionState, AppService, CoreError, LaunchAccount, MicrosoftAuthClient,
    MicrosoftLoginCancel,
};
use rusqlite::{Connection, params};
use tempfile::TempDir;

const PROFILE_UUID: &str = "069a79f4-44e9-4726-a5be-fca90e38aaf5";

#[tokio::test]
async fn m30_acct_001_full_chain_stores_tokens_locally_only() {
    let server = FixtureMicrosoft::success_after(2);
    let fixture = MsAccountFixture::new(&server);
    let client = server.client();

    let grant = client.begin_device_code().await.unwrap();
    assert_eq!(grant.user_code, "AB12-CD34");
    assert_eq!(grant.verification_uri, "https://www.microsoft.com/link");
    assert_eq!(grant.poll_interval_seconds, 0, "轮询间隔必须来自服务端");

    let cancel = MicrosoftLoginCancel::new();
    let account = fixture
        .service
        .complete_microsoft_device_login(&client, &grant, &cancel)
        .await
        .unwrap();

    assert_eq!(account.kind, AccountKind::Microsoft);
    assert_eq!(account.username, "Steve");
    assert_eq!(account.player_uuid, PROFILE_UUID);
    assert!(account.is_default, "首个账户必须自动成为默认");
    assert!(account.last_validated_at_unix_seconds.is_some());
    assert_eq!(server.device_poll_count(), 3, "两次 pending 后第三次成功");

    let raw = Connection::open(&fixture.database_path)
        .unwrap()
        .query_row(
            "SELECT access_token, client_token, msa_refresh_token, mc_expires_at_unix_seconds
             FROM accounts WHERE id = ?1",
            params![account.id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<i64>>(3)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(raw.0, "mc-access-token-1");
    assert_eq!(raw.1, "", "Microsoft 账户不使用 client_token");
    assert_eq!(raw.2, "msa-refresh-token");
    assert!(raw.3.unwrap() > 0, "MC 令牌过期时间必须入库");

    let serialized = serde_json::to_string(&fixture.service.list_accounts().unwrap()).unwrap();
    assert!(
        !serialized.contains("mc-access-token"),
        "账户列表不得携带 MC 令牌"
    );
    assert!(
        !serialized.contains("msa-refresh-token"),
        "账户列表不得携带刷新令牌"
    );
}

#[tokio::test]
async fn m30_acct_002_cancel_during_poll() {
    let server = FixtureMicrosoft::never_authorized();
    let fixture = MsAccountFixture::new(&server);
    let client = server.client();
    let grant = client.begin_device_code().await.unwrap();
    let cancel = MicrosoftLoginCancel::new();

    let service = fixture.service.clone();
    let cancel_in_task = cancel.clone();
    let task = tokio::spawn(async move {
        service
            .complete_microsoft_device_login(&client, &grant, &cancel_in_task)
            .await
    });
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    cancel.cancel();
    let error = task.await.unwrap().expect_err("取消必须以已取消错误结束");

    assert!(
        matches!(error, CoreError::AccountLoginCancelled(_)),
        "{error}"
    );
    assert!(fixture.service.list_accounts().unwrap().is_empty());
}

#[tokio::test]
async fn m30_acct_003_declined_authorization_is_readable() {
    let server = FixtureMicrosoft::with_device_polls(vec![(
        400,
        serde_json::json!({"error": "authorization_declined"}).to_string(),
    )]);
    let fixture = MsAccountFixture::new(&server);
    let client = server.client();
    let grant = client.begin_device_code().await.unwrap();

    let error = fixture
        .service
        .complete_microsoft_device_login(&client, &grant, &MicrosoftLoginCancel::new())
        .await
        .expect_err("用户拒绝必须失败");

    assert!(error.to_string().contains("拒绝了授权"), "{error}");
    assert!(fixture.service.list_accounts().unwrap().is_empty());
}

#[tokio::test]
async fn m30_acct_004_account_without_minecraft_is_rejected() {
    let server = FixtureMicrosoft::with_profile_response((
        404,
        serde_json::json!({"error": "NOT_FOUND", "errorMessage": "The server has not found anything matching the request URI"}).to_string(),
    ));
    let fixture = MsAccountFixture::new(&server);
    let client = server.client();
    let grant = client.begin_device_code().await.unwrap();

    let error = fixture
        .service
        .complete_microsoft_device_login(&client, &grant, &MicrosoftLoginCancel::new())
        .await
        .expect_err("未拥有游戏必须失败");

    assert!(error.to_string().contains("未拥有 Minecraft"), "{error}");
    assert!(fixture.service.list_accounts().unwrap().is_empty());
}

#[tokio::test]
async fn m30_acct_005_xsts_business_error_is_mapped() {
    let server = FixtureMicrosoft::with_xsts_response((
        401,
        serde_json::json!({
            "Identity": "0",
            "XErr": 2_148_916_233_u64,
            "Message": "The account doesn't have an Xbox profile",
            "Redirect": "https://start.ui.xboxlive.com/AddChildToFamily"
        })
        .to_string(),
    ));
    let fixture = MsAccountFixture::new(&server);
    let client = server.client();
    let grant = client.begin_device_code().await.unwrap();

    let error = fixture
        .service
        .complete_microsoft_device_login(&client, &grant, &MicrosoftLoginCancel::new())
        .await
        .expect_err("XSTS 业务错误必须失败");

    assert!(error.to_string().contains("xbox.com"), "{error}");
    assert!(fixture.service.list_accounts().unwrap().is_empty());
}

#[tokio::test]
async fn m30_acct_006_refresh_rotates_and_persists_tokens() {
    let server = FixtureMicrosoft::success_after(0);
    let fixture = MsAccountFixture::new(&server);
    let account = fixture.login(&server).await;

    let refreshed = fixture
        .service
        .refresh_account_session(&account.id)
        .await
        .unwrap();

    assert_eq!(refreshed.session_state, AccountSessionState::Valid);
    let raw = Connection::open(&fixture.database_path)
        .unwrap()
        .query_row(
            "SELECT access_token, msa_refresh_token FROM accounts WHERE id = ?1",
            params![account.id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .unwrap();
    assert_eq!(raw.0, "mc-access-token-2", "刷新链必须重走并写入新 MC 令牌");
    assert_eq!(raw.1, "msa-refresh-token-2", "轮换的刷新令牌必须持久化");
}

#[tokio::test]
async fn m30_acct_007_revoked_refresh_marks_expired_and_blocks_launch() {
    let server = FixtureMicrosoft::with_refresh_script(vec![(
        400,
        serde_json::json!({"error": "invalid_grant", "error_description": "The user revoked access"}).to_string(),
    )]);
    let fixture = MsAccountFixture::new(&server);
    let account = fixture.login(&server).await;

    let error = fixture
        .service
        .refresh_account_session(&account.id)
        .await
        .expect_err("吊销的刷新令牌必须失败");

    assert!(matches!(error, CoreError::AccountCredentials(_)), "{error}");
    let stored = fixture
        .service
        .list_accounts()
        .unwrap()
        .into_iter()
        .find(|candidate| candidate.id == account.id)
        .unwrap();
    assert_eq!(stored.session_state, AccountSessionState::Expired);
    let error = fixture
        .service
        .account_launch_identity(None)
        .await
        .expect_err("会话过期必须拒绝启动");
    assert!(error.to_string().contains("重新登录"), "{error}");
}

#[tokio::test]
async fn m30_acct_008_launch_identity_uses_msa_user_type() {
    let server = FixtureMicrosoft::success_after(0);
    let fixture = MsAccountFixture::new(&server);
    fixture.login(&server).await;

    let identity = fixture.service.account_launch_identity(None).await.unwrap();

    let expected = LaunchAccount::microsoft("Steve", PROFILE_UUID, "mc-access-token-1").unwrap();
    assert_eq!(
        identity, expected,
        "启动身份必须使用 msa 用户类型与 MC 访问令牌"
    );
}

#[tokio::test]
async fn m30_acct_009_near_expiry_token_refreshed_before_launch() {
    let server = FixtureMicrosoft::success_after(0);
    let fixture = MsAccountFixture::new(&server);
    let account = fixture.login(&server).await;
    let soon = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
        + 100;
    Connection::open(&fixture.database_path)
        .unwrap()
        .execute(
            "UPDATE accounts SET mc_expires_at_unix_seconds = ?1 WHERE id = ?2",
            params![soon, account.id],
        )
        .unwrap();

    let identity = fixture.service.account_launch_identity(None).await.unwrap();

    let expected = LaunchAccount::microsoft("Steve", PROFILE_UUID, "mc-access-token-2").unwrap();
    assert_eq!(identity, expected, "临期 MC 令牌必须先经刷新链换新");
    let refresh_token: String = Connection::open(&fixture.database_path)
        .unwrap()
        .query_row(
            "SELECT msa_refresh_token FROM accounts WHERE id = ?1",
            params![account.id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        refresh_token, "msa-refresh-token-2",
        "轮换的刷新令牌必须持久化"
    );
}

#[tokio::test]
async fn m30_acct_010_slow_down_extends_poll_interval() {
    let server = FixtureMicrosoft::with_device_polls(vec![
        (400, serde_json::json!({"error": "slow_down"}).to_string()),
        (200, FixtureMicrosoft::msa_success_body()),
    ]);
    let fixture = MsAccountFixture::new(&server);
    let client = server.client();
    let grant = client.begin_device_code().await.unwrap();

    fixture
        .service
        .complete_microsoft_device_login(&client, &grant, &MicrosoftLoginCancel::new())
        .await
        .unwrap();

    let gaps = server.device_poll_gaps_seconds();
    assert_eq!(gaps.len(), 1, "slow_down 后应再轮询一次");
    assert!(
        gaps[0] >= 4.9,
        "slow_down 必须在原间隔（0s）基础上 +5s，实际 {:?}s",
        gaps[0]
    );
}

#[tokio::test]
async fn m30_acct_011_device_code_error_surfaces_service_detail() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { break };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let _ = read_request(&mut reader);
            respond(
                stream,
                400,
                &serde_json::json!({
                    "error": "invalid_request",
                    "error_description": "AADSTS9002327: Device code flow is not enabled for this application."
                })
                .to_string(),
            );
        }
    });
    let client = MicrosoftAuthClient::with_base_urls(
        "fixture-client-id",
        &format!("http://{address}"),
        "http://127.0.0.1:1",
        "http://127.0.0.1:1",
        "http://127.0.0.1:1",
    )
    .unwrap();

    let error = client
        .begin_device_code()
        .await
        .expect_err("设备码 400 必须失败");

    assert!(
        error.to_string().contains("AADSTS9002327"),
        "服务端错误详情必须透传：{error}"
    );
    drop(server);
}

#[tokio::test]
async fn m30_acct_012_invalid_app_registration_maps_to_review_guidance() {
    let server = FixtureMicrosoft::with_mc_login_response((
        403,
        serde_json::json!({
            "path": "/authentication/login_with_xbox",
            "errorMessage": "Invalid app registration, see https://aka.ms/AppRegInfo for more information"
        })
        .to_string(),
    ));
    let fixture = MsAccountFixture::new(&server);
    let client = server.client();
    let grant = client.begin_device_code().await.unwrap();

    let error = fixture
        .service
        .complete_microsoft_device_login(&client, &grant, &MicrosoftLoginCancel::new())
        .await
        .expect_err("未获 Mojang 允许名单的应用必须失败");

    assert!(
        error.to_string().contains("Mojang") && error.to_string().contains("允许名单"),
        "必须给出审批指引：{error}"
    );
    assert!(fixture.service.list_accounts().unwrap().is_empty());
}

struct MsAccountFixture {
    _directory: TempDir,
    database_path: std::path::PathBuf,
    service: AppService,
}

impl MsAccountFixture {
    fn new(server: &FixtureMicrosoft) -> Self {
        let directory = TempDir::new().unwrap();
        let database_path = directory.path().join("state.sqlite3");
        let data_directory = directory.path().join("data");
        let service = AppService::open(&database_path, &data_directory).unwrap();
        service.skip_onboarding().unwrap();
        let service = service.with_microsoft_auth_client(server.client());
        Self {
            _directory: directory,
            database_path,
            service,
        }
    }

    async fn login(&self, server: &FixtureMicrosoft) -> moyumax_core::AccountSummary {
        let client = server.client();
        let grant = client.begin_device_code().await.unwrap();
        self.service
            .complete_microsoft_device_login(&client, &grant, &MicrosoftLoginCancel::new())
            .await
            .unwrap()
    }
}

type Script = Arc<Mutex<VecDeque<(u16, String)>>>;

struct FixtureMicrosoft {
    msa_url: String,
    xbl_url: String,
    xsts_url: String,
    mcs_url: String,
    device_poll_times: Arc<Mutex<Vec<Instant>>>,
    _threads: Vec<thread::JoinHandle<()>>,
}

impl FixtureMicrosoft {
    fn success_after(pending: usize) -> Self {
        let script = (0..pending)
            .map(|_| {
                (
                    400_u16,
                    serde_json::json!({"error": "authorization_pending"}).to_string(),
                )
            })
            .collect();
        Self::new(script, vec![], xsts_ok(), profile_ok(), None)
    }

    fn never_authorized() -> Self {
        Self::new(
            vec![
                (
                    400_u16,
                    serde_json::json!({"error": "authorization_pending"}).to_string(),
                );
                10_000
            ],
            vec![],
            xsts_ok(),
            profile_ok(),
            None,
        )
    }

    fn with_device_polls(script: Vec<(u16, String)>) -> Self {
        Self::new(script, vec![], xsts_ok(), profile_ok(), None)
    }

    fn with_refresh_script(script: Vec<(u16, String)>) -> Self {
        Self::new(vec![], script, xsts_ok(), profile_ok(), None)
    }

    fn with_xsts_response(response: (u16, String)) -> Self {
        Self::new(vec![], vec![], response, profile_ok(), None)
    }

    fn with_profile_response(response: (u16, String)) -> Self {
        Self::new(vec![], vec![], xsts_ok(), response, None)
    }

    fn with_mc_login_response(response: (u16, String)) -> Self {
        Self::new(vec![], vec![], xsts_ok(), profile_ok(), Some(response))
    }

    fn msa_success_body() -> String {
        serde_json::json!({
            "access_token": "msa-access-token",
            "refresh_token": "msa-refresh-token",
            "expires_in": 3600
        })
        .to_string()
    }

    fn new(
        device_poll_script: Vec<(u16, String)>,
        refresh_script: Vec<(u16, String)>,
        xsts_response: (u16, String),
        profile_response: (u16, String),
        mc_login_response: Option<(u16, String)>,
    ) -> Self {
        let device_poll_times: Arc<Mutex<Vec<Instant>>> = Arc::new(Mutex::new(Vec::new()));
        let device_polls: Script = Arc::new(Mutex::new(VecDeque::from(device_poll_script)));
        let refresh_results: Script = Arc::new(Mutex::new(VecDeque::from(refresh_script)));
        let mc_counter = Arc::new(AtomicUsize::new(0));

        let (msa_url, msa_thread) = {
            let device_polls = Arc::clone(&device_polls);
            let refresh_results = Arc::clone(&refresh_results);
            let device_poll_times = Arc::clone(&device_poll_times);
            spawn_server(move |_method, path, body| {
                if path == "/devicecode" {
                    return (
                        200,
                        serde_json::json!({
                            "device_code": "fixture-device-code",
                            "user_code": "AB12-CD34",
                            "verification_uri": "https://www.microsoft.com/link",
                            "expires_in": 900,
                            "interval": 0
                        })
                        .to_string(),
                    );
                }
                if body.contains("grant_type=refresh_token") {
                    return refresh_results
                        .lock()
                        .unwrap()
                        .pop_front()
                        .unwrap_or_else(|| {
                            (
                                200,
                                serde_json::json!({
                                    "access_token": "msa-access-token-2",
                                    "refresh_token": "msa-refresh-token-2",
                                    "expires_in": 3600
                                })
                                .to_string(),
                            )
                        });
                }
                device_poll_times.lock().unwrap().push(Instant::now());
                device_polls
                    .lock()
                    .unwrap()
                    .pop_front()
                    .unwrap_or_else(|| (200, Self::msa_success_body()))
            })
        };
        let (xbl_url, xbl_thread) = spawn_server(|_method, _path, _body| {
            (
                200,
                serde_json::json!({
                    "Token": "xbl-token",
                    "DisplayClaims": {"xui": [{"uhs": "fixture-uhs"}]}
                })
                .to_string(),
            )
        });
        let (xsts_url, xsts_thread) = {
            let response = xsts_response.clone();
            spawn_server(move |_method, _path, _body| response.clone())
        };
        let (mcs_url, mcs_thread) = {
            let profile_response = profile_response.clone();
            let mc_counter = Arc::clone(&mc_counter);
            spawn_server(move |_method, path, _body| {
                if path == "/authentication/login_with_xbox" {
                    if let Some(response) = &mc_login_response {
                        return response.clone();
                    }
                    let count = mc_counter.fetch_add(1, Ordering::SeqCst) + 1;
                    return (
                        200,
                        serde_json::json!({
                            "access_token": format!("mc-access-token-{count}"),
                            "expires_in": 86_400
                        })
                        .to_string(),
                    );
                }
                profile_response.clone()
            })
        };
        Self {
            msa_url,
            xbl_url,
            xsts_url,
            mcs_url,
            device_poll_times,
            _threads: vec![msa_thread, xbl_thread, xsts_thread, mcs_thread],
        }
    }

    fn client(&self) -> MicrosoftAuthClient {
        MicrosoftAuthClient::with_base_urls(
            "fixture-client-id",
            &self.msa_url,
            &self.xbl_url,
            &self.xsts_url,
            &self.mcs_url,
        )
        .unwrap()
    }

    fn device_poll_count(&self) -> usize {
        self.device_poll_times.lock().unwrap().len()
    }

    fn device_poll_gaps_seconds(&self) -> Vec<f64> {
        let times = self.device_poll_times.lock().unwrap();
        times
            .windows(2)
            .map(|pair| pair[1].duration_since(pair[0]).as_secs_f64())
            .collect()
    }
}

fn xsts_ok() -> (u16, String) {
    (
        200,
        serde_json::json!({
            "Token": "xsts-token",
            "DisplayClaims": {"xui": [{"uhs": "fixture-uhs"}]}
        })
        .to_string(),
    )
}

fn profile_ok() -> (u16, String) {
    (
        200,
        serde_json::json!({
            "id": "069a79f444e94726a5befca90e38aaf5",
            "name": "Steve"
        })
        .to_string(),
    )
}

fn spawn_server(
    handler: impl Fn(&str, &str, &str) -> (u16, String) + Send + Sync + 'static,
) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let handler = Arc::new(handler);
    let handle = thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { break };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let (method, path, body) = read_request(&mut reader);
            let (status, response_body) = handler(&method, &path, &body);
            respond(stream, status, &response_body);
        }
    });
    (format!("http://{address}"), handle)
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

fn respond(mut stream: TcpStream, status: u16, body: &str) {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        404 => "Not Found",
        _ => "Error",
    };
    write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .unwrap();
    stream.flush().unwrap();
}
