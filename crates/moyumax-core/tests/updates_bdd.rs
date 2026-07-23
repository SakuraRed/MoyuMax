use std::{
    cmp::Ordering,
    io::{BufRead, BufReader, Write},
    net::TcpListener,
    thread,
};

use moyumax_core::{AppService, ReleaseInfo, UpdateClient, compare_versions, min_version_block};
use sha2::{Digest, Sha256};
use tempfile::TempDir;

#[test]
fn m25_ver_001_version_compare_follows_semver_rules() {
    assert_eq!(compare_versions("0.1.0", "0.2.0"), Ordering::Less);
    assert_eq!(compare_versions("0.2.0", "0.1.0"), Ordering::Greater);
    assert_eq!(compare_versions("0.1.0-preview.1", "0.1.0"), Ordering::Less);
    assert_eq!(
        compare_versions("0.1.0", "0.1.0-preview.1"),
        Ordering::Greater
    );
    assert_eq!(
        compare_versions("0.1.0-preview.1", "0.1.0-preview.2"),
        Ordering::Less
    );
    assert_eq!(compare_versions("v0.1.0", "0.1.0"), Ordering::Equal);
    assert_eq!(
        compare_versions("0.1.0-preview.1", "0.1.0-preview.1"),
        Ordering::Equal
    );
}

#[tokio::test]
async fn m25_upd_001_check_reports_new_version_and_up_to_date() {
    let release = serde_json::json!({
        "tag_name": "v0.2.0",
        "name": "0.2.0",
        "body": "新功能与修复\nmoyumax-min-app-version: 0.1.0",
        "html_url": "https://github.com/SakuraRed/MoyuMax/releases/tag/v0.2.0",
        "assets": [{
            "name": "MoyuMax_0.2.0_x64-setup.exe",
            "browser_download_url": "https://example.com/setup.exe",
            "size": 1024,
            "digest": "sha256:abc"
        }]
    });
    let server = FixtureRelease::new(200, &release.to_string());
    let client = UpdateClient::with_base_url(&server.url()).unwrap();

    let newer = client.check_latest("0.1.0-preview.1").await.unwrap();
    let info = newer.expect("低版本必须检测到新版本");
    assert_eq!(info.tag, "v0.2.0");
    assert_eq!(info.min_app_version.as_deref(), Some("0.1.0"));
    let installer = info.installer.expect("应识别 Windows 安装包资产");
    assert_eq!(installer.size, 1024);
    assert_eq!(installer.sha256.as_deref(), Some("abc"));

    let same = client.check_latest("0.2.0").await.unwrap();
    assert!(same.is_none(), "同级版本不得提示更新");
}

#[tokio::test]
async fn m25_upd_002_download_verifies_sha256_and_cleans_up_on_failure() {
    let payload = b"fake-installer-bytes".to_vec();
    let digest = encode_hex(Sha256::digest(&payload));
    let server = FixtureRelease::new(200, &String::from_utf8_lossy(&payload));
    let directory = TempDir::new().unwrap();
    let client = UpdateClient::with_base_url("http://127.0.0.1:1/").unwrap();
    let good = moyumax_core::UpdateAsset {
        name: "setup.exe".to_owned(),
        url: server.url(),
        size: payload.len() as u64,
        sha256: Some(digest.clone()),
    };

    let path = client
        .download_installer(&good, directory.path())
        .await
        .unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), payload);

    let bad = moyumax_core::UpdateAsset {
        sha256: Some("0".repeat(64)),
        ..good
    };
    let error = client
        .download_installer(&bad, directory.path())
        .await
        .expect_err("错误摘要必须校验失败");
    assert!(error.to_string().contains("SHA-256"));
    assert!(
        !directory.path().join(".setup.exe.partial").exists(),
        "校验失败不得留下半成品"
    );
}

#[test]
fn m25_upd_003_min_version_blocks_crossing_upgrades() {
    let release = ReleaseInfo {
        tag: "v0.5.0".to_owned(),
        name: String::new(),
        notes: String::new(),
        page_url: String::new(),
        min_app_version: Some("0.3.0".to_owned()),
        installer: None,
    };
    let blocked = min_version_block("0.1.0-preview.1", &release);
    assert!(blocked.is_some(), "跨越最低版本必须被阻止");
    assert!(blocked.unwrap().contains("0.3.0"));
    assert!(min_version_block("0.3.0", &release).is_none());
    assert!(min_version_block("0.4.0", &release).is_none());
}

#[test]
fn m25_upd_004_update_checks_setting_defaults_on_and_persists() {
    let directory = TempDir::new().unwrap();
    let service = AppService::open(
        &directory.path().join("state.sqlite3"),
        &directory.path().join("data"),
    )
    .unwrap();
    assert!(service.update_checks_enabled().unwrap(), "提示开关默认开启");
    service.set_update_checks_enabled(false).unwrap();
    let reopened = AppService::open(
        &directory.path().join("state.sqlite3"),
        &directory.path().join("data"),
    )
    .unwrap();
    assert!(!reopened.update_checks_enabled().unwrap());
}

struct FixtureRelease {
    address: std::net::SocketAddr,
    _thread: thread::JoinHandle<()>,
}

impl FixtureRelease {
    fn new(status: u16, body: &str) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let body = body.to_owned();
        let server_thread = thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { break };
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                loop {
                    line.clear();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                        break;
                    }
                }
                let reason = if status == 200 { "OK" } else { "Error" };
                write!(
                    stream,
                    "HTTP/1.1 {} {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    status,
                    reason,
                    body.len()
                )
                .unwrap();
                stream.write_all(body.as_bytes()).unwrap();
                stream.flush().unwrap();
            }
        });
        Self {
            address,
            _thread: server_thread,
        }
    }

    fn url(&self) -> String {
        format!("http://{}/", self.address)
    }
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
