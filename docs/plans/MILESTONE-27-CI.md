# 里程碑 27：GitHub Actions Windows CI

## 目标

每次推送与 PR 都在干净的 Windows 环境执行与本地一致的完整验证门：Rust 静态检查与测试、前端静态检查与测试、Playwright 行为验收、NSIS 构建与产物上传。

## 范围

1. `.github/workflows/ci.yml`（windows-latest）：
   - `rust` 作业：`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets --all-features -- -D warnings`、`cargo test --workspace`；rust-toolchain.toml 决定的工具链；Cargo 缓存。
   - `frontend` 作业：corepack pnpm@10.26.2、`pnpm install --frozen-lockfile`、`pnpm lint`、`pnpm test`、`pnpm build`、`playwright install chromium`、`pnpm test:e2e`（CI 上 1420 端口空闲，使用标准配置）。
   - `bundle` 作业：完整 `pnpm tauri build`，上传 NSIS 产物与 SBOM 为工件（保留 14 天）。
2. `pnpm tauri build` 产物命名保留 preview 语义；CI 不执行签名与发布。
3. 文档：README 增加 CI 徽章与本地验证门对照表；工作流命令与本地 `.continue-here.md` 第 10 节逐行一致。

## 非目标

- 不做发布工作流（创建 Release/标签需要用户另行授权）。
- 不做矩阵（仅 Windows x64，与当前平台承诺一致）。
- 不做 ignored live 探针（外部服务依赖在 CI 默认跳过，与本机语义一致）。

## 验证

- 工作流 YAML 通过解析校验；每个步骤命令与本地验证门逐行一致（本机已逐一证明）。
- 首次推送后的实际运行结果由用户确认（本机无法执行 GitHub Actions）。
