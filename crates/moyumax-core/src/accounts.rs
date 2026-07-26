//! 账户模型、Authlib Injector（Yggdrasil）外置登录与 Microsoft 设备码登录。
//!
//! 离线、外置与 Microsoft 账户统一存入 `accounts` 表；外置登录只保存令牌与玩家档案，
//! 密码只在当次认证请求中使用，绝不落盘；Microsoft 账户保存 MSA 刷新令牌与
//! MC 访问令牌（含过期时间）。令牌不序列化到前端——`AccountSummary` 不含任何令牌字段。

use std::{fmt, time::Duration};

use reqwest::{Client, StatusCode, Url};
use rusqlite::{OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    AppService, CoreError, LaunchAccount, Result,
    msauth::{DeviceCodeGrant, MicrosoftAuthClient, MicrosoftLoginCancel, MicrosoftProfile},
    unix_timestamp,
};

pub const LITTLESKIN_YGGDRASIL_URL: &str = "https://littleskin.cn/api/yggdrasil";

/// MoyuMax 的 Microsoft 应用注册（公共客户端，设备码流）。
///
/// 2026-07-24 由项目所有者在 Azure 重新注册（含个人 Microsoft 帐户 +
/// 允许公共客户端流 + 设备代码流）。前一个注册（a5897d46…）因帐户类型不含
/// 个人 MSA 报 AADSTS700016，已弃用。Client ID 是公开标识，不是机密；
/// 设备码登录链路（MSA → Xbox Live → XSTS → Minecraft Services）在
/// `msauth` 模块实现并以本常量发起。
pub const MICROSOFT_APP_CLIENT_ID: &str = "7f00149b-8b23-4ac4-86ef-d982a58c07a3";

const COMPAT_OFFLINE_PLAYER: &str = "MoyuMaxPlayer";
/// MC 访问令牌剩余有效期不足该秒数时，启动前先经刷新链换新。
const MC_TOKEN_REFRESH_MARGIN_SECONDS: i64 = 300;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AccountKind {
    Offline,
    Authlib,
    Microsoft,
}

impl AccountKind {
    fn from_database(value: &str) -> Result<Self> {
        match value {
            "offline" => Ok(Self::Offline),
            "authlib" => Ok(Self::Authlib),
            "microsoft" => Ok(Self::Microsoft),
            _ => Err(CoreError::InvalidStoredState(format!(
                "未知账户类型：{value}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AccountSessionState {
    Valid,
    Expired,
}

impl AccountSessionState {
    fn from_database(value: &str) -> Result<Self> {
        match value {
            "valid" => Ok(Self::Valid),
            "expired" => Ok(Self::Expired),
            _ => Err(CoreError::InvalidStoredState(format!(
                "未知账户会话状态：{value}"
            ))),
        }
    }
}

/// 账户的非敏感视图（可安全序列化到前端；绝不包含令牌）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountSummary {
    pub id: String,
    pub kind: AccountKind,
    pub username: String,
    pub player_uuid: String,
    pub server_url: Option<String>,
    pub is_default: bool,
    pub session_state: AccountSessionState,
    pub created_at_unix_seconds: i64,
    pub last_validated_at_unix_seconds: Option<i64>,
}

struct StoredAccount {
    summary: AccountSummary,
    access_token: String,
    client_token: String,
    msa_refresh_token: String,
    mc_expires_at_unix_seconds: Option<i64>,
}

impl fmt::Debug for StoredAccount {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("StoredAccount")
            .field("summary", &self.summary)
            .field("access_token", &"<redacted>")
            .field("client_token", &"<redacted>")
            .field("msa_refresh_token", &"<redacted>")
            .field(
                "mc_expires_at_unix_seconds",
                &self.mc_expires_at_unix_seconds,
            )
            .finish()
    }
}

pub struct YggdrasilProfile {
    pub access_token: String,
    pub client_token: String,
    pub player_name: String,
    pub player_uuid: String,
}

impl fmt::Debug for YggdrasilProfile {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("YggdrasilProfile")
            .field("player_name", &self.player_name)
            .field("player_uuid", &self.player_uuid)
            .field("access_token", &"<redacted>")
            .field("client_token", &"<redacted>")
            .finish()
    }
}

#[derive(Clone)]
pub struct YggdrasilClient {
    client: Client,
    base_url: Url,
}

impl YggdrasilClient {
    pub fn littleskin() -> Result<Self> {
        Self::with_base_url(LITTLESKIN_YGGDRASIL_URL)
    }

    pub fn with_base_url(base_url: &str) -> Result<Self> {
        let mut base_url = Url::parse(base_url)
            .map_err(|error| CoreError::Account(format!("认证服务器地址无效：{error}")))?;
        if !base_url.path().ends_with('/') {
            base_url.set_path(&format!("{}/", base_url.path()));
        }
        let localhost = matches!(base_url.host_str(), Some("127.0.0.1" | "localhost"));
        if base_url.scheme() != "https" && !(base_url.scheme() == "http" && localhost) {
            return Err(CoreError::Account(
                "认证服务器必须使用 https 地址".to_owned(),
            ));
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

    pub async fn authenticate(
        &self,
        username: &str,
        password: &str,
        client_token: &str,
    ) -> Result<YggdrasilProfile> {
        if username.trim().is_empty() || password.is_empty() {
            return Err(CoreError::Account("用户名和密码不能为空".to_owned()));
        }
        let payload = serde_json::json!({
            "agent": { "name": "Minecraft", "version": 1 },
            "username": username,
            "password": password,
            "clientToken": client_token,
            "requestUser": false
        });
        let response = self
            .client
            .post(self.endpoint("authserver/authenticate")?)
            .json(&payload)
            .send()
            .await
            .map_err(account_network_error)?;
        let profile: YggdrasilAuthResponse = self.read_response(response).await?;
        let selected = profile
            .selected_profile
            .ok_or_else(|| CoreError::Account("认证服务器没有返回玩家档案".to_owned()))?;
        Ok(YggdrasilProfile {
            access_token: profile.access_token,
            client_token: profile
                .client_token
                .unwrap_or_else(|| client_token.to_owned()),
            player_name: selected.name,
            player_uuid: selected.id,
        })
    }

    pub async fn refresh(
        &self,
        access_token: &str,
        client_token: &str,
    ) -> Result<YggdrasilProfile> {
        let payload = serde_json::json!({
            "accessToken": access_token,
            "clientToken": client_token,
            "requestUser": false
        });
        let response = self
            .client
            .post(self.endpoint("authserver/refresh")?)
            .json(&payload)
            .send()
            .await
            .map_err(account_network_error)?;
        let profile: YggdrasilAuthResponse = self.read_response(response).await?;
        let selected = profile
            .selected_profile
            .ok_or_else(|| CoreError::Account("认证服务器没有返回玩家档案".to_owned()))?;
        Ok(YggdrasilProfile {
            access_token: profile.access_token,
            client_token: profile
                .client_token
                .unwrap_or_else(|| client_token.to_owned()),
            player_name: selected.name,
            player_uuid: selected.id,
        })
    }

    async fn read_response<T: serde::de::DeserializeOwned>(
        &self,
        response: reqwest::Response,
    ) -> Result<T> {
        let status = response.status();
        if status == StatusCode::FORBIDDEN || status == StatusCode::UNAUTHORIZED {
            let message = response
                .json::<YggdrasilErrorResponse>()
                .await
                .ok()
                .and_then(|error| error.error_message)
                .unwrap_or_else(|| "用户名或密码错误".to_owned());
            return Err(CoreError::AccountCredentials(message));
        }
        if !status.is_success() {
            return Err(CoreError::Account(format!(
                "认证服务器返回 HTTP {}",
                status.as_u16()
            )));
        }
        Ok(response.json().await?)
    }

    fn endpoint(&self, path: &str) -> Result<Url> {
        self.base_url
            .join(path)
            .map_err(|error| CoreError::Account(format!("认证服务器地址无效：{error}")))
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct YggdrasilAuthResponse {
    access_token: String,
    client_token: Option<String>,
    selected_profile: Option<YggdrasilSelectedProfile>,
}

#[derive(Debug, Deserialize)]
struct YggdrasilSelectedProfile {
    id: String,
    name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct YggdrasilErrorResponse {
    error_message: Option<String>,
}

fn account_network_error(error: reqwest::Error) -> CoreError {
    CoreError::AccountNetwork(format!("无法连接认证服务器：{error}"))
}

impl AppService {
    pub fn list_accounts(&self) -> Result<Vec<AccountSummary>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "
            SELECT id, kind, username, player_uuid, server_url, is_default, session_state,
                   created_at_unix_seconds, last_validated_at_unix_seconds
            FROM accounts
            ORDER BY created_at_unix_seconds, id
            ",
        )?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, bool>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, i64>(7)?,
                    row.get::<_, Option<i64>>(8)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        rows.into_iter()
            .map(
                |(
                    id,
                    kind,
                    username,
                    player_uuid,
                    server_url,
                    is_default,
                    session_state,
                    created_at,
                    last_validated_at,
                )| {
                    Ok(AccountSummary {
                        id,
                        kind: AccountKind::from_database(&kind)?,
                        username,
                        player_uuid,
                        server_url,
                        is_default,
                        session_state: AccountSessionState::from_database(&session_state)?,
                        created_at_unix_seconds: created_at,
                        last_validated_at_unix_seconds: last_validated_at,
                    })
                },
            )
            .collect()
    }

    /// 创建离线账户；首个账户自动成为默认。
    pub fn add_offline_account(&self, username: &str) -> Result<AccountSummary> {
        LaunchAccount::offline(username)?;
        let player_uuid = LaunchAccount::offline(username)?.player_uuid().to_string();
        let summary = AccountSummary {
            id: Uuid::new_v4().to_string(),
            kind: AccountKind::Offline,
            username: username.to_owned(),
            player_uuid,
            server_url: None,
            is_default: false,
            session_state: AccountSessionState::Valid,
            created_at_unix_seconds: unix_timestamp(),
            last_validated_at_unix_seconds: None,
        };
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute(
            "
            INSERT INTO accounts (
                id, kind, username, player_uuid, server_url, access_token, client_token,
                is_default, session_state, created_at_unix_seconds,
                last_validated_at_unix_seconds
            ) VALUES (?1, 'offline', ?2, ?3, NULL, '0', '', 0, 'valid', ?4, NULL)
            ",
            params![
                summary.id,
                summary.username,
                summary.player_uuid,
                summary.created_at_unix_seconds
            ],
        )?;
        let has_default: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM accounts WHERE is_default = 1 AND id != ?1)",
            params![summary.id],
            |row| row.get(0),
        )?;
        let is_default = !has_default;
        if is_default {
            transaction.execute(
                "UPDATE accounts SET is_default = 1 WHERE id = ?1",
                params![summary.id],
            )?;
        }
        transaction.commit()?;
        Ok(AccountSummary {
            is_default,
            ..summary
        })
    }

    /// 外置登录：只保存令牌与玩家档案，密码绝不落盘。
    pub async fn add_authlib_account(
        &self,
        client: &YggdrasilClient,
        server_url: &str,
        username: &str,
        password: &str,
    ) -> Result<AccountSummary> {
        let client_token = Uuid::new_v4().simple().to_string();
        let profile = client
            .authenticate(username, password, &client_token)
            .await?;
        let now = unix_timestamp();
        let summary = AccountSummary {
            id: Uuid::new_v4().to_string(),
            kind: AccountKind::Authlib,
            username: profile.player_name.clone(),
            player_uuid: profile.player_uuid.clone(),
            server_url: Some(server_url.to_owned()),
            is_default: false,
            session_state: AccountSessionState::Valid,
            created_at_unix_seconds: now,
            last_validated_at_unix_seconds: Some(now),
        };
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute(
            "
            INSERT INTO accounts (
                id, kind, username, player_uuid, server_url, access_token, client_token,
                is_default, session_state, created_at_unix_seconds,
                last_validated_at_unix_seconds
            ) VALUES (?1, 'authlib', ?2, ?3, ?4, ?5, ?6, 0, 'valid', ?7, ?8)
            ",
            params![
                summary.id,
                summary.username,
                summary.player_uuid,
                summary.server_url,
                profile.access_token,
                profile.client_token,
                summary.created_at_unix_seconds,
                summary.last_validated_at_unix_seconds,
            ],
        )?;
        // 新登录的外置账户直接成为默认：刚登录的账户显然是接下来要使用的。
        transaction.execute(
            "UPDATE accounts SET is_default = 0 WHERE is_default = 1",
            [],
        )?;
        transaction.execute(
            "UPDATE accounts SET is_default = 1 WHERE id = ?1",
            params![summary.id],
        )?;
        transaction.commit()?;
        Ok(AccountSummary {
            is_default: true,
            ..summary
        })
    }

    pub fn set_default_account(&self, account_id: &str) -> Result<()> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let changed = transaction.execute(
            "UPDATE accounts SET is_default = 0 WHERE is_default = 1 AND id != ?1",
            params![account_id],
        )?;
        let marked = transaction.execute(
            "UPDATE accounts SET is_default = 1 WHERE id = ?1",
            params![account_id],
        )?;
        if marked == 0 {
            return Err(CoreError::Account("账户不存在".to_owned()));
        }
        let _ = changed;
        transaction.commit()?;
        Ok(())
    }

    /// 移除账户；移除默认账户时最早的剩余账户自动成为默认。
    pub fn remove_account(&self, account_id: &str) -> Result<()> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let was_default: bool = transaction
            .query_row(
                "SELECT is_default FROM accounts WHERE id = ?1",
                params![account_id],
                |row| row.get(0),
            )
            .optional()?
            .ok_or_else(|| CoreError::Account("账户不存在".to_owned()))?;
        transaction.execute("DELETE FROM accounts WHERE id = ?1", params![account_id])?;
        if was_default {
            transaction.execute(
                "
                UPDATE accounts SET is_default = 1
                WHERE id = (
                    SELECT id FROM accounts ORDER BY created_at_unix_seconds, id LIMIT 1
                )
                ",
                [],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    /// 刷新外置或 Microsoft 账户会话；令牌被吊销时标记过期，网络错误保持原状态。
    pub async fn refresh_account_session(&self, account_id: &str) -> Result<AccountSummary> {
        let stored = self.stored_account(account_id)?;
        match stored.summary.kind {
            AccountKind::Offline => Ok(stored.summary),
            AccountKind::Authlib => self.refresh_authlib_session(&stored).await,
            AccountKind::Microsoft => {
                let client = self.microsoft_auth_client()?;
                self.refresh_microsoft_session(&client, &stored).await
            }
        }
    }

    async fn refresh_authlib_session(&self, stored: &StoredAccount) -> Result<AccountSummary> {
        let account_id = stored.summary.id.as_str();
        let server_url = stored
            .summary
            .server_url
            .as_deref()
            .ok_or_else(|| CoreError::Account("账户缺少认证服务器地址".to_owned()))?;
        let client = YggdrasilClient::with_base_url(server_url)?;
        match client
            .refresh(&stored.access_token, &stored.client_token)
            .await
        {
            Ok(profile) => {
                let now = unix_timestamp();
                let mut connection = self.connection()?;
                let transaction =
                    connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
                transaction.execute(
                    "
                    UPDATE accounts
                    SET access_token = ?2, client_token = ?3, session_state = 'valid',
                        last_validated_at_unix_seconds = ?4
                    WHERE id = ?1
                    ",
                    params![account_id, profile.access_token, profile.client_token, now],
                )?;
                transaction.commit()?;
                self.account_summary(account_id)
            }
            Err(CoreError::AccountCredentials(message)) => {
                self.connection()?.execute(
                    "UPDATE accounts SET session_state = 'expired' WHERE id = ?1",
                    params![account_id],
                )?;
                Err(CoreError::AccountCredentials(format!(
                    "会话已被认证服务器吊销，请重新登录：{message}"
                )))
            }
            Err(error) => Err(error),
        }
    }

    /// Microsoft 会话刷新：MSA 刷新令牌轮换 + 完整 Xbox 链，随事务更新。
    async fn refresh_microsoft_session(
        &self,
        client: &MicrosoftAuthClient,
        stored: &StoredAccount,
    ) -> Result<AccountSummary> {
        if stored.msa_refresh_token.is_empty() {
            return Err(CoreError::InvalidStoredState(
                "Microsoft 账户缺少刷新令牌".to_owned(),
            ));
        }
        match client.refresh_profile(&stored.msa_refresh_token).await {
            Ok(profile) => {
                self.persist_microsoft_profile(&stored.summary.id, &profile)?;
                self.account_summary(&stored.summary.id)
            }
            Err(CoreError::AccountCredentials(message)) => {
                self.connection()?.execute(
                    "UPDATE accounts SET session_state = 'expired' WHERE id = ?1",
                    params![stored.summary.id],
                )?;
                Err(CoreError::AccountCredentials(format!(
                    "Microsoft 会话已失效，请重新登录：{message}"
                )))
            }
            Err(error) => Err(error),
        }
    }

    /// Microsoft 设备码登录完成：轮询 + Xbox 链 + 档案，成功后入库。
    /// 同一玩家 UUID 的 Microsoft 账户已存在时更新令牌（重新登录语义）。
    pub async fn complete_microsoft_device_login(
        &self,
        client: &MicrosoftAuthClient,
        grant: &DeviceCodeGrant,
        cancel: &MicrosoftLoginCancel,
    ) -> Result<AccountSummary> {
        let profile = client.poll_device_code(grant, cancel).await?;
        let existing = {
            let connection = self.connection()?;
            connection
                .query_row(
                    "SELECT id FROM accounts WHERE kind = 'microsoft' AND player_uuid = ?1",
                    params![profile.player_uuid],
                    |row| row.get::<_, String>(0),
                )
                .optional()?
        };
        let account_id = match existing {
            Some(id) => id,
            None => {
                let id = Uuid::new_v4().to_string();
                let now = unix_timestamp();
                let mut connection = self.connection()?;
                let transaction =
                    connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
                transaction.execute(
                    "
                    INSERT INTO accounts (
                        id, kind, username, player_uuid, server_url, access_token, client_token,
                        is_default, session_state, created_at_unix_seconds,
                        last_validated_at_unix_seconds
                    ) VALUES (?1, 'microsoft', ?2, ?3, NULL, '', '', 0, 'valid', ?4, ?4)
                    ",
                    params![id, profile.player_name, profile.player_uuid, now],
                )?;
                transaction.commit()?;
                id
            }
        };
        // 新登录（或重新登录）的 Microsoft 账户直接成为默认。
        {
            let mut connection = self.connection()?;
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            transaction.execute(
                "UPDATE accounts SET is_default = 0 WHERE is_default = 1",
                [],
            )?;
            transaction.execute(
                "UPDATE accounts SET is_default = 1 WHERE id = ?1",
                params![account_id],
            )?;
            transaction.commit()?;
        }
        self.persist_microsoft_profile(&account_id, &profile)?;
        self.account_summary(&account_id)
    }

    /// 把 Microsoft 档案的令牌与过期时间随事务写入账户行。
    fn persist_microsoft_profile(
        &self,
        account_id: &str,
        profile: &MicrosoftProfile,
    ) -> Result<()> {
        let now = unix_timestamp();
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute(
            "
            UPDATE accounts
            SET username = ?2, player_uuid = ?3, access_token = ?4, msa_refresh_token = ?5,
                mc_expires_at_unix_seconds = ?6, session_state = 'valid',
                last_validated_at_unix_seconds = ?7
            WHERE id = ?1
            ",
            params![
                account_id,
                profile.player_name,
                profile.player_uuid,
                profile.mc_access_token,
                profile.msa_refresh_token,
                profile.mc_expires_at_unix_seconds,
                now,
            ],
        )?;
        transaction.commit()?;
        Ok(())
    }

    /// 解析启动身份：默认账户或指定账户；外置令牌失效时拒绝启动；
    /// Microsoft 账户的 MC 令牌临期（剩余不足 5 分钟）时先经刷新链换新。
    pub async fn account_launch_identity(&self, account_id: Option<&str>) -> Result<LaunchAccount> {
        if self.list_accounts()?.is_empty() {
            // 兼容路径：没有任何账户时创建默认离线账户，与早期版本行为一致。
            self.add_offline_account(COMPAT_OFFLINE_PLAYER)?;
        }
        let summary = match account_id {
            Some(account_id) => self.account_summary(account_id)?,
            None => self
                .list_accounts()?
                .into_iter()
                .find(|account| account.is_default)
                .ok_or_else(|| {
                    CoreError::Account("没有默认账户，请先在账户管理中设置".to_owned())
                })?,
        };
        match summary.kind {
            AccountKind::Offline => LaunchAccount::offline(&summary.username),
            AccountKind::Authlib => {
                if summary.session_state == AccountSessionState::Expired {
                    return Err(CoreError::AccountCredentials(
                        "该账户会话已过期，请重新登录后再启动游戏".to_owned(),
                    ));
                }
                let stored = self.stored_account(&summary.id)?;
                LaunchAccount::yggdrasil(
                    &stored.summary.username,
                    &stored.summary.player_uuid,
                    &stored.access_token,
                    &stored.client_token,
                )
            }
            AccountKind::Microsoft => {
                if summary.session_state == AccountSessionState::Expired {
                    return Err(CoreError::AccountCredentials(
                        "该 Microsoft 账户会话已过期，请重新登录后再启动游戏".to_owned(),
                    ));
                }
                let mut stored = self.stored_account(&summary.id)?;
                let expires = stored.mc_expires_at_unix_seconds.unwrap_or(0);
                if expires <= unix_timestamp() + MC_TOKEN_REFRESH_MARGIN_SECONDS {
                    let client = self.microsoft_auth_client()?;
                    self.refresh_microsoft_session(&client, &stored).await?;
                    stored = self.stored_account(&summary.id)?;
                }
                LaunchAccount::microsoft(
                    &stored.summary.username,
                    &stored.summary.player_uuid,
                    &stored.access_token,
                )
            }
        }
    }

    fn account_summary(&self, account_id: &str) -> Result<AccountSummary> {
        self.list_accounts()?
            .into_iter()
            .find(|account| account.id == account_id)
            .ok_or_else(|| CoreError::Account("账户不存在".to_owned()))
    }

    fn stored_account(&self, account_id: &str) -> Result<StoredAccount> {
        let connection = self.connection()?;
        let row = connection
            .query_row(
                "
                SELECT id, kind, username, player_uuid, server_url, is_default, session_state,
                       created_at_unix_seconds, last_validated_at_unix_seconds,
                       access_token, client_token, msa_refresh_token, mc_expires_at_unix_seconds
                FROM accounts WHERE id = ?1
                ",
                params![account_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, Option<String>>(4)?,
                        row.get::<_, bool>(5)?,
                        row.get::<_, String>(6)?,
                        row.get::<_, i64>(7)?,
                        row.get::<_, Option<i64>>(8)?,
                        row.get::<_, String>(9)?,
                        row.get::<_, String>(10)?,
                        row.get::<_, String>(11)?,
                        row.get::<_, Option<i64>>(12)?,
                    ))
                },
            )
            .optional()?;
        let Some((
            id,
            kind,
            username,
            player_uuid,
            server_url,
            is_default,
            session_state,
            created_at,
            last_validated_at,
            access_token,
            client_token,
            msa_refresh_token,
            mc_expires_at_unix_seconds,
        )) = row
        else {
            return Err(CoreError::Account("账户不存在".to_owned()));
        };
        Ok(StoredAccount {
            summary: AccountSummary {
                id,
                kind: AccountKind::from_database(&kind)?,
                username,
                player_uuid,
                server_url,
                is_default,
                session_state: AccountSessionState::from_database(&session_state)?,
                created_at_unix_seconds: created_at,
                last_validated_at_unix_seconds: last_validated_at,
            },
            access_token,
            client_token,
            msa_refresh_token,
            mc_expires_at_unix_seconds,
        })
    }
}
