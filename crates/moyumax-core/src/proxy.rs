//! HTTP 代理偏好与核心统一的 reqwest 客户端构造入口。
//!
//! 与 PCL-CE 5.8 对齐：跟随系统（默认，保持 reqwest system-proxy 现状）、
//! 直连、自定义代理三种模式。偏好持久化在 `app_settings`，`AppService::open`
//! 启动时读入全局；核心内所有 reqwest 客户端一律经 [`http_client_builder`]
//! 构造，使偏好对全部 HTTP 流量生效。

use std::sync::{OnceLock, RwLock};

use serde::{Deserialize, Serialize};

use crate::{AppService, CoreError, Result, read_setting, write_setting};

const SETTING_PROXY_PREFERENCE: &str = "proxy_preference";

/// HTTP 代理偏好。默认跟随系统。
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(
    rename_all = "camelCase",
    tag = "mode",
    rename_all_fields = "camelCase"
)]
pub enum ProxyPreference {
    /// 跟随系统代理（Windows 注册表与环境变量，reqwest system-proxy 现状）。
    #[default]
    System,
    /// 不使用任何代理，包括系统代理。
    Direct,
    /// 自定义代理：仅接受 `http://`、`https://` 或 `socks5h://` 地址。
    Custom { url: String },
}

impl ProxyPreference {
    /// 校验偏好可安全生效；自定义地址必须带受支持协议且被 reqwest 接受。
    pub fn validate(&self) -> Result<()> {
        match self {
            Self::System | Self::Direct => Ok(()),
            Self::Custom { url } => {
                let trimmed = url.trim();
                let supported = trimmed.starts_with("http://")
                    || trimmed.starts_with("https://")
                    || trimmed.starts_with("socks5h://");
                if !supported {
                    return Err(CoreError::InvalidInstallRequest(
                        "代理地址必须以 http://、https:// 或 socks5h:// 开头".to_owned(),
                    ));
                }
                reqwest::Proxy::all(trimmed).map_err(|error| {
                    CoreError::InvalidInstallRequest(format!("代理地址无效：{error}"))
                })?;
                Ok(())
            }
        }
    }
}

static ACTIVE_PROXY_PREFERENCE: OnceLock<RwLock<ProxyPreference>> = OnceLock::new();

fn active_proxy_slot() -> &'static RwLock<ProxyPreference> {
    ACTIVE_PROXY_PREFERENCE.get_or_init(|| RwLock::new(ProxyPreference::default()))
}

/// 当前全局生效的代理偏好。
#[must_use]
pub fn active_proxy_preference() -> ProxyPreference {
    active_proxy_slot()
        .read()
        .map(|guard| guard.clone())
        .unwrap_or_default()
}

/// 更新全局生效的代理偏好；此后构造的 HTTP 客户端立即按新偏好工作，
/// 已构造的客户端保持原配置直到重启或重建。
pub fn set_active_proxy_preference(preference: ProxyPreference) {
    if let Ok(mut guard) = active_proxy_slot().write() {
        *guard = preference;
    }
}

/// 核心统一的 reqwest 客户端构造入口：按全局代理偏好应用代理设置。
/// 各调用方在返回值上继续链式设置各自的超时、UA 与重定向策略。
pub(crate) fn http_client_builder() -> reqwest::ClientBuilder {
    let builder = reqwest::Client::builder();
    match active_proxy_preference() {
        ProxyPreference::System => builder,
        ProxyPreference::Direct => builder.no_proxy(),
        ProxyPreference::Custom { url } => match reqwest::Proxy::all(url.trim()) {
            Ok(proxy) => builder.proxy(proxy),
            // 写入时已校验，这里只是防御性回退：保持跟随系统而不是静默直连。
            Err(_) => builder,
        },
    }
}

impl AppService {
    /// 读取持久化的代理偏好；从未设置时返回默认的跟随系统。
    pub fn proxy_preference(&self) -> Result<ProxyPreference> {
        let connection = self.connection()?;
        read_setting(&connection, SETTING_PROXY_PREFERENCE)?
            .map(|serialized| serde_json::from_str(&serialized).map_err(CoreError::from))
            .transpose()
            .map(|preference| preference.unwrap_or_default())
    }

    /// 持久化代理偏好并更新全局生效值；非法地址拒绝写入。
    pub fn set_proxy_preference(&self, preference: &ProxyPreference) -> Result<()> {
        preference.validate()?;
        let serialized = serde_json::to_string(preference)?;
        let connection = self.connection()?;
        write_setting(&connection, SETTING_PROXY_PREFERENCE, &serialized)?;
        set_active_proxy_preference(preference.clone());
        Ok(())
    }
}
