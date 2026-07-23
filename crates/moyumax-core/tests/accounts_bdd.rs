use std::{
    io::{BufRead, BufReader, Write},
    net::TcpListener,
    thread,
};

use moyumax_core::{
    AccountKind, AccountSessionState, AppService, CoreError, LaunchAccount, YggdrasilClient,
};
use rusqlite::{Connection, params};
use tempfile::TempDir;

#[test]
fn m20_acct_001_offline_account_creation_and_validation() {
    let fixture = AccountFixture::new();

    let account = fixture.service.add_offline_account("Steve_2026").unwrap();

    assert_eq!(account.kind, AccountKind::Offline);
    assert!(account.is_default, "首个账户必须自动成为默认");
    assert_eq!(
        account.player_uuid,
        LaunchAccount::offline("Steve_2026")
            .unwrap()
            .player_uuid()
            .to_string()
    );
    let accounts = fixture.service.list_accounts().unwrap();
    assert_eq!(accounts.len(), 1);
    assert_eq!(accounts[0].session_state, AccountSessionState::Valid);
    assert!(fixture.service.add_offline_account("x").is_err());
    assert!(fixture.service.add_offline_account("bad name!").is_err());
    assert!(
        fixture
            .service
            .add_offline_account("this_name_is_way_too_long")
            .is_err()
    );
}

#[tokio::test]
async fn m20_acct_002_authlib_login_stores_tokens_but_never_the_password() {
    let server = FixtureYggdrasil::ok();
    let fixture = AccountFixture::new();
    let client = YggdrasilClient::with_base_url(&server.base_url()).unwrap();

    let account = fixture
        .service
        .add_authlib_account(
            &client,
            &server.base_url(),
            "Alex@littleskin.cn",
            "s3cret-password",
        )
        .await
        .unwrap();

    assert_eq!(account.kind, AccountKind::Authlib);
    assert_eq!(account.username, "Alex");
    assert_eq!(account.player_uuid, "069a79f4-44e9-4726-a5be-fca90e38aaf5");
    assert!(account.is_default);
    assert!(account.last_validated_at_unix_seconds.is_some());
    let raw = Connection::open(&fixture.database_path)
        .unwrap()
        .query_row(
            "SELECT access_token, client_token FROM accounts WHERE id = ?1",
            params![account.id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .unwrap();
    assert_eq!(raw.0, "fixture-access-token");
    assert_eq!(raw.1, "fixture-client-token");
    let all_tokens: String = Connection::open(&fixture.database_path)
        .unwrap()
        .query_row(
            "SELECT GROUP_CONCAT(access_token || client_token, '') FROM accounts",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(
        !all_tokens.contains("s3cret-password"),
        "密码绝不写入数据库"
    );
    let serialized = serde_json::to_string(&fixture.service.list_accounts().unwrap()).unwrap();
    assert!(
        !serialized.contains("fixture-access-token"),
        "账户列表不得携带令牌"
    );
}

#[tokio::test]
async fn m20_acct_003_credentials_error_is_distinct_from_network_error() {
    let server = FixtureYggdrasil::rejected();
    let fixture = AccountFixture::new();
    let client = YggdrasilClient::with_base_url(&server.base_url()).unwrap();

    let error = fixture
        .service
        .add_authlib_account(&client, &server.base_url(), "Alex@littleskin.cn", "wrong")
        .await
        .expect_err("凭据错误必须可读分类");

    assert!(
        matches!(error, CoreError::AccountCredentials(_)),
        "403 必须归类为凭据错误：{error}"
    );
    assert!(error.to_string().contains("凭据无效或会话已过期"));
    assert!(fixture.service.list_accounts().unwrap().is_empty());

    let unreachable = YggdrasilClient::with_base_url("http://127.0.0.1:1/").unwrap();
    let error = fixture
        .service
        .add_authlib_account(&unreachable, "http://127.0.0.1:1/", "Alex", "pw")
        .await
        .expect_err("网络错误必须与凭据错误区分");
    assert!(matches!(error, CoreError::AccountNetwork(_)), "{error}");
}

#[tokio::test]
async fn m20_acct_004_default_is_unique_and_launch_uses_default_identity() {
    let server = FixtureYggdrasil::ok();
    let fixture = AccountFixture::new();
    fixture.service.add_offline_account("Steve_2026").unwrap();
    let authlib = fixture
        .service
        .add_authlib_account(
            &YggdrasilClient::with_base_url(&server.base_url()).unwrap(),
            &server.base_url(),
            "Alex@littleskin.cn",
            "pw",
        )
        .await
        .unwrap();

    fixture.service.set_default_account(&authlib.id).unwrap();
    let defaults = fixture
        .service
        .list_accounts()
        .unwrap()
        .into_iter()
        .filter(|account| account.is_default)
        .count();
    assert_eq!(defaults, 1, "默认账户必须唯一");

    let identity = fixture.service.account_launch_identity(None).unwrap();
    assert_eq!(
        identity.player_uuid().to_string(),
        "069a79f4-44e9-4726-a5be-fca90e38aaf5"
    );
}

#[tokio::test]
async fn m20_acct_005_revoked_token_marks_expired_and_blocks_launch() {
    let ok = FixtureYggdrasil::ok();
    let fixture = AccountFixture::new();
    let account = fixture
        .service
        .add_authlib_account(
            &YggdrasilClient::with_base_url(&ok.base_url()).unwrap(),
            &ok.base_url(),
            "Alex@littleskin.cn",
            "pw",
        )
        .await
        .unwrap();
    drop(ok);
    let rejected = FixtureYggdrasil::rejected();
    Connection::open(&fixture.database_path)
        .unwrap()
        .execute(
            "UPDATE accounts SET server_url = ?1 WHERE id = ?2",
            params![rejected.base_url(), account.id],
        )
        .unwrap();

    let error = fixture
        .service
        .refresh_account_session(&account.id)
        .await
        .expect_err("吊销的令牌必须刷新失败");

    assert!(matches!(error, CoreError::AccountCredentials(_)));
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
        .expect_err("会话过期必须拒绝启动");
    assert!(error.to_string().contains("重新登录"));
}

#[test]
fn m20_acct_006_removing_default_promotes_the_earliest_remaining() {
    let fixture = AccountFixture::new();
    let first = fixture.service.add_offline_account("Steve_2026").unwrap();
    let second = fixture.service.add_offline_account("Alex_2027").unwrap();
    assert!(first.is_default);
    assert!(!second.is_default);

    fixture.service.remove_account(&first.id).unwrap();

    let accounts = fixture.service.list_accounts().unwrap();
    assert_eq!(accounts.len(), 1);
    assert!(accounts[0].is_default, "移除默认后最早剩余账户必须接任");
    assert!(fixture.service.remove_account("missing-id").is_err());
}

#[test]
fn m20_acct_007_empty_store_creates_compatible_offline_default() {
    let fixture = AccountFixture::new();

    let identity = fixture.service.account_launch_identity(None).unwrap();

    assert_eq!(
        identity.player_uuid(),
        LaunchAccount::offline("MoyuMaxPlayer")
            .unwrap()
            .player_uuid()
    );
    let accounts = fixture.service.list_accounts().unwrap();
    assert_eq!(accounts.len(), 1);
    assert!(accounts[0].is_default);
    assert_eq!(accounts[0].username, "MoyuMaxPlayer");
}

struct AccountFixture {
    _directory: TempDir,
    database_path: std::path::PathBuf,
    service: AppService,
}

impl AccountFixture {
    fn new() -> Self {
        let directory = TempDir::new().unwrap();
        let database_path = directory.path().join("state.sqlite3");
        let data_directory = directory.path().join("data");
        let service = AppService::open(&database_path, &data_directory).unwrap();
        service.skip_onboarding().unwrap();
        Self {
            _directory: directory,
            database_path,
            service,
        }
    }
}

struct FixtureYggdrasil {
    address: std::net::SocketAddr,
    _thread: thread::JoinHandle<()>,
}

impl FixtureYggdrasil {
    fn ok() -> Self {
        Self::new(
            200,
            &serde_json::json!({
                "accessToken": "fixture-access-token",
                "clientToken": "fixture-client-token",
                "selectedProfile": {
                    "id": "069a79f4-44e9-4726-a5be-fca90e38aaf5",
                    "name": "Alex"
                }
            })
            .to_string(),
        )
    }

    fn rejected() -> Self {
        Self::new(
            403,
            &serde_json::json!({
                "error": "ForbiddenOperationException",
                "errorMessage": "Invalid credentials. Invalid username or password."
            })
            .to_string(),
        )
    }

    fn new(status: u16, body: &str) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let body = body.to_owned();
        let server_thread = thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { break };
                let reason = if status == 200 { "OK" } else { "Forbidden" };
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut request_line = String::new();
                if reader.read_line(&mut request_line).unwrap_or(0) == 0 {
                    continue;
                }
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                        break;
                    }
                }
                let mut stream = stream;
                write!(
                    stream,
                    "HTTP/1.1 {} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    status,
                    reason,
                    body.len(),
                    body
                )
                .unwrap();
                stream.flush().unwrap();
            }
        });
        Self {
            address,
            _thread: server_thread,
        }
    }

    fn base_url(&self) -> String {
        format!("http://{}/", self.address)
    }
}
