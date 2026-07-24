//! Microsoft 设备码登录与 Xbox Live / Minecraft Services 认证链路。
//!
//! 链路：设备码 → MSA 令牌 → Xbox Live 用户认证 → XSTS（Minecraft 依赖方）
//! → Minecraft Services 登录 → 玩家档案。所有基础地址可注入：生产使用官方
//! 端点，测试使用本地替身。令牌绝不写日志；敏感类型的 `Debug` 一律脱敏。

use std::{
    fmt,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use reqwest::Client;
use serde::Deserialize;
use uuid::Uuid;

use crate::{CoreError, Result, accounts::MICROSOFT_APP_CLIENT_ID, unix_timestamp};

// common 租户同时接受组织与个人 Microsoft 帐户；应用注册须包含个人帐户
// （signInAudience = AzureADandPersonalMicrosoftAccount 或 PersonalMicrosoftAccount）。
const MSA_BASE_URL: &str = "https://login.microsoftonline.com/common/oauth2/v2.0";
const XBL_BASE_URL: &str = "https://user.auth.xboxlive.com";
const XSTS_BASE_URL: &str = "https://xsts.auth.xboxlive.com";
const MCS_BASE_URL: &str = "https://api.minecraftservices.com";
const OAUTH_SCOPE: &str = "XboxLive.signin offline_access";
/// 轮询取消检查粒度（毫秒）。
const CANCEL_CHECK_GRANULARITY: Duration = Duration::from_millis(200);

/// Microsoft 登录的取消令牌；取消后置位，轮询在下一检查点停止。
#[derive(Clone, Default)]
pub struct MicrosoftLoginCancel {
    flag: Arc<AtomicBool>,
}

impl MicrosoftLoginCancel {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.flag.store(true, Ordering::SeqCst);
    }

    pub fn reset(&self) {
        self.flag.store(false, Ordering::SeqCst);
    }

    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.flag.load(Ordering::SeqCst)
    }
}

impl fmt::Debug for MicrosoftLoginCancel {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MicrosoftLoginCancel")
            .field("cancelled", &self.is_cancelled())
            .finish()
    }
}

/// 设备码授权。`user_code` 与 `verification_uri` 展示给用户；
/// `device_code` 是轮询凭据，绝不序列化到前端。
#[derive(Clone)]
pub struct DeviceCodeGrant {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in_seconds: u64,
    pub poll_interval_seconds: u64,
}

impl fmt::Debug for DeviceCodeGrant {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DeviceCodeGrant")
            .field("device_code", &"<redacted>")
            .field("user_code", &self.user_code)
            .field("verification_uri", &self.verification_uri)
            .field("expires_in_seconds", &self.expires_in_seconds)
            .field("poll_interval_seconds", &self.poll_interval_seconds)
            .finish()
    }
}

/// 完整登录链路的产物。全部字段只在核心内部流转与入库，绝不序列化到前端。
#[derive(Clone)]
pub struct MicrosoftProfile {
    pub player_name: String,
    pub player_uuid: String,
    pub mc_access_token: String,
    pub mc_expires_at_unix_seconds: i64,
    pub msa_refresh_token: String,
}

impl fmt::Debug for MicrosoftProfile {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MicrosoftProfile")
            .field("player_name", &self.player_name)
            .field("player_uuid", &self.player_uuid)
            .field("mc_access_token", &"<redacted>")
            .field(
                "mc_expires_at_unix_seconds",
                &self.mc_expires_at_unix_seconds,
            )
            .field("msa_refresh_token", &"<redacted>")
            .finish()
    }
}

struct MsaTokens {
    access_token: String,
    refresh_token: String,
}

impl fmt::Debug for MicrosoftAuthClient {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MicrosoftAuthClient")
            .field("client_id", &self.client_id)
            .field("msa_base_url", &self.msa_base_url)
            .field("xbl_base_url", &self.xbl_base_url)
            .field("xsts_base_url", &self.xsts_base_url)
            .field("mcs_base_url", &self.mcs_base_url)
            .finish()
    }
}

#[derive(Clone)]
pub struct MicrosoftAuthClient {
    client: Client,
    client_id: String,
    msa_base_url: String,
    xbl_base_url: String,
    xsts_base_url: String,
    mcs_base_url: String,
}

impl MicrosoftAuthClient {
    /// 生产端点（MoyuMax 应用注册的设备码公共客户端）。
    pub fn production() -> Result<Self> {
        Self::with_base_urls(
            MICROSOFT_APP_CLIENT_ID,
            MSA_BASE_URL,
            XBL_BASE_URL,
            XSTS_BASE_URL,
            MCS_BASE_URL,
        )
    }

    pub fn with_base_urls(
        client_id: &str,
        msa_base_url: &str,
        xbl_base_url: &str,
        xsts_base_url: &str,
        mcs_base_url: &str,
    ) -> Result<Self> {
        let client = Client::builder().timeout(Duration::from_secs(30)).build()?;
        Ok(Self {
            client,
            client_id: client_id.to_owned(),
            msa_base_url: msa_base_url.trim_end_matches('/').to_owned(),
            xbl_base_url: xbl_base_url.trim_end_matches('/').to_owned(),
            xsts_base_url: xsts_base_url.trim_end_matches('/').to_owned(),
            mcs_base_url: mcs_base_url.trim_end_matches('/').to_owned(),
        })
    }

    /// 第一步：请求设备码。
    pub async fn begin_device_code(&self) -> Result<DeviceCodeGrant> {
        let response = self
            .client
            .post(format!("{}/devicecode", self.msa_base_url))
            .form(&[
                ("client_id", self.client_id.as_str()),
                ("scope", OAUTH_SCOPE),
            ])
            .send()
            .await
            .map_err(ms_network_error)?;
        if !response.status().is_success() {
            let status = response.status();
            let detail = response
                .json::<MsaTokenError>()
                .await
                .ok()
                .and_then(|error| error.error_description.or(Some(error.error)))
                .filter(|text| !text.is_empty());
            return Err(CoreError::Account(match detail {
                Some(detail) => format!("无法获取 Microsoft 设备码：{detail}"),
                None => format!("无法获取 Microsoft 设备码（HTTP {status}），请稍后重试"),
            }));
        }
        let payload: DeviceCodeResponse = response
            .json()
            .await
            .map_err(|_| CoreError::Account("认证服务器返回了无法解析的设备码响应".to_owned()))?;
        Ok(DeviceCodeGrant {
            device_code: payload.device_code,
            user_code: payload.user_code,
            verification_uri: payload.verification_uri,
            expires_in_seconds: payload.expires_in,
            poll_interval_seconds: payload.interval.unwrap_or(5),
        })
    }

    /// 第二步：按服务端间隔轮询设备码，直到用户完成授权、拒绝、过期或取消。
    pub async fn poll_device_code(
        &self,
        grant: &DeviceCodeGrant,
        cancel: &MicrosoftLoginCancel,
    ) -> Result<MicrosoftProfile> {
        let msa = self.poll_msa_tokens(grant, cancel).await?;
        self.complete_login(&msa).await
    }

    async fn poll_msa_tokens(
        &self,
        grant: &DeviceCodeGrant,
        cancel: &MicrosoftLoginCancel,
    ) -> Result<MsaTokens> {
        let mut interval = grant.poll_interval_seconds;
        let deadline = unix_timestamp() + i64::try_from(grant.expires_in_seconds).unwrap_or(900);
        loop {
            sleep_cancellable(Duration::from_secs(interval), cancel).await?;
            let response = self
                .client
                .post(format!("{}/token", self.msa_base_url))
                .form(&[
                    ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                    ("client_id", self.client_id.as_str()),
                    ("device_code", grant.device_code.as_str()),
                ])
                .send()
                .await
                .map_err(ms_network_error)?;
            if response.status().is_success() {
                let tokens: MsaTokenResponse = response.json().await.map_err(|_| {
                    CoreError::Account("认证服务器返回了无法解析的令牌响应".to_owned())
                })?;
                let refresh_token = tokens.refresh_token.ok_or_else(|| {
                    CoreError::Account("认证服务器未返回刷新令牌，请重试登录".to_owned())
                })?;
                return Ok(MsaTokens {
                    access_token: tokens.access_token,
                    refresh_token,
                });
            }
            let error: MsaTokenError = response.json().await.unwrap_or(MsaTokenError {
                error: "unknown".to_owned(),
                error_description: None,
            });
            match error.error.as_str() {
                "authorization_pending" => {}
                "slow_down" => interval += 5,
                "authorization_declined" => {
                    return Err(CoreError::Account(
                        "你在浏览器中拒绝了授权请求，登录未完成".to_owned(),
                    ));
                }
                "expired_token" => {
                    return Err(CoreError::Account(
                        "设备码已过期，请重新发起登录".to_owned(),
                    ));
                }
                other => {
                    let description = error.error_description.unwrap_or_else(|| other.to_owned());
                    return Err(CoreError::Account(format!(
                        "Microsoft 登录失败：{description}"
                    )));
                }
            }
            if unix_timestamp() >= deadline {
                return Err(CoreError::Account(
                    "设备码已过期，请重新发起登录".to_owned(),
                ));
            }
        }
    }

    /// 第三至五步：Xbox Live → XSTS → Minecraft Services → 玩家档案。
    async fn complete_login(&self, msa: &MsaTokens) -> Result<MicrosoftProfile> {
        let (xbl_token, _uhs) = self.xbl_authenticate(&msa.access_token).await?;
        let (xsts_token, uhs) = self.xsts_authorize(&xbl_token).await?;
        let (mc_token, expires_in) = self.minecraft_login(&uhs, &xsts_token).await?;
        let (player_name, player_uuid) = self.fetch_profile(&mc_token).await?;
        Ok(MicrosoftProfile {
            player_name,
            player_uuid,
            mc_access_token: mc_token,
            mc_expires_at_unix_seconds: unix_timestamp()
                + i64::try_from(expires_in).unwrap_or(86_400),
            msa_refresh_token: msa.refresh_token.clone(),
        })
    }

    async fn xbl_authenticate(&self, msa_access_token: &str) -> Result<(String, String)> {
        let response = self
            .client
            .post(format!("{}/user/authenticate", self.xbl_base_url))
            .json(&serde_json::json!({
                "Properties": {
                    "AuthMethod": "RPS",
                    "SiteName": "user.auth.xboxlive.com",
                    "RpsTicket": format!("d={msa_access_token}"),
                },
                "RelyingParty": "http://auth.xboxlive.com",
                "TokenType": "JWT",
            }))
            .send()
            .await
            .map_err(ms_network_error)?;
        if !response.status().is_success() {
            return Err(CoreError::Account(format!(
                "Xbox Live 用户认证失败（HTTP {}），请稍后重试",
                response.status()
            )));
        }
        let payload: XboxAuthResponse = response
            .json()
            .await
            .map_err(|_| CoreError::Account("Xbox Live 返回了无法解析的响应".to_owned()))?;
        let uhs = payload
            .display_claims
            .xui
            .first()
            .map(|info| info.uhs.clone())
            .ok_or_else(|| CoreError::Account("Xbox Live 响应缺少用户标识".to_owned()))?;
        Ok((payload.token, uhs))
    }

    async fn xsts_authorize(&self, xbl_token: &str) -> Result<(String, String)> {
        let response = self
            .client
            .post(format!("{}/xsts/authorize", self.xsts_base_url))
            .json(&serde_json::json!({
                "Properties": {
                    "SandboxId": "RETAIL",
                    "UserTokens": [xbl_token],
                },
                "RelyingParty": "rp://api.minecraftservices.com/",
                "TokenType": "JWT",
            }))
            .send()
            .await
            .map_err(ms_network_error)?;
        if !response.status().is_success() {
            let error: XstsErrorResponse = response.json().await.unwrap_or(XstsErrorResponse {
                xerr: None,
                message: None,
            });
            return Err(match error.xerr {
                Some(2_148_916_233) => CoreError::Account(
                    "该 Microsoft 账户还没有 Xbox 档案，请先在 xbox.com 创建后再登录".to_owned(),
                ),
                Some(2_148_916_235) => CoreError::Account(
                    "Xbox Live 在你的国家/地区不可用，无法使用该账户登录".to_owned(),
                ),
                Some(2_148_916_236 | 2_148_916_237) => CoreError::Account(
                    "Xbox Live 要求完成成人验证（韩国地区），请先在 xbox.com 完成验证".to_owned(),
                ),
                Some(2_148_916_238) => CoreError::Account(
                    "该账户是未成年账户，需要由家长在 Xbox 家庭设置中授权后才能登录".to_owned(),
                ),
                Some(xerr) => {
                    CoreError::Account(format!("Xbox 安全令牌服务拒绝了登录（XErr {xerr}）"))
                }
                None => CoreError::Account("Xbox 安全令牌服务拒绝了登录".to_owned()),
            });
        }
        let payload: XboxAuthResponse = response
            .json()
            .await
            .map_err(|_| CoreError::Account("Xbox 安全令牌服务返回了无法解析的响应".to_owned()))?;
        let uhs = payload
            .display_claims
            .xui
            .first()
            .map(|info| info.uhs.clone())
            .ok_or_else(|| CoreError::Account("Xbox 安全令牌服务响应缺少用户标识".to_owned()))?;
        Ok((payload.token, uhs))
    }

    async fn minecraft_login(&self, uhs: &str, xsts_token: &str) -> Result<(String, u64)> {
        let response = self
            .client
            .post(format!(
                "{}/authentication/login_with_xbox",
                self.mcs_base_url
            ))
            .json(&serde_json::json!({
                "identityToken": format!("XBL3.0 x={uhs};{xsts_token}"),
            }))
            .send()
            .await
            .map_err(ms_network_error)?;
        if !response.status().is_success() {
            return Err(CoreError::Account(format!(
                "Minecraft Services 登录失败（HTTP {}），请稍后重试",
                response.status()
            )));
        }
        let payload: McLoginResponse = response.json().await.map_err(|_| {
            CoreError::Account("Minecraft Services 返回了无法解析的响应".to_owned())
        })?;
        Ok((payload.access_token, payload.expires_in))
    }

    async fn fetch_profile(&self, mc_access_token: &str) -> Result<(String, String)> {
        let response = self
            .client
            .get(format!("{}/minecraft/profile", self.mcs_base_url))
            .bearer_auth(mc_access_token)
            .send()
            .await
            .map_err(ms_network_error)?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(CoreError::Account(
                "该 Microsoft 账户未拥有 Minecraft，请使用已购买游戏的账户登录".to_owned(),
            ));
        }
        if !response.status().is_success() {
            return Err(CoreError::Account(format!(
                "无法获取 Minecraft 玩家档案（HTTP {}）",
                response.status()
            )));
        }
        let payload: McProfileResponse = response
            .json()
            .await
            .map_err(|_| CoreError::Account("玩家档案接口返回了无法解析的响应".to_owned()))?;
        if payload.name.is_empty() || payload.name.len() > 16 {
            return Err(CoreError::Account(
                "玩家档案接口返回的玩家名无效".to_owned(),
            ));
        }
        let uuid = Uuid::parse_str(&payload.id)
            .map_err(|_| CoreError::Account("玩家档案接口返回的 UUID 无效".to_owned()))?;
        Ok((payload.name, uuid.to_string()))
    }

    /// 刷新链：MSA 刷新令牌（轮换）→ 完整 Xbox 链 → 新档案。
    pub async fn refresh_profile(&self, msa_refresh_token: &str) -> Result<MicrosoftProfile> {
        let msa = self.refresh_msa_tokens(msa_refresh_token).await?;
        self.complete_login(&msa).await
    }

    async fn refresh_msa_tokens(&self, refresh_token: &str) -> Result<MsaTokens> {
        let response = self
            .client
            .post(format!("{}/token", self.msa_base_url))
            .form(&[
                ("grant_type", "refresh_token"),
                ("client_id", self.client_id.as_str()),
                ("refresh_token", refresh_token),
                ("scope", OAUTH_SCOPE),
            ])
            .send()
            .await
            .map_err(ms_network_error)?;
        if response.status().is_success() {
            let tokens: MsaTokenResponse = response
                .json()
                .await
                .map_err(|_| CoreError::Account("认证服务器返回了无法解析的令牌响应".to_owned()))?;
            return Ok(MsaTokens {
                access_token: tokens.access_token,
                refresh_token: tokens
                    .refresh_token
                    .unwrap_or_else(|| refresh_token.to_owned()),
            });
        }
        let error: MsaTokenError = response.json().await.unwrap_or(MsaTokenError {
            error: "unknown".to_owned(),
            error_description: None,
        });
        if error.error == "invalid_grant" {
            return Err(CoreError::AccountCredentials(
                "Microsoft 会话已被吊销，请重新登录".to_owned(),
            ));
        }
        let description = error
            .error_description
            .unwrap_or_else(|| error.error.clone());
        Err(CoreError::Account(format!(
            "Microsoft 会话刷新失败：{description}"
        )))
    }
}

async fn sleep_cancellable(duration: Duration, cancel: &MicrosoftLoginCancel) -> Result<()> {
    let mut remaining = duration;
    while !remaining.is_zero() {
        if cancel.is_cancelled() {
            return Err(CoreError::AccountLoginCancelled(
                "Microsoft 登录已取消".to_owned(),
            ));
        }
        let step = remaining.min(CANCEL_CHECK_GRANULARITY);
        tokio::time::sleep(step).await;
        remaining = remaining.saturating_sub(step);
    }
    if cancel.is_cancelled() {
        return Err(CoreError::AccountLoginCancelled(
            "Microsoft 登录已取消".to_owned(),
        ));
    }
    Ok(())
}

fn ms_network_error(error: reqwest::Error) -> CoreError {
    CoreError::AccountNetwork(format!("无法连接 Microsoft 认证服务：{error}"))
}

#[derive(Debug, Deserialize)]
struct DeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    expires_in: u64,
    interval: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct MsaTokenResponse {
    access_token: String,
    refresh_token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MsaTokenError {
    error: String,
    error_description: Option<String>,
}

#[derive(Debug, Deserialize)]
struct XboxAuthResponse {
    #[serde(rename = "Token")]
    token: String,
    #[serde(rename = "DisplayClaims")]
    display_claims: XboxDisplayClaims,
}

#[derive(Debug, Deserialize)]
struct XboxDisplayClaims {
    xui: Vec<XboxUserInfo>,
}

#[derive(Debug, Deserialize)]
struct XboxUserInfo {
    uhs: String,
}

#[derive(Debug, Deserialize)]
struct XstsErrorResponse {
    #[serde(rename = "XErr")]
    xerr: Option<u64>,
    #[serde(rename = "Message")]
    #[allow(dead_code)]
    message: Option<String>,
}

#[derive(Debug, Deserialize)]
struct McLoginResponse {
    access_token: String,
    expires_in: u64,
}

#[derive(Debug, Deserialize)]
struct McProfileResponse {
    id: String,
    name: String,
}
