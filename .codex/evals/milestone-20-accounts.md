# EVAL DEFINITION：里程碑 20 账户入口（离线与 Authlib Injector 外置登录）

## Capability Evals

- [x] 离线账户创建与玩家名校验；默认账户唯一；移除生效。
- [x] 外置登录 authenticate/refresh 走真实 HTTP 协议（fixture 验证）；凭据错误与网络错误分类可读。
- [x] 密码绝不落盘；令牌不出现在前端序列化、日志或诊断。
- [x] 启动解析默认账户身份；外置令牌失效时拒绝启动并提示重新登录。
- [x] Microsoft 仅文字说明，无可点击伪装入口。

## Regression Evals

- [x] M1–M19 全部 BDD、Vitest、Playwright 与 Rust 工作区测试通过。
- [x] M4 离线启动语义不变（默认离线账户等价于原硬编码行为）。
- [x] 960×600 与 200% 放大下账户区与表单无横向溢出。

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

只有外置协议真实走通、密码不落盘、令牌不出库、启动使用默认身份、失效拒绝启动时，才可将本里程碑标记为 validated。只做界面列表没有真实认证、把密码写进数据库、或失效仍静默启动，均不算通过。

## 2026-07-23 验证报告

- Capability：5/5 PASS。
- Regression：3/3 PASS。
- `accounts_bdd`：7/7 PASS（离线创建与校验与首个默认、登录只存令牌且密码不出现在数据库与列表序列化、403 凭据错误与网络错误分类、默认唯一与启动身份解析、吊销令牌标记过期并阻断启动、移除默认后最早剩余接任、空库兼容创建默认离线账户）。
- `cargo test --workspace`：157 个非忽略测试 PASS；5 个 ignored 保持不变。
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`、`cargo fmt --all -- --check`：PASS。
- `pnpm --filter @moyumax/desktop lint`（0 错误 0 警告）、`pnpm test`（16/16）、`pnpm build`：PASS。
- Playwright：78/78 PASS，其中 accounts 6/6 PASS（离线创建、外置登录与凭据错误、默认唯一与移除接任、过期刷新失败、Microsoft 占位、960×600 与 200% 缩放）。本机端口 1420 被第三方进程 HYP.exe 占用，按同等配置改用 1421 复跑通过；标准 `pnpm test:e2e` 在端口空闲时不变。
- `pnpm --filter @moyumax/desktop tauri build`：x64 NSIS 生成成功（仍无 Authenticode，属发布缺口 #13）。
- `git diff --check`：PASS。
- 范围说明：Microsoft 登录未实现（应用注册外部阻塞），仅有文字说明；统一通行证复用同一 Yggdrasil 协议，预设地址后续补；密码保险库与主密码属 4.5 独立后续项；真实 LittleSkin 探针留待发布候选执行；schema 升至 v15（accounts 表，自动迁移）。

状态：validated（范围限于本 eval 所列条目）。
