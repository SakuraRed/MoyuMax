# MoyuMax

[![CI](https://github.com/SakuraRed/MoyuMax/actions/workflows/ci.yml/badge.svg)](https://github.com/SakuraRed/MoyuMax/actions/workflows/ci.yml)

MoyuMax 是一款面向 Minecraft Java Edition 的开源、免费、安装式统一管理启动器。首发平台为 Windows 10 22H2 及以上的 x64 系统。

项目当前处于公开预览前的增量开发阶段。已经完成首次运行、当前推荐 Vanilla/Fabric 与 Azul Zulu 的真实安装执行、隔离实例启动、Modrinth 模组及必需依赖原子安装、异常退出后的本地崩溃报告和脱敏诊断包、实例本地回收站，以及游戏会话前后原子世界备份。首个公开预览仍缺少正式账户入口、其余加载器、完整任务控制、运行期间定时增量备份与世界回滚、三语 i18n 和内置 CLI，因此当前构建只标记为开发预览版。

## 技术骨架

- Rust：本地领域核心、SQLite 状态、事务和未来 CLI。
- Tauri 2：Windows 桌面生命周期与系统集成。
- Svelte 5 + TypeScript：编译型声明式 WebView 界面，避免命令式 DOM 状态漂移。
- BDD + TDD：`features/` 定义行为，Rust 与 TypeScript 测试实现自动验收。

## 开发

要求：Rust 1.96.0、Node.js 22、pnpm 10.26.2、Windows WebView2。

```powershell
corepack pnpm install --frozen-lockfile
corepack pnpm test
corepack pnpm test:e2e
corepack pnpm build
cargo test --workspace
corepack pnpm --filter @moyumax/desktop tauri dev
```

## 文档

- [UI/UX 需求书](docs/UI-UX-REQUIREMENTS.md)
- [技术栈决策](docs/architecture/ADR-0001-DESKTOP-STACK.md)

## CI 与验证门

GitHub Actions（`.github/workflows/ci.yml`）在干净 Windows 环境执行与本地一致的完整验证门：

| 作业 | 内容 |
|---|---|
| rust | `cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets --all-features -- -D warnings`、`cargo test --workspace` |
| frontend | `pnpm install --frozen-lockfile`、`pnpm lint`、`pnpm test`、`pnpm build`、Playwright e2e |
| bundle | 图标/SBOM 一致性校验、`pnpm tauri build`（x64 NSIS，未签名）与产物上传 |

## 许可

客户端以 `GPL-3.0-only` 发布，完整条款见 [LICENSE](LICENSE)。第三方组件清单见 [THIRD-PARTY-LICENSES](docs/THIRD-PARTY-LICENSES.md)，SBOM 见 [SBOM.json](docs/SBOM.json)（由 `node scripts/generate-sbom.mjs` 可复现生成）。
