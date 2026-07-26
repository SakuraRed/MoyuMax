use std::{io::Write, net::TcpListener, thread};

use moyumax_core::{
    NetplayRoomConfig, easytier_args, parse_easytier_release, parse_stun_mapped_address,
    validate_room_name, validate_room_secret,
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
    let host = easytier_args(
        &NetplayRoomConfig {
            network_name: "room01".to_owned(),
            network_secret: "secret-123".to_owned(),
            is_host: true,
        },
        15991,
    );
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
    // no-TUN 架构：不建虚拟网卡，普通权限即可运行。
    assert!(host.contains(&"--no-tun".to_owned()));
    assert!(host.contains(&"--use-smoltcp".to_owned()));
    assert!(
        host.windows(2)
            .any(|pair| pair == ["--hostname", "H|MoyuMax"])
    );
    assert!(
        host.windows(2)
            .any(|pair| pair == ["--rpc-portal", "127.0.0.1:15991"])
    );

    let join = easytier_args(
        &NetplayRoomConfig {
            network_name: "room01".to_owned(),
            network_secret: "secret-123".to_owned(),
            is_host: false,
        },
        15992,
    );
    assert!(join.contains(&"--dhcp".to_owned()));
    assert!(!join.contains(&"--ipv4".to_owned()));
    assert!(
        join.windows(2)
            .any(|pair| pair == ["--hostname", "J|MoyuMax"])
    );
    assert!(
        join.windows(2)
            .any(|pair| pair == ["--private-mode", "true"])
    );
    assert!(
        join.windows(2)
            .any(|pair| pair == ["--relay-network-whitelist", "room01"])
    );
}

#[test]
fn net_003b_node_info_ipv4_parsing() {
    let assigned = serde_json::json!({"ipv4_addr": "10.144.144.2/24"});
    assert_eq!(
        moyumax_core::parse_easytier_node_ipv4(&assigned),
        Some("10.144.144.2".to_owned())
    );
    let pending = serde_json::json!({"ipv4_addr": ""});
    assert_eq!(moyumax_core::parse_easytier_node_ipv4(&pending), None);
    let missing = serde_json::json!({});
    assert_eq!(moyumax_core::parse_easytier_node_ipv4(&missing), None);
}

#[test]
fn net_003c_mc_lan_announcement_port_parsing() {
    assert_eq!(
        moyumax_core::parse_mc_lan_port("[MOTD]My World[/MOTD][AD]25565[/AD]"),
        Some(25565)
    );
    assert_eq!(moyumax_core::parse_mc_lan_port("garbage"), None);
    assert_eq!(moyumax_core::parse_mc_lan_port("[AD]notaport[/AD]"), None);
}

#[test]
fn net_007_peer_parsing_filters_local_and_public_server() {
    // 实测样例（EasyTier v2.6.4）：本机节点 cost=="Local"，公共服务器节点
    // hostname 以 PublicServer_ 开头，两者都不进入成员列表。
    let payload = serde_json::json!([
        {"cidr":"10.144.144.1/24","ipv4":"10.144.144.1","hostname":"H|MoyuMax","cost":"Local","lat_ms":"-","loss_rate":"-","rx_bytes":"-","tx_bytes":"-","tunnel_proto":"-","nat_type":"PortRestricted","id":"890732056","version":"2.6.4-8428a89d"},
        {"cidr":"","ipv4":"10.144.144.2","hostname":"J|MoyuMax","cost":"p2p","lat_ms":"23.4","loss_rate":"0","rx_bytes":"1","tx_bytes":"2","tunnel_proto":"tcp,tcp6","nat_type":"PortRestricted","id":"1","version":"2.6.4"},
        {"cidr":"","ipv4":"192.168.23.2","hostname":"PublicServer_hangzhou","cost":"relay(1)","lat_ms":"12.0","loss_rate":"0","rx_bytes":"1","tx_bytes":"2","tunnel_proto":"tcp","nat_type":"NoNat","id":"2","version":"2.6.4"},
        {"cidr":"","ipv4":"10.144.144.3","hostname":"J|MoyuMax","cost":"relay(2)","lat_ms":"-","loss_rate":"0","rx_bytes":"1","tx_bytes":"2","tunnel_proto":"udp","nat_type":"Symmetric","id":"3","version":"2.6.4"}
    ]);

    let peers = moyumax_core::parse_easytier_peers(&payload);

    assert_eq!(peers.len(), 2, "本机与公共服务器节点必须过滤");
    let p2p = &peers[0];
    assert_eq!(p2p.ipv4, "10.144.144.2");
    assert_eq!(p2p.hostname, "MoyuMax", "显示名必须去掉角色前缀");
    assert!(!p2p.is_host);
    assert_eq!(p2p.latency_ms, Some(23.4));
    assert_eq!(p2p.connection, "p2p");
    let relay = &peers[1];
    assert_eq!(relay.latency_ms, None, "lat_ms 非数字必须留空");
    assert_eq!(relay.connection, "relay", "非 p2p 一律记为中继");
}

#[test]
fn net_007b_peer_parsing_marks_host_by_hostname_prefix() {
    let payload = serde_json::json!([
        {"ipv4":"10.144.144.1","hostname":"H|MoyuMax","cost":"p2p","lat_ms":"18"},
        {"ipv4":"10.144.144.9","hostname":"other-machine","cost":"p2p","lat_ms":"42"}
    ]);

    let peers = moyumax_core::parse_easytier_peers(&payload);

    assert!(peers[0].is_host, "H| 前缀必须判定为主机");
    assert_eq!(peers[0].hostname, "MoyuMax");
    assert!(!peers[1].is_host, "无前缀节点按成员处理");
    assert_eq!(peers[1].hostname, "other-machine");
    // 非数组输入安全返回空列表（RPC 异常输出不panic）。
    assert!(moyumax_core::parse_easytier_peers(&serde_json::json!({})).is_empty());
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
    moyumax_core::download_and_verify(&client, &asset, &staging, &|_, _| {})
        .await
        .unwrap();
    assert_eq!(std::fs::read(&staging).unwrap(), zip_bytes);

    let tampered = moyumax_core::EasyTierAsset {
        sha256: "0".repeat(64),
        ..asset
    };
    let staging2 = tools.path().join("pack2.part");
    let error = moyumax_core::download_and_verify(&client, &tampered, &staging2, &|_, _| {})
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

#[tokio::test]
#[ignore = "live:真实下载 EasyTier 与 wintun 验证完整链路"]
async fn live_easytier_download_and_wintun() {
    let client = moyumax_core::netplay_http_client().unwrap();
    let tools = TempDir::new().unwrap();
    let binary = moyumax_core::ensure_easytier_binary(&client, tools.path(), &|current, total| {
        eprintln!("progress {current}/{total}");
    })
    .await
    .unwrap();
    assert!(binary.is_file());
    assert!(binary.with_file_name("wintun.dll").is_file());
    assert!(binary.with_file_name(".verified").is_file());
    // 第二次调用必须复用,不再下载。
    let again = moyumax_core::ensure_easytier_binary(&client, tools.path(), &|_, _| {})
        .await
        .unwrap();
    assert_eq!(again, binary);
}
