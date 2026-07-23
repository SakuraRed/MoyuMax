# EVAL DEFINITION：里程碑 25 启动器更新检查与安全下载

## Capability Evals

- [x] 手动检查更新：最新/有新版/失败三种结果可读；版本比较含 prerelease 规则。
- [x] 下载安装包经 SHA-256 与大小校验；校验失败删除文件。
- [x] 最低可升级版本声明被遵守，跨越时阻止下载并说明。
- [x] 更新提示开关持久化；无自动下载/自动安装路径。
- [x] 卸载不删除应用数据（安装器契约锁定）。

## Regression Evals

- [x] M1–M24 全部 BDD、Vitest、Playwright 与 Rust 工作区测试通过。
- [x] 960×600 与 200% 放大下更新区无横向溢出。

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

只有校验真实执行、失败清理、最低版本阻断、全程手动触发时，才可将本里程碑标记为 validated。下载不校验、存在任何自动安装路径、或失败仍留文件，均不算通过。

## 2026-07-23 验证报告

- Capability：5/5 PASS。
- Regression：2/2 PASS。
- `updates_bdd`：5/5 PASS（semver 与 prerelease 规则、新版本检测与已最新、最低版本解析与阻断、SHA-256 校验通过与失败清理、提示开关默认开启与持久化）。
- `cargo test --workspace`：167 个非忽略测试 PASS；5 个 ignored 保持不变。
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`、`cargo fmt --all -- --check`：PASS。
- `pnpm --filter @moyumax/desktop lint`（0 错误 0 警告）、`pnpm test`（19/19）、`pnpm build`：PASS。
- Playwright：97/97 PASS，其中 self-update 4/4 PASS（已最新无下载入口、新版本展示与校验下载、校验失败可读错误、960×600 与 200% 缩放）。本机端口 1420 被第三方进程 HYP.exe 占用，按同等配置改用 1421 复跑通过；标准 `pnpm test:e2e` 在端口空闲时不变。
- `pnpm --filter @moyumax/desktop tauri build`：x64 NSIS 生成成功（仍无 Authenticode，属发布缺口 #13）；安装器契约测试继续通过（卸载不删除数据）。
- `git diff --check`：PASS。
- 范围说明：检查走 GitHub Releases API（真实接口在发布候选时复核）；失败回滚路径为重新安装旧版本安装包（应用数据不被安装器改动）；卸载数据分类选择向导与 Authenticode 属缺口 #13 后半；后台定期检查未实现（仅手动）。

状态：validated（范围限于本 eval 所列条目）。
