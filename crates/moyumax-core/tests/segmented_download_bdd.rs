use std::{
    fs,
    io::{BufRead, BufReader, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use moyumax_core::{
    ArtifactDownloader, ArtifactKind, CoreError, DownloadInterrupt, ResolvedArtifact, SourcePolicy,
};
use sha1::{Digest, Sha1};
use sha2::Sha512;
use tempfile::TempDir;

#[tokio::test]
async fn m10_download_001_segmented_download_uses_parallel_ranges_and_merges() {
    let body: Vec<u8> = (0..40 * 1024 * 1024_u32)
        .map(|index| (index % 251) as u8)
        .collect();
    let server = RangeServer::new(body.clone(), ServerOptions::default());
    let fixture = SegmentFixture::new(&server.url("/big.jar"), &body);

    let report = fixture
        .downloader
        .fetch_with_policy(
            &fixture.artifact,
            &fixture.staging,
            &fixture.shared,
            &SourcePolicy::MirrorFirst,
            None,
        )
        .await
        .expect("分段下载应完成");

    assert!(report.segmented, "支持 Range 的大文件必须走分段模式");
    assert_eq!(report.segment_count, 3, "40 MiB 应分为 3 个分段");
    assert_eq!(fs::read(report.result.staged_file).unwrap(), body);
    let ranges = server.ranges();
    assert!(ranges.len() >= 3, "应产生多个分段请求: {ranges:?}");
    let mut covered = ranges.clone();
    covered.sort();
    let mut expected = 0_u64;
    for (start, end) in &covered {
        assert_eq!(*start, expected, "分段范围必须不重叠且连续: {covered:?}");
        expected = *end + 1;
    }
    assert_eq!(expected, body.len() as u64, "分段必须覆盖完整文件");
    assert!(!fixture.segments_dir().exists(), "合并后分段目录必须清理");
}

#[tokio::test]
async fn m10_download_001_corrupted_segment_is_redownloaded_precisely() {
    let body: Vec<u8> = (0..24 * 1024 * 1024_u32)
        .map(|index| (index % 241) as u8)
        .collect();
    let server = RangeServer::new(body.clone(), ServerOptions::default());
    let url = server.url("/big.jar");
    let fixture = SegmentFixture::new(&url, &body);
    let segments_dir = fixture.segments_dir();
    fs::create_dir_all(&segments_dir).unwrap();
    let split = 16 * 1024 * 1024_usize;
    // 分段 0 已完成且内容正确;分段 1 标记完成但文件长度不足,必须精准重下。
    fs::write(segments_dir.join("seg-000000.part"), &body[..split]).unwrap();
    fs::write(
        segments_dir.join("seg-000001.part"),
        &body[split..split + 1024],
    )
    .unwrap();
    let manifest = format!(
        r#"{{"url":"{url}","total":{},"etag":null,"lastModified":null,"segments":[{{"index":0,"start":0,"end":{split},"completed":{split},"done":true}},{{"index":1,"start":{split},"end":{},"completed":1024,"done":true}}]}}"#,
        body.len(),
        body.len()
    );
    fs::write(segments_dir.join("manifest.json"), manifest).unwrap();

    let report = fixture
        .downloader
        .fetch_with_policy(
            &fixture.artifact,
            &fixture.staging,
            &fixture.shared,
            &SourcePolicy::MirrorFirst,
            None,
        )
        .await
        .expect("损坏分段应被精准重下并完成");

    assert_eq!(fs::read(report.result.staged_file).unwrap(), body);
    let ranges = server.ranges();
    assert_eq!(ranges.len(), 1, "只有损坏分段允许重新请求: {ranges:?}");
    assert_eq!(ranges[0].0, split as u64);
}

#[tokio::test]
async fn m10_download_001_merged_hash_failure_never_commits() {
    let body: Vec<u8> = (0..24 * 1024 * 1024_u32)
        .map(|index| (index % 233) as u8)
        .collect();
    let server = RangeServer::new(body.clone(), ServerOptions::default());
    let url = server.url("/big.jar");
    let fixture = SegmentFixture::new(&url, &body);
    let segments_dir = fixture.segments_dir();
    fs::create_dir_all(&segments_dir).unwrap();
    let split = 16 * 1024 * 1024_usize;
    // 两个分段都标记完成,但分段 1 内容被替换为同长度脏数据;
    // 清单层面无法发现,最终完整哈希必须拦截提交。
    fs::write(segments_dir.join("seg-000000.part"), &body[..split]).unwrap();
    fs::write(
        segments_dir.join("seg-000001.part"),
        vec![0xEE_u8; body.len() - split],
    )
    .unwrap();
    let manifest = format!(
        r#"{{"url":"{url}","total":{},"etag":null,"lastModified":null,"segments":[{{"index":0,"start":0,"end":{split},"completed":{split},"done":true}},{{"index":1,"start":{split},"end":{},"completed":{},"done":true}}]}}"#,
        body.len(),
        body.len(),
        body.len() - split
    );
    fs::write(segments_dir.join("manifest.json"), manifest).unwrap();

    let error = fixture
        .downloader
        .fetch_with_policy(
            &fixture.artifact,
            &fixture.staging,
            &fixture.shared,
            &SourcePolicy::MirrorFirst,
            None,
        )
        .await
        .expect_err("合并后哈希不一致必须失败");

    assert!(error.to_string().contains("SHA"), "应为哈希错误: {error}");
    assert!(server.ranges().is_empty(), "清单可信时不应发起请求");
    assert!(!fixture.staged_file().exists(), "不得生成最终文件");
    assert!(
        !fixture
            .shared
            .join(&fixture.artifact.relative_path)
            .exists(),
        "不得进入共享存储"
    );
}

#[tokio::test]
async fn m10_download_001_range_ignored_degrades_to_single_connection() {
    let body: Vec<u8> = (0..24 * 1024 * 1024_u32)
        .map(|index| (index % 227) as u8)
        .collect();
    let server = RangeServer::new(
        body.clone(),
        ServerOptions {
            honor_range: false,
            ..ServerOptions::default()
        },
    );
    let fixture = SegmentFixture::new(&server.url("/big.jar"), &body);

    let report = fixture
        .downloader
        .fetch_with_policy(
            &fixture.artifact,
            &fixture.staging,
            &fixture.shared,
            &SourcePolicy::MirrorFirst,
            None,
        )
        .await
        .expect("Range 被忽略时应安全降级");

    assert!(!report.segmented);
    let reason = report.degraded_reason.expect("必须记录降级原因");
    assert!(reason.contains("Range"), "降级原因应说明 Range: {reason}");
    assert_eq!(fs::read(report.result.staged_file).unwrap(), body);
}

#[tokio::test]
async fn m10_download_001_etag_change_discards_segments() {
    let body: Vec<u8> = (0..24 * 1024 * 1024_u32)
        .map(|index| (index % 223) as u8)
        .collect();
    let server = RangeServer::new(
        body.clone(),
        ServerOptions {
            etag: Some("\"v2\"".to_owned()),
            ..ServerOptions::default()
        },
    );
    let url = server.url("/big.jar");
    let fixture = SegmentFixture::new(&url, &body);
    let segments_dir = fixture.segments_dir();
    fs::create_dir_all(&segments_dir).unwrap();
    let split = 16 * 1024 * 1024_usize;
    fs::write(segments_dir.join("seg-000000.part"), &body[..split]).unwrap();
    let manifest = format!(
        r#"{{"url":"{url}","total":{},"etag":"\"v1\"","lastModified":null,"segments":[{{"index":0,"start":0,"end":{split},"completed":{split},"done":true}},{{"index":1,"start":{split},"end":{},"completed":0,"done":false}}]}}"#,
        body.len(),
        body.len()
    );
    fs::write(segments_dir.join("manifest.json"), manifest).unwrap();

    let report = fixture
        .downloader
        .fetch_with_policy(
            &fixture.artifact,
            &fixture.staging,
            &fixture.shared,
            &SourcePolicy::MirrorFirst,
            None,
        )
        .await
        .expect("对象证据变化后应废弃分段并重新下载");

    let reason = report.degraded_reason.expect("必须记录证据变化");
    assert!(reason.contains("ETag"), "降级原因应说明 ETag: {reason}");
    assert!(!fixture.segments_dir().exists(), "旧分段必须废弃");
    assert_eq!(fs::read(report.result.staged_file).unwrap(), body);
}

#[tokio::test]
async fn m10_task_002_pause_and_resume_reuses_segments() {
    let body: Vec<u8> = (0..48 * 1024 * 1024_u32)
        .map(|index| (index % 239) as u8)
        .collect();
    let server = RangeServer::new(
        body.clone(),
        ServerOptions {
            throttle: Some((256 * 1024, 20)),
            ..ServerOptions::default()
        },
    );
    let url = server.url("/big.jar");
    let fixture = SegmentFixture::new(&url, &body);

    // 单许可执行器让分段按顺序下载,暂停窗口确定。
    let ordered_downloader = ArtifactDownloader::new(1).unwrap();
    let interrupt = DownloadInterrupt::new();
    let handle = tokio::spawn({
        let downloader = ordered_downloader.clone();
        let artifact = fixture.artifact.clone();
        let staging = fixture.staging.clone();
        let shared = fixture.shared.clone();
        let interrupt = interrupt.clone();
        async move {
            downloader
                .fetch_with_policy(
                    &artifact,
                    &staging,
                    &shared,
                    &SourcePolicy::MirrorFirst,
                    Some(&interrupt),
                )
                .await
        }
    });
    // 等待首个分段完成且第二个分段已经开始,然后暂停。
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let seg0_done = {
            let path = fixture.segments_dir().join("seg-000000.part");
            path.metadata().map(|meta| meta.len()).unwrap_or(0) == fixture.segment_span(0)
        };
        let seg1_started = {
            let path = fixture.segments_dir().join("seg-000001.part");
            path.metadata().map(|meta| meta.len()).unwrap_or(0) > 0
        };
        if seg0_done && seg1_started {
            break;
        }
        assert!(Instant::now() < deadline, "等待分段进入暂停窗口超时");
        tokio::time::sleep(Duration::from_millis(15)).await;
    }
    interrupt.interrupt();
    let error = handle.await.unwrap().expect_err("暂停中断必须返回暂停错误");
    assert!(matches!(error, CoreError::TaskPaused));
    let before_resume = server.ranges();

    let report = fixture
        .downloader
        .fetch_with_policy(
            &fixture.artifact,
            &fixture.staging,
            &fixture.shared,
            &SourcePolicy::MirrorFirst,
            None,
        )
        .await
        .expect("恢复后应复用有效分段完成下载");

    assert_eq!(fs::read(report.result.staged_file).unwrap(), body);
    let after_resume = server.ranges_since(before_resume.len());
    assert!(!after_resume.is_empty(), "恢复必须继续未完成分段");
    for (start, _end) in &after_resume {
        assert!(
            *start >= 16 * 1024 * 1024,
            "已完成分段不得重新下载: {after_resume:?}"
        );
    }
}

#[tokio::test]
async fn m10_download_002_two_files_share_bounded_pool_fairly() {
    let body_a: Vec<u8> = (0..24 * 1024 * 1024_u32)
        .map(|index| (index % 229) as u8)
        .collect();
    let body_b: Vec<u8> = (0..24 * 1024 * 1024_u32)
        .map(|index| ((index + 7) % 229) as u8)
        .collect();
    let server = RangeServer::new_routes(
        vec![
            ("/a.jar".to_owned(), body_a.clone()),
            ("/b.jar".to_owned(), body_b.clone()),
        ],
        ServerOptions {
            throttle: Some((256 * 1024, 30)),
            ..ServerOptions::default()
        },
    );
    let directory = TempDir::new().unwrap();
    let _keep = &directory;
    let downloader = ArtifactDownloader::new(2).unwrap();
    let artifact_a = big_artifact(&server.url("/a.jar"), "a.jar", &body_a);
    let artifact_b = big_artifact(&server.url("/b.jar"), "b.jar", &body_b);
    let staging = directory.path().join("staging");
    let shared = directory.path().join("shared");

    let started = Instant::now();
    let (result_a, result_b) = tokio::join!(
        downloader.fetch_with_policy(
            &artifact_a,
            &staging,
            &shared,
            &SourcePolicy::MirrorFirst,
            None
        ),
        downloader.fetch_with_policy(
            &artifact_b,
            &staging,
            &shared,
            &SourcePolicy::MirrorFirst,
            None
        )
    );
    let report_a = result_a.expect("文件 A 应完成");
    let report_b = result_b.expect("文件 B 应完成");
    let elapsed = started.elapsed();

    assert_eq!(fs::read(&report_a.result.staged_file).unwrap(), body_a);
    assert_eq!(fs::read(&report_b.result.staged_file).unwrap(), body_b);
    assert!(report_a.segmented && report_b.segmented);
    assert!(
        report_a.segment_count <= 8 && report_b.segment_count <= 8,
        "单文件分段数必须有界"
    );
    // 两个文件的请求必须在时间上重叠,证明共享预算下并行推进而不是串行饿死;
    // 服务端同时服务的请求数不得突破有界池(允许连接关闭瞬间的抖动)。
    let overlap = server.max_overlapping_requests();
    assert!(overlap >= 2, "两个任务应并行取得进展: 重叠 {overlap}");
    assert!(
        server.peak_concurrency() <= 4,
        "连接数不得无界: 峰值 {}",
        server.peak_concurrency()
    );
    // 每连接限速 256 KiB/30ms:两个文件完全串行约需 22 秒,共享预算并行应显著更快。
    // 阈值放宽到 20s 以容忍 CI 单核慢 runner(本机约 3s,慢 runner 约 11s),
    // 并行结构本身已由上面的重叠与峰值断言强约束。
    assert!(
        elapsed < Duration::from_secs(20),
        "两个任务应共享预算并行推进而不是互相饿死: {elapsed:?}"
    );
}

#[tokio::test]
async fn m10_download_bench_segmented_vs_single_connection() {
    let body: Vec<u8> = (0..8 * 1024 * 1024_u32)
        .map(|index| (index % 245) as u8)
        .collect();
    let mut measurements = Vec::new();
    for _round in 0..3 {
        let server = RangeServer::new(
            body.clone(),
            ServerOptions {
                throttle: Some((64 * 1024, 30)),
                ..ServerOptions::default()
            },
        );
        let artifact = big_artifact(&server.url("/big.jar"), "big.jar", &body);
        let single_start = Instant::now();
        let single_dir = TempDir::new().unwrap();
        let single_result = ArtifactDownloader::new(4)
            .unwrap()
            .fetch_with_interrupt(
                &artifact,
                &single_dir.path().join("staging"),
                &single_dir.path().join("shared"),
                None,
            )
            .await
            .expect("单连接应完成");
        let single_elapsed = single_start.elapsed();

        let segmented_start = Instant::now();
        let segmented_dir = TempDir::new().unwrap();
        let segmented_report = ArtifactDownloader::new(8)
            .unwrap()
            .with_segment_target_bytes(1024 * 1024)
            .fetch_with_policy(
                &artifact,
                &segmented_dir.path().join("staging"),
                &segmented_dir.path().join("shared"),
                &SourcePolicy::MirrorFirst,
                None,
            )
            .await
            .expect("分段应完成");
        let segmented_elapsed = segmented_start.elapsed();

        assert!(single_result.staged_file.is_file());
        assert!(segmented_report.segmented);
        assert_eq!(segmented_report.segment_count, 8);
        measurements.push((
            single_elapsed.as_secs_f64(),
            segmented_elapsed.as_secs_f64(),
        ));
        drop(server);
    }
    let median = |mut values: Vec<f64>| {
        values.sort_by(|left, right| left.partial_cmp(right).unwrap());
        values[values.len() / 2]
    };
    let single_median = median(measurements.iter().map(|(value, _)| *value).collect());
    let segmented_median = median(measurements.iter().map(|(_, value)| *value).collect());
    let ratio = single_median / segmented_median;
    let report = serde_json::json!({
        "generatedAt": "bench",
        "rounds": measurements,
        "singleMedianSeconds": single_median,
        "segmentedMedianSeconds": segmented_median,
        "ratio": ratio,
        "note": "受控 Range 源每连接限速 64 KiB/30ms,8 MiB 文件分 8 段,3 轮取中位数",
    });
    let output_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../output");
    fs::create_dir_all(&output_dir).unwrap();
    fs::write(
        output_dir.join("download-bench-latest.json"),
        serde_json::to_string_pretty(&report).unwrap(),
    )
    .unwrap();
    assert!(
        ratio >= 1.8,
        "单连接未占满链路时分段吞吐应不低于 1.8x: 单连接 {single_median:.2}s 分段 {segmented_median:.2}s 比率 {ratio:.2}"
    );
}

struct SegmentFixture {
    _directory: TempDir,
    staging: PathBuf,
    shared: PathBuf,
    artifact: ResolvedArtifact,
    downloader: ArtifactDownloader,
}

impl SegmentFixture {
    fn new(url: &str, expected: &[u8]) -> Self {
        let directory = TempDir::new().unwrap();
        Self {
            staging: directory.path().join("staging"),
            shared: directory.path().join("shared"),
            artifact: big_artifact(url, "big.jar", expected),
            downloader: ArtifactDownloader::new(4).unwrap(),
            _directory: directory,
        }
    }

    fn segments_dir(&self) -> PathBuf {
        self.staging
            .join(&self.artifact.relative_path)
            .with_extension("segments")
    }

    fn staged_file(&self) -> PathBuf {
        self.staging.join(&self.artifact.relative_path)
    }

    fn segment_span(&self, index: u32) -> u64 {
        let total = self.artifact.size;
        let count = 3_u64;
        let base = total.div_ceil(count);
        let start = u64::from(index) * base;
        let end = (start + base).min(total);
        end - start
    }
}

fn big_artifact(url: &str, name: &str, expected: &[u8]) -> ResolvedArtifact {
    ResolvedArtifact {
        kind: ArtifactKind::GameClient,
        relative_path: format!("minecraft/versions/test/{name}"),
        url: url.to_owned(),
        size: u64::try_from(expected.len()).unwrap(),
        sha1: Some(digest_sha1(expected)),
        sha256: None,
        sha512: Some(digest_sha512(expected)),
    }
}

fn digest_sha1(bytes: &[u8]) -> String {
    let mut hasher = Sha1::new();
    hasher.update(bytes);
    encode_hex(hasher.finalize())
}

fn digest_sha512(bytes: &[u8]) -> String {
    let mut hasher = Sha512::new();
    hasher.update(bytes);
    encode_hex(hasher.finalize())
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

#[derive(Clone)]
struct ServerOptions {
    honor_range: bool,
    etag: Option<String>,
    throttle: Option<(usize, u64)>,
}

impl Default for ServerOptions {
    fn default() -> Self {
        Self {
            honor_range: true,
            etag: None,
            throttle: None,
        }
    }
}

struct RangeServer {
    address: std::net::SocketAddr,
    state: Arc<ServerState>,
    _thread: thread::JoinHandle<()>,
}

struct ServerState {
    routes: Vec<(String, Vec<u8>)>,
    options: ServerOptions,
    requests: Mutex<Vec<Option<(u64, u64)>>>,
    timeline: Mutex<Vec<(String, Instant, Instant)>>,
    concurrent: AtomicUsize,
    peak: AtomicUsize,
}

impl RangeServer {
    fn new(body: Vec<u8>, options: ServerOptions) -> Self {
        Self::new_routes(vec![("/big.jar".to_owned(), body)], options)
    }

    fn new_routes(routes: Vec<(String, Vec<u8>)>, options: ServerOptions) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let state = Arc::new(ServerState {
            routes,
            options,
            requests: Mutex::new(Vec::new()),
            timeline: Mutex::new(Vec::new()),
            concurrent: AtomicUsize::new(0),
            peak: AtomicUsize::new(0),
        });
        let shared = Arc::clone(&state);
        let server_thread = thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                let state = Arc::clone(&shared);
                thread::spawn(move || serve_range(stream, &state));
            }
        });
        Self {
            address,
            state,
            _thread: server_thread,
        }
    }

    fn url(&self, path: &str) -> String {
        format!("http://{}{path}", self.address)
    }

    fn ranges(&self) -> Vec<(u64, u64)> {
        self.state
            .requests
            .lock()
            .unwrap()
            .iter()
            .flatten()
            .copied()
            .collect()
    }

    fn ranges_since(&self, offset: usize) -> Vec<(u64, u64)> {
        self.state
            .requests
            .lock()
            .unwrap()
            .iter()
            .skip(offset)
            .flatten()
            .copied()
            .collect()
    }

    fn peak_concurrency(&self) -> usize {
        self.state.peak.load(Ordering::Acquire)
    }

    /// 任意时刻同时处于服务中的请求数的最大值。
    fn max_overlapping_requests(&self) -> usize {
        let timeline = self.state.timeline.lock().unwrap();
        let mut points: Vec<(Instant, i64)> = Vec::new();
        for (_, start, end) in timeline.iter() {
            points.push((*start, 1));
            points.push((*end, -1));
        }
        points.sort_by_key(|(instant, _)| *instant);
        let mut current = 0_i64;
        let mut peak = 0_i64;
        for (_, delta) in points {
            current += delta;
            peak = peak.max(current);
        }
        peak as usize
    }
}

fn serve_range(mut stream: TcpStream, state: &Arc<ServerState>) {
    let current = state.concurrent.fetch_add(1, Ordering::AcqRel) + 1;
    state.peak.fetch_max(current, Ordering::AcqRel);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        serve_range_inner(&mut stream, state);
    }));
    state.concurrent.fetch_sub(1, Ordering::AcqRel);
    let _ = result;
}

fn serve_range_inner(stream: &mut TcpStream, state: &Arc<ServerState>) {
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
    let body = match state.routes.iter().find(|(route, _)| route == path) {
        Some((_, body)) => body,
        None => {
            write!(
                stream,
                "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            )
            .unwrap();
            return;
        }
    };
    let range = request.lines().find_map(|line| {
        let value = line
            .strip_prefix("Range: bytes=")
            .or_else(|| line.strip_prefix("range: bytes="))?;
        let (start, end) = value.split_once('-')?;
        Some((start.parse::<u64>().ok()?, end.parse::<u64>().ok()?))
    });
    state.requests.lock().unwrap().push(range);
    let etag_header = state
        .options
        .etag
        .as_ref()
        .map(|etag| format!("ETag: {etag}\r\n"))
        .unwrap_or_default();
    if state.options.honor_range
        && let Some((start, end)) = range
    {
        let chunk = &body[start as usize..=end as usize];
        let started = Instant::now();
        write!(
            stream,
            "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {start}-{end}/{}\r\n{etag_header}Connection: close\r\n\r\n",
            chunk.len(),
            body.len()
        )
        .unwrap();
        write_throttled(stream, chunk, state.options.throttle);
        state
            .timeline
            .lock()
            .unwrap()
            .push((path.to_owned(), started, Instant::now()));
        return;
    }
    let started = Instant::now();
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n{etag_header}Connection: close\r\n\r\n",
        body.len()
    )
    .unwrap();
    write_throttled(stream, body, state.options.throttle);
    state
        .timeline
        .lock()
        .unwrap()
        .push((path.to_owned(), started, Instant::now()));
}

fn write_throttled(stream: &mut TcpStream, body: &[u8], throttle: Option<(usize, u64)>) {
    match throttle {
        Some((chunk_size, sleep_ms)) => {
            for chunk in body.chunks(chunk_size) {
                if stream.write_all(chunk).is_err() {
                    return;
                }
                let _ = stream.flush();
                thread::sleep(Duration::from_millis(sleep_ms));
            }
        }
        None => {
            let _ = stream.write_all(body);
            let _ = stream.flush();
        }
    }
}
