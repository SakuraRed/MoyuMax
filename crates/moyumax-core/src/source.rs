//! 下载来源策略与镜像路由。
//!
//! 按持久化策略为每个下载 URL 生成有序候选：默认内置镜像优先
//! （Minecraft 走 BMCLAPI，Modrinth/CurseForge 走 MCI Mirror），
//! 官方优先保留官方链路并可回退，自定义源绝不切换。
//! 路由表按 2026-07-23 实测与官方文档固定，见 M10 计划文档。

use serde::{Deserialize, Serialize};

use crate::{AppService, CoreError, Result, read_setting, write_setting};

const SETTING_DOWNLOAD_SOURCE_POLICY: &str = "download_source_policy";
const BMCLAPI_BASE: &str = "https://bmclapi2.bangbang93.com";
const MCI_MIRROR_BASE: &str = "https://mod.mcimirror.top";

/// 下载来源策略。默认内置镜像优先。
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SourcePolicy {
    /// 内置镜像优先，官方源作为回退。
    #[serde(rename = "mirrorFirst")]
    #[default]
    MirrorFirst,
    /// 官方源优先，内置镜像作为回退；CurseForge 官方不可用。
    #[serde(rename = "officialFirst")]
    OfficialFirst,
    /// 自定义源：只允许按给定基址替换，失败不切换任何来源。
    #[serde(rename = "custom", rename_all = "camelCase")]
    Custom {
        minecraft_base: Option<String>,
        modrinth_base: Option<String>,
    },
}

/// 下载域名分类，决定镜像映射与 CurseForge 特殊规则。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceDomain {
    Minecraft,
    Modrinth,
    CurseForge,
    Other,
}

/// 候选来源渠道。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SourceChannel {
    Mirror,
    Official,
    Custom,
}

impl SourceChannel {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Mirror => "内置镜像",
            Self::Official => "官方源",
            Self::Custom => "自定义源",
        }
    }
}

/// 一个有序下载候选。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadCandidate {
    pub url: String,
    pub channel: SourceChannel,
    pub label: String,
}

/// CurseForge 官方源不可用，调用方应展示该提示且不得发起直连。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceCandidates {
    Ready(Vec<DownloadCandidate>),
    CurseForgeOfficialUnavailable {
        mirror: DownloadCandidate,
    },
    /// 自定义源不覆盖该域名或缺失基址；不得切换任何来源。
    CustomUnsupported {
        reason: String,
    },
}

pub fn classify_domain(url: &str) -> SourceDomain {
    let host = url::host_of(url);
    if matches!(
        host,
        "api.modrinth.com" | "cdn.modrinth.com" | "staging-api.modrinth.com"
    ) {
        SourceDomain::Modrinth
    } else if matches!(
        host,
        "api.curseforge.com"
            | "edge.forgecdn.net"
            | "media.forgecdn.net"
            | "mediafilez.forgecdn.net"
    ) {
        SourceDomain::CurseForge
    } else if matches!(
        host,
        "piston-meta.mojang.com"
            | "piston-data.mojang.com"
            | "launchermeta.mojang.com"
            | "launcher.mojang.com"
            | "resources.download.minecraft.net"
            | "libraries.minecraft.net"
            | "meta.fabricmc.net"
            | "maven.fabricmc.net"
            | "meta.quiltmc.org"
            | "maven.quiltmc.org"
            | "maven.neoforged.net"
            | "files.minecraftforge.net"
    ) {
        SourceDomain::Minecraft
    } else {
        SourceDomain::Other
    }
}

/// 按 BMCLAPI 官方文档路由表生成 Minecraft 域名镜像 URL。
fn bmclapi_mirror(url: &str) -> Option<String> {
    let (host, path) = url::host_and_path(url);
    let base = BMCLAPI_BASE;
    match host {
        "piston-meta.mojang.com"
        | "piston-data.mojang.com"
        | "launchermeta.mojang.com"
        | "launcher.mojang.com" => Some(format!("{base}{path}")),
        "resources.download.minecraft.net" => Some(format!("{base}/assets{path}")),
        "libraries.minecraft.net" => Some(format!("{base}/maven{path}")),
        "meta.fabricmc.net" => Some(format!("{base}/fabric-meta{path}")),
        "maven.fabricmc.net" => Some(format!("{base}/maven{path}")),
        "meta.quiltmc.org" => Some(format!("{base}/quilt-meta{path}")),
        "maven.quiltmc.org" => {
            let stripped = path.strip_prefix("/repository/release").unwrap_or(path);
            Some(format!("{base}/maven{stripped}"))
        }
        "maven.neoforged.net" => {
            let stripped = path.strip_prefix("/releases").unwrap_or(path);
            Some(format!("{base}/maven{stripped}"))
        }
        "files.minecraftforge.net" => {
            let stripped = path.strip_prefix("/maven").unwrap_or(path);
            Some(format!("{base}/maven{stripped}"))
        }
        _ => None,
    }
}

/// 按 MCI Mirror 文档生成 Modrinth/CurseForge 域名镜像 URL。
fn mci_mirror(url: &str) -> Option<String> {
    let (host, path) = url::host_and_path(url);
    let base = MCI_MIRROR_BASE;
    match host {
        "api.modrinth.com" | "staging-api.modrinth.com" => Some(format!("{base}/modrinth{path}")),
        "cdn.modrinth.com" => Some(format!("{base}{path}")),
        "api.curseforge.com" => Some(format!("{base}/curseforge{path}")),
        "edge.forgecdn.net" | "media.forgecdn.net" | "mediafilez.forgecdn.net" => {
            Some(format!("{base}{path}"))
        }
        _ => None,
    }
}

fn mirror_candidate(url: &str, domain: SourceDomain) -> Option<DownloadCandidate> {
    let mirror_url = match domain {
        SourceDomain::Minecraft => bmclapi_mirror(url),
        SourceDomain::Modrinth | SourceDomain::CurseForge => mci_mirror(url),
        SourceDomain::Other => None,
    }?;
    let label = match domain {
        SourceDomain::Minecraft => "BMCLAPI 镜像",
        SourceDomain::Modrinth | SourceDomain::CurseForge => "MCI Mirror",
        SourceDomain::Other => "内置镜像",
    };
    Some(DownloadCandidate {
        url: mirror_url,
        channel: SourceChannel::Mirror,
        label: label.to_owned(),
    })
}

fn official_candidate(url: &str, domain: SourceDomain) -> DownloadCandidate {
    let label = match domain {
        SourceDomain::Modrinth => "Modrinth 官方",
        SourceDomain::CurseForge => "CurseForge 官方",
        SourceDomain::Minecraft => "Mojang 官方",
        SourceDomain::Other => "官方源",
    };
    DownloadCandidate {
        url: url.to_owned(),
        channel: SourceChannel::Official,
        label: label.to_owned(),
    }
}

fn custom_candidate(
    url: &str,
    domain: SourceDomain,
    base: Option<&String>,
) -> Result<DownloadCandidate> {
    let Some(base) = base else {
        return Err(CoreError::InvalidInstallRequest(
            "自定义源未配置该域名的基址，请在来源设置中补全或更改策略".to_owned(),
        ));
    };
    let (_, path) = url::host_and_path(url);
    let mapped = match domain {
        SourceDomain::Minecraft => {
            let (host, _) = url::host_and_path(url);
            let trimmed = base.trim_end_matches('/');
            match host {
                "resources.download.minecraft.net" => format!("{trimmed}/assets{path}"),
                "libraries.minecraft.net" | "maven.fabricmc.net" | "files.minecraftforge.net" => {
                    format!("{trimmed}/maven{path}")
                }
                _ => format!("{trimmed}{path}"),
            }
        }
        SourceDomain::Modrinth => format!("{}{}", base.trim_end_matches('/'), path),
        _ => {
            return Err(CoreError::InvalidInstallRequest(
                "自定义源不支持该域名，且不会切换到任何其他来源".to_owned(),
            ));
        }
    };
    Ok(DownloadCandidate {
        url: mapped,
        channel: SourceChannel::Custom,
        label: "自定义源".to_owned(),
    })
}

/// 为下载 URL 生成有序候选。CurseForge 官方优先时返回不可用提示。
#[must_use]
pub fn candidates_for(url: &str, policy: &SourcePolicy) -> SourceCandidates {
    let domain = classify_domain(url);
    let mirror = mirror_candidate(url, domain);
    match policy {
        SourcePolicy::MirrorFirst => {
            let mut candidates = Vec::new();
            if let Some(mirror) = mirror {
                candidates.push(mirror);
            }
            candidates.push(official_candidate(url, domain));
            SourceCandidates::Ready(candidates)
        }
        SourcePolicy::OfficialFirst => {
            if domain == SourceDomain::CurseForge {
                let mirror = mirror.unwrap_or_else(|| DownloadCandidate {
                    url: url.to_owned(),
                    channel: SourceChannel::Mirror,
                    label: "MCI Mirror".to_owned(),
                });
                return SourceCandidates::CurseForgeOfficialUnavailable { mirror };
            }
            let mut candidates = vec![official_candidate(url, domain)];
            if let Some(mirror) = mirror {
                candidates.push(mirror);
            }
            SourceCandidates::Ready(candidates)
        }
        SourcePolicy::Custom {
            minecraft_base,
            modrinth_base,
        } => {
            let base = match domain {
                SourceDomain::Minecraft => minecraft_base.as_ref(),
                SourceDomain::Modrinth => modrinth_base.as_ref(),
                _ => None,
            };
            match custom_candidate(url, domain, base) {
                Ok(candidate) => SourceCandidates::Ready(vec![candidate]),
                Err(error) => SourceCandidates::CustomUnsupported {
                    reason: error.to_string(),
                },
            }
        }
    }
}

impl AppService {
    pub fn download_source_policy(&self) -> Result<SourcePolicy> {
        let connection = self.connection()?;
        read_setting(&connection, SETTING_DOWNLOAD_SOURCE_POLICY)?
            .map(|serialized| serde_json::from_str(&serialized).map_err(CoreError::from))
            .transpose()
            .map(|policy| policy.unwrap_or_default())
    }

    pub fn set_download_source_policy(&self, policy: &SourcePolicy) -> Result<()> {
        let serialized = serde_json::to_string(policy)?;
        let connection = self.connection()?;
        write_setting(&connection, SETTING_DOWNLOAD_SOURCE_POLICY, &serialized)?;
        Ok(())
    }
}

/// URL 解析的最小辅助：避免为镜像映射引入完整 URL 库的额外语义。
mod url {
    pub fn host_of(url: &str) -> &str {
        host_and_path(url).0
    }

    pub fn host_and_path(url: &str) -> (&str, &str) {
        let without_scheme = url
            .strip_prefix("https://")
            .or_else(|| url.strip_prefix("http://"))
            .unwrap_or(url);
        match without_scheme.find('/') {
            Some(index) => (&without_scheme[..index], &without_scheme[index..]),
            None => (without_scheme, "/"),
        }
    }
}
