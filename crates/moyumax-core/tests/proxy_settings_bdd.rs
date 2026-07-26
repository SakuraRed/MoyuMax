//! M34 代理设置 BDD：偏好持久化、地址校验与三种代理模式的线上行为。
//!
//! 线上行为用例经 [`netplay_http_client`] 观察（它是 8 处统一构造点之一），
//! 验证全局偏好 → `http_client_builder` → 真实请求线的完整链路。

use std::{
    io::{BufRead, BufReader, Write},
    net::TcpListener,
    path::PathBuf,
    sync::{Arc, Mutex, MutexGuard},
    thread,
    time::{Duration, Instant},
};

use moyumax_core::{
    AppService, ProxyPreference, active_proxy_preference, netplay_http_client,
    set_active_proxy_preference,
};
use tempfile::TempDir;

// 进程内全局代理偏好被多个用例读写，用文件级互斥锁串行化，避免并行用例互相污染。
static GLOBAL_PROXY_TEST_LOCK: Mutex<()> = Mutex::new(());

fn lock_global_proxy() -> MutexGuard<'static, ()> {
    let guard = GLOBAL_PROXY_TEST_LOCK.lock().unwrap();
    set_active_proxy_preference(ProxyPreference::System);
    guard
}

struct ServiceFixture {
    _directory: TempDir,
    database_path: PathBuf,
    data_directory: PathBuf,
    service: AppService,
}

impl ServiceFixture {
    fn new() -> Self {
        let directory = TempDir::new().unwrap();
        let database_path = directory.path().join("state.sqlite3");
        let data_directory = directory.path().join("data");
        let service = AppService::open(&database_path, &data_directory).unwrap();
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
}

#[test]
fn m34_proxy_001_default_system_and_preference_roundtrips() {
    let _guard = lock_global_proxy();
    let fixture = ServiceFixture::new();
    assert_eq!(
        fixture.service.proxy_preference().unwrap(),
        ProxyPreference::System
    );
    assert_eq!(
        active_proxy_preference(),
        ProxyPreference::System,
        "open 应把缺省偏好读入全局"
    );

    let custom = ProxyPreference::Custom {
        url: "http://127.0.0.1:10808".to_owned(),
    };
    fixture.service.set_proxy_preference(&custom).unwrap();
    assert_eq!(fixture.service.proxy_preference().unwrap(), custom);
    assert_eq!(
        active_proxy_preference(),
        custom,
        "set_proxy_preference 必须同步更新全局"
    );

    let reopened = fixture.reopen();
    assert_eq!(reopened.proxy_preference().unwrap(), custom);
    assert_eq!(
        active_proxy_preference(),
        custom,
        "重启后 open 必须把持久化偏好读入全局"
    );

    reopened
        .set_proxy_preference(&ProxyPreference::Direct)
        .unwrap();
    assert_eq!(
        reopened.proxy_preference().unwrap(),
        ProxyPreference::Direct
    );
    assert_eq!(active_proxy_preference(), ProxyPreference::Direct);
}

#[test]
fn m34_proxy_002_invalid_proxy_urls_rejected() {
    let _guard = lock_global_proxy();
    let fixture = ServiceFixture::new();
    for bad in [
        "",
        "127.0.0.1:10808",
        "ftp://127.0.0.1:10808",
        "socks5://127.0.0.1:10808",
        "http://",
    ] {
        let preference = ProxyPreference::Custom {
            url: bad.to_owned(),
        };
        assert!(
            fixture.service.set_proxy_preference(&preference).is_err(),
            "非法代理地址 {bad:?} 必须被拒绝"
        );
    }
    assert_eq!(
        fixture.service.proxy_preference().unwrap(),
        ProxyPreference::System,
        "拒绝后不得写入"
    );
    assert_eq!(
        active_proxy_preference(),
        ProxyPreference::System,
        "拒绝后全局不得改变"
    );

    for good in [
        "http://127.0.0.1:10808",
        "https://127.0.0.1:10808",
        "socks5h://127.0.0.1:10808",
    ] {
        let preference = ProxyPreference::Custom {
            url: good.to_owned(),
        };
        fixture.service.set_proxy_preference(&preference).unwrap();
        assert_eq!(fixture.service.proxy_preference().unwrap(), preference);
    }
}

// 文件级互斥锁只用于串行化全局偏好读写；tokio 测试默认 current-thread 运行时，
// 持有 guard 跨 await 不存在阻塞同线程其他任务的问题。
#[tokio::test]
#[allow(clippy::await_holding_lock)]
async fn m34_proxy_003_direct_mode_sends_origin_form_requests() {
    let _guard = lock_global_proxy();
    set_active_proxy_preference(ProxyPreference::Direct);
    let server = RequestLineCapture::respond_ok();
    let client = netplay_http_client().unwrap();
    let body = client
        .get(format!("{}/asset.bin", server.url()))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert_eq!(body, "ok");
    let line = server.take_request_line();
    assert!(
        line.starts_with("GET /asset.bin "),
        "直连模式必须发送源站形式请求行，实际：{line}"
    );
}

#[tokio::test]
#[allow(clippy::await_holding_lock)]
async fn m34_proxy_004_custom_mode_routes_http_via_absolute_uri() {
    let _guard = lock_global_proxy();
    let proxy = RequestLineCapture::respond_ok();
    set_active_proxy_preference(ProxyPreference::Custom { url: proxy.url() });
    let client = netplay_http_client().unwrap();
    // 目标端口 9 无服务：能拿到应答即证明请求确实经代理转发。
    let body = client
        .get("http://127.0.0.1:9/never-reachable/path")
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert_eq!(body, "ok", "自定义代理下 http 请求必须由代理应答");
    let line = proxy.take_request_line();
    assert!(
        line.starts_with("GET http://127.0.0.1:9/never-reachable/path "),
        "经代理的 http 请求必须使用绝对 URI，实际：{line}"
    );
}

#[tokio::test]
#[allow(clippy::await_holding_lock)]
async fn m34_proxy_005_custom_mode_tunnels_https_via_connect() {
    let _guard = lock_global_proxy();
    let proxy = ConnectCapture::new();
    set_active_proxy_preference(ProxyPreference::Custom { url: proxy.url() });
    let client = netplay_http_client().unwrap();
    // 桩收到 CONNECT 即关闭，客户端必然报错；断言重点是代理收到了 CONNECT host:port。
    let result = client
        .get("https://piston-meta.mojang.com/v1/packages/")
        .send()
        .await;
    assert!(result.is_err(), "CONNECT 桩关闭隧道后请求必须失败");
    let line = proxy.take_request_line();
    assert_eq!(line, "CONNECT piston-meta.mojang.com:443 HTTP/1.1");
}

/// 捕获首个请求行的最小 HTTP 桩：读取请求后回 200 与固定正文。
struct RequestLineCapture {
    url: String,
    request_line: Arc<Mutex<Option<String>>>,
    _thread: thread::JoinHandle<()>,
}

impl RequestLineCapture {
    fn respond_ok() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let request_line = Arc::new(Mutex::new(None));
        let captured = Arc::clone(&request_line);
        let server_thread = thread::spawn(move || {
            let Ok((stream, _)) = listener.accept() else {
                return;
            };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut first = String::new();
            if reader.read_line(&mut first).unwrap_or(0) == 0 {
                return;
            }
            *captured.lock().unwrap() = Some(first.trim_end().to_owned());
            let mut line = String::new();
            loop {
                line.clear();
                if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                    break;
                }
            }
            let mut stream = stream;
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok"
            )
            .unwrap();
            stream.flush().unwrap();
        });
        Self {
            url,
            request_line,
            _thread: server_thread,
        }
    }

    fn url(&self) -> String {
        self.url.clone()
    }

    fn take_request_line(&self) -> String {
        take_captured(&self.request_line)
    }
}

/// 只捕获 CONNECT 请求行的 TCP 桩：读取首行即关闭连接。
struct ConnectCapture {
    url: String,
    request_line: Arc<Mutex<Option<String>>>,
    _thread: thread::JoinHandle<()>,
}

impl ConnectCapture {
    fn new() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let request_line = Arc::new(Mutex::new(None));
        let captured = Arc::clone(&request_line);
        let server_thread = thread::spawn(move || {
            let Ok((stream, _)) = listener.accept() else {
                return;
            };
            let mut reader = BufReader::new(stream);
            let mut first = String::new();
            if reader.read_line(&mut first).unwrap_or(0) > 0 {
                *captured.lock().unwrap() = Some(first.trim_end().to_owned());
            }
        });
        Self {
            url,
            request_line,
            _thread: server_thread,
        }
    }

    fn url(&self) -> String {
        self.url.clone()
    }

    fn take_request_line(&self) -> String {
        take_captured(&self.request_line)
    }
}

fn take_captured(slot: &Arc<Mutex<Option<String>>>) -> String {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(line) = slot.lock().unwrap().take() {
            return line;
        }
        assert!(Instant::now() < deadline, "桩未在 5 秒内收到请求");
        thread::sleep(Duration::from_millis(10));
    }
}
