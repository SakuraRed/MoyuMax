//! Forge/NeoForge spec-1 安装器处理器执行。
//!
//! 解析安装器内的 `install_profile.json` 与 `version.json`，按序执行
//! 客户端处理器（映射合并、改名、二进制补丁），产物先写暂存区，
//! 通过 `_SHA`（提供时）与存在性校验后才允许进入共享存储。
//! 处理器运行器可注入：生产使用托管 Java 子进程，测试使用确定性假运行器。

use std::{
    collections::HashMap,
    fs,
    io::Read,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use zip::ZipArchive;

use crate::{CoreError, Result};

/// Maven 坐标：`group:artifact:version[:classifier][@extension]`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MavenCoordinate {
    pub group: String,
    pub artifact: String,
    pub version: String,
    pub classifier: Option<String>,
    pub extension: String,
}

impl MavenCoordinate {
    pub fn parse(value: &str) -> Result<Self> {
        let (coordinate, extension) = match value.split_once('@') {
            Some((coordinate, extension)) => (coordinate, extension.to_owned()),
            None => (value, "jar".to_owned()),
        };
        let parts: Vec<&str> = coordinate.split(':').collect();
        if !(3..=4).contains(&parts.len()) {
            return Err(CoreError::InvalidInstallRequest(format!(
                "Maven 坐标格式无效：{value}"
            )));
        }
        let validate = |part: &str, label: &str| -> Result<()> {
            if part.is_empty()
                || !part.bytes().all(|byte| {
                    byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_' | b'+')
                })
            {
                return Err(CoreError::InvalidInstallRequest(format!(
                    "Maven 坐标{label}包含非法字符：{value}"
                )));
            }
            Ok(())
        };
        validate(parts[0], "group")?;
        validate(parts[1], "artifact")?;
        validate(parts[2], "version")?;
        if let Some(classifier) = parts.get(3) {
            validate(classifier, "classifier")?;
        }
        validate(&extension, "extension")?;
        Ok(Self {
            group: parts[0].to_owned(),
            artifact: parts[1].to_owned(),
            version: parts[2].to_owned(),
            classifier: parts.get(3).map(|value| (*value).to_owned()),
            extension,
        })
    }

    /// Maven 仓库相对路径：`group/artifact/version/artifact-version[-classifier].ext`。
    #[must_use]
    pub fn relative_path(&self) -> String {
        let classifier = self
            .classifier
            .as_ref()
            .map(|value| format!("-{value}"))
            .unwrap_or_default();
        format!(
            "{}/{}/{}/{}-{}{}.{}",
            self.group.replace('.', "/"),
            self.artifact,
            self.version,
            self.artifact,
            self.version,
            classifier,
            self.extension
        )
    }
}

/// spec-1 `install_profile.json` 结构。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallProfile {
    pub spec: u32,
    #[serde(default)]
    pub profile: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub minecraft: String,
    #[serde(default)]
    pub json: String,
    #[serde(default)]
    pub data: HashMap<String, HashMap<String, String>>,
    #[serde(default)]
    pub processors: Vec<ProfileProcessor>,
    #[serde(default)]
    pub libraries: Vec<ProfileLibrary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileProcessor {
    #[serde(default)]
    pub sides: Option<Vec<String>>,
    pub jar: String,
    #[serde(default)]
    pub classpath: Vec<String>,
    #[serde(default)]
    pub args: Vec<String>,
}

impl ProfileProcessor {
    /// 只执行客户端或双侧处理器。
    #[must_use]
    pub fn runs_on_client(&self) -> bool {
        match &self.sides {
            None => true,
            Some(sides) => sides.iter().any(|side| side == "client"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileLibrary {
    pub name: String,
    #[serde(default)]
    pub downloads: Option<ProfileLibraryDownloads>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileLibraryDownloads {
    pub artifact: Option<ProfileLibraryArtifact>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileLibraryArtifact {
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub sha1: String,
    #[serde(default)]
    pub size: u64,
}

/// 处理器调用计划：已展开的类路径、主类与参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessorInvocation {
    pub jars: Vec<PathBuf>,
    pub main_class: String,
    pub args: Vec<String>,
}

/// 处理器运行器：生产为托管 Java 子进程，测试为确定性闭包。
pub type ProcessorRunner = dyn Fn(&ProcessorInvocation, &Path) -> Result<()> + Send + Sync;

/// 从安装器读取 install_profile 与 version.json，拒绝非 spec-1。
pub fn read_install_profile<R: Read + std::io::Seek>(
    reader: R,
) -> Result<(InstallProfile, serde_json::Value)> {
    let mut archive = ZipArchive::new(reader)
        .map_err(|error| CoreError::Archive(format!("安装器不是有效的 ZIP：{error}")))?;
    let profile_text = read_zip_text(&mut archive, "install_profile.json")?;
    let profile: InstallProfile = serde_json::from_str(&profile_text)?;
    if profile.spec != 1 {
        return Err(CoreError::InvalidInstallRequest(format!(
            "不支持的 install_profile spec {}，仅支持 spec 1",
            profile.spec
        )));
    }
    let json_entry = profile.json.trim_start_matches('/').to_owned();
    let version_text = read_zip_text(&mut archive, &json_entry)?;
    let version_json = serde_json::from_str(&version_text)?;
    Ok((profile, version_json))
}

/// 从安装器解出 data 目录内文件（如 /data/client.lzma）到工作目录。
pub fn extract_installer_data(installer: &Path, data_path: &str, target: &Path) -> Result<()> {
    let normalized = data_path.trim_start_matches('/');
    if normalized.contains("..") {
        return Err(CoreError::Archive(format!(
            "安装器数据路径不安全：{data_path}"
        )));
    }
    let file = fs::File::open(installer)?;
    let mut archive = ZipArchive::new(file)
        .map_err(|error| CoreError::Archive(format!("安装器不是有效的 ZIP：{error}")))?;
    let mut entry = archive
        .by_name(normalized)
        .map_err(|_| CoreError::Archive(format!("安装器缺少数据文件：{data_path}")))?;
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut output = fs::File::create(target)?;
    std::io::copy(&mut entry, &mut output)?;
    Ok(())
}

fn read_zip_text<R: Read + std::io::Seek>(
    archive: &mut ZipArchive<R>,
    name: &str,
) -> Result<String> {
    let mut entry = archive
        .by_name(name)
        .map_err(|_| CoreError::Archive(format!("安装器缺少 {name}")))?;
    let mut text = String::new();
    entry.read_to_string(&mut text)?;
    Ok(text)
}

/// 读取处理器 JAR manifest 的 Main-Class。
pub fn read_main_class(jar: &Path) -> Result<String> {
    let file = fs::File::open(jar)?;
    let mut archive = ZipArchive::new(file)
        .map_err(|error| CoreError::Archive(format!("处理器 JAR 无法读取：{error}")))?;
    let manifest = read_zip_text(&mut archive, "META-INF/MANIFEST.MF")?;
    // Manifest 折行规则:仅以下一个空格开头的行才是续行;
    // Main-Class 未折行时,下一属性(如 Specification-Title)不得并入。
    let mut main_class: Option<String> = None;
    let mut in_main_class = false;
    for line in manifest.lines() {
        if line.starts_with(' ') {
            if in_main_class && let Some(value) = &mut main_class {
                value.push_str(line.trim());
            }
            continue;
        }
        in_main_class = false;
        if let Some(value) = line.strip_prefix("Main-Class:") {
            main_class = Some(value.trim().to_owned());
            in_main_class = true;
        }
    }
    main_class
        .filter(|value| !value.is_empty())
        .ok_or_else(|| CoreError::Archive("处理器 JAR 缺少 Main-Class".to_owned()))
}

/// 占位符展开表。键不含花括号。
pub type PlaceholderMap = HashMap<String, String>;

/// 展开 install_profile 的 `{KEY}` 占位符；未知占位符直接报错（与启动语义一致）。
/// 注意与 Mojang 版本 JSON 的 `${key}` 形式不同,这里不带 `$`。
pub fn expand_placeholders(value: &str, placeholders: &PlaceholderMap) -> Result<String> {
    let mut output = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(start) = rest.find('{') {
        output.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let Some(end) = after.find('}') else {
            return Err(CoreError::InvalidInstallRequest(format!(
                "占位符缺少闭合括号:{value}"
            )));
        };
        let key = &after[..end];
        let Some(replacement) = placeholders.get(key) else {
            return Err(CoreError::InvalidInstallRequest(format!(
                "未知安装器占位符:{{{key}}}"
            )));
        };
        output.push_str(replacement);
        rest = &after[end + 1..];
    }
    output.push_str(rest);
    Ok(output)
}

/// 校验产物 SHA-1（提供时）与存在性。
pub fn verify_processor_output(path: &Path, expected_sha1: Option<&str>) -> Result<()> {
    let metadata = fs::metadata(path).map_err(|error| {
        CoreError::InvalidInstallRequest(format!("处理器产物不存在：{}（{error}）", path.display()))
    })?;
    if metadata.len() == 0 {
        return Err(CoreError::InvalidInstallRequest(format!(
            "处理器产物为空：{}",
            path.display()
        )));
    }
    if let Some(expected) = expected_sha1 {
        let expected = expected.trim().trim_matches('\'');
        let payload = fs::read(path)?;
        let mut hasher = Sha1::new();
        hasher.update(&payload);
        let actual = encode_hex(hasher.finalize());
        if actual != expected {
            return Err(CoreError::Download(format!(
                "处理器产物 SHA-1 不匹配：预期 {expected}，实际 {actual}"
            )));
        }
    }
    Ok(())
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

/// 生产处理器运行器：用托管 Java 执行 `java -cp jars MainClass args`。
/// Windows 下必须 CREATE_NO_WINDOW,否则每个处理器都会弹出一个
/// 控制台黑窗(实测 NeoForge 链连续弹六个)。
#[must_use]
pub fn java_processor_runner(java_executable: PathBuf) -> Box<ProcessorRunner> {
    Box::new(move |invocation: &ProcessorInvocation, work_dir: &Path| {
        let classpath = invocation
            .jars
            .iter()
            .map(|jar| jar.to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join(";");
        let mut command = std::process::Command::new(&java_executable);
        command
            .arg("-cp")
            .arg(&classpath)
            .arg(&invocation.main_class)
            .args(&invocation.args)
            .current_dir(work_dir);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(CREATE_NO_WINDOW);
        }
        let output = command
            .output()
            .map_err(|error| CoreError::Launch(format!("处理器启动失败：{error}")))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let summary: String = stderr.chars().take(600).collect();
            return Err(CoreError::Launch(format!(
                "处理器 {} 退出码 {:?}：{summary}",
                invocation.main_class,
                output.status.code()
            )));
        }
        Ok(())
    })
}

/// 处理器执行计划：按序的调用、补丁产物位置与校验值。
#[derive(Debug)]
pub struct LoaderInstallPlan {
    pub invocations: Vec<ProcessorInvocation>,
    pub patched_output: PathBuf,
    pub patched_sha1: Option<String>,
    pub patched_coordinate: MavenCoordinate,
}

/// 依据 install_profile 构建客户端处理器执行计划。
/// `library_dir` 为暂存区中的受管库根（与共享存储同构）,
/// `shared_library_dir` 为共享存储库根:此前任务已提交共享的库会被本次
/// 下载直接复用而不复制到暂存,查找处理器库必须带共享回退,否则必然
/// 误报"处理器库缺失"(实测 jopt-simple 二次安装必现)。
/// `minecraft_jar` 为官方客户端 JAR,`work_dir` 为处理器工作目录。
pub fn plan_loader_processors(
    profile: &InstallProfile,
    installer: &Path,
    library_dir: &Path,
    shared_library_dir: &Path,
    minecraft_jar: &Path,
    minecraft_version: &str,
    work_dir: &Path,
) -> Result<LoaderInstallPlan> {
    fs::create_dir_all(work_dir)?;
    let version_json_target = work_dir.join("version.json");
    // VERSION_JSON 占位符引用安装器内的 version.json,先落盘到工作目录。
    extract_installer_data(installer, &profile.json, &version_json_target)?;

    let mut placeholders: PlaceholderMap = HashMap::new();
    placeholders.insert("ROOT".to_owned(), path_text(work_dir));
    placeholders.insert("LIBRARY_DIR".to_owned(), path_text(library_dir));
    placeholders.insert("INSTALLER".to_owned(), path_text(installer));
    placeholders.insert("MINECRAFT_JAR".to_owned(), path_text(minecraft_jar));
    placeholders.insert("SIDE".to_owned(), "client".to_owned());
    placeholders.insert("MINECRAFT_VERSION".to_owned(), minecraft_version.to_owned());
    placeholders.insert("VERSION_JSON".to_owned(), path_text(&version_json_target));
    for (key, sides) in &profile.data {
        let raw = sides.get("client").ok_or_else(|| {
            CoreError::InvalidInstallRequest(format!("install_profile 数据 {key} 缺少 client 取值"))
        })?;
        // PATCHED 是处理器产物(输出),必须始终指向暂存库根:此前运行可能
        // 已把同名产物提交进共享存储,回退过去会把本次产物写错位置。
        let resolved = if key == "PATCHED" && raw.starts_with('[') && raw.ends_with(']') {
            let coordinate = MavenCoordinate::parse(&raw[1..raw.len() - 1])?;
            path_text(&library_dir.join(coordinate.relative_path()))
        } else {
            resolve_data_value(raw, installer, library_dir, shared_library_dir, work_dir)?
        };
        placeholders.insert(key.clone(), resolved);
    }

    let mut invocations = Vec::new();
    for processor in &profile.processors {
        if !processor.runs_on_client() {
            continue;
        }
        let jar_coordinate = MavenCoordinate::parse(&processor.jar)?;
        let mut jars = vec![resolve_library_jar(
            library_dir,
            shared_library_dir,
            &jar_coordinate,
        )?];
        for classpath in &processor.classpath {
            let coordinate = MavenCoordinate::parse(classpath)?;
            jars.push(resolve_library_jar(
                library_dir,
                shared_library_dir,
                &coordinate,
            )?);
        }
        let main_class = read_main_class(&jars[0])?;
        let args = processor
            .args
            .iter()
            .map(|argument| {
                // 直接方括号坐标(installertools 约定由启动器替换成实际路径,
                // 实测 NeoForge MCP_DATA 的 --input 就带 [neoform@zip])。
                if argument.starts_with('[') && argument.ends_with(']') {
                    let coordinate = MavenCoordinate::parse(&argument[1..argument.len() - 1])?;
                    return Ok(path_text(&resolve_library_jar(
                        library_dir,
                        shared_library_dir,
                        &coordinate,
                    )?));
                }
                expand_placeholders(argument, &placeholders)
            })
            .collect::<Result<Vec<_>>>()?;
        invocations.push(ProcessorInvocation {
            jars,
            main_class,
            args,
        });
    }

    let patched_raw = profile
        .data
        .get("PATCHED")
        .and_then(|sides| sides.get("client"))
        .ok_or_else(|| {
            CoreError::InvalidInstallRequest("install_profile 缺少 PATCHED 产物声明".to_owned())
        })?;
    let patched_coordinate = parse_bracketed_coordinate(patched_raw)?;
    let patched_output = library_dir.join(patched_coordinate.relative_path());
    let patched_sha1 = profile
        .data
        .get("PATCHED_SHA")
        .and_then(|sides| sides.get("client"))
        .map(|value| value.trim().trim_matches('\'').to_owned());
    Ok(LoaderInstallPlan {
        invocations,
        patched_output,
        patched_sha1,
        patched_coordinate,
    })
}

/// 按序执行处理器并校验补丁产物。
pub fn run_loader_processors(
    plan: &LoaderInstallPlan,
    runner: &ProcessorRunner,
    work_dir: &Path,
) -> Result<()> {
    for invocation in &plan.invocations {
        runner(invocation, work_dir)?;
    }
    verify_processor_output(&plan.patched_output, plan.patched_sha1.as_deref())
}

/// 解析处理器库 jar:暂存库根优先,缺失时回退共享存储(复用不复制)。
fn resolve_library_jar(
    library_dir: &Path,
    shared_library_dir: &Path,
    coordinate: &MavenCoordinate,
) -> Result<PathBuf> {
    let relative = coordinate.relative_path();
    let staged = library_dir.join(&relative);
    if staged.is_file() {
        return Ok(staged);
    }
    let shared = shared_library_dir.join(&relative);
    if shared.is_file() {
        return Ok(shared);
    }
    Err(CoreError::InvalidInstallRequest(format!(
        "处理器库缺失：{}（暂存与共享存储均无）",
        staged.display()
    )))
}

fn resolve_data_value(
    raw: &str,
    installer: &Path,
    library_dir: &Path,
    shared_library_dir: &Path,
    work_dir: &Path,
) -> Result<String> {
    if raw.starts_with('[') && raw.ends_with(']') {
        let coordinate = MavenCoordinate::parse(&raw[1..raw.len() - 1])?;
        let staged = library_dir.join(coordinate.relative_path());
        if staged.is_file() {
            return Ok(path_text(&staged));
        }
        let shared = shared_library_dir.join(coordinate.relative_path());
        if shared.is_file() {
            // 输入型数据(映射表等)允许共享存储回退。
            return Ok(path_text(&shared));
        }
        // 尚不存在的中间产物(处理链内自产数据与 PATCHED):保持暂存路径。
        return Ok(path_text(&staged));
    }
    if raw.starts_with('/') {
        let target = work_dir.join("data").join(raw.trim_start_matches('/'));
        extract_installer_data(installer, raw, &target)?;
        return Ok(path_text(&target));
    }
    Ok(raw.trim().trim_matches('\'').to_owned())
}

fn parse_bracketed_coordinate(raw: &str) -> Result<MavenCoordinate> {
    if !(raw.starts_with('[') && raw.ends_with(']')) {
        return Err(CoreError::InvalidInstallRequest(format!(
            "产物声明不是 Maven 引用：{raw}"
        )));
    }
    MavenCoordinate::parse(&raw[1..raw.len() - 1])
}

fn path_text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use zip::{ZipWriter, write::SimpleFileOptions};

    fn jar_with_manifest(manifest: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("processor.jar");
        let file = fs::File::create(&path).expect("create jar");
        let mut writer = ZipWriter::new(file);
        writer
            .start_file("META-INF/MANIFEST.MF", SimpleFileOptions::default())
            .expect("start manifest");
        std::io::Write::write_all(&mut writer, manifest.as_bytes()).expect("write manifest");
        writer.finish().expect("finish jar");
        (dir, path)
    }

    #[test]
    fn main_class_single_line_does_not_absorb_next_attribute() {
        // Forge installertools 的真实 manifest:Main-Class 未折行,
        // 下一行是 Specification-Title,不得并入(矩阵实机暴露的回归)。
        let manifest = "Manifest-Version: 1.0\r\nMain-Class: net.minecraftforge.installertools.ConsoleTool\r\nSpecification-Title: Installer Tools\r\nSpecification-Version: 2.1.0\r\n";
        let (_dir, jar) = jar_with_manifest(manifest);
        assert_eq!(
            read_main_class(&jar).expect("main class"),
            "net.minecraftforge.installertools.ConsoleTool"
        );
    }

    #[test]
    fn main_class_wrapped_line_joins_space_continuations() {
        let manifest = "Manifest-Version: 1.0\r\nMain-Class: net.minecraftforge.installertools.Console\r\n Tool\r\nSpecification-Title: Installer Tools\r\n";
        let (_dir, jar) = jar_with_manifest(manifest);
        assert_eq!(
            read_main_class(&jar).expect("main class"),
            "net.minecraftforge.installertools.ConsoleTool"
        );
    }

    #[test]
    fn main_class_missing_is_an_error() {
        let manifest = "Manifest-Version: 1.0\r\nSpecification-Title: Installer Tools\r\n";
        let (_dir, jar) = jar_with_manifest(manifest);
        assert!(read_main_class(&jar).is_err());
    }
}
