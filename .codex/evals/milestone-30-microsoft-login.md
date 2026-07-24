# EVAL DEFINITION：里程碑 30 Microsoft 设备码登录

## Capability Evals

- [x] 设备码流：请求/解析设备码，展示用户码与验证地址，按服务端间隔轮询，
  `slow_down` 退避，用户取消、用户拒绝、设备码过期均为可读错误且不入库。
- [x] 完整登录链：MSA → Xbox Live → XSTS → Minecraft Services → 玩家档案；
  成功后账户入库（kind=microsoft），MSA 刷新令牌与 MC 令牌只存数据库。
- [x] 拥有权与 Xbox 业务错误：档案 404 归类"未拥有 Minecraft"；XSTS
  2148916233/2148916235/2148916238 映射为可读错误；均不产生账户记录。
- [x] 会话刷新：刷新令牌轮换随事务更新；invalid_grant 标记 expired 并阻断启动；
  网络错误保持原状态。
- [x] 启动身份：Microsoft 账户 `user_type=msa` 与 MC 令牌注入启动参数；
  MC 令牌临期自动刷新；会话 expired 拒绝启动。
- [x] 令牌保密：`AccountSummary` 与事件负载不含令牌；日志/Debug 脱敏。

## Regression Evals

- [x] M1–M29 全部 BDD、Vitest、Playwright 与 Rust 工作区测试通过。
- [x] M20 离线与 Authlib 账户语义不变（共存、默认账户、移除回填默认）。
- [x] 960×600 与 200% 放大下账户区与设备码面板无横向溢出。

## Deterministic Graders

- `cargo test --workspace`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo fmt --all -- --check`
- `pnpm --filter @moyumax/desktop lint`
- `pnpm --filter @moyumax/desktop test`
- `pnpm --filter @moyumax/desktop test:e2e`
- `pnpm --filter @moyumax/desktop build`
- `git diff --check`

## Completion Rule

只有设备码轮询真实遵守服务端间隔、Xbox 四步链路真实顺序执行、令牌只落数据库、
刷新令牌轮换持久化、expired 阻断启动时，才可将本里程碑标记为 validated。
跳过拥有权检查、把令牌写进前端负载/日志、或用假令牌通过测试，均不算通过。

## 2026-07-24 验证报告

- Capability：6/6 PASS。
- Regression：3/3 PASS。
- `msauth_bdd`：10/10 PASS（设备码解析与间隔来源、完整链路入库与令牌保密、
  轮询取消、用户拒绝、未拥有游戏、XSTS 2148916233 映射、刷新轮换持久化、
  invalid_grant 标记过期并阻断启动、msa 启动身份、临期令牌启动前刷新、
  slow_down +5s 退避）。测试使用四端点本地 HTTP 替身；生产端点经
  `with_microsoft_auth_client` 注入点替换，生产路径始终为官方端点。
- `cargo test --workspace`：190 个非忽略测试 PASS、0 失败；5 个 ignored 保持不变。
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`、
  `cargo fmt --all -- --check`：PASS。
- `pnpm --filter @moyumax/desktop lint`（0 错误 0 警告）、`pnpm test`（19/19）、
  `pnpm build`：PASS。
- Playwright：114/114 PASS（含 microsoft-login 4/4：完整流程入库、取消、
  失败可读错误、960×600 与 200% 缩放；M20-ACCT-005 更新为真实入口断言）。
- `pnpm --filter @moyumax/desktop tauri build`：x64 NSIS 生成成功；已用自签名
  开发证书 + DigiCert 时间戳签名，复制到
  `D:\downloads\MoyuMax_0.1.0-preview.1_x64-setup.exe`。关于页声明同步更新为
  「自签名开发预览构建」。
- `git diff --check`、`node scripts/generate-sbom.mjs --check`（604 依赖一致，
  新增 serde_urlencoded/ryu 为 reqwest form 特性传递依赖）：PASS。
- 依赖变化：workspace reqwest 启用 `form` 特性（设备码/令牌端点的
  x-www-form-urlencoded 编码），无新增 crate。
- 范围说明：真实 Microsoft 链路（真实账户授权、真实 Xbox 链、正版服务器加入）
  未在本机验证——需要真实账户交互，列入 `docs/review/M30-microsoft-login.md`
  人工审查清单；统一通行证预设地址仍未实现；「打开链接」经 rundll32 打开系统
  浏览器，仅允许 https。

状态：validated（范围限于本 eval 所列条目；真实链路以人工审查为准）。
