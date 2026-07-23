# EVAL DEFINITION：里程碑 24 内置 CLI（开发者模式）

## Capability Evals

- [x] 未启用时任何命令拒绝并退出码 4；GUI 开启后立即可用。
- [x] instances list / tasks list / backups list 读命令输出版本化 JSON 且退出码 0。
- [x] tasks pause-all 与 backups create 写命令生效；--dry-run 输出计划且不落盘。
- [x] 未知命令退出码 2 并给出用法；JSON 信封含 schemaVersion/command/ok。

## Regression Evals

- [x] M1–M23 全部 BDD、Vitest、Playwright 与 Rust 工作区测试通过。
- [x] GUI 全局暂停/备份语义与 CLI 完全一致（共用核心）。
- [x] 960×600 与 200% 放大下开发者区无横向溢出。

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

只有门禁真实拦截、写命令与 GUI 共用核心、dry-run 不落盘、退出码与信封稳定时，才可将本里程碑标记为 validated。CLI 另起写路径、凭据进入输出、或未启用仍可执行，均不算通过。

## 2026-07-23 验证报告

- Capability：4/4 PASS。
- Regression：3/3 PASS。
- `cli_bdd`（src-tauri）：5/5 PASS（未启用拒绝含指引、实例清单版本化 JSON、pause-all dry-run 不落盘与真实写、backups create dry-run 与真实创建及缺参用法错误、未知命令退出码 2 与用法文本）。
- 真实 Release exe 冒烟：`moyumax-desktop.exe --cli instances list`（隔离 MOYUMAX_STATE_DIR）输出 cli_disabled JSON 且退出码 4。
- `cargo test --workspace`：162 个非忽略测试 PASS；5 个 ignored 保持不变。
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`、`cargo fmt --all -- --check`：PASS。
- `pnpm --filter @moyumax/desktop lint`（0 错误 0 警告）、`pnpm test`（19/19）、`pnpm build`：PASS。
- Playwright：93/93 PASS，其中 cli 2/2 PASS（开发者区开关/风险/持久化、960×600 与 200% 缩放）。本机端口 1420 被第三方进程 HYP.exe 占用，按同等配置改用 1421 复跑通过；标准 `pnpm test:e2e` 在端口空闲时不变。
- `pnpm --filter @moyumax/desktop tauri build`：x64 NSIS 生成成功（仍无 Authenticode，属发布缺口 #13）。
- `git diff --check`：PASS。
- 范围说明：安装/启动游戏等长时命令与进度事件协议后续单独设计；CLI 只读本地状态库，不开放远程；凭据相关数据不在命令集内。

状态：validated（范围限于本 eval 所列条目）。
