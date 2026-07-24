# 里程碑 30：Microsoft 设备码登录

## 目标

为首个公开预览补上正式账户入口的最后一块：Microsoft 账户登录。应用注册已就绪
（Client ID `a5897d46-0863-48dd-84f2-467896967591`，个人 Microsoft 帐户 + 允许公共
客户端流），采用 OAuth 2.0 设备码流：启动器展示用户码与验证地址，用户在任意浏览器
完成登录，启动器轮询获得令牌后依次完成 Xbox Live、XSTS、Minecraft Services 认证，
最终取得玩家档案并入库。

## 范围

- 设备码流：请求设备码、展示用户码与验证地址、按服务端给定间隔轮询、用户取消、
  用户拒绝与设备码过期的可读错误。
- Xbox 链路：MSA 令牌 → Xbox Live 用户认证 → XSTS（Minecraft 依赖方）→
  Minecraft Services 登录 → 玩家档案。XSTS 业务错误（无 Xbox 账户、地区不可用、
  未成年需家长验证）归类为可读错误。
- 拥有权检查：档案 404 归类为"该 Microsoft 账户未拥有 Minecraft"，不入库。
- 令牌存储：MSA 刷新令牌与 MC 访问令牌只存数据库，绝不序列化到前端；
  `AccountSummary` 不新增任何令牌字段；刷新令牌轮换时随事务更新。
- 会话刷新：`refresh_account_session` 支持 Microsoft 分支；刷新令牌被吊销
  （invalid_grant）标记 `expired` 并阻断启动。
- 启动身份：Microsoft 账户 `user_type=msa`，使用 MC 访问令牌；MC 令牌临期
  （剩余不足 5 分钟）时先经刷新链换新再启动。
- 前端：设置页账户区新增 Microsoft 登录入口，设备码面板（用户码、验证地址、
  复制、打开链接、取消），进度事件驱动状态更新；三语文案。
- 默认离线、默认不联网：除用户主动发起登录/刷新外不产生任何认证请求。

## 非目标

- 统一通行证预设地址（后续里程碑）。
- 真实 Microsoft 链路的 live 探针（留待发布候选，由用户手动验证）。
- 设备码自动复制到剪贴板或内嵌浏览器登录（保持系统浏览器，避免 WebView 内
  处理凭据）。

## 架构

- 核心新增 `msauth.rs`：`MicrosoftAuthClient`（基础地址可注入，测试用本地
  fixture）、`DeviceCodeGrant`、`MicrosoftProfile`、轮询取消令牌
  （`Arc<AtomicBool>`）、XSTS 业务错误映射。
- `accounts.rs`：schema v17 为 `accounts` 表增加 `msa_refresh_token` 与
  `mc_expires_at_unix_seconds` 两列；`AccountKind::Microsoft`；
  `complete_microsoft_device_login`、`refresh_microsoft_session`；
  `account_launch_identity` 改为 async（唯一生产调用点在 async 命令内）。
- `launch.rs`：`LaunchAccount` 增加 `user_type` 字段（offline/yggdrasil 为
  `legacy`，microsoft 为 `msa`）与 `LaunchAccount::microsoft` 构造器。
- 桌面层：`start_microsoft_device_login` / `cancel_microsoft_device_login`
  命令与 `microsoft-device-login` 进度事件；轮询在后台 tokio 任务执行。
- 前端：`runtime.ts` 类型与浏览器 mock（模拟 pending→completed 事件序列），
  SettingsCenter 设备码面板。

## 安全边界

- 不记录任何令牌、设备码（user_code 除外，它本就是给用户看的）到日志。
- 设备码轮询严格按服务端 interval，收到 `slow_down` 时 +5 秒。
- 所有认证请求 HTTPS；测试 fixture 除外（127.0.0.1）。
- 令牌只进 `accounts` 表；`Debug` 实现一律脱敏。
