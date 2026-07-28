//! 启动器自身更新检查与安全下载。
//!
//! 通过 GitHub Releases API 检查最新发布；下载安装包时按发布资产摘要校验
//! SHA-256 与大小，校验失败删除文件。下载与安装全程手动触发，不做任何
//! 自动下载、自动安装或后台定期检查。

use std::{
    cmp::Ordering,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use reqwest::{Client, Url};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{AppService, CoreError, Result, read_setting, write_setting};

const GITHUB_RELEASES_LATEST: &str =
    "https://api.github.com/repos/SakuraRed/MoyuMax/releases/latest";
const SETTING_UPDATE_CHECKS: &str = "update_checks_enabled";
const MAX_INSTALLER_BYTES: u64 = 512 * 1024 * 1024;
const MIN_VERSION_MARKER: &str = "moyumax-min-app-version:";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateAsset {
    pub name: String,
    pub url: String,
    pub size: u64,
    pub sha256: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseInfo {
    pub tag: String,
    pub name: String,
    pub notes: String,
    pub page_url: String,
    pub min_app_version: Option<String>,
    pub installer: Option<UpdateAsset>,
}

#[derive(Clone)]
pub struct UpdateClient {
    client: Client,
    base_url: Url,
}

impl UpdateClient {
    pub fn new() -> Result<Self> {
        Self::with_base_url(GITHUB_RELEASES_LATEST)
    }

    pub fn with_base_url(base_url: &str) -> Result<Self> {
        let base_url = Url::parse(base_url)
            .map_err(|error| CoreError::Content(format!("更新检查地址无效：{error}")))?;
        let localhost = matches!(base_url.host_str(), Some("127.0.0.1" | "localhost"));
        if base_url.scheme() != "https" && !(base_url.scheme() == "http" && localhost) {
            return Err(CoreError::Content("更新检查地址必须使用 https".to_owned()));
        }
        let user_agent = format!(
            "SakuraRed/MoyuMax/{} (github.com/SakuraRed/MoyuMax)",
            env!("CARGO_PKG_VERSION")
        );
        let client = crate::http_client_builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(20))
            .user_agent(user_agent)
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        Ok(Self { client, base_url })
    }

    /// 检查最新发布；有新版本时返回其信息，已是最新返回 None。
    pub async fn check_latest(&self, current_version: &str) -> Result<Option<ReleaseInfo>> {
        let response = self
            .client
            .get(self.base_url.clone())
            .send()
            .await
            .map_err(|error| CoreError::Content(format!("无法检查更新：{error}")))?;
        if !response.status().is_success() {
            return Err(CoreError::Content(format!(
                "更新检查返回 HTTP {}",
                response.status().as_u16()
            )));
        }
        let release: GitHubRelease = response.json().await?;
        let info = release.into_release_info();
        if compare_versions(current_version, &info.tag) == Ordering::Less {
            Ok(Some(info))
        } else {
            Ok(None)
        }
    }

    /// 下载安装包到受管目录并经 SHA-256 与大小校验；失败删除文件。
    pub async fn download_installer(
        &self,
        asset: &UpdateAsset,
        destination_directory: &Path,
    ) -> Result<PathBuf> {
        if asset.size == 0 || asset.size > MAX_INSTALLER_BYTES {
            return Err(CoreError::Content("安装包大小超出可接受范围".to_owned()));
        }
        let url = Url::parse(&asset.url)
            .map_err(|error| CoreError::Content(format!("安装包地址无效：{error}")))?;
        let localhost = matches!(url.host_str(), Some("127.0.0.1" | "localhost"));
        if url.scheme() != "https" && !(url.scheme() == "http" && localhost) {
            return Err(CoreError::Content("安装包地址必须使用 https".to_owned()));
        }
        fs::create_dir_all(destination_directory)?;
        let target = destination_directory.join(&asset.name);
        let partial = destination_directory.join(format!(".{}.partial", asset.name));
        let result =
            async {
                // GitHub 发行资产固定经 302 跳到 CDN;有界手动跟随,每一跳强制 https,
                // 不交给全局重定向策略,避免被带到任意主机。
                let mut url = url;
                let mut response = None;
                for _ in 0..5 {
                    let attempt =
                        self.client.get(url.clone()).send().await.map_err(|error| {
                            CoreError::Content(format!("下载安装包失败：{error}"))
                        })?;
                    if attempt.status().is_redirection() {
                        let location = attempt
                            .headers()
                            .get(reqwest::header::LOCATION)
                            .and_then(|value| value.to_str().ok())
                            .ok_or_else(|| {
                                CoreError::Content("安装包重定向缺少 Location".to_owned())
                            })?;
                        let next = url.join(location).map_err(|error| {
                            CoreError::Content(format!("安装包重定向地址无效：{error}"))
                        })?;
                        let localhost = matches!(next.host_str(), Some("127.0.0.1" | "localhost"));
                        if next.scheme() != "https" && !(next.scheme() == "http" && localhost) {
                            return Err(CoreError::Content(format!(
                                "安装包重定向目标必须使用 https：{next}"
                            )));
                        }
                        url = next;
                        continue;
                    }
                    response = Some(attempt);
                    break;
                }
                let response = response
                    .ok_or_else(|| CoreError::Content("安装包重定向次数过多".to_owned()))?;
                if !response.status().is_success() {
                    return Err(CoreError::Content(format!(
                        "下载安装包返回 HTTP {}",
                        response.status().as_u16()
                    )));
                }
                let bytes = response
                    .bytes()
                    .await
                    .map_err(|error| CoreError::Content(format!("读取安装包失败：{error}")))?;
                if bytes.len() as u64 != asset.size {
                    return Err(CoreError::Content(format!(
                        "安装包大小不一致：期望 {} 字节，实际 {} 字节",
                        asset.size,
                        bytes.len()
                    )));
                }
                let digest = Sha256::digest(&bytes);
                let actual = encode_hex(digest);
                if let Some(expected) = &asset.sha256
                    && !actual.eq_ignore_ascii_case(expected)
                {
                    return Err(CoreError::Content(format!(
                        "安装包 SHA-256 校验失败：期望 {expected}，实际 {actual}"
                    )));
                }
                fs::write(&partial, &bytes)?;
                fs::rename(&partial, &target)?;
                Ok(target)
            }
            .await;
        if result.is_err() {
            let _ = fs::remove_file(&partial);
        }
        result
    }
}

impl AppService {
    /// 更新提示开关（默认开启；只控制提示，不产生自动下载）。
    pub fn update_checks_enabled(&self) -> Result<bool> {
        let connection = self.connection()?;
        Ok(read_setting(&connection, SETTING_UPDATE_CHECKS)?.is_none_or(|value| value == "true"))
    }

    pub fn set_update_checks_enabled(&self, enabled: bool) -> Result<()> {
        let connection = self.connection()?;
        write_setting(
            &connection,
            SETTING_UPDATE_CHECKS,
            if enabled { "true" } else { "false" },
        )?;
        Ok(())
    }
}

/// 数据兼容检查：当前版本低于发布声明的最低可升级版本时返回说明。
pub fn min_version_block(current_version: &str, release: &ReleaseInfo) -> Option<String> {
    let min = release.min_app_version.as_deref()?;
    if compare_versions(current_version, min) == Ordering::Less {
        Some(format!(
            "版本 {min} 及以上才能直接升级到 {}；请先从发布页安装中间版本",
            release.tag
        ))
    } else {
        None
    }
}

/// 简化 semver 比较：数字三元组优先，其次正式版高于预发布，再次预发布按段比较。
pub fn compare_versions(left: &str, right: &str) -> Ordering {
    let left = ParsedVersion::parse(left);
    let right = ParsedVersion::parse(right);
    for index in 0..3 {
        match left.nums[index].cmp(&right.nums[index]) {
            Ordering::Equal => {}
            order => return order,
        }
    }
    match (&left.pre, &right.pre) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Greater,
        (Some(_), None) => Ordering::Less,
        (Some(left_pre), Some(right_pre)) => compare_prerelease(left_pre, right_pre),
    }
}

struct ParsedVersion {
    nums: [u64; 3],
    pre: Option<String>,
}

impl ParsedVersion {
    fn parse(version: &str) -> Self {
        let version = version.strip_prefix('v').unwrap_or(version);
        let (base, pre) = match version.split_once('-') {
            Some((base, pre)) => (base, Some(pre.to_owned())),
            None => (version, None),
        };
        let mut nums = [0_u64; 3];
        for (index, part) in base.split('.').take(3).enumerate() {
            nums[index] = part.parse().unwrap_or(0);
        }
        Self { nums, pre }
    }
}

fn compare_prerelease(left: &str, right: &str) -> Ordering {
    let left_parts = left.split('.').collect::<Vec<_>>();
    let right_parts = right.split('.').collect::<Vec<_>>();
    for index in 0..left_parts.len().max(right_parts.len()) {
        let left_part = left_parts.get(index).copied().unwrap_or("0");
        let right_part = right_parts.get(index).copied().unwrap_or("0");
        match (left_part.parse::<u64>(), right_part.parse::<u64>()) {
            (Ok(left_num), Ok(right_num)) => match left_num.cmp(&right_num) {
                Ordering::Equal => {}
                order => return order,
            },
            _ => match left_part.cmp(right_part) {
                Ordering::Equal => {}
                order => return order,
            },
        }
    }
    Ordering::Equal
}

#[derive(Debug, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    name: Option<String>,
    body: Option<String>,
    html_url: String,
    assets: Vec<GitHubAsset>,
}

#[derive(Debug, Deserialize)]
struct GitHubAsset {
    name: String,
    browser_download_url: String,
    size: u64,
    digest: Option<String>,
}

impl GitHubRelease {
    fn into_release_info(self) -> ReleaseInfo {
        let notes = self.body.unwrap_or_default();
        let min_app_version = notes.lines().find_map(|line| {
            line.trim()
                .strip_prefix(MIN_VERSION_MARKER)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
        });
        let installer = self
            .assets
            .into_iter()
            .filter(|asset| asset.name.ends_with("-setup.exe"))
            .map(|asset| UpdateAsset {
                name: asset.name,
                url: asset.browser_download_url,
                size: asset.size,
                sha256: asset
                    .digest
                    .and_then(|digest| digest.strip_prefix("sha256:").map(str::to_owned)),
            })
            .next();
        ReleaseInfo {
            tag: self.tag_name,
            name: self.name.unwrap_or_default(),
            notes,
            page_url: self.html_url,
            min_app_version,
            installer,
        }
    }
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
