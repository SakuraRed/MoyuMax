//! 实例服务器管理 BDD:servers.dat 读写往返、字段保留、地址校验、
//! 原子写入收敛,以及 Server List Ping 的本地 TCP 桩解析。

use std::{
    fs,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::PathBuf,
    thread,
    time::Duration,
};

use moyumax_core::AppService;
use rusqlite::{Connection, params};
use tempfile::TempDir;

#[test]
fn m34_srv_001_add_list_roundtrip_through_servers_dat() {
    let fixture = ServerFixture::new();
    assert!(
        fixture
            .service
            .list_instance_servers(&fixture.instance_id)
            .unwrap()
            .is_empty(),
        "没有 servers.dat 时必须为空列表"
    );

    let servers = fixture
        .service
        .add_instance_server(&fixture.instance_id, "大厅", "mc.example.com:25566")
        .unwrap();
    assert_eq!(servers.len(), 1);
    assert_eq!(servers[0].name, "大厅");
    assert_eq!(servers[0].address, "mc.example.com:25566");
    assert!(
        fixture.servers_dat().is_file(),
        "添加后必须生成 servers.dat"
    );

    // 重新读取文件,确认写回内容可解析且一致(读写往返)。
    let reread = fixture
        .service
        .list_instance_servers(&fixture.instance_id)
        .unwrap();
    assert_eq!(reread, servers);

    let servers = fixture
        .service
        .add_instance_server(&fixture.instance_id, "生存", "[::1]")
        .unwrap();
    assert_eq!(servers.len(), 2);
    assert_eq!(servers[1].address, "[::1]");
}

#[test]
fn m34_srv_002_existing_fields_and_unknown_tags_are_parsed() {
    let fixture = ServerFixture::new();
    fs::write(fixture.servers_dat(), raw_servers_dat()).unwrap();

    let servers = fixture
        .service
        .list_instance_servers(&fixture.instance_id)
        .unwrap();

    assert_eq!(servers.len(), 2, "缺 ip 的条目必须跳过");
    assert_eq!(servers[0].name, "服甲");
    assert_eq!(servers[0].address, "mc.example.com:25566");
    assert_eq!(servers[0].icon.as_deref(), Some("data:image/png;base64,AA"));
    assert_eq!(servers[0].accept_textures, Some(true));
    assert_eq!(servers[1].name, "服乙");
    assert_eq!(servers[1].accept_textures, None);
}

#[test]
fn m34_srv_003_update_preserves_icon_and_unknown_tags() {
    let fixture = ServerFixture::new();
    fs::write(fixture.servers_dat(), raw_servers_dat()).unwrap();

    let servers = fixture
        .service
        .update_instance_server(&fixture.instance_id, 0, "服甲改", "play.example.com")
        .unwrap();

    assert_eq!(servers[0].name, "服甲改");
    assert_eq!(servers[0].address, "play.example.com");
    assert_eq!(
        servers[0].icon.as_deref(),
        Some("data:image/png;base64,AA"),
        "更新名称/地址不得丢掉 icon"
    );
    assert_eq!(servers[0].accept_textures, Some(true));

    // 未识别字段(条目内 unknownI、根级 extraRoot)必须随原树回写。
    let bytes = fs::read(fixture.servers_dat()).unwrap();
    assert!(contains_bytes(&bytes, b"unknownI"), "条目内未知字段丢失");
    assert!(
        contains_bytes(&bytes, &42_i32.to_be_bytes()),
        "未知字段的值丢失"
    );
    assert!(contains_bytes(&bytes, b"extraRoot"), "根级未知字段丢失");
    assert!(contains_bytes(&bytes, b"keep-me"), "根级未知字段的值丢失");
}

#[test]
fn m34_srv_004_remove_by_index_and_out_of_range_rejected() {
    let fixture = ServerFixture::new();
    fs::write(fixture.servers_dat(), raw_servers_dat()).unwrap();

    let servers = fixture
        .service
        .remove_instance_server(&fixture.instance_id, 0)
        .unwrap();
    assert_eq!(servers.len(), 1);
    assert_eq!(servers[0].name, "服乙");

    let error = fixture
        .service
        .remove_instance_server(&fixture.instance_id, 5)
        .expect_err("越界序号必须拒绝");
    assert!(error.to_string().contains("序号"));
    assert_eq!(
        fixture
            .service
            .list_instance_servers(&fixture.instance_id)
            .unwrap()
            .len(),
        1,
        "拒绝后列表不得变化"
    );
}

#[test]
fn m34_srv_005_invalid_name_or_address_is_rejected_without_write() {
    let fixture = ServerFixture::new();
    fs::write(fixture.servers_dat(), raw_servers_dat()).unwrap();
    let before = fs::read(fixture.servers_dat()).unwrap();

    for address in [
        "",
        "   ",
        ":25565",
        "host:",
        "host:0",
        "host:65536",
        "host:abc",
        "host:-1",
        "::1",
        "has space.com",
        "[::1",
        "[::1]x",
    ] {
        let error = fixture
            .service
            .add_instance_server(&fixture.instance_id, "合法名称", address)
            .expect_err(&format!("地址 {address:?} 必须拒绝"));
        assert!(
            error.to_string().contains("无法管理实例服务器"),
            "错误信息须可读:{error}"
        );
    }
    for name in ["", "   "] {
        fixture
            .service
            .add_instance_server(&fixture.instance_id, name, "mc.example.com")
            .expect_err("空名称必须拒绝");
    }
    fixture
        .service
        .update_instance_server(&fixture.instance_id, 0, "服甲", "host:70000")
        .expect_err("更新时的非法地址也必须拒绝");

    assert_eq!(
        fs::read(fixture.servers_dat()).unwrap(),
        before,
        "任何校验失败都不得触碰 servers.dat"
    );
}

#[test]
fn m34_srv_006_write_leaves_no_partial_and_recovers_interrupted_state() {
    let fixture = ServerFixture::new();
    fixture
        .service
        .add_instance_server(&fixture.instance_id, "大厅", "mc.example.com")
        .unwrap();
    let partial = fixture
        .servers_dat()
        .parent()
        .unwrap()
        .join(".servers.dat.moyu-partial");
    let backup = fixture
        .servers_dat()
        .parent()
        .unwrap()
        .join(".servers.dat.moyu-backup");
    assert!(!partial.exists(), "完成后 partial 必须清理");
    assert!(!backup.exists(), "完成后 backup 必须清理");
    let committed = fs::read(fixture.servers_dat()).unwrap();

    // 模拟中断点:旧文件已改名 backup、新文件未就位 → 重启后必须恢复旧文件。
    fs::rename(fixture.servers_dat(), &backup).unwrap();
    let reopened = fixture.reopen();
    assert!(
        fixture.servers_dat().is_file(),
        "中断后 servers.dat 必须恢复"
    );
    assert!(!backup.exists(), "恢复后 backup 必须清理");
    assert_eq!(
        fs::read(fixture.servers_dat()).unwrap(),
        committed,
        "恢复内容必须是中断前的完整文件"
    );
    assert_eq!(
        reopened
            .list_instance_servers(&fixture.instance_id)
            .unwrap()
            .len(),
        1
    );

    // 模拟中断点:新文件已就位、backup 残留 → 重启后清理 backup,内容以新文件为准。
    fs::write(&backup, b"stale").unwrap();
    fs::write(&partial, b"partial").unwrap();
    fixture.reopen();
    assert!(!backup.exists(), "残留 backup 必须清理");
    assert!(!partial.exists(), "残留 partial 必须清理");
    assert_eq!(fs::read(fixture.servers_dat()).unwrap(), committed);
}

#[test]
fn m34_srv_007_ping_parses_modern_json_status() {
    let fixture = ServerFixture::new();
    let port = spawn_modern_stub(
        r#"{"version":{"name":"26.2","protocol":800},"players":{"max":20,"online":3},"description":"§aMoyuMax §7测试服"}"#
            .to_owned(),
    );

    let status = fixture
        .service
        .ping_minecraft_server(&format!("127.0.0.1:{port}"))
        .unwrap();

    assert!(status.online);
    assert_eq!(status.motd.as_deref(), Some("§aMoyuMax §7测试服"));
    assert_eq!(status.players_online, Some(3));
    assert_eq!(status.players_max, Some(20));
    assert_eq!(status.version_name.as_deref(), Some("26.2"));
    assert!(status.latency_ms.is_some());
}

#[test]
fn m34_srv_008_ping_flattens_chat_component_motd() {
    let fixture = ServerFixture::new();
    let port = spawn_modern_stub(
        r#"{"version":{"name":"1.20.1"},"players":{"max":100,"online":7},"description":{"text":"§l欢迎 ","extra":[{"text":"来到 "},{"text":"服务器"}]}}"#
            .to_owned(),
    );

    let status = fixture
        .service
        .ping_minecraft_server(&format!("127.0.0.1:{port}"))
        .unwrap();

    assert!(status.online);
    assert_eq!(status.motd.as_deref(), Some("§l欢迎 来到 服务器"));
    assert_eq!(status.players_online, Some(7));
}

#[test]
fn m34_srv_009_ping_falls_back_to_legacy_protocol() {
    let fixture = ServerFixture::new();
    // legacy 响应串:NUL 分隔的 §1、协议、版本、MOTD、在线、上限。
    let port = spawn_legacy_stub("§1\x0047\x001.6.4\x00§c老服务器\x003\x0010".to_owned());

    let status = fixture
        .service
        .ping_minecraft_server(&format!("127.0.0.1:{port}"))
        .unwrap();

    assert!(status.online, "现代协议失败后必须回退 legacy");
    assert_eq!(status.motd.as_deref(), Some("§c老服务器"));
    assert_eq!(status.version_name.as_deref(), Some("1.6.4"));
    assert_eq!(status.players_online, Some(3));
    assert_eq!(status.players_max, Some(10));
}

#[test]
fn m34_srv_010_ping_unreachable_returns_offline_not_error() {
    let fixture = ServerFixture::new();
    // 绑定后立即释放,保证端口处于拒连状态。
    let port = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();

    let status = fixture
        .service
        .ping_minecraft_server(&format!("127.0.0.1:{port}"))
        .unwrap();

    assert!(!status.online);
    assert_eq!(status.motd, None);
    assert_eq!(status.latency_ms, None);
}

#[test]
fn m34_srv_011_ping_rejects_invalid_address() {
    let fixture = ServerFixture::new();
    fixture
        .service
        .ping_minecraft_server("host:99999")
        .expect_err("非法端口必须拒绝");
    fixture
        .service
        .ping_minecraft_server("")
        .expect_err("空地址必须拒绝");
}

// ---------- 测试桩与工具 ----------

struct ServerFixture {
    // 仅持有以维持临时目录生命周期。
    _directory: TempDir,
    database_path: PathBuf,
    data_directory: PathBuf,
    instance_id: String,
    instance_root: PathBuf,
    service: AppService,
}

impl ServerFixture {
    fn new() -> Self {
        let directory = TempDir::new().unwrap();
        let database_path = directory.path().join("state.sqlite3");
        let data_directory = directory.path().join("data");
        let instance_id = "instance-id".to_owned();
        let instance_root = data_directory.join("instances").join(&instance_id);
        let service = AppService::open(&database_path, &data_directory).unwrap();
        service.skip_onboarding().unwrap();
        fs::create_dir_all(instance_root.join(".minecraft")).unwrap();
        Connection::open(&database_path)
            .unwrap()
            .execute(
                "
                INSERT INTO instances (
                    id, name, game_version, loader_kind, loader_version,
                    root_directory, state, created_at_unix_seconds
                ) VALUES (?1, '服务器测试', '26.2', 'fabric', '0.19.3', ?2, 'ready', 1)
                ",
                params![instance_id, instance_root.to_string_lossy()],
            )
            .unwrap();
        Self {
            _directory: directory,
            database_path,
            data_directory,
            instance_id,
            instance_root,
            service,
        }
    }

    fn servers_dat(&self) -> PathBuf {
        self.instance_root.join(".minecraft").join("servers.dat")
    }

    /// 重新打开服务,触发启动收敛(与真实重启路径一致)。
    fn reopen(&self) -> AppService {
        AppService::open(&self.database_path, &self.data_directory).unwrap()
    }
}

/// 手工拼字节(不经过被测写入路径),含 icon/acceptTextures、条目内未知
/// int 字段、根级未知字符串字段,以及一个缺 ip 必须被跳过的条目。
fn raw_servers_dat() -> Vec<u8> {
    let mut bytes = vec![0x0A, 0x00, 0x00]; // compound 根,空名
    bytes.push(0x09); // list "servers"
    push_utf(&mut bytes, "servers");
    bytes.push(0x0A); // 元素类型 compound
    bytes.extend_from_slice(&3_i32.to_be_bytes());
    // 条目 1:完整字段 + 未知 int
    push_string_tag(&mut bytes, "name", "服甲");
    push_string_tag(&mut bytes, "ip", "mc.example.com:25566");
    push_string_tag(&mut bytes, "icon", "data:image/png;base64,AA");
    bytes.push(0x01);
    push_utf(&mut bytes, "acceptTextures");
    bytes.push(0x01);
    bytes.push(0x03);
    push_utf(&mut bytes, "unknownI");
    bytes.extend_from_slice(&42_i32.to_be_bytes());
    bytes.push(0x00);
    // 条目 2:仅必需字段
    push_string_tag(&mut bytes, "name", "服乙");
    push_string_tag(&mut bytes, "ip", "192.168.1.2");
    bytes.push(0x00);
    // 条目 3:缺 ip,读取时必须跳过
    push_string_tag(&mut bytes, "name", "坏条目");
    bytes.push(0x00);
    // 根级未知字段
    push_string_tag(&mut bytes, "extraRoot", "keep-me");
    bytes.push(0x00);
    bytes
}

fn push_utf(bytes: &mut Vec<u8>, value: &str) {
    bytes.extend_from_slice(&(value.len() as u16).to_be_bytes());
    bytes.extend_from_slice(value.as_bytes());
}

fn push_string_tag(bytes: &mut Vec<u8>, name: &str, value: &str) {
    bytes.push(0x08);
    push_utf(bytes, name);
    push_utf(bytes, value);
}

fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

/// 现代协议桩:读 handshake 与 status request 两帧,回一帧 JSON 状态。
fn spawn_modern_stub(response_body: String) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        read_frame(&mut stream);
        read_frame(&mut stream);
        let mut packet = vec![0x00_u8];
        write_varint(&mut packet, response_body.len() as i32);
        packet.extend_from_slice(response_body.as_bytes());
        let mut frame = Vec::new();
        write_varint(&mut frame, packet.len() as i32);
        frame.extend_from_slice(&packet);
        stream.write_all(&frame).unwrap();
        stream.flush().unwrap();
    });
    port
}

/// legacy 协议桩:第一次连接(现代握手)直接关闭,第二次回 0xFF 踢出串。
fn spawn_legacy_stub(response: String) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    thread::spawn(move || {
        let (modern_attempt, _) = listener.accept().unwrap();
        drop(modern_attempt);
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut received = [0_u8; 512];
        let count = stream.read(&mut received).unwrap();
        assert_eq!(received[0], 0xFE, "legacy 请求首字节必须是 0xFE");
        assert!(count > 3);
        let units: Vec<u16> = response.encode_utf16().collect();
        let mut frame = vec![0xFF_u8];
        frame.extend_from_slice(&(units.len() as u16).to_be_bytes());
        for unit in units {
            frame.extend_from_slice(&unit.to_be_bytes());
        }
        stream.write_all(&frame).unwrap();
        stream.flush().unwrap();
    });
    port
}

fn read_frame(stream: &mut TcpStream) -> usize {
    let length = read_varint(stream) as usize;
    let mut remaining = length;
    let mut buffer = [0_u8; 4096];
    while remaining > 0 {
        let chunk = remaining.min(4096);
        let read = stream.read(&mut buffer[..chunk]).unwrap();
        remaining -= read;
    }
    length
}

fn read_varint(stream: &mut TcpStream) -> i32 {
    let mut result = 0_u32;
    let mut shift = 0_u32;
    loop {
        let mut byte = [0_u8; 1];
        stream.read_exact(&mut byte).unwrap();
        result |= u32::from(byte[0] & 0x7F) << shift;
        if byte[0] & 0x80 == 0 {
            return result as i32;
        }
        shift += 7;
    }
}

fn write_varint(out: &mut Vec<u8>, value: i32) {
    let mut remaining = value as u32;
    loop {
        if remaining & !0x7F == 0 {
            out.push(remaining as u8);
            break;
        }
        out.push((remaining & 0x7F) as u8 | 0x80);
        remaining >>= 7;
    }
}
