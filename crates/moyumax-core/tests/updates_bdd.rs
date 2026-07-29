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
                let body = body.clone();
                thread::spawn(move || {
                    let mut reader = BufReader::new(stream.try_clone().unwrap());
                    let mut line = String::new();
                    loop {
                        line.clear();
                        if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                            break;
                        }
                    }
                    let reason = if status == 200 { "OK" } else { "Error" };
                    let _ = write!(
                        stream,
                        "HTTP/1.1 {} {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        status,
                        reason,
                        body.len()
                    );
                    let _ = stream.write_all(body.as_bytes());
                    let _ = stream.flush();
                });
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

    fn redirect_to(location: &str) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let location = location.to_owned();
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
                write!(
                    stream,
                    "HTTP/1.1 302 Found\r\nLocation: {location}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
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
}

#[tokio::test]
async fn m25_upd_005_download_follows_bounded_https_redirects() {
    let payload = b"redirected-installer".to_vec();
    let digest = encode_hex(Sha256::digest(&payload));
    let origin = FixtureRelease::new(200, &String::from_utf8_lossy(&payload));
    let redirect = FixtureRelease::redirect_to(&origin.url());
    let directory = TempDir::new().unwrap();
    let client = UpdateClient::with_base_url("http://127.0.0.1:1/").unwrap();
    let asset = moyumax_core::UpdateAsset {
        name: "setup.exe".to_owned(),
        url: redirect.url(),
        size: payload.len() as u64,
        sha256: Some(digest),
    };
    let path = client
        .download_installer(&asset, directory.path())
        .await
        .expect("302 重定向必须被跟随");
    assert_eq!(std::fs::read(&path).unwrap(), payload);
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

#[derive(Clone, Copy)]
enum CutMode {
    /// 首请求截断,之后按 Range 206 续传。
    ResumeAfterCut,
    /// 首请求截断,之后一律 200 整体重发(忽略 Range)。
    IgnoreRange,
    /// 每个请求都截断,永远无法完成。
    AlwaysCut,
}

/// 断流夹具:声明完整 Content-Length 但只发前缀后断开,模拟中途断流。
struct CutFixture {
    address: std::net::SocketAddr,
    _thread: thread::JoinHandle<()>,
}

impl CutFixture {
    fn start(payload: Vec<u8>, cut_at: usize, mode: CutMode) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let connection_count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let server_thread = thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { break };
                let payload = payload.clone();
                let connection_count = connection_count.clone();
                thread::spawn(move || {
                    let mut reader = BufReader::new(stream.try_clone().unwrap());
                    let mut range_from: Option<usize> = None;
                    let mut line = String::new();
                    loop {
                        line.clear();
                        if reader.read_line(&mut line).unwrap_or(0) == 0 {
                            return;
                        }
                        let trimmed = line.trim_end();
                        if trimmed.is_empty() {
                            break;
                        }
                        let lowered = trimmed.to_ascii_lowercase();
                        if let Some(value) = lowered.strip_prefix("range: bytes=") {
                            range_from = value.trim_end_matches('-').parse().ok();
                        }
                    }
                    let count = connection_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    let must_cut = match mode {
                        CutMode::AlwaysCut => true,
                        _ => count == 0,
                    };
                    if must_cut {
                        let _ = write!(
                            stream,
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            payload.len()
                        );
                        let _ = stream.write_all(&payload[..cut_at]);
                        let _ = stream.flush();
                        return;
                    }
                    match (mode, range_from) {
                        (CutMode::ResumeAfterCut, Some(from))
                            if from > 0 && from < payload.len() =>
                        {
                            let body = &payload[from..];
                            let _ = write!(
                                stream,
                                "HTTP/1.1 206 Partial Content\r\nContent-Range: bytes {}-{}/{}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                                from,
                                payload.len() - 1,
                                payload.len(),
                                body.len()
                            );
                            let _ = stream.write_all(body);
                        }
                        _ => {
                            let _ = write!(
                                stream,
                                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                                payload.len()
                            );
                            let _ = stream.write_all(&payload);
                        }
                    }
                    let _ = stream.flush();
                });
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

fn installer_asset(url: String, payload: &[u8]) -> moyumax_core::UpdateAsset {
    moyumax_core::UpdateAsset {
        name: "setup.exe".to_owned(),
        url,
        size: payload.len() as u64,
        sha256: Some(encode_hex(Sha256::digest(payload))),
    }
}

#[tokio::test]
async fn m25_upd_006_download_resumes_after_mid_stream_cut() {
    let payload: Vec<u8> = (0..200_000_u32).map(|index| (index % 251) as u8).collect();
    let server = CutFixture::start(payload.clone(), 80_000, CutMode::ResumeAfterCut);
    let directory = TempDir::new().unwrap();
    let client = UpdateClient::with_base_url("http://127.0.0.1:1/").unwrap();
    let asset = installer_asset(server.url(), &payload);

    let path = client
        .download_installer(&asset, directory.path())
        .await
        .expect("断流后必须带 Range 续传收敛");
    assert_eq!(std::fs::read(&path).unwrap(), payload);
    assert!(
        !directory.path().join(".setup.exe.partial").exists(),
        "成功后半成品必须已转正"
    );
}

#[tokio::test]
async fn m25_upd_007_download_restarts_when_range_is_ignored() {
    let payload: Vec<u8> = (0..120_000_u32).map(|index| (index % 241) as u8).collect();
    let server = CutFixture::start(payload.clone(), 50_000, CutMode::IgnoreRange);
    let directory = TempDir::new().unwrap();
    let client = UpdateClient::with_base_url("http://127.0.0.1:1/").unwrap();
    let asset = installer_asset(server.url(), &payload);

    let path = client
        .download_installer(&asset, directory.path())
        .await
        .expect("来源忽略 Range 时必须丢弃半成品整体重下");
    assert_eq!(
        std::fs::read(&path).unwrap(),
        payload,
        "若把整体重发追加到半成品后,文件必然膨胀错乱"
    );
}

#[tokio::test]
async fn m25_upd_008_download_gives_up_after_bounded_attempts_and_cleans_partial() {
    let payload: Vec<u8> = (0..90_000_u32).map(|index| (index % 233) as u8).collect();
    let server = CutFixture::start(payload.clone(), 30_000, CutMode::AlwaysCut);
    let directory = TempDir::new().unwrap();
    let client = UpdateClient::with_base_url("http://127.0.0.1:1/").unwrap();
    let asset = installer_asset(server.url(), &payload);

    let error = client
        .download_installer(&asset, directory.path())
        .await
        .expect_err("持续断流必须在有界次数内放弃");
    assert!(
        error.to_string().contains("已重试"),
        "报错应说明已重试:{error}"
    );
    assert!(
        !directory.path().join(".setup.exe.partial").exists(),
        "放弃后不得留下半成品"
    );
}
