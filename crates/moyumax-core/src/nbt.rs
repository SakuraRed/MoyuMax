//! 最小 NBT(命名二进制标签)读写:未压缩大端格式,覆盖标准 13 种标签,读写对称可往返。
//!
//! 诚实边界:字符串按标准 UTF-8 处理,与 Java MUTF-8 的差异仅体现在 NUL 与
//! 增补字符上(服务器名称/地址不包含这些字符);不处理 gzip 包装(servers.dat
//! 从不压缩)。

pub(crate) const TAG_COMPOUND: u8 = 10;

const TAG_END: u8 = 0;
const TAG_BYTE: u8 = 1;
const TAG_SHORT: u8 = 2;
const TAG_INT: u8 = 3;
const TAG_LONG: u8 = 4;
const TAG_FLOAT: u8 = 5;
const TAG_DOUBLE: u8 = 6;
const TAG_BYTE_ARRAY: u8 = 7;
const TAG_STRING: u8 = 8;
const TAG_LIST: u8 = 9;
const TAG_INT_ARRAY: u8 = 11;
const TAG_LONG_ARRAY: u8 = 12;

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum NbtTag {
    Byte(i8),
    Short(i16),
    Int(i32),
    Long(i64),
    Float(f32),
    Double(f64),
    ByteArray(Vec<u8>),
    String(String),
    /// 列表元素类型 id 与元素;空列表的元素 id 原样保留以便往返。
    List(u8, Vec<NbtTag>),
    Compound(Vec<(String, NbtTag)>),
    IntArray(Vec<i32>),
    LongArray(Vec<i64>),
}

impl NbtTag {
    pub(crate) fn as_compound(&self) -> Option<&Vec<(String, NbtTag)>> {
        match self {
            Self::Compound(entries) => Some(entries),
            _ => None,
        }
    }

    pub(crate) fn as_string(&self) -> Option<&str> {
        match self {
            Self::String(value) => Some(value),
            _ => None,
        }
    }
}

/// 解析根标签(必须是 compound),返回根名称与标签树。
pub(crate) fn read_root(bytes: &[u8]) -> Result<(String, NbtTag), String> {
    let mut reader = Reader { bytes, offset: 0 };
    let tag = reader.u8()?;
    if tag != TAG_COMPOUND {
        return Err(format!("NBT 根标签必须是 compound,实际是 {tag}"));
    }
    let name = reader.string()?;
    let payload = reader.payload(TAG_COMPOUND)?;
    Ok((name, payload))
}

/// 序列化根标签(compound),与 `read_root` 对称。
pub(crate) fn write_root(name: &str, root: &NbtTag) -> Vec<u8> {
    let mut out = Vec::with_capacity(256);
    out.push(TAG_COMPOUND);
    write_string(&mut out, name);
    write_payload(&mut out, root);
    out
}

struct Reader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl Reader<'_> {
    fn take(&mut self, count: usize) -> Result<&[u8], String> {
        if self.bytes.len() - self.offset < count {
            return Err("NBT 数据被截断".to_owned());
        }
        let slice = &self.bytes[self.offset..self.offset + count];
        self.offset += count;
        Ok(slice)
    }

    fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }

    fn i16(&mut self) -> Result<i16, String> {
        Ok(i16::from_be_bytes(self.take(2)?.try_into().unwrap()))
    }

    fn u16(&mut self) -> Result<u16, String> {
        Ok(u16::from_be_bytes(self.take(2)?.try_into().unwrap()))
    }

    fn i32(&mut self) -> Result<i32, String> {
        Ok(i32::from_be_bytes(self.take(4)?.try_into().unwrap()))
    }

    fn i64(&mut self) -> Result<i64, String> {
        Ok(i64::from_be_bytes(self.take(8)?.try_into().unwrap()))
    }

    fn f32(&mut self) -> Result<f32, String> {
        Ok(f32::from_be_bytes(self.take(4)?.try_into().unwrap()))
    }

    fn f64(&mut self) -> Result<f64, String> {
        Ok(f64::from_be_bytes(self.take(8)?.try_into().unwrap()))
    }

    fn string(&mut self) -> Result<String, String> {
        let length = usize::from(self.u16()?);
        let raw = self.take(length)?;
        String::from_utf8(raw.to_vec()).map_err(|_| "NBT 字符串不是合法 UTF-8".to_owned())
    }

    /// 长度前缀防御:负值拒绝,正值得超过剩余字节数(每个元素至少 1 字节)也拒绝。
    fn length(&mut self) -> Result<usize, String> {
        let length = self.i32()?;
        if length < 0 {
            return Err(format!("NBT 长度不能为负:{length}"));
        }
        let length = length as usize;
        if length > self.bytes.len() - self.offset {
            return Err(format!("NBT 长度 {length} 超出剩余数据"));
        }
        Ok(length)
    }

    fn payload(&mut self, tag: u8) -> Result<NbtTag, String> {
        match tag {
            TAG_BYTE => Ok(NbtTag::Byte(self.u8()? as i8)),
            TAG_SHORT => Ok(NbtTag::Short(self.i16()?)),
            TAG_INT => Ok(NbtTag::Int(self.i32()?)),
            TAG_LONG => Ok(NbtTag::Long(self.i64()?)),
            TAG_FLOAT => Ok(NbtTag::Float(self.f32()?)),
            TAG_DOUBLE => Ok(NbtTag::Double(self.f64()?)),
            TAG_BYTE_ARRAY => {
                let length = self.length()?;
                Ok(NbtTag::ByteArray(self.take(length)?.to_vec()))
            }
            TAG_STRING => Ok(NbtTag::String(self.string()?)),
            TAG_LIST => {
                let element = self.u8()?;
                let length = self.length()?;
                if length > 0 && (element == TAG_END || element > TAG_LONG_ARRAY) {
                    return Err(format!("NBT 列表元素类型非法:{element}"));
                }
                let mut items = Vec::with_capacity(length.min(4096));
                for _ in 0..length {
                    items.push(self.payload(element)?);
                }
                Ok(NbtTag::List(element, items))
            }
            TAG_COMPOUND => {
                let mut entries = Vec::new();
                loop {
                    let child = self.u8()?;
                    if child == TAG_END {
                        break;
                    }
                    let name = self.string()?;
                    let value = self.payload(child)?;
                    entries.push((name, value));
                }
                Ok(NbtTag::Compound(entries))
            }
            TAG_INT_ARRAY => {
                let length = self.length()?;
                let mut values = Vec::with_capacity(length.min(4096));
                for _ in 0..length {
                    values.push(self.i32()?);
                }
                Ok(NbtTag::IntArray(values))
            }
            TAG_LONG_ARRAY => {
                let length = self.length()?;
                let mut values = Vec::with_capacity(length.min(4096));
                for _ in 0..length {
                    values.push(self.i64()?);
                }
                Ok(NbtTag::LongArray(values))
            }
            other => Err(format!("NBT 标签类型非法:{other}")),
        }
    }
}

fn write_string(out: &mut Vec<u8>, value: &str) {
    let length = u16::try_from(value.len()).expect("NBT 字符串长度超出 u16 上限");
    out.extend_from_slice(&length.to_be_bytes());
    out.extend_from_slice(value.as_bytes());
}

fn write_payload(out: &mut Vec<u8>, tag: &NbtTag) {
    match tag {
        NbtTag::Byte(value) => out.push(*value as u8),
        NbtTag::Short(value) => out.extend_from_slice(&value.to_be_bytes()),
        NbtTag::Int(value) => out.extend_from_slice(&value.to_be_bytes()),
        NbtTag::Long(value) => out.extend_from_slice(&value.to_be_bytes()),
        NbtTag::Float(value) => out.extend_from_slice(&value.to_be_bytes()),
        NbtTag::Double(value) => out.extend_from_slice(&value.to_be_bytes()),
        NbtTag::ByteArray(values) => {
            write_length(out, values.len());
            out.extend_from_slice(values);
        }
        NbtTag::String(value) => write_string(out, value),
        NbtTag::List(element, items) => {
            out.push(*element);
            write_length(out, items.len());
            for item in items {
                write_payload(out, item);
            }
        }
        NbtTag::Compound(entries) => {
            for (name, value) in entries {
                out.push(tag_id(value));
                write_string(out, name);
                write_payload(out, value);
            }
            out.push(TAG_END);
        }
        NbtTag::IntArray(values) => {
            write_length(out, values.len());
            for value in values {
                out.extend_from_slice(&value.to_be_bytes());
            }
        }
        NbtTag::LongArray(values) => {
            write_length(out, values.len());
            for value in values {
                out.extend_from_slice(&value.to_be_bytes());
            }
        }
    }
}

fn write_length(out: &mut Vec<u8>, length: usize) {
    let length = i32::try_from(length).expect("NBT 集合长度超出 i32 上限");
    out.extend_from_slice(&length.to_be_bytes());
}

fn tag_id(tag: &NbtTag) -> u8 {
    match tag {
        NbtTag::Byte(_) => TAG_BYTE,
        NbtTag::Short(_) => TAG_SHORT,
        NbtTag::Int(_) => TAG_INT,
        NbtTag::Long(_) => TAG_LONG,
        NbtTag::Float(_) => TAG_FLOAT,
        NbtTag::Double(_) => TAG_DOUBLE,
        NbtTag::ByteArray(_) => TAG_BYTE_ARRAY,
        NbtTag::String(_) => TAG_STRING,
        NbtTag::List(_, _) => TAG_LIST,
        NbtTag::Compound(_) => TAG_COMPOUND,
        NbtTag::IntArray(_) => TAG_INT_ARRAY,
        NbtTag::LongArray(_) => TAG_LONG_ARRAY,
    }
}
