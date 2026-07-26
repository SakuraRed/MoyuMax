//! 联机能力：EasyTier 组网（房间号/密码）、STUN 简化 NAT 检测、本地端口转发。
//!
//! EasyTier 二进制经 GitHub Releases 下载：资产元数据（含 sha256 digest）取自
//! GitHub API，下载后强制 SHA-256 校验再解包到受管工具目录；校验失败即拒绝，
//! 绝不使用未校验的二进制。端口转发只做本机 TCP 转发；绑定非回环地址属风险
//! 操作，由桌面层在授权确认后显式传入。

use std::{
    fs,
    io::Read as _,
    net::{Ipv4Addr, SocketAddr, UdpSocket},
    path::{Path, PathBuf},
    time::Duration,
};

use futures_util::StreamExt;
use reqwest::Client;
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::{CoreError, Result};

/// GitHub Releases 元数据（EasyTier 官方仓库）。
pub const EASYTIER_RELEASE_API: &str =
    "https://api.github.com/repos/EasyTier/EasyTier/releases/latest";
/// EasyTier 社区共享公共节点（无公网 IP 组网，用于节点发现与中转）。
///
/// 官方共享节点 `public.easytier.top/.cn` 已停止解析（2026-07 实测 NXDOMAIN），
/// 这里采用社区公开的可中转节点（与 PCL-CE 相同做法：多节点冗余，全部连上，
/// 任一共通节点即可完成发现）。节点来源：AstralGame 社区服务器列表。
pub const EASYTIER_PUBLIC_PEERS: [&str; 3] = [
    "tcp://et.gbc.moe:11010",
    "tcp://easytier.weiai.org.cn:11010",
    "tcp://ros.scpsl.com.cn:11010",
];
/// wintun 官方预编译包（签名 DLL，官方许可允许随软件分发）。
pub const WINTUN_URL: &str = "https://www.wintun.net/builds/wintun-0.14.1.zip";
/// wintun 0.14.1 官方页面公布的 SHA-256（https://www.wintun.net/）。
pub const WINTUN_SHA256: &str = "07c256185d6ee3652e09fa55c0b673e2624b565e02c4b9091c79ca7d2f24ef51";
/// Windows x86_64 资产名前缀。
const EASYTIER_ASSET_PREFIX: &str = "easytier-windows-x86_64-";
/// STUN 服务器（Google 公共 STUN）。
const STUN_SERVER: &str = "stun.l.google.com:19302";
const STUN_TIMEOUT: Duration = Duration::from_secs(3);

/// EasyTier Windows 资产（URL、SHA-256、大小、版本）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EasyTierAsset {
    pub version: String,
    pub url: String,
    pub sha256: String,
    pub size: u64,
}

/// 联机房间配置（房间号/密码，主机或加入）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetplayRoomConfig {
    pub network_name: String,
    pub network_secret: String,
    pub is_host: bool,
}

/// 简化 NAT 检测报告。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NatReport {
    /// STUN 观测到的公网映射地址（含端口）。
    pub mapped_address: String,
    /// 映射地址是否与本机出站地址不一致（在 NAT 之后）。
    pub behind_nat: bool,
    /// 对联机影响的说明（简化检测结论，非完整 NAT 分类）。
    pub impact: &'static str,
}

/// 解析 GitHub Releases 元数据，取 Windows x86_64 资产与其 sha256。
pub fn parse_easytier_release(payload: &serde_json::Value) -> Result<EasyTierAsset> {
    let version = payload
        .get("tag_name")
        .and_then(|value| value.as_str())
        .ok_or_else(|| CoreError::Content("EasyTier 发布元数据缺少版本号".to_owned()))?
        .to_owned();
    let assets = payload
        .get("assets")
        .and_then(|value| value.as_array())
        .ok_or_else(|| CoreError::Content("EasyTier 发布元数据缺少资产列表".to_owned()))?;
    for asset in assets {
        let name = asset
            .get("name")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        if !name.starts_with(EASYTIER_ASSET_PREFIX) || !name.ends_with(".zip") {
            continue;
        }
        let url = asset
            .get("browser_download_url")
            .and_then(|value| value.as_str())
            .ok_or_else(|| CoreError::Content("EasyTier 资产缺少下载地址".to_owned()))?
            .to_owned();
        let digest = asset
            .get("digest")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .strip_prefix("sha256:")
            .ok_or_else(|| CoreError::Content("EasyTier 资产缺少 SHA-256 摘要".to_owned()))?;
        if digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(CoreError::Content(
                "EasyTier 资产 SHA-256 摘要无效".to_owned(),
            ));
        }
        let size = asset
            .get("size")
            .and_then(|value| value.as_u64())
            .unwrap_or(0);
        return Ok(EasyTierAsset {
            version,
            url,
            sha256: digest.to_owned(),
            size,
        });
    }
    Err(CoreError::Content(
        "EasyTier 发布中没有 Windows x86_64 资产".to_owned(),
    ))
}

/// 下载 EasyTier 并解包到受管工具目录，返回 easytier-core.exe 路径。
/// 同时放置 wintun.dll（EasyTier 优先加载同目录 wintun，免装 Npcap、免管理员）。
/// 已安装且标记摘要匹配时直接复用；下载进度经回调上报（已下载字节、总字节）。
pub async fn ensure_easytier_binary(
    client: &Client,
    tools_dir: &Path,
    progress: &(dyn Fn(u64, u64) + Send + Sync),
) -> Result<PathBuf> {
    let release: serde_json::Value = client
        .get(EASYTIER_RELEASE_API)
        .send()
        .await
        .map_err(|error| CoreError::Content(format!("无法获取 EasyTier 发布信息：{error}")))?
        .json()
        .await
        .map_err(|error| CoreError::Content(format!("EasyTier 发布信息无法解析：{error}")))?;
    let asset = parse_easytier_release(&release)?;
    let install_dir = tools_dir.join("easytier").join(&asset.version);
    let binary = install_dir.join("easytier-core.exe");
    let marker = install_dir.join(".verified");
    let wintun = install_dir.join("wintun.dll");
    let packet = install_dir.join("Packet.dll");
    // 复用条件：core、wintun 与 Packet.dll 都在,且标记摘要与发布摘要一致。
    // (此前误用 zip 摘要直接比对 exe,必然失配导致每次重下;
    //  解包名单曾漏掉包内自带的 Packet.dll,导致运行时缺失报错。)
    if binary.is_file()
        && wintun.is_file()
        && packet.is_file()
        && fs::read_to_string(&marker)
            .map(|content| content.trim() == asset.sha256)
            .unwrap_or(false)
    {
        return Ok(binary);
    }
    let staging = install_dir.join("download.zip.part");
    fs::create_dir_all(&install_dir)?;
    let download_result = download_and_verify(client, &asset, &staging, progress).await;
    if let Err(error) = download_result {
        let _ = fs::remove_file(&staging);
        return Err(error);
    }
    extract_easytier(&staging, &install_dir)?;
    let _ = fs::remove_file(&staging);
    if !binary.is_file() {
        return Err(CoreError::Content(
            "EasyTier 包内缺少 easytier-core.exe".to_owned(),
        ));
    }
    ensure_wintun_dll(client, &install_dir).await?;
    fs::write(&marker, format!("{}\n", asset.sha256))?;
    // zip 包的 SHA-256 已在下载时校验；解包出的 exe 与包摘要不同属正常,
    // 不再二次比较（此前的"解包后校验"误用了 zip 摘要,必然失败)。
    Ok(binary)
}

/// 下载 wintun.dll（固定版本 + 官方公布 SHA-256）到 EasyTier 同目录。
/// EasyTier 官方包已自带 wintun.dll 时跳过（优先用包内配套版本）。
async fn ensure_wintun_dll(client: &Client, install_dir: &Path) -> Result<PathBuf> {
    let target = install_dir.join("wintun.dll");
    if target.is_file() {
        return Ok(target);
    }
    let staging = install_dir.join("wintun.zip.part");
    let asset = EasyTierAsset {
        version: "0.14.1".to_owned(),
        url: WINTUN_URL.to_owned(),
        sha256: WINTUN_SHA256.to_owned(),
        size: 0,
    };
    let outcome = download_and_verify(client, &asset, &staging, &|_, _| {}).await;
    if let Err(error) = outcome {
        let _ = fs::remove_file(&staging);
        return Err(error);
    }
    let file = fs::File::open(&staging)?;
    let mut zip = zip::ZipArchive::new(file).map_err(zip_error)?;
    let mut found = false;
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index).map_err(zip_error)?;
        let Some(name) = entry.enclosed_name() else {
            continue;
        };
        if name.to_string_lossy().replace('\\', "/") != "wintun/bin/amd64/wintun.dll" {
            continue;
        }
        let mut data = Vec::with_capacity(entry.size() as usize);
        entry.read_to_end(&mut data)?;
        fs::write(&target, &data)?;
        found = true;
        break;
    }
    let _ = fs::remove_file(&staging);
    if !found {
        return Err(CoreError::Content(
            "wintun 包内缺少 amd64/wintun.dll".to_owned(),
        ));
    }
    Ok(target)
}

/// 联机下载用的 HTTP 客户端（带项目 UA 与超时）。
pub fn netplay_http_client() -> Result<Client> {
    crate::http_client_builder()
        .user_agent(format!(
            "SakuraRed/MoyuMax/{} (github.com/SakuraRed/MoyuMax)",
            env!("CARGO_PKG_VERSION")
        ))
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(180))
        .build()
        .map_err(CoreError::Network)
}

/// 下载资产到暂存文件并强制 SHA-256 校验；失败即拒绝且调用方负责清理。
/// 进度回调（已下载字节、总字节，总量未知时为 0）。
pub async fn download_and_verify(
    client: &Client,
    asset: &EasyTierAsset,
    staging: &Path,
    progress: &(dyn Fn(u64, u64) + Send + Sync),
) -> Result<()> {
    let response = client
        .get(&asset.url)
        .send()
        .await
        .map_err(|error| CoreError::Content(format!("EasyTier 下载失败：{error}")))?;
    if !response.status().is_success() {
        return Err(CoreError::Content(format!(
            "EasyTier 下载失败（HTTP {}）",
            response.status()
        )));
    }
    let total = response.content_length().unwrap_or(asset.size);
    let mut hasher = Sha256::new();
    let mut writer = std::io::BufWriter::new(fs::File::create(staging)?);
    let mut stream = response.bytes_stream();
    let mut received = 0_u64;
    progress(0, total);
    while let Some(chunk) = stream.next().await {
        let chunk =
            chunk.map_err(|error| CoreError::Content(format!("EasyTier 下载中断：{error}")))?;
        hasher.update(&chunk);
        std::io::Write::write_all(&mut writer, &chunk)?;
        received += chunk.len() as u64;
        progress(received, total);
    }
    std::io::Write::flush(&mut writer)?;
    let digest = encode_hex(hasher.finalize());
    if !digest.eq_ignore_ascii_case(&asset.sha256) {
        return Err(CoreError::Content(
            "EasyTier 下载 SHA-256 校验失败，已拒绝使用".to_owned(),
        ));
    }
    Ok(())
}

fn extract_easytier(archive: &Path, target: &Path) -> Result<()> {
    let file = fs::File::open(archive)?;
    let mut zip = zip::ZipArchive::new(file).map_err(zip_error)?;
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index).map_err(zip_error)?;
        let Some(name) = entry.enclosed_name() else {
            continue;
        };
        let file_name = name
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("");
        // EasyTier 官方包自带 Packet.dll 与 wintun.dll,必须一并解出,
        // 否则运行时动态加载 packet.dll 失败。
        if !matches!(
            file_name,
            "easytier-core.exe" | "easytier-cli.exe" | "Packet.dll" | "wintun.dll"
        ) {
            continue;
        }
        let mut data = Vec::with_capacity(entry.size() as usize);
        entry.read_to_end(&mut data)?;
        fs::write(target.join(file_name), &data)?;
    }
    Ok(())
}

fn encode_hex(bytes: impl AsRef<[u8]>) -> String {
    bytes
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn zip_error(error: zip::result::ZipError) -> CoreError {
    CoreError::Archive(format!("EasyTier 包无法解包：{error}"))
}

/// 校验房间号：4-32 位字母、数字、连字符或下划线。
pub fn validate_room_name(name: &str) -> Result<String> {
    let trimmed = name.trim();
    if !(4..=32).contains(&trimmed.len())
        || !trimmed
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(CoreError::Content(
            "房间号必须是 4-32 位字母、数字、连字符或下划线".to_owned(),
        ));
    }
    Ok(trimmed.to_owned())
}

/// 校验房间密码：8-64 位可见 ASCII。
pub fn validate_room_secret(secret: &str) -> Result<String> {
    if !(8..=64).contains(&secret.len())
        || !secret.bytes().all(|byte| (0x21..=0x7e).contains(&byte))
    {
        return Err(CoreError::Content(
            "房间密码必须是 8-64 位可见字符（不含空格）".to_owned(),
        ));
    }
    Ok(secret.to_owned())
}

/// 构造 easytier-core 启动参数。
///
/// 采用与 PCL-CE 相同的 no-TUN 架构（2026-07 实测：TUN 模式在普通权限下
/// 必然 "Failed to create adapter" 退出，房间瞬间消失；no-TUN + smoltcp
/// 用户态协议栈无需管理员即可工作）：
/// - `--no-tun --use-smoltcp --enable-kcp-proxy --enable-quic-proxy`：不建虚拟网卡，
///   连接经用户态栈代理；客机用 `port-forward` 把本机回环端口映射到主机虚拟 IP:端口。
/// - 主机固定 10.144.144.1；加入者 DHCP 自动分配，实际地址经 RPC `node info` 读回。
/// - `--private-mode` 只允许同密钥节点；`--relay-network-whitelist` 限制中转范围。
/// - `--rpc-portal` 固定到 127.0.0.1 的随机空闲端口，避免与本机其他 EasyTier 实例冲突。
/// - `--hostname` 以 `H|`/`J|` 前缀标记主机/客机角色，成员列表据此判定角色。
pub fn easytier_args(config: &NetplayRoomConfig, rpc_port: u16) -> Vec<String> {
    let mut args = vec![
        "--no-tun".to_owned(),
        "--use-smoltcp".to_owned(),
        "--enable-kcp-proxy".to_owned(),
        "--enable-quic-proxy".to_owned(),
        "--private-mode".to_owned(),
        "true".to_owned(),
        "--relay-network-whitelist".to_owned(),
        config.network_name.clone(),
        "--network-name".to_owned(),
        config.network_name.clone(),
        "--network-secret".to_owned(),
        config.network_secret.clone(),
    ];
    for peer in EASYTIER_PUBLIC_PEERS {
        args.push("-p".to_owned());
        args.push(peer.to_owned());
    }
    if config.is_host {
        args.push("--ipv4".to_owned());
        args.push("10.144.144.1".to_owned());
    } else {
        args.push("--dhcp".to_owned());
    }
    args.push("--hostname".to_owned());
    args.push(if config.is_host {
        "H|MoyuMax".to_owned()
    } else {
        "J|MoyuMax".to_owned()
    });
    args.push("--rpc-portal".to_owned());
    args.push(format!("127.0.0.1:{rpc_port}"));
    args
}

/// 从 `easytier-cli -o json node info` 输出中解析本机虚拟 IPv4（去掉 `/24` 后缀）。
/// DHCP 尚未分配时 `ipv4_addr` 为空串，返回 None。
pub fn parse_easytier_node_ipv4(payload: &serde_json::Value) -> Option<String> {
    let raw = payload.get("ipv4_addr")?.as_str()?;
    let ip = raw.split('/').next()?.trim();
    if ip.is_empty() {
        return None;
    }
    ip.parse::<Ipv4Addr>().ok()?;
    Some(ip.to_owned())
}

/// 联机房间成员视图（`easytier-cli peer` 解析后的非敏感信息）。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EasyTierPeerView {
    /// 成员的虚拟 IPv4。
    pub ipv4: String,
    /// 显示名（已去掉 `H|`/`J|` 角色前缀）。
    pub hostname: String,
    /// 是否房间主机（hostname 以 `H|` 前缀标记）。
    pub is_host: bool,
    /// 往返延迟（毫秒）；对端未上报时为空。
    pub latency_ms: Option<f64>,
    /// 连接方式：p2p 直连 / relay 中继（本机节点已在解析时过滤）。
    pub connection: String,
}

/// 解析 `easytier-cli -o json peer` 输出为房间成员列表。
/// 过滤本机节点（cost 为 Local）与公共服务器节点（hostname 以 `PublicServer_` 开头）；
/// cost 为 `p2p` 记为直连，其余记为中继；`lat_ms` 非数字时延迟留空。
pub fn parse_easytier_peers(payload: &serde_json::Value) -> Vec<EasyTierPeerView> {
    let Some(entries) = payload.as_array() else {
        return Vec::new();
    };
    entries.iter().filter_map(parse_easytier_peer).collect()
}

fn parse_easytier_peer(entry: &serde_json::Value) -> Option<EasyTierPeerView> {
    let cost = entry
        .get("cost")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    if cost.eq_ignore_ascii_case("local") {
        return None;
    }
    let raw_hostname = entry
        .get("hostname")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    if raw_hostname.starts_with("PublicServer_") {
        return None;
    }
    let (is_host, display) = if let Some(name) = raw_hostname.strip_prefix("H|") {
        (true, name)
    } else if let Some(name) = raw_hostname.strip_prefix("J|") {
        (false, name)
    } else {
        (false, raw_hostname)
    };
    let latency_ms = entry
        .get("lat_ms")
        .and_then(|value| value.as_str())
        .and_then(|text| text.parse::<f64>().ok());
    Some(EasyTierPeerView {
        ipv4: entry
            .get("ipv4")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .to_owned(),
        hostname: display.to_owned(),
        is_host,
        latency_ms,
        connection: if cost == "p2p" { "p2p" } else { "relay" }.to_owned(),
    })
}

/// 在本机回环上取一个空闲 TCP 端口（用于 RPC 门户与客机转发绑定）。
pub fn find_free_tcp_port() -> Option<u16> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").ok()?;
    Some(listener.local_addr().ok()?.port())
}

/// 解析 Minecraft「对局域网开放」UDP 组播公告（224.0.2.60:4445），
/// 报文形如 `[MOTD]世界名[/MOTD][AD]25565[/AD]`，取 `[AD]` 中的端口。
pub fn parse_mc_lan_port(message: &str) -> Option<u16> {
    let start = message.find("[AD]")? + 4;
    let end = message[start..].find("[/AD]")? + start;
    message[start..end].trim().parse().ok()
}

/// 监听 MC 局域网组播公告（224.0.2.60:4445），每收到一个端口就回调一次。
/// 阻塞循环，调用方放到独立线程；`stop` 置位后在下一个读超时内退出。
pub fn listen_mc_lan_announcements(
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    on_port: impl Fn(u16) + Send + 'static,
) -> Result<std::thread::JoinHandle<()>> {
    use std::sync::atomic::Ordering;
    let socket = UdpSocket::bind("0.0.0.0:4445")?;
    socket.join_multicast_v4(&Ipv4Addr::new(224, 0, 2, 60), &Ipv4Addr::UNSPECIFIED)?;
    socket.set_read_timeout(Some(Duration::from_millis(500)))?;
    Ok(std::thread::spawn(move || {
        let mut buffer = [0_u8; 1024];
        while !stop.load(Ordering::Relaxed) {
            match socket.recv_from(&mut buffer) {
                Ok((received, _)) => {
                    let text = String::from_utf8_lossy(&buffer[..received]);
                    if let Some(port) = parse_mc_lan_port(&text) {
                        on_port(port);
                    }
                }
                Err(error)
                    if error.kind() == std::io::ErrorKind::WouldBlock
                        || error.kind() == std::io::ErrorKind::TimedOut => {}
                Err(_) => break,
            }
        }
    }))
}

/// 解析 STUN Binding Response，取 XOR-MAPPED-ADDRESS（IPv4）。
pub fn parse_stun_mapped_address(request_id: &[u8; 12], response: &[u8]) -> Result<SocketAddr> {
    if response.len() < 20 || response[0] != 0x01 || response[1] != 0x01 {
        return Err(CoreError::Content("STUN 响应无效".to_owned()));
    }
    let declared_length = u16::from_be_bytes([response[2], response[3]]) as usize;
    if response.len() < 20 + declared_length {
        return Err(CoreError::Content("STUN 响应长度不足".to_owned()));
    }
    if response[4..8] != [0x21, 0x12, 0xa4, 0x42] || &response[8..20] != request_id {
        return Err(CoreError::Content("STUN 响应与请求不匹配".to_owned()));
    }
    let mut offset = 20;
    while offset + 4 <= 20 + declared_length {
        let attr_type = u16::from_be_bytes([response[offset], response[offset + 1]]);
        let attr_length = u16::from_be_bytes([response[offset + 2], response[offset + 3]]) as usize;
        let value_start = offset + 4;
        if value_start + attr_length > response.len() {
            break;
        }
        if attr_type == 0x0020 && attr_length >= 8 && response[value_start + 1] == 0x01 {
            let port =
                u16::from_be_bytes([response[value_start + 2], response[value_start + 3]]) ^ 0x2112;
            let ip = Ipv4Addr::new(
                response[value_start + 4] ^ 0x21,
                response[value_start + 5] ^ 0x12,
                response[value_start + 6] ^ 0xa4,
                response[value_start + 7] ^ 0x42,
            );
            return Ok(SocketAddr::new(ip.into(), port));
        }
        if attr_type == 0x0001 && attr_length >= 8 && response[value_start + 1] == 0x01 {
            let port = u16::from_be_bytes([response[value_start + 2], response[value_start + 3]]);
            let ip = Ipv4Addr::new(
                response[value_start + 4],
                response[value_start + 5],
                response[value_start + 6],
                response[value_start + 7],
            );
            return Ok(SocketAddr::new(ip.into(), port));
        }
        offset = value_start + attr_length + ((4 - (attr_length % 4)) % 4);
    }
    Err(CoreError::Content("STUN 响应缺少映射地址".to_owned()))
}

/// 简化 NAT 检测：一次 STUN Binding 请求，比较映射地址与本机出站地址。
/// 诚实标注为简化结论（不是完整 NAT 分类）。同步阻塞实现，调用方放到阻塞线程池。
pub fn detect_nat() -> Result<NatReport> {
    let socket = UdpSocket::bind("0.0.0.0:0")?;
    socket.set_read_timeout(Some(STUN_TIMEOUT))?;
    socket.set_write_timeout(Some(STUN_TIMEOUT))?;
    socket.connect(STUN_SERVER)?;
    let local_address = socket.local_addr()?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos() as u64);
    let mut request_id = [0_u8; 12];
    request_id[..8].copy_from_slice(&now.to_be_bytes());
    request_id[8..].copy_from_slice(&(now as u32).to_be_bytes());
    let mut request = Vec::with_capacity(20);
    request.extend_from_slice(&[0x00, 0x01, 0x00, 0x00, 0x21, 0x12, 0xa4, 0x42]);
    request.extend_from_slice(&request_id);
    socket.send(&request)?;
    let mut buffer = [0_u8; 1024];
    let (received, _) = socket.recv_from(&mut buffer)?;
    let mapped = parse_stun_mapped_address(&request_id, &buffer[..received])?;
    let behind_nat = mapped.ip() != local_address.ip();
    Ok(NatReport {
        mapped_address: mapped.to_string(),
        behind_nat,
        impact: if behind_nat {
            "你在 NAT 之后，直连入站通常不可达；建议使用联机房间组网"
        } else {
            "你的公网映射地址与本机一致，具备直连条件（防火墙仍需放行）"
        },
    })
}
