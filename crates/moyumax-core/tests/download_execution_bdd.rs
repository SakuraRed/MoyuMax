use std::{
    fs,
    io::{BufRead, BufReader, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    thread,
};

use moyumax_core::{
    ArchiveLimits, ArtifactDownloader, ArtifactKind, DownloadDisposition, ResolvedArtifact,
    extract_zip_safely,
};
use sha1::{Digest, Sha1};
use sha2::Sha512;
use tempfile::TempDir;
use zip::{ZipWriter, write::SimpleFileOptions};

#[tokio::test]
async fn m3_execute_002_range_source_resumes_existing_partial_file() {
    let body = b"0123456789abcdefghijklmnopqrstuvwxyz".to_vec();
    let server = TestServer::new(body.clone(), false);
    let fixture = DownloadFixture::new(&server.url, &body);
    let partial = fixture.partial_path();
    fs::create_dir_all(partial.parent().unwrap()).unwrap();
    fs::write(&partial, &body[..10]).unwrap();

    let result = fixture
        .downloader
        .fetch(&fixture.artifact, &fixture.staging, &fixture.shared)
        .await
        .expect("range download should resume");

    assert_eq!(result.disposition, DownloadDisposition::Resumed);
    assert_eq!(fs::read(result.staged_file).unwrap(), body);
    assert!(server.requests().iter().any(|request| {
        request
            .lines()
            .any(|line| line.eq_ignore_ascii_case("Range: bytes=10-"))
    }));
}

#[tokio::test]
async fn m3_execute_003_source_ignoring_range_restarts_without_appending() {
    let body = b"server returned the complete artifact".to_vec();
    let server = TestServer::new(body.clone(), true);
    let fixture = DownloadFixture::new(&server.url, &body);
    let partial = fixture.partial_path();
    fs::create_dir_all(partial.parent().unwrap()).unwrap();
    fs::write(&partial, b"stale-prefix").unwrap();

    let result = fixture
        .downloader
        .fetch(&fixture.artifact, &fixture.staging, &fixture.shared)
        .await
        .expect("full response should replace partial content");

    assert_eq!(result.disposition, DownloadDisposition::Restarted);
    assert_eq!(fs::read(result.staged_file).unwrap(), body);
}

#[tokio::test]
async fn m3_execute_004_checksum_failure_never_replaces_shared_file() {
    let expected = b"trusted bytes".to_vec();
    let returned = b"tampered byte".to_vec();
    let server = TestServer::new(returned.clone(), false);
    let fixture = DownloadFixture::new(&server.url, &expected);
    let shared_file = fixture.shared.join(&fixture.artifact.relative_path);
    fs::create_dir_all(shared_file.parent().unwrap()).unwrap();
    fs::write(&shared_file, b"existing shared file").unwrap();

    let error = fixture
        .downloader
        .fetch(&fixture.artifact, &fixture.staging, &fixture.shared)
        .await
        .expect_err("checksum mismatch must fail");

    assert!(error.to_string().contains("SHA-1"));
    assert_eq!(fs::read(shared_file).unwrap(), b"existing shared file");
    assert_eq!(fs::read(fixture.partial_path()).unwrap(), returned);
}

#[tokio::test]
async fn m5_install_001_sha512_mismatch_is_rejected_even_when_sha1_matches() {
    let body = b"sha512 must also match".to_vec();
    let server = TestServer::new(body.clone(), false);
    let mut fixture = DownloadFixture::new(&server.url, &body);
    fixture.artifact.sha512 = Some("00".repeat(64));

    let error = fixture
        .downloader
        .fetch(&fixture.artifact, &fixture.staging, &fixture.shared)
        .await
        .expect_err("SHA-512 mismatch must fail");

    assert!(error.to_string().contains("SHA-512"));
    assert_eq!(fs::read(fixture.partial_path()).unwrap(), body);
}

#[test]
fn m3_execute_005_java_archive_path_traversal_is_rejected() {
    let directory = TempDir::new().unwrap();
    let archive_path = directory.path().join("malicious.zip");
    let target = directory.path().join("java");
    let escaped = directory.path().join("escape.txt");
    let archive = fs::File::create(&archive_path).unwrap();
    let mut writer = ZipWriter::new(archive);
    writer
        .start_file("../escape.txt", SimpleFileOptions::default())
        .unwrap();
    writer.write_all(b"must not escape").unwrap();
    writer.finish().unwrap();

    let error = extract_zip_safely(&archive_path, &target, ArchiveLimits::java_default())
        .expect_err("parent traversal must be rejected");

    assert!(error.to_string().contains("路径"));
    assert!(!escaped.exists());
}

struct DownloadFixture {
    _directory: TempDir,
    staging: PathBuf,
    shared: PathBuf,
    artifact: ResolvedArtifact,
    downloader: ArtifactDownloader,
}

impl DownloadFixture {
    fn new(url: &str, expected: &[u8]) -> Self {
        let directory = TempDir::new().unwrap();
        let staging = directory.path().join("staging");
        let shared = directory.path().join("shared");
        Self {
            artifact: ResolvedArtifact {
                kind: ArtifactKind::GameClient,
                relative_path: "minecraft/versions/test/test.jar".to_owned(),
                url: url.to_owned(),
                size: u64::try_from(expected.len()).unwrap(),
                sha1: Some(sha1(expected)),
                sha256: None,
                sha512: Some(sha512(expected)),
            },
            downloader: ArtifactDownloader::new(2).unwrap(),
            _directory: directory,
            staging,
            shared,
        }
    }

    fn partial_path(&self) -> PathBuf {
        partial_path(&self.staging.join(&self.artifact.relative_path))
    }
}

fn partial_path(staged_file: &Path) -> PathBuf {
    let extension = staged_file
        .extension()
        .and_then(|value| value.to_str())
        .map_or_else(|| "part".to_owned(), |value| format!("{value}.part"));
    staged_file.with_extension(extension)
}

fn sha1(bytes: &[u8]) -> String {
    let mut hasher = Sha1::new();
    hasher.update(bytes);
    encode_hex(hasher.finalize())
}

fn sha512(bytes: &[u8]) -> String {
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

struct TestServer {
    url: String,
    requests: Arc<Mutex<Vec<String>>>,
    _thread: thread::JoinHandle<()>,
}

impl TestServer {
    fn new(body: Vec<u8>, ignore_range: bool) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let captured = Arc::clone(&requests);
        let server_thread = thread::spawn(move || {
            if let Ok((stream, _)) = listener.accept() {
                serve_request(stream, &body, ignore_range, &captured);
            }
        });
        Self {
            url: format!("http://{address}/artifact.bin"),
            requests,
            _thread: server_thread,
        }
    }

    fn requests(&self) -> Vec<String> {
        self.requests.lock().unwrap().clone()
    }
}

fn serve_request(
    mut stream: TcpStream,
    body: &[u8],
    ignore_range: bool,
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
    requests.lock().unwrap().push(request.clone());
    let range_start = request.lines().find_map(|line| {
        line.strip_prefix("Range: bytes=")
            .or_else(|| line.strip_prefix("range: bytes="))
            .and_then(|value| value.trim_end_matches('-').parse::<usize>().ok())
    });
    let (status, response_body, extra_header) = if !ignore_range {
        if let Some(start) = range_start {
            (
                "206 Partial Content",
                &body[start..],
                format!(
                    "Content-Range: bytes {start}-{}/{}\r\n",
                    body.len() - 1,
                    body.len()
                ),
            )
        } else {
            ("200 OK", body, String::new())
        }
    } else {
        ("200 OK", body, String::new())
    };
    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Length: {}\r\n{extra_header}Connection: close\r\n\r\n",
        response_body.len()
    )
    .unwrap();
    stream.write_all(response_body).unwrap();
    stream.flush().unwrap();
}
