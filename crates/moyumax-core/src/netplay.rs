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

use reqwest::Client;
use sha2::{Digest, Sha256};

use crate::{CoreError, Result};

/// GitHub Releases 元数据（EasyTier 官方仓库）。
pub const EASYTIER_RELEASE_API: &str =
    "https://api.github.com/repos/EasyTier/EasyTier/releases/latest";
/// EasyTier 官方共享公共节点（无公网 IP 组网）。
pub const EASYTIER_PUBLIC_PEER: &str = "tcp://public.easytier.top:11010";
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
/// 已存在且 SHA-256 匹配时直接复用。
pub async fn ensure_easytier_binary(client: &Client, tools_dir: &Path) -> Result<PathBuf> {
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
    if binary.is_file() && sha256_file(&binary)? == asset.sha256 {
        return Ok(binary);
    }
    let staging = install_dir.join("download.zip.part");
    fs::create_dir_all(&install_dir)?;
    let download_result = download_and_verify(client, &asset, &staging).await;
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
    if sha256_file(&binary)? != asset.sha256 {
        let _ = fs::remove_dir_all(&install_dir);
        return Err(CoreError::Content(
            "EasyTier 解包后校验失败，已删除".to_owned(),
        ));
    }
    Ok(binary)
}

/// 联机下载用的 HTTP 客户端（带项目 UA 与超时）。
pub fn netplay_http_client() -> Result<Client> {
    Client::builder()
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
pub async fn download_and_verify(
    client: &Client,
    asset: &EasyTierAsset,
    staging: &Path,
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
    let bytes = response
        .bytes()
        .await
        .map_err(|error| CoreError::Content(format!("EasyTier 下载中断：{error}")))?;
    let digest = encode_hex(Sha256::digest(&bytes));
    if !digest.eq_ignore_ascii_case(&asset.sha256) {
        return Err(CoreError::Content(
            "EasyTier 下载 SHA-256 校验失败，已拒绝使用".to_owned(),
        ));
    }
    fs::write(staging, &bytes)?;
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
        if !matches!(file_name, "easytier-core.exe" | "easytier-cli.exe") {
            continue;
        }
        let mut data = Vec::with_capacity(entry.size() as usize);
        entry.read_to_end(&mut data)?;
        fs::write(target.join(file_name), &data)?;
    }
    Ok(())
}

fn sha256_file(path: &Path) -> Result<String> {
    let mut hasher = Sha256::new();
    let mut file = fs::File::open(path)?;
    let mut buffer = [0_u8; 65536];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(encode_hex(hasher.finalize()))
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

/// 构造 easytier-core 启动参数；主机固定 10.144.144.1，加入者 DHCP 自动分配。
pub fn easytier_args(config: &NetplayRoomConfig) -> Vec<String> {
    let mut args = vec![
        "--network-name".to_owned(),
        config.network_name.clone(),
        "--network-secret".to_owned(),
        config.network_secret.clone(),
        "-p".to_owned(),
        EASYTIER_PUBLIC_PEER.to_owned(),
    ];
    if config.is_host {
        args.push("--ipv4".to_owned());
        args.push("10.144.144.1".to_owned());
    } else {
        args.push("--dhcp".to_owned());
    }
    args
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

/// 本机 TCP 端口转发：接受 listen 上的连接并桥接到 target。
/// 返回 JoinHandle，由桌面层持有生命周期。
pub async fn spawn_port_forward(
    listen: SocketAddr,
    target: SocketAddr,
) -> Result<tokio::task::JoinHandle<()>> {
    if target.ip().is_unspecified() {
        return Err(CoreError::Content("转发目标必须指定具体地址".to_owned()));
    }
    let listener = tokio::net::TcpListener::bind(listen)
        .await
        .map_err(|error| CoreError::Content(format!("无法监听 {listen}：{error}")))?;
    let handle = tokio::spawn(async move {
        loop {
            let Ok((inbound, _)) = listener.accept().await else {
                break;
            };
            tokio::spawn(async move {
                if let Ok(outbound) = tokio::net::TcpStream::connect(target).await {
                    let (mut inbound_read, mut inbound_write) = inbound.into_split();
                    let (mut outbound_read, mut outbound_write) = outbound.into_split();
                    let client_to_server = tokio::io::copy(&mut inbound_read, &mut outbound_write);
                    let server_to_client = tokio::io::copy(&mut outbound_read, &mut inbound_write);
                    let _ = tokio::join!(client_to_server, server_to_client);
                }
            });
        }
    });
    Ok(handle)
}
