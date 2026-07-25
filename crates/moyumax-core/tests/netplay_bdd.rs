use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    thread,
};

use moyumax_core::{
    NetplayRoomConfig, easytier_args, parse_easytier_release, parse_stun_mapped_address,
    spawn_port_forward, validate_room_name, validate_room_secret,
};
use sha2::{Digest, Sha256};
use tempfile::TempDir;

#[test]
fn net_001_release_parsing_extracts_windows_asset_and_digest() {
    let payload = serde_json::json!({
        "tag_name": "v2.4.2",
        "assets": [
            {
                "name": "easytier-linux-x86_64-v2.4.2.zip",
                "browser_download_url": "https://example.com/linux.zip",
                "digest": "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "size": 100
            },
            {
                "name": "easytier-windows-x86_64-v2.4.2.zip",
                "browser_download_url": "https://example.com/win.zip",
                "digest": "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                "size": 200
            }
        ]
    });

    let asset = parse_easytier_release(&payload).unwrap();

    assert_eq!(asset.version, "v2.4.2");
    assert_eq!(asset.url, "https://example.com/win.zip");
    assert_eq!(asset.sha256, "b".repeat(64));
    assert_eq!(asset.size, 200);

    let missing = serde_json::json!({"tag_name": "v1", "assets": []});
    assert!(parse_easytier_release(&missing).is_err());
}

#[test]
fn net_002_room_name_and_secret_validation() {
    assert_eq!(validate_room_name(" my-room_01 ").unwrap(), "my-room_01");
    assert!(validate_room_name("abc").is_err(), "过短必须拒绝");
    assert!(validate_room_name("有中文").is_err(), "非 ASCII 必须拒绝");
    assert!(validate_room_name(&"a".repeat(33)).is_err(), "过长必须拒绝");
    assert_eq!(validate_room_secret("secret-123").unwrap(), "secret-123");
    assert!(validate_room_secret("short").is_err());
    assert!(
        validate_room_secret("has space here").is_err(),
        "空格必须拒绝"
    );
}

#[test]
fn net_003_easytier_args_host_static_ip_and_join_dhcp() {
    let host = easytier_args(&NetplayRoomConfig {
        network_name: "room01".to_owned(),
        network_secret: "secret-123".to_owned(),
        is_host: true,
    });
    assert!(
        host.windows(2)
            .any(|pair| pair == ["--ipv4", "10.144.144.1"])
    );
    assert!(
        host.windows(2)
            .any(|pair| pair == ["--network-name", "room01"])
    );
    assert!(
        host.windows(2)
            .any(|pair| pair == ["--network-secret", "secret-123"])
    );

    let join = easytier_args(&NetplayRoomConfig {
        network_name: "room01".to_owned(),
        network_secret: "secret-123".to_owned(),
        is_host: false,
    });
    assert!(join.contains(&"--dhcp".to_owned()));
    assert!(!join.contains(&"--ipv4".to_owned()));
}

#[test]
fn net_004_stun_xor_mapped_address_parsing() {
    let request_id = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12];
    let port: u16 = 54321 ^ 0x2112;
    let ip = [203 ^ 0x21, 0x12, 113 ^ 0xa4, 55 ^ 0x42];
    let mut response = vec![0x01, 0x01, 0x00, 0x0c, 0x21, 0x12, 0xa4, 0x42];
    response.extend_from_slice(&request_id);
    response.extend_from_slice(&[0x00, 0x20, 0x00, 0x08, 0x00, 0x01]);
    response.extend_from_slice(&port.to_be_bytes());
    response.extend_from_slice(&ip);

    let mapped = parse_stun_mapped_address(&request_id, &response).unwrap();

    assert_eq!(mapped.ip().to_string(), "203.0.113.55");
    assert_eq!(mapped.port(), 54321);

    let wrong_id = [9_u8; 12];
    assert!(parse_stun_mapped_address(&wrong_id, &response).is_err());
    assert!(parse_stun_mapped_address(&request_id, &[0u8; 4]).is_err());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn net_005_port_forward_bridges_tcp_traffic() {
    // 回环 echo 服务器
    let echo = TcpListener::bind("127.0.0.1:0").unwrap();
    let echo_addr = echo.local_addr().unwrap();
    thread::spawn(move || {
        for stream in echo.incoming() {
            let Ok(mut stream) = stream else { break };
            thread::spawn(move || {
                let mut buffer = [0_u8; 256];
                while let Ok(read) = stream.read(&mut buffer) {
                    if read == 0 {
                        break;
                    }
                    if stream.write_all(&buffer[..read]).is_err() {
                        break;
                    }
                }
            });
        }
    });
    // 取一个空闲端口作为转发监听(避免固定端口与历史进程冲突)
    let forward_addr = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap();
    let handle = spawn_port_forward(forward_addr, echo_addr).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    let mut client = TcpStream::connect(forward_addr).unwrap();
    client.write_all(b"ping-easytier").unwrap();
    let mut buffer = [0_u8; 32];
    let read = client.read(&mut buffer).unwrap();

    assert_eq!(&buffer[..read], b"ping-easytier");
    handle.abort();
}

#[tokio::test]
async fn net_006_easytier_download_verifies_sha256_before_install() {
    let zip_bytes = {
        let payload = b"fake-easytier-core-binary".to_vec();
        let mut cursor = std::io::Cursor::new(Vec::new());
        let mut writer = zip::ZipWriter::new(&mut cursor);
        let options = zip::write::SimpleFileOptions::default();
        writer.start_file("easytier-core.exe", options).unwrap();
        writer.write_all(&payload).unwrap();
        writer.finish().unwrap();
        cursor.into_inner()
    };
    let zip_sha256 = encode_hex(Sha256::digest(&zip_bytes));
    let api = spawn_zip_server(zip_bytes.clone());
    let tools = TempDir::new().unwrap();
    let client = reqwest::Client::new();

    let asset = moyumax_core::EasyTierAsset {
        version: "v9.9.9".to_owned(),
        url: format!("{api}/pack.zip"),
        sha256: zip_sha256,
        size: zip_bytes.len() as u64,
    };
    let staging = tools.path().join("pack.part");
    moyumax_core::download_and_verify(&client, &asset, &staging)
        .await
        .unwrap();
    assert_eq!(std::fs::read(&staging).unwrap(), zip_bytes);

    let tampered = moyumax_core::EasyTierAsset {
        sha256: "0".repeat(64),
        ..asset
    };
    let staging2 = tools.path().join("pack2.part");
    let error = moyumax_core::download_and_verify(&client, &tampered, &staging2)
        .await
        .expect_err("摘要不符必须拒绝");
    assert!(error.to_string().contains("SHA-256"), "{error}");
}

fn encode_hex(bytes: impl AsRef<[u8]>) -> String {
    bytes
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn spawn_zip_server(zip: Vec<u8>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let mut reader = std::io::BufReader::new(stream.try_clone().unwrap());
            let mut request_line = String::new();
            let _ = std::io::BufRead::read_line(&mut reader, &mut request_line);
            loop {
                let mut line = String::new();
                let read = std::io::BufRead::read_line(&mut reader, &mut line).unwrap_or(0);
                if read == 0 || line == "\r\n" {
                    break;
                }
            }
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/zip\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                zip.len()
            )
            .unwrap();
            stream.write_all(&zip).unwrap();
            stream.flush().unwrap();
        }
    });
    format!("http://{address}")
}
