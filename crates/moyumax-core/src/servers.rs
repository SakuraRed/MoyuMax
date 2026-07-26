//! 实例服务器列表管理:读写实例 `.minecraft/servers.dat`(NBT),并提供
//! Minecraft Server List Ping(1.7+ JSON 协议,失败时回退 ≤1.6 legacy 协议)。
//!
//! 回写策略:解析整棵 NBT 树后只改动 `servers` 列表,根级与条目内未识别的
//! 字段随原树原样回写。写入走 partial→backup→rename 三步替换(Windows 上
//! rename 不能覆盖已存在目标,必须三步),任何失败都回滚到写入前状态,
//! 启动时收敛中断的替换。

use std::{
    fs,
    io::{Read, Write},
    net::{TcpStream, ToSocketAddrs},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};

use crate::{
    AppService, CoreError, ManagedInstanceSummary, Result,
    nbt::{self, NbtTag},
};

const PARTIAL_NAME: &str = ".servers.dat.moyu-partial";
const BACKUP_NAME: &str = ".servers.dat.moyu-backup";
const DEFAULT_PORT: u16 = 25565;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
const IO_TIMEOUT: Duration = Duration::from_secs(4);
const MAX_NAME_CHARS: usize = 64;
/// 状态响应 JSON 上限 1 MiB,防止恶意端点耗尽内存。
const MAX_STATUS_BYTES: usize = 1 << 20;
const MAX_LEGACY_STATUS_CHARS: usize = 4096;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceServerEntry {
    pub name: String,
    /// 服务器地址(NBT 中的 ip 字段),形如 host[:port]。
    pub address: String,
    /// 服务器图标(data:image/png;base64,...),读取时保留、回写时原样带上。
    pub icon: Option<String>,
    pub accept_textures: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MinecraftServerStatus {
    pub online: bool,
    /// MOTD 纯文本(保留 § 格式码;JSON chat 组件的 color/样式字段不映射)。
    pub motd: Option<String>,
    pub players_online: Option<u32>,
    pub players_max: Option<u32>,
    pub version_name: Option<String>,
    /// 从发起连接到收完状态响应的耗时,非游戏内真实 tick 延迟。
    pub latency_ms: Option<u64>,
}

impl MinecraftServerStatus {
    fn offline() -> Self {
        Self {
            online: false,
            motd: None,
            players_online: None,
            players_max: None,
            version_name: None,
            latency_ms: None,
        }
    }
}

impl AppService {
    /// 读取实例 servers.dat 中的服务器列表;文件不存在时为空列表。
    pub fn list_instance_servers(&self, instance_id: &str) -> Result<Vec<InstanceServerEntry>> {
        let instance = self.ready_instance(instance_id)?;
        let (_root_name, root) = read_servers_tree(&servers_dat_path(&instance))?;
        Ok(collect_entries(&root))
    }

    /// 追加一个服务器,返回写入后的完整列表。
    pub fn add_instance_server(
        &self,
        instance_id: &str,
        name: &str,
        address: &str,
    ) -> Result<Vec<InstanceServerEntry>> {
        let name = validate_server_name(name)?;
        let address = validate_server_address(address)?;
        self.mutate_servers(instance_id, |items| {
            items.push(server_compound(&name, &address));
            Ok(())
        })
    }

    /// 按序号删除服务器(序号即列表顺序),返回写入后的完整列表。
    pub fn remove_instance_server(
        &self,
        instance_id: &str,
        index: u32,
    ) -> Result<Vec<InstanceServerEntry>> {
        self.mutate_servers(instance_id, |items| {
            let raw = raw_index_of_entry(items, index as usize)
                .ok_or_else(|| invalid("服务器序号超出列表范围"))?;
            items.remove(raw);
            Ok(())
        })
    }

    /// 按序号更新服务器名称与地址,条目内其他字段(icon 等)原样保留。
    pub fn update_instance_server(
        &self,
        instance_id: &str,
        index: u32,
        name: &str,
        address: &str,
    ) -> Result<Vec<InstanceServerEntry>> {
        let name = validate_server_name(name)?;
        let address = validate_server_address(address)?;
        self.mutate_servers(instance_id, |items| {
            let raw = raw_index_of_entry(items, index as usize)
                .ok_or_else(|| invalid("服务器序号超出列表范围"))?;
            let NbtTag::Compound(fields) = &mut items[raw] else {
                return Err(invalid("服务器序号超出列表范围"));
            };
            set_string_field(fields, "name", &name);
            set_string_field(fields, "ip", &address);
            Ok(())
        })
    }

    /// 探测服务器状态:先尝试 1.7+ JSON 协议,失败后回退 legacy(≤1.6)。
    /// 地址非法返回 Err;连接失败/超时返回 online=false 而非 Err,由界面显示离线。
    pub fn ping_minecraft_server(&self, address: &str) -> Result<MinecraftServerStatus> {
        let (host, port) = parse_server_address(address)?;
        Ok(ping_modern(&host, port)
            .or_else(|| ping_legacy(&host, port))
            .unwrap_or_else(MinecraftServerStatus::offline))
    }

    fn mutate_servers(
        &self,
        instance_id: &str,
        mutate: impl FnOnce(&mut Vec<NbtTag>) -> Result<()>,
    ) -> Result<Vec<InstanceServerEntry>> {
        let instance = self.ready_instance(instance_id)?;
        let path = servers_dat_path(&instance);
        let (root_name, mut root) = read_servers_tree(&path)?;
        let NbtTag::Compound(fields) = &mut root else {
            return Err(invalid("servers.dat 根标签必须是 compound"));
        };
        if !fields.iter().any(|(name, _)| name == "servers") {
            fields.push((
                "servers".to_owned(),
                NbtTag::List(nbt::TAG_COMPOUND, Vec::new()),
            ));
        }
        let Some((_, NbtTag::List(element, items))) =
            fields.iter_mut().find(|(name, _)| name == "servers")
        else {
            unreachable!("servers 列表刚被确保存在");
        };
        if *element != nbt::TAG_COMPOUND {
            return Err(invalid("servers.dat 的 servers 列表元素必须是 compound"));
        }
        mutate(items)?;
        let bytes = nbt::write_root(&root_name, &root);
        atomic_replace(&path, &bytes)?;
        Ok(collect_entries(&root))
    }

    /// 启动收敛:中断的三步替换必须收敛到完整旧文件或完整新文件之一。
    pub(crate) fn recover_interrupted_server_writes(&self) -> Result<()> {
        for instance in self.list_instances()? {
            let path = servers_dat_path(&instance);
            let Some(directory) = path.parent() else {
                continue;
            };
            let partial = directory.join(PARTIAL_NAME);
            let backup = directory.join(BACKUP_NAME);
            if !path.exists() && backup.exists() {
                // 中断点在“旧文件已改名、新文件未就位”:恢复旧文件。
                fs::rename(&backup, &path)?;
            } else if backup.exists() {
                // 新文件已就位,备份是残留,清理即可。
                let _ = fs::remove_file(&backup);
            }
            if partial.exists() {
                let _ = fs::remove_file(&partial);
            }
        }
        Ok(())
    }
}

fn servers_dat_path(instance: &ManagedInstanceSummary) -> PathBuf {
    Path::new(&instance.root_directory)
        .join(".minecraft")
        .join("servers.dat")
}

fn read_servers_tree(path: &Path) -> Result<(String, NbtTag)> {
    if !path.exists() {
        return Ok((String::new(), NbtTag::Compound(Vec::new())));
    }
    let bytes = fs::read(path)?;
    nbt::read_root(&bytes).map_err(|reason| invalid(&format!("servers.dat 无法解析:{reason}")))
}

fn collect_entries(root: &NbtTag) -> Vec<InstanceServerEntry> {
    let Some(fields) = root.as_compound() else {
        return Vec::new();
    };
    let Some((_, NbtTag::List(_, items))) = fields.iter().find(|(name, _)| name == "servers")
    else {
        return Vec::new();
    };
    let mut entries = Vec::new();
    for item in items {
        let NbtTag::Compound(fields) = item else {
            continue;
        };
        // 缺名称或地址的条目游戏内也不可用,读取时跳过。
        let (Some(name), Some(address)) =
            (string_field(fields, "name"), string_field(fields, "ip"))
        else {
            continue;
        };
        entries.push(InstanceServerEntry {
            name: name.to_owned(),
            address: address.to_owned(),
            icon: string_field(fields, "icon").map(str::to_owned),
            accept_textures: byte_field(fields, "acceptTextures").map(|value| value != 0),
        });
    }
    entries
}

fn string_field<'a>(fields: &'a [(String, NbtTag)], key: &str) -> Option<&'a str> {
    fields
        .iter()
        .find(|(name, _)| name == key)
        .and_then(|(_, value)| value.as_string())
}

/// 界面序号(只计有名称+地址的有效条目)映射到 NBT 原始列表位置,
/// 保证读取时的跳过逻辑不会让删除/编辑错位。
fn raw_index_of_entry(items: &[NbtTag], display_index: usize) -> Option<usize> {
    let mut seen = 0_usize;
    for (raw, item) in items.iter().enumerate() {
        let NbtTag::Compound(fields) = item else {
            continue;
        };
        if string_field(fields, "name").is_some() && string_field(fields, "ip").is_some() {
            if seen == display_index {
                return Some(raw);
            }
            seen += 1;
        }
    }
    None
}

fn byte_field(fields: &[(String, NbtTag)], key: &str) -> Option<i8> {
    fields
        .iter()
        .find(|(name, _)| name == key)
        .and_then(|(_, value)| match value {
            NbtTag::Byte(value) => Some(*value),
            _ => None,
        })
}

fn set_string_field(fields: &mut Vec<(String, NbtTag)>, key: &str, value: &str) {
    if let Some((_, slot)) = fields.iter_mut().find(|(name, _)| name == key) {
        *slot = NbtTag::String(value.to_owned());
    } else {
        fields.push((key.to_owned(), NbtTag::String(value.to_owned())));
    }
}

fn server_compound(name: &str, address: &str) -> NbtTag {
    NbtTag::Compound(vec![
        ("name".to_owned(), NbtTag::String(name.to_owned())),
        ("ip".to_owned(), NbtTag::String(address.to_owned())),
    ])
}

fn validate_server_name(name: &str) -> Result<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(invalid("服务器名称不能为空"));
    }
    if trimmed.chars().count() > MAX_NAME_CHARS {
        return Err(invalid("服务器名称过长(最多 64 字符)"));
    }
    if trimmed.chars().any(char::is_control) {
        return Err(invalid("服务器名称不能包含控制字符"));
    }
    Ok(trimmed.to_owned())
}

fn validate_server_address(address: &str) -> Result<String> {
    let trimmed = address.trim();
    parse_server_address(trimmed)?;
    Ok(trimmed.to_owned())
}

/// 解析 host[:port];IPv6 必须用方括号。端口缺省 25565,显式端口须在 1-65535。
fn parse_server_address(address: &str) -> Result<(String, u16)> {
    if address.is_empty() {
        return Err(invalid("服务器地址不能为空"));
    }
    if address.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(invalid("服务器地址不能包含空白或控制字符"));
    }
    let (host, port_text) = if let Some(rest) = address.strip_prefix('[') {
        let Some(end) = rest.find(']') else {
            return Err(invalid("IPv6 地址缺少右方括号"));
        };
        let host = &rest[..end];
        let tail = &rest[end + 1..];
        let port = if tail.is_empty() {
            None
        } else {
            let Some(port) = tail.strip_prefix(':') else {
                return Err(invalid("IPv6 地址后只能跟 :端口"));
            };
            Some(port)
        };
        (host, port)
    } else {
        match address.matches(':').count() {
            0 => (address, None),
            1 => {
                let (host, port) = address.split_once(':').unwrap();
                (host, Some(port))
            }
            _ => return Err(invalid("IPv6 地址需用方括号包裹,如 [::1]:25565")),
        }
    };
    if host.is_empty() {
        return Err(invalid("服务器地址缺少主机名"));
    }
    let port = match port_text {
        None => DEFAULT_PORT,
        Some(text) => {
            if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
                return Err(invalid("端口必须是 1-65535 的数字"));
            }
            let value: u32 = text
                .parse()
                .map_err(|_| invalid("端口必须是 1-65535 的数字"))?;
            if value == 0 || value > 65535 {
                return Err(invalid("端口必须在 1-65535 之间"));
            }
            u16::try_from(value).unwrap()
        }
    };
    Ok((host.to_owned(), port))
}

/// 三步替换:写 partial → 旧文件改名 backup → partial 改名目标 → 删 backup。
/// 最后一步失败时把 backup 放回原位,保证始终是完整文件。
fn atomic_replace(path: &Path, bytes: &[u8]) -> Result<()> {
    let directory = path
        .parent()
        .ok_or_else(|| invalid("servers.dat 路径无效"))?;
    fs::create_dir_all(directory)?;
    let partial = directory.join(PARTIAL_NAME);
    let backup = directory.join(BACKUP_NAME);
    {
        let mut file = fs::File::create(&partial)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    let result = (|| -> Result<()> {
        if backup.exists() {
            fs::remove_file(&backup)?;
        }
        if path.exists() {
            fs::rename(path, &backup)?;
        }
        if let Err(error) = fs::rename(&partial, path) {
            if backup.exists() {
                let _ = fs::rename(&backup, path);
            }
            return Err(CoreError::from(error));
        }
        if backup.exists() {
            fs::remove_file(&backup)?;
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&partial);
    }
    result
}

fn invalid(reason: &str) -> CoreError {
    CoreError::InstanceServer(reason.to_owned())
}

// ---------- Server List Ping ----------

fn connect(host: &str, port: u16) -> Option<TcpStream> {
    let addresses = (host, port).to_socket_addrs().ok()?;
    let mut stream = None;
    for address in addresses {
        match TcpStream::connect_timeout(&address, CONNECT_TIMEOUT) {
            Ok(candidate) => {
                stream = Some(candidate);
                break;
            }
            Err(_) => continue,
        }
    }
    let stream = stream?;
    stream.set_read_timeout(Some(IO_TIMEOUT)).ok()?;
    stream.set_write_timeout(Some(IO_TIMEOUT)).ok()?;
    Some(stream)
}

fn ping_modern(host: &str, port: u16) -> Option<MinecraftServerStatus> {
    let started = Instant::now();
    let mut stream = connect(host, port)?;
    // handshake:packet id 0 + protocol -1 + host + port + next state 1(status)
    let mut packet = Vec::new();
    write_varint(&mut packet, 0);
    write_varint(&mut packet, -1);
    write_varint(&mut packet, host.len() as i32);
    packet.extend_from_slice(host.as_bytes());
    packet.extend_from_slice(&port.to_be_bytes());
    write_varint(&mut packet, 1);
    let mut framed = Vec::new();
    write_varint(&mut framed, packet.len() as i32);
    framed.extend_from_slice(&packet);
    stream.write_all(&framed).ok()?;
    // status request:长度 1 + packet id 0
    stream.write_all(&[1, 0]).ok()?;
    stream.flush().ok()?;

    let _length = read_varint(&mut stream)?;
    let _packet_id = read_varint(&mut stream)?;
    let json_length = read_varint(&mut stream)?;
    if json_length < 0 || json_length as usize > MAX_STATUS_BYTES {
        return None;
    }
    let mut buffer = vec![0_u8; json_length as usize];
    stream.read_exact(&mut buffer).ok()?;
    let json: serde_json::Value = serde_json::from_slice(&buffer).ok()?;

    let motd = json.get("description").and_then(motd_text);
    let players_online = json
        .get("players")
        .and_then(|players| players.get("online"))
        .and_then(serde_json::Value::as_u64)
        .and_then(|value| u32::try_from(value).ok());
    let players_max = json
        .get("players")
        .and_then(|players| players.get("max"))
        .and_then(serde_json::Value::as_u64)
        .and_then(|value| u32::try_from(value).ok());
    let version_name = json
        .get("version")
        .and_then(|version| version.get("name"))
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned);
    Some(MinecraftServerStatus {
        online: true,
        motd,
        players_online,
        players_max,
        version_name,
        latency_ms: Some(elapsed_millis(started)),
    })
}

/// MOTD 可能是字符串(含 § 码)或 chat 组件;组件拍平 text/extra 为纯文本,
/// color 等样式字段不映射(诚实边界,界面只对 § 码着色)。
fn motd_text(description: &serde_json::Value) -> Option<String> {
    match description {
        serde_json::Value::String(text) => Some(text.clone()),
        serde_json::Value::Object(_) => {
            let mut out = String::new();
            flatten_chat(description, &mut out);
            Some(out)
        }
        _ => None,
    }
}

fn flatten_chat(component: &serde_json::Value, out: &mut String) {
    if let Some(text) = component.get("text").and_then(serde_json::Value::as_str) {
        out.push_str(text);
    }
    if let Some(extra) = component.get("extra").and_then(serde_json::Value::as_array) {
        for child in extra {
            flatten_chat(child, out);
        }
    }
}

fn ping_legacy(host: &str, port: u16) -> Option<MinecraftServerStatus> {
    let started = Instant::now();
    let mut stream = connect(host, port)?;
    let mut payload = vec![0xFE, 0x01, 0xFA];
    push_utf16_short_string(&mut payload, "MC|PingHost");
    let host_units: Vec<u16> = host.encode_utf16().collect();
    // 剩余字节数 = 协议 1 + 主机长度 2 + 主机 UTF-16 字节 + 端口 4
    let rest_length = 1 + 2 + host_units.len() * 2 + 4;
    payload.extend_from_slice(&(rest_length as u16).to_be_bytes());
    payload.push(74); // 协议号 74(1.6.2),仅作礼貌声明
    payload.extend_from_slice(&(host_units.len() as u16).to_be_bytes());
    for unit in &host_units {
        payload.extend_from_slice(&unit.to_be_bytes());
    }
    payload.extend_from_slice(&u32::from(port).to_be_bytes());
    stream.write_all(&payload).ok()?;
    stream.flush().ok()?;

    let mut prefix = [0_u8; 3];
    stream.read_exact(&mut prefix).ok()?;
    if prefix[0] != 0xFF {
        return None;
    }
    let units = usize::from(u16::from_be_bytes([prefix[1], prefix[2]]));
    if units == 0 || units > MAX_LEGACY_STATUS_CHARS {
        return None;
    }
    let mut buffer = vec![0_u8; units * 2];
    stream.read_exact(&mut buffer).ok()?;
    let decoded: Vec<u16> = buffer
        .chunks_exact(2)
        .map(|chunk| u16::from_be_bytes([chunk[0], chunk[1]]))
        .collect();
    let text = String::from_utf16(&decoded).ok()?;
    // 响应形如 §1\0<协议>\0<版本>\0<MOTD>\0<在线>\0<上限>
    let mut parts = text.split('\0');
    let signature = parts.next()?;
    if !signature.starts_with('§') {
        return None;
    }
    let _protocol = parts.next();
    let version_name = parts.next().map(str::to_owned);
    let motd = parts.next().map(str::to_owned);
    let players_online = parts.next().and_then(|value| value.parse::<u32>().ok());
    let players_max = parts.next().and_then(|value| value.parse::<u32>().ok());
    Some(MinecraftServerStatus {
        online: true,
        motd,
        players_online,
        players_max,
        version_name,
        latency_ms: Some(elapsed_millis(started)),
    })
}

fn push_utf16_short_string(out: &mut Vec<u8>, value: &str) {
    let units: Vec<u16> = value.encode_utf16().collect();
    out.extend_from_slice(&(units.len() as u16).to_be_bytes());
    for unit in units {
        out.extend_from_slice(&unit.to_be_bytes());
    }
}

fn elapsed_millis(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
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

fn read_varint(stream: &mut TcpStream) -> Option<i32> {
    let mut result = 0_u32;
    let mut shift = 0_u32;
    loop {
        let mut byte = [0_u8; 1];
        stream.read_exact(&mut byte).ok()?;
        result |= u32::from(byte[0] & 0x7F) << shift;
        if byte[0] & 0x80 == 0 {
            return Some(result as i32);
        }
        shift += 7;
        if shift >= 35 {
            return None;
        }
    }
}
